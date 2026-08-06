//! Browser-neutral terminal graphics negotiation and bounded frame rendering.
//!
//! This module deliberately knows nothing about browser transport or CDP. It
//! consumes the metadata-only presentation contract and transient payloads,
//! retaining at most one displayed frame and one pending replacement.

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};

use base64::Engine as _;

use crate::presentation::{
    BrowserFrame, CaptureScale, FrameCleanupReason, FrameOwnershipEvent, FrameOwnershipRecord,
    FrameStoragePolicy, LatestFrameMailbox, PaneGeometry, PixelSize, PresentationContractError,
    TargetResourceIdentity, ViewportGeometry,
};

/// Maximum encoded payload retained for a single frame.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes emitted by one Kitty render command.
pub const MAX_OUTPUT_BYTES: usize = 6 * 1024 * 1024;
/// Maximum semantic text retained by the fallback renderer.
pub const MAX_SEMANTIC_BYTES: usize = 16 * 1024;
/// Maximum ownership records retained for diagnostics.
pub const MAX_OWNERSHIP_RECORDS: usize = 32;

const KITTY_INTRO: &[u8] = b"\x1b_G";
const KITTY_END: &[u8] = b"\x1b\\";

/// Terminal capability selected by negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsMode {
    /// Kitty graphics protocol, including terminals which advertise a
    /// compatible implementation such as WezTerm or Ghostty.
    Kitty,
    /// Ratatui-safe textual/ANSI presentation.
    Semantic,
}

impl GraphicsMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Kitty => "Kitty graphics",
            Self::Semantic => "semantic fallback",
        }
    }
}
impl Default for GraphicsMode {
    fn default() -> Self {
        Self::Semantic
    }
}

/// A deterministic environment snapshot used for capability negotiation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalEnvironment<'a> {
    pub term: Option<&'a str>,
    pub term_program: Option<&'a str>,
    pub kitty_window_id: Option<&'a str>,
}

impl<'a> TerminalEnvironment<'a> {
    pub const fn new(
        term: Option<&'a str>,
        term_program: Option<&'a str>,
        kitty_window_id: Option<&'a str>,
    ) -> Self {
        Self {
            term,
            term_program,
            kitty_window_id,
        }
    }

    /// Read the process environment at call time rather than compile time.
    pub fn from_process() -> TerminalEnvironmentOwned {
        TerminalEnvironmentOwned {
            term: std::env::var("TERM").ok(),
            term_program: std::env::var("TERM_PROGRAM").ok(),
            kitty_window_id: std::env::var("KITTY_WINDOW_ID").ok(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalEnvironmentOwned {
    pub term: Option<String>,
    pub term_program: Option<String>,
    pub kitty_window_id: Option<String>,
}

impl TerminalEnvironmentOwned {
    pub fn as_borrowed(&self) -> TerminalEnvironment<'_> {
        TerminalEnvironment::new(
            self.term.as_deref(),
            self.term_program.as_deref(),
            self.kitty_window_id.as_deref(),
        )
    }
}

/// Capability negotiation is intentionally conservative: an explicit Kitty
/// window marker or a known compatible terminal is required for pixel output.
pub fn negotiate(env: TerminalEnvironment<'_>) -> GraphicsMode {
    if env.kitty_window_id.is_some_and(|value| !value.is_empty()) {
        return GraphicsMode::Kitty;
    }
    let program = env.term_program.unwrap_or_default().to_ascii_lowercase();
    if matches!(program.as_str(), "kitty" | "wezterm" | "ghostty") {
        return GraphicsMode::Kitty;
    }
    GraphicsMode::Semantic
}
/// A bounded terminal cell rectangle. It is kept independent of Ratatui so
/// the renderer can be used by another terminal frontend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaneArea {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl PaneArea {
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsError {
    Invalid(String),
    Presentation(PresentationContractError),
    PayloadTooLarge { actual: usize, maximum: usize },
    OutputTooLarge { actual: usize, maximum: usize },
}

impl Display for GraphicsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => write!(formatter, "invalid terminal graphics state: {message}"),
            Self::Presentation(error) => Display::fmt(error, formatter),
            Self::PayloadTooLarge { actual, maximum } => {
                write!(formatter, "frame payload is {actual} bytes; maximum is {maximum}")
            }
            Self::OutputTooLarge { actual, maximum } => {
                write!(formatter, "terminal output is {actual} bytes; maximum is {maximum}")
            }
        }
    }
}

impl std::error::Error for GraphicsError {}

impl From<PresentationContractError> for GraphicsError {
    fn from(error: PresentationContractError) -> Self {
        Self::Presentation(error)
    }
}

/// A rendered command. Semantic output is plain text and can be passed to a
/// Ratatui `Paragraph`; Kitty output is an explicit protocol byte sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFrame {
    pub mode: GraphicsMode,
    pub generation: Option<u64>,
    pub pane: PaneArea,
    pub bytes: Vec<u8>,
}

impl RenderedFrame {
    pub fn as_text(&self) -> Option<&str> {
        (self.mode == GraphicsMode::Semantic)
            .then(|| std::str::from_utf8(&self.bytes).ok())
            .flatten()
    }
}

/// Bounded diagnostics exposed to the TUI status line and diagnostic tools.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GraphicsDiagnostics {
    pub mode: GraphicsMode,
    pub accepted_frames: u64,
    pub presented_frames: u64,
    pub replaced_frames: u64,
    pub dropped_frames: u64,
    pub stale_frames: u64,
    pub current_bytes: usize,
    pub pending_bytes: usize,
    pub geometry_revision: u64,
    pub cleanup_count: u64,
}

#[derive(Debug, Clone)]
struct PayloadFrame {
    frame: BrowserFrame,
    bytes: Vec<u8>,
}

/// Terminal-side adapter around the presentation latest-frame mailbox.
///
/// `LatestFrameMailbox` bounds metadata to two frames; this adapter applies the
/// same bound to payload bytes and drops bytes at the exact moment metadata is
/// replaced, preventing a hidden unbounded byte queue.
pub struct TerminalGraphics {
    mode: GraphicsMode,
    mailbox: LatestFrameMailbox,
    current_payload: Option<PayloadFrame>,
    pending_payload: Option<PayloadFrame>,
    pane: PaneArea,
    geometry: Option<ViewportGeometry>,
    geometry_revision: u64,
    browser_revision: u64,
    cleanup_count: u64,
    cleaned: bool,
    ownership: VecDeque<FrameOwnershipRecord>,
}

impl TerminalGraphics {
    pub fn new(mode: GraphicsMode, identity: TargetResourceIdentity) -> Result<Self, GraphicsError> {
        Ok(Self {
            mode,
            mailbox: LatestFrameMailbox::new(identity)?,
            current_payload: None,
            pending_payload: None,
            pane: PaneArea::default(),
            geometry: None,
            geometry_revision: 0,
            browser_revision: 0,
            cleanup_count: 0,
            cleaned: false,
            ownership: VecDeque::new(),
        })
    }

    pub fn for_environment(
        environment: TerminalEnvironment<'_>,
        identity: TargetResourceIdentity,
    ) -> Result<Self, GraphicsError> {
        Self::new(negotiate(environment), identity)
    }

    pub const fn mode(&self) -> GraphicsMode {
        self.mode
    }

    pub const fn pane(&self) -> PaneArea {
        self.pane
    }

    pub const fn geometry_revision(&self) -> u64 {
        self.geometry_revision
    }

    pub fn geometry(&self) -> Option<&ViewportGeometry> {
        self.geometry.as_ref()
    }

    /// Update pane geometry and release frames acquired for the old pane.
    /// Geometry revisions are monotonic and old frames cannot be presented
    /// after this operation.
    pub fn resize(
        &mut self,
        pane: PaneArea,
        viewport: PixelSize,
        content: PixelSize,
        capture_scale: CaptureScale,
        browser_revision: u64,
    ) -> Result<bool, GraphicsError> {
        if pane.is_empty() {
            return Err(GraphicsError::Invalid("pane must be non-empty".into()));
        }
        let changed = self.pane != pane
            || self.geometry.as_ref().is_none_or(|geometry| {
                geometry.viewport != viewport
                    || geometry.content != content
                    || geometry.capture_scale.get() != capture_scale.get()
                    || geometry.browser_revision != browser_revision
            });
        if !changed {
            return Ok(false);
        }
        self.clear_payloads(FrameCleanupReason::Resize);
        self.geometry_revision = self.geometry_revision.saturating_add(1).max(1);
        self.browser_revision = browser_revision;
        self.pane = pane;
        self.geometry = Some(ViewportGeometry::new(
            PaneGeometry {
                origin: crate::presentation::PixelPoint::new(pane.x as u32, pane.y as u32),
                size: PixelSize::new(pane.width as u32, pane.height as u32),
            },
            viewport,
            content,
            capture_scale,
            browser_revision,
            self.geometry_revision,
        )?);
        self.cleaned = false;
        Ok(true)
    }

    /// Submit one bounded payload. The mailbox rejects stale metadata before
    /// this adapter retains any bytes.
    pub fn submit(
        &mut self,
        frame: BrowserFrame,
        payload: &[u8],
    ) -> Result<SubmitResult, GraphicsError> {
        if payload.len() > MAX_FRAME_BYTES {
            return Err(GraphicsError::PayloadTooLarge {
                actual: payload.len(),
                maximum: MAX_FRAME_BYTES,
            });
        }
        if self.cleaned {
            return Err(GraphicsError::Invalid("graphics renderer is shut down".into()));
        }
        if let Some(geometry) = self.geometry.as_ref() {
            geometry.check_snapshot(frame.browser_revision, frame.geometry_revision)?;
        }
        let outcome = self.mailbox.submit(frame.clone())?;
        match outcome {
            crate::presentation::SubmitOutcome::Current => {
                self.current_payload = Some(PayloadFrame {
                    frame: frame.clone(),
                    bytes: payload.to_vec(),
                });
                self.record(frame, FrameOwnershipEvent::Presented);
                Ok(SubmitResult::Presented)
            }
            crate::presentation::SubmitOutcome::Pending => {
                self.pending_payload = Some(PayloadFrame {
                    frame: frame.clone(),
                    bytes: payload.to_vec(),
                });
                self.record(frame, FrameOwnershipEvent::Queued);
                Ok(SubmitResult::Queued)
            }
            crate::presentation::SubmitOutcome::ReplacedPending => {
                self.pending_payload = Some(PayloadFrame {
                    frame: frame.clone(),
                    bytes: payload.to_vec(),
                });
                self.record(
                    frame,
                    FrameOwnershipEvent::Released {
                        reason: FrameCleanupReason::Replaced,
                    },
                );
                Ok(SubmitResult::Replaced)
            }
            crate::presentation::SubmitOutcome::DroppedStale => {
                self.record(
                    frame,
                    FrameOwnershipEvent::Released {
                        reason: FrameCleanupReason::Dropped,
                    },
                );
                Ok(SubmitResult::Stale)
            }
        }
    }

    /// Promote a pending frame after a Ratatui draw completed.
    pub fn present_pending(&mut self) -> Result<bool, GraphicsError> {
        let Some(geometry) = self.geometry.as_ref() else {
            return Ok(false);
        };
        let Some(previous) = self
            .mailbox
            .promote_pending(self.browser_revision, geometry.geometry_revision)?
        else {
            return Ok(false);
        };
        let next = self.pending_payload.take().ok_or_else(|| {
            GraphicsError::Invalid("frame metadata and payload mailbox diverged".into())
        })?;
        self.current_payload = Some(next);
        self.record(
            previous,
            FrameOwnershipEvent::Released {
                reason: FrameCleanupReason::Presented,
            },
        );
        Ok(true)
    }

    pub fn render_current(&self, semantic: &str) -> Result<RenderedFrame, GraphicsError> {
        let semantic = bounded_text(semantic, MAX_SEMANTIC_BYTES);
        let Some(current) = self.current_payload.as_ref() else {
            return Ok(RenderedFrame {
                mode: GraphicsMode::Semantic,
                generation: None,
                pane: self.pane,
                bytes: semantic.into_bytes(),
            });
        };
        if self.mode == GraphicsMode::Semantic {
            return Ok(RenderedFrame {
                mode: GraphicsMode::Semantic,
                generation: Some(current.frame.generation),
                pane: self.pane,
                bytes: semantic.into_bytes(),
            });
        }
        let encoded = base64::engine::general_purpose::STANDARD.encode(&current.bytes);
        let mut output = Vec::with_capacity(KITTY_INTRO.len() + encoded.len() + KITTY_END.len() + 80);
        output.extend_from_slice(KITTY_INTRO);
        output.extend_from_slice(
            format!(
                "a=p,x={},y={},w={},h={},f=100,m=0;",
                self.pane.x, self.pane.y, self.pane.width, self.pane.height
            )
            .as_bytes(),
        );
        output.extend_from_slice(encoded.as_bytes());
        output.extend_from_slice(KITTY_END);
        if output.len() > MAX_OUTPUT_BYTES {
            return Err(GraphicsError::OutputTooLarge {
                actual: output.len(),
                maximum: MAX_OUTPUT_BYTES,
            });
        }
        Ok(RenderedFrame {
            mode: GraphicsMode::Kitty,
            generation: Some(current.frame.generation),
            pane: self.pane,
            bytes: output,
        })
    }

    /// Return a no-artifact cleanup command. It is idempotent and semantic mode
    /// deliberately emits no bytes.
    pub fn cleanup(&mut self) -> Vec<u8> {
        if self.cleaned {
            return Vec::new();
        }
        self.clear_payloads(FrameCleanupReason::Shutdown);
        self.cleaned = true;
        self.cleanup_count = self.cleanup_count.saturating_add(1);
        if self.mode == GraphicsMode::Kitty {
            let mut output = Vec::with_capacity(16);
            output.extend_from_slice(b"\x1b_Ga=d,d=A\x1b\\");
            output
        } else {
            Vec::new()
        }
    }

    pub fn shutdown(&mut self) -> Vec<u8> {
        if self.cleaned {
            return Vec::new();
        }
        self.cleanup()
    }

    pub fn diagnostics(&self) -> GraphicsDiagnostics {
        let counters = self.mailbox.counters();
        GraphicsDiagnostics {
            mode: self.mode,
            accepted_frames: counters.accepted,
            presented_frames: counters.presented,
            replaced_frames: counters.replaced_pending,
            dropped_frames: counters.dropped,
            stale_frames: counters.stale_rejected,
            current_bytes: self.current_payload.as_ref().map_or(0, |frame| frame.bytes.len()),
            pending_bytes: self.pending_payload.as_ref().map_or(0, |frame| frame.bytes.len()),
            geometry_revision: self.geometry_revision,
            cleanup_count: self.cleanup_count,
        }
    }
    pub fn diagnostics_label(&self) -> String {
        let diagnostics = self.diagnostics();
        format!(
            "{} a:{} p:{} q:{} d:{} s:{} b:{}/{} g:{}",
            diagnostics.mode.label(),
            diagnostics.accepted_frames,
            diagnostics.presented_frames,
            diagnostics.replaced_frames,
            diagnostics.dropped_frames,
            diagnostics.stale_frames,
            diagnostics.current_bytes,
            diagnostics.pending_bytes,
            diagnostics.geometry_revision,
        )
    }

    pub fn ownership(&self) -> impl Iterator<Item = &FrameOwnershipRecord> {
        self.ownership.iter()
    }

    pub fn storage_policy(&self) -> FrameStoragePolicy {
        FrameStoragePolicy::default()
    }

    fn clear_payloads(&mut self, reason: FrameCleanupReason) {
        if let Some(frame) = self.current_payload.take() {
            self.record(
                frame.frame,
                FrameOwnershipEvent::Released { reason },
            );
        }
        if let Some(frame) = self.pending_payload.take() {
            self.record(
                frame.frame,
                FrameOwnershipEvent::Released { reason },
            );
        }
        self.mailbox.clear();
    }

    fn record(&mut self, frame: BrowserFrame, event: FrameOwnershipEvent) {
        if self.ownership.len() == MAX_OWNERSHIP_RECORDS {
            self.ownership.pop_front();
        }
        self.ownership.push_back(FrameOwnershipRecord {
            generation: frame.generation,
            browser_revision: frame.browser_revision,
            geometry_revision: frame.geometry_revision,
            event,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitResult {
    Presented,
    Queued,
    Replaced,
    Stale,
}

fn bounded_text(value: &str, maximum: usize) -> String {
    let mut end = value.len().min(maximum);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut output = value[..end].to_string();
    output.retain(|character| !character.is_control() || character == '\n' || character == '\t');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::{
        BrowserFrame, FrameDamage, FrameDropCounts, FrameEncoding, PRESENTATION_CONTRACT_SCHEMA_VERSION,
    };

    fn identity() -> TargetResourceIdentity {
        TargetResourceIdentity::new("tab-1", Some("terminal".into())).unwrap()
    }

    fn frame(generation: u64, geometry_revision: u64) -> BrowserFrame {
        BrowserFrame {
            schema_version: PRESENTATION_CONTRACT_SCHEMA_VERSION,
            generation,
            identity: identity(),
            acquired_at_ms: generation,
            viewport: PixelSize::new(640, 480),
            content: PixelSize::new(640, 480),
            capture_scale: CaptureScale::FULL,
            encoding: FrameEncoding::Png,
            keyframe: true,
            damage: FrameDamage::Full,
            browser_revision: 1,
            geometry_revision,
            dropped: FrameDropCounts::default(),
        }
    }

    #[test]
    fn capability_prefers_kitty_and_falls_back_semantically() {
        assert_eq!(
            negotiate(TerminalEnvironment::new(None, Some("kitty"), None)),
            GraphicsMode::Kitty
        );
        assert_eq!(
            negotiate(TerminalEnvironment::new(Some("xterm"), Some("unknown"), None)),
            GraphicsMode::Semantic
        );
    }

    #[test]
    fn payload_mailbox_replaces_pending_without_growth() {
        let mut graphics = TerminalGraphics::new(GraphicsMode::Semantic, identity()).unwrap();
        graphics
            .resize(
                PaneArea::new(1, 1, 40, 20),
                PixelSize::new(640, 480),
                PixelSize::new(640, 480),
                CaptureScale::FULL,
                1,
            )
            .unwrap();
        assert_eq!(graphics.submit(frame(1, 1), b"one").unwrap(), SubmitResult::Presented);
        assert_eq!(graphics.submit(frame(2, 1), b"two").unwrap(), SubmitResult::Queued);
        assert_eq!(graphics.submit(frame(3, 1), b"three").unwrap(), SubmitResult::Replaced);
        let diagnostics = graphics.diagnostics();
        assert_eq!(diagnostics.pending_bytes, 5);
        assert_eq!(diagnostics.replaced_frames, 1);
    }

    #[test]
    fn stale_frame_is_rejected_and_resize_cleans_payloads() {
        let mut graphics = TerminalGraphics::new(GraphicsMode::Kitty, identity()).unwrap();
        graphics
            .resize(
                PaneArea::new(0, 0, 40, 20),
                PixelSize::new(640, 480),
                PixelSize::new(640, 480),
                CaptureScale::FULL,
                1,
            )
            .unwrap();
        graphics.submit(frame(2, 1), b"frame").unwrap();
        assert_eq!(graphics.submit(frame(1, 1), b"old").unwrap(), SubmitResult::Stale);
        graphics
            .resize(
                PaneArea::new(0, 0, 50, 20),
                PixelSize::new(800, 480),
                PixelSize::new(800, 480),
                CaptureScale::FULL,
                1,
            )
            .unwrap();
        assert_eq!(graphics.diagnostics().current_bytes, 0);
        assert_eq!(graphics.diagnostics().pending_bytes, 0);
    }

    #[test]
    fn semantic_output_is_plain_and_shutdown_has_no_artifact() {
        let mut graphics = TerminalGraphics::new(GraphicsMode::Semantic, identity()).unwrap();
        let output = graphics.render_current("hello\x1b[31m").unwrap();
        assert_eq!(output.as_text(), Some("hello[31m"));
        assert!(graphics.shutdown().is_empty());
        assert!(graphics.shutdown().is_empty());
    }

    #[test]
    fn kitty_output_and_cleanup_are_bounded() {
        let mut graphics = TerminalGraphics::new(GraphicsMode::Kitty, identity()).unwrap();
        graphics
            .resize(
                PaneArea::new(2, 3, 40, 20),
                PixelSize::new(640, 480),
                PixelSize::new(640, 480),
                CaptureScale::FULL,
                1,
            )
            .unwrap();
        graphics.submit(frame(1, 1), b"png").unwrap();
        let output = graphics.render_current("ignored").unwrap();
        assert!(output.bytes.starts_with(KITTY_INTRO));
        assert!(output.bytes.len() <= MAX_OUTPUT_BYTES);
        assert!(!graphics.shutdown().is_empty());
    }
}
