//! Resource bounds shared by browser adapters and the native engine.

/// Maximum UTF-8 byte length for a backend or context identifier.
pub const MAX_BACKEND_ID_BYTES: usize = 128;
/// Maximum UTF-8 byte length for a backend version string.
pub const MAX_VERSION_BYTES: usize = 128;
/// Maximum UTF-8 byte length for a browser-family name.
pub const MAX_BROWSER_FAMILY_BYTES: usize = 64;
/// Maximum number of capabilities in one backend profile.
pub const MAX_CAPABILITIES: usize = 32;
/// Maximum number of dependencies in one capability declaration.
pub const MAX_DEPENDENCIES: usize = 16;
/// Maximum number of limitations in one backend profile.
pub const MAX_LIMITATIONS: usize = 16;
/// Maximum UTF-8 byte length of one capability limitation.
pub const MAX_LIMITATION_BYTES: usize = 256;
/// Maximum number of selection requirements in one request.
pub const MAX_SELECTION_REQUIREMENTS: usize = 32;
/// Maximum number of candidate backend identifiers in one request.
pub const MAX_BACKEND_CANDIDATES: usize = 32;
/// Maximum UTF-8 byte length of a backend diagnostic string.
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;
/// Maximum number of browsing contexts in one response.
pub const MAX_CONTEXTS: usize = 64;
/// Maximum number of entries in one storage map.
pub const MAX_STORAGE_ENTRIES: usize = 128;
/// Maximum number of download identifiers in one response.
pub const MAX_DOWNLOADS: usize = 64;
/// Maximum number of bytes in one encoded capture payload.
pub const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of bytes in one serialized JSON payload.
pub const MAX_JSON_BYTES: usize = 64 * 1024;
/// Maximum nesting depth accepted by the bounded JSON visitor.
pub const MAX_JSON_DEPTH: usize = 16;
/// Maximum UTF-8 byte length for general text payloads.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
/// Maximum number of page targets represented in a topology snapshot.
pub const TOPOLOGY_MAX_TARGETS: usize = 32;
/// Maximum number of frames represented in a topology snapshot.
pub const TOPOLOGY_MAX_FRAMES: usize = 128;
