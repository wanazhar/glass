//! The small keymap we teach. Everything else is an alias of these verbs.

use super::state::DevSurface;
use serde::Deserialize;
use std::sync::OnceLock;

const SHORTCUT_INVENTORY: &str = include_str!("shortcuts.json");

#[derive(Debug, Deserialize)]
struct ShortcutInventory {
    schema_version: u32,
    help_lines: Vec<ShortcutHelpLine>,
}

#[derive(Debug, Deserialize)]
struct ShortcutHelpLine {
    keys: Vec<String>,
    text: String,
}

fn documented_shortcut_help() -> &'static str {
    static HELP: OnceLock<String> = OnceLock::new();
    HELP.get_or_init(|| {
        let inventory: ShortcutInventory =
            serde_json::from_str(SHORTCUT_INVENTORY).expect("invalid TUI shortcut inventory");
        assert_eq!(
            inventory.schema_version, 1,
            "unsupported TUI shortcut inventory schema"
        );
        for line in &inventory.help_lines {
            assert!(!line.keys.is_empty(), "TUI shortcut help line has no keys");
            for key in &line.keys {
                assert!(
                    line.text.contains(key),
                    "TUI shortcut help line omits its key {key:?}"
                );
            }
        }
        inventory
            .help_lines
            .into_iter()
            .map(|line| format!("  {}", line.text))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub fn curriculum_help(surface: DevSurface) -> String {
    let surface_lines = match surface {
        DevSurface::Agent => {
            "AGENT · COMPOSER READY\n  type to talk · Enter sends · / commands\n  Ctrl-Tab surfaces · Ctrl-Shift-T project threads\n  Ctrl-K Glass actions · Ctrl-F fork · Ctrl-R rewind\n  Ctrl-E edit last · Ctrl-O expand transcript"
        }
        DevSurface::Code => {
            "CODE\n  ↑/↓ preview · Enter opens · Ctrl-L ask with this buffer · Ctrl-Shift-X remove context"
        }
        DevSurface::App => {
            "BROWSER / PREVIEW\n  connected page, native-first · / browser actions · g source"
        }
        DevSurface::Terminal => {
            "TERMINAL\n  j/k select · Enter logs · Space restart · u Browser · x stop"
        }
        DevSurface::Git => {
            "GIT\n  Enter/d diff · Space stage · c commit · o open · x discard · r review"
        }
        DevSurface::Tasks => "TASKS\n  j/k todos · Enter complete · Space activate",
        DevSurface::Debug => {
            "DEBUG\n  j/k select · [ ] pane · Enter jump · Space continue · n/i/o step"
        }
        DevSurface::More => "MORE\n  j/k or click routes · Enter/double-click runs · / actions",
    };
    format!(
        "DO THIS\n  type in Agent      composer is ready immediately\n  /                  Core commands and Glass workspace actions\n  Ctrl-Tab            next surface · Ctrl-Shift-Tab previous\n  Alt-1…5             direct primary surface when terminal supports Alt\n  Ctrl-K              Glass action palette\n  Esc                 dismiss menu / return to surface\n  ?                   close help\n\n{surface_lines}\n\nWORKSPACE\n  Agent · Code · Browser · Terminal · Git\n  Tasks, Debug, and services are available in / actions\n\nKEYS\n{}\n  click surface tabs · double-click open · right-click / long-press context actions",
        documented_shortcut_help()
    )
}

pub fn dock_placeholder(surface: DevSurface) -> String {
    let command_hint = if surface == DevSurface::Agent {
        "/ commands · Ctrl-Tab surfaces"
    } else {
        "Ctrl-K actions · Ctrl-Tab surfaces"
    };
    format!("Message Glass about {}…    {command_hint}", surface.label())
}
