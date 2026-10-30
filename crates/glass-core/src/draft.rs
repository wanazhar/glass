//! Private composer drafts survive an explicit quit and resume without becoming
//! conversation records. Human-requested secret input uses a separate transient
//! channel and is never written here.

use std::{fs::OpenOptions, io::Read, path::PathBuf};

const MAX_DRAFT_BYTES: usize = 128 * 1024;

fn path(session: &crate::session::Session) -> PathBuf {
    session.path.with_extension("draft")
}

pub fn load(session: &crate::session::Session) -> Result<Option<String>, String> {
    let path = path(session);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read saved composer draft: {error}")),
    };
    let metadata = file
        .metadata()
        .map_err(|error| format!("Cannot inspect saved composer draft: {error}"))?;
    if !metadata.is_file() {
        return Err("Saved composer draft is not a regular file".into());
    }
    if metadata.len() > MAX_DRAFT_BYTES as u64 {
        return Err("Saved composer draft exceeds the 128 KiB limit".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 || metadata.permissions().mode() & 0o077 != 0 {
            return Err("Saved composer draft must be private and have no hard links".into());
        }
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take((MAX_DRAFT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read saved composer draft: {error}"))?;
    if bytes.len() > MAX_DRAFT_BYTES {
        return Err("Saved composer draft exceeds the 128 KiB limit".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "Saved composer draft is not valid UTF-8".into())
}

pub fn save(session: &crate::session::Session, text: &str) -> Result<(), String> {
    if text.len() > MAX_DRAFT_BYTES {
        return Err(
            "Composer draft exceeds the 128 KiB save limit; copy it before quitting".into(),
        );
    }
    let path = path(session);
    if text.is_empty() {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Cannot clear saved composer draft: {error}")),
        }
    } else {
        crate::auth::secure_write(&path, text.as_bytes())
    }
}

pub fn clear(session: &crate::session::Session) -> Result<(), String> {
    save(session, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn private_draft_round_trips_without_entering_conversation_history() {
        let dir = std::env::temp_dir().join(format!(
            "glass-core-draft-{}-{}",
            std::process::id(),
            crate::auth::random(8).unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let session = crate::session::Session::create(&dir).unwrap();
        let text = "unsent prompt with line two";

        save(&session, text).unwrap();

        assert_eq!(load(&session).unwrap().as_deref(), Some(text));
        assert!(!session.records.iter().any(|record| matches!(
            record,
            crate::session::Record::User(value) if value == text
        )));
        assert_eq!(
            std::fs::metadata(path(&session))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        clear(&session).unwrap();
        assert_eq!(load(&session).unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
