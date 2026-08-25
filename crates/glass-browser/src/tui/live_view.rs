//! Live visual plane for Glass TUIs.
//!
//! Wraps the browser-owned presentation pipeline with the policy the TUIs
//! need: Herdr when the environment provides it, then explicit Kitty output
//! or the true-color ANSI half-block renderer from
//! [`crate::terminal_graphics`]. Latest frame wins; nothing queues
//! unboundedly.

use crate::cli::args::{TuiLiveBackend, TuiLiveFit, TuiLiveMode, TuiLiveQuality};
use crate::terminal_graphics::{AnsiCanvas, FrameFit};

/// Decided visual policy for the current terminal and CLI flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualPath {
    /// Continuous frames through Herdr's owned pane graphics.
    Herdr,
    /// Continuous frames through the Kitty terminal graphics protocol.
    Kitty,
    /// Continuous frames through the ANSI half-block renderer.
    Ansi,
    /// No continuous pixels; semantic evidence only.
    SemanticOnly { reason: String },
}

pub fn decide_path(
    mode: TuiLiveMode,
    backend: TuiLiveBackend,
    herdr_available: bool,
) -> VisualPath {
    match (mode, backend) {
        (TuiLiveMode::Off, _) => VisualPath::SemanticOnly {
            reason: "live pixels are disabled (--tui-live off)".into(),
        },
        (_, TuiLiveBackend::Herdr) if herdr_available => VisualPath::Herdr,
        (_, TuiLiveBackend::Herdr) => VisualPath::SemanticOnly {
            reason: "Herdr pane graphics were requested but the environment is unavailable".into(),
        },
        (_, TuiLiveBackend::Kitty) => VisualPath::Kitty,
        (_, TuiLiveBackend::Ansi) => VisualPath::Ansi,
        (TuiLiveMode::Auto, TuiLiveBackend::Auto) if herdr_available => VisualPath::Herdr,
        (TuiLiveMode::Auto, TuiLiveBackend::Auto) => VisualPath::SemanticOnly {
            reason: "no native graphics backend detected; use --tui-live on for ANSI".into(),
        },
        (TuiLiveMode::On, TuiLiveBackend::Auto) => VisualPath::Ansi,
    }
}

/// Frame interval for a quality profile, in milliseconds.
pub fn frame_interval_ms(quality: TuiLiveQuality) -> u64 {
    match quality {
        TuiLiveQuality::Data => 333,
        TuiLiveQuality::Balanced => 160,
        TuiLiveQuality::Smooth => 80,
    }
}

/// Pane cell size for a quality profile.
pub fn pane_size(quality: TuiLiveQuality, available: (u16, u16)) -> (u16, u16) {
    let budget = match quality {
        TuiLiveQuality::Data => 40,
        TuiLiveQuality::Balanced => 80,
        TuiLiveQuality::Smooth => 120,
    };
    (
        available.0.min(budget).max(8),
        available.1.min(budget / 2).max(4),
    )
}

pub fn frame_fit(fit: TuiLiveFit) -> FrameFit {
    match fit {
        TuiLiveFit::Contain => FrameFit::Contain,
        TuiLiveFit::Cover => FrameFit::Cover,
        TuiLiveFit::Actual => FrameFit::Actual,
    }
}

/// Cell payload for one rendered ANSI frame: each cell is one half-block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiPane {
    pub columns: u16,
    pub rows: u16,
    /// `rows` rows of `columns` cells, each `top`/`bottom` RGB pairs.
    pub cells: Vec<crate::terminal_graphics::AnsiCell>,
}

impl AnsiPane {
    pub fn from_png(
        canvas: &mut AnsiCanvas,
        png: &[u8],
        columns: u16,
        rows: u16,
        fit: FrameFit,
    ) -> Result<Self, crate::terminal_graphics::GraphicsError> {
        canvas.update_png(png, columns, rows, fit)?;
        Ok(Self {
            columns,
            rows,
            cells: canvas.cells().to_vec(),
        })
    }
}

/// Read dimensions from a PNG header without retaining decoded pixels.
pub fn png_dimensions(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes(png[16..20].try_into().ok()?),
        u32::from_be_bytes(png[20..24].try_into().ok()?),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_never_enables_pixels_and_on_falls_back_to_ansi() {
        assert!(matches!(
            decide_path(TuiLiveMode::Off, TuiLiveBackend::Auto, true),
            VisualPath::SemanticOnly { .. }
        ));
        assert_eq!(
            decide_path(TuiLiveMode::On, TuiLiveBackend::Auto, false),
            VisualPath::Ansi
        );
        assert_eq!(
            decide_path(TuiLiveMode::On, TuiLiveBackend::Auto, true),
            VisualPath::Ansi
        );
    }

    #[test]
    fn auto_requires_a_native_backend_and_herdr_is_preferred() {
        assert!(matches!(
            decide_path(TuiLiveMode::Auto, TuiLiveBackend::Auto, false),
            VisualPath::SemanticOnly { .. }
        ));
        assert_eq!(
            decide_path(TuiLiveMode::Auto, TuiLiveBackend::Auto, true),
            VisualPath::Herdr
        );
    }

    #[test]
    fn explicit_backends_are_honored() {
        assert_eq!(
            decide_path(TuiLiveMode::On, TuiLiveBackend::Herdr, true),
            VisualPath::Herdr
        );
        assert!(matches!(
            decide_path(TuiLiveMode::On, TuiLiveBackend::Herdr, false),
            VisualPath::SemanticOnly { .. }
        ));
        assert_eq!(
            decide_path(TuiLiveMode::On, TuiLiveBackend::Kitty, false),
            VisualPath::Kitty
        );
        assert_eq!(
            decide_path(TuiLiveMode::Auto, TuiLiveBackend::Kitty, false),
            VisualPath::Kitty
        );
        assert_eq!(
            decide_path(TuiLiveMode::Auto, TuiLiveBackend::Ansi, false),
            VisualPath::Ansi
        );
    }

    #[test]
    fn quality_profiles_bound_interval_and_pane() {
        assert_eq!(frame_interval_ms(TuiLiveQuality::Data), 333);
        assert_eq!(frame_interval_ms(TuiLiveQuality::Smooth), 80);
        assert_eq!(pane_size(TuiLiveQuality::Data, (200, 60)), (40, 20));
        assert_eq!(pane_size(TuiLiveQuality::Smooth, (60, 20)), (60, 20));
    }
}
