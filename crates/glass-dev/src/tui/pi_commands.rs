//! Pi's user-facing slash-command catalog.
//!
//! The runtime also accepts commands registered by loaded Pi extensions. This
//! catalog is the stable built-in surface exposed by the pinned SDK and is
//! used to make the Agent command modal useful before a command is typed.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PiCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub argument_hint: Option<&'static str>,
}

pub(crate) const BUILTIN_PI_COMMANDS: &[PiCommand] = &[
    PiCommand {
        name: "settings",
        description: "Open settings menu",
        argument_hint: None,
    },
    PiCommand {
        name: "model",
        description: "Select model (opens selector UI)",
        argument_hint: Some("<provider/model>"),
    },
    PiCommand {
        name: "tree",
        description: "Navigate session tree (switch branches)",
        argument_hint: None,
    },
    PiCommand {
        name: "thinking",
        description: "Set thinking level",
        argument_hint: Some("<level>"),
    },
    PiCommand {
        name: "scoped-models",
        description: "Enable/disable models for Ctrl+P cycling",
        argument_hint: None,
    },
    PiCommand {
        name: "export",
        description: "Export session (HTML default, or .html/.jsonl)",
        argument_hint: Some("[path]"),
    },
    PiCommand {
        name: "import",
        description: "Import and resume a JSONL session",
        argument_hint: Some("<path>"),
    },
    PiCommand {
        name: "share",
        description: "Share session as a secret GitHub gist",
        argument_hint: None,
    },
    PiCommand {
        name: "copy",
        description: "Copy the last agent message to the clipboard",
        argument_hint: None,
    },
    PiCommand {
        name: "name",
        description: "Set session display name",
        argument_hint: Some("[name]"),
    },
    PiCommand {
        name: "session",
        description: "Show session info and stats",
        argument_hint: None,
    },
    PiCommand {
        name: "changelog",
        description: "Show Pi changelog entries",
        argument_hint: None,
    },
    PiCommand {
        name: "hotkeys",
        description: "Show keyboard shortcuts",
        argument_hint: None,
    },
    PiCommand {
        name: "fork",
        description: "Create a fork from a previous user message",
        argument_hint: Some("[entry-id]"),
    },
    PiCommand {
        name: "clone",
        description: "Duplicate the current session at its position",
        argument_hint: None,
    },
    PiCommand {
        name: "trust",
        description: "Save the project trust decision",
        argument_hint: Some("[yes|no|reset]"),
    },
    PiCommand {
        name: "login",
        description: "Configure provider authentication",
        argument_hint: Some("[provider]"),
    },
    PiCommand {
        name: "logout",
        description: "Remove provider authentication",
        argument_hint: Some("[provider]"),
    },
    PiCommand {
        name: "new",
        description: "Start a new session",
        argument_hint: None,
    },
    PiCommand {
        name: "compact",
        description: "Manually compact the session context",
        argument_hint: Some("[instructions]"),
    },
    PiCommand {
        name: "resume",
        description: "Resume a different session",
        argument_hint: Some("[session-path]"),
    },
    PiCommand {
        name: "reload",
        description: "Reload Pi resources and extension commands",
        argument_hint: None,
    },
    PiCommand {
        name: "quit",
        description: "Quit Glass Dev",
        argument_hint: None,
    },
];

pub(crate) fn is_builtin(name: &str) -> bool {
    BUILTIN_PI_COMMANDS
        .iter()
        .any(|command| command.name == name)
}

#[cfg(test)]
mod tests {
    use super::{BUILTIN_PI_COMMANDS, is_builtin};

    #[test]
    fn catalog_matches_the_pinned_pi_builtin_surface() {
        let expected = [
            "settings",
            "model",
            "tree",
            "thinking",
            "scoped-models",
            "export",
            "import",
            "share",
            "copy",
            "name",
            "session",
            "changelog",
            "hotkeys",
            "fork",
            "clone",
            "trust",
            "login",
            "logout",
            "new",
            "compact",
            "resume",
            "reload",
            "quit",
        ];
        let actual = BUILTIN_PI_COMMANDS
            .iter()
            .map(|command| command.name)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(actual.iter().all(|name| is_builtin(name)));
    }
}
