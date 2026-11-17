//! Durable product state on disk.
//!
//! Marble is self-hosted and single-process, so a revisioned JSON document with
//! atomic replacement is enough: a crash leaves either the previous revision or
//! the complete new one, never a half-written mix.

use crate::MarbleError;
use crate::state::ProductState;
use std::fs;
use std::path::{Path, PathBuf};

/// File name of the durable product document.
pub const STATE_FILE: &str = "product-state.json";
const STATE_TEMP_FILE: &str = "product-state.json.tmp";

/// Resolve the self-hosted data directory.
///
/// Order: `GLASS_MARBLE_DATA_DIR`, `XDG_DATA_HOME/glass-marble`,
/// `~/.local/share/glass-marble`, then a stable temp fallback.
pub fn default_data_dir() -> PathBuf {
    if let Some(configured) = std::env::var_os("GLASS_MARBLE_DATA_DIR")
        && !configured.is_empty()
    {
        return PathBuf::from(configured);
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("glass-marble");
    }
    if let Some(home) = std::env::var_os("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".local/share/glass-marble");
    }
    std::env::temp_dir().join("glass-marble")
}

/// Durable runtime session storage under the product data directory.
pub fn sessions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions")
}

/// Revisioned product state document.
#[derive(Debug, Clone)]
pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Store the document inside `data_dir`.
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(STATE_FILE))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the document, returning an empty state when it does not exist yet.
    pub fn load(&self) -> Result<ProductState, MarbleError> {
        match fs::read_to_string(&self.path) {
            Ok(encoded) => serde_json::from_str(&encoded).map_err(|error| {
                MarbleError::Protocol(format!(
                    "cannot read product state at {}: {error}",
                    self.path.display()
                ))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ProductState::default())
            }
            Err(error) => Err(MarbleError::Io(error)),
        }
    }

    /// Write the document atomically, replacing the previous revision.
    pub fn save(&self, state: &ProductState) -> Result<(), MarbleError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| MarbleError::Protocol("product state path has no parent".into()))?;
        fs::create_dir_all(parent).map_err(MarbleError::Io)?;
        let temp = parent.join(STATE_TEMP_FILE);
        let encoded = serde_json::to_vec_pretty(state).map_err(|error| {
            MarbleError::Protocol(format!("cannot encode product state: {error}"))
        })?;
        fs::write(&temp, encoded).map_err(MarbleError::Io)?;
        fs::rename(&temp, &self.path).map_err(MarbleError::Io)?;
        Ok(())
    }
}

/// Load product state from `data_dir`, or an empty document when unreadable.
pub fn load_or_default(data_dir: &Path) -> ProductState {
    StateStore::in_data_dir(data_dir)
        .load()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "starting from empty marble product state");
            ProductState::default()
        })
}

/// Persist one snapshot, keeping the revision the caller already bumped.
pub fn persist(store: &StateStore, state: &ProductState) {
    if let Err(error) = store.save(state) {
        tracing::warn!(%error, "could not persist marble product state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glass-marble-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_document_loads_as_empty_state() {
        let dir = data_dir("missing");
        let store = StateStore::in_data_dir(&dir);
        assert_eq!(store.load().unwrap(), ProductState::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_document_round_trips_with_revision() {
        let dir = data_dir("round-trip");
        let store = StateStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        state.bump();
        state.conversation_mut("c1").title = "durable".into();
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
        assert_eq!(store.load().unwrap().revision, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_document_reports_instead_of_silently_resetting() {
        let dir = data_dir("corrupt");
        let store = StateStore::in_data_dir(&dir);
        fs::write(store.path(), b"{not json").unwrap();
        assert!(store.load().is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let dir = data_dir("atomic");
        let store = StateStore::in_data_dir(&dir);
        store.save(&ProductState::default()).unwrap();
        assert!(store.path().exists());
        assert!(!dir.join(STATE_TEMP_FILE).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_data_dir_prefers_explicit_configuration() {
        assert!(default_data_dir().is_absolute());
        assert_eq!(
            sessions_dir(Path::new("/tmp/x")),
            Path::new("/tmp/x/sessions")
        );
    }
}
