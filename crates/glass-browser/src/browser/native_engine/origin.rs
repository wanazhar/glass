/// Origin state carried by a Phase 1 native document.
///
/// Local fixture and data documents intentionally use an opaque placeholder.
/// A tuple-origin model belongs to the later resource/security workstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeOrigin {
    Opaque,
}

impl NativeOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
        }
    }
}
