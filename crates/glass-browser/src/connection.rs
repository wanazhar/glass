//! Independent connection-environment and presentation-policy contracts.
//!
//! Width chooses layout only. Transport and graphics claims require separate
//! evidence, and unknown measurements stay unknown.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_CONNECTION_EVIDENCE: usize = 16;
pub const MAX_CONNECTION_EVIDENCE_BYTES: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LayoutClass {
    Phone,
    Compact,
    Wide,
}

impl LayoutClass {
    pub const fn from_columns(columns: u16) -> Self {
        if columns <= 72 {
            Self::Phone
        } else if columns <= 109 {
            Self::Compact
        } else {
            Self::Wide
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportClass {
    Local,
    RemoteFast,
    RemoteConstrained,
    Mosh,
    UnknownRemote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GraphicsClass {
    Kitty,
    Sixel,
    ITermInline,
    Herdr,
    Ansi,
    SemanticOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellKind {
    Local,
    Ssh,
    Mosh,
    UnknownRemote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MultiplexerKind {
    None,
    Tmux,
    Screen,
    Herdr,
    Nested,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivityClass {
    Interactive,
    Settled,
    Idle,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PixelIntent {
    Off,
    Auto,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QualityIntent {
    Auto,
    Data,
    Balanced,
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresentationProfile {
    LocalSmooth,
    LocalBalanced,
    LocalDegraded,
    RemoteInteractive,
    RemoteConstrained,
    MobileRemote,
    MoshSemantic,
    SemanticOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PolicyReason {
    ExplicitPixelsDisabled,
    LocalTransport,
    RemoteTransportMeasuredFast,
    RemoteTransportConstrained,
    RemoteTransportUnknown,
    PhoneRemoteSemanticDefault,
    MoshStateSynchronized,
    GraphicsUnavailable,
    BackgroundPaused,
    IdleThrottled,
    CaptureScaleReduced,
    FrameRateReduced,
    WriterBackpressure,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionMeasurements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_throughput_mbps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_write_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_pixel_width: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_pixel_height: Option<u16>,
}

impl ConnectionMeasurements {
    pub fn validate(self) -> Result<Self, ConnectionError> {
        for (field, value, maximum) in [
            ("rttMs", self.rtt_ms, 120_000.0),
            (
                "estimatedThroughputMbps",
                self.estimated_throughput_mbps,
                1_000_000.0,
            ),
            (
                "terminalWriteLatencyMs",
                self.terminal_write_latency_ms,
                120_000.0,
            ),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0 || value > maximum) {
                return Err(ConnectionError::InvalidMeasurement(field));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectionSignals {
    pub ssh: bool,
    pub mosh: bool,
    pub tmux: bool,
    pub screen: bool,
    pub herdr: bool,
}

impl ConnectionSignals {
    pub fn from_process() -> Self {
        Self {
            ssh: std::env::var_os("SSH_CONNECTION").is_some()
                || std::env::var_os("SSH_TTY").is_some(),
            mosh: std::env::var_os("MOSH_CONNECTION").is_some(),
            tmux: std::env::var_os("TMUX").is_some(),
            screen: std::env::var_os("STY").is_some(),
            herdr: std::env::var("HERDR_ENV").is_ok_and(|value| value == "1"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionOverrides {
    pub layout: Option<LayoutClass>,
    pub transport: Option<TransportClass>,
    pub graphics: Option<GraphicsClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionEnvironment {
    pub layout: LayoutClass,
    pub transport: TransportClass,
    pub graphics: GraphicsClass,
    pub shell: ShellKind,
    pub multiplexer: MultiplexerKind,
    pub terminal_columns: u16,
    pub terminal_rows: u16,
    pub measurements: ConnectionMeasurements,
    pub evidence: Vec<String>,
}

impl ConnectionEnvironment {
    pub fn detect(
        terminal_columns: u16,
        terminal_rows: u16,
        signals: &ConnectionSignals,
        probed_graphics: Option<GraphicsClass>,
        measurements: ConnectionMeasurements,
        overrides: ConnectionOverrides,
    ) -> Result<Self, ConnectionError> {
        if terminal_columns == 0 || terminal_rows == 0 {
            return Err(ConnectionError::InvalidGeometry);
        }
        let measurements = measurements.validate()?;
        let shell = if signals.mosh {
            ShellKind::Mosh
        } else if signals.ssh {
            ShellKind::Ssh
        } else {
            ShellKind::Local
        };
        let transport = overrides.transport.unwrap_or(match shell {
            ShellKind::Local => TransportClass::Local,
            ShellKind::Mosh => TransportClass::Mosh,
            ShellKind::Ssh | ShellKind::UnknownRemote => classify_remote_measurements(measurements),
        });
        let multiplexers = [signals.tmux, signals.screen, signals.herdr]
            .into_iter()
            .filter(|value| *value)
            .count();
        let multiplexer = if multiplexers > 1 {
            MultiplexerKind::Nested
        } else if signals.herdr {
            MultiplexerKind::Herdr
        } else if signals.tmux {
            MultiplexerKind::Tmux
        } else if signals.screen {
            MultiplexerKind::Screen
        } else {
            MultiplexerKind::None
        };
        let graphics = overrides.graphics.unwrap_or_else(|| {
            probed_graphics.unwrap_or(if shell == ShellKind::Local {
                GraphicsClass::Unknown
            } else {
                GraphicsClass::SemanticOnly
            })
        });
        let mut evidence = Vec::new();
        push_evidence(
            &mut evidence,
            match shell {
                ShellKind::Local => "no SSH/Mosh metadata; local shell",
                ShellKind::Ssh => "SSH metadata present; link quality unknown",
                ShellKind::Mosh => "Mosh metadata present; terminal pixels not assumed",
                ShellKind::UnknownRemote => "remote shell metadata incomplete",
            },
        );
        if overrides.layout.is_some() {
            push_evidence(&mut evidence, "layout selected explicitly");
        }
        if overrides.transport.is_some() {
            push_evidence(&mut evidence, "transport selected explicitly");
        }
        if overrides.graphics.is_some() {
            push_evidence(&mut evidence, "graphics selected explicitly");
        } else if probed_graphics.is_some() {
            push_evidence(&mut evidence, "graphics selected by active probe");
        } else {
            push_evidence(&mut evidence, "graphics capability unproven");
        }
        Ok(Self {
            layout: overrides
                .layout
                .unwrap_or_else(|| LayoutClass::from_columns(terminal_columns)),
            transport,
            graphics,
            shell,
            multiplexer,
            terminal_columns,
            terminal_rows,
            measurements,
            evidence,
        })
    }

    pub const fn remote(&self) -> bool {
        !matches!(self.transport, TransportClass::Local)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationPolicy {
    pub profile: PresentationProfile,
    /// Zero means terminal pixel acquisition/presentation is paused.
    pub requested_fps: u16,
    pub active_floor_fps: u16,
    pub capture_scale: f32,
    pub semantic_primary: bool,
    pub continuous_pixels: bool,
    pub drop_obsolete_frames: bool,
    pub reasons: Vec<PolicyReason>,
}

impl PresentationPolicy {
    pub fn select(
        environment: &ConnectionEnvironment,
        activity: ActivityClass,
        pixel_intent: PixelIntent,
        quality: QualityIntent,
    ) -> Self {
        let mut reasons = BTreeSet::new();
        if pixel_intent == PixelIntent::Off {
            reasons.insert(PolicyReason::ExplicitPixelsDisabled);
            return semantic_policy(PresentationProfile::SemanticOnly, reasons);
        }
        if environment.transport == TransportClass::Mosh {
            reasons.insert(PolicyReason::MoshStateSynchronized);
            return semantic_policy(PresentationProfile::MoshSemantic, reasons);
        }
        if matches!(
            environment.graphics,
            GraphicsClass::SemanticOnly | GraphicsClass::Unknown
        ) && pixel_intent == PixelIntent::Auto
        {
            reasons.insert(PolicyReason::GraphicsUnavailable);
            return semantic_policy(PresentationProfile::SemanticOnly, reasons);
        }
        if environment.layout == LayoutClass::Phone
            && environment.remote()
            && pixel_intent == PixelIntent::Auto
        {
            reasons.insert(PolicyReason::PhoneRemoteSemanticDefault);
            return semantic_policy(PresentationProfile::MobileRemote, reasons);
        }

        let (profile, mut requested_fps, floor, scale, semantic_primary) = match environment
            .transport
        {
            TransportClass::Local => {
                reasons.insert(PolicyReason::LocalTransport);
                match quality {
                    QualityIntent::Data => (PresentationProfile::LocalDegraded, 20, 20, 0.5, false),
                    QualityIntent::Balanced | QualityIntent::Auto => {
                        (PresentationProfile::LocalBalanced, 30, 30, 1.0, false)
                    }
                    QualityIntent::Smooth => (PresentationProfile::LocalSmooth, 60, 30, 1.0, false),
                }
            }
            TransportClass::RemoteFast => {
                reasons.insert(PolicyReason::RemoteTransportMeasuredFast);
                let fps = match quality {
                    QualityIntent::Data => 20,
                    QualityIntent::Balanced | QualityIntent::Auto => 24,
                    QualityIntent::Smooth => 30,
                };
                (PresentationProfile::RemoteInteractive, fps, 20, 0.85, false)
            }
            TransportClass::RemoteConstrained | TransportClass::UnknownRemote => {
                reasons.insert(
                    if environment.transport == TransportClass::RemoteConstrained {
                        PolicyReason::RemoteTransportConstrained
                    } else {
                        PolicyReason::RemoteTransportUnknown
                    },
                );
                let fps = match quality {
                    QualityIntent::Data => 3,
                    QualityIntent::Balanced | QualityIntent::Auto => 6,
                    QualityIntent::Smooth => 12,
                };
                (
                    PresentationProfile::RemoteConstrained,
                    fps,
                    3,
                    if quality == QualityIntent::Smooth {
                        0.65
                    } else {
                        0.5
                    },
                    true,
                )
            }
            TransportClass::Mosh => unreachable!("Mosh handled above"),
        };
        let continuous_pixels = match activity {
            ActivityClass::Interactive => true,
            ActivityClass::Settled => {
                requested_fps = requested_fps.min(5);
                reasons.insert(PolicyReason::IdleThrottled);
                false
            }
            ActivityClass::Idle => {
                requested_fps = requested_fps.min(3);
                reasons.insert(PolicyReason::IdleThrottled);
                false
            }
            ActivityClass::Background => {
                requested_fps = 0;
                reasons.insert(PolicyReason::BackgroundPaused);
                false
            }
        };
        Self {
            profile,
            requested_fps,
            active_floor_fps: floor,
            capture_scale: scale,
            semantic_primary,
            continuous_pixels,
            drop_obsolete_frames: true,
            reasons: reasons.into_iter().collect(),
        }
    }
}

fn semantic_policy(
    profile: PresentationProfile,
    reasons: BTreeSet<PolicyReason>,
) -> PresentationPolicy {
    PresentationPolicy {
        profile,
        requested_fps: 0,
        active_floor_fps: 0,
        capture_scale: 0.5,
        semantic_primary: true,
        continuous_pixels: false,
        drop_obsolete_frames: true,
        reasons: reasons.into_iter().collect(),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationObservatory {
    pub requested_fps: f64,
    pub acquisition_fps: f64,
    pub presentation_fps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_age_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_write_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_to_present_ms: Option<f64>,
    pub capture_scale: f32,
    pub encoded_bytes_per_second: f64,
    pub producer_dropped: u64,
    pub mailbox_dropped: u64,
    pub writer_dropped: u64,
    pub stale_rejected: u64,
    pub browser_revision: u64,
    pub geometry_revision: u64,
    pub frame_generation: u64,
}

impl PresentationObservatory {
    pub fn validate(self) -> Result<Self, ConnectionError> {
        for (field, value) in [
            ("requestedFps", Some(self.requested_fps)),
            ("acquisitionFps", Some(self.acquisition_fps)),
            ("presentationFps", Some(self.presentation_fps)),
            ("frameAgeMs", self.frame_age_ms),
            ("captureLatencyMs", self.capture_latency_ms),
            ("encodeLatencyMs", self.encode_latency_ms),
            ("decodeLatencyMs", self.decode_latency_ms),
            ("terminalWriteLatencyMs", self.terminal_write_latency_ms),
            ("inputToPresentMs", self.input_to_present_ms),
            ("encodedBytesPerSecond", Some(self.encoded_bytes_per_second)),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(ConnectionError::InvalidMeasurement(field));
            }
        }
        if !(0.5..=1.0).contains(&self.capture_scale) {
            return Err(ConnectionError::InvalidMeasurement("captureScale"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionError {
    InvalidGeometry,
    InvalidMeasurement(&'static str),
}

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGeometry => formatter.write_str("terminal geometry must be non-zero"),
            Self::InvalidMeasurement(field) => write!(formatter, "invalid {field} measurement"),
        }
    }
}

impl std::error::Error for ConnectionError {}

fn push_evidence(evidence: &mut Vec<String>, value: &str) {
    if evidence.len() < MAX_CONNECTION_EVIDENCE {
        evidence.push(value.chars().take(MAX_CONNECTION_EVIDENCE_BYTES).collect());
    }
}

fn classify_remote_measurements(measurements: ConnectionMeasurements) -> TransportClass {
    if measurements.rtt_ms.is_some_and(|value| value >= 150.0)
        || measurements
            .estimated_throughput_mbps
            .is_some_and(|value| value <= 5.0)
    {
        TransportClass::RemoteConstrained
    } else if measurements.rtt_ms.is_some_and(|value| value <= 80.0)
        && measurements
            .estimated_throughput_mbps
            .is_some_and(|value| value >= 10.0)
    {
        TransportClass::RemoteFast
    } else {
        TransportClass::UnknownRemote
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn environment(
        columns: u16,
        signals: ConnectionSignals,
        graphics: Option<GraphicsClass>,
    ) -> ConnectionEnvironment {
        ConnectionEnvironment::detect(
            columns,
            32,
            &signals,
            graphics,
            ConnectionMeasurements::default(),
            ConnectionOverrides::default(),
        )
        .unwrap()
    }

    #[test]
    fn terminal_width_changes_layout_but_never_transport() {
        let signals = ConnectionSignals {
            ssh: true,
            ..ConnectionSignals::default()
        };
        let phone = environment(60, signals.clone(), None);
        let wide = environment(160, signals, None);
        assert_eq!(phone.layout, LayoutClass::Phone);
        assert_eq!(wide.layout, LayoutClass::Wide);
        assert_eq!(phone.transport, TransportClass::UnknownRemote);
        assert_eq!(wide.transport, TransportClass::UnknownRemote);
    }

    #[test]
    fn remote_matrix_is_conservative_and_mosh_is_semantic() {
        let unknown = environment(
            140,
            ConnectionSignals {
                ssh: true,
                tmux: true,
                ..ConnectionSignals::default()
            },
            None,
        );
        assert_eq!(unknown.graphics, GraphicsClass::SemanticOnly);
        assert_eq!(unknown.multiplexer, MultiplexerKind::Tmux);
        let policy = PresentationPolicy::select(
            &unknown,
            ActivityClass::Interactive,
            PixelIntent::On,
            QualityIntent::Smooth,
        );
        assert_eq!(policy.profile, PresentationProfile::RemoteConstrained);
        assert_eq!(policy.requested_fps, 12);
        let mosh = environment(
            78,
            ConnectionSignals {
                mosh: true,
                ..ConnectionSignals::default()
            },
            Some(GraphicsClass::Kitty),
        );
        let policy = PresentationPolicy::select(
            &mosh,
            ActivityClass::Interactive,
            PixelIntent::On,
            QualityIntent::Smooth,
        );
        assert_eq!(policy.profile, PresentationProfile::MoshSemantic);
        assert_eq!(policy.requested_fps, 0);
    }

    #[test]
    fn local_profiles_restore_sixty_and_thirty_fps() {
        let local = environment(
            140,
            ConnectionSignals::default(),
            Some(GraphicsClass::Kitty),
        );
        let smooth = PresentationPolicy::select(
            &local,
            ActivityClass::Interactive,
            PixelIntent::On,
            QualityIntent::Smooth,
        );
        let balanced = PresentationPolicy::select(
            &local,
            ActivityClass::Interactive,
            PixelIntent::On,
            QualityIntent::Balanced,
        );
        assert_eq!((smooth.requested_fps, smooth.active_floor_fps), (60, 30));
        assert_eq!(balanced.requested_fps, 30);
    }

    #[test]
    fn phone_remote_is_semantic_by_default_but_pixels_can_be_explicit() {
        let phone = environment(
            60,
            ConnectionSignals {
                ssh: true,
                ..ConnectionSignals::default()
            },
            Some(GraphicsClass::Kitty),
        );
        let automatic = PresentationPolicy::select(
            &phone,
            ActivityClass::Interactive,
            PixelIntent::Auto,
            QualityIntent::Smooth,
        );
        let explicit = PresentationPolicy::select(
            &phone,
            ActivityClass::Interactive,
            PixelIntent::On,
            QualityIntent::Smooth,
        );
        assert_eq!(
            (automatic.profile, automatic.requested_fps),
            (PresentationProfile::MobileRemote, 0)
        );
        assert_eq!(
            (explicit.profile, explicit.requested_fps),
            (PresentationProfile::RemoteConstrained, 12)
        );
    }

    #[test]
    fn activity_throttles_without_erasing_active_profile() {
        let local = environment(
            140,
            ConnectionSignals::default(),
            Some(GraphicsClass::Kitty),
        );
        let idle = PresentationPolicy::select(
            &local,
            ActivityClass::Idle,
            PixelIntent::On,
            QualityIntent::Smooth,
        );
        let background = PresentationPolicy::select(
            &local,
            ActivityClass::Background,
            PixelIntent::On,
            QualityIntent::Smooth,
        );
        assert_eq!(
            (idle.profile, idle.requested_fps),
            (PresentationProfile::LocalSmooth, 3)
        );
        assert_eq!(background.requested_fps, 0);
        assert!(background.reasons.contains(&PolicyReason::BackgroundPaused));
    }

    #[test]
    fn explicit_overrides_are_independent_and_explained() {
        let environment = ConnectionEnvironment::detect(
            160,
            40,
            &ConnectionSignals {
                ssh: true,
                ..ConnectionSignals::default()
            },
            None,
            ConnectionMeasurements::default(),
            ConnectionOverrides {
                layout: Some(LayoutClass::Phone),
                transport: Some(TransportClass::RemoteFast),
                graphics: Some(GraphicsClass::Sixel),
            },
        )
        .unwrap();
        assert_eq!(
            (
                environment.layout,
                environment.transport,
                environment.graphics
            ),
            (
                LayoutClass::Phone,
                TransportClass::RemoteFast,
                GraphicsClass::Sixel
            )
        );
        assert_eq!(environment.terminal_columns, 160);
        assert!(
            environment
                .evidence
                .iter()
                .any(|value| value.contains("transport selected explicitly"))
        );
    }

    #[test]
    fn issue_33_phone_design_assets_decode_with_real_dimensions() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for (name, minimum_width, minimum_height) in [
            ("remote-ios-concept.jpg", 390, 800),
            ("remote-android-concept.jpg", 200, 400),
        ] {
            let path = root.join(name);
            let size = imagesize::size(&path).unwrap_or_else(|error| {
                panic!(
                    "{} must be a decodable release design asset: {error}",
                    path.display()
                )
            });
            assert!(size.width >= minimum_width && size.height >= minimum_height);
        }
    }
}
