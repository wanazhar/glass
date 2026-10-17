//! Resolve GC after an installer has replaced the running executable.
use std::path::{Path, PathBuf};

pub fn path() -> Result<PathBuf, String> {
    let current =
        std::env::current_exe().map_err(|e| format!("Cannot locate GC executable: {e}"))?;
    replacement(&current)
        .or_else(|| {
            // Linux retains access to the running inode even if no replacement exists.
            #[cfg(target_os = "linux")]
            {
                // A PID path also remains meaningful when GC_BIN is inherited by a shell.
                let running = PathBuf::from(format!("/proc/{}/exe", std::process::id()));
                running.is_file().then_some(running)
            }
            #[cfg(not(target_os = "linux"))]
            {
                None
            }
        })
        .ok_or_else(|| "GC executable is unavailable; reinstall GC and restart the gateway".into())
}

fn replacement(current: &Path) -> Option<PathBuf> {
    if current.is_file() {
        return Some(current.to_path_buf());
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        if let Some(original) = current.as_os_str().as_bytes().strip_suffix(b" (deleted)") {
            let original = Path::new(std::ffi::OsStr::from_bytes(original));
            if original.is_file() {
                return Some(original.to_path_buf());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_handles_deleted_images_without_rewriting_existing_paths() {
        let root =
            std::env::temp_dir().join(format!("gc-executable-{}", crate::auth::random(8).unwrap()));
        std::fs::create_dir_all(&root).unwrap();
        let installed = root.join("glass-core");
        std::fs::write(&installed, b"installed").unwrap();
        assert_eq!(replacement(&installed), Some(installed.clone()));
        #[cfg(target_os = "linux")]
        {
            let deleted = root.join("glass-core (deleted)");
            assert_eq!(replacement(&deleted), Some(installed));
            std::fs::write(&deleted, b"literal filename").unwrap();
            assert_eq!(replacement(&deleted), Some(deleted));
        }
        assert!(replacement(&root.join("missing")).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
