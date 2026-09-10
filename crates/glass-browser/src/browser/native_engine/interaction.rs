use super::dom::NativeNodeId;

/// Maximum native event records retained for diagnostic/effect inspection.
pub const MAX_NATIVE_EFFECTS: usize = 256;
pub(crate) const MAX_NATIVE_KEY_BYTES: usize = 64;

/// Semantic actions understood by the bounded native interaction layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeAction {
    Click { target: String },
    Type { target: String, text: String },
    Clear { target: String },
    Check { target: String },
    Uncheck { target: String },
    Select { target: String, value: String },
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

/// Bounded native effect metadata. It never contains raw input or form values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeEffect {
    pub revision: u64,
    pub node_id: NativeNodeId,
    pub kind: NativeEventKind,
}
