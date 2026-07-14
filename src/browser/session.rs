use base64::{Engine, engine::general_purpose::STANDARD};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::Mutex;

use super::cdp::{CdpClient, CdpEventWithParams};
use super::chrome::{
    ChromeProcess, PortLaunchLock, check_chrome_health, get_browser_ws_url, get_ws_url,
    is_port_occupied, launch_chrome_with_options, resolve_chrome_path,
};
use super::dom::{
    AxNode, CompactAxNode, CompactInteractiveElement, DomNode, backend_node_reference,
    find_interactive_elements, format_tree, parse_accessibility_tree, parse_dom_tree,
    project_compact_accessibility,
};
use super::mouse::{MouseEngine, Point};
use super::policy::{BrowserPolicy, PolicyCapability, PolicyError, PolicyPreset};
use super::profile::ProfileManager;

pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

/// Maximum UTF-8 byte length of visible text returned by a compact observation.
pub const COMPACT_TEXT_MAX_BYTES: usize = 16 * 1024;
const TEXT_TRUNCATION_MARKER: &str = "\n[truncated]";
const COMPACT_OBSERVATION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(1);
const COMPACT_OBSERVATION_MAX_ATTEMPTS: u8 = 2;
const COMPACT_PAGE_STATE_EXPRESSION: &str = r#"(() => {
    const key = '__glassObservationRevision';
    let state = globalThis[key];
    if (!state) {
        state = {revision: 0};
        const observer = new MutationObserver(() => { state.revision += 1; });
        observer.observe(document, {subtree:true, childList:true, attributes:true, characterData:true});
        globalThis[key] = state;
    }
    const summary = {scanned_elements:0, scan_limit:512, shadow_roots:0, child_frames:0, canvases:0, truncated:false};
    const walker = document.createTreeWalker(document, NodeFilter.SHOW_ELEMENT);
    while (walker.nextNode()) {
        if (summary.scanned_elements >= summary.scan_limit) { summary.truncated = true; break; }
        const element = walker.currentNode;
        summary.scanned_elements += 1;
        if (element.shadowRoot) summary.shadow_roots += 1;
        if (element.localName === 'iframe' || element.localName === 'frame') summary.child_frames += 1;
        if (element.localName === 'canvas') summary.canvases += 1;
    }
    return JSON.stringify({url:location.href, title:document.title, ready_state:document.readyState,
        text:(() => { const source=document.body ? document.body.innerText : ''; const bytes=new Uint8Array(16384);
            const encoded=new TextEncoder().encodeInto(source, bytes); summary.text_truncated=encoded.read < source.length;
            return new TextDecoder().decode(bytes.subarray(0, encoded.written)); })(),
        mutation_revision:state.revision, boundaries:summary});
})()"#;
const OWNED_BROWSER_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const AMBIGUOUS_CANDIDATE_LIMIT: usize = 8;
const CANDIDATE_LABEL_MAX_BYTES: usize = 160;
const TOPOLOGY_MAX_TARGETS: usize = 32;
const TOPOLOGY_MAX_FRAMES: usize = 128;
const TOPOLOGY_ID_MAX_BYTES: usize = 256;
const TOPOLOGY_TEXT_MAX_BYTES: usize = 1024;
const TOPOLOGY_MAX_EVENTS: usize = 64;
const POPUP_WITNESS_LIFETIME_MS: u64 = 5_000;
const POPUP_EVIDENCE_DEADLINE: Duration = Duration::from_secs(2);
const POPUP_VERIFY_CALL_TIMEOUT: Duration = Duration::from_secs(2);
const POPUP_RELEASE_ACK_TIMEOUT: Duration = Duration::from_millis(500);
const POPUP_ERROR_MESSAGE_MAX_BYTES: usize = 512;
const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const WAIT_LAST_STATE_MAX_BYTES: usize = 512;
const NETWORK_IN_FLIGHT_LIMIT: usize = 1024;
const MAX_WAIT_DEADLINE: Duration = Duration::from_secs(300);
const MAX_WAIT_CONDITION_BYTES: usize = 4 * 1024;
const MAX_DIAGNOSTIC_DURATION: Duration = Duration::from_secs(30);
const MAX_DIAGNOSTIC_EVENTS: usize = 128;
const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 2 * 1024;
const MAX_DIAGNOSTIC_URL_BYTES: usize = 4 * 1024;
// Eight megapixels bounds worst-case 4-byte pixels plus base64 below 64 MiB
// before Chrome is asked to encode or the generic CDP actor receives JSON.
const MAX_VISUAL_PIXELS: f64 = 8.0 * 1024.0 * 1024.0;
const MAX_VISUAL_BASE64_BYTES: usize = 64 * 1024 * 1024;
const VISUAL_HEADER_BASE64_BYTES: usize = 64 * 1024;
const HIT_TEST_FUNCTION: &str = r#"async function() {
    let element = this && this.nodeType === Node.ELEMENT_NODE ? this : this && this.parentElement;
    if (element) element = element.closest('button,a,input,select,textarea,[role],[tabindex]') || element;
    if (!element || !element.isConnected) return {ok:false, reason:'detached'};
    const sample = () => {
        const rect = element.getBoundingClientRect();
        return {left:rect.left, top:rect.top, width:rect.width, height:rect.height};
    };
    element.scrollIntoView({block:'nearest', inline:'nearest'});
    const first = sample();
    if (!element.isConnected) return {ok:false, reason:'detached'};
    if (element.getAnimations({subtree:true}).some(animation => animation.playState === 'running'))
        return {ok:false, reason:'unstable_geometry'};
    const second = sample();
    const style = getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || second.width <= 0 || second.height <= 0)
        return {ok:false, reason:'not_visible'};
    if (element.matches(':disabled') || element.getAttribute('aria-disabled') === 'true')
        return {ok:false, reason:'disabled'};
    if ([first.left, first.top, first.width, first.height].some((value, index) => Math.abs(value - [second.left, second.top, second.width, second.height][index]) > 1))
        return {ok:false, reason:'unstable_geometry'};
    const x = second.left + second.width / 2;
    const y = second.top + second.height / 2;
    if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight)
        return {ok:false, reason:'outside_viewport'};
    const hit = document.elementFromPoint(x, y);
    if (!hit || (hit !== element && !element.contains(hit)))
        return {ok:false, reason:'hit_test_blocked'};
    return {ok:true, x, y};
}"#;
const WAIT_TARGET_STATE_FUNCTION: &str = r#"function() {
    let element = this && this.nodeType === Node.ELEMENT_NODE ? this : this && this.parentElement;
    if (!element || !element.isConnected) return {attached:false, visible:false, enabled:false};
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    const visible = style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity) !== 0 && rect.width > 0 && rect.height > 0;
    const enabled = !element.matches(':disabled') && element.getAttribute('aria-disabled') !== 'true';
    return {attached:true, visible, enabled, geometry:[rect.left, rect.top, rect.width, rect.height].map(value => Math.round(value * 10) / 10).join(',')};
}"#;

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub port: u16,
    pub chrome_path: Option<PathBuf>,
    pub profile: String,
    pub incognito: bool,
    /// Attach to an existing Chrome CDP endpoint instead of launching Chrome.
    pub attach: bool,
    /// Explicit Chrome page target ID, required whenever the endpoint has more
    /// than one page target.
    pub target_id: Option<String>,
    pub frame_id: Option<String>,
    pub headed: bool,
    pub interaction_mode: InteractionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InteractionMode {
    Human,
    Fast,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            port: 9222,
            chrome_path: None,
            profile: "default".to_string(),
            incognito: false,
            attach: false,
            target_id: None,
            frame_id: None,
            headed: false,
            interaction_mode: InteractionMode::Human,
        }
    }
}

impl SessionOptions {
    /// Validate combinations that cannot be honored by an attached session.
    pub fn validate(&self) -> BrowserResult<()> {
        if self
            .target_id
            .as_deref()
            .is_some_and(|target_id| target_id.trim().is_empty())
        {
            return Err("target ID cannot be empty".into());
        }
        if self
            .frame_id
            .as_deref()
            .is_some_and(|frame_id| frame_id.trim().is_empty())
        {
            return Err("frame ID cannot be empty".into());
        }

        if self.attach {
            if self.incognito {
                return Err("--attach cannot be combined with --incognito".into());
            }
            if self.profile != "default" {
                return Err(
                    "--attach cannot be combined with a named --profile; attached Chrome owns its profile"
                        .into(),
                );
            }
            if self.chrome_path.is_some() {
                return Err("--attach cannot be combined with --chrome-path".into());
            }
            if self.headed {
                return Err("--attach cannot be combined with --headed".into());
            }
        } else {
            ProfileManager::validate_name(&self.profile)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub ready_state: String,
    #[serde(default)]
    pub target_id: String,
    #[serde(default)]
    pub frame_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PageTargetInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opener_id: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub url: String,
    pub active: bool,
    pub out_of_process: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyEventSummary {
    pub sequence: u64,
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone)]
struct DestroyedPageTarget {
    target: PageTargetInfo,
    observed_sequence: u64,
}

#[derive(Default)]
struct TopologyRegistry {
    targets: Vec<PageTargetInfo>,
    frames: Vec<FrameInfo>,
    active_target_id: Option<String>,
    active_frame_id: Option<String>,
    active_target_session_id: Option<String>,
    active_session_id: Option<String>,
    frame_sessions: HashMap<String, String>,
    frame_parents: HashMap<String, String>,
    events: VecDeque<TopologyEventSummary>,
    sequence: u64,
    event_loss_count: u64,
    target_sequences: HashMap<String, u64>,
    destroyed_targets: VecDeque<DestroyedPageTarget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveElement {
    pub reference: String,
    pub role: String,
    pub name: String,
    pub description: String,
    pub backend_dom_node_id: i64,
}

/// An explicit, deterministic element lookup strategy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    Reference(String),
    AccessibleName(String),
    RoleAndName { role: String, name: String },
    Text(String),
    Css(String),
    Ordinal(usize),
}

/// A bounded description returned when a locator is ambiguous.
#[derive(Debug, Clone, Serialize)]
pub struct CandidateSummary {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A bounded, structured targeting failure safe for agent-facing protocols.
#[derive(Debug, Clone, Serialize)]
pub struct TargetError {
    pub kind: TargetErrorKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<TargetActionabilityReason>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CandidateSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetActionabilityReason {
    Detached,
    NotVisible,
    Disabled,
    UnstableGeometry,
    OutsideViewport,
    HitTestBlocked,
    GeometryChanged,
    NodeUnavailable,
    VerificationFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetErrorKind {
    Ambiguous,
    NotFound,
    StaleReference,
    NotActionable,
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.kind {
            TargetErrorKind::Ambiguous => "element target is ambiguous",
            TargetErrorKind::NotFound => "element target was not found",
            TargetErrorKind::StaleReference => "element reference is stale",
            TargetErrorKind::NotActionable => "element target is not actionable",
        };
        formatter.write_str(message)
    }
}

impl Error for TargetError {}

#[derive(Debug)]
enum TargetResolution {
    Unique(ResolvedElement),
    Ambiguous(Vec<CandidateSummary>),
    NotFound,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessibilitySnapshot {
    pub page: PageInfo,
    pub roots: Vec<AxNode>,
    pub interactive: Vec<InteractiveElement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitCondition {
    Lifecycle(String),
    UrlExact(String),
    UrlPrefix(String),
    TargetAttached(String),
    TargetVisible(String),
    TargetHidden(String),
    TargetEnabled(String),
    TargetStable(String),
    Text(String),
    JavaScript(String),
    NetworkQuiet(Duration),
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitOutcome {
    pub condition: String,
    pub elapsed_ms: u64,
    pub last_state: String,
    pub target_id: String,
    pub frame_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WaitTimeout {
    pub condition: String,
    pub deadline_ms: u64,
    pub last_state: String,
    pub reason: &'static str,
}

impl std::fmt::Display for WaitTimeout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "wait timed out for {}", self.condition)
    }
}

impl Error for WaitTimeout {}

/// Bounded accessibility state included with compact page observations.
#[derive(Debug, Clone, Serialize)]
pub struct CompactAccessibilitySnapshot {
    pub page: PageInfo,
    /// Page generation used by every published interactive reference.
    pub revision: u64,
    pub roots: Vec<CompactAxNode>,
    pub interactive: Vec<CompactInteractiveElement>,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

/// Structured page state. Default observations omit optional deep data.
#[derive(Debug, Clone, Serialize)]
pub struct PageContext {
    pub page: PageInfo,
    pub text: String,
    /// Full DOM data is included only by an explicit deep-DOM observation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomNode>,
    pub accessibility: CompactAccessibilitySnapshot,
    pub consistency: ObservationConsistency,
    pub boundaries: ObservationBoundarySummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub incomplete: Vec<ObservationIncompleteReason>,
    /// Base64 PNG data is populated only when visual context is explicitly requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObservationConsistency {
    pub consistent: bool,
    pub attempts: u8,
    pub start_revision: u64,
    pub end_revision: u64,
    pub start_mutation_revision: u64,
    pub end_mutation_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticReport {
    pub target_id: String,
    pub frame_id: String,
    pub duration_ms: u64,
    pub console: Vec<ConsoleEvidence>,
    pub network: Vec<NetworkEvidence>,
    pub dropped_events: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsoleEvidence {
    pub level: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkEvidence {
    pub request_id: String,
    pub method: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub safe_header_names: Vec<String>,
    pub redirect_count: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadOutcome {
    pub guid: String,
    pub suggested_filename: String,
    pub state: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub target_id: String,
    pub frame_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum VisualFormat {
    Png,
    Jpeg,
    Webp,
}

impl VisualFormat {
    pub fn as_cdp(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
        }
    }
}

impl std::str::FromStr for VisualClip {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let values = value
            .split(',')
            .map(|part| {
                part.trim()
                    .parse::<f64>()
                    .map_err(|_| "clip must be x,y,width,height".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 4 {
            return Err("clip must be x,y,width,height".to_string());
        }
        Ok(Self {
            x: values[0],
            y: values[1],
            width: values[2],
            height: values[3],
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VisualClip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct VisualCaptureOptions {
    pub format: VisualFormat,
    pub quality: Option<u8>,
    pub scale: f64,
    pub clip: Option<VisualClip>,
    pub full_page: bool,
    pub target: Option<String>,
}

impl Default for VisualCaptureOptions {
    fn default() -> Self {
        Self {
            format: VisualFormat::Png,
            quality: None,
            scale: 1.0,
            clip: None,
            full_page: false,
            target: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VisualCaptureMetadata {
    pub format: VisualFormat,
    pub width: usize,
    pub height: usize,
    pub encoded_bytes: usize,
    pub device_scale_factor: f64,
    pub scale: f64,
    pub full_page: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<VisualClip>,
    pub target_id: String,
    pub frame_id: String,
}

#[derive(Debug)]
pub struct VisualCapture {
    pub data: String,
    pub metadata: VisualCaptureMetadata,
}

#[cfg(feature = "visual-compare")]
#[derive(Debug, Serialize)]
pub struct VisualComparison {
    pub width: u32,
    pub height: u32,
    pub changed_pixels: u64,
    pub changed_ratio: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference_box: Option<VisualClip>,
}

#[cfg(feature = "visual-compare")]
pub fn compare_png_visuals(first: &str, second: &str) -> BrowserResult<VisualComparison> {
    let first = decode_png_for_comparison(first)?;
    let second = decode_png_for_comparison(second)?;
    if first.0 != second.0 || first.1 != second.1 || first.2 != second.2 {
        return Err("visual comparison requires equal PNG dimensions and color layout".into());
    }
    let (width, height, samples, first_pixels) = first;
    let second_pixels = second.3;
    let mut changed = 0_u64;
    let mut left = width;
    let mut top = height;
    let mut right = 0_u32;
    let mut bottom = 0_u32;
    for (index, (a, b)) in first_pixels
        .chunks_exact(samples)
        .zip(second_pixels.chunks_exact(samples))
        .enumerate()
    {
        if a != b {
            changed += 1;
            let x = index as u32 % width;
            let y = index as u32 / width;
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    Ok(VisualComparison {
        width,
        height,
        changed_pixels: changed,
        changed_ratio: changed as f64 / (u64::from(width) * u64::from(height)) as f64,
        difference_box: (changed > 0).then_some(VisualClip {
            x: f64::from(left),
            y: f64::from(top),
            width: f64::from(right - left + 1),
            height: f64::from(bottom - top + 1),
        }),
    })
}

#[cfg(feature = "visual-compare")]
fn decode_png_for_comparison(value: &str) -> BrowserResult<(u32, u32, usize, Vec<u8>)> {
    if value.len() > MAX_VISUAL_BASE64_BYTES {
        return Err("comparison PNG exceeded 64 MiB base64 budget".into());
    }
    let encoded = STANDARD.decode(value.as_bytes())?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let output_size = reader.output_buffer_size();
    if output_size > 32 * 1024 * 1024 {
        return Err("comparison PNG decoded size exceeded 32 MiB".into());
    }
    let mut pixels = vec![0; output_size];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    let samples = info.color_type.samples();
    Ok((info.width, info.height, samples, pixels))
}

#[derive(Debug, Serialize)]
pub struct ScreencastFrame {
    pub data: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScreencastStats {
    pub received: u64,
    pub dropped: u64,
}

pub struct ScreencastScope {
    cdp: CdpClient,
    session_id: Option<String>,
    receiver: tokio::sync::mpsc::Receiver<super::cdp::CdpScreencastFrame>,
    armed: bool,
}

struct ScreencastStartupGuard {
    cdp: CdpClient,
    session_id: Option<String>,
    armed: bool,
}

impl ScreencastStartupGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ScreencastStartupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.cdp.close_screencast_channel();
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = stop_screencast_for(&cdp, session_id.as_deref()).await;
        });
    }
}

impl ScreencastScope {
    pub async fn next_frame(&mut self) -> Option<ScreencastFrame> {
        while let Some(frame) = self.receiver.recv().await {
            if frame.session_id == self.session_id {
                return Some(ScreencastFrame {
                    data: frame.data,
                    metadata: frame.metadata,
                });
            }
        }
        None
    }

    pub fn stats(&self) -> ScreencastStats {
        let (received, dropped) = self.cdp.screencast_stats();
        ScreencastStats { received, dropped }
    }

    pub async fn stop(mut self) -> BrowserResult<ScreencastStats> {
        stop_screencast_for(&self.cdp, self.session_id.as_deref()).await?;
        let (received, dropped) = self.cdp.close_screencast_channel();
        self.armed = false;
        Ok(ScreencastStats { received, dropped })
    }
}

impl Drop for ScreencastScope {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.cdp.close_screencast_channel();
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = stop_screencast_for(&cdp, session_id.as_deref()).await;
        });
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ObservationBoundarySummary {
    pub scanned_elements: usize,
    pub scan_limit: usize,
    pub shadow_roots: usize,
    pub child_frames: usize,
    pub canvases: usize,
    pub truncated: bool,
    #[serde(default)]
    pub text_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationIncompleteReason {
    VisibleText,
    AccessibilityNode,
    AccessibilityLabel,
    Control,
    ShadowBoundary,
    FrameBoundary,
    Canvas,
    BoundaryScan,
    MutationRace,
}

/// The completed browser operation represented by an [`ActionOutcome`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Click,
    ClickExpectPopup,
    DoubleClick,
    Hover,
    Drag,
    Type,
    KeyDown,
    KeyUp,
    KeyPress,
    Shortcut,
    Clear,
    Check,
    Uncheck,
    Select,
    Upload,
    Scroll,
}

/// A resolved browser target recorded in an action result.
#[derive(Debug, Clone, Serialize)]
pub struct ActionTarget {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A compact, serializable result from an input action.
///
/// `revision` is the generation after the action invalidated page context. A
/// caller should observe again before reusing a previous element reference.
#[derive(Debug, Clone, Serialize)]
pub struct ActionOutcome {
    pub action: ActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ActionTarget>,
    pub revision: u64,
    pub target_id: String,
    pub frame_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
}

/// Evidence returned by [`BrowserSession::click_expect_popup`].
#[derive(Debug, Clone, Serialize)]
pub struct PopupClickOutcome {
    pub action: ActionKind,
    pub target: ActionTarget,
    pub revision: u64,
    pub target_id: String,
    pub frame_id: String,
    pub causally_verified_popup: bool,
    pub popup_id: String,
    pub opener_id: String,
    pub evidence: PopupVerificationEvidence,
}

#[derive(Debug, Clone, Serialize)]
pub struct PopupVerificationEvidence {
    pub trusted_click_witness: bool,
    pub release_acknowledged: bool,
    pub release_ack_wait_ms: f64,
    pub topology_sequence_before_release: u64,
    pub popup_observed_sequence: u64,
    pub attached: bool,
    pub ready_state: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupClickErrorKind {
    ReleaseFailed,
    WitnessMissing,
    TopologyLagged,
    PopupMissing,
    PopupAmbiguous,
    PopupDestroyed,
    PopupOpenerMismatch,
    PopupUnreadable,
}

/// A fail-closed popup-click failure with a stable machine-readable kind.
#[derive(Debug, Clone, Serialize)]
pub struct PopupClickError {
    pub kind: PopupClickErrorKind,
    pub message: String,
}

impl std::fmt::Display for PopupClickError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "popup click {:?}: {}", self.kind, self.message)
    }
}

impl Error for PopupClickError {}

#[derive(Debug, Clone)]
struct PopupTopologySnapshot {
    original_target_id: String,
    original_frame_id: String,
    preexisting_target_ids: HashSet<String>,
    sequence: u64,
    event_loss_count: u64,
}

#[derive(Debug, Clone)]
struct PopupCandidate {
    target: PageTargetInfo,
    observed_sequence: u64,
}

#[derive(Debug, Clone)]
struct CompactPageContext {
    page: PageInfo,
    text: String,
    accessibility: CompactAccessibilitySnapshot,
    consistency: ObservationConsistency,
    boundaries: ObservationBoundarySummary,
    incomplete: Vec<ObservationIncompleteReason>,
}

impl CompactPageContext {
    fn into_page_context(self) -> PageContext {
        PageContext {
            page: self.page,
            text: self.text,
            dom: None,
            accessibility: self.accessibility,
            consistency: self.consistency,
            boundaries: self.boundaries,
            incomplete: self.incomplete,
            screenshot: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EvaluatedPageState {
    url: String,
    title: String,
    #[serde(default)]
    ready_state: String,
    #[serde(default)]
    mutation_revision: u64,
    #[serde(default)]
    boundaries: ObservationBoundarySummary,
    text: String,
}

impl AccessibilitySnapshot {
    pub fn format(&self) -> String {
        let mut output = format!(
            "url: {}\ntitle: {}\nreadyState: {}\n\n{}",
            self.page.url,
            self.page.title,
            self.page.ready_state,
            format_tree(&self.roots, 0)
        );
        if !self.interactive.is_empty() {
            output.push_str("\nInteractive elements:\n");
            for element in &self.interactive {
                output.push_str(&format!(
                    "{} [{}] {}\n",
                    element.reference, element.role, element.name
                ));
            }
        }
        output
    }
}

/// A browser process, one CDP page connection, and its profile state.
pub struct BrowserSession {
    cdp: CdpClient,
    chrome: Option<ChromeProcess>,
    disposable_profile: Option<DisposableProfileDir>,
    profile: String,
    interaction_mode: InteractionMode,
    mouse: MouseEngine,
    pointer: Mutex<Option<Point>>,
    page_revision: Arc<AtomicU64>,
    observation_cache: Mutex<Option<CachedObservation>>,
    network_wait_leases: Arc<Mutex<NetworkLeaseState>>,
    diagnostic_leases: Arc<Mutex<DiagnosticLeaseState>>,
    download_scope: Arc<Mutex<()>>,
    topology: Arc<Mutex<TopologyRegistry>>,
    popup_click_scope: Mutex<()>,
    upload_root: PathBuf,
    policy: BrowserPolicy,
    policy_interception: Option<PolicyInterception>,
}

struct CachedObservation {
    revision: u64,
    context: CompactPageContext,
}

type PausedPolicyRequests = Arc<Mutex<HashSet<(Option<String>, String)>>>;

struct PolicyInterception {
    cdp: CdpClient,
    sessions: Arc<Mutex<HashSet<String>>>,
    paused: PausedPolicyRequests,
    last_denial: Arc<Mutex<Option<PolicyError>>>,
    worker: tokio::task::JoinHandle<()>,
}

impl PolicyInterception {
    async fn start(
        cdp: CdpClient,
        policy: BrowserPolicy,
        initial_session: String,
    ) -> BrowserResult<Self> {
        let mut events = cdp.subscribe_events_with_params();
        let sessions = Arc::new(Mutex::new(HashSet::from([initial_session.clone()])));
        let paused = Arc::new(Mutex::new(HashSet::new()));
        let last_denial = Arc::new(Mutex::new(None));
        let worker_cdp = cdp.clone();
        let worker_sessions = Arc::clone(&sessions);
        let worker_paused = Arc::clone(&paused);
        let worker_denial = Arc::clone(&last_denial);
        let worker = tokio::spawn(async move {
            loop {
                let event = match events.recv().await {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        *worker_denial.lock().await = Some(PolicyError::Denied {
                            operation: "navigation".to_string(),
                            reason: format!(
                                "policy event stream lagged by {count}; paused requests remain blocked"
                            ),
                        });
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                if event.method == "Target.attachedToTarget" {
                    if let Some(session_id) = event.params["sessionId"].as_str() {
                        let session_id = session_id.to_string();
                        if enable_fetch_for(&worker_cdp, &session_id).await.is_ok() {
                            worker_sessions.lock().await.insert(session_id.clone());
                            let _ = worker_cdp
                                .send_to_session(
                                    &session_id,
                                    "Runtime.runIfWaitingForDebugger",
                                    None,
                                )
                                .await;
                        }
                    }
                    continue;
                }
                if event.method != "Fetch.requestPaused" {
                    continue;
                }
                let Some(request_id) = event.params["requestId"].as_str() else {
                    continue;
                };
                let request_id = request_id.to_string();
                let key = (event.session_id.clone(), request_id.clone());
                worker_paused.lock().await.insert(key.clone());
                let url = event.params["request"]["url"].as_str().unwrap_or_default();
                let decision = policy.require_url(url).await;
                let (method, params) = match decision {
                    Ok(_) => (
                        "Fetch.continueRequest",
                        serde_json::json!({"requestId": &request_id}),
                    ),
                    Err(error) => {
                        *worker_denial.lock().await = Some(error);
                        (
                            "Fetch.failRequest",
                            serde_json::json!({
                                "requestId": &request_id,
                                "errorReason": "BlockedByClient"
                            }),
                        )
                    }
                };
                let _ = match event.session_id.as_deref() {
                    Some(session_id) => {
                        worker_cdp
                            .send_to_session(session_id, method, Some(params))
                            .await
                    }
                    None => worker_cdp.send(method, Some(params)).await,
                };
                worker_paused.lock().await.remove(&key);
            }
        });
        if let Err(error) = enable_fetch_for(&cdp, &initial_session).await {
            worker.abort();
            return Err(error);
        }
        Ok(Self {
            cdp,
            sessions,
            paused,
            last_denial,
            worker,
        })
    }

    async fn take_denial(&self) -> Option<PolicyError> {
        self.last_denial.lock().await.take()
    }

    async fn shutdown(self) {
        for (session_id, request_id) in self.paused.lock().await.clone() {
            let params = Some(serde_json::json!({
                "requestId": request_id,
                "errorReason": "Aborted"
            }));
            let _ = match session_id.as_deref() {
                Some(session_id) => {
                    self.cdp
                        .send_to_session(session_id, "Fetch.failRequest", params)
                        .await
                }
                None => self.cdp.send("Fetch.failRequest", params).await,
            };
        }
        for session_id in self.sessions.lock().await.clone() {
            let _ = disable_fetch_for(&self.cdp, Some(&session_id)).await;
        }
        self.worker.abort();
    }
}

/// A unique user-data directory owned by an incognito Glass session.
///
/// Chrome still receives `--incognito`; the fresh directory also prevents it
/// from inheriting a user's default browser profile or leaving state behind
/// after a normal Glass shutdown.
#[derive(Debug)]
struct DisposableProfileDir {
    path: PathBuf,
}

const DISPOSABLE_OWNER_FILE: &str = ".glass-owner.json";
const DISPOSABLE_CLEANUP_BATCH: usize = 1024;

#[derive(Debug, Serialize, Deserialize)]
struct DisposableProfileOwner {
    pid: u32,
    process_start: u64,
}

impl DisposableProfileDir {
    fn create() -> BrowserResult<Self> {
        static NEXT_DISPOSABLE_PROFILE: AtomicU64 = AtomicU64::new(0);

        let root = std::env::temp_dir().join("glass");
        std::fs::create_dir_all(&root)?;
        Self::cleanup_abandoned(&root)?;
        let pid = std::process::id();
        let process_start = process_start_identity(pid)
            .ok_or("could not determine Glass process start identity")?;
        for _ in 0..32 {
            let sequence = NEXT_DISPOSABLE_PROFILE.fetch_add(1, Ordering::Relaxed);
            let nonce = format!(
                "{}-{}-{sequence}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            );
            let path = root.join(format!("incognito-{nonce}"));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let owner = DisposableProfileOwner { pid, process_start };
                    let owner_json = serde_json::to_vec(&owner)?;
                    if let Err(error) = std::fs::write(path.join(DISPOSABLE_OWNER_FILE), owner_json)
                    {
                        let _ = std::fs::remove_dir_all(&path);
                        return Err(error.into());
                    }
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("could not allocate a unique incognito user-data directory".into())
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn cleanup_abandoned(root: &Path) -> BrowserResult<()> {
        let mut candidates = Vec::new();
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("incognito-")
            {
                continue;
            }
            let bytes = match std::fs::read(entry.path().join(DISPOSABLE_OWNER_FILE)) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let owner = match serde_json::from_slice::<DisposableProfileOwner>(&bytes) {
                Ok(owner) if owner.pid != 0 && owner.process_start != 0 => owner,
                _ => continue,
            };
            candidates.push((entry.path(), owner));
            if candidates.len() == DISPOSABLE_CLEANUP_BATCH {
                reap_disposable_candidates(&mut candidates)?;
            }
        }
        reap_disposable_candidates(&mut candidates)
    }
}

fn reap_disposable_candidates(
    candidates: &mut Vec<(PathBuf, DisposableProfileOwner)>,
) -> BrowserResult<()> {
    if candidates.is_empty() {
        return Ok(());
    }
    let pids = candidates
        .iter()
        .map(|(_, owner)| Pid::from_u32(owner.pid))
        .collect::<Vec<_>>();
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
    for (path, owner) in candidates.drain(..) {
        let live_start = system
            .process(Pid::from_u32(owner.pid))
            .map(|process| process.start_time());
        if live_start != Some(owner.process_start)
            && let Err(error) = std::fs::remove_dir_all(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(error.into());
        }
    }
    Ok(())
}

fn process_start_identity(pid: u32) -> Option<u64> {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).map(|process| process.start_time())
}

impl Drop for DisposableProfileDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.path.display(), %error, "could not remove disposable incognito profile");
        }
    }
}

impl BrowserSession {
    /// PID of Chrome launched by this session, absent for attached sessions.
    pub fn owned_chrome_pid(&self) -> Option<u32> {
        self.chrome.as_ref().map(|chrome| chrome.pid)
    }

    /// Number of CDP commands issued by this session's page connection.
    pub fn cdp_request_count(&self) -> u64 {
        self.cdp.request_count()
    }

    pub async fn start(options: &SessionOptions) -> BrowserResult<Self> {
        let policy = BrowserPolicy::development(std::env::current_dir()?)?;
        Self::start_with_policy(options, policy).await
    }

    pub async fn start_with_policy(
        options: &SessionOptions,
        mut policy: BrowserPolicy,
    ) -> BrowserResult<Self> {
        options.validate()?;
        if options.attach {
            policy.require(PolicyCapability::Attach)?;
        }
        if !options.attach && !options.incognito {
            policy.require(PolicyCapability::PersistentProfile)?;
        }
        let resolver_rules = policy.prepare_hardened_session(options.attach).await?;
        let profile_manager = ProfileManager::new();
        let mut disposable_profile = None;
        let mut chrome = None;

        // Hold an OS-backed lock until the launched child has been verified
        // and its CDP connection is established. A second Glass process that
        // starts at the same time will re-check the port after this session
        // owns it instead of accepting our endpoint as its own.
        let _launch_lock = if options.attach {
            None
        } else {
            Some(PortLaunchLock::acquire(options.port).await?)
        };

        if options.attach {
            if !check_chrome_health(options.port).await {
                return Err(format!(
                    "cannot attach: no healthy Chrome CDP endpoint is listening on port {}; start Chrome with remote debugging or choose another --port",
                    options.port
                )
                .into());
            }
        } else {
            if is_port_occupied(options.port).await {
                return Err(format!(
                    "CDP port {} is already occupied; use --attach to connect to that Chrome endpoint or choose another --port",
                    options.port
                )
                .into());
            }

            let chrome_path = resolve_chrome_path(options.chrome_path.clone())
                .ok_or("Chrome/Chromium not found; run install-chromium or pass --chrome-path")?;
            let profile_dir = if options.incognito {
                let directory = DisposableProfileDir::create()?;
                let path = directory.path().to_path_buf();
                disposable_profile = Some(directory);
                path
            } else {
                profile_manager.ensure_profile_dir(&options.profile)?
            };
            chrome = Some(
                launch_chrome_with_options(
                    &chrome_path,
                    options.port,
                    Some(&profile_dir),
                    options.headed,
                    options.incognito,
                    resolver_rules.as_deref(),
                )
                .await?,
            );
        }

        let ws_url = match if options.attach {
            get_ws_url(options.port, options.target_id.as_deref()).await
        } else {
            wait_for_ws_url(options.port, options.target_id.as_deref()).await
        } {
            Ok(url) => url,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };
        let target_id = ws_url
            .rsplit('/')
            .next()
            .filter(|id| !id.is_empty())
            .ok_or("page WebSocket URL contained no target ID")?
            .to_string();
        let browser_ws_url = get_browser_ws_url(options.port).await?;
        let cdp = match CdpClient::connect(&browser_ws_url).await {
            Ok(cdp) => cdp,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };

        cdp.send_browser(
            "Target.setDiscoverTargets",
            Some(serde_json::json!({"discover": true})),
        )
        .await?;
        let attached = cdp
            .send_browser(
                "Target.attachToTarget",
                Some(serde_json::json!({"targetId": target_id, "flatten": true})),
            )
            .await?;
        let session_id = attached["sessionId"]
            .as_str()
            .ok_or("Target.attachToTarget returned no sessionId")?
            .to_string();
        cdp.set_active_target_route(
            Some(target_id.clone()),
            Some(session_id.clone()),
            None,
            None,
        );

        let setup = cdp.enable_observation_events().await;
        if let Err(error) = setup {
            cdp.close().await;
            if let Some(process) = chrome.as_mut() {
                let _ = process.shutdown().await;
            }
            return Err(Box::new(error));
        }
        let policy_interception = if policy.preset() == PolicyPreset::Hardened {
            Some(PolicyInterception::start(cdp.clone(), policy.clone(), session_id.clone()).await?)
        } else {
            None
        };

        let page_revision = Arc::new(AtomicU64::new(1));
        let mut events = cdp.subscribe_events();
        let revision_for_events = Arc::clone(&page_revision);
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if context_event_invalidates_observation(&event.method) {
                    revision_for_events.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        let topology = Arc::new(Mutex::new(TopologyRegistry {
            active_target_id: Some(target_id.clone()),
            active_target_session_id: Some(session_id.clone()),
            active_session_id: Some(session_id.clone()),
            ..TopologyRegistry::default()
        }));
        let mut topology_events = cdp.subscribe_events_with_params();
        let topology_for_events = Arc::clone(&topology);
        let cdp_for_events = cdp.clone();
        tokio::spawn(async move {
            loop {
                match topology_events.recv().await {
                    Ok(event) => {
                        let mut topology = topology_for_events.lock().await;
                        let selected_frame = topology.active_frame_id.clone();
                        let selected_session = topology.active_session_id.clone();
                        let selected_context_invalidated = event.method == "Page.frameNavigated"
                            && event.params["frame"]["id"].as_str() == selected_frame.as_deref();
                        if apply_topology_event(&mut topology, &event) {
                            cdp_for_events.set_active_target_route(None, None, None, None);
                        } else if selected_frame.is_some() && topology.active_frame_id.is_none() {
                            cdp_for_events.set_active_route(
                                topology.active_session_id.clone(),
                                None,
                                None,
                            );
                        } else if selected_session != topology.active_session_id
                            || selected_context_invalidated
                        {
                            cdp_for_events.set_active_route(
                                topology.active_session_id.clone(),
                                topology.active_frame_id.clone(),
                                None,
                            );
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        topology_for_events.lock().await.event_loss_count += 1;
                        let _ = resync_topology(&cdp_for_events, &topology_for_events).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        cdp.send_browser(
            "Target.setAutoAttach",
            Some(serde_json::json!({
                "autoAttach": true,
                "waitForDebuggerOnStart": policy.preset() == PolicyPreset::Hardened,
                "flatten": true
            })),
        )
        .await?;

        let session = Self {
            cdp,
            chrome,
            disposable_profile,
            profile: options.profile.clone(),
            interaction_mode: options.interaction_mode,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision,
            observation_cache: Mutex::new(None),
            network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
            diagnostic_leases: Arc::new(Mutex::new(DiagnosticLeaseState::default())),
            download_scope: Arc::new(Mutex::new(())),
            topology,
            popup_click_scope: Mutex::new(()),
            upload_root: std::fs::canonicalize(std::env::current_dir()?)?,
            policy,
            policy_interception,
        };
        let initialize_frame = async {
            let frame_id = match options.frame_id.as_deref() {
                Some(frame_id) => frame_id.to_string(),
                None => {
                    session
                        .list_frames()
                        .await?
                        .into_iter()
                        .next()
                        .ok_or("active target returned no main frame")?
                        .id
                }
            };
            session.select_frame(&frame_id).await?;
            Ok::<(), Box<dyn Error>>(())
        }
        .await;
        if let Err(error) = initialize_frame {
            let _ = session.close().await;
            return Err(error);
        }
        Ok(session)
    }

    /// Explicit privileged escape hatch for benchmark and protocol diagnostics.
    /// Hardened sessions deny it unless `raw-cdp` is deliberately allowed.
    pub fn raw_cdp(&self) -> BrowserResult<&CdpClient> {
        self.policy.require(PolicyCapability::RawCdp)?;
        Ok(&self.cdp)
    }

    pub fn profile_name(&self) -> &str {
        &self.profile
    }

    pub fn policy(&self) -> &BrowserPolicy {
        &self.policy
    }

    pub async fn list_targets(&self) -> BrowserResult<Vec<PageTargetInfo>> {
        let raw = self.cdp.send_browser("Target.getTargets", None).await?;
        let active = self.topology.lock().await.active_target_id.clone();
        let mut targets = Vec::new();
        for info in raw["targetInfos"].as_array().into_iter().flatten() {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let Some(id) = info["targetId"].as_str() else {
                continue;
            };
            validate_topology_id(id)?;
            if targets.len() == TOPOLOGY_MAX_TARGETS {
                return Err("page target limit exceeded".into());
            }
            targets.push(PageTargetInfo {
                id: id.to_string(),
                url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
                title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
                opener_id: retained_optional_topology_id(info["openerId"].as_str())?,
                active: active.as_deref() == Some(id),
            });
        }
        self.topology.lock().await.targets = targets.clone();
        Ok(targets)
    }

    pub async fn topology_events(&self) -> Vec<TopologyEventSummary> {
        self.topology.lock().await.events.iter().cloned().collect()
    }

    async fn route_identity(&self) -> BrowserResult<(String, String)> {
        self.cdp
            .operation_identity()
            .ok_or_else(|| "operation has no target/frame identity".into())
    }

    async fn ensured_route_identity(&self) -> BrowserResult<(String, String)> {
        if let Ok(route) = self.route_identity().await {
            return Ok(route);
        }
        let main_frame = self
            .list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.parent_id.is_none())
            .ok_or("active target returned no main frame")?;
        self.select_frame(&main_frame.id).await?;
        self.route_identity().await
    }

    pub async fn create_target(&self, url: &str) -> BrowserResult<PageTargetInfo> {
        let url = normalize_url(url);
        self.policy.require_url(&url).await?;
        let result = self
            .cdp
            .send_browser("Target.createTarget", Some(serde_json::json!({"url": url})))
            .await?;
        let id = result["targetId"]
            .as_str()
            .ok_or("Target.createTarget returned no targetId")?;
        validate_topology_id(id)?;
        let targets = self.list_targets().await?;
        targets
            .into_iter()
            .find(|target| target.id == id)
            .ok_or_else(|| "created target was not discoverable".into())
    }

    pub async fn select_target(&self, target_id: &str) -> BrowserResult<PageTargetInfo> {
        validate_topology_id(target_id)?;
        let target = self
            .list_targets()
            .await?
            .into_iter()
            .find(|target| target.id == target_id)
            .ok_or("page target was not found")?;
        let attached = self
            .cdp
            .send_browser(
                "Target.attachToTarget",
                Some(serde_json::json!({"targetId": target_id, "flatten": true})),
            )
            .await?;
        let new_session = attached["sessionId"]
            .as_str()
            .ok_or("Target.attachToTarget returned no sessionId")?
            .to_string();
        let old_session = self.topology.lock().await.active_session_id.clone();
        if let Err(error) = self.cdp.enable_observation_events_for(&new_session).await {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": new_session})),
                )
                .await;
            return Err(error.into());
        }
        if let Some(interception) = &self.policy_interception {
            if let Err(error) = enable_fetch_for(&self.cdp, &new_session).await {
                let _ = self
                    .cdp
                    .send_browser(
                        "Target.detachFromTarget",
                        Some(serde_json::json!({"sessionId": new_session})),
                    )
                    .await;
                return Err(error);
            }
            interception
                .sessions
                .lock()
                .await
                .insert(new_session.clone());
        }
        if let Err(error) = self
            .cdp
            .send_to_session(
                &new_session,
                "Target.setAutoAttach",
                Some(serde_json::json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": self.policy.preset() == PolicyPreset::Hardened,
                    "flatten": true
                })),
            )
            .await
        {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": new_session})),
                )
                .await;
            return Err(error.into());
        }
        let prepared = async {
            let raw_frames = self
                .cdp
                .send_to_session(&new_session, "Page.getFrameTree", None)
                .await?;
            let mut frames = Vec::new();
            collect_frames(&raw_frames["frameTree"], None, None, &mut frames)?;
            let main_frame = frames
                .iter()
                .find(|frame| frame.parent_id.is_none())
                .ok_or("selected target returned no main frame")?
                .id
                .clone();
            Ok::<_, Box<dyn Error>>((frames, main_frame))
        }
        .await;
        let (mut frames, main_frame) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = self
                    .cdp
                    .send_browser(
                        "Target.detachFromTarget",
                        Some(serde_json::json!({"sessionId": new_session})),
                    )
                    .await;
                return Err(error);
            }
        };
        for frame in &mut frames {
            frame.active = frame.id == main_frame;
        }
        {
            let mut topology = self.topology.lock().await;
            topology.active_target_id = Some(target_id.to_string());
            topology.active_session_id = Some(new_session.clone());
            topology.active_target_session_id = Some(new_session.clone());
            topology.active_frame_id = Some(main_frame.clone());
            topology.frames = frames;
        }
        self.cdp.set_active_target_route(
            Some(target_id.to_string()),
            Some(new_session.clone()),
            Some(main_frame),
            None,
        );
        if let Some(old_session) = old_session {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": old_session})),
                )
                .await;
        }
        self.invalidate_observation();
        Ok(PageTargetInfo {
            active: true,
            ..target
        })
    }

    pub async fn close_target(&self, target_id: &str) -> BrowserResult<()> {
        validate_topology_id(target_id)?;
        let result = self
            .cdp
            .send_browser(
                "Target.closeTarget",
                Some(serde_json::json!({"targetId": target_id})),
            )
            .await?;
        if result["success"].as_bool() != Some(true) {
            return Err("Chrome refused to close target".into());
        }
        let mut topology = self.topology.lock().await;
        if topology.active_target_id.as_deref() == Some(target_id) {
            topology.active_target_id = None;
            topology.active_session_id = None;
            topology.active_target_session_id = None;
            topology.active_frame_id = None;
            topology.frames.clear();
            topology.frame_sessions.clear();
            topology.frame_parents.clear();
            self.cdp.set_active_target_route(None, None, None, None);
        }
        topology.targets.retain(|target| target.id != target_id);
        Ok(())
    }

    pub async fn list_frames(&self) -> BrowserResult<Vec<FrameInfo>> {
        let (target_id, target_session, active) = {
            let topology = self.topology.lock().await;
            (
                topology.active_target_id.clone(),
                topology.active_target_session_id.clone(),
                topology.active_frame_id.clone(),
            )
        };
        if target_id.is_none() {
            return Err("no active target is selected".into());
        }
        let target_session = target_session.ok_or("active target has no CDP session")?;
        let raw = self
            .cdp
            .send_to_session(&target_session, "Page.getFrameTree", None)
            .await?;
        let mut frames = Vec::new();
        collect_frames(&raw["frameTree"], None, active.as_deref(), &mut frames)?;
        let (attached_sessions, frame_parents) = {
            let topology = self.topology.lock().await;
            (
                topology.frame_sessions.clone(),
                topology.frame_parents.clone(),
            )
        };
        let mut discovered_frame_sessions = Vec::new();
        let mut stale_frame_sessions = HashSet::new();
        let mut queried_sessions = HashSet::new();
        for (attached_frame_id, session_id) in &attached_sessions {
            if !queried_sessions.insert(session_id.clone()) {
                continue;
            }
            let oopif_tree = match self
                .cdp
                .send_to_session(session_id, "Page.getFrameTree", None)
                .await
            {
                Ok(tree) => tree,
                Err(error) if error.code == -32_001 => {
                    stale_frame_sessions.insert(session_id.clone());
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let start = frames.len();
            collect_frames(
                &oopif_tree["frameTree"],
                frame_parents.get(attached_frame_id).map(String::as_str),
                active.as_deref(),
                &mut frames,
            )?;
            for frame in &frames[start..] {
                discovered_frame_sessions.push((frame.id.clone(), session_id.clone()));
            }
            for frame in &mut frames[start..] {
                frame.out_of_process = true;
            }
        }
        let mut topology = self.topology.lock().await;
        topology
            .frame_sessions
            .retain(|_, session| !stale_frame_sessions.contains(session));
        for (frame_id, session_id) in discovered_frame_sessions {
            topology.frame_sessions.insert(frame_id, session_id);
        }
        topology.frames = frames.clone();
        Ok(frames)
    }

    pub async fn select_frame(&self, frame_id: &str) -> BrowserResult<FrameInfo> {
        validate_topology_id(frame_id)?;
        let frame = self
            .list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.id == frame_id)
            .ok_or("frame was not found")?;
        let session_id = {
            let topology = self.topology.lock().await;
            topology
                .frame_sessions
                .get(frame_id)
                .cloned()
                .or_else(|| topology.active_target_session_id.clone())
                .ok_or("active target has no CDP session")?
        };
        let context_id = if frame.parent_id.is_none() {
            None
        } else {
            let world = self
                .cdp
                .send_to_session(
                    &session_id,
                    "Page.createIsolatedWorld",
                    Some(serde_json::json!({"frameId": frame_id, "worldName":"glass"})),
                )
                .await?;
            Some(
                world["executionContextId"]
                    .as_i64()
                    .ok_or("Page.createIsolatedWorld returned no executionContextId")?,
            )
        };
        self.cdp.set_active_route(
            Some(session_id.clone()),
            Some(frame_id.to_string()),
            context_id,
        );
        {
            let mut topology = self.topology.lock().await;
            topology.active_frame_id = Some(frame_id.to_string());
            topology.active_session_id = Some(session_id);
        }
        self.invalidate_observation();
        Ok(FrameInfo {
            active: true,
            ..frame
        })
    }

    /// Whether the Chrome process was explicitly attached rather than launched
    /// by this session.
    pub fn is_attached(&self) -> bool {
        self.chrome.is_none()
    }

    /// Whether this session owns the Chrome process and will stop it on close.
    pub fn owns_chrome(&self) -> bool {
        self.chrome.is_some()
    }

    pub async fn close(mut self) -> BrowserResult<()> {
        if self.chrome.is_some() {
            // `Browser.close` lets Chrome commit profile-backed storage before
            // the owned child process falls back to termination below. A page
            // target can close its websocket before replying, so this is best
            // effort and intentionally bounded.
            let _ =
                tokio::time::timeout(OWNED_BROWSER_CLOSE_TIMEOUT, self.cdp.close_browser()).await;
        }
        if let Some(interception) = self.policy_interception.take() {
            interception.shutdown().await;
        }
        self.cdp.close().await;
        let shutdown_result = if let Some(process) = self.chrome.as_mut() {
            process.shutdown().await
        } else {
            Ok(())
        };
        self.chrome = None;
        // Drop after the owned child has stopped so Chrome no longer holds
        // files in the disposable user-data directory.
        drop(self.disposable_profile.take());
        shutdown_result
    }

    pub async fn page_info(&self) -> BrowserResult<PageInfo> {
        self.cdp.with_current_route(async {
        let raw = self
            .cdp
            .evaluate(
                "JSON.stringify({url: location.href, title: document.title, ready_state: document.readyState})",
            )
            .await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("document state evaluation returned a non-string value")?;
                let mut page: PageInfo = serde_json::from_str(json)?;
                (page.target_id, page.frame_id) = self.route_identity().await?;
                Ok(page)
        }).await
    }

    pub async fn navigate(&self, url: &str) -> BrowserResult<PageInfo> {
        self.navigate_with_deadline(url, Duration::from_secs(20))
            .await
    }

    pub async fn navigate_with_deadline(
        &self,
        url: &str,
        deadline: Duration,
    ) -> BrowserResult<PageInfo> {
        self.cdp
            .with_current_target_route(async {
                validate_wait_deadline(deadline)?;
                let url = normalize_url(url);
                self.policy.require_url(&url).await?;
                if let Some(interception) = &self.policy_interception
                    && let Some(error) = interception.take_denial().await
                {
                    return Err(error.into());
                }
                let result = async {
                    let mut events = self.cdp.subscribe_events();
                    let started = tokio::time::Instant::now();
                    let navigation = tokio::time::timeout(deadline, self.cdp.navigate(&url))
                        .await
                        .map_err(|_| {
                            wait_timeout("lifecycle", deadline, "navigate_command_pending")
                        })??;
                    if let Some(frame_id) = navigation["frameId"].as_str() {
                        validate_topology_id(frame_id)?;
                        self.topology.lock().await.active_frame_id = Some(frame_id.to_string());
                        self.cdp
                            .set_active_frame_context(Some(frame_id.to_string()), None);
                    }
                    let remaining = deadline.saturating_sub(started.elapsed());
                    self.wait_loop(
                        WaitCondition::Lifecycle("complete".to_string()),
                        remaining,
                        deadline,
                        &mut events,
                        true,
                    )
                    .await?;
                    let remaining = deadline.saturating_sub(started.elapsed());
                    let main_frame = self
                        .list_frames()
                        .await?
                        .into_iter()
                        .find(|frame| frame.parent_id.is_none())
                        .ok_or("navigated target returned no main frame")?;
                    self.select_frame(&main_frame.id).await?;
                    let page = tokio::time::timeout(remaining, self.page_info())
                        .await
                        .map_err(|_| wait_timeout("lifecycle", deadline, "page_info_pending"))??;
                    self.invalidate_observation();
                    Ok(page)
                }
                .await;
                if let Some(error) = match &self.policy_interception {
                    Some(interception) => interception.take_denial().await,
                    None => None,
                } {
                    return Err(error.into());
                }
                result
            })
            .await
    }

    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        self.policy.require(PolicyCapability::Evaluate)?;
        self.cdp
            .with_current_route(async {
                let result = self.evaluate_value(expression).await;
                // Arbitrary JavaScript may mutate DOM, styles, form state, or history.
                // Invalidate synchronously so the next cached observation cannot race
                // the asynchronous CDP mutation event stream.
                self.invalidate_observation();
                result
            })
            .await
    }

    pub async fn text(&self) -> BrowserResult<String> {
        self.cdp
            .with_current_route(async {
                let value = self
                    .evaluate_value("document.body ? document.body.innerText : ''")
                    .await?;
                Ok(truncate_visible_text(
                    value.as_str().unwrap_or_default(),
                    COMPACT_TEXT_MAX_BYTES,
                ))
            })
            .await
    }

    /// Fetch the full DOM only for an explicit deep-inspection operation.
    pub async fn deep_dom(&self) -> BrowserResult<DomNode> {
        self.cdp
            .with_current_route(async {
                let raw = self.cdp.get_deep_document().await?;
                parse_dom_tree(&raw).ok_or_else(|| {
                    "CDP deep DOM response contained no parseable root node"
                        .to_string()
                        .into()
                })
            })
            .await
    }

    /// Collect compact page context without a deep DOM or screenshot.
    pub async fn observe(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, true).await
    }

    /// Collect compact context and explicitly include the full DOM tree.
    pub async fn observe_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, true).await
    }

    /// Collect structured context and explicitly include a current screenshot.
    pub async fn observe_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, true).await
    }

    /// Collect context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, true).await
    }

    /// Collect fresh compact context, bypassing the compact-context cache.
    pub async fn observe_fresh(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false).await
    }

    /// Collect fresh context and explicitly include the full DOM tree.
    pub async fn observe_fresh_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, false).await
    }

    /// Collect fresh structured context and explicitly include a screenshot.
    pub async fn observe_fresh_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, false).await
    }

    /// Collect fresh context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_fresh_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, false).await
    }

    async fn observe_internal(
        &self,
        include_dom: bool,
        include_screenshot: bool,
        use_cache: bool,
    ) -> BrowserResult<PageContext> {
        if let Some(interception) = &self.policy_interception
            && let Some(error) = interception.take_denial().await
        {
            return Err(error.into());
        }
        self.cdp
            .with_current_route(async {
                let mut context = self
                    .compact_observation(use_cache)
                    .await?
                    .into_page_context();
                if include_dom {
                    context.dom = Some(self.deep_dom().await?);
                }
                if include_screenshot {
                    context.screenshot = Some(self.screenshot_base64().await?);
                }
                Ok(context)
            })
            .await
    }

    async fn compact_observation(&self, use_cache: bool) -> BrowserResult<CompactPageContext> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        if use_cache {
            let cached_context = {
                let cache = self.observation_cache.lock().await;
                cache
                    .as_ref()
                    .filter(|cached| cached.revision == revision)
                    .map(|cached| cached.context.clone())
            };
            if let Some(context) = cached_context {
                return Ok(context);
            }
        }

        let (target_id, frame_id) = self.route_identity().await?;
        let world = self
            .cdp
            .send(
                "Page.createIsolatedWorld",
                Some(serde_json::json!({"frameId": frame_id, "worldName": "glass-observation"})),
            )
            .await?;
        let context_id = world["executionContextId"]
            .as_i64()
            .ok_or("Page.createIsolatedWorld returned no executionContextId")?;
        let mut collected = None;
        for attempt in 1..=COMPACT_OBSERVATION_MAX_ATTEMPTS {
            let start_revision = self.page_revision.load(Ordering::Relaxed);
            let attempt_result = tokio::time::timeout(COMPACT_OBSERVATION_ATTEMPT_TIMEOUT, async {
                let start = self.compact_page_state(context_id).await?;
                let accessibility = self.cdp.get_accessibility_tree().await?;
                let end = self.compact_page_state(context_id).await?;
                BrowserResult::Ok((start, accessibility, end))
            })
            .await
            .map_err(|_| "compact observation attempt exceeded its one-second deadline")??;
            let end_revision = self.page_revision.load(Ordering::Relaxed);
            let consistent = start_revision == end_revision
                && attempt_result.0.mutation_revision == attempt_result.2.mutation_revision;
            collected = Some((
                attempt,
                consistent,
                start_revision,
                end_revision,
                attempt_result,
            ));
            if consistent {
                break;
            }
        }
        let (
            attempts,
            consistent,
            start_revision,
            end_revision,
            (start_state, accessibility_raw, page_state),
        ) = collected.expect("observation always performs at least one attempt");
        let page = PageInfo {
            url: page_state.url,
            title: page_state.title,
            ready_state: page_state.ready_state,
            target_id,
            frame_id,
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let compact_accessibility = project_compact_accessibility(&full_roots, end_revision);
        let (mut text, locally_truncated) =
            truncate_visible_text_with_status(&page_state.text, COMPACT_TEXT_MAX_BYTES);
        let text_truncated = locally_truncated || page_state.boundaries.text_truncated;
        if page_state.boundaries.text_truncated && !text.ends_with(TEXT_TRUNCATION_MARKER) {
            let content_limit = COMPACT_TEXT_MAX_BYTES.saturating_sub(TEXT_TRUNCATION_MARKER.len());
            while text.len() > content_limit {
                text.pop();
            }
            text.push_str(TEXT_TRUNCATION_MARKER);
        }
        let mut incomplete = Vec::new();
        if text_truncated {
            incomplete.push(ObservationIncompleteReason::VisibleText);
        }
        if compact_accessibility.nodes_truncated {
            incomplete.push(ObservationIncompleteReason::AccessibilityNode);
        }
        if compact_accessibility.labels_truncated {
            incomplete.push(ObservationIncompleteReason::AccessibilityLabel);
        }
        if compact_accessibility.controls_truncated {
            incomplete.push(ObservationIncompleteReason::Control);
        }
        if page_state.boundaries.shadow_roots > 0 {
            incomplete.push(ObservationIncompleteReason::ShadowBoundary);
        }
        if page_state.boundaries.child_frames > 0 {
            incomplete.push(ObservationIncompleteReason::FrameBoundary);
        }
        if page_state.boundaries.canvases > 0 {
            incomplete.push(ObservationIncompleteReason::Canvas);
        }
        if page_state.boundaries.truncated {
            incomplete.push(ObservationIncompleteReason::BoundaryScan);
        }
        if !consistent {
            incomplete.push(ObservationIncompleteReason::MutationRace);
        }
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            revision: end_revision,
            roots: compact_accessibility.roots,
            interactive: compact_accessibility.interactive,
            truncated: compact_accessibility.truncated,
        };
        let context = CompactPageContext {
            page,
            text,
            accessibility,
            consistency: ObservationConsistency {
                consistent,
                attempts,
                start_revision,
                end_revision,
                start_mutation_revision: start_state.mutation_revision,
                end_mutation_revision: page_state.mutation_revision,
            },
            boundaries: page_state.boundaries,
            incomplete,
        };
        if consistent && self.page_revision.load(Ordering::Relaxed) == end_revision {
            *self.observation_cache.lock().await = Some(CachedObservation {
                revision: end_revision,
                context: context.clone(),
            });
        }
        Ok(context)
    }

    async fn compact_page_state(&self, context_id: i64) -> BrowserResult<EvaluatedPageState> {
        let raw = self
            .cdp
            .evaluate_in_context(COMPACT_PAGE_STATE_EXPRESSION, Some(context_id))
            .await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("compact page-state evaluation returned a non-string value")?;
        Ok(serde_json::from_str(json)?)
    }

    pub async fn screenshot_png(&self) -> BrowserResult<Vec<u8>> {
        let data = self.screenshot_base64().await?;
        Ok(STANDARD.decode(data.as_bytes())?)
    }

    /// Capture a PNG while preserving CDP's base64 payload for image APIs.
    pub async fn screenshot_base64(&self) -> BrowserResult<String> {
        self.policy.require(PolicyCapability::Screenshot)?;
        self.cdp
            .with_current_route(async { Ok(self.cdp.screenshot("png").await?) })
            .await
    }

    /// Capture exact opt-in visual evidence with explicit effective metadata.
    pub async fn capture_visual(
        &self,
        options: &VisualCaptureOptions,
    ) -> BrowserResult<VisualCapture> {
        self.policy.require(PolicyCapability::Screenshot)?;
        validate_visual_options(options)?;
        self.cdp
            .with_current_route(async {
                let metrics = self.cdp.get_layout_metrics().await?;
                let dpr = runtime_value(&self.cdp.evaluate("devicePixelRatio").await?)?
                    .as_f64()
                    .unwrap_or(1.0);
                let (_, selected_frame_id) = self.route_identity().await?;
                let selected_child_frame = {
                    let topology = self.topology.lock().await;
                    topology
                        .frames
                        .iter()
                        .find(|frame| frame.id == selected_frame_id)
                        .is_some_and(|frame| frame.parent_id.is_some())
                };
                if selected_child_frame {
                    return Err("exact visual capture of a selected child frame is not supported; select its page target or the main frame".into());
                }
                let mut clip = options.clip;
                if options.full_page {
                    clip = Some(visual_rect(&metrics["cssContentSize"])?);
                } else if let Some(target) = options.target.as_deref() {
                    let element = self.resolve_element(target).await?;
                    let model = match (element.node_id, element.backend_dom_node_id) {
                        (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
                        (_, Some(backend_id)) => {
                            self.cdp.get_box_model_for_backend(backend_id).await?
                        }
                        _ => return Err("visual target has no DOM node identity".into()),
                    };
                    let mut element_clip = visual_quad_rect(&model["model"]["border"])?;
                    let viewport = visual_viewport_rect(&metrics["cssVisualViewport"])?;
                    element_clip.x += viewport.x;
                    element_clip.y += viewport.y;
                    clip = Some(element_clip);
                } else if clip.is_none() && options.scale != 1.0 {
                    clip = Some(visual_viewport_rect(&metrics["cssVisualViewport"])?);
                }
                let viewport = visual_viewport_rect(&metrics["cssVisualViewport"])?;
                validate_effective_visual_clip(
                    Some(clip.unwrap_or(viewport)),
                    if clip.is_some() { options.scale } else { dpr },
                )?;
                let mut params = serde_json::json!({
                    "format": options.format.as_cdp(),
                    "optimizeForSpeed": true,
                    "captureBeyondViewport": options.full_page || clip.is_some(),
                    "fromSurface": true
                });
                if let Some(quality) = options.quality {
                    params["quality"] = Value::from(quality);
                }
                if let Some(clip) = clip {
                    params["clip"] = serde_json::json!({
                        "x": clip.x,
                        "y": clip.y,
                        "width": clip.width,
                        "height": clip.height,
                        "scale": options.scale
                    });
                }
                if options.full_page {
                    let latest = self.cdp.get_layout_metrics().await?;
                    let latest_clip = visual_rect(&latest["cssContentSize"])?;
                    if !visual_clips_match(clip.expect("full-page capture has a clip"), latest_clip) {
                        return Err("full-page geometry changed during capture preparation".into());
                    }
                } else if let Some(target) = options.target.as_deref() {
                    let element = self.resolve_element(target).await?;
                    let model = match (element.node_id, element.backend_dom_node_id) {
                        (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
                        (_, Some(backend_id)) => self.cdp.get_box_model_for_backend(backend_id).await?,
                        _ => return Err("visual target has no DOM node identity".into()),
                    };
                    let latest_metrics = self.cdp.get_layout_metrics().await?;
                    let viewport = visual_viewport_rect(&latest_metrics["cssVisualViewport"])?;
                    let mut latest_clip = visual_quad_rect(&model["model"]["border"])?;
                    latest_clip.x += viewport.x;
                    latest_clip.y += viewport.y;
                    if !visual_clips_match(clip.expect("element capture has a clip"), latest_clip) {
                        return Err("element geometry changed during capture preparation".into());
                    }
                }
                let data = self.cdp.screenshot_with_params(params).await?;
                if data.len() > MAX_VISUAL_BASE64_BYTES {
                    return Err("visual base64 payload exceeded 64 MiB".into());
                }
                let encoded_bytes = decoded_base64_len(&data)?;
                let header_end = data.len().min(VISUAL_HEADER_BASE64_BYTES) / 4 * 4;
                let header = STANDARD.decode(&data.as_bytes()[..header_end])?;
                let size = imagesize::blob_size(&header)?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(VisualCapture {
                    metadata: VisualCaptureMetadata {
                        format: options.format,
                        width: size.width,
                        height: size.height,
                        encoded_bytes,
                        device_scale_factor: dpr,
                        scale: options.scale,
                        full_page: options.full_page,
                        clip,
                        target_id,
                        frame_id,
                    },
                    data,
                })
            })
            .await
    }

    pub async fn start_screencast(
        &self,
        format: VisualFormat,
        quality: u8,
        max_width: u32,
        max_height: u32,
    ) -> BrowserResult<ScreencastScope> {
        self.policy.require(PolicyCapability::Screenshot)?;
        if format == VisualFormat::Webp {
            return Err("CDP screencast supports only png or jpeg".into());
        }
        if quality > 100
            || max_width == 0
            || max_height == 0
            || max_width > 4096
            || max_height > 4096
            || f64::from(max_width) * f64::from(max_height) > MAX_VISUAL_PIXELS
        {
            return Err(
                "screencast quality must be 0..=100 and dimensions must fit the 8 MP budget".into(),
            );
        }
        let session_id = self.cdp.current_session_id();
        let receiver = self.cdp.open_screencast_channel(session_id.clone())?;
        let mut startup = ScreencastStartupGuard {
            cdp: self.cdp.clone(),
            session_id: session_id.clone(),
            armed: true,
        };
        let parameters = Some(serde_json::json!({
            "format": format.as_cdp(),
            "quality": quality,
            "maxWidth": max_width,
            "maxHeight": max_height,
            "everyNthFrame": 1
        }));
        let start_result = match session_id.as_deref() {
            Some(session_id) => {
                self.cdp
                    .send_to_session(session_id, "Page.startScreencast", parameters)
                    .await
            }
            None => self.cdp.send("Page.startScreencast", parameters).await,
        };
        if let Err(error) = start_result {
            return Err(error.into());
        }
        startup.disarm();
        Ok(ScreencastScope {
            cdp: self.cdp.clone(),
            session_id,
            receiver,
            armed: true,
        })
    }

    pub async fn scroll(&self, dx: f64, dy: f64) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                self.cdp.scroll_by(dx, dy).await?;
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                Ok(ActionOutcome {
                    action: ActionKind::Scroll,
                    target: None,
                    revision: self.invalidate_observation(),
                    target_id,
                    frame_id,
                    evidence: None,
                })
            })
            .await
    }

    pub async fn snapshot(&self) -> BrowserResult<AccessibilitySnapshot> {
        self.cdp
            .with_current_route(async {
                let revision = self.page_revision.load(Ordering::Relaxed);
                let raw = self.cdp.get_accessibility_tree().await?;
                let roots = parse_accessibility_tree(&raw);
                let interactive = interactive_elements(&roots, revision);
                Ok(AccessibilitySnapshot {
                    page: self.page_info().await?,
                    roots,
                    interactive,
                })
            })
            .await
    }

    pub async fn wait(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        self.cdp
            .with_current_route(async {
                validate_wait_deadline(deadline)?;
                condition.validate()?;
                if let WaitCondition::NetworkQuiet(quiet) = condition {
                    return tokio::time::timeout(
                        deadline,
                        self.wait_for_network_quiet(quiet, deadline),
                    )
                    .await
                    .map_err(|_| {
                        wait_timeout("network_quiet", deadline, "network_check_pending")
                    })?;
                }
                let mut events = self.cdp.subscribe_events();
                self.wait_loop(condition, deadline, deadline, &mut events, false)
                    .await
            })
            .await
    }

    async fn wait_loop(
        &self,
        condition: WaitCondition,
        deadline: Duration,
        reported_deadline: Duration,
        events: &mut tokio::sync::broadcast::Receiver<super::cdp::CdpEvent>,
        require_load_event: bool,
    ) -> BrowserResult<WaitOutcome> {
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut previous_geometry = None;
        let description = condition.description();
        let mut load_event_seen = !require_load_event;
        let mut last_state = "not_checked".to_string();
        loop {
            let now = tokio::time::Instant::now();
            if now >= expires {
                return Err(wait_timeout(&description, reported_deadline, &last_state).into());
            }
            let remaining = expires - now;
            let (matched, state, geometry) = tokio::time::timeout(
                remaining,
                self.check_wait_condition(&condition, previous_geometry.as_deref()),
            )
            .await
            .map_err(|_| wait_timeout(&description, reported_deadline, &last_state))??;
            last_state = bounded_wait_state(&state);
            previous_geometry = geometry;
            if matched && load_event_seen {
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                return Ok(WaitOutcome {
                    condition: description,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state,
                    target_id,
                    frame_id,
                });
            }
            let now = tokio::time::Instant::now();
            let remaining = expires - now;
            tokio::select! {
                _ = tokio::time::sleep(WAIT_POLL_INTERVAL.min(remaining)) => {}
                event = events.recv() => match event {
                    Ok(event) => { load_event_seen |= event.method == "Page.loadEventFired"; }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return Err("CDP event stream closed during wait".into()),
                }
            }
        }
    }

    async fn check_wait_condition(
        &self,
        condition: &WaitCondition,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        match condition {
            WaitCondition::Lifecycle(expected) => {
                let page = self.page_info().await?;
                Ok((page.ready_state == *expected, page.ready_state, None))
            }
            WaitCondition::UrlExact(expected) => {
                let page = self.page_info().await?;
                Ok((page.url == *expected, page.url, None))
            }
            WaitCondition::UrlPrefix(prefix) => {
                let page = self.page_info().await?;
                Ok((page.url.starts_with(prefix), page.url, None))
            }
            WaitCondition::Text(expected) => {
                let expression = visible_text_contains_expression(expected)?;
                let value = self.evaluate_value(&expression).await?;
                let matched = value.as_bool().unwrap_or(false);
                Ok((matched, format!("present={matched}"), None))
            }
            WaitCondition::JavaScript(expression) => {
                let value = self.evaluate_value(expression).await?;
                let matched = value
                    .as_bool()
                    .ok_or("wait JavaScript predicate must return a boolean")?;
                Ok((matched, matched.to_string(), None))
            }
            WaitCondition::TargetAttached(target)
            | WaitCondition::TargetVisible(target)
            | WaitCondition::TargetHidden(target)
            | WaitCondition::TargetEnabled(target)
            | WaitCondition::TargetStable(target) => {
                self.check_target_wait(condition, target, previous_geometry)
                    .await
            }
            WaitCondition::NetworkQuiet(_) => unreachable!("handled by wait"),
        }
    }

    async fn check_target_wait(
        &self,
        condition: &WaitCondition,
        target: &str,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        let element = match self.resolve_element(target).await {
            Ok(element) => element,
            Err(error)
                if error
                    .downcast_ref::<TargetError>()
                    .is_some_and(|error| error.kind == TargetErrorKind::NotFound) =>
            {
                let matched = matches!(condition, WaitCondition::TargetHidden(_));
                return Ok((matched, "detached".to_string(), None));
            }
            Err(error) => return Err(error),
        };
        if matches!(condition, WaitCondition::TargetAttached(_)) {
            return Ok((true, "attached".to_string(), None));
        }
        let object_id = self
            .cdp
            .resolve_node_object(element.node_id, element.backend_dom_node_id)
            .await?;
        let raw = self
            .cdp
            .call_on_object(&object_id, WAIT_TARGET_STATE_FUNCTION)
            .await;
        let _ = self.cdp.release_object(&object_id).await;
        let value = runtime_value(&raw?)?;
        let visible = value["visible"].as_bool().unwrap_or(false);
        let enabled = value["enabled"].as_bool().unwrap_or(false);
        let geometry = value["geometry"].as_str().map(str::to_string);
        let matched = match condition {
            WaitCondition::TargetVisible(_) => visible,
            WaitCondition::TargetHidden(_) => !visible,
            WaitCondition::TargetEnabled(_) => visible && enabled,
            WaitCondition::TargetStable(_) => {
                visible
                    && geometry
                        .as_deref()
                        .is_some_and(|geometry| previous_geometry == Some(geometry))
            }
            _ => unreachable!(),
        };
        Ok((matched, value.to_string(), geometry))
    }

    async fn wait_for_network_quiet(
        &self,
        quiet: Duration,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        if quiet.is_zero() {
            return Err("network quiet duration must be positive".into());
        }
        let mut events = self.cdp.subscribe_events_with_params();
        let mut guard =
            NetworkDomainGuard::acquire(self.cdp.clone(), Arc::clone(&self.network_wait_leases))
                .await?;
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut empty_since = started;
        let mut in_flight = HashSet::new();
        let mut overflowed = false;
        loop {
            let now = tokio::time::Instant::now();
            if in_flight.is_empty() && !overflowed && now.duration_since(empty_since) >= quiet {
                guard.disable().await?;
                let (target_id, frame_id) = self.route_identity().await?;
                return Ok(WaitOutcome {
                    condition: "network_quiet".to_string(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state: "in_flight=0".to_string(),
                    target_id,
                    frame_id,
                });
            }
            if now >= expires {
                return Err(WaitTimeout {
                    condition: "network_quiet".to_string(),
                    deadline_ms: deadline.as_millis() as u64,
                    last_state: if overflowed {
                        "in_flight=overflow".to_string()
                    } else {
                        format!("in_flight={}", in_flight.len())
                    },
                    reason: "deadline_exceeded",
                }
                .into());
            }
            tokio::select! {
                _ = tokio::time::sleep((expires - now).min(WAIT_POLL_INTERVAL)) => {}
                event = events.recv() => match event {
                    Ok(event) => {
                      let request_id = event.params["requestId"].as_str();
                      match event.method.as_str() {
                        "Network.requestWillBeSent" => {
                            if let Some(id) = request_id {
                                if in_flight.len() < NETWORK_IN_FLIGHT_LIMIT {
                                    in_flight.insert(id.to_string());
                                } else {
                                    overflowed = true;
                                }
                            }
                        }
                        "Network.loadingFinished" | "Network.loadingFailed" => {
                            if let Some(id) = request_id { in_flight.remove(id); }
                            if in_flight.is_empty() && !overflowed { empty_since = tokio::time::Instant::now(); }
                        }
                        _ => {}
                      }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => return Err("network wait event stream lagged".into()),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err("network wait event stream closed".into()),
                }
            }
        }
    }

    /// Collect explicitly scoped, bounded, secret-redacted browser evidence.
    pub async fn diagnostics(&self, duration: Duration) -> BrowserResult<DiagnosticReport> {
        if duration.is_zero() || duration > MAX_DIAGNOSTIC_DURATION {
            return Err("diagnostic duration must be between 1 ms and 30 seconds".into());
        }
        self.cdp
            .with_current_route(async {
                let (target_id, frame_id) = self.route_identity().await?;
                let route_session_id = self.cdp.current_session_id();
                let mut events = self.cdp.subscribe_events_with_params();
                let mut guard = DiagnosticDomainGuard::acquire(
                    self.cdp.clone(),
                    Arc::clone(&self.network_wait_leases),
                    Arc::clone(&self.diagnostic_leases),
                )
                .await?;
                let started = tokio::time::Instant::now();
                let deadline = started + duration;
                let mut console = Vec::new();
                let mut network = Vec::new();
                let mut request_indexes = HashMap::new();
                let mut dropped_events = 0_u64;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        event = events.recv() => match event {
                            Ok(event) if event.session_id == route_session_id => collect_diagnostic_event(
                                &event,
                                &mut console,
                                &mut network,
                                &mut request_indexes,
                                &mut dropped_events,
                            ),
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                dropped_events = dropped_events.saturating_add(count);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
                guard.disable().await?;
                Ok(DiagnosticReport {
                    target_id,
                    frame_id,
                    duration_ms: started.elapsed().as_millis() as u64,
                    console,
                    network,
                    dropped_events,
                })
            })
            .await
    }

    pub async fn accept_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(true).await?;
        self.invalidate_observation();
        Ok(())
    }

    pub async fn dismiss_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(false).await?;
        self.invalidate_observation();
        Ok(())
    }

    /// Wait for one explicitly authorized download lifecycle.
    pub async fn wait_for_download(
        &self,
        destination: &Path,
        deadline: Duration,
    ) -> BrowserResult<DownloadOutcome> {
        self.policy.require(PolicyCapability::Download)?;
        if deadline.is_zero() || deadline > MAX_DIAGNOSTIC_DURATION {
            return Err("download deadline must be between 1 ms and 30 seconds".into());
        }
        let destination = self.policy.require_existing_path(destination)?;
        if !destination.is_dir() || !destination.starts_with(&self.upload_root) {
            return Err(
                "download destination must be a directory inside the authorized root".into(),
            );
        }
        let (target_id, frame_id) = self.route_identity().await?;
        let _download_scope = self.download_scope.lock().await;
        let mut events = self.cdp.subscribe_events_with_params();
        let mut download_guard =
            DownloadBehaviorGuard::acquire(self.cdp.clone(), &destination).await?;
        let result = tokio::time::timeout(deadline, async {
            let mut guid = None;
            let mut filename = String::new();
            loop {
                match events.recv().await {
                    Ok(event) if event.method == "Browser.downloadWillBegin" => {
                        if event.params["frameId"].as_str() != Some(frame_id.as_str()) {
                            continue;
                        }
                        guid = event.params["guid"].as_str().map(bounded_diagnostic_text);
                        filename = bounded_diagnostic_text(
                            event.params["suggestedFilename"]
                                .as_str()
                                .unwrap_or("download"),
                        );
                    }
                    Ok(event) if event.method == "Browser.downloadProgress" => {
                        let Some(active_guid) = guid.as_deref() else {
                            continue;
                        };
                        if event.params["guid"].as_str() != Some(active_guid) {
                            continue;
                        }
                        let state = event.params["state"].as_str().unwrap_or("inProgress");
                        if matches!(state, "completed" | "canceled") {
                            return BrowserResult::Ok(DownloadOutcome {
                                guid: active_guid.to_string(),
                                suggested_filename: filename,
                                state: state.to_ascii_lowercase(),
                                received_bytes: finite_nonnegative_u64(
                                    &event.params["receivedBytes"],
                                ),
                                total_bytes: finite_nonnegative_u64(&event.params["totalBytes"]),
                                target_id: target_id.clone(),
                                frame_id: frame_id.clone(),
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        return Err(format!("download event stream dropped {count} events").into());
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err("download event stream closed".into());
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| Err("download deadline exceeded".into()));
        download_guard.disable().await?;
        result
    }

    /// Click an element and return its structured action outcome.
    pub async fn click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(target, false).await
    }

    /// Click one element that is expected to synchronously open exactly one popup.
    ///
    /// This operation never selects the popup. An unanswered `mouseReleased`
    /// request is accepted only with the causal evidence documented by the
    /// automation contract; ordinary [`Self::click`] retains strict ACK behavior.
    pub async fn click_expect_popup(&self, target: &str) -> BrowserResult<PopupClickOutcome> {
        let _scope = self.popup_click_scope.lock().await;
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await
                    .map_err(|error| {
                        tracing::debug!(%error, "popup target node could not be resolved");
                        TargetError {
                            kind: TargetErrorKind::NotActionable,
                            reason: Some(TargetActionabilityReason::NodeUnavailable),
                            candidates: Vec::new(),
                        }
                    })?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                let original_session_id = self
                    .cdp
                    .current_session_id()
                    .ok_or("popup click requires an attached page session")?;
                let original_frame_id = self
                    .cdp
                    .active_frame()
                    .ok_or("popup click requires an active frame")?;
                let backend_node_id = match (element.backend_dom_node_id, element.node_id) {
                    (Some(backend_node_id), _) => backend_node_id,
                    (None, Some(node_id)) => self
                        .cdp
                        .backend_node_id_for_node(node_id)
                        .await
                        .map_err(|error| {
                            popup_error(
                                PopupClickErrorKind::WitnessMissing,
                                format!(
                                    "resolved popup target has no readable backend identity: {error}"
                                ),
                            )
                        })?,
                    (None, None) => {
                        return Err(popup_error(
                            PopupClickErrorKind::WitnessMissing,
                            "popup target has no exact node identity",
                        ));
                    }
                };
                let mut witness = self
                    .arm_popup_witness(&original_session_id, &original_frame_id, backend_node_id)
                    .await?;
                let operation = self
                    .perform_popup_click(&remote.object_id, &element, &mut witness)
                    .await;
                let cleanup = witness.cleanup().await;
                match (operation, cleanup) {
                    (Ok(outcome), Ok(())) => Ok(outcome),
                    (Err(error), _) => Err(error),
                    (Ok(_), Err(error)) => Err(error),
                }
            })
            .await
    }

    async fn arm_popup_witness(
        &self,
        session_id: &str,
        frame_id: &str,
        backend_node_id: i64,
    ) -> BrowserResult<PopupWitnessGuard> {
        let cdp = self.cdp.clone();
        let session_id = session_id.to_string();
        let frame_id = frame_id.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = arm_popup_witness_owned(cdp, session_id, frame_id, backend_node_id).await;
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                popup_error(
                    PopupClickErrorKind::WitnessMissing,
                    "popup witness worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    async fn perform_popup_click(
        &self,
        object_id: &str,
        element: &ResolvedElement,
        witness: &mut PopupWitnessGuard,
    ) -> BrowserResult<PopupClickOutcome> {
        let local_point = self.verified_action_point(object_id).await?;
        let point = self.target_viewport_point(local_point).await?;
        let mut pointer = self.pointer.lock().await;
        let start = match (self.interaction_mode, *pointer) {
            (_, Some(point)) => point,
            (InteractionMode::Human, None) => self
                .viewport_center()
                .await
                .unwrap_or(Point { x: 640.0, y: 360.0 }),
            (InteractionMode::Fast, None) => point,
        };
        let path = interaction_path(self.interaction_mode, &self.mouse, start, point);
        if self.interaction_mode == InteractionMode::Human && pointer.is_none() {
            self.cdp
                .dispatch_mouse_event("mouseMoved", start.x, start.y, None, None)
                .await?;
        }
        for window in path.windows(2) {
            let next = window[1];
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                .await?;
        }
        let press_point = self.verified_action_point(object_id).await?;
        if (press_point.x - local_point.x).abs() > 1.0
            || (press_point.y - local_point.y).abs() > 1.0
        {
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::GeometryChanged),
                candidates: Vec::new(),
            }
            .into());
        }
        self.cdp
            .dispatch_mouse_event("mousePressed", point.x, point.y, Some("left"), Some(1))
            .await?;
        let mut pressed = PressedButtonGuard {
            cdp: self.cdp.clone(),
            point,
            click_count: 1,
            armed: true,
        };
        if self.interaction_mode == InteractionMode::Human {
            tokio::time::sleep(self.mouse.click_delay()).await;
        }

        // This snapshot is intentionally adjacent to and before the release.
        let snapshot = self.popup_topology_snapshot().await?;
        let release_started = std::time::Instant::now();
        let release = self
            .cdp
            .dispatch_mouse_event_with_timeout(
                "mouseReleased",
                point.x,
                point.y,
                Some("left"),
                Some(1),
                POPUP_RELEASE_ACK_TIMEOUT,
            )
            .await;
        let release_ack_wait_ms = release_started.elapsed().as_secs_f64() * 1_000.0;
        let release_acknowledged = match release {
            Ok(_) => true,
            Err(error) if error.is_response_timeout() => false,
            Err(error) => {
                return Err(popup_error(
                    PopupClickErrorKind::ReleaseFailed,
                    format!("mouseReleased failed without a response timeout: {error}"),
                ));
            }
        };
        // The release was either acknowledged or causally witnessed. Never emit
        // the guard's second, fire-and-forget release for the timeout case.
        pressed.armed = false;

        let candidate = self.wait_for_causal_popup(&snapshot, witness).await?;
        let ready_state = self.verify_popup_readiness(&snapshot, &candidate).await?;
        *pointer = Some(point);
        Ok(PopupClickOutcome {
            action: ActionKind::ClickExpectPopup,
            target: ActionTarget {
                label: element.label.clone(),
                reference: element.reference.clone(),
            },
            revision: self.invalidate_observation(),
            target_id: snapshot.original_target_id.clone(),
            frame_id: snapshot.original_frame_id.clone(),
            causally_verified_popup: true,
            popup_id: candidate.target.id.clone(),
            opener_id: snapshot.original_target_id.clone(),
            evidence: PopupVerificationEvidence {
                trusted_click_witness: true,
                release_acknowledged,
                release_ack_wait_ms,
                topology_sequence_before_release: snapshot.sequence,
                popup_observed_sequence: candidate.observed_sequence,
                attached: true,
                ready_state,
            },
        })
    }

    async fn popup_topology_snapshot(&self) -> BrowserResult<PopupTopologySnapshot> {
        let raw_targets = popup_verification_call(
            self.cdp.send_browser("Target.getTargets", None),
            "pre-release target snapshot",
        )
        .await?;
        let mut preexisting_target_ids = HashSet::new();
        for info in raw_targets["targetInfos"].as_array().into_iter().flatten() {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let id = info["targetId"].as_str().ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "pre-release target snapshot contained a page without an ID",
                )
            })?;
            validate_topology_id(id)?;
            preexisting_target_ids.insert(id.to_string());
        }
        let topology = self.topology.lock().await;
        let original_target_id = topology
            .active_target_id
            .clone()
            .ok_or("popup click has no active target")?;
        let original_frame_id = topology
            .active_frame_id
            .clone()
            .ok_or("popup click has no active frame")?;
        Ok(PopupTopologySnapshot {
            original_target_id,
            original_frame_id,
            preexisting_target_ids,
            sequence: topology.sequence,
            event_loss_count: topology.event_loss_count,
        })
    }

    async fn wait_for_causal_popup(
        &self,
        snapshot: &PopupTopologySnapshot,
        witness: &PopupWitnessGuard,
    ) -> BrowserResult<PopupCandidate> {
        let deadline = tokio::time::Instant::now() + POPUP_EVIDENCE_DEADLINE;
        let mut witnessed = false;
        loop {
            if !witnessed {
                witnessed = witness.fired().await?;
            }

            let assessment = {
                let topology = self.topology.lock().await;
                assess_popup_topology(snapshot, &topology, witnessed)
            };
            match assessment {
                Ok(candidate) => return Ok(candidate),
                Err(error)
                    if matches!(
                        error.kind,
                        PopupClickErrorKind::TopologyLagged
                            | PopupClickErrorKind::PopupAmbiguous
                            | PopupClickErrorKind::PopupDestroyed
                    ) =>
                {
                    return Err(error.into());
                }
                Err(error) if tokio::time::Instant::now() >= deadline => {
                    return Err(error.into());
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    async fn verify_popup_readiness(
        &self,
        snapshot: &PopupTopologySnapshot,
        candidate: &PopupCandidate,
    ) -> BrowserResult<String> {
        let mut attachment = self.attach_popup(&candidate.target.id).await?;
        popup_verification_call(
            self.cdp.send_to_session(
                &attachment.session_id,
                "Runtime.runIfWaitingForDebugger",
                None,
            ),
            "popup debugger resume",
        )
        .await?;
        let result = popup_verification_call(
            self.cdp.send_to_session(
                &attachment.session_id,
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.readyState",
                    "returnByValue": true,
                    "awaitPromise": false
                })),
            ),
            "popup readiness evaluation",
        )
        .await?;
        let ready_state = result["result"]["value"]
            .as_str()
            .filter(|state| matches!(*state, "loading" | "interactive" | "complete"))
            .map(str::to_string)
            .ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup returned no valid document.readyState",
                )
            })?;
        self.final_popup_verification(snapshot, candidate).await?;
        attachment.detach().await?;
        Ok(ready_state)
    }

    async fn attach_popup(&self, target_id: &str) -> BrowserResult<PopupAttachmentGuard> {
        let cdp = self.cdp.clone();
        let target_id = target_id.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = match tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.send_browser(
                    "Target.attachToTarget",
                    Some(serde_json::json!({"targetId": target_id, "flatten": true})),
                ),
            )
            .await
            {
                Ok(Ok(value)) => value["sessionId"]
                    .as_str()
                    .map(|session_id| PopupAttachmentGuard {
                        cdp: cdp.clone(),
                        session_id: session_id.to_string(),
                        armed: true,
                    })
                    .ok_or_else(|| {
                        popup_typed_error(
                            PopupClickErrorKind::PopupUnreadable,
                            "popup attach returned no session ID",
                        )
                    }),
                Ok(Err(error)) => Err(popup_typed_error(
                    PopupClickErrorKind::PopupUnreadable,
                    format!("popup attach failed: {error}"),
                )),
                Err(_) => Err(popup_typed_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup attach exceeded its bounded deadline",
                )),
            };
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup attach worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    async fn final_popup_verification(
        &self,
        snapshot: &PopupTopologySnapshot,
        candidate: &PopupCandidate,
    ) -> BrowserResult<()> {
        let (stable_sequence, stable_loss) = {
            let topology = self.topology.lock().await;
            let current = assess_popup_topology(snapshot, &topology, true)?;
            if current.target.id != candidate.target.id {
                return Err(popup_error(
                    PopupClickErrorKind::PopupAmbiguous,
                    "popup candidate changed during readiness verification",
                ));
            }
            (topology.sequence, topology.event_loss_count)
        };
        let targets = popup_verification_call(
            self.cdp.send_browser("Target.getTargets", None),
            "final authoritative popup target discovery",
        )
        .await?;
        let mut matches = Vec::new();
        for info in targets["targetInfos"].as_array().ok_or_else(|| {
            popup_error(
                PopupClickErrorKind::PopupUnreadable,
                "final target discovery returned no target list",
            )
        })? {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let id = info["targetId"].as_str().ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "final target discovery contained a page without an ID",
                )
            })?;
            validate_topology_id(id)?;
            if !snapshot.preexisting_target_ids.contains(id)
                && info["openerId"].as_str() == Some(snapshot.original_target_id.as_str())
            {
                matches.push(id);
            }
        }
        if matches.len() != 1 || matches[0] != candidate.target.id {
            return Err(popup_error(
                if matches.len() > 1 {
                    PopupClickErrorKind::PopupAmbiguous
                } else {
                    PopupClickErrorKind::PopupDestroyed
                },
                format!(
                    "final target discovery found {} live later opener matches",
                    matches.len()
                ),
            ));
        }
        let topology = self.topology.lock().await;
        if topology.sequence != stable_sequence || topology.event_loss_count != stable_loss {
            return Err(popup_error(
                PopupClickErrorKind::TopologyLagged,
                "popup topology changed during final authoritative verification",
            ));
        }
        let current = assess_popup_topology(snapshot, &topology, true)?;
        if current.target.id != candidate.target.id {
            return Err(popup_error(
                PopupClickErrorKind::PopupAmbiguous,
                "popup candidate changed at final topology verification",
            ));
        }
        Ok(())
    }

    /// Double-click an element with the same target, scroll, and pointer
    /// contract as a single click.
    pub async fn double_click(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.pointer_click(target, true).await
    }

    pub async fn hover(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                let local = self.verified_action_point(&remote.object_id).await?;
                let point = self.target_viewport_point(local).await?;
                self.move_pointer(point).await?;
                self.action_outcome(ActionKind::Hover, Some(element), None)
                    .await
            })
            .await
    }

    pub async fn drag(&self, source: &str, destination: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let source = self.resolve_element(source).await?;
                let source_object = self
                    .cdp
                    .resolve_node_object(source.node_id, source.backend_dom_node_id)
                    .await?;
                let source_guard = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id: source_object,
                };
                let destination = self.resolve_element(destination).await?;
                let destination_object = self
                    .cdp
                    .resolve_node_object(destination.node_id, destination.backend_dom_node_id)
                    .await?;
                let destination_guard = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id: destination_object,
                };
                let source_local = self.verified_action_point(&source_guard.object_id).await?;
                let destination_local = self
                    .verified_action_point(&destination_guard.object_id)
                    .await?;
                let source_point = self.target_viewport_point(source_local).await?;
                let destination_point = self.target_viewport_point(destination_local).await?;
                self.move_pointer(source_point).await?;
                let verified_source = self.verified_action_point(&source_guard.object_id).await?;
                if (verified_source.x - source_local.x).abs() > 1.0
                    || (verified_source.y - source_local.y).abs() > 1.0
                {
                    return Err(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::GeometryChanged),
                        candidates: Vec::new(),
                    }
                    .into());
                }
                self.cdp
                    .dispatch_mouse_event(
                        "mousePressed",
                        source_point.x,
                        source_point.y,
                        Some("left"),
                        Some(1),
                    )
                    .await?;
                let mut pressed = PressedButtonGuard {
                    cdp: self.cdp.clone(),
                    point: source_point,
                    click_count: 1,
                    armed: true,
                };
                let drag_path = interaction_path(
                    self.interaction_mode,
                    &self.mouse,
                    source_point,
                    destination_point,
                );
                for window in drag_path.windows(2) {
                    let point = window[1];
                    if self.interaction_mode == InteractionMode::Human {
                        tokio::time::sleep(self.mouse.move_delay(window[0], point)).await;
                    }
                    self.cdp
                        .dispatch_mouse_event("mouseMoved", point.x, point.y, Some("left"), Some(1))
                        .await?;
                }
                let verified_destination = self
                    .verified_action_point(&destination_guard.object_id)
                    .await?;
                if (verified_destination.x - destination_local.x).abs() > 1.0
                    || (verified_destination.y - destination_local.y).abs() > 1.0
                {
                    return Err(TargetError {
                        kind: TargetErrorKind::NotActionable,
                        reason: Some(TargetActionabilityReason::GeometryChanged),
                        candidates: Vec::new(),
                    }
                    .into());
                }
                self.cdp
                    .dispatch_mouse_event(
                        "mouseReleased",
                        destination_point.x,
                        destination_point.y,
                        Some("left"),
                        Some(1),
                    )
                    .await?;
                pressed.armed = false;
                *self.pointer.lock().await = Some(destination_point);
                self.action_outcome(ActionKind::Drag, Some(source), None)
                    .await
            })
            .await
    }

    pub async fn key_down(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.keyboard_action(ActionKind::KeyDown, key, "rawKeyDown", 0)
            .await
    }

    pub async fn key_up(&self, key: &str) -> BrowserResult<ActionOutcome> {
        self.keyboard_action(ActionKind::KeyUp, key, "keyUp", 0)
            .await
    }

    pub async fn key_press(&self, key: &str) -> BrowserResult<ActionOutcome> {
        validate_key(key)?;
        self.cdp
            .with_current_route(async {
                let code = key_code(key);
                self.cdp
                    .dispatch_key_event_with_modifiers("rawKeyDown", key, &code, "", 0)
                    .await?;
                if key.chars().count() == 1 {
                    self.cdp
                        .dispatch_key_event_with_modifiers("char", key, &code, key, 0)
                        .await?;
                }
                self.cdp
                    .dispatch_key_event_with_modifiers("keyUp", key, &code, "", 0)
                    .await?;
                self.action_outcome(ActionKind::KeyPress, None, None).await
            })
            .await
    }

    pub async fn shortcut(&self, shortcut: &str) -> BrowserResult<ActionOutcome> {
        let (modifiers, key) = parse_shortcut(shortcut)?;
        self.cdp
            .with_current_route(async {
                let code = key_code(&key);
                self.cdp
                    .dispatch_key_event_with_modifiers("rawKeyDown", &key, &code, "", modifiers)
                    .await?;
                self.cdp
                    .dispatch_key_event_with_modifiers("keyUp", &key, &code, "", modifiers)
                    .await?;
                self.action_outcome(ActionKind::Shortcut, None, None).await
            })
            .await
    }

    pub async fn clear(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self.cdp.resolve_node_object(element.node_id, element.backend_dom_node_id).await?;
                let remote = RemoteObjectGuard { cdp: self.cdp.clone(), object_id };
                let editable = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement || this.isContentEditable}").await?)?;
                if editable.as_bool() != Some(true) { return Err("clear target is not editable".into()); }
                let clicked = self.click(target).await?;
                self.shortcut(if cfg!(target_os = "macos") {
                    "Meta+A"
                } else {
                    "Control+A"
                })
                .await?;
                self.key_press("Backspace").await?;
                let empty = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement ? this.value === '' : this.textContent === ''}").await?)?;
                if empty.as_bool() != Some(true) { return Err("clear target did not become empty".into()); }
                self.action_outcome_from_target(ActionKind::Clear, clicked.target)
                    .await
            })
            .await
    }

    pub async fn check(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.set_checked(target, true).await
    }

    pub async fn uncheck(&self, target: &str) -> BrowserResult<ActionOutcome> {
        self.set_checked(target, false).await
    }

    pub async fn select_option(&self, target: &str, value: &str) -> BrowserResult<ActionOutcome> {
        if value.is_empty() || value.len() > 4096 {
            return Err("select value must be 1..=4096 bytes".into());
        }
        let value_json = serde_json::to_string(value)?;
        self.form_object_action(target, ActionKind::Select, &format!(r#"function() {{ if (!(this instanceof HTMLSelectElement)) return {{ok:false,reason:'not_select'}}; const option = Array.from(this.options).find(option => option.value === {value_json}); if (!option) return {{ok:false,reason:'option_not_found'}}; this.value = option.value; this.dispatchEvent(new Event('input',{{bubbles:true}})); this.dispatchEvent(new Event('change',{{bubbles:true}})); return {{ok:this.value === option.value}}; }}"#)).await
    }

    pub async fn upload_files(
        &self,
        target: &str,
        paths: &[PathBuf],
    ) -> BrowserResult<ActionOutcome> {
        self.policy.require(PolicyCapability::Upload)?;
        self.cdp.with_current_route(async {
            if paths.is_empty() || paths.len() > 16 { return Err("upload requires 1..=16 files".into()); }
            let mut files = Vec::with_capacity(paths.len());
            for path in paths {
                let canonical = self.policy.require_existing_path(path)?;
                if !canonical.is_file() { return Err("upload path must be a regular file".into()); }
                if !canonical.starts_with(&self.upload_root) { return Err("upload path is outside the allowed workspace root".into()); }
                files.push(canonical.to_string_lossy().into_owned());
            }
            let element = self.resolve_element(target).await?;
            let object_id = self.cdp.resolve_node_object(element.node_id, element.backend_dom_node_id).await?;
            let remote = RemoteObjectGuard { cdp: self.cdp.clone(), object_id };
            self.verified_action_point(&remote.object_id).await?;
            let input = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return {ok:this instanceof HTMLInputElement && this.type === 'file'}}").await?)?;
            if input["ok"].as_bool() != Some(true) { return Err("upload target is not a file input".into()); }
            if element.node_id.is_none() && element.backend_dom_node_id.is_none() { return Err("file input target has no DOM node ID".into()); }
            self.cdp.set_file_input_files(element.node_id, element.backend_dom_node_id, &files).await?;
            let verified = runtime_value(&self.cdp.call_on_object(&remote.object_id, "function(){return this.files.length}").await?)?;
            if verified.as_u64() != Some(files.len() as u64) { return Err("file input did not retain the requested file count".into()); }
            self.action_outcome(ActionKind::Upload, Some(element), Some(serde_json::json!({"file_count": files.len()}))).await
        }).await
    }

    async fn pointer_click(
        &self,
        target: &str,
        double_click: bool,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await
                    .map_err(|error| {
                        tracing::debug!(%error, "target node could not be resolved");
                        TargetError {
                            kind: TargetErrorKind::NotActionable,
                            reason: Some(TargetActionabilityReason::NodeUnavailable),
                            candidates: Vec::new(),
                        }
                    })?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                let local_point = self.verified_action_point(&remote.object_id).await?;
                let point = self.target_viewport_point(local_point).await?;
                let events = if double_click {
                    self.mouse.generate_double_click_events(point)
                } else {
                    self.mouse.generate_click_events(point)
                };
                self.dispatch_pointer_events(&remote.object_id, local_point, point, events)
                    .await?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(ActionOutcome {
                    action: if double_click {
                        ActionKind::DoubleClick
                    } else {
                        ActionKind::Click
                    },
                    target: Some(ActionTarget {
                        label: element.label,
                        reference: element.reference,
                    }),
                    revision: self.invalidate_observation(),
                    target_id,
                    frame_id,
                    evidence: None,
                })
            })
            .await
    }

    async fn dispatch_pointer_events(
        &self,
        object_id: &str,
        local_point: Point,
        point: Point,
        events: Vec<super::mouse::MouseEvent>,
    ) -> BrowserResult<()> {
        let mut pointer = self.pointer.lock().await;
        let start = match (self.interaction_mode, *pointer) {
            (_, Some(point)) => point,
            (InteractionMode::Human, None) => self
                .viewport_center()
                .await
                .unwrap_or(Point { x: 640.0, y: 360.0 }),
            (InteractionMode::Fast, None) => point,
        };
        let path = interaction_path(self.interaction_mode, &self.mouse, start, point);
        if self.interaction_mode == InteractionMode::Human && pointer.is_none() {
            self.cdp
                .dispatch_mouse_event("mouseMoved", start.x, start.y, None, None)
                .await?;
        }
        for window in path.windows(2) {
            let next = window[1];
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                .await?;
        }
        let press_point = self.verified_action_point(object_id).await?;
        if (press_point.x - local_point.x).abs() > 1.0
            || (press_point.y - local_point.y).abs() > 1.0
        {
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::GeometryChanged),
                candidates: Vec::new(),
            }
            .into());
        }
        let mut pressed = None;
        for event in events {
            if event.event_type == "mousePressed" {
                pressed = Some(PressedButtonGuard {
                    cdp: self.cdp.clone(),
                    point,
                    click_count: event.click_count,
                    armed: true,
                });
            }
            self.cdp
                .dispatch_mouse_event(
                    &event.event_type,
                    event.x,
                    event.y,
                    Some(&event.button),
                    Some(event.click_count),
                )
                .await?;
            if event.event_type == "mouseReleased"
                && let Some(mut guard) = pressed.take()
            {
                guard.armed = false;
            }
            if self.interaction_mode == InteractionMode::Human && event.event_type == "mousePressed"
            {
                tokio::time::sleep(self.mouse.click_delay()).await;
            }
        }
        *pointer = Some(point);
        Ok(())
    }

    pub async fn type_text(
        &self,
        text: &str,
        target: Option<&str>,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let target = match target {
                    Some(target) => self.click(target).await?.target,
                    None => None,
                };
                self.cdp.insert_text(text).await?;
                let (target_id, frame_id) = self.route_identity().await?;
                Ok(ActionOutcome {
                    action: ActionKind::Type,
                    target,
                    revision: self.invalidate_observation(),
                    target_id,
                    frame_id,
                    evidence: None,
                })
            })
            .await
    }

    async fn move_pointer(&self, destination: Point) -> BrowserResult<()> {
        let mut pointer = self.pointer.lock().await;
        let start = pointer.unwrap_or(destination);
        for window in
            interaction_path(self.interaction_mode, &self.mouse, start, destination).windows(2)
        {
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], window[1])).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", window[1].x, window[1].y, None, None)
                .await?;
        }
        if start == destination {
            self.cdp
                .dispatch_mouse_event("mouseMoved", destination.x, destination.y, None, None)
                .await?;
        }
        *pointer = Some(destination);
        Ok(())
    }

    async fn keyboard_action(
        &self,
        action: ActionKind,
        key: &str,
        event_type: &str,
        modifiers: i64,
    ) -> BrowserResult<ActionOutcome> {
        validate_key(key)?;
        self.cdp
            .with_current_route(async {
                self.cdp
                    .dispatch_key_event_with_modifiers(
                        event_type,
                        key,
                        &key_code(key),
                        "",
                        modifiers,
                    )
                    .await?;
                self.action_outcome(action, None, None).await
            })
            .await
    }

    async fn set_checked(&self, target: &str, checked: bool) -> BrowserResult<ActionOutcome> {
        let action = if checked {
            ActionKind::Check
        } else {
            ActionKind::Uncheck
        };
        let script = format!(
            r#"function() {{ if (!(this instanceof HTMLInputElement) || !['checkbox','radio'].includes(this.type)) return {{ok:false,reason:'not_checkable'}}; if (this.checked !== {checked}) this.click(); return {{ok:this.checked === {checked}}}; }}"#
        );
        self.form_object_action(target, action, &script).await
    }

    async fn form_object_action(
        &self,
        target: &str,
        action: ActionKind,
        function: &str,
    ) -> BrowserResult<ActionOutcome> {
        self.cdp
            .with_current_route(async {
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await?;
                let remote = RemoteObjectGuard {
                    cdp: self.cdp.clone(),
                    object_id,
                };
                self.verified_action_point(&remote.object_id).await?;
                let result = self.cdp.call_on_object(&remote.object_id, function).await?;
                let value = runtime_value(&result)?;
                if value["ok"].as_bool() != Some(true) {
                    return Err(format!(
                        "form action failed: {}",
                        value["reason"].as_str().unwrap_or("verification_failed")
                    )
                    .into());
                }
                self.action_outcome(action, Some(element), None).await
            })
            .await
    }

    async fn action_outcome(
        &self,
        action: ActionKind,
        element: Option<ResolvedElement>,
        evidence: Option<Value>,
    ) -> BrowserResult<ActionOutcome> {
        let target = element.map(|element| ActionTarget {
            label: element.label,
            reference: element.reference,
        });
        let mut outcome = self.action_outcome_from_target(action, target).await?;
        outcome.evidence = evidence;
        Ok(outcome)
    }

    async fn action_outcome_from_target(
        &self,
        action: ActionKind,
        target: Option<ActionTarget>,
    ) -> BrowserResult<ActionOutcome> {
        if let Some(interception) = &self.policy_interception {
            // A same-route command is an ordering barrier for synchronous
            // click/form navigation. The interception itself remains active
            // for delayed page-authored navigation after this action returns.
            let _ = self.cdp.evaluate("0").await;
            tokio::task::yield_now().await;
            if let Some(error) = interception.take_denial().await {
                return Err(error.into());
            }
        }
        let (target_id, frame_id) = self.route_identity().await?;
        Ok(ActionOutcome {
            action,
            target,
            revision: self.invalidate_observation(),
            target_id,
            frame_id,
            evidence: None,
        })
    }

    async fn viewport_center(&self) -> BrowserResult<Point> {
        let value = self
            .evaluate_value("[window.innerWidth / 2, window.innerHeight / 2]")
            .await?;
        let coordinates = value
            .as_array()
            .filter(|coordinates| coordinates.len() == 2)
            .ok_or("viewport evaluation returned invalid coordinates")?;
        let x = coordinates[0]
            .as_f64()
            .ok_or("viewport width was not numeric")?;
        let y = coordinates[1]
            .as_f64()
            .ok_or("viewport height was not numeric")?;
        Ok(Point { x, y })
    }

    async fn target_viewport_point(&self, point: Point) -> BrowserResult<Point> {
        let Some(frame_id) = self.cdp.active_frame() else {
            return Ok(point);
        };
        let frames = self.list_frames().await?;
        let Some(frame) = frames.iter().find(|frame| frame.id == frame_id) else {
            return Err("selected frame is no longer attached".into());
        };
        if frame.parent_id.is_none() {
            return Ok(point);
        }
        let (x, y) = self.cdp.frame_viewport_offset(&frame_id).await?;
        Ok(Point {
            x: point.x + x,
            y: point.y + y,
        })
    }

    async fn evaluate_value(&self, expression: &str) -> BrowserResult<Value> {
        let raw = self.cdp.evaluate(expression).await?;
        runtime_value(&raw)
    }

    fn invalidate_observation(&self) -> u64 {
        self.page_revision.fetch_add(1, Ordering::Relaxed) + 1
    }

    async fn resolve_element(&self, target: &str) -> BrowserResult<ResolvedElement> {
        let locator = Locator::parse(target)?;
        match self.resolve_locator(&locator).await? {
            TargetResolution::Unique(element) => Ok(element),
            TargetResolution::Ambiguous(candidates) => Err(TargetError {
                kind: TargetErrorKind::Ambiguous,
                reason: None,
                candidates,
            }
            .into()),
            TargetResolution::NotFound => Err(TargetError {
                kind: TargetErrorKind::NotFound,
                reason: None,
                candidates: Vec::new(),
            }
            .into()),
        }
    }

    async fn resolve_locator(&self, locator: &Locator) -> BrowserResult<TargetResolution> {
        if let Locator::Reference(target) = locator {
            let reference = parse_revisioned_reference(target)?
                .ok_or_else(|| format!("invalid revisioned element reference: {target}"))?;
            let current_revision = self.page_revision.load(Ordering::Relaxed);
            if reference.revision != current_revision {
                return Err(TargetError {
                    kind: TargetErrorKind::StaleReference,
                    reason: None,
                    candidates: Vec::new(),
                }
                .into());
            }
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(reference.backend_dom_node_id),
                label: target.to_string(),
                reference: Some(target.to_string()),
            }));
        }

        let snapshot = self.snapshot().await?;
        let matches: Vec<&InteractiveElement> = match locator {
            Locator::AccessibleName(name) => snapshot
                .interactive
                .iter()
                .filter(|element| element.name.eq_ignore_ascii_case(name))
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::RoleAndName { role, name } => snapshot
                .interactive
                .iter()
                .filter(|element| {
                    element.role.eq_ignore_ascii_case(role)
                        && element.name.eq_ignore_ascii_case(name)
                })
                .take(AMBIGUOUS_CANDIDATE_LIMIT + 1)
                .collect(),
            Locator::Ordinal(index) => snapshot.interactive.get(index - 1).into_iter().collect(),
            Locator::Reference(_) | Locator::Text(_) | Locator::Css(_) => Vec::new(),
        };
        if matches.len() == 1 {
            let element = matches[0];
            return Ok(TargetResolution::Unique(ResolvedElement {
                node_id: None,
                backend_dom_node_id: Some(element.backend_dom_node_id),
                label: format!("{} {}", element.role, element.name),
                reference: Some(element.reference.clone()),
            }));
        }
        if matches.len() > 1 {
            return Ok(TargetResolution::Ambiguous(
                matches
                    .into_iter()
                    .take(AMBIGUOUS_CANDIDATE_LIMIT)
                    .map(|element| CandidateSummary {
                        label: bounded_candidate_label(&format!(
                            "{} {}",
                            element.role, element.name
                        )),
                        reference: Some(element.reference.clone()),
                    })
                    .collect(),
            ));
        }

        match locator {
            Locator::Css(selector) => {
                let expression = css_query_expression(selector)?;
                let (count, nodes) = self
                    .cdp
                    .bounded_element_query(&expression, AMBIGUOUS_CANDIDATE_LIMIT)
                    .await?;
                dom_nodes_resolution(count, nodes, format!("css={selector}"), "css match")
            }
            Locator::Text(text) => {
                let expression = text_query_expression(text)?;
                let (count, nodes) = self
                    .cdp
                    .bounded_element_query(&expression, AMBIGUOUS_CANDIDATE_LIMIT)
                    .await?;
                if count > 1 {
                    return Ok(TargetResolution::Ambiguous(
                        (1..=count.min(AMBIGUOUS_CANDIDATE_LIMIT))
                            .map(|index| CandidateSummary {
                                label: format!("text match {index}"),
                                reference: None,
                            })
                            .collect(),
                    ));
                }
                dom_nodes_resolution(count, nodes, format!("text={text}"), "text match")
            }
            Locator::Reference(_)
            | Locator::AccessibleName(_)
            | Locator::RoleAndName { .. }
            | Locator::Ordinal(_) => Ok(TargetResolution::NotFound),
        }
    }

    async fn verified_action_point(&self, object_id: &str) -> BrowserResult<Point> {
        let raw = match self.cdp.call_on_object(object_id, HIT_TEST_FUNCTION).await {
            Ok(raw) => raw,
            Err(error) => {
                tracing::debug!(%error, "target node could not be verified");
                return Err(TargetError {
                    kind: TargetErrorKind::NotActionable,
                    reason: Some(TargetActionabilityReason::NodeUnavailable),
                    candidates: Vec::new(),
                }
                .into());
            }
        };
        let value = runtime_value(&raw)?;
        if value["ok"].as_bool() != Some(true) {
            let reason = value["reason"].as_str().unwrap_or("verification_failed");
            tracing::debug!(reason, "target actionability check failed");
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(actionability_reason(reason)),
                candidates: Vec::new(),
            }
            .into());
        }
        let x = value["x"]
            .as_f64()
            .ok_or("verified target x was not numeric")?;
        let y = value["y"]
            .as_f64()
            .ok_or("verified target y was not numeric")?;
        Ok(Point { x, y })
    }
}

fn interactive_elements(roots: &[AxNode], revision: u64) -> Vec<InteractiveElement> {
    find_interactive_elements(roots)
        .into_iter()
        .filter_map(|node| {
            let backend_dom_node_id = node.backend_dom_node_id?;
            Some(InteractiveElement {
                reference: backend_node_reference(revision, backend_dom_node_id),
                role: node.role.clone(),
                name: node.name.clone(),
                description: node.description.clone(),
                backend_dom_node_id,
            })
        })
        .collect()
}

fn truncate_visible_text(text: &str, max_bytes: usize) -> String {
    truncate_visible_text_with_status(text, max_bytes).0
}

fn collect_diagnostic_event(
    event: &CdpEventWithParams,
    console: &mut Vec<ConsoleEvidence>,
    network: &mut Vec<NetworkEvidence>,
    request_indexes: &mut HashMap<String, usize>,
    dropped: &mut u64,
) {
    match event.method.as_str() {
        "Runtime.consoleAPICalled" => {
            push_bounded(
                console,
                ConsoleEvidence {
                    level: bounded_diagnostic_text(event.params["type"].as_str().unwrap_or("log")),
                    text: "[console arguments redacted]".to_string(),
                },
                dropped,
            );
        }
        "Log.entryAdded" => {
            let entry = &event.params["entry"];
            push_bounded(
                console,
                ConsoleEvidence {
                    level: bounded_diagnostic_text(entry["level"].as_str().unwrap_or("log")),
                    text: redact_diagnostic_text(entry["text"].as_str().unwrap_or("")),
                },
                dropped,
            );
        }
        "Network.requestWillBeSent" => {
            let Some(request_id) = event.params["requestId"].as_str() else {
                return;
            };
            if let Some(index) = request_indexes.get(request_id).copied() {
                network[index].redirect_count = network[index].redirect_count.saturating_add(1);
                return;
            }
            if network.len() >= MAX_DIAGNOSTIC_EVENTS {
                *dropped = dropped.saturating_add(1);
                return;
            }
            let request = &event.params["request"];
            let index = network.len();
            request_indexes.insert(request_id.to_string(), index);
            network.push(NetworkEvidence {
                request_id: bounded_diagnostic_text(request_id),
                method: bounded_diagnostic_text(request["method"].as_str().unwrap_or("")),
                url: redact_diagnostic_url(request["url"].as_str().unwrap_or("")),
                status: None,
                failure: None,
                safe_header_names: safe_header_names(&request["headers"]),
                redirect_count: u16::from(event.params.get("redirectResponse").is_some()),
            });
        }
        "Network.responseReceived" => {
            if let Some(index) = event.params["requestId"]
                .as_str()
                .and_then(|id| request_indexes.get(id))
                .copied()
            {
                network[index].status = event.params["response"]["status"]
                    .as_u64()
                    .and_then(|status| u16::try_from(status).ok());
            }
        }
        "Network.loadingFailed" => {
            if let Some(index) = event.params["requestId"]
                .as_str()
                .and_then(|id| request_indexes.get(id))
                .copied()
            {
                network[index].failure = Some(bounded_diagnostic_text(
                    event.params["errorText"]
                        .as_str()
                        .unwrap_or("request_failed"),
                ));
            }
        }
        _ => {}
    }
}

fn push_bounded<T>(values: &mut Vec<T>, value: T, dropped: &mut u64) {
    if values.len() < MAX_DIAGNOSTIC_EVENTS {
        values.push(value);
    } else {
        *dropped = dropped.saturating_add(1);
    }
}

fn bounded_diagnostic_text(value: &str) -> String {
    truncate_utf8_bytes(value, MAX_DIAGNOSTIC_TEXT_BYTES)
}

fn redact_diagnostic_text(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if ["authorization", "cookie", "password", "token", "secret"]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return "[redacted sensitive console entry]".to_string();
    }
    let redacted = value
        .split_whitespace()
        .map(|part| {
            if part.starts_with("http://") || part.starts_with("https://") {
                redact_diagnostic_url(part)
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    bounded_diagnostic_text(&redacted)
}

fn truncate_utf8_bytes(value: &str, max_bytes: usize) -> String {
    let mut end = value.len().min(max_bytes);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn redact_diagnostic_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return truncate_utf8_bytes(
            value.split('?').next().unwrap_or(""),
            MAX_DIAGNOSTIC_URL_BYTES,
        );
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    if url.query().is_some() {
        let names = url
            .query_pairs()
            .map(|(name, _)| name.into_owned())
            .collect::<Vec<_>>();
        url.set_query(None);
        if !names.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(names.iter().map(|name| (name.as_str(), "[redacted]")));
        }
    }
    url.set_fragment(None);
    truncate_utf8_bytes(url.as_str(), MAX_DIAGNOSTIC_URL_BYTES)
}

fn safe_header_names(headers: &Value) -> Vec<String> {
    let mut names = headers
        .as_object()
        .into_iter()
        .flat_map(|headers| headers.keys())
        .filter(|name| {
            !matches!(
                name.to_ascii_lowercase().as_str(),
                "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
            )
        })
        .take(32)
        .map(|name| truncate_utf8_bytes(name, 128))
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn finite_nonnegative_u64(value: &Value) -> u64 {
    value
        .as_f64()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.min(u64::MAX as f64) as u64)
        .unwrap_or(0)
}

fn validate_visual_options(options: &VisualCaptureOptions) -> BrowserResult<()> {
    if !options.scale.is_finite() || !(0.1..=4.0).contains(&options.scale) {
        return Err("visual scale must be finite and between 0.1 and 4.0".into());
    }
    if options.full_page as u8 + options.clip.is_some() as u8 + options.target.is_some() as u8 > 1 {
        return Err("full-page, clip, and element capture are mutually exclusive".into());
    }
    if options.format == VisualFormat::Png && options.quality.is_some() {
        return Err("PNG capture does not accept quality".into());
    }
    if options.quality.is_some_and(|quality| quality > 100) {
        return Err("visual quality must be between 0 and 100".into());
    }
    if let Some(clip) = options.clip {
        for value in [clip.x, clip.y, clip.width, clip.height] {
            if !value.is_finite() {
                return Err("visual clip values must be finite".into());
            }
        }
        if clip.x < 0.0 || clip.y < 0.0 || clip.width <= 0.0 || clip.height <= 0.0 {
            return Err("visual clip must have non-negative origin and positive size".into());
        }
        validate_effective_visual_clip(Some(clip), options.scale)?;
    }
    Ok(())
}

fn validate_effective_visual_clip(clip: Option<VisualClip>, scale: f64) -> BrowserResult<()> {
    let Some(clip) = clip else { return Ok(()) };
    let width = clip.width * scale;
    let height = clip.height * scale;
    if width > 16_384.0 || height > 16_384.0 || width * height > MAX_VISUAL_PIXELS {
        return Err("visual output exceeds the 16384-axis or 8-megapixel budget".into());
    }
    Ok(())
}

fn visual_clips_match(first: VisualClip, second: VisualClip) -> bool {
    [
        (first.x, second.x),
        (first.y, second.y),
        (first.width, second.width),
        (first.height, second.height),
    ]
    .into_iter()
    .all(|(first, second)| (first - second).abs() <= 0.5)
}

fn visual_rect(value: &Value) -> BrowserResult<VisualClip> {
    let clip = VisualClip {
        x: value["x"].as_f64().unwrap_or(0.0),
        y: value["y"].as_f64().unwrap_or(0.0),
        width: value["width"].as_f64().ok_or("visual width was missing")?,
        height: value["height"]
            .as_f64()
            .ok_or("visual height was missing")?,
    };
    validate_visual_options(&VisualCaptureOptions {
        clip: Some(clip),
        ..VisualCaptureOptions::default()
    })?;
    Ok(clip)
}

fn visual_viewport_rect(value: &Value) -> BrowserResult<VisualClip> {
    visual_rect(&serde_json::json!({
        "x": value["pageX"].as_f64().unwrap_or(0.0),
        "y": value["pageY"].as_f64().unwrap_or(0.0),
        "width": value["clientWidth"].as_f64().ok_or("visual viewport width was missing")?,
        "height": value["clientHeight"].as_f64().ok_or("visual viewport height was missing")?
    }))
}

fn decoded_base64_len(value: &str) -> BrowserResult<usize> {
    if !value.len().is_multiple_of(4) {
        return Err("visual base64 payload had invalid length".into());
    }
    let padding = value
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    Ok(value.len() / 4 * 3 - padding)
}

async fn stop_screencast_for(cdp: &CdpClient, session_id: Option<&str>) -> BrowserResult<()> {
    match session_id {
        Some(session_id) => {
            cdp.send_to_session(session_id, "Page.stopScreencast", None)
                .await?;
        }
        None => {
            cdp.send("Page.stopScreencast", None).await?;
        }
    }
    Ok(())
}

async fn disable_fetch_for(cdp: &CdpClient, session_id: Option<&str>) -> BrowserResult<()> {
    match session_id {
        Some(session_id) => {
            cdp.send_to_session(session_id, "Fetch.disable", None)
                .await?
        }
        None => cdp.send("Fetch.disable", None).await?,
    };
    Ok(())
}

async fn enable_fetch_for(cdp: &CdpClient, session_id: &str) -> BrowserResult<()> {
    cdp.send_to_session(
        session_id,
        "Fetch.enable",
        Some(serde_json::json!({
            "patterns": [{
                "urlPattern": "*",
                "resourceType": "Document",
                "requestStage": "Request"
            }]
        })),
    )
    .await?;
    Ok(())
}

fn visual_quad_rect(value: &Value) -> BrowserResult<VisualClip> {
    let values = value
        .as_array()
        .ok_or("visual element border quad was missing")?;
    if values.len() != 8 {
        return Err("visual element border quad must contain eight coordinates".into());
    }
    let coordinates = values
        .iter()
        .map(|value| {
            value
                .as_f64()
                .ok_or("visual border coordinate was not numeric")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let xs = [
        coordinates[0],
        coordinates[2],
        coordinates[4],
        coordinates[6],
    ];
    let ys = [
        coordinates[1],
        coordinates[3],
        coordinates[5],
        coordinates[7],
    ];
    let left = xs.into_iter().fold(f64::INFINITY, f64::min);
    let right = xs.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let top = ys.into_iter().fold(f64::INFINITY, f64::min);
    let bottom = ys.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let width = right - left;
    let height = bottom - top;
    if width <= 0.0 || height <= 0.0 {
        return Err("visual element has empty geometry".into());
    }
    Ok(VisualClip {
        x: left,
        y: top,
        width,
        height,
    })
}

fn truncate_visible_text_with_status(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }

    let content_limit = max_bytes.saturating_sub(TEXT_TRUNCATION_MARKER.len());
    let mut end = content_limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    let mut truncated = text[..end].to_string();
    if max_bytes >= TEXT_TRUNCATION_MARKER.len() {
        truncated.push_str(TEXT_TRUNCATION_MARKER);
    }
    (truncated, true)
}

fn interaction_path(
    mode: InteractionMode,
    mouse: &MouseEngine,
    start: Point,
    end: Point,
) -> Vec<Point> {
    match mode {
        InteractionMode::Human => mouse.generate_path(start, end),
        InteractionMode::Fast => vec![start, end],
    }
}

fn validate_key(key: &str) -> BrowserResult<()> {
    if key.is_empty() || key.len() > 64 || key.chars().any(char::is_control) {
        return Err("key must be 1..=64 printable UTF-8 bytes".into());
    }
    Ok(())
}

fn key_code(key: &str) -> String {
    match key {
        " " => "Space".to_string(),
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Enter" | "Tab" | "Escape"
        | "Backspace" | "Delete" | "Home" | "End" | "PageUp" | "PageDown" => key.to_string(),
        _ if key.chars().count() == 1 => {
            let character = key.chars().next().unwrap();
            if character.is_ascii_alphabetic() {
                format!("Key{}", character.to_ascii_uppercase())
            } else if character.is_ascii_digit() {
                format!("Digit{character}")
            } else {
                key.to_string()
            }
        }
        _ => key.to_string(),
    }
}

fn parse_shortcut(value: &str) -> BrowserResult<(i64, String)> {
    if value.is_empty() || value.len() > 256 {
        return Err("shortcut must be 1..=256 bytes".into());
    }
    let mut modifiers = 0;
    let mut key = None;
    for part in value.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "alt" => modifiers |= 1,
            "control" | "ctrl" => modifiers |= 2,
            "meta" | "cmd" | "command" => modifiers |= 4,
            "shift" => modifiers |= 8,
            _ if key.is_none() => key = Some(part.to_string()),
            _ => return Err("shortcut must contain exactly one non-modifier key".into()),
        }
    }
    let key = key.ok_or("shortcut requires a non-modifier key")?;
    validate_key(&key)?;
    Ok((modifiers, key))
}

fn context_event_invalidates_observation(method: &str) -> bool {
    matches!(
        method,
        "Page.frameNavigated"
            | "Page.loadEventFired"
            | "Page.frameStartedLoading"
            | "Page.frameStoppedLoading"
            | "DOM.documentUpdated"
            | "DOM.childNodeInserted"
            | "DOM.childNodeRemoved"
            | "DOM.attributeModified"
            | "DOM.attributeRemoved"
            | "DOM.characterDataModified"
            | "DOM.setChildNodes"
    )
}

#[derive(Debug)]
struct ResolvedElement {
    node_id: Option<i64>,
    backend_dom_node_id: Option<i64>,
    label: String,
    reference: Option<String>,
}

struct PressedButtonGuard {
    cdp: CdpClient,
    point: Point,
    click_count: u32,
    armed: bool,
}

struct RemoteObjectGuard {
    cdp: CdpClient,
    object_id: String,
}

struct PopupWitnessGuard {
    cdp: CdpClient,
    session_id: String,
    state_object_id: String,
    element_object_id: String,
    armed: bool,
}

struct PopupAttachmentGuard {
    cdp: CdpClient,
    session_id: String,
    armed: bool,
}

struct NetworkDomainGuard {
    cdp: CdpClient,
    leases: Arc<Mutex<NetworkLeaseState>>,
    session_id: Option<String>,
    armed: bool,
}

struct DiagnosticDomainGuard {
    cdp: CdpClient,
    network: NetworkDomainGuard,
    leases: Arc<Mutex<DiagnosticLeaseState>>,
    session_id: Option<String>,
    armed: bool,
}

struct DownloadBehaviorGuard {
    cdp: CdpClient,
    armed: bool,
}

#[derive(Default)]
struct DiagnosticLeaseState {
    counts: HashMap<Option<String>, usize>,
}

#[derive(Default)]
struct NetworkLeaseState {
    counts: HashMap<Option<String>, usize>,
}

impl NetworkDomainGuard {
    async fn acquire(cdp: CdpClient, leases: Arc<Mutex<NetworkLeaseState>>) -> BrowserResult<Self> {
        let session_id = cdp.current_session_id();
        let mut state = leases.lock().await;
        let count = state.counts.entry(session_id.clone()).or_default();
        *count += 1;
        let mut guard = Self {
            cdp,
            leases: Arc::clone(&leases),
            session_id: session_id.clone(),
            armed: true,
        };
        if *count == 1
            && let Err(error) = guard
                .cdp
                .set_domain_enabled_for(session_id.clone(), "Network", true)
                .await
        {
            state.counts.remove(&session_id);
            guard.armed = false;
            drop(state);
            let _ = guard
                .cdp
                .set_domain_enabled_for(session_id, "Network", false)
                .await;
            return Err(error.into());
        }
        drop(state);
        Ok(guard)
    }

    async fn disable(&mut self) -> BrowserResult<()> {
        let state = self.leases.lock().await;
        release_network_lease_locked(&self.cdp, &self.session_id, state).await?;
        self.armed = false;
        Ok(())
    }
}

impl DiagnosticDomainGuard {
    async fn acquire(
        cdp: CdpClient,
        network_leases: Arc<Mutex<NetworkLeaseState>>,
        leases: Arc<Mutex<DiagnosticLeaseState>>,
    ) -> BrowserResult<Self> {
        let network = NetworkDomainGuard::acquire(cdp.clone(), network_leases).await?;
        let session_id = cdp.current_session_id();
        let mut state = leases.lock().await;
        let count = state.counts.entry(session_id.clone()).or_default();
        *count += 1;
        if *count == 1 {
            if let Err(error) = cdp
                .set_domain_enabled_for(session_id.clone(), "Runtime", true)
                .await
            {
                state.counts.remove(&session_id);
                return Err(error.into());
            }
            if let Err(error) = cdp
                .set_domain_enabled_for(session_id.clone(), "Log", true)
                .await
            {
                state.counts.remove(&session_id);
                let _ = cdp
                    .set_domain_enabled_for(session_id.clone(), "Runtime", false)
                    .await;
                return Err(error.into());
            }
        }
        drop(state);
        Ok(Self {
            cdp,
            network,
            leases,
            session_id,
            armed: true,
        })
    }

    async fn disable(&mut self) -> BrowserResult<()> {
        let mut state = self.leases.lock().await;
        let count = state.counts.entry(self.session_id.clone()).or_default();
        *count = count.saturating_sub(1);
        if *count == 0 {
            state.counts.remove(&self.session_id);
            self.cdp
                .set_domain_enabled_for(self.session_id.clone(), "Log", false)
                .await?;
            self.cdp
                .set_domain_enabled_for(self.session_id.clone(), "Runtime", false)
                .await?;
        }
        drop(state);
        self.network.disable().await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for DiagnosticDomainGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let leases = Arc::clone(&self.leases);
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let mut state = leases.lock().await;
            let count = state.counts.entry(session_id.clone()).or_default();
            *count = count.saturating_sub(1);
            if *count == 0 {
                state.counts.remove(&session_id);
                let _ = cdp
                    .set_domain_enabled_for(session_id.clone(), "Log", false)
                    .await;
                let _ = cdp
                    .set_domain_enabled_for(session_id, "Runtime", false)
                    .await;
            }
        });
    }
}

impl DownloadBehaviorGuard {
    async fn acquire(cdp: CdpClient, destination: &Path) -> BrowserResult<Self> {
        cdp.set_download_behavior("allow", Some(destination), true)
            .await?;
        Ok(Self { cdp, armed: true })
    }

    async fn disable(&mut self) -> BrowserResult<()> {
        self.cdp.set_download_behavior("deny", None, false).await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for DownloadBehaviorGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        tokio::spawn(async move {
            let _ = cdp.set_download_behavior("deny", None, false).await;
        });
    }
}

impl Drop for NetworkDomainGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let leases = Arc::clone(&self.leases);
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = release_network_lease(&cdp, &leases, &session_id).await;
        });
    }
}

async fn release_network_lease(
    cdp: &CdpClient,
    leases: &Mutex<NetworkLeaseState>,
    session_id: &Option<String>,
) -> BrowserResult<()> {
    release_network_lease_locked(cdp, session_id, leases.lock().await).await
}

async fn release_network_lease_locked(
    cdp: &CdpClient,
    session_id: &Option<String>,
    mut state: tokio::sync::MutexGuard<'_, NetworkLeaseState>,
) -> BrowserResult<()> {
    let count = state.counts.entry(session_id.clone()).or_default();
    *count = count.saturating_sub(1);
    if *count == 0 {
        state.counts.remove(session_id);
        cdp.set_domain_enabled_for(session_id.clone(), "Network", false)
            .await?;
    }
    Ok(())
}

impl Drop for RemoteObjectGuard {
    fn drop(&mut self) {
        let cdp = self.cdp.clone();
        let object_id = self.object_id.clone();
        tokio::spawn(async move {
            let _ = cdp.release_object(&object_id).await;
        });
    }
}

impl PopupWitnessGuard {
    async fn fired(&self) -> BrowserResult<bool> {
        let value = popup_verification_call(
            self.cdp.send_to_session(
                &self.session_id,
                "Runtime.callFunctionOn",
                Some(serde_json::json!({
                    "objectId": self.state_object_id,
                    "functionDeclaration": "function(){return this.read();}",
                    "returnByValue": true,
                    "awaitPromise": false
                })),
            ),
            "popup witness read",
        )
        .await?;
        value["result"]["value"].as_bool().ok_or_else(|| {
            popup_error(
                PopupClickErrorKind::WitnessMissing,
                "popup witness returned no trusted-click state",
            )
        })
    }

    async fn cleanup(&mut self) -> BrowserResult<()> {
        cleanup_popup_witness(
            &self.cdp,
            &self.session_id,
            &self.state_object_id,
            &self.element_object_id,
        )
        .await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for PopupWitnessGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        let state_object_id = self.state_object_id.clone();
        let element_object_id = self.element_object_id.clone();
        tokio::spawn(async move {
            let _ = cleanup_popup_witness(&cdp, &session_id, &state_object_id, &element_object_id)
                .await;
        });
    }
}

impl PopupAttachmentGuard {
    async fn detach(&mut self) -> BrowserResult<()> {
        popup_verification_call(
            self.cdp.send_browser(
                "Target.detachFromTarget",
                Some(serde_json::json!({"sessionId": self.session_id})),
            ),
            "popup detach",
        )
        .await?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for PopupAttachmentGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            let _ = tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": session_id})),
                ),
            )
            .await;
        });
    }
}

impl Drop for PressedButtonGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let cdp = self.cdp.clone();
        let point = self.point;
        let click_count = self.click_count;
        tokio::spawn(async move {
            let _ = cdp
                .dispatch_mouse_event(
                    "mouseReleased",
                    point.x,
                    point.y,
                    Some("left"),
                    Some(click_count),
                )
                .await;
        });
    }
}

impl WaitCondition {
    pub fn parse(value: &str) -> BrowserResult<Self> {
        if value.len() > MAX_WAIT_CONDITION_BYTES {
            return Err("wait condition exceeds 4096 bytes".into());
        }
        let (kind, argument) = value
            .split_once('=')
            .ok_or("wait condition must use <kind>=<value>")?;
        if argument.is_empty() {
            return Err("wait condition value cannot be empty".into());
        }
        Ok(match kind {
            "lifecycle" if matches!(argument, "load" | "domcontentloaded" | "complete") => {
                Self::Lifecycle(
                    if argument == "load" {
                        "complete"
                    } else {
                        argument
                    }
                    .to_string(),
                )
            }
            "url" => Self::UrlExact(argument.to_string()),
            "url-prefix" => Self::UrlPrefix(argument.to_string()),
            "target-attached" => Self::TargetAttached(argument.to_string()),
            "target-visible" => Self::TargetVisible(argument.to_string()),
            "target-hidden" => Self::TargetHidden(argument.to_string()),
            "target-enabled" => Self::TargetEnabled(argument.to_string()),
            "target-stable" => Self::TargetStable(argument.to_string()),
            "text" => Self::Text(argument.to_string()),
            "js" => Self::JavaScript(argument.to_string()),
            "network-quiet" => {
                let duration = Duration::from_millis(argument.parse::<u64>()?);
                if duration.is_zero() || duration > MAX_WAIT_DEADLINE {
                    return Err("network quiet duration must be between 1 ms and 300000 ms".into());
                }
                Self::NetworkQuiet(duration)
            }
            "lifecycle" => return Err("unsupported lifecycle wait value".into()),
            _ => return Err("unknown wait condition kind".into()),
        })
    }

    fn description(&self) -> String {
        match self {
            Self::Lifecycle(_) => "lifecycle".to_string(),
            Self::UrlExact(_) => "url_exact".to_string(),
            Self::UrlPrefix(_) => "url_prefix".to_string(),
            Self::TargetAttached(_) => "target_attached".to_string(),
            Self::TargetVisible(_) => "target_visible".to_string(),
            Self::TargetHidden(_) => "target_hidden".to_string(),
            Self::TargetEnabled(_) => "target_enabled".to_string(),
            Self::TargetStable(_) => "target_stable".to_string(),
            Self::Text(_) => "text".to_string(),
            Self::JavaScript(_) => "javascript_predicate".to_string(),
            Self::NetworkQuiet(_) => "network_quiet".to_string(),
        }
    }

    fn validate(&self) -> BrowserResult<()> {
        let value = match self {
            Self::Lifecycle(value)
            | Self::UrlExact(value)
            | Self::UrlPrefix(value)
            | Self::TargetAttached(value)
            | Self::TargetVisible(value)
            | Self::TargetHidden(value)
            | Self::TargetEnabled(value)
            | Self::TargetStable(value)
            | Self::Text(value)
            | Self::JavaScript(value) => Some(value),
            Self::NetworkQuiet(duration) => {
                if duration.is_zero() || *duration > MAX_WAIT_DEADLINE {
                    return Err("network quiet duration must be between 1 ms and 300000 ms".into());
                }
                None
            }
        };
        if value.is_some_and(|value| value.is_empty() || value.len() > MAX_WAIT_CONDITION_BYTES) {
            return Err("wait condition value must contain 1-4096 bytes".into());
        }
        Ok(())
    }
}

fn bounded_wait_state(value: &str) -> String {
    truncate_visible_text(value, WAIT_LAST_STATE_MAX_BYTES)
}

fn validate_wait_deadline(deadline: Duration) -> BrowserResult<()> {
    if deadline.is_zero() || deadline > MAX_WAIT_DEADLINE {
        return Err("wait deadline must be between 1 ms and 300000 ms".into());
    }
    Ok(())
}

fn wait_timeout(condition: &str, deadline: Duration, last_state: &str) -> WaitTimeout {
    WaitTimeout {
        condition: condition.to_string(),
        deadline_ms: deadline.as_millis() as u64,
        last_state: bounded_wait_state(last_state),
        reason: "deadline_exceeded",
    }
}

impl Locator {
    pub fn parse(value: &str) -> BrowserResult<Self> {
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return Err("element target cannot be empty".into());
        }
        if parse_revisioned_reference(value)?.is_some() {
            return Ok(Self::Reference(value.to_string()));
        }
        if let Some(reference) = value.strip_prefix("ref=") {
            if reference.is_empty() {
                return Err("reference locator cannot be empty".into());
            }
            return Ok(Self::Reference(reference.to_string()));
        }
        if let Some(name) = value.strip_prefix("name=") {
            return nonempty_locator(name, "accessible name").map(Self::AccessibleName);
        }
        if let Some(text) = value.strip_prefix("text=") {
            return nonempty_locator(text, "text").map(Self::Text);
        }
        if let Some(selector) = value.strip_prefix("css=") {
            return nonempty_locator(selector, "CSS selector").map(Self::Css);
        }
        if let Some(index) = value.strip_prefix("ordinal=") {
            let index = index
                .parse::<usize>()
                .ok()
                .filter(|index| *index > 0)
                .ok_or("ordinal locator must be a positive one-based integer")?;
            return Ok(Self::Ordinal(index));
        }
        if let Some(rest) = value.strip_prefix("role=") {
            let (role, name) = rest
                .split_once(";name=")
                .ok_or("role locator must use role=<role>;name=<accessible name>")?;
            return Ok(Self::RoleAndName {
                role: nonempty_locator(role, "role")?,
                name: nonempty_locator(name, "accessible name")?,
            });
        }
        Ok(Self::AccessibleName(value.to_string()))
    }
}

fn nonempty_locator(value: &str, kind: &str) -> BrowserResult<String> {
    if value.is_empty() {
        return Err(format!("{kind} locator cannot be empty").into());
    }
    Ok(value.to_string())
}

fn dom_nodes_resolution(
    count: usize,
    nodes: Vec<i64>,
    label: String,
    candidate_kind: &str,
) -> BrowserResult<TargetResolution> {
    match count {
        0 => Ok(TargetResolution::NotFound),
        1 if nodes.len() == 1 => Ok(TargetResolution::Unique(ResolvedElement {
            node_id: Some(nodes[0]),
            backend_dom_node_id: None,
            label,
            reference: None,
        })),
        1 => Err("unique element query returned no DOM node".into()),
        _ => Ok(TargetResolution::Ambiguous(
            nodes
                .into_iter()
                .take(AMBIGUOUS_CANDIDATE_LIMIT)
                .enumerate()
                .map(|(index, _)| CandidateSummary {
                    label: format!("{candidate_kind} {}", index + 1),
                    reference: None,
                })
                .collect(),
        )),
    }
}

fn css_query_expression(selector: &str) -> BrowserResult<String> {
    let selector = serde_json::to_string(selector)?;
    Ok(format!(
        "(() => {{ const nodes = document.querySelectorAll({selector}); const out = []; for (let i = 0; i < Math.min(nodes.length, {AMBIGUOUS_CANDIDATE_LIMIT}); i++) out.push(nodes[i]); out.glassCount = nodes.length; return out; }})()"
    ))
}

fn text_query_expression(text: &str) -> BrowserResult<String> {
    let text = serde_json::to_string(text)?;
    Ok(format!(
        r#"(() => {{
            const wanted = ({text}).replace(/\s+/g, ' ').trim();
            const matches = [];
            let count = 0;
            const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_ELEMENT);
            for (let element = walker.currentNode; element; element = walker.nextNode()) {{
                const style = getComputedStyle(element);
                const rect = element.getBoundingClientRect();
                if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || rect.width <= 0 || rect.height <= 0) continue;
                if (element.checkVisibility && !element.checkVisibility({{checkOpacity:true, checkVisibilityCSS:true}})) continue;
                let clipped = false;
                let visibleLeft = rect.left, visibleTop = rect.top, visibleRight = rect.right, visibleBottom = rect.bottom;
                for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {{
                    const ancestorStyle = getComputedStyle(ancestor);
                    if (ancestorStyle.display === 'none' || ancestorStyle.visibility === 'hidden' || Number(ancestorStyle.opacity) === 0) {{ clipped = true; break; }}
                    if (/(hidden|clip)/.test(ancestorStyle.overflow + ancestorStyle.overflowX + ancestorStyle.overflowY)) {{
                        const bounds = ancestor.getBoundingClientRect();
                        visibleLeft = Math.max(visibleLeft, bounds.left); visibleTop = Math.max(visibleTop, bounds.top);
                        visibleRight = Math.min(visibleRight, bounds.right); visibleBottom = Math.min(visibleBottom, bounds.bottom);
                        if (visibleRight <= visibleLeft || visibleBottom <= visibleTop) {{ clipped = true; break; }}
                    }}
                }}
                if (clipped) continue;
                const actual = (element.innerText || '').replace(/\s+/g, ' ').trim();
                if (actual !== wanted) continue;
                const candidate = element.closest('button,a,input,select,textarea,[role],[tabindex]') || element;
                if (matches.includes(candidate)) continue;
                for (let index = matches.length - 1; index >= 0; index--) {{
                    if (matches[index].contains(candidate)) {{ matches.splice(index, 1); count--; }}
                }}
                count++;
                if (matches.length < {AMBIGUOUS_CANDIDATE_LIMIT}) matches.push(candidate);
            }}
            matches.glassCount = count;
            return matches;
        }})()"#
    ))
}

fn visible_text_contains_expression(text: &str) -> BrowserResult<String> {
    let text = serde_json::to_string(text)?;
    Ok(format!(
        r#"(() => {{
            const wanted = {text};
            const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT);
            for (let node = walker.nextNode(); node; node = walker.nextNode()) {{
                if (!(node.nodeValue || '').includes(wanted)) continue;
                const element = node.parentElement;
                if (!element) continue;
                if (element.checkVisibility && !element.checkVisibility({{checkOpacity:true, checkVisibilityCSS:true}})) continue;
                const rect = element.getBoundingClientRect();
                if (rect.width <= 0 || rect.height <= 0) continue;
                let left = rect.left, top = rect.top, right = rect.right, bottom = rect.bottom, hidden = false;
                for (let ancestor = element; ancestor; ancestor = ancestor.parentElement) {{
                    const style = getComputedStyle(ancestor);
                    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) {{ hidden = true; break; }}
                    if (/(hidden|clip)/.test(style.overflow + style.overflowX + style.overflowY)) {{
                        const bounds = ancestor.getBoundingClientRect();
                        left = Math.max(left, bounds.left); top = Math.max(top, bounds.top);
                        right = Math.min(right, bounds.right); bottom = Math.min(bottom, bounds.bottom);
                        if (right <= left || bottom <= top) {{ hidden = true; break; }}
                    }}
                }}
                if (!hidden) return true;
            }}
            return false;
        }})()"#
    ))
}

fn bounded_candidate_label(value: &str) -> String {
    if value.len() <= CANDIDATE_LABEL_MAX_BYTES {
        return value.to_string();
    }
    let mut end = CANDIDATE_LABEL_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn validate_topology_id(value: &str) -> BrowserResult<()> {
    if value.is_empty() {
        return Err("topology ID cannot be empty".into());
    }
    if value.len() > TOPOLOGY_ID_MAX_BYTES {
        return Err(format!("topology ID exceeds {TOPOLOGY_ID_MAX_BYTES} UTF-8 bytes").into());
    }
    Ok(())
}

fn bounded_topology_text(value: &str) -> String {
    if value.len() <= TOPOLOGY_TEXT_MAX_BYTES {
        return value.to_string();
    }
    let mut end = TOPOLOGY_TEXT_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn collect_frames(
    frame_tree: &Value,
    parent_id: Option<&str>,
    active_frame_id: Option<&str>,
    frames: &mut Vec<FrameInfo>,
) -> BrowserResult<()> {
    if frames.len() == TOPOLOGY_MAX_FRAMES {
        return Err("frame limit exceeded".into());
    }
    let frame = frame_tree
        .get("frame")
        .ok_or("Page.getFrameTree returned a node without frame data")?;
    let id = frame["id"]
        .as_str()
        .ok_or("Page.getFrameTree returned a frame without an ID")?;
    validate_topology_id(id)?;
    frames.push(FrameInfo {
        id: id.to_string(),
        parent_id: parent_id.map(str::to_string),
        url: bounded_topology_text(frame["url"].as_str().unwrap_or_default()),
        active: active_frame_id == Some(id),
        out_of_process: false,
    });
    if let Some(children) = frame_tree["childFrames"].as_array() {
        for child in children {
            collect_frames(child, Some(id), active_frame_id, frames)?;
        }
    }
    Ok(())
}

fn push_topology_event(topology: &mut TopologyRegistry, kind: &str, id: &str) -> u64 {
    topology.sequence = topology
        .sequence
        .checked_add(1)
        .expect("topology sequence exhausted");
    let sequence = topology.sequence;
    topology.events.push_back(TopologyEventSummary {
        sequence,
        kind: kind.to_string(),
        id: bounded_topology_id(id),
    });
    while topology.events.len() > TOPOLOGY_MAX_EVENTS {
        topology.events.pop_front();
    }
    sequence
}

fn bounded_topology_id(value: &str) -> String {
    if value.len() <= TOPOLOGY_ID_MAX_BYTES {
        return value.to_string();
    }
    let mut end = TOPOLOGY_ID_MAX_BYTES - "…".len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn retained_optional_topology_id(value: Option<&str>) -> BrowserResult<Option<String>> {
    value
        .map(|id| {
            validate_topology_id(id)?;
            Ok(id.to_string())
        })
        .transpose()
}

fn popup_typed_error(kind: PopupClickErrorKind, message: impl Into<String>) -> PopupClickError {
    let message = message.into();
    PopupClickError {
        kind,
        message: truncate_utf8_bytes(&message, POPUP_ERROR_MESSAGE_MAX_BYTES),
    }
}

fn popup_witness_install_function() -> String {
    format!(
        r#"function() {{
            const element = this;
            const nativeAdd = EventTarget.prototype.addEventListener;
            const nativeRemove = EventTarget.prototype.removeEventListener;
            const nativeApply = Reflect.apply;
            const nativeSetTimeout = setTimeout;
            const nativeClearTimeout = clearTimeout;
            const state = {{fired:false, cleaned:false}};
            const listener = function(event) {{
                if (event.isTrusted === true && event.currentTarget === element) state.fired = true;
            }};
            let timer;
            const cleanup = function() {{
                if (state.cleaned) return true;
                state.cleaned = true;
                nativeApply(nativeRemove, element, ['click', listener, true]);
                nativeClearTimeout(timer);
                return true;
            }};
            Object.defineProperties(state, {{
                read: {{value:function(){{return state.fired;}}, enumerable:false}},
                cleanup: {{value:cleanup, enumerable:false}}
            }});
            nativeApply(nativeAdd, element, ['click', listener, {{capture:true, once:true}}]);
            timer = nativeSetTimeout(cleanup, {POPUP_WITNESS_LIFETIME_MS});
            return state;
        }}"#
    )
}

async fn popup_witness_call<F>(future: F, step: &str) -> Result<Value, PopupClickError>
where
    F: std::future::Future<Output = Result<Value, super::cdp::CdpError>>,
{
    match tokio::time::timeout(POPUP_VERIFY_CALL_TIMEOUT, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            format!("{step} failed: {error}"),
        )),
        Err(_) => Err(popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            format!("{step} exceeded its bounded deadline"),
        )),
    }
}

async fn arm_popup_witness_owned(
    cdp: CdpClient,
    session_id: String,
    frame_id: String,
    backend_node_id: i64,
) -> Result<PopupWitnessGuard, PopupClickError> {
    let world = popup_witness_call(
        cdp.send_to_session(
            &session_id,
            "Page.createIsolatedWorld",
            Some(serde_json::json!({
                "frameId": frame_id,
                "worldName": "glass-popup-witness"
            })),
        ),
        "popup witness isolated world",
    )
    .await?;
    let context_id = world["executionContextId"].as_i64().ok_or_else(|| {
        popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            "popup witness isolated world returned no context ID",
        )
    })?;
    let resolved = popup_witness_call(
        cdp.send_to_session(
            &session_id,
            "DOM.resolveNode",
            Some(serde_json::json!({
                "backendNodeId": backend_node_id,
                "executionContextId": context_id
            })),
        ),
        "popup witness exact-node resolution",
    )
    .await?;
    let element_object_id = resolved["object"]["objectId"]
        .as_str()
        .ok_or_else(|| {
            popup_typed_error(
                PopupClickErrorKind::WitnessMissing,
                "popup witness exact-node resolution returned no object",
            )
        })?
        .to_string();
    let install = popup_witness_call(
        cdp.send_to_session(
            &session_id,
            "Runtime.callFunctionOn",
            Some(serde_json::json!({
                "objectId": element_object_id,
                "functionDeclaration": popup_witness_install_function(),
                "returnByValue": false,
                "awaitPromise": false
            })),
        ),
        "popup witness installation",
    )
    .await;
    let state_object_id = match install {
        Ok(value) => match value["result"]["objectId"].as_str() {
            Some(id) => id.to_string(),
            None => {
                let _ = tokio::time::timeout(
                    POPUP_VERIFY_CALL_TIMEOUT,
                    cdp.release_object_for_session(&session_id, &element_object_id),
                )
                .await;
                return Err(popup_typed_error(
                    PopupClickErrorKind::WitnessMissing,
                    "popup witness installation returned no private state",
                ));
            }
        },
        Err(error) => {
            let _ = tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.release_object_for_session(&session_id, &element_object_id),
            )
            .await;
            return Err(error);
        }
    };
    Ok(PopupWitnessGuard {
        cdp,
        session_id,
        state_object_id,
        element_object_id,
        armed: true,
    })
}

async fn cleanup_popup_witness(
    cdp: &CdpClient,
    session_id: &str,
    state_object_id: &str,
    element_object_id: &str,
) -> BrowserResult<()> {
    let cleanup = tokio::time::timeout(
        POPUP_VERIFY_CALL_TIMEOUT,
        cdp.send_to_session(
            session_id,
            "Runtime.callFunctionOn",
            Some(serde_json::json!({
                "objectId": state_object_id,
                "functionDeclaration": "function(){return this.cleanup();}",
                "returnByValue": true,
                "awaitPromise": false
            })),
        ),
    )
    .await;
    let release_state = tokio::time::timeout(
        POPUP_VERIFY_CALL_TIMEOUT,
        cdp.release_object_for_session(session_id, state_object_id),
    )
    .await;
    let release_element = tokio::time::timeout(
        POPUP_VERIFY_CALL_TIMEOUT,
        cdp.release_object_for_session(session_id, element_object_id),
    )
    .await;
    if !matches!(cleanup, Ok(Ok(_)))
        || !matches!(release_state, Ok(Ok(_)))
        || !matches!(release_element, Ok(Ok(_)))
    {
        return Err(popup_error(
            PopupClickErrorKind::PopupUnreadable,
            "popup witness remote-object cleanup failed",
        ));
    }
    Ok(())
}

fn popup_error(kind: PopupClickErrorKind, message: impl Into<String>) -> Box<dyn Error> {
    Box::new(popup_typed_error(kind, message))
}

fn assess_popup_topology(
    snapshot: &PopupTopologySnapshot,
    topology: &TopologyRegistry,
    witnessed: bool,
) -> Result<PopupCandidate, PopupClickError> {
    if !witnessed {
        return Err(popup_typed_error(
            PopupClickErrorKind::WitnessMissing,
            "trusted-click witness was not observed",
        ));
    }
    if topology.event_loss_count != snapshot.event_loss_count {
        return Err(popup_typed_error(
            PopupClickErrorKind::TopologyLagged,
            "topology event stream lagged after mouseReleased",
        ));
    }

    let later_targets = topology.targets.iter().filter_map(|target| {
        let sequence = *topology.target_sequences.get(&target.id)?;
        (!snapshot.preexisting_target_ids.contains(&target.id) && sequence > snapshot.sequence)
            .then_some((target, sequence))
    });
    let mut matching = Vec::new();
    let mut nonmatching_count = 0usize;
    for (target, sequence) in later_targets {
        if target.opener_id.as_deref() == Some(snapshot.original_target_id.as_str()) {
            matching.push(PopupCandidate {
                target: target.clone(),
                observed_sequence: sequence,
            });
        } else {
            nonmatching_count += 1;
        }
    }
    if matching.len() > 1 {
        return Err(popup_typed_error(
            PopupClickErrorKind::PopupAmbiguous,
            format!(
                "trusted click produced {} opener-matching page targets",
                matching.len()
            ),
        ));
    }
    if let Some(candidate) = matching.pop() {
        return Ok(candidate);
    }
    if topology.destroyed_targets.iter().any(|destroyed| {
        destroyed.observed_sequence > snapshot.sequence
            && !snapshot
                .preexisting_target_ids
                .contains(&destroyed.target.id)
            && destroyed.target.opener_id.as_deref() == Some(snapshot.original_target_id.as_str())
    }) {
        return Err(popup_typed_error(
            PopupClickErrorKind::PopupDestroyed,
            "the opener-matching popup was destroyed before verification",
        ));
    }
    if nonmatching_count > 0 {
        return Err(popup_typed_error(
            PopupClickErrorKind::PopupOpenerMismatch,
            "later page targets did not name the original target as opener",
        ));
    }
    Err(popup_typed_error(
        PopupClickErrorKind::PopupMissing,
        "no later opener-matching popup was observed",
    ))
}

async fn popup_verification_call<F>(future: F, step: &str) -> BrowserResult<Value>
where
    F: std::future::Future<Output = Result<Value, super::cdp::CdpError>>,
{
    match tokio::time::timeout(POPUP_VERIFY_CALL_TIMEOUT, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(popup_error(
            PopupClickErrorKind::PopupUnreadable,
            format!("{step} failed: {error}"),
        )),
        Err(_) => Err(popup_error(
            PopupClickErrorKind::PopupUnreadable,
            format!("{step} exceeded its bounded deadline"),
        )),
    }
}

/// Apply one bounded lifecycle notification. Returns true when the selected
/// target was lost and CDP command routing must be cleared.
fn apply_topology_event(topology: &mut TopologyRegistry, event: &CdpEventWithParams) -> bool {
    match event.method.as_str() {
        "Target.targetCreated" | "Target.targetInfoChanged" => {
            let info = &event.params["targetInfo"];
            if info["type"].as_str() != Some("page") {
                return false;
            }
            let Some(id) = info["targetId"].as_str() else {
                return false;
            };
            if validate_topology_id(id).is_err() {
                push_topology_event(topology, "rejected-target", id);
                return false;
            }
            let Ok(opener_id) = retained_optional_topology_id(info["openerId"].as_str()) else {
                push_topology_event(topology, "rejected-target-opener", id);
                return false;
            };
            let target = PageTargetInfo {
                id: id.to_string(),
                url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
                title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
                opener_id,
                active: topology.active_target_id.as_deref() == Some(id),
            };
            if let Some(existing) = topology.targets.iter_mut().find(|target| target.id == id) {
                *existing = target;
            } else if topology.targets.len() < TOPOLOGY_MAX_TARGETS {
                topology.targets.push(target);
            } else {
                push_topology_event(topology, "rejected-target-budget", id);
                return false;
            }
            let sequence = push_topology_event(topology, "target-updated", id);
            topology.target_sequences.insert(id.to_string(), sequence);
        }
        "Target.targetDestroyed" | "Target.targetCrashed" => {
            let Some(id) = event.params["targetId"].as_str() else {
                return false;
            };
            let removed = topology
                .targets
                .iter()
                .find(|target| target.id == id)
                .cloned();
            topology.targets.retain(|target| target.id != id);
            topology.target_sequences.remove(id);
            let sequence = push_topology_event(topology, event.method.as_str(), id);
            if let Some(target) = removed {
                topology.destroyed_targets.push_back(DestroyedPageTarget {
                    target,
                    observed_sequence: sequence,
                });
                while topology.destroyed_targets.len() > TOPOLOGY_MAX_EVENTS {
                    topology.destroyed_targets.pop_front();
                }
            }
            if topology.active_target_id.as_deref() == Some(id) {
                topology.active_target_id = None;
                topology.active_target_session_id = None;
                topology.active_session_id = None;
                topology.active_frame_id = None;
                topology.frames.clear();
                topology.frame_sessions.clear();
                topology.frame_parents.clear();
                return true;
            }
        }
        "Target.detachedFromTarget" => {
            let Some(session_id) = event.params["sessionId"].as_str() else {
                return false;
            };
            push_topology_event(topology, "Target.detachedFromTarget", session_id);
            if topology.active_target_session_id.as_deref() == Some(session_id) {
                topology.active_target_id = None;
                topology.active_target_session_id = None;
                topology.active_session_id = None;
                topology.active_frame_id = None;
                topology.frames.clear();
                return true;
            }
            let active_session_detached = topology.active_session_id.as_deref() == Some(session_id);
            topology
                .frame_sessions
                .retain(|_, attached_session| attached_session != session_id);
            if active_session_detached {
                topology.active_frame_id = None;
                topology.active_session_id = topology.active_target_session_id.clone();
            }
        }
        "Target.attachedToTarget" => {
            let info = &event.params["targetInfo"];
            if info["type"].as_str() != Some("iframe") {
                return false;
            }
            let (Some(frame_id), Some(session_id)) = (
                info["targetId"].as_str(),
                event.params["sessionId"].as_str(),
            ) else {
                return false;
            };
            if validate_topology_id(frame_id).is_err() || validate_topology_id(session_id).is_err()
            {
                return false;
            }
            if topology.frame_sessions.len() < TOPOLOGY_MAX_FRAMES {
                topology
                    .frame_sessions
                    .insert(frame_id.to_string(), session_id.to_string());
                push_topology_event(topology, "Target.attachedToTarget", frame_id);
            }
        }
        "Page.frameAttached" | "Page.frameNavigated" | "Page.frameDetached" => {
            let event_session = event.session_id.as_deref();
            let belongs_to_topology = event_session == topology.active_target_session_id.as_deref()
                || event_session.is_some_and(|session_id| {
                    topology
                        .frame_sessions
                        .values()
                        .any(|attached| attached == session_id)
                });
            if !belongs_to_topology {
                return false;
            }
            let id = event.params["frameId"]
                .as_str()
                .or_else(|| event.params["frame"]["id"].as_str());
            if let Some(id) = id
                && validate_topology_id(id).is_ok()
            {
                push_topology_event(topology, event.method.as_str(), id);
                if event.method == "Page.frameAttached"
                    && let Some(parent_id) = event.params["parentFrameId"].as_str()
                    && validate_topology_id(parent_id).is_ok()
                {
                    topology
                        .frame_parents
                        .insert(id.to_string(), parent_id.to_string());
                }
                if event.method == "Page.frameDetached" {
                    topology.frame_parents.remove(id);
                }
            }
            let selected_was_affected = id.is_some_and(|changed_id| {
                topology.active_frame_id.as_deref() == Some(changed_id)
                    || frame_is_descendant_of(
                        &topology.frames,
                        topology.active_frame_id.as_deref(),
                        changed_id,
                    )
            });
            let selected_is_main = topology
                .active_frame_id
                .as_deref()
                .and_then(|selected| topology.frames.iter().find(|frame| frame.id == selected))
                .is_some_and(|frame| frame.parent_id.is_none());
            topology.frames.clear();
            if selected_was_affected
                && matches!(
                    event.method.as_str(),
                    "Page.frameNavigated" | "Page.frameDetached"
                )
                && !(event.method == "Page.frameNavigated" && selected_is_main)
            {
                topology.active_frame_id = None;
            }
        }
        _ => {}
    }
    false
}

fn frame_is_descendant_of(frames: &[FrameInfo], selected: Option<&str>, ancestor: &str) -> bool {
    let Some(mut current) = selected else {
        return false;
    };
    while let Some(frame) = frames.iter().find(|frame| frame.id == current) {
        let Some(parent) = frame.parent_id.as_deref() else {
            return false;
        };
        if parent == ancestor {
            return true;
        }
        current = parent;
    }
    false
}

async fn resync_topology(
    cdp: &CdpClient,
    registry: &Arc<Mutex<TopologyRegistry>>,
) -> BrowserResult<()> {
    let raw = cdp.send_browser("Target.getTargets", None).await?;
    let (active_target, active_frame, target_session) = {
        let topology = registry.lock().await;
        (
            topology.active_target_id.clone(),
            topology.active_frame_id.clone(),
            topology.active_target_session_id.clone(),
        )
    };
    let mut targets = Vec::new();
    for info in raw["targetInfos"].as_array().into_iter().flatten() {
        if info["type"].as_str() != Some("page") {
            continue;
        }
        let id = info["targetId"]
            .as_str()
            .ok_or("Target.getTargets returned a page without an ID")?;
        validate_topology_id(id)?;
        if targets.len() == TOPOLOGY_MAX_TARGETS {
            return Err("page target limit exceeded during topology resync".into());
        }
        targets.push(PageTargetInfo {
            id: id.to_string(),
            url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
            title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
            opener_id: retained_optional_topology_id(info["openerId"].as_str())?,
            active: active_target.as_deref() == Some(id),
        });
    }
    let mut frames = Vec::new();
    if let Some(target_session) = target_session {
        let raw = cdp
            .send_to_session(&target_session, "Page.getFrameTree", None)
            .await?;
        collect_frames(
            &raw["frameTree"],
            None,
            active_frame.as_deref(),
            &mut frames,
        )?;
    }
    let mut topology = registry.lock().await;
    topology.targets = targets;
    topology.frames = frames;
    let sequence = push_topology_event(&mut topology, "resynchronized", "topology");
    topology.target_sequences = topology
        .targets
        .iter()
        .map(|target| (target.id.clone(), sequence))
        .collect();
    Ok(())
}

fn actionability_reason(reason: &str) -> TargetActionabilityReason {
    match reason {
        "detached" => TargetActionabilityReason::Detached,
        "not_visible" => TargetActionabilityReason::NotVisible,
        "disabled" => TargetActionabilityReason::Disabled,
        "unstable_geometry" => TargetActionabilityReason::UnstableGeometry,
        "outside_viewport" => TargetActionabilityReason::OutsideViewport,
        "hit_test_blocked" => TargetActionabilityReason::HitTestBlocked,
        _ => TargetActionabilityReason::VerificationFailed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RevisionedElementReference {
    revision: u64,
    backend_dom_node_id: i64,
}

/// Parse the public `r<revision>:b<backend-node-id>` reference shape.
///
/// Values that do not resemble this exact shape remain normal accessible-name
/// or CSS-selector targets. A malformed value with the marker is an explicit
/// error instead of a silent fallback.
fn parse_revisioned_reference(value: &str) -> BrowserResult<Option<RevisionedElementReference>> {
    let Some(rest) = value.strip_prefix('r') else {
        return Ok(None);
    };
    let Some((revision, backend_dom_node_id)) = rest.split_once(":b") else {
        return Ok(None);
    };
    if revision.is_empty() || backend_dom_node_id.is_empty() {
        return Err(format!("invalid revisioned element reference: {value}").into());
    }
    let revision = revision
        .parse::<u64>()
        .map_err(|_| format!("invalid element reference revision: {value}"))?;
    let backend_dom_node_id = backend_dom_node_id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| format!("invalid backend node ID in element reference: {value}"))?;
    Ok(Some(RevisionedElementReference {
        revision,
        backend_dom_node_id,
    }))
}

fn runtime_value(raw: &Value) -> BrowserResult<Value> {
    if let Some(exception) = raw.get("exceptionDetails") {
        return Err(format!("JavaScript evaluation failed: {exception}").into());
    }
    Ok(raw["result"]["value"].clone())
}

async fn wait_for_ws_url(port: u16, target_id: Option<&str>) -> BrowserResult<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match get_ws_url(port, target_id).await {
            Ok(url) => return Ok(url),
            Err(error)
                if error.to_string().starts_with("No page target")
                    && tokio::time::Instant::now() < deadline =>
            {
                tracing::debug!(%error, "waiting for Chrome page target");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("about:")
        || url.starts_with("file:")
        || url.starts_with("data:")
    {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    fn test_session(cdp: CdpClient) -> BrowserSession {
        cdp.set_active_target_route(
            Some("test-target".to_string()),
            None,
            Some("test-frame".to_string()),
            None,
        );
        BrowserSession {
            cdp,
            chrome: None,
            disposable_profile: None,
            profile: "test".to_string(),
            interaction_mode: InteractionMode::Fast,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision: Arc::new(AtomicU64::new(1)),
            observation_cache: Mutex::new(None),
            network_wait_leases: Arc::new(Mutex::new(NetworkLeaseState::default())),
            diagnostic_leases: Arc::new(Mutex::new(DiagnosticLeaseState::default())),
            download_scope: Arc::new(Mutex::new(())),
            topology: Arc::new(Mutex::new(TopologyRegistry {
                active_target_id: Some("test-target".to_string()),
                active_frame_id: Some("test-frame".to_string()),
                ..TopologyRegistry::default()
            })),
            popup_click_scope: Mutex::new(()),
            upload_root: std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
            policy: BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap(),
            policy_interception: None,
        }
    }

    #[test]
    fn attach_options_reject_launch_only_configuration() {
        let attached = SessionOptions {
            attach: true,
            ..SessionOptions::default()
        };
        assert!(attached.validate().is_ok());

        let mut incognito = attached.clone();
        incognito.incognito = true;
        assert!(
            incognito
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--incognito")
        );

        let mut profile = attached.clone();
        profile.profile = "work".to_string();
        assert!(
            profile
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--profile")
        );

        let mut chrome_path = attached.clone();
        chrome_path.chrome_path = Some(PathBuf::from("/tmp/chrome"));
        assert!(
            chrome_path
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--chrome-path")
        );

        let mut headed = attached;
        headed.headed = true;
        assert!(
            headed
                .validate()
                .unwrap_err()
                .to_string()
                .contains("--headed")
        );
    }

    #[test]
    fn target_id_must_not_be_empty() {
        let options = SessionOptions {
            target_id: Some("   ".to_string()),
            ..SessionOptions::default()
        };

        assert!(
            options
                .validate()
                .unwrap_err()
                .to_string()
                .contains("target ID")
        );
    }

    #[test]
    fn disposable_incognito_directories_are_unique_and_removed() {
        let first = DisposableProfileDir::create().unwrap();
        let first_path = first.path().to_path_buf();
        let second = DisposableProfileDir::create().unwrap();
        let second_path = second.path().to_path_buf();

        assert_ne!(first_path, second_path);
        assert!(first_path.is_dir());
        assert!(second_path.is_dir());

        drop(first);
        drop(second);
        assert!(!first_path.exists());
        assert!(!second_path.exists());
    }

    #[test]
    fn disposable_cleanup_removes_only_provably_abandoned_profiles() {
        let root = std::env::temp_dir().join(format!(
            "glass-disposable-cleanup-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let active = root.join("incognito-active");
        let dead = root.join("incognito-dead");
        let malformed = root.join("incognito-malformed");
        for path in [&active, &dead, &malformed] {
            std::fs::create_dir(path).unwrap();
        }
        let active_owner = DisposableProfileOwner {
            pid: std::process::id(),
            process_start: process_start_identity(std::process::id()).unwrap(),
        };
        std::fs::write(
            active.join(DISPOSABLE_OWNER_FILE),
            serde_json::to_vec(&active_owner).unwrap(),
        )
        .unwrap();
        let dead_owner = DisposableProfileOwner {
            pid: u32::MAX,
            process_start: 1,
        };
        std::fs::write(
            dead.join(DISPOSABLE_OWNER_FILE),
            serde_json::to_vec(&dead_owner).unwrap(),
        )
        .unwrap();
        std::fs::write(malformed.join(DISPOSABLE_OWNER_FILE), b"not-json").unwrap();

        DisposableProfileDir::cleanup_abandoned(&root).unwrap();

        assert!(active.exists());
        assert!(!dead.exists());
        assert!(malformed.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disposable_cleanup_scans_beyond_one_memory_batch() {
        let root = std::env::temp_dir().join(format!(
            "glass-disposable-batch-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let owner = serde_json::to_vec(&DisposableProfileOwner {
            pid: u32::MAX,
            process_start: 1,
        })
        .unwrap();
        for index in 0..=DISPOSABLE_CLEANUP_BATCH {
            let path = root.join(format!("incognito-{index:04}"));
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join(DISPOSABLE_OWNER_FILE), &owner).unwrap();
        }

        DisposableProfileDir::cleanup_abandoned(&root).unwrap();

        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn disposable_profile_is_recovered_after_forced_process_exit() {
        let path_record = std::env::temp_dir().join(format!(
            "glass-crash-profile-path-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "browser::session::tests::disposable_profile_crash_helper",
                "--ignored",
            ])
            .env("GLASS_CRASH_PROFILE_PATH_RECORD", &path_record)
            .status()
            .unwrap();
        assert!(!status.success());
        let abandoned = PathBuf::from(std::fs::read_to_string(&path_record).unwrap());
        assert!(abandoned.exists());

        DisposableProfileDir::cleanup_abandoned(&std::env::temp_dir().join("glass")).unwrap();

        assert!(!abandoned.exists());
        std::fs::remove_file(path_record).unwrap();
    }

    #[test]
    #[ignore = "subprocess helper for forced-exit recovery"]
    fn disposable_profile_crash_helper() {
        let Some(path_record) = std::env::var_os("GLASS_CRASH_PROFILE_PATH_RECORD") else {
            return;
        };
        let profile = DisposableProfileDir::create().unwrap();
        std::fs::write(path_record, profile.path().to_string_lossy().as_bytes()).unwrap();
        std::mem::forget(profile);
        std::process::exit(86);
    }

    async fn observation_server(include_dom: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut saw_runtime = false;
            let mut saw_accessibility = false;
            let mut saw_deep_dom = false;

            for _ in 0..if include_dom { 5 } else { 4 } {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Page.createIsolatedWorld") => {
                        serde_json::json!({"executionContextId": 71})
                    }
                    Some("Runtime.evaluate") => {
                        saw_runtime = true;
                        let expression = request["params"]["expression"].as_str().unwrap();
                        assert!(expression.contains("document.body.innerText"));
                        assert!(!expression.contains(".slice(0,"));
                        let text = format!(
                            "{}😀{}",
                            "a".repeat(4_095),
                            "b".repeat(COMPACT_TEXT_MAX_BYTES)
                        );
                        let page_state = serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text,
                            "mutation_revision": 0,
                            "boundaries": {
                                "scanned_elements": 12,
                                "scan_limit": 512,
                                "shadow_roots": 1,
                                "child_frames": 1,
                                "canvases": 1,
                                "truncated": false
                            }
                        })
                        .to_string();
                        serde_json::json!({
                            "result": {"value": page_state}
                        })
                    }
                    Some("Accessibility.getFullAXTree") => {
                        saw_accessibility = true;
                        serde_json::json!({"nodes": []})
                    }
                    Some("DOM.getDocument") => {
                        saw_deep_dom = true;
                        assert_eq!(request["params"], serde_json::json!({"depth": -1}));
                        serde_json::json!({
                            "root": {
                                "nodeId": 1,
                                "nodeName": "#document",
                                "nodeValue": "",
                                "children": []
                            }
                        })
                    }
                    method => panic!("unexpected compact-observation command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }

            assert!(saw_runtime);
            assert!(saw_accessibility);
            assert_eq!(saw_deep_dom, include_dom);
        });
        (format!("ws://{address}"), server)
    }

    async fn mutation_race_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut runtime_revision = 0_u64;
            for _ in 0..7 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Page.createIsolatedWorld") => {
                        serde_json::json!({"executionContextId": 72})
                    }
                    Some("Runtime.evaluate") => {
                        runtime_revision += 1;
                        serde_json::json!({"result": {"value": serde_json::json!({
                            "url": "https://race.test",
                            "title": "Race",
                            "ready_state": "complete",
                            "text": "changing",
                            "mutation_revision": runtime_revision,
                            "boundaries": {"scanned_elements": 1, "scan_limit": 512,
                                "shadow_roots": 0, "child_frames": 0, "canvases": 0,
                                "truncated": false}
                        }).to_string()}})
                    }
                    Some("Accessibility.getFullAXTree") => serde_json::json!({"nodes": []}),
                    method => panic!("unexpected mutation-race command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({
                            "id": request["id"], "result": result
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        (format!("ws://{address}"), server)
    }

    async fn diagnostic_cleanup_server() -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut methods = Vec::new();
            for _ in 0..6 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                methods.push(request["method"].as_str().unwrap().to_string());
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": {}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
            methods
        });
        (format!("ws://{address}"), server)
    }

    async fn large_accessibility_server() -> (String, tokio::task::JoinHandle<()>, String) {
        let huge_text = "x".repeat(33 * 1024);
        let tree = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "root",
                    "role": {"value": "RootWebArea"},
                    "name": {"value": huge_text.clone()},
                    "description": {"value": huge_text.clone()},
                    "value": {"value": huge_text.clone()},
                    "childIds": ["save"]
                },
                {
                    "nodeId": "save",
                    "parentId": "root",
                    "backendDOMNodeId": 42,
                    "role": {"value": "button"},
                    "name": {"value": "Save"},
                    "description": {"value": huge_text.clone()},
                    "value": {"value": huge_text.clone()},
                    "childIds": []
                }
            ]
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let text_for_server = huge_text.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            for _ in 0..6 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Page.createIsolatedWorld") => {
                        serde_json::json!({"executionContextId": 73})
                    }
                    Some("Runtime.evaluate") => serde_json::json!({
                        "result": {"value": serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text_for_server.clone(),
                        }).to_string()}
                    }),
                    Some("Accessibility.getFullAXTree") => tree.clone(),
                    method => panic!("unexpected compact-observation command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        (format!("ws://{address}"), server, huge_text)
    }

    #[test]
    fn normalizes_urls_without_touching_supported_schemes() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(normalize_url(" about:blank "), "about:blank");
        assert_eq!(
            normalize_url("file:///tmp/page.html"),
            "file:///tmp/page.html"
        );
    }

    #[test]
    fn interaction_modes_plan_smooth_or_direct_motion() {
        let mouse = MouseEngine::new();
        let start = Point { x: 10.0, y: 20.0 };
        let end = Point { x: 410.0, y: 220.0 };

        let human = interaction_path(InteractionMode::Human, &mouse, start, end);
        let fast = interaction_path(InteractionMode::Fast, &mouse, start, end);

        assert!(human.len() > 2);
        assert_eq!(human.first(), Some(&start));
        assert_eq!(human.last(), Some(&end));
        assert_eq!(fast, vec![start, end]);
    }

    #[test]
    fn topology_events_never_select_a_popup_and_clear_a_lost_active_target() {
        let mut topology = TopologyRegistry {
            active_target_id: Some("page-1".to_string()),
            active_session_id: Some("session-1".to_string()),
            ..TopologyRegistry::default()
        };
        let popup = CdpEventWithParams {
            method: "Target.targetCreated".to_string(),
            params: serde_json::json!({"targetInfo": {
                "type": "page", "targetId": "popup-1", "url": "about:blank",
                "title": "", "openerId": "page-1"
            }}),
            session_id: None,
        };
        assert!(!apply_topology_event(&mut topology, &popup));
        assert_eq!(topology.active_target_id.as_deref(), Some("page-1"));
        assert_eq!(topology.targets[0].opener_id.as_deref(), Some("page-1"));

        let crashed = CdpEventWithParams {
            method: "Target.targetCrashed".to_string(),
            params: serde_json::json!({"targetId": "page-1"}),
            session_id: None,
        };
        assert!(apply_topology_event(&mut topology, &crashed));
        assert!(topology.active_target_id.is_none());
        assert!(topology.events.len() <= TOPOLOGY_MAX_EVENTS);
    }

    #[test]
    fn frame_collection_is_bounded_and_preserves_parents() {
        let tree = serde_json::json!({
            "frame": {"id":"root", "url":"https://root.test"},
            "childFrames": [{"frame":{"id":"child", "url":"https://child.test"}}]
        });
        let mut frames = Vec::new();
        collect_frames(&tree, None, Some("child"), &mut frames).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].parent_id.as_deref(), Some("root"));
        assert!(frames[1].active);
    }

    #[test]
    fn keyboard_shortcuts_are_bounded_and_map_modifiers() {
        assert_eq!(
            parse_shortcut("Control+Shift+A").unwrap(),
            (10, "A".to_string())
        );
        assert_eq!(key_code("a"), "KeyA");
        assert!(parse_shortcut("Control+A+B").is_err());
        assert!(validate_key("").is_err());
    }

    #[test]
    fn invalidates_context_only_for_page_or_dom_mutations() {
        assert!(context_event_invalidates_observation(
            "DOM.childNodeInserted"
        ));
        assert!(context_event_invalidates_observation("Page.frameNavigated"));
        assert!(!context_event_invalidates_observation(
            "Network.loadingFinished"
        ));
    }

    #[test]
    fn structured_context_omits_screenshot_until_explicitly_populated() {
        let page = PageInfo {
            url: "https://example.test".to_string(),
            title: "Example".to_string(),
            ready_state: "complete".to_string(),
            target_id: "target-1".to_string(),
            frame_id: "frame-1".to_string(),
        };
        let mut context = PageContext {
            page: page.clone(),
            text: "Example".to_string(),
            dom: None,
            accessibility: CompactAccessibilitySnapshot {
                page,
                revision: 7,
                roots: Vec::new(),
                interactive: Vec::new(),
                truncated: false,
            },
            consistency: ObservationConsistency {
                consistent: true,
                attempts: 1,
                start_revision: 0,
                end_revision: 0,
                start_mutation_revision: 0,
                end_mutation_revision: 0,
            },
            boundaries: ObservationBoundarySummary::default(),
            incomplete: Vec::new(),
            screenshot: None,
        };

        let structured = serde_json::to_value(&context).unwrap();
        assert!(structured.get("dom").is_none());
        assert!(structured.get("screenshot").is_none());
        assert_eq!(structured["accessibility"]["revision"], 7);

        context.screenshot = Some("png-data".to_string());
        let visual = serde_json::to_value(&context).unwrap();
        assert_eq!(visual["screenshot"], "png-data");
    }

    #[test]
    fn revisioned_references_are_parsed_and_validate_their_shape() {
        assert_eq!(
            parse_revisioned_reference("r7:b42").unwrap(),
            Some(RevisionedElementReference {
                revision: 7,
                backend_dom_node_id: 42,
            })
        );
        assert_eq!(parse_revisioned_reference("Save").unwrap(), None);
        assert!(parse_revisioned_reference("r7:b0").is_err());
        assert!(parse_revisioned_reference("r:b42").is_err());
    }

    #[test]
    fn locators_parse_explicit_strategies_without_role_only_fallbacks() {
        assert_eq!(
            Locator::parse("r7:b42").unwrap(),
            Locator::Reference("r7:b42".to_string())
        );
        assert_eq!(
            Locator::parse("name=Save").unwrap(),
            Locator::AccessibleName("Save".to_string())
        );
        assert_eq!(
            Locator::parse("role=button;name=Save").unwrap(),
            Locator::RoleAndName {
                role: "button".to_string(),
                name: "Save".to_string(),
            }
        );
        assert_eq!(Locator::parse("ordinal=2").unwrap(), Locator::Ordinal(2));
        assert_eq!(
            Locator::parse("Save").unwrap(),
            Locator::AccessibleName("Save".to_string())
        );
        assert!(Locator::parse("role=button").is_err());
        assert!(Locator::parse("ordinal=0").is_err());
        assert!(Locator::parse("css=").is_err());
    }

    #[test]
    fn wait_conditions_parse_typed_forms_and_reject_unbounded_values() {
        assert_eq!(
            WaitCondition::parse("lifecycle=load").unwrap(),
            WaitCondition::Lifecycle("complete".to_string())
        );
        assert_eq!(
            WaitCondition::parse("target-visible=name=Save").unwrap(),
            WaitCondition::TargetVisible("name=Save".to_string())
        );
        assert_eq!(
            WaitCondition::parse("network-quiet=250").unwrap(),
            WaitCondition::NetworkQuiet(Duration::from_millis(250))
        );
        assert!(WaitCondition::parse("network-quiet=0").is_err());
        assert!(WaitCondition::parse(&format!("text={}", "x".repeat(4096))).is_err());
        assert!(WaitCondition::parse("lifecycle=forever").is_err());
        assert!(WaitCondition::parse("unknown=value").is_err());
        assert!(validate_wait_deadline(Duration::from_millis(1)).is_ok());
        assert!(validate_wait_deadline(Duration::from_secs(301)).is_err());
    }

    #[test]
    fn ambiguity_candidate_labels_are_utf8_safe_and_bounded() {
        let label = bounded_candidate_label(&"界".repeat(100));
        assert!(label.len() <= CANDIDATE_LABEL_MAX_BYTES);
        assert!(label.ends_with('…'));
        assert!(std::str::from_utf8(label.as_bytes()).is_ok());
    }

    #[test]
    fn action_outcomes_are_compact_and_serializable() {
        let outcome = ActionOutcome {
            action: ActionKind::Click,
            target: Some(ActionTarget {
                label: "button Save".to_string(),
                reference: Some("r9:b42".to_string()),
            }),
            revision: 10,
            target_id: "target-1".to_string(),
            frame_id: "frame-1".to_string(),
            evidence: None,
        };

        let value = serde_json::to_value(outcome).unwrap();
        assert_eq!(value["action"], "click");
        assert_eq!(value["target"]["reference"], "r9:b42");
        assert_eq!(value["revision"], 10);
    }

    #[test]
    fn diagnostics_redact_secrets_and_bound_retention() {
        let redacted = redact_diagnostic_url(
            "https://user:pass@example.test/path?token=secret&empty=#fragment",
        );
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("user"));
        assert!(!redacted.contains("pass"));
        assert!(!redacted.contains("fragment"));
        assert!(redacted.contains("token=%5Bredacted%5D"));

        let headers = serde_json::json!({
            "Authorization": "Bearer secret",
            "Cookie": "session=secret",
            "X-Trace": "safe",
            "Accept": "*/*"
        });
        assert_eq!(safe_header_names(&headers), vec!["Accept", "X-Trace"]);

        let event = CdpEventWithParams {
            method: "Network.requestWillBeSent".to_string(),
            session_id: None,
            params: serde_json::json!({
                "requestId": "request-1",
                "request": {
                    "method": "POST",
                    "url": "https://example.test/api?password=hunter2",
                    "headers": headers,
                    "postData": "never-retain-this"
                }
            }),
        };
        let mut console = Vec::new();
        let mut network = Vec::new();
        let mut indexes = HashMap::new();
        let mut dropped = 0;
        collect_diagnostic_event(
            &event,
            &mut console,
            &mut network,
            &mut indexes,
            &mut dropped,
        );
        let serialized = serde_json::to_string(&network).unwrap();
        assert!(!serialized.contains("hunter2"));
        assert!(!serialized.contains("never-retain-this"));
        assert!(!serialized.contains("Authorization"));
        assert_eq!(network[0].method, "POST");
        assert_eq!(
            redact_diagnostic_text("Authorization: Bearer top-secret"),
            "[redacted sensitive console entry]"
        );
        let console_event = CdpEventWithParams {
            method: "Runtime.consoleAPICalled".to_string(),
            session_id: None,
            params: serde_json::json!({
                "type": "error",
                "args": [{"value": "hunter2"}]
            }),
        };
        for _ in 0..=MAX_DIAGNOSTIC_EVENTS {
            collect_diagnostic_event(
                &console_event,
                &mut console,
                &mut network,
                &mut indexes,
                &mut dropped,
            );
        }
        assert_eq!(console.len(), MAX_DIAGNOSTIC_EVENTS);
        assert_eq!(console[0].text, "[console arguments redacted]");
        assert_eq!(dropped, 1);
    }

    #[test]
    fn visual_capture_validation_uses_effective_viewport_and_scale() {
        let viewport = visual_viewport_rect(&serde_json::json!({
            "pageX": 15.0,
            "pageY": 25.0,
            "clientWidth": 800.0,
            "clientHeight": 600.0
        }))
        .unwrap();
        assert_eq!(viewport.x, 15.0);
        assert_eq!(viewport.y, 25.0);
        assert_eq!(viewport.width, 800.0);
        assert!(validate_effective_visual_clip(Some(viewport), 2.0).is_ok());
        assert!(
            validate_effective_visual_clip(
                Some(VisualClip {
                    x: 0.0,
                    y: 0.0,
                    width: 8_000.0,
                    height: 8_000.0
                }),
                4.0
            )
            .is_err()
        );
        assert_eq!(decoded_base64_len("aGVsbG8=").unwrap(), 5);
        assert!(!visual_clips_match(
            viewport,
            VisualClip {
                x: 16.0,
                ..viewport
            }
        ));
    }

    #[test]
    fn full_snapshot_controls_use_revisioned_backend_references() {
        let roots = vec![AxNode {
            ax_node_id: "button".to_string(),
            backend_dom_node_id: Some(42),
            role: "button".to_string(),
            name: "Save".to_string(),
            description: String::new(),
            value: None,
            children: Vec::new(),
            bounds: None,
            interactive: true,
        }];
        let controls = interactive_elements(&roots, 12);
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].reference, "r12:b42");
        assert_eq!(controls[0].backend_dom_node_id, 42);
    }

    #[test]
    fn compact_text_cap_is_utf8_safe_and_marks_truncation() {
        let text = "🙂".repeat(COMPACT_TEXT_MAX_BYTES);
        let compact = truncate_visible_text(&text, COMPACT_TEXT_MAX_BYTES);

        assert!(compact.len() <= COMPACT_TEXT_MAX_BYTES);
        assert!(compact.ends_with(TEXT_TRUNCATION_MARKER));
        assert!(compact.is_char_boundary(compact.len()));
    }

    #[tokio::test]
    async fn default_observation_is_compact_and_never_requests_deep_dom() {
        let (url, server) = observation_server(false).await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        assert!(context.dom.is_none());
        assert!(context.screenshot.is_none());
        assert!(context.text.contains('😀'));
        assert!(context.text.ends_with(TEXT_TRUNCATION_MARKER));
        assert!(context.text.len() <= COMPACT_TEXT_MAX_BYTES);
        assert!(std::str::from_utf8(context.text.as_bytes()).is_ok());
        assert!(context.consistency.consistent);
        assert_eq!(context.consistency.attempts, 1);
        assert_eq!(context.boundaries.shadow_roots, 1);
        assert_eq!(
            context.incomplete,
            vec![
                ObservationIncompleteReason::VisibleText,
                ObservationIncompleteReason::ShadowBoundary,
                ObservationIncompleteReason::FrameBoundary,
                ObservationIncompleteReason::Canvas,
            ]
        );
        let serialized = serde_json::to_value(&context).unwrap();
        assert!(serialized.get("dom").is_none());
        assert!(serialized.get("screenshot").is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn mutation_race_retries_once_marks_incomplete_and_is_not_cached() {
        let (url, server) = mutation_race_server().await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        assert!(!context.consistency.consistent);
        assert_eq!(context.consistency.attempts, 2);
        assert!(
            context.consistency.end_mutation_revision > context.consistency.start_mutation_revision
        );
        assert!(
            context
                .incomplete
                .contains(&ObservationIncompleteReason::MutationRace)
        );
        assert!(session.observation_cache.lock().await.is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn diagnostic_cancellation_disables_every_scoped_domain() {
        let (url, server) = diagnostic_cleanup_server().await;
        let cdp = CdpClient::connect(&url).await.unwrap();
        let session = test_session(cdp.clone());
        cdp.set_active_target_route(
            Some("test-target".to_string()),
            Some("diagnostic-session".to_string()),
            Some("test-frame".to_string()),
            None,
        );
        assert!(
            tokio::time::timeout(
                Duration::from_millis(25),
                session.diagnostics(Duration::from_secs(5))
            )
            .await
            .is_err()
        );
        let methods = tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        for method in [
            "Network.enable",
            "Runtime.enable",
            "Log.enable",
            "Log.disable",
            "Runtime.disable",
            "Network.disable",
        ] {
            assert!(
                methods.iter().any(|actual| actual == method),
                "missing {method}"
            );
        }
    }

    #[tokio::test]
    async fn deep_dom_observation_is_explicit_and_not_cached() {
        let (url, server) = observation_server(true).await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let deep = session.observe_with_dom().await.unwrap();
        assert_eq!(deep.dom.as_ref().unwrap().node_name, "#document");
        assert!(serde_json::to_value(&deep).unwrap().get("dom").is_some());

        let compact = session.observe().await.unwrap();
        assert!(compact.dom.is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn compact_observation_bounds_accessibility_while_snapshot_remains_full() {
        let (url, server, huge_text) = large_accessibility_server().await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        let serialized = serde_json::to_string(&context).unwrap();
        assert!(context.accessibility.truncated);
        assert_eq!(context.accessibility.revision, 1);
        assert_eq!(context.accessibility.roots[0].role, "RootWebArea");
        assert_eq!(context.accessibility.interactive[0].reference, "r1:b42");
        assert_eq!(context.accessibility.interactive[0].role, "button");
        assert_eq!(context.accessibility.interactive[0].name, "Save");
        assert!(!serialized.contains(&huge_text));
        assert!(
            serialized.len()
                <= COMPACT_TEXT_MAX_BYTES + crate::browser::dom::COMPACT_AX_TEXT_MAX_BYTES + 2_048
        );

        let cached = {
            let cache = session.observation_cache.lock().await;
            cache.as_ref().unwrap().context.clone()
        };
        let cached_json = serde_json::to_string(&cached.into_page_context()).unwrap();
        assert!(!cached_json.contains(&huge_text));

        let snapshot = session.snapshot().await.unwrap();
        assert_eq!(snapshot.roots[0].name, huge_text);
        assert_eq!(snapshot.interactive[0].reference, "r1:b42");
        assert_eq!(snapshot.interactive[0].description.len(), 33 * 1024);

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn hardened_navigation_intercepts_private_redirects_before_following() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let enable = websocket.next().await.unwrap().unwrap();
            let enable: Value = match enable {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected Fetch.enable"),
            };
            assert_eq!(enable["method"], "Fetch.enable");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": enable["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "method": "Fetch.requestPaused",
                        "sessionId": "route-1",
                        "params": {
                            "requestId": "redirect-1",
                            "request": {"url": "http://127.0.0.1/private"}
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let blocked = websocket.next().await.unwrap().unwrap();
            let blocked: Value = match blocked {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected Fetch.failRequest"),
            };
            assert_eq!(blocked["method"], "Fetch.failRequest");
            assert_eq!(blocked["params"]["requestId"], "redirect-1");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": blocked["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let disable = websocket.next().await.unwrap().unwrap();
            let disable: Value = match disable {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected Fetch.disable"),
            };
            assert_eq!(disable["method"], "Fetch.disable");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": disable["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });
        let cdp = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        let policy = BrowserPolicy::hardened(std::env::current_dir().unwrap()).unwrap();
        let interception = PolicyInterception::start(cdp.clone(), policy, "route-1".to_string())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(matches!(
            interception.take_denial().await,
            Some(PolicyError::Denied { .. })
        ));
        interception.shutdown().await;
        cdp.close().await;
        server.await.unwrap();
    }

    fn popup_test_snapshot() -> PopupTopologySnapshot {
        PopupTopologySnapshot {
            original_target_id: "original".to_string(),
            original_frame_id: "frame".to_string(),
            preexisting_target_ids: HashSet::from([
                "original".to_string(),
                "preexisting".to_string(),
            ]),
            sequence: 10,
            event_loss_count: 0,
        }
    }

    fn popup_test_target(id: &str, opener: Option<&str>) -> PageTargetInfo {
        PageTargetInfo {
            id: id.to_string(),
            url: "about:blank".to_string(),
            title: String::new(),
            opener_id: opener.map(str::to_string),
            active: false,
        }
    }

    fn popup_topology_with(targets: Vec<(&str, Option<&str>, u64)>) -> TopologyRegistry {
        let mut topology = TopologyRegistry::default();
        for (id, opener, sequence) in targets {
            topology.targets.push(popup_test_target(id, opener));
            topology.target_sequences.insert(id.to_string(), sequence);
            topology.sequence = topology.sequence.max(sequence);
        }
        topology
    }

    #[test]
    fn popup_topology_accepts_exactly_one_later_live_matching_opener() {
        let topology = popup_topology_with(vec![
            ("original", None, 1),
            ("preexisting", Some("original"), 9),
            ("popup", Some("original"), 11),
        ]);
        let candidate = assess_popup_topology(&popup_test_snapshot(), &topology, true).unwrap();
        assert_eq!(candidate.target.id, "popup");
        assert_eq!(candidate.observed_sequence, 11);
    }

    #[test]
    fn popup_witness_uses_only_isolated_native_event_state() {
        let source = popup_witness_install_function();
        assert!(source.contains("EventTarget.prototype.addEventListener"));
        assert!(source.contains("EventTarget.prototype.removeEventListener"));
        assert!(source.contains("nativeApply(nativeAdd"));
        assert!(source.contains("event.isTrusted === true"));
        assert!(source.contains("event.currentTarget === element"));
        assert!(!source.contains("Runtime.addBinding"));
        assert!(!source.contains("__glass"));
        assert!(!source.contains("element.addEventListener"));
    }

    #[test]
    fn popup_errors_are_bounded_and_serializable() {
        let error = popup_typed_error(PopupClickErrorKind::PopupMissing, "x".repeat(2_000));
        assert!(error.message.len() <= POPUP_ERROR_MESSAGE_MAX_BYTES);
        assert_eq!(
            serde_json::to_value(error).unwrap()["kind"],
            "popup_missing"
        );
    }

    #[test]
    fn popup_timing_evidence_is_explicitly_serializable() {
        let evidence = PopupVerificationEvidence {
            trusted_click_witness: true,
            release_acknowledged: false,
            release_ack_wait_ms: 500.5,
            topology_sequence_before_release: 1,
            popup_observed_sequence: 2,
            attached: true,
            ready_state: "complete".to_string(),
        };
        let value = serde_json::to_value(evidence).unwrap();
        assert_eq!(value["release_ack_wait_ms"], 500.5);
    }

    #[tokio::test]
    async fn cancelled_popup_attach_detaches_a_late_session() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let attach = websocket.next().await.unwrap().unwrap();
            let attach: Value = match attach {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            assert_eq!(attach["method"], "Target.attachToTarget");
            tokio::time::sleep(Duration::from_millis(50)).await;
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": attach["id"],
                        "result": {"sessionId": "late-popup-session"}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            let detach = tokio::time::timeout(Duration::from_secs(1), websocket.next())
                .await
                .expect("late attachment was not detached")
                .unwrap()
                .unwrap();
            let detach: Value = match detach {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text CDP request"),
            };
            assert_eq!(detach["method"], "Target.detachFromTarget");
            assert_eq!(detach["params"]["sessionId"], "late-popup-session");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": detach["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });
        let cdp = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        let session = test_session(cdp.clone());
        {
            let attach = session.attach_popup("popup");
            tokio::pin!(attach);
            tokio::select! {
                _ = &mut attach => panic!("attach unexpectedly completed"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
        server.await.unwrap();
        cdp.close().await;
    }

    #[test]
    fn popup_topology_rejects_missing_witness_preexisting_and_unrelated_targets() {
        let topology = popup_topology_with(vec![("unrelated", None, 11)]);
        assert_eq!(
            assess_popup_topology(&popup_test_snapshot(), &topology, false)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::WitnessMissing
        );
        assert_eq!(
            assess_popup_topology(
                &popup_test_snapshot(),
                &popup_topology_with(vec![("preexisting", Some("original"), 12)]),
                true,
            )
            .unwrap_err()
            .kind,
            PopupClickErrorKind::PopupMissing
        );
        assert_eq!(
            assess_popup_topology(&popup_test_snapshot(), &topology, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupOpenerMismatch
        );
    }

    #[test]
    fn popup_topology_rejects_wrong_opener_ambiguity_lag_and_destroyed_target() {
        let snapshot = popup_test_snapshot();
        let wrong = popup_topology_with(vec![("popup", Some("other"), 11)]);
        assert_eq!(
            assess_popup_topology(&snapshot, &wrong, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupOpenerMismatch
        );
        let ambiguous = popup_topology_with(vec![
            ("popup-1", Some("original"), 11),
            ("popup-2", Some("original"), 12),
        ]);
        assert_eq!(
            assess_popup_topology(&snapshot, &ambiguous, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupAmbiguous
        );
        let mut lagged = popup_topology_with(vec![("popup", Some("original"), 11)]);
        lagged.event_loss_count = 1;
        assert_eq!(
            assess_popup_topology(&snapshot, &lagged, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::TopologyLagged
        );
        let mut destroyed = TopologyRegistry::default();
        destroyed.destroyed_targets.push_back(DestroyedPageTarget {
            target: popup_test_target("popup", Some("original")),
            observed_sequence: 11,
        });
        assert_eq!(
            assess_popup_topology(&snapshot, &destroyed, true)
                .unwrap_err()
                .kind,
            PopupClickErrorKind::PopupDestroyed
        );
    }

    #[tokio::test]
    async fn popup_readiness_maps_protocol_failure_to_typed_unreadable_error() {
        let protocol_error: super::super::cdp::CdpError =
            serde_json::from_value(serde_json::json!({
                "code": -32000,
                "message": "target closed"
            }))
            .unwrap();
        let error = popup_verification_call(async { Err(protocol_error) }, "readiness")
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<PopupClickError>().unwrap().kind,
            PopupClickErrorKind::PopupUnreadable
        );
    }

    #[cfg(feature = "visual-compare")]
    fn comparison_png(width: u32, height: u32, pixels: &[u8]) -> String {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(pixels).unwrap();
        }
        STANDARD.encode(encoded)
    }

    #[cfg(feature = "visual-compare")]
    #[test]
    fn compares_equal_sized_pngs_with_exact_difference_bounds() {
        let first = comparison_png(2, 2, &[0; 16]);
        let mut changed_pixels = [0; 16];
        changed_pixels[4..8].copy_from_slice(&[255, 0, 0, 255]);
        let second = comparison_png(2, 2, &changed_pixels);

        let comparison = compare_png_visuals(&first, &second).unwrap();
        assert_eq!(comparison.changed_pixels, 1);
        assert_eq!(comparison.changed_ratio, 0.25);
        let bounds = comparison.difference_box.unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (1.0, 0.0, 1.0, 1.0)
        );
    }
}
