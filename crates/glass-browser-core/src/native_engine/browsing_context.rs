use super::origin::NativeOrigin;

/// Stable context ID for the first native-engine browsing context.
pub const NATIVE_CONTEXT_ID: &str = "native-context";

/// Internal context projection owned by the native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBrowsingContext {
    pub context_id: String,
    pub url: String,
    pub origin: NativeOrigin,
    pub active: bool,
}
