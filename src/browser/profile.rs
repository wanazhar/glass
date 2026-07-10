use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info};

/// A browser session profile with cookies and settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub cookies: Vec<serde_json::Value>,
    pub local_storage: Option<serde_json::Value>,
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

/// Manages browser session persistence (cookies, local storage).
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

    /// Get the path for a specific profile's data file.
    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{name}.json"))
    }

    /// Create a new profile.
    pub fn create_profile(&self, name: &str) -> Result<Profile, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&self.profiles_dir)?;

        let profile = Profile::new(name);
        let json = serde_json::to_string_pretty(&profile)?;
        std::fs::write(self.profile_path(name), json)?;

        info!("Created profile: {name}");
        Ok(profile)
    }

    /// Load an existing profile.
    pub fn load_profile(&self, name: &str) -> Result<Profile, Box<dyn std::error::Error>> {
        let path = self.profile_path(name);
        if !path.exists() {
            return self.create_profile(name);
        }

        let json = std::fs::read_to_string(&path)?;
        let profile: Profile = serde_json::from_str(&json)?;
        debug!("Loaded profile: {name} ({} cookies)", profile.cookies.len());
        Ok(profile)
    }

    /// Save a profile to disk.
    pub fn save_profile(&self, profile: &Profile) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&self.profiles_dir)?;

        let mut profile = profile.clone();
        profile.updated_at = chrono::Utc::now().to_rfc3339();

        let json = serde_json::to_string_pretty(&profile)?;
        std::fs::write(self.profile_path(&profile.name), json)?;

        debug!("Saved profile: {}", profile.name);
        Ok(())
    }

    /// List all saved profiles.
    pub fn list_profiles(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        if !self.profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    profiles.push(name.to_string());
                }
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    /// Delete a profile.
    pub fn delete_profile(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = self.profile_path(name);
        if path.exists() {
            std::fs::remove_file(&path)?;
            info!("Deleted profile: {name}");
        }
        Ok(())
    }

    /// Get the Chrome user-data-dir for a profile.
    pub fn chrome_data_dir(&self, name: &str) -> PathBuf {
        self.profiles_dir.join(format!("{name}_chrome"))
    }

    /// Sync cookies from CDP to the profile.
    pub fn sync_cookies_from_cdp(
        &self,
        profile: &mut Profile,
        cdp_cookies: &serde_json::Value,
    ) {
        if let Some(cookies) = cdp_cookies["cookies"].as_array() {
            profile.cookies = cookies.clone();
            debug!("Synced {} cookies to profile", cookies.len());
        }
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Incognito session state (no persistence).
pub struct IncognitoSession {
    pub cookies: Vec<serde_json::Value>,
}

impl IncognitoSession {
    pub fn new() -> Self {
        Self {
            cookies: Vec::new(),
        }
    }
}

impl Default for IncognitoSession {
    fn default() -> Self {
        Self::new()
    }
}
