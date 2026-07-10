use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::{debug, info};

/// A browser session profile with cookies and settings metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub cookies: Vec<Value>,
    pub local_storage: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

impl Profile {
    pub fn new(name: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            name: name.to_string(),
            cookies: Vec::new(),
            local_storage: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// Manages browser session persistence.
pub struct ProfileManager {
    profiles_dir: PathBuf,
}

impl ProfileManager {
    pub fn new() -> Self {
        let profiles_dir = dirs::config_dir()
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

    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{name}.json"))
    }

    pub fn create_profile(&self, name: &str) -> Result<Profile, Box<dyn std::error::Error>> {
        Self::validate_name(name)?;
        std::fs::create_dir_all(&self.profiles_dir)?;
        let profile = Profile::new(name);
        self.write_profile(&profile)?;
        info!(%name, "created profile");
        Ok(profile)
    }

    pub fn load_profile(&self, name: &str) -> Result<Profile, Box<dyn std::error::Error>> {
        Self::validate_name(name)?;
        let path = self.profile_path(name);
        if !path.exists() {
            return self.create_profile(name);
        }
        let profile: Profile = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        debug!(%name, cookies = profile.cookies.len(), "loaded profile");
        Ok(profile)
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<(), Box<dyn std::error::Error>> {
        Self::validate_name(&profile.name)?;
        self.write_profile(profile)
    }

    fn write_profile(&self, profile: &Profile) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&self.profiles_dir)?;
        let mut profile = profile.clone();
        profile.updated_at = chrono::Utc::now().to_rfc3339();
        let path = self.profile_path(&profile.name);
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, serde_json::to_vec_pretty(&profile)?)?;
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    pub fn list_profiles(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        if !self.profiles_dir.exists() {
            return Ok(Vec::new());
        }
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&self.profiles_dir)? {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
            {
                profiles.push(name.to_string());
            }
        }
        profiles.sort();
        Ok(profiles)
    }

    pub fn delete_profile(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        Self::validate_name(name)?;
        let path = self.profile_path(name);
        if path.exists() {
            std::fs::remove_file(path)?;
            info!(%name, "deleted profile");
        }
        Ok(())
    }

    pub fn chrome_data_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{name}_chrome"))
    }

    pub fn sync_cookies_from_cdp(&self, profile: &mut Profile, cdp_cookies: &Value) {
        if let Some(cookies) = cdp_cookies["cookies"].as_array() {
            profile.cookies = cookies.clone();
            debug!(count = cookies.len(), "synced cookies to profile");
        }
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}
