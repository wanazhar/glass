//! Browser-neutral, bounded contracts for browser frame presentation.
//!
//! This module deliberately contains no browser transport, terminal, or image
//! protocol types. A frame is metadata only: implementations may keep a
//! transient encoded payload beside it, but this contract never owns or
//! serializes frame bytes. The [`LatestFrameMailbox`] is the sole frame
//! retention primitive and is bounded to a current frame plus one pending frame.

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Schema version for presentation metadata and metrics envelopes.
pub const PRESENTATION_CONTRACT_SCHEMA_VERSION: u32 = 1;
/// Maximum UTF-8 bytes in a target or resource identity.
pub const MAX_IDENTITY_BYTES: usize = 256;
/// Maximum frame dimension in either axis.
pub const MAX_FRAME_DIMENSION: u32 = 32_768;
/// Maximum damage rectangles in one frame.
pub const MAX_DAMAGE_RECTS: usize = 64;
/// Maximum target frame rate accepted by this contract.
pub const MAX_FRAME_RATE: u16 = 60;
/// Minimum configurable frame rate (idle presentation may use 1 fps).
pub const MIN_FRAME_RATE: u16 = 1;
/// Minimum capture scale (the emergency quality state).
pub const MIN_CAPTURE_SCALE: f32 = 0.5;
/// Maximum capture scale (full-resolution capture).
pub const MAX_CAPTURE_SCALE: f32 = 1.0;

/// A typed validation failure from a presentation contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentationContractError {
    Invalid { field: String, reason: String },
    StaleRevision { expected: u64, actual: u64 },
    TargetMismatch,
    OutsidePane,
}

impl PresentationContractError {
    fn invalid(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            field: field.into(),
            reason: reason.into(),
        }
    }
}

impl Display for PresentationContractError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { field, reason } => write!(f, "invalid {field}: {reason}"),
            Self::StaleRevision { expected, actual } => {
                write!(f, "stale presentation revision: expected {expected}, got {actual}")
            }
            Self::TargetMismatch => write!(f, "frame target does not match mailbox target"),
            Self::OutsidePane => write!(f, "point is outside the presentation pane"),
        }
    }
}

impl std::error::Error for PresentationContractError {}

/// A pixel width and height. Zero-sized geometry is not valid for presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

impl PixelSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn validate(&self, field: &str) -> Result<(), PresentationContractError> {
        if self.width == 0 || self.height == 0 {
            return Err(PresentationContractError::invalid(
                field,
                "width and height must be non-zero",
            ));
        }
        if self.width > MAX_FRAME_DIMENSION || self.height > MAX_FRAME_DIMENSION {
            return Err(PresentationContractError::invalid(
                field,
                format!("dimensions must be at most {MAX_FRAME_DIMENSION} pixels"),
            ));
        }
        Ok(())
    }
}

/// A pixel coordinate in a pane or viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelPoint {
    pub x: u32,
    pub y: u32,
}

/// A bounded pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn validate(&self, field: &str) -> Result<(), PresentationContractError> {
        PixelSize::new(self.width, self.height).validate(field)?;
        let right = self
            .x
            .checked_add(self.width)
            .ok_or_else(|| PresentationContractError::invalid(field, "rectangle overflows"))?;
        let bottom = self
            .y
            .checked_add(self.height)
            .ok_or_else(|| PresentationContractError::invalid(field, "rectangle overflows"))?;
        if right > MAX_FRAME_DIMENSION || bottom > MAX_FRAME_DIMENSION {
            return Err(PresentationContractError::invalid(
                field,
                format!("rectangle must fit within {MAX_FRAME_DIMENSION} pixels"),
            ));
        }
        Ok(())
    }

    pub fn contains(&self, point: PixelPoint) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x.saturating_add(self.width)
            && point.y < self.y.saturating_add(self.height)
    }
}

/// A pane's placement in the terminal's pixel coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaneGeometry {
    pub origin: PixelPoint,
    pub size: PixelSize,
}

impl PaneGeometry {
    pub fn rect(&self) -> PixelRect {
        PixelRect::new(self.origin.x, self.origin.y, self.size.width, self.size.height)
    }

    pub fn validate(&self) -> Result<(), PresentationContractError> {
        self.size.validate("pane.size")?;
        self.rect().validate("pane")
    }
}

/// A capture scale independent from frame pacing.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CaptureScale(f32);

impl CaptureScale {
    pub const FULL: Self = Self(1.0);
    pub const EMERGENCY: Self = Self(0.5);

    pub fn new(value: f32) -> Result<Self, PresentationContractError> {
        if !value.is_finite() || !(MIN_CAPTURE_SCALE..=MAX_CAPTURE_SCALE).contains(&value) {
            return Err(PresentationContractError::invalid(
                "captureScale",
                format!("must be finite and between {MIN_CAPTURE_SCALE} and {MAX_CAPTURE_SCALE}"),
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    pub fn apply(self, size: PixelSize) -> Result<PixelSize, PresentationContractError> {
        size.validate("size")?;
        let width = (size.width as f32 * self.0).round().max(1.0) as u32;
        let height = (size.height as f32 * self.0).round().max(1.0) as u32;
        PixelSize::new(width, height).validate("scaledSize")?;
        Ok(PixelSize::new(width, height))
    }
}

/// An independently validated target presentation rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrameRate(u16);

impl FrameRate {
    pub fn new(value: u16) -> Result<Self, PresentationContractError> {
        if !(MIN_FRAME_RATE..=MAX_FRAME_RATE).contains(&value) {
            return Err(PresentationContractError::invalid(
                "targetFrameRate",
                format!("must be between {MIN_FRAME_RATE} and {MAX_FRAME_RATE} fps"),
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Frame capture and pacing controls. The two controls are intentionally
/// independent: reducing scale does not implicitly lower the target rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationConfig {
    pub target_frame_rate: FrameRate,
    pub capture_scale: CaptureScale,
}

impl Default for PresentationConfig {
    fn default() -> Self {
        Self {
            target_frame_rate: FrameRate(60),
            capture_scale: CaptureScale::FULL,
        }
    }
}

impl PresentationConfig {
    pub fn new(
        target_frame_rate: u16,
        capture_scale: f32,
    ) -> Result<Self, PresentationContractError> {
        Ok(Self {
            target_frame_rate: FrameRate::new(target_frame_rate)?,
            capture_scale: CaptureScale::new(capture_scale)?,
        })
    }

    pub fn validate(&self) -> Result<(), PresentationContractError> {
        FrameRate::new(self.target_frame_rate.0)?;
        CaptureScale::new(self.capture_scale.0)?;
        Ok(())
    }
}

/// Browser viewport and pane placement tied to one browser revision.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewportGeometry {
    pub pane: PaneGeometry,
    pub viewport: PixelSize,
    pub content: PixelSize,
    pub capture_scale: CaptureScale,
    pub browser_revision: u64,
}

impl ViewportGeometry {
    pub fn validate(&self) -> Result<(), PresentationContractError> {
        self.pane.validate()?;
        self.viewport.validate("viewport")?;
        self.content.validate("content")?;
        CaptureScale::new(self.capture_scale.0)?;
        Ok(())
    }

    pub fn check_revision(&self, revision: u64) -> Result<(), PresentationContractError> {
        if revision != self.browser_revision {
            return Err(PresentationContractError::StaleRevision {
                expected: self.browser_revision,
                actual: revision,
            });
        }
        Ok(())
    }

    /// Convert an absolute pane pixel coordinate to a browser viewport pixel.
    /// The revision must be supplied by the caller; stale snapshots are rejected.
    pub fn pane_to_viewport(
        &self,
        point: PixelPoint,
        revision: u64,
    ) -> Result<PixelPoint, PresentationContractError> {
        self.validate()?;
        self.check_revision(revision)?;
        let pane = self.pane.rect();
        if !pane.contains(point) {
            return Err(PresentationContractError::OutsidePane);
        }
        let local_x = point.x - pane.x;
        let local_y = point.y - pane.y;
        Ok(PixelPoint {
            x: ((local_x as u64 * self.viewport.width as u64) / pane.width as u64) as u32,
            y: ((local_y as u64 * self.viewport.height as u64) / pane.height as u64) as u32,
        })
    }

    /// Convert viewport pixels to the encoded capture's pixel dimensions.
    pub fn viewport_to_capture(
        &self,
        point: PixelPoint,
        revision: u64,
    ) -> Result<PixelPoint, PresentationContractError> {
        self.validate()?;
        self.check_revision(revision)?;
        if point.x >= self.viewport.width || point.y >= self.viewport.height {
            return Err(PresentationContractError::OutsidePane);
        }
        Ok(PixelPoint {
            x: ((point.x as f32 * self.capture_scale.0).floor() as u32)
                .min(self.viewport.width.saturating_sub(1)),
            y: ((point.y as f32 * self.capture_scale.0).floor() as u32)
                .min(self.viewport.height.saturating_sub(1)),
        })
    }
}

/// A target and its presentation resource identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetResourceIdentity {
    pub target_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
}

impl TargetResourceIdentity {
    pub fn validate(&self) -> Result<(), PresentationContractError> {
        validate_identity("targetId", &self.target_id)?;
        if let Some(resource_id) = &self.resource_id {
            validate_identity("resourceId", resource_id)?;
        }
        Ok(())
    }
}

/// Supported frame encodings. The presentation contract does not decode them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameEncoding {
    Png,
    Jpeg,
    Webp,
    Av1,
    RawRgba,
}

/// A damage description for a frame, bounded to a small set of rectangles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum FrameDamage {
    Full,
    Rectangles { rects: Vec<PixelRect> },
}

impl FrameDamage {
    pub fn validate(&self) -> Result<(), PresentationContractError> {
        if let Self::Rectangles { rects } = self {
            if rects.len() > MAX_DAMAGE_RECTS {
                return Err(PresentationContractError::invalid(
                    "damage.rects",
                    format!("at most {MAX_DAMAGE_RECTS} rectangles are allowed"),
                ));
            }
            for rect in rects {
                rect.validate("damage.rect")?;
            }
        }
        Ok(())
    }
}

/// Counts observed by the producer before this frame was acquired.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameDropCounts {
    pub producer_dropped: u64,
    pub mailbox_dropped: u64,
    pub stale_rejected: u64,
}

/// Metadata for one acquired browser frame. No pixel payload is retained or
/// serialized; implementations own transient bytes separately and explicitly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserFrame {
    pub schema_version: u32,
    pub generation: u64,
    pub identity: TargetResourceIdentity,
    pub acquired_at_ms: u64,
    pub viewport: PixelSize,
    pub content: PixelSize,
    pub capture_scale: CaptureScale,
    pub encoding: FrameEncoding,
    pub keyframe: bool,
    pub damage: FrameDamage,
    pub browser_revision: u64,
    pub dropped: FrameDropCounts,
}

impl BrowserFrame {
    pub fn validate(&self) -> Result<(), PresentationContractError> {
        if self.schema_version != PRESENTATION_CONTRACT_SCHEMA_VERSION {
            return Err(PresentationContractError::invalid(
                "schemaVersion",
                "unsupported presentation contract schema version",
            ));
        }
        self.identity.validate()?;
        PixelSize::new(self.viewport.width, self.viewport.height).validate("viewport")?;
        PixelSize::new(self.content.width, self.content.height).validate("content")?;
        CaptureScale::new(self.capture_scale.0)?;
        self.damage.validate()
    }

    pub fn from_json(input: &str) -> Result<Self, PresentationContractError> {
        let frame: Self = serde_json::from_str(input).map_err(|error| {
            PresentationContractError::invalid("$", format!("invalid browser frame: {error}"))
        })?;
        frame.validate()?;
        Ok(frame)
    }

    pub fn to_json(&self) -> Result<String, PresentationContractError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            PresentationContractError::invalid("$", format!("failed to serialize frame: {error}"))
        })
    }

    pub fn revision_token(&self) -> RevisionToken {
        RevisionToken {
            browser_revision: self.browser_revision,
            frame_generation: self.generation,
        }
    }

    pub fn accepts_revision(&self, revision: u64) -> Result<(), PresentationContractError> {
        if revision != self.browser_revision {
            return Err(PresentationContractError::StaleRevision {
                expected: self.browser_revision,
                actual: revision,
            });
        }
        Ok(())
    }
}

/// Explicit revision relationship for frames, geometry, and semantic overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevisionToken {
    pub browser_revision: u64,
    pub frame_generation: u64,
}

impl RevisionToken {
    pub fn require_same_browser_revision(
        self,
        other: Self,
    ) -> Result<(), PresentationContractError> {
        if self.browser_revision != other.browser_revision {
            return Err(PresentationContractError::StaleRevision {
                expected: self.browser_revision,
                actual: other.browser_revision,
            });
        }
        Ok(())
    }
}

/// Reasons a presentation may intentionally degrade or shed work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DegradationReason {
    Backpressure,
    TerminalCapability,
    BackendCapability,
    CpuPressure,
    MemoryPressure,
    CaptureScaleReduced,
    FrameRateReduced,
    StaleRevision,
    HiddenTarget,
    UnsupportedSurface,
}

/// Presentation policy mode, separate from terminal graphics selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresentationMode {
    Interactive,
    Settled,
    Idle,
    Background,
    SemanticOnly,
    Degraded { reason: DegradationReason },
}

/// Counters for one bounded latest-frame mailbox.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxCounters {
    pub accepted: u64,
    pub presented: u64,
    pub replaced_pending: u64,
    pub dropped: u64,
    pub stale_rejected: u64,
}

/// Result of offering a frame to the mailbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Current,
    Pending,
    ReplacedPending,
    DroppedStale,
}

/// Bounded latest-state frame delivery. It retains at most one current frame
/// and one newest pending frame; obsolete pending frames are dropped eagerly.
#[derive(Debug, Default)]
pub struct LatestFrameMailbox {
    identity: Option<TargetResourceIdentity>,
    current: Option<BrowserFrame>,
    pending: Option<BrowserFrame>,
    counters: MailboxCounters,
}

impl LatestFrameMailbox {
    pub fn new(identity: TargetResourceIdentity) -> Result<Self, PresentationContractError> {
        identity.validate()?;
        Ok(Self {
            identity: Some(identity),
            ..Self::default()
        })
    }

    pub fn submit(&mut self, frame: BrowserFrame) -> Result<SubmitOutcome, PresentationContractError> {
        frame.validate()?;
        if self.identity.as_ref() != Some(&frame.identity) {
            return Err(PresentationContractError::TargetMismatch);
        }
        let revision_is_stale = self.current.as_ref().is_some_and(|current| {
            frame.browser_revision < current.browser_revision
        }) || self.pending.as_ref().is_some_and(|pending| {
            frame.browser_revision < pending.browser_revision
        });
        if revision_is_stale
            || self
                .current
                .as_ref()
                .is_some_and(|current| frame.generation <= current.generation)
            || self
                .pending
                .as_ref()
                .is_some_and(|pending| frame.generation <= pending.generation)
        {
            self.counters.stale_rejected = self.counters.stale_rejected.saturating_add(1);
            self.counters.dropped = self.counters.dropped.saturating_add(1);
            return Ok(SubmitOutcome::DroppedStale);
        }
        self.counters.accepted = self.counters.accepted.saturating_add(1);
        if self.current.is_none() {
            self.current = Some(frame);
            return Ok(SubmitOutcome::Current);
        }
        if self.pending.is_some() {
            self.pending = Some(frame);
            self.counters.replaced_pending = self.counters.replaced_pending.saturating_add(1);
            self.counters.dropped = self.counters.dropped.saturating_add(1);
            Ok(SubmitOutcome::ReplacedPending)
        } else {
            self.pending = Some(frame);
            Ok(SubmitOutcome::Pending)
        }
    }

    pub fn current(&self) -> Option<&BrowserFrame> {
        self.current.as_ref()
    }

    pub fn pending(&self) -> Option<&BrowserFrame> {
        self.pending.as_ref()
    }

    pub fn counters(&self) -> MailboxCounters {
        self.counters
    }

    pub fn len(&self) -> usize {
        usize::from(self.current.is_some()) + usize::from(self.pending.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.current.is_none() && self.pending.is_none()
    }

    /// Mark the current frame presented and promote the newest pending frame.
    pub fn promote_pending(&mut self) -> Option<BrowserFrame> {
        let Some(next) = self.pending.take() else {
            return None;
        };
        let previous = self.current.replace(next);
        self.counters.presented = self.counters.presented.saturating_add(1);
        previous
    }
 
    /// Explicitly release all metadata and reset the mailbox. No bytes are
    /// present in this contract, so cleanup is deterministic and allocation-free.
    pub fn clear(&mut self) {
        self.current = None;
        self.pending = None;
        self.identity = None;
    }
}

/// Why a frame's ownership changed. These events are metadata-only cleanup
/// signals; they do not imply persistence of the frame payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameCleanupReason {
    Presented,
    Replaced,
    Dropped,
    Resize,
    ModeChange,
    Shutdown,
    PanicRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum FrameOwnershipEvent {
    Acquired,
    Queued,
    Presented,
    Released { reason: FrameCleanupReason },
}

/// A revision-bound ownership event for lifecycle observers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameOwnershipRecord {
    pub generation: u64,
    pub browser_revision: u64,
    pub event: FrameOwnershipEvent,
}

/// Whether a backend may persist payload bytes. Default is deliberately false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameStoragePolicy {
    pub persist_bytes: bool,
}

impl Default for FrameStoragePolicy {
    fn default() -> Self {
        Self { persist_bytes: false }
    }
}

/// Cumulative presentation metrics. Latencies are integer milliseconds and are
/// updated by deterministic caller-supplied timestamps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationMetrics {
    pub acquired_frames: u64,
    pub presented_frames: u64,
    pub dropped_frames: u64,
    pub stale_frames: u64,
    pub encoded_bytes: u64,
    pub acquire_latency_ms: u64,
    pub present_latency_ms: u64,
    pub input_to_frame_latency_ms: u64,
    pub ack_delay_ms: u64,
    pub ack_count: u64,
    pub pending_count: u32,
    #[serde(skip)]
    last_input_at_ms: Option<u64>,
}

impl PresentationMetrics {
    pub fn record_input(&mut self, at_ms: u64) {
        self.last_input_at_ms = Some(at_ms);
    }

    pub fn record_acquired(
        &mut self,
        frame: &BrowserFrame,
        observed_at_ms: u64,
        encoded_bytes: u64,
    ) -> Result<(), PresentationContractError> {
        frame.validate()?;
        self.acquired_frames = self.acquired_frames.saturating_add(1);
        self.encoded_bytes = self.encoded_bytes.saturating_add(encoded_bytes);
        self.acquire_latency_ms = self
            .acquire_latency_ms
            .saturating_add(observed_at_ms.saturating_sub(frame.acquired_at_ms));
        Ok(())
    }

    pub fn record_presented(
        &mut self,
        frame: &BrowserFrame,
        presented_at_ms: u64,
    ) -> Result<(), PresentationContractError> {
        frame.validate()?;
        self.presented_frames = self.presented_frames.saturating_add(1);
        self.present_latency_ms = self
            .present_latency_ms
            .saturating_add(presented_at_ms.saturating_sub(frame.acquired_at_ms));
        if let Some(input_at_ms) = self.last_input_at_ms {
            self.input_to_frame_latency_ms = self
                .input_to_frame_latency_ms
                .saturating_add(presented_at_ms.saturating_sub(input_at_ms));
            self.last_input_at_ms = None;
        }
        Ok(())
    }

    pub fn record_ack_delay(&mut self, delay_ms: u64) {
        self.ack_delay_ms = self.ack_delay_ms.saturating_add(delay_ms);
        self.ack_count = self.ack_count.saturating_add(1);
    }

    pub fn record_dropped(&mut self, count: u64) {
        self.dropped_frames = self.dropped_frames.saturating_add(count);
    }

    pub fn record_stale(&mut self, count: u64) {
        self.stale_frames = self.stale_frames.saturating_add(count);
    }

    pub fn set_pending_count(&mut self, count: usize) -> Result<(), PresentationContractError> {
        if count > 1 {
            return Err(PresentationContractError::invalid(
                "pendingCount",
                "presentation may retain at most one pending frame",
            ));
        }
        self.pending_count = count as u32;
        Ok(())
    }
}

fn validate_identity(field: &str, value: &str) -> Result<(), PresentationContractError> {
    if value.is_empty() || value.len() > MAX_IDENTITY_BYTES {
        return Err(PresentationContractError::invalid(
            field,
            format!("must be 1..={MAX_IDENTITY_BYTES} UTF-8 bytes"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> TargetResourceIdentity {
        TargetResourceIdentity {
            target_id: "target-1".into(),
            resource_id: Some("resource-1".into()),
        }
    }

    fn frame(generation: u64, revision: u64) -> BrowserFrame {
        BrowserFrame {
            schema_version: PRESENTATION_CONTRACT_SCHEMA_VERSION,
            generation,
            identity: identity(),
            acquired_at_ms: generation * 10,
            viewport: PixelSize::new(100, 50),
            content: PixelSize::new(100, 50),
            capture_scale: CaptureScale::FULL,
            encoding: FrameEncoding::Png,
            keyframe: generation == 1,
            damage: FrameDamage::Full,
            browser_revision: revision,
            dropped: FrameDropCounts::default(),
        }
    }

    #[test]
    fn mailbox_replaces_pending_and_never_exceeds_two_frames() {
        let mut mailbox = LatestFrameMailbox::new(identity()).unwrap();
        assert_eq!(mailbox.submit(frame(1, 1)).unwrap(), SubmitOutcome::Current);
        assert_eq!(mailbox.submit(frame(2, 1)).unwrap(), SubmitOutcome::Pending);
        assert_eq!(mailbox.submit(frame(3, 1)).unwrap(), SubmitOutcome::ReplacedPending);
        assert_eq!(mailbox.len(), 2);
        assert_eq!(mailbox.pending().unwrap().generation, 3);
        assert_eq!(mailbox.counters().replaced_pending, 1);
        assert_eq!(mailbox.counters().dropped, 1);
        mailbox.promote_pending();
        assert_eq!(mailbox.current().unwrap().generation, 3);
        assert_eq!(mailbox.len(), 1);
    }

    #[test]
    fn mailbox_rejects_stale_and_wrong_target_frames() {
        let mut mailbox = LatestFrameMailbox::new(identity()).unwrap();
        mailbox.submit(frame(4, 2)).unwrap();
        assert_eq!(mailbox.submit(frame(3, 2)).unwrap(), SubmitOutcome::DroppedStale);
        let mut wrong = frame(5, 2);
        wrong.identity.target_id = "other".into();
        assert_eq!(mailbox.submit(wrong), Err(PresentationContractError::TargetMismatch));
        assert_eq!(mailbox.counters().stale_rejected, 1);
    }

    #[test]
    fn geometry_converts_and_rejects_stale_snapshot() {
        let geometry = ViewportGeometry {
            pane: PaneGeometry {
                origin: PixelPoint { x: 10, y: 20 },
                size: PixelSize::new(200, 100),
            },
            viewport: PixelSize::new(100, 50),
            content: PixelSize::new(100, 50),
            capture_scale: CaptureScale::FULL,
            browser_revision: 7,
        };
        assert_eq!(
            geometry
                .pane_to_viewport(PixelPoint { x: 110, y: 70 }, 7)
                .unwrap(),
            PixelPoint { x: 50, y: 25 }
        );
        assert!(matches!(
            geometry.pane_to_viewport(PixelPoint { x: 110, y: 70 }, 6),
            Err(PresentationContractError::StaleRevision { .. })
        ));
    }

    #[test]
    fn metrics_update_latency_bytes_and_pending_bounds() {
        let mut metrics = PresentationMetrics::default();
        let one = frame(1, 1);
        metrics.record_input(12);
        metrics.record_acquired(&one, 20, 128).unwrap();
        metrics.record_presented(&one, 35).unwrap();
        metrics.record_ack_delay(4);
        metrics.record_dropped(2);
        metrics.record_stale(1);
        metrics.set_pending_count(1).unwrap();
        assert_eq!(metrics.acquire_latency_ms, 10);
        assert_eq!(metrics.present_latency_ms, 25);
        assert_eq!(metrics.input_to_frame_latency_ms, 23);
        assert_eq!(metrics.encoded_bytes, 128);
        assert_eq!(metrics.ack_delay_ms, 4);
        assert!(metrics.set_pending_count(2).is_err());
    }

    #[test]
    fn scale_and_rate_validation_are_independent() {
        assert!(FrameRate::new(0).is_err());
        assert!(FrameRate::new(61).is_err());
        assert_eq!(PresentationConfig::new(30, 0.5).unwrap().target_frame_rate.get(), 30);
        assert!(CaptureScale::new(0.49).is_err());
        assert!(CaptureScale::new(1.01).is_err());
        assert!(PresentationConfig::new(60, 0.5).is_ok());
    }

    #[test]
    fn serde_limits_reject_unknown_fields_and_oversized_damage() {
        let mut value = serde_json::to_value(frame(1, 1)).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(BrowserFrame::from_json(&value.to_string()).is_err());
        let mut oversized = frame(1, 1);
        oversized.damage = FrameDamage::Rectangles {
            rects: (0..=MAX_DAMAGE_RECTS)
                .map(|_| PixelRect::new(0, 0, 1, 1))
                .collect(),
        };
        assert!(oversized.to_json().is_err());
        let mut long = frame(1, 1);
        long.identity.target_id = "x".repeat(MAX_IDENTITY_BYTES + 1);
        assert!(long.to_json().is_err());
    }
}
