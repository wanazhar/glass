//! Resource bounds shared by browser adapters and the native engine.

pub const MAX_BACKEND_ID_BYTES: usize = 128;
pub const MAX_VERSION_BYTES: usize = 128;
pub const MAX_BROWSER_FAMILY_BYTES: usize = 64;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_DEPENDENCIES: usize = 16;
pub const MAX_LIMITATIONS: usize = 16;
pub const MAX_LIMITATION_BYTES: usize = 256;
pub const MAX_SELECTION_REQUIREMENTS: usize = 32;
pub const MAX_BACKEND_CANDIDATES: usize = 32;
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;
pub const MAX_CONTEXTS: usize = 64;
pub const MAX_STORAGE_ENTRIES: usize = 128;
pub const MAX_DOWNLOADS: usize = 64;
pub const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_JSON_BYTES: usize = 64 * 1024;
pub const MAX_JSON_DEPTH: usize = 16;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const TOPOLOGY_MAX_TARGETS: usize = 32;
pub const TOPOLOGY_MAX_FRAMES: usize = 128;
