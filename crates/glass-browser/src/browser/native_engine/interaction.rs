use super::dom::NativeNodeId;

/// Maximum native event records retained for diagnostic/effect inspection.
pub const MAX_NATIVE_EFFECTS: usize = 256;

/// Semantic actions understood by the first native interaction slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeAction {
    Click { target: String },
    Type { target: String, text: String },
    Scroll { delta_x: i32, delta_y: i32 },
}

/// Native event kinds retained by the single-owner document coordinator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeEventKind {
    Blur,
    Focus,
    Click,
    Input,
    Change,
    Scroll,
}

/// Bounded native effect metadata. It never contains raw input or form values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeEffect {
    pub revision: u64,
    pub node_id: NativeNodeId,
    pub kind: NativeEventKind,
}
