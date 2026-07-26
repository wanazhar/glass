pub mod cdp;
pub mod chrome;
pub mod dom;
pub mod mouse;
pub mod policy;
pub mod profile;
pub mod session;

// Re-export key session types to the browser module level.
pub use session::{
    AccessibilityDiff, BrowserResult, BrowserSession, Cookie, DiffChange, DiffElement,
    FillFieldResult, FillFormOutcome, GeoLocation, InteractionMode, InterceptGuard, NetworkEntry,
    NetworkRecorder, NetworkRecording, PdfOptions, PopupClickOutcome, RequestPattern, RetryPolicy,
    RetryPredicate, StorageEntry, StorageItems, WebAuthnGuard, WebAuthnOptions, diff_accessibility,
};
