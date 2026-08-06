//! Chrome user-data directory profile management.
//!
//! Manages named profiles backed by Chrome user-data directories. Profiles
//! persist cookies, localStorage, and all browser state across sessions.

use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use tracing::info;

/// Manages Chrome user-data directories used as named Glass profiles.
///
/// Chrome owns cookies, local storage, and all other persisted browser state.
/// Glass intentionally does not maintain a second JSON representation of that
/// state: the directory passed to `--user-data-dir` is the sole source of
/// truth.
pub struct ProfileManager {
    profiles_dir: PathBuf,
}

/// Process-held ownership of a profile by one named workspace.
#[derive(Debug)]
pub struct ProfileLock {
    _file: File,
    pub profile: String,
    pub workspace: String,
}

impl ProfileManager {
    pub fn new() -> Self {
        let profiles_dir = std::env::var_os("GLASS_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(dirs::config_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("glass")
            .join("profiles");
        Self { profiles_dir }
    }

    pub fn validate_name(name: &str) -> Result<(), Box<dyn std::error::Error>> {
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.len() > 64
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
        {
            return Err(
                "profile names must be 1-64 characters of A-Z, a-z, 0-9, '-' or '_'".into(),
            );
        }
        Ok(())
    }

    /// Return the canonical Chrome user-data directory for a profile.
    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.profile_data_root().join(name)
    }

    /// Create (or return) a named profile's Chrome user-data directory.
    ///
    /// Older Glass versions stored Chrome data in `<name>_chrome` alongside a
    /// JSON metadata file. Move that existing browser data into the canonical
    /// directory before use so Chrome remains the authority for persistence.
    pub fn create_profile(&self, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Self::validate_name(name)?;
        std::fs::create_dir_all(self.profile_data_root())?;

        let profile_dir = self.profile_dir(name);
        let legacy_chrome_dir = self.legacy_chrome_data_dir(name);
        if !profile_dir.exists() && legacy_chrome_dir.is_dir() {
            std::fs::rename(&legacy_chrome_dir, &profile_dir)?;
            info!(%name, "migrated legacy Chrome profile directory");
        }
        std::fs::create_dir_all(&profile_dir)?;
        info!(%name, path = %profile_dir.display(), "ensured Chrome profile directory");
        Ok(profile_dir)
    }

    /// Ensure a named profile exists and return its Chrome user-data directory.
    pub fn ensure_profile_dir(&self, name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        self.create_profile(name)
    }

    pub fn list_profiles(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut profiles = Vec::new();
        let profile_data_root = self.profile_data_root();
        if profile_data_root.exists() {
            for entry in std::fs::read_dir(profile_data_root)? {
                let entry = entry?;
                if entry.path().is_dir()
                    && let Some(name) = entry.file_name().to_str()
                    && Self::validate_name(name).is_ok()
                {
                    profiles.push(name.to_string());
                }
            }
        }

        // Older releases stored profile data and metadata directly under the
        // profiles root. Include those names until they are migrated on use.
        if self.profiles_dir.exists() {
            for entry in std::fs::read_dir(&self.profiles_dir)? {
                let entry = entry?;
                let path = entry.path();
                let file_name = entry.file_name();
                let Some(file_name) = file_name.to_str() else {
                    continue;
                };

                let name = if path.is_dir() {
                    file_name.strip_suffix("_chrome").map(str::to_string)
                } else if path.extension().and_then(|extension| extension.to_str()) == Some("json")
                {
                    Some(file_name.trim_end_matches(".json").to_string())
                } else {
                    None
                };

                if let Some(name) = name
                    && Self::validate_name(&name).is_ok()
                {
                    profiles.push(name);
                }
            }
        }
        profiles.sort();
        profiles.dedup();
        Ok(profiles)
    }

    /// Delete all persisted state for a named profile.
    ///
    /// The canonical Chrome user-data directory is removed, as are the legacy
    /// browser directory and JSON metadata left by older Glass releases.
    pub fn delete_profile(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Self::validate_name(name)?;

        remove_dir_if_exists(&self.profile_dir(name))?;
        remove_dir_if_exists(&self.legacy_chrome_data_dir(name))?;

        let legacy_metadata = self.legacy_metadata_path(name);
        if legacy_metadata.exists() {
            std::fs::remove_file(legacy_metadata)?;
        }
        info!(%name, "deleted persisted Chrome profile data");
        Ok(())
    }
    /// Acquire an exclusive process lock tying this profile to one workspace.
    /// The guard must remain alive for the duration of browser ownership.
    pub fn lock_profile(&self, profile: &str, workspace: &str) -> Result<ProfileLock, Box<dyn std::error::Error>> {
        Self::validate_name(profile)?;
        Self::validate_name(workspace)?;
        let profile_dir = self.create_profile(profile)?;
        let lock_path = profile_dir.join(".glass-workspace.lock");
        let file = OpenOptions::new().create(true).read(true).write(true).open(lock_path)?;
        file.try_lock_exclusive().map_err(|error| {
            format!("profile {profile} is already owned by another workspace: {error}")
        })?;
        Ok(ProfileLock { _file: file, profile: profile.to_owned(), workspace: workspace.to_owned() })
    }

    fn legacy_chrome_data_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{name}_chrome"))
    }

    fn profile_data_root(&self) -> PathBuf {
        self.profiles_dir.join("data")
    }

    fn legacy_metadata_path(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{name}.json"))
    }

    #[cfg(test)]
    fn with_profiles_dir(profiles_dir: PathBuf) -> Self {
        Self { profiles_dir }
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn test_directory() -> PathBuf {
        let id = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("glass-profile-test-{}-{id}", std::process::id()))
    }

    #[test]
    fn validates_profile_names() {
        assert!(ProfileManager::validate_name("work").is_ok());
        assert!(ProfileManager::validate_name("personal_2026").is_ok());
        assert!(ProfileManager::validate_name("../escape").is_err());
        assert!(ProfileManager::validate_name("with space").is_err());
    }

    #[test]
    fn profiles_are_chrome_data_directories_and_delete_removes_legacy_data() {
        let profiles_dir = test_directory();
        let manager = ProfileManager::with_profiles_dir(profiles_dir.clone());
        let profile_dir = manager.create_profile("work").unwrap();
        std::fs::write(profile_dir.join("Cookies"), "persisted-by-chrome").unwrap();
        std::fs::create_dir_all(manager.legacy_chrome_data_dir("work")).unwrap();
        std::fs::write(
            manager.legacy_chrome_data_dir("work").join("History"),
            "legacy",
        )
        .unwrap();
        std::fs::write(manager.legacy_metadata_path("work"), "{}\n").unwrap();

        assert_eq!(manager.list_profiles().unwrap(), vec!["work"]);
        manager.delete_profile("work").unwrap();

        assert!(!profile_dir.exists());
        assert!(!manager.legacy_chrome_data_dir("work").exists());
        assert!(!manager.legacy_metadata_path("work").exists());
        assert!(manager.list_profiles().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(profiles_dir);
    }

    #[test]
    fn create_profile_migrates_legacy_chrome_data() {
        let profiles_dir = test_directory();
        let manager = ProfileManager::with_profiles_dir(profiles_dir.clone());
        std::fs::create_dir_all(manager.legacy_chrome_data_dir("work")).unwrap();
        std::fs::write(
            manager.legacy_chrome_data_dir("work").join("Cookies"),
            "legacy",
        )
        .unwrap();

        let profile_dir = manager.create_profile("work").unwrap();

        assert!(profile_dir.join("Cookies").exists());
        assert!(!manager.legacy_chrome_data_dir("work").exists());
        let _ = std::fs::remove_dir_all(profiles_dir);
    }

    #[test]
    fn profile_names_ending_in_chrome_are_not_confused_with_legacy_directories() {
        let profiles_dir = test_directory();
        let manager = ProfileManager::with_profiles_dir(profiles_dir.clone());

        manager.create_profile("work_chrome").unwrap();

        assert_eq!(manager.list_profiles().unwrap(), vec!["work_chrome"]);
        let _ = std::fs::remove_dir_all(profiles_dir);
    }
}
