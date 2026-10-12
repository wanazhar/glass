//! Display-only catch-up. Never sends a request or changes model history.
use crate::session::{Record, Session};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Cached {
    version: u8,
    bytes: u64,
    modified: u128,
    identity: (u64, u64, i64, i64),
    text: String,
}

fn excerpt(text: &str, cap: usize) -> String {
    let prefix: String = text.chars().take(cap * 4).collect();
    let clean = glass_tui::markdown::sanitize(&prefix);
    let flat = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    glass_tui::text::clip(&flat, cap)
}

pub fn build(records: &[Record]) -> Option<String> {
    // The latest request defines scope. Never attribute an older answer to it.
    let index = records.iter().rposition(|r| matches!(r, Record::User(_)))?;
    let Record::User(request) = &records[index] else {
        return None;
    };
    let tail = &records[index + 1..];
    let answer = tail.iter().rev().find_map(|r| match r {
        Record::Assistant(t) if !t.trim().is_empty() => Some(t.as_str()),
        _ => None,
    });
    let failure = tail
        .iter()
        .rev()
        .take_while(|r| !matches!(r, Record::Assistant(t) if !t.trim().is_empty()))
        .find_map(|r| match r {
            Record::Meta(t) if t.starts_with("error:") => Some(t.as_str()),
            _ => None,
        });
    let mut text = format!("Recap · welcome back\n\nRequest: {}", excerpt(request, 180));
    if let Some(answer) = answer {
        text.push_str(&format!(
            "\n\nLatest reply excerpt: {}",
            excerpt(answer, 360)
        ));
    }
    if let Some(failure) = failure {
        text.push_str(&format!("\n\nBlocker: {}\n\nNext: Review the saved error; /retry can continue with completed tool results.", excerpt(failure, 180)));
    } else if answer.is_none() {
        text.push_str("\n\nNext: This request has no saved reply. Send a follow-up to continue.");
    }
    text.push_str("\n\nSaved conversation · 0 extra tokens · /recap to reopen");
    Some(text)
}

pub fn cached(session: &Session) -> Option<String> {
    let metadata = std::fs::metadata(&session.path).ok()?;
    use std::os::unix::fs::MetadataExt;
    let identity = (
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    );
    let bytes = metadata.len();
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let path = session.path.with_extension("recap.json");
    // Bound cache reads; a damaged cache must not affect opening a conversation.
    if std::fs::symlink_metadata(&path)
        .ok()
        .is_some_and(|m| m.is_file() && m.len() < 8192)
        && let Ok(data) = std::fs::read(&path)
        && let Ok(cache) = serde_json::from_slice::<Cached>(&data)
        && cache.version == 1
        && cache.bytes == bytes
        && cache.modified == modified
        && cache.identity == identity
    {
        return Some(cache.text);
    }
    let text = build(&session.history_records())?;
    let cache = Cached {
        version: 1,
        bytes,
        modified,
        identity,
        text: text.clone(),
    };
    // Atomic replacement, private permissions, no model-visible session records.
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temporary = path.with_extension(format!("recap-{}.tmp", std::process::id()));
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
    {
        let result = serde_json::to_vec(&cache)
            .map_err(std::io::Error::other)
            .and_then(|data| file.write_all(&data));
        if result.is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
        let _ = std::fs::remove_file(&temporary);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_and_new_request() {
        assert!(build(&[]).is_none());
        let text = build(&[
            Record::User("old".into()),
            Record::Assistant("old answer".into()),
            Record::User("new".into()),
        ])
        .unwrap();
        assert!(!text.contains("old answer"));
        assert!(text.contains("no saved reply"));
    }
    #[test]
    fn errors_and_unicode_are_bounded() {
        let text = build(&[
            Record::User("你好".repeat(500)),
            Record::Meta("error: provider unavailable".into()),
        ])
        .unwrap();
        assert!(text.contains("Blocker:"));
        assert!(text.contains("0 extra tokens"));
        assert!(text.len() < 1800);
    }
    #[test]
    fn reply_is_an_excerpt_not_a_completion_claim() {
        let text = build(&[
            Record::User("ship".into()),
            Record::Assistant("I plan to investigate".into()),
        ])
        .unwrap();
        assert!(text.contains("Latest reply excerpt: I plan to investigate"));
        assert!(!text.contains("Next:"));
    }
    #[test]
    fn cache_invalidates_and_never_enters_model_history() {
        let dir = std::env::temp_dir().join(format!(
            "gc-recap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("first request".into()));
        let before = session.records.clone();
        let first = cached(&session).unwrap();
        assert_eq!(cached(&session).unwrap(), first);
        assert_eq!(session.records, before);
        session.append(Record::Assistant("saved reply".into()));
        let second = cached(&session).unwrap();
        assert_ne!(first, second);
        assert!(second.contains("saved reply"));
        std::fs::write(session.path.with_extension("recap.json"), "broken cache").unwrap();
        assert_eq!(cached(&session).unwrap(), second);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn recovered_error_is_not_a_current_blocker() {
        let text = build(&[
            Record::User("task".into()),
            Record::Meta("error: old failure".into()),
            Record::Assistant("recovered".into()),
        ])
        .unwrap();
        assert!(!text.contains("Blocker:"));
    }
    #[test]
    fn same_length_replacement_with_preserved_mtime_invalidates() {
        let dir = std::env::temp_dir().join(format!(
            "gc-recap-stamp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("old request".into()));
        assert!(cached(&session).unwrap().contains("old request"));
        let modified = std::fs::metadata(&session.path)
            .unwrap()
            .modified()
            .unwrap();
        let contents = std::fs::read_to_string(&session.path)
            .unwrap()
            .replace("old request", "new request");
        let replacement = session.path.with_extension("replacement");
        std::fs::write(&replacement, contents).unwrap();
        std::fs::File::open(&replacement)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        std::fs::rename(&replacement, &session.path).unwrap();
        let reopened = Session::open_readonly(&dir, &session.id).unwrap();
        assert!(cached(&reopened).unwrap().contains("new request"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
