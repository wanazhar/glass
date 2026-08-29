//! The small keymap we teach. Everything else is an alias of these verbs.

use super::state::DevSurface;

/// Product verbs advertised in help and the unfocused dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum WorkspaceVerb {
    Primary,
    Back,
    Palette,
    Actions,
    FocusDock,
    OpenFile,
    Help,
    NextSurface,
    Quit,
}

impl WorkspaceVerb {
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Enter",
            Self::Back => "Esc",
            Self::Palette => ":",
            Self::Actions => "a",
            Self::FocusDock => "dock",
            Self::OpenFile => "Ctrl-P",
            Self::Help => "?",
            Self::NextSurface => "Tab",
            Self::Quit => "Ctrl-C",
        }
    }
}

pub fn curriculum_help(surface: DevSurface) -> String {
    let surface_lines = match surface {
        DevSurface::Trust => "TRUST\n  I inspect · 1 once · T project · O untrusted",
        DevSurface::Agent => {
            "AGENT\n  type in the dock to talk · Enter sends\n  :plan accept after a Plan turn"
        }
        DevSurface::Code => "CODE\n  Enter opens · i inserts · gc comments",
        DevSurface::App => "APP\n  Enter activates · g source · C comment",
        DevSurface::Terminal => "TERMINAL\n  s start suite · u attach URL",
        DevSurface::Git => "GIT\n  Enter/d diff · :git commit",
        DevSurface::Tasks => "TASKS\n  a actions · :task create",
        DevSurface::Debug => "DEBUG\n  a actions · :debug threads",
        DevSurface::More => "MORE\n  :doctor · :cockpit start",
    };
    format!(
        "DO THIS\n  type in the dock   talk to Glass\n  :                  search commands\n  a                  this surface's actions\n  Enter              do the highlighted thing\n  Esc                back\n  ?                  close help\n\n{surface_lines}\n\nMORE\n  Ctrl-P file · Tab next surface
  AGENT · CODE · APP · TERMINAL · TASKS · GIT · DEBUG · MORE"
    )
}

pub fn dock_placeholder(surface: DevSurface, mode: crate::AgentTurnMode) -> String {
    format!(
        "{}  Ask Glass about {}…    : search · a actions",
        mode.label().to_ascii_uppercase(),
        surface.label()
    )
}

#[allow(dead_code)]
pub fn unfocused_footer_hint() -> &'static str {
    "Enter · : search · a actions · click dock to talk"
}
