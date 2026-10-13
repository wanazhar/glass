//! Durable per-attempt retry lineage.
//!
//! `provider::stream_with_retry` counts attempts in a local variable that is
//! gone by the time anyone asks what happened, so the only durable trace of a
//! retry is an aggregate counter on the request report. This writes one line
//! per retry — attempt number, the backoff actually applied, the caller-safe
//! cause — next to the session log, so the inspector can show the lineage
//! instead of "3 retries".
//!
//! Nothing here is inferred. An attempt is recorded when the retry loop asks
//! for it; a delay is the value the loop slept on; a request id is present
//! only when the provider put one in the text it returned.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::Path;

/// One retry attempt, as the retry loop reported it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attempt {
    /// 1-based retry number: the first re-send is attempt 1.
    pub attempt: u8,
    /// Backoff in seconds the loop applied before this attempt.
    pub delay_s: u64,
    /// Caller-safe failure cause, already sanitized by the provider error type.
    pub cause: String,
    /// Session log length when the retry was scheduled.
    ///
    /// This is what lets the inspector attach a retry to the failure it belongs
    /// to: an error record at index N owns every attempt recorded at an index
    /// in `(previous error index, N]`. It is a durable coordinate, not a clock
    /// reading, so it survives a resumed session.
    pub record_index: usize,
    /// Wall-clock milliseconds at the moment this line was written.
    pub recorded_at_ms: u128,
    /// Provider request/response id, when the provider reported one.
    ///
    /// `None` means the provider never sent an id we could read. It is never
    /// filled in from the endpoint, the model, or a counter.
    #[serde(default)]
    pub request_id: Option<String>,
}

/// Where the attempt lineage lives next to a session log.
pub fn path_for(session_path: &Path) -> std::path::PathBuf {
    session_path.with_extension("attempts.jsonl")
}

/// Append one attempt. Diagnostics only: a failure here never fails a turn.
pub fn append(
    session_path: &Path,
    attempt: u8,
    delay_s: u64,
    cause: &str,
    record_index: usize,
    request_id: Option<String>,
) -> Result<(), String> {
    let recorded_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let record = Attempt {
        attempt,
        delay_s,
        cause: crate::live::sanitize_provider_detail(cause),
        record_index,
        recorded_at_ms,
        request_id,
    };
    let path = path_for(session_path);
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(&path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Attempt log is not a regular file".into());
    }
    let mut bytes = serde_json::to_vec(&record).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    file.write_all(&bytes).map_err(|e| e.to_string())
}

/// Recent attempt records, newest last.
///
/// Returns every line the bounded tail could hold; the caller decides how many
/// to show. A malformed line is skipped rather than failing the whole read,
/// because a torn tail must not hide the lineage that was written before it.
#[cfg(test)]
pub fn records(session_path: &Path) -> Result<Vec<Attempt>, String> {
    Ok(crate::requests::tail(&path_for(session_path))?
        .into_iter()
        .filter_map(|value| serde_json::from_value::<Attempt>(value).ok())
        .collect())
}

/// Read attempts, reporting whether the bounded tail may have cut the lineage.
///
/// `complete == false` means the tail read hit its cap and older attempts may
/// exist. The caller shows that as "lineage may be truncated" rather than
/// presenting a short list as the whole history.
pub fn window(session_path: &Path) -> Result<(Vec<Attempt>, bool), String> {
    let (rows, complete): (Vec<Value>, bool) = crate::requests::tail_window(&path_for(session_path))?;
    Ok((
        rows.into_iter()
            .filter_map(|value| serde_json::from_value::<Attempt>(value).ok())
            .collect(),
        complete,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_retry_is_written_with_its_attempt_number_delay_and_cause() {
        let dir = std::env::temp_dir().join(format!("gc-attempts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("session.jsonl");
        std::fs::write(&log, "").unwrap();
        append(&log, 1, 1, "http 503: overloaded", 7, None).unwrap();
        append(&log, 2, 2, "http 503: overloaded", 7, None).unwrap();
        let (rows, complete) = window(&log).unwrap();
        assert!(complete);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter().map(|r| r.attempt).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(rows.iter().map(|r| r.delay_s).collect::<Vec<_>>(), vec![1, 2]);
        assert!(rows.iter().all(|r| r.cause.contains("503")));
        // The durable coordinate, not a clock reading, is what groups a retry
        // with the failure it belongs to.
        assert!(rows.iter().all(|r| r.record_index == 7));
        assert!(rows.iter().all(|r| r.request_id.is_none()));
        assert!(rows.iter().all(|r| r.recorded_at_ms > 0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_absent_request_id_stays_absent_rather_than_being_invented() {
        let dir = std::env::temp_dir().join(format!("gc-attempts-id-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("session.jsonl");
        std::fs::write(&log, "").unwrap();
        append(&log, 1, 0, "transport: connection reset", 1, None).unwrap();
        append(&log, 1, 0, "transport: connection reset", 1, Some("resp_abc".into())).unwrap();
        let rows = records(&log).unwrap();
        assert_eq!(rows[0].request_id, None);
        assert_eq!(rows[1].request_id.as_deref(), Some("resp_abc"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_torn_tail_does_not_hide_the_lineage_written_before_it() {
        let dir = std::env::temp_dir().join(format!("gc-attempts-torn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("session.jsonl");
        std::fs::write(&log, "").unwrap();
        append(&log, 1, 1, "http 429: slow down", 3, None).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(path_for(&log))
            .unwrap();
        file.write_all(b"{\"attempt\":2,\"delay").unwrap();
        let rows = records(&log).unwrap();
        assert_eq!(rows.len(), 1, "the torn line is skipped, not fatal");
        assert_eq!(rows[0].attempt, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recorded_causes_are_sanitized_before_they_reach_disk() {
        let dir = std::env::temp_dir().join(format!("gc-attempts-safe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("session.jsonl");
        std::fs::write(&log, "").unwrap();
        append(
            &log,
            1,
            0,
            "Authorization: Bearer sk-live-SENTINEL at https://endpoint.invalid/v1",
            0,
            None,
        )
        .unwrap();
        let text = std::fs::read_to_string(path_for(&log)).unwrap();
        assert!(!text.contains("sk-live-SENTINEL"), "{text}");
        assert!(!text.contains("endpoint.invalid"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}