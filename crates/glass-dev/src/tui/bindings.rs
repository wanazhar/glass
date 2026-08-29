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
        DevSurface::Terminal => {
            "TERMINAL\n  j/k select · Enter logs · Space restart · u App · x stop"
        }
        DevSurface::Git => {
            "GIT\n  Enter/d diff · Space stage · c commit · o open · x discard · r review"
        }
        DevSurface::Tasks => "TASKS\n  j/k todos · Enter complete · Space activate",
        DevSurface::Debug => {
            "DEBUG\n  j/k select · [ ] pane · Enter jump · Space continue · n/i/o step"
        }
        DevSurface::More => "MORE\n  j/k routes · Enter runs doctor/cockpit/kernels",
    };
    format!(
        "DO THIS\n  type in the dock   talk to Glass\n  :                  search commands\n  a                  this surface's actions\n  Enter              do the highlighted thing\n  Esc                back\n  ?                  close help\n\n{surface_lines}\n\nMORE\n  Agent · Code · App · Terminal · Tasks · Git · Debug · More\n\nKEYS\n  Ctrl-L  dock · Alt-A dock from editor\n  Ctrl-P  file · Ctrl-K / Ctrl-Shift-P palette\n  Ctrl-Shift-A  Ask/Plan/Agent · /ask /plan /agent /todo\n  Ctrl-G  App · Tab surfaces · App Alt-Left/Right history\n  editor Ctrl-O symbols · Ctrl-D steer · Ctrl-X abort\n  click dock · double-click open · right-click / long-press = a"
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
