use super::dom::NativeNodeId;
use super::error::NativeEngineError;
use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Maximum native event records retained for diagnostic/effect inspection.
pub const MAX_NATIVE_EFFECTS: usize = 256;
pub(crate) const MAX_NATIVE_KEY_BYTES: usize = 64;
/// Maximum files accepted by one native file-input action.
pub const MAX_NATIVE_FILE_COUNT: usize = 16;
/// Maximum bytes retained by one native file object.
pub const MAX_NATIVE_FILE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum bytes retained by all files in one native file-input action.
pub const MAX_NATIVE_FILE_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_NATIVE_FILE_NAME_BYTES: usize = 1024;
const MAX_NATIVE_FILE_TYPE_BYTES: usize = 128;

mod base64_bytes {
    use super::*;

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}

/// A bounded native file object. File contents are copied into the native
/// engine so no filesystem path crosses the page or content-process boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFile {
    pub name: String,
    #[serde(rename = "type")]
    pub media_type: String,
    pub last_modified: u64,
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
}

impl NativeFile {
    /// Read one authorized regular file into a bounded native file object.
    pub fn from_path(path: &Path) -> Result<Self, NativeEngineError> {
        let metadata = std::fs::metadata(path).map_err(|error| NativeEngineError::Worker {
            operation: "read native upload file metadata".into(),
            reason: error.to_string(),
        })?;
        if !metadata.is_file() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "native upload path must be a regular file".into(),
            });
        }
        let size = usize::try_from(metadata.len()).map_err(|_| {
            NativeEngineError::limit("native upload file", MAX_NATIVE_FILE_BYTES, usize::MAX)
        })?;
        if size > MAX_NATIVE_FILE_BYTES {
            return Err(NativeEngineError::limit(
                "native upload file",
                MAX_NATIVE_FILE_BYTES,
                size,
            ));
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| NativeEngineError::invalid("native upload file name", "must be UTF-8"))?
            .to_owned();
        let mut file = File::open(path).map_err(|error| NativeEngineError::Worker {
            operation: "read native upload file".into(),
            reason: error.to_string(),
        })?;
        let mut bytes = Vec::with_capacity(size);
        file.read_to_end(&mut bytes)
            .map_err(|error| NativeEngineError::Worker {
                operation: "read native upload file".into(),
                reason: error.to_string(),
            })?;
        let last_modified = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
            .unwrap_or(0);
        let file = Self {
            name,
            media_type: media_type_for_name(path),
            last_modified,
            bytes,
        };
        file.validate()?;
        Ok(file)
    }

    pub fn validate(&self) -> Result<(), NativeEngineError> {
        if self.name.is_empty()
            || self.name.len() > MAX_NATIVE_FILE_NAME_BYTES
            || self.name.chars().any(char::is_control)
            || self.name.contains(['/', '\\'])
        {
            return Err(NativeEngineError::invalid(
                "native upload file name",
                "must be a bounded printable leaf name",
            ));
        }
        if self.media_type.len() > MAX_NATIVE_FILE_TYPE_BYTES
            || self
                .media_type
                .bytes()
                .any(|byte| !(0x20..=0x7e).contains(&byte))
        {
            return Err(NativeEngineError::invalid(
                "native upload media type",
                "must be a bounded printable value",
            ));
        }
        if self.bytes.len() > MAX_NATIVE_FILE_BYTES {
            return Err(NativeEngineError::limit(
                "native upload file",
                MAX_NATIVE_FILE_BYTES,
                self.bytes.len(),
            ));
        }
        Ok(())
    }

    pub fn validate_many(files: &[Self]) -> Result<(), NativeEngineError> {
        if files.is_empty() || files.len() > MAX_NATIVE_FILE_COUNT {
            return Err(NativeEngineError::invalid(
                "native upload files",
                "must contain 1..=16 files",
            ));
        }
        let mut total = 0usize;
        for file in files {
            file.validate()?;
            total = total.checked_add(file.bytes.len()).ok_or_else(|| {
                NativeEngineError::limit(
                    "native upload files",
                    MAX_NATIVE_FILE_TOTAL_BYTES,
                    usize::MAX,
                )
            })?;
            if total > MAX_NATIVE_FILE_TOTAL_BYTES {
                return Err(NativeEngineError::limit(
                    "native upload files",
                    MAX_NATIVE_FILE_TOTAL_BYTES,
                    total,
                ));
            }
        }
        Ok(())
    }
}

fn media_type_for_name(path: &Path) -> String {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return "application/octet-stream".into();
    };
    match extension.to_ascii_lowercase().as_str() {
        "css" => "text/css",
        "csv" => "text/csv",
        "gif" => "image/gif",
        "htm" | "html" => "text/html",
        "jpeg" | "jpg" => "image/jpeg",
        "js" => "text/javascript",
        "json" => "application/json",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "txt" => "text/plain",
        "wasm" => "application/wasm",
        "webp" => "image/webp",
        "xml" => "application/xml",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .into()
}

/// Semantic actions understood by the bounded native interaction layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeAction {
    Click {
        target: String,
    },
    DoubleClick {
        target: String,
    },
    Hover {
        target: String,
    },
    Drag {
        source: String,
        destination: String,
    },
    Upload {
        target: String,
        files: Vec<NativeFile>,
    },
    Type {
        target: String,
        text: String,
    },
    Clear {
        target: String,
    },
    Check {
        target: String,
    },
    Uncheck {
        target: String,
    },
    Select {
        target: String,
        value: String,
    },
    KeyDown {
        key: String,
    },
    KeyUp {
        key: String,
    },
    Shortcut {
        shortcut: String,
    },
    KeyPress {
        key: String,
    },
    Scroll {
        delta_x: i32,
        delta_y: i32,
    },
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
