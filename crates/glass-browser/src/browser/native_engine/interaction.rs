use super::dom::NativeNodeId;

/// Maximum native event records retained for diagnostic/effect inspection.
pub const MAX_NATIVE_EFFECTS: usize = 256;
pub(crate) const MAX_NATIVE_KEY_BYTES: usize = 64;

/// Semantic actions understood by the bounded native interaction layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeAction {
    Click { target: String },
    DoubleClick { target: String },
    Hover { target: String },
    Drag { source: String, destination: String },
    Type { target: String, text: String },
    Clear { target: String },
    Check { target: String },
    Uncheck { target: String },
    Select { target: String, value: String },
    KeyDown { key: String },
    KeyUp { key: String },
    Shortcut { shortcut: String },
    KeyPress { key: String },
    Scroll { delta_x: i32, delta_y: i32 },
}

/// Native event kinds retained by the single-owner document coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeEventKind {
    Blur,
    Focus,
    ReadyStateChange,
    DomContentLoaded,
    Load,
    Error,
    PageHide,
    Unload,
    PageShow,
    BeforeUnload,
    HashChange,
    PopState,
    Invalid,
    KeyDown,
    KeyUp,
    Submit,
    Click,
    MouseOver,
    MouseEnter,
    DragStart,
    DragEnter,
    DragOver,
    Drop,
    DragEnd,
    Input,
    Change,
    Scroll,
}

pub(crate) fn validate_native_key(key: &str) -> Result<(), super::error::NativeEngineError> {
    if key.is_empty() || key.len() > MAX_NATIVE_KEY_BYTES || key.chars().any(char::is_control) {
        return Err(super::error::NativeEngineError::invalid(
            "action key",
            "must be 1..=64 printable UTF-8 bytes",
        ));
    }
    Ok(())
}

pub(crate) fn validate_native_edit_key(key: &str) -> Result<(), super::error::NativeEngineError> {
    validate_native_key(key)?;
    if matches!(key, "Backspace" | "Delete") || key.chars().count() == 1 {
        return Ok(());
    }
    Err(super::error::NativeEngineError::TargetNotActionable {
        reason: "native key press supports printable keys, Backspace, and Delete".into(),
    })
}

/// Parse the bounded shortcut vocabulary shared by the native action owner.
/// The bit values intentionally match the CDP modifier mask used by the
/// existing Chromium session adapter: Alt=1, Control=2, Meta=4, Shift=8.
pub(crate) fn parse_native_shortcut(
    value: &str,
) -> Result<(i64, String), super::error::NativeEngineError> {
    if value.is_empty() || value.len() > 256 {
        return Err(super::error::NativeEngineError::invalid(
            "shortcut",
            "must be 1..=256 bytes",
        ));
    }
    let mut modifiers = 0;
    let mut key = None;
    for part in value.split('+') {
        if part.is_empty() {
            return Err(super::error::NativeEngineError::invalid(
                "shortcut",
                "must contain one non-empty key",
            ));
        }
        match part.to_ascii_lowercase().as_str() {
            "alt" => modifiers |= 1,
            "control" | "ctrl" => modifiers |= 2,
            "meta" | "cmd" | "command" => modifiers |= 4,
            "shift" => modifiers |= 8,
            _ if key.is_none() => key = Some(part.to_owned()),
            _ => {
                return Err(super::error::NativeEngineError::invalid(
                    "shortcut",
                    "must contain exactly one non-modifier key",
                ));
            }
        }
    }
    let key = key.ok_or_else(|| {
        super::error::NativeEngineError::invalid(
            "shortcut",
            "must contain exactly one non-modifier key",
        )
    })?;
    validate_native_key(&key)?;
    Ok((modifiers, key))
}

/// Bounded native effect metadata. It never contains raw input or form values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeEffect {
    pub revision: u64,
    pub node_id: NativeNodeId,
    pub kind: NativeEventKind,
}
