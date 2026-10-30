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
        assert_eq!(inventory.schema_version, 1);
        for line in &inventory.help_lines {
            assert!(!line.keys.is_empty());
            for key in &line.keys {
                assert!(line.text.contains(key));
            }
        }
        // Keep the binding inventory as the single source of truth while
        // presenting only the daily shortcuts in the compact help panel.
        inventory
            .help_lines
            .iter()
            .filter(|line| {
                line.keys
                    .iter()
                    .any(|key| matches!(key.as_str(), "Ctrl-L" | "Ctrl-P"))
            })
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub fn curriculum_help(surface: DevSurface) -> String {
    let surface_lines = match surface {
        DevSurface::Agent => {
            "AGENT\n  Type a message · Enter sends · PgUp/PgDn reads history\n  Ctrl-Shift-T threads · Ctrl-F fork · Ctrl-R rewind · Ctrl-E edit last\n  Ctrl-O expand selected tool · Ctrl-Shift-O inspect full output"
        }
        DevSurface::Code => {
            "CODE\n  ↑/↓ preview · Enter opens · Ctrl-O symbols · Ctrl-L ask with this buffer\n  Ctrl-Shift-A attach a range · Ctrl-Shift-X toggle file context"
        }
        DevSurface::App => "BROWSER\n  / browser actions · g source · Alt-Left/Right history",
        DevSurface::Terminal => {
            "TERMINAL\n  j/k select · Enter logs · Space restart · u browser · x stop"
        }
        DevSurface::Git => {
            "GIT\n  Enter diff · Space stage · c commit · o open · x discard · r review"
        }
        DevSurface::Tasks => "TASKS\n  j/k todos · Enter complete · Space activate",
        DevSurface::Debug => {
            "DEBUG\n  j/k select · [ ] pane · Enter jump · Space continue · n/i/o step"
        }
        DevSurface::More => "MORE\n  j/k or click routes · Enter/double-click runs · / actions",
    };
    format!(
        "START\n  Type to talk · Enter sends\n  / Core + Glass commands · Ctrl-K workspace actions · ? shortcuts\n\nMOVE AROUND\n  Ctrl-Tab next surface · Ctrl-Shift-Tab previous\n  Agent · Code · Browser · Git · Terminal · Tasks · Debug\n\n{surface_lines}\n\nMORE\n{}",
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
