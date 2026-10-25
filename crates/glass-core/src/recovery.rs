//! Failure recovery inspector.
//!
//! Turns the records a failed turn already left on disk into a list a user can
//! act on: the `error:` records in the session log, the per-retry lineage in
//! the attempt log, and the bounded request reports.
//!
//! Everything here is a reading. The inspector starts no work, re-sends
//! nothing and adds no startup cost. Where the runtime did not record
//! something, the field says so rather than carrying a plausible value: an
//! absent provider request id stays absent, a timestamp that was never written
//! stays null, and a lineage the bounded read may have cut is marked
//! truncated instead of being presented as the whole history.
//!
//! One rule is enforced rather than documented. The recommendation attached to
//! a failure never claims that a completed mutation will be replayed, and the
//! type has no way to say so: the only constructor takes a failure class and a
//! measured mutation state, and it always reports the replay answer as no.

use crate::attempt_log::Attempt;
use crate::session::Session;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Failures listed, newest first. The bound is stated in the output.
pub const MAX_FAILURES: usize = 32;

/// Attempts shown for one failure. `attempt_count` stays the true number.
pub const MAX_ATTEMPTS_PER_FAILURE: usize = 8;

/// Longest failure cause rendered. Provider detail is already capped upstream;
/// this is the inspector's own second bound.
const MAX_CAUSE_CHARS: usize = 512;

/// Why a recorded failure happened, as far as the recorded text can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    Authentication,
    RateLimit,
    ContextLimit,
    OutputLimit,
    ReasoningWithoutAnswer,
    ProviderOrTransport,
    /// Retries were recorded and the turn finished. Not a failure; listed so a
    /// lineage with no attached error is visible rather than invisible.
    RecoveredAfterRetry,
    /// The recorded cause matched nothing known. No action is derived from it.
    Unclassified,
}

impl FailureClass {
    /// The single classifier for recorded provider outcomes.
    ///
    /// Shared with the diagnostic bundle so `/diagnostics` and `/recovery` can
    /// never disagree about what a failure was. The label strings are the
    /// bundle's existing vocabulary; the HTTP status rules are added because a
    /// 403 or a 503 matched none of the text rules and were being reported as
    /// unspecified, which would have recommended a retry for a credential
    /// failure.
    pub fn classify(text: &str) -> Self {
        let lower = text.to_ascii_lowercase();
        let status = |codes: &[u16]| {
            codes.iter().any(|code| {
                lower
                    .split(|c: char| !c.is_ascii_digit())
                    .any(|word| word == code.to_string())
            })
        };
        if lower.contains("output limit") {
            FailureClass::OutputLimit
        } else if lower.contains("reasoning") {
            FailureClass::ReasoningWithoutAnswer
        } else if lower.contains("429") || lower.contains("rate") {
            FailureClass::RateLimit
        } else if lower.contains("context") || status(&[413]) {
            FailureClass::ContextLimit
        } else if lower.contains("auth") || status(&[401, 403]) {
            FailureClass::Authentication
        } else if lower.contains("transport")
            || lower.contains("error")
            || lower.contains("failed")
            || status(&[500, 502, 503, 504])
        {
            FailureClass::ProviderOrTransport
        } else {
            FailureClass::Unclassified
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FailureClass::Authentication => "authentication",
            FailureClass::RateLimit => "rate_limit",
            FailureClass::ContextLimit => "context_limit",
            FailureClass::OutputLimit => "output_limit",
            FailureClass::ReasoningWithoutAnswer => "reasoning_without_answer",
            FailureClass::ProviderOrTransport => "provider_or_transport",
            FailureClass::RecoveredAfterRetry => "recovered_after_retry",
            FailureClass::Unclassified => "none_or_unspecified",
        }
    }

    /// How much the failure gets in the way.
    pub fn severity(self) -> Severity {
        match self {
            FailureClass::Authentication | FailureClass::ContextLimit => Severity::Blocking,
            FailureClass::RateLimit
            | FailureClass::ProviderOrTransport
            | FailureClass::OutputLimit
            | FailureClass::ReasoningWithoutAnswer => Severity::Degraded,
            FailureClass::RecoveredAfterRetry => Severity::Notice,
            FailureClass::Unclassified => Severity::Unclassified,
        }
    }

    /// The one bounded action this class maps to.
    pub fn action(self) -> SafeAction {
        match self {
            FailureClass::Authentication => SafeAction::CheckAuthentication,
            FailureClass::RateLimit => SafeAction::Wait,
            FailureClass::ContextLimit => SafeAction::RaiseContextWindow,
            // The provider said the answer did not fit the output budget, so
            // the request has to change; re-sending it unchanged truncates
            // again.
            FailureClass::OutputLimit => SafeAction::FixInput,
            FailureClass::ReasoningWithoutAnswer | FailureClass::ProviderOrTransport => {
                SafeAction::Retry
            }
            FailureClass::RecoveredAfterRetry => SafeAction::Inspect,
            // Nothing is known about the cause, so nothing is asserted beyond
            // not doing anything.
            FailureClass::Unclassified => SafeAction::Wait,
        }
    }
}

/// How badly a failure gets in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Needs the user before anything else can work.
    Blocking,
    /// The turn failed but the harness recovered on its own.
    Degraded,
    /// Recorded for lineage; nothing is wrong now.
    Notice,
    /// The cause did not match a known class.
    Unclassified,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Blocking => "blocking",
            Severity::Degraded => "degraded",
            Severity::Notice => "notice",
            Severity::Unclassified => "unclassified",
        }
    }
}

/// The bounded set of things the inspector is willing to suggest.
///
/// There is no variant that re-runs work: every action here either changes the
/// next request or does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafeAction {
    /// Send the request again. The failed attempt produced no event, so no
    /// tool ran from it.
    Retry,
    /// Change the request before sending it again.
    FixInput,
    /// Reconnect the provider.
    CheckAuthentication,
    /// Make the request smaller.
    RaiseContextWindow,
    /// Do nothing yet.
    Wait,
    /// Take no action; the record is there to be read.
    Inspect,
}

impl SafeAction {
    pub fn label(self) -> &'static str {
        match self {
            SafeAction::Retry => "retry",
            SafeAction::FixInput => "fix_input",
            SafeAction::CheckAuthentication => "check_authentication",
            SafeAction::RaiseContextWindow => "raise_context_window",
            SafeAction::Wait => "wait",
            SafeAction::Inspect => "inspect",
        }
    }

    pub fn guidance(self) -> &'static str {
        match self {
            SafeAction::Retry => {
                "Re-send the request. The attempt that failed produced no event, so nothing ran from it."
            }
            SafeAction::FixInput => {
                "Change the request before sending it again; the same input produced the same failure."
            }
            SafeAction::CheckAuthentication => {
                "Reconnect with /login. The fallback chain never routes around a credential failure."
            }
            SafeAction::RaiseContextWindow => {
                "Make the request smaller: /compact, or a model whose window fits it."
            }
            SafeAction::Wait => "Do nothing yet, then look again.",
            SafeAction::Inspect => "No action. This row is recorded lineage, not a failure.",
        }
    }
}

/// What the session log shows about work that already ran in the failed turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationState {
    /// No completed tool result or command is recorded in this turn.
    NoneRecorded,
    /// A tool result or command is already recorded in this turn.
    Completed,
}

impl MutationState {
    pub fn label(self) -> &'static str {
        match self {
            MutationState::NoneRecorded => "none_recorded",
            MutationState::Completed => "completed_in_turn",
        }
    }
}

/// One bounded recommendation.
///
/// `replays_completed_mutation` is a method, not a field: there is no way to
/// construct one that answers yes, so the inspector cannot promise a replay
/// even by accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recommendation {
    action: SafeAction,
    detail: String,
}

impl Recommendation {
    /// Build the one recommendation allowed for a failure.
    pub fn new(class: FailureClass, mutation_state: MutationState) -> Self {
        let action = class.action();
        let mut detail = action.guidance().to_string();
        if mutation_state == MutationState::Completed {
            // The turn already executed something. Say so, in the sentence a
            // user reads before pressing anything.
            detail.push_str(
                " Work already recorded in this turn stays in context: re-sending the request continues from those results and does not run them again.",
            );
        }
        Self { action, detail }
    }

    pub fn action(&self) -> SafeAction {
        self.action
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    /// Always false. No recommendation this module can produce re-runs work
    /// that already completed.
    pub fn replays_completed_mutation(&self) -> bool {
        false
    }

    fn to_json(&self) -> Value {
        json!({
            "action": self.action.label(),
            "guidance": self.action.guidance(),
            "detail": self.detail,
            "replays_completed_mutation": self.replays_completed_mutation(),
        })
    }
}

/// One recorded failure with everything the inspector could actually learn.
#[derive(Debug, Clone)]
pub struct Failure {
    pub raw_record: usize,
    pub turn: usize,
    pub cause: String,
    pub class: FailureClass,
    pub severity: Severity,
    /// Retries recorded for this failure, oldest first.
    pub attempts: Vec<Attempt>,
    /// Retries recorded for this failure, including any not shown.
    pub attempt_count: usize,
    /// False when the bounded attempt read may have cut older lineage.
    pub attempts_complete: bool,
    /// Wall-clock time of the last recorded retry. Null when no retry was
    /// recorded, because the session log itself carries no clock.
    pub timestamp_ms: Option<u128>,
    /// Provider request/response id, when the provider reported one.
    pub provider_request_id: Option<String>,
    pub mutation_state: MutationState,
    pub recommendation: Recommendation,
}

impl Failure {
    /// The backoff this run actually applied before its last retry. This is a
    /// measurement of what the harness did, not a provider cooldown.
    pub fn last_applied_backoff_s(&self) -> Option<u64> {
        self.attempts.last().map(|a| a.delay_s)
    }

    fn attempts_json(&self) -> Vec<Value> {
        self.attempts
            .iter()
            .map(|attempt| {
                json!({
                    "attempt": attempt.attempt,
                    "delay_s": attempt.delay_s,
                    "cause": attempt.cause,
                    "recorded_at_ms": attempt.recorded_at_ms,
                    "provider_request_id": attempt.request_id,
                })
            })
            .collect()
    }

    fn to_json(&self) -> Value {
        json!({
            "raw_record": self.raw_record,
            "turn": self.turn,
            "severity": self.severity.label(),
            "class": self.class.label(),
            "cause": self.cause,
            "attempt_count": self.attempt_count,
            "attempts_shown": self.attempts.len(),
            "attempts_complete": self.attempts_complete,
            "attempts": self.attempts_json(),
            "timestamp_ms": self.timestamp_ms,
            "timestamp_known": self.timestamp_ms.is_some(),
            "timestamp_source": if self.timestamp_ms.is_some() {
                "retry attempt record"
            } else {
                "unrecorded: the session log stores no clock and no retry was recorded"
            },
            "provider_request_id": self.provider_request_id,
            "provider_request_id_known": self.provider_request_id.is_some(),
            "provider_request_id_source": if self.provider_request_id.is_some() {
                "reported by the provider"
            } else {
                "unknown: the provider returned no id"
            },
            "cooldown": {
                "provider_retry_after_s": Value::Null,
                "provider_retry_after_known": false,
                "last_applied_backoff_s": self.last_applied_backoff_s(),
                "note": "glass-core parses no Retry-After header, so no provider cooldown is known. The backoff shown is the delay this run actually used.",
            },
            "mutation_state": self.mutation_state.label(),
            "recommended_action": self.recommendation.to_json(),
        })
    }
}

/// One `error:` record found while scanning the session log.
struct Recorded {
    raw_record: usize,
    turn: usize,
    cause: String,
    completed_runs: usize,
}

/// The `k` field of a raw record, read without materializing its body.
///
/// Every record this harness writes starts `{"k":"…`, so the common path is a
/// prefix scan. Anything else is parsed, which cannot be cheaper but must not
/// be wrong.
fn record_kind(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("{\"k\":\"") {
        return rest.find('"').map(|end| rest[..end].to_string());
    }
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("k")?
        .as_str()
        .map(str::to_string)
}

/// Stream the session log, keeping only bounded failure metadata.
///
/// The scan is forward over the whole log and retains nothing but counters and
/// the last [`MAX_FAILURES`] error records, so no transcript body is held.
fn scan(path: &Path) -> Result<(Vec<Recorded>, usize, usize), String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), 0, 0));
        }
        Err(error) => return Err(error.to_string()),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Session log is not a regular file".into());
    }
    let mut recorded: Vec<Recorded> = Vec::new();
    let mut turn = 0usize;
    let mut records = 0usize;
    let mut runs_in_turn = 0usize;
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let Ok(line) = line else { break };
        records = index + 1;
        match record_kind(&line).as_deref() {
            Some("user") => {
                turn += 1;
                runs_in_turn = 0;
            }
            // A bash record and a tool result only exist once the call ran.
            Some("bash") | Some("tool") => runs_in_turn += 1,
            Some("meta") => {
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let Some(text) = value.get("t").and_then(Value::as_str) else {
                    continue;
                };
                let Some(cause) = text.strip_prefix("error:") else {
                    continue;
                };
                if recorded.len() == MAX_FAILURES {
                    recorded.remove(0);
                }
                recorded.push(Recorded {
                    raw_record: index,
                    turn,
                    cause: clip(cause),
                    completed_runs: runs_in_turn,
                });
            }
            _ => {}
        }
    }
    Ok((recorded, records, turn))
}

/// The inspector's own bound on a rendered cause, applied after the provider
/// detail sanitizer so a second pass can never widen what the first removed.
fn clip(text: &str) -> String {
    let sanitized = crate::live::sanitize_provider_detail(text.trim());
    if sanitized.chars().count() <= MAX_CAUSE_CHARS {
        return sanitized;
    }
    sanitized
        .chars()
        .take(MAX_CAUSE_CHARS)
        .chain(std::iter::once('…'))
        .collect()
}

/// Bind each failure to the retries that ran inside it.
///
/// An `error:` record at index `n` owns every attempt recorded at an index in
/// `(previous error index, n]`. That is a durable coordinate, so the binding
/// survives a resumed session and needs no clock.
///
/// Returns the failures newest first, plus the attempts no failure claimed —
/// retries that happened and were followed by a completed turn.
fn bind(mut recorded: Vec<Recorded>, attempts: &[Attempt]) -> (Vec<Failure>, &[Attempt]) {
    recorded.reverse();
    let mut cursor = attempts.len();
    let mut failures = Vec::with_capacity(recorded.len());
    for index in 0..recorded.len() {
        // `recorded` is newest first, so the failure that came before this one
        // in the log is the next entry, not the previous one.
        let lower = recorded.get(index + 1).map_or(0, |older| older.raw_record);
        let mut first = cursor;
        while first > 0 && attempts[first - 1].record_index > lower {
            first -= 1;
        }
        let owned = &attempts[first..cursor];
        cursor = first;
        let entry = &recorded[index];
        let class = FailureClass::classify(&entry.cause);
        let mutation_state = if entry.completed_runs > 0 {
            MutationState::Completed
        } else {
            MutationState::NoneRecorded
        };
        let request_id = owned
            .iter()
            .rev()
            .find_map(|attempt| attempt.request_id.clone())
            .or_else(|| crate::transport::request_id(&entry.cause));
        failures.push(Failure {
            raw_record: entry.raw_record,
            turn: entry.turn,
            cause: entry.cause.clone(),
            class,
            severity: class.severity(),
            attempts: tail(owned),
            attempt_count: owned.len(),
            attempts_complete: true,
            timestamp_ms: owned.last().map(|attempt| attempt.recorded_at_ms),
            provider_request_id: request_id,
            mutation_state,
            recommendation: Recommendation::new(class, mutation_state),
        });
    }
    let unclaimed = &attempts[..cursor];
    (failures, unclaimed)
}

/// The most recent `MAX_ATTEMPTS_PER_FAILURE`, oldest first. The true count
/// stays on the failure, so a capped row never reads as the whole lineage.
fn tail(owned: &[Attempt]) -> Vec<Attempt> {
    owned
        .iter()
        .skip(owned.len().saturating_sub(MAX_ATTEMPTS_PER_FAILURE))
        .cloned()
        .collect()
}

/// The retries no failure claimed, i.e. lineage that ended in a completed
/// turn. Listed so a non-empty attempt log is never silently unexplained.
fn recovered(unclaimed: &[Attempt]) -> Option<Failure> {
    let last = unclaimed.last()?;
    Some(Failure {
        raw_record: last.record_index,
        turn: 0,
        cause: last.cause.clone(),
        class: FailureClass::RecoveredAfterRetry,
        severity: Severity::Notice,
        attempts: tail(unclaimed),
        attempt_count: unclaimed.len(),
        attempts_complete: true,
        timestamp_ms: Some(last.recorded_at_ms),
        provider_request_id: unclaimed
            .iter()
            .rev()
            .find_map(|attempt| attempt.request_id.clone()),
        mutation_state: MutationState::NoneRecorded,
        recommendation: Recommendation::new(
            FailureClass::RecoveredAfterRetry,
            MutationState::NoneRecorded,
        ),
    })
}

/// The failure list, newest first, with the attempt lineage bound to each row.
pub fn failures(session: &Session) -> Result<(Vec<Failure>, bool), String> {
    let (recorded, _, _) = scan(&session.path)?;
    let (attempts, complete) = crate::attempt_log::window(&session.path)?;
    let (mut failures, unclaimed) = bind(recorded, &attempts);
    if let Some(recovered) = recovered(unclaimed) {
        failures.insert(0, recovered);
    }
    // A truncated tail can hide attempts, so no row may claim a complete
    // lineage from one.
    for failure in &mut failures {
        failure.attempts_complete = complete;
    }
    Ok((failures, complete))
}

/// Whether the inspector should offer a transport reset, and why.
///
/// Offered only for a connection-level failure: a new transport cannot
/// reconnect credentials or make a request fit a smaller window, and offering
/// it there would imply a fix that does not exist.
pub fn reset_offer(failures: &[Failure], safety: &crate::transport::Safety) -> (bool, String) {
    if !safety.is_safe() {
        return (false, safety.reason().to_string());
    }
    let newest = failures
        .iter()
        .find(|failure| failure.class != FailureClass::RecoveredAfterRetry);
    match newest {
        None => (
            false,
            "no recorded provider failure to recover from".to_string(),
        ),
        Some(failure) if failure.class == FailureClass::ProviderOrTransport => (
            true,
            "rebuilds the provider transport from the current configuration and appends one marker to the session log; no record is removed and nothing is re-sent".to_string(),
        ),
        Some(failure) => (
            false,
            format!(
                "the newest failure is {}; a new transport cannot change that — recommended action is {}",
                failure.class.label(),
                failure.recommendation.action().label()
            ),
        ),
    }
}

/// The local inspector view: a bounded list with everything that was recorded.
///
/// This is local-only. It carries provider request ids and therefore is not a
/// shareable artifact; `diagnostic_bundle::snapshot` is.
pub fn inspector(session: &Session) -> Value {
    let (mut failures, complete) = failures(session).unwrap_or_default();
    failures.truncate(MAX_FAILURES);
    let safety = crate::transport::reset_safety(session, false);
    let (offered, reason) = reset_offer(&failures, &safety);
    json!({
        "schema_version": 1,
        "application": "glass-core",
        "session": session.id,
        "scope": "Local recovery inspection of durable records only. Reads the session log and the attempt log; starts no request and adds no background work.",
        "bounds": {
            "max_failures": MAX_FAILURES,
            "max_attempts_per_failure": MAX_ATTEMPTS_PER_FAILURE,
            "attempt_lineage_complete": complete,
            "transcript_bodies_loaded": false,
        },
        "failures": failures.iter().map(Failure::to_json).collect::<Vec<_>>(),
        "transport_reset": {
            "offered": offered,
            "reason": reason,
            "preserves_session_log": true,
            "resends_requests": false,
            "marker_kind": crate::transport::RESET_KIND,
        },
        "cooldown_note": "No provider cooldown is known: glass-core parses no Retry-After header and keeps no circuit-breaker state. Backoffs shown are the delays this run actually applied.",
        "sharing": "Local view. Provider request ids are present here and are omitted from gc diagnostics bundles.",
    })
}

/// `HH:MM:SS UTC` for a recorded millisecond stamp.
fn clock(unix_ms: u128) -> String {
    let day = (unix_ms / 1000) % 86_400;
    format!(
        "{:02}:{:02}:{:02} UTC",
        day / 3600,
        (day % 3600) / 60,
        day % 60
    )
}

/// One selectable row: severity, class, attempts, timestamp, action.
fn row(failure: &Failure) -> String {
    let attempts = if failure.attempt_count == 0 {
        "0 attempts".to_string()
    } else if failure.attempt_count == 1 {
        "1 attempt".to_string()
    } else {
        format!("{} attempts", failure.attempt_count)
    };
    let timestamp = match failure.timestamp_ms {
        Some(ms) => clock(ms),
        None => "time unknown".to_string(),
    };
    format!(
        "{} · {} · {attempts} · {timestamp} · {}",
        failure.severity.label(),
        failure.class.label(),
        failure.recommendation.action().label()
    )
}

/// The navigable inspector behind `/recovery`.
///
/// The raw JSON stays reachable: every row opens a detail view that offers the
/// formatted and the raw rendering, and the list carries an entry for the
/// shareable diagnostic bundle and for the local JSON export.
pub fn view(
    keys: &crate::input::KeyReader,
    session: &mut Session,
    factory: &crate::warmer::ProviderFactory,
) {
    let (mut listed, complete) = failures(session).unwrap_or_default();
    listed.truncate(MAX_FAILURES);
    let safety = crate::transport::reset_safety(session, false);
    let (reset_offered, reset_reason) = reset_offer(&listed, &safety);

    if listed.is_empty() {
        crate::modal::info(
            keys,
            "Failure recovery",
            "No provider failure is recorded for this session. The attempt log is empty and the session log holds no error records.",
        );
        return;
    }

    let mut entries: Vec<String> = listed.iter().map(row).collect();
    entries.push("Diagnostic bundle · shareable, redacted".to_string());
    entries.push("Export inspector JSON · local, includes provider request ids".to_string());
    if reset_offered {
        entries.push("Reset provider transport · keeps the session log".to_string());
    } else {
        entries.push(format!(
            "Reset provider transport · unavailable: {reset_reason}"
        ));
    }

    let note = format!(
        "Newest first · {}/{} failures shown · attempt lineage {} · Esc back",
        listed.len(),
        MAX_FAILURES,
        if complete {
            "complete"
        } else {
            "possibly truncated"
        }
    );
    while let Some(choice) = crate::modal::choose(keys, "Failure recovery", &note, &entries) {
        match choice {
            index if index < listed.len() => {
                let failure = &listed[index];
                let mut detail = json!({
                    "severity": failure.severity.label(),
                    "class": failure.class.label(),
                    "cause": failure.cause,
                    "attempts": failure.attempts_json(),
                    "attempt_count": failure.attempt_count,
                    "attempts_complete": failure.attempts_complete,
                    "timestamp_ms": failure.timestamp_ms,
                    "timestamp_known": failure.timestamp_ms.is_some(),
                    "provider_request_id": failure.provider_request_id,
                    "provider_request_id_known": failure.provider_request_id.is_some(),
                    "last_applied_backoff_s": failure.last_applied_backoff_s(),
                    "provider_cooldown_known": false,
                    "mutation_state": failure.mutation_state.label(),
                    "recommended_action": failure.recommendation.to_json(),
                    "raw_record": failure.raw_record,
                    "turn": failure.turn,
                    "full_history": format!("gc export {}", session.id),
                });
                if let Some(map) = detail.as_object_mut() {
                    map.insert(
                        "detail".into(),
                        Value::String(failure.recommendation.detail().to_string()),
                    );
                }
                crate::structured::view(
                    keys,
                    &format!("Failure · {}", failure.class.label()),
                    &detail.to_string(),
                );
            }
            index if index == listed.len() => match crate::diagnostic_bundle::snapshot(session) {
                Ok(bundle) => {
                    crate::structured::view(keys, "Diagnostic bundle", &bundle.to_string())
                }
                Err(error) => crate::modal::info(keys, "Diagnostics unavailable", &error),
            },
            index if index == listed.len() + 1 => {
                let value = inspector(session);
                crate::structured::view(
                    keys,
                    "Recovery inspector · local JSON",
                    &value.to_string(),
                );
            }
            _ => match crate::transport::reset(session, factory) {
                Ok((_provider, receipt)) => crate::modal::info(
                    keys,
                    "Transport reset",
                    &format!(
                        "Provider rebuilt. Session log went from {} to {} records; nothing was removed or reordered. Marker kind: {}.",
                        receipt.records_before, receipt.records_after, receipt.marker_kind
                    ),
                ),
                Err(error) => crate::modal::info(keys, "Transport reset failed", &error),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Record;

    #[test]
    fn clock_formats_timestamps_larger_than_i64_without_wrapping() {
        let unix_ms = (u128::from(u64::try_from(i64::MAX).unwrap()) + 1) * 1000;
        assert_eq!(clock(unix_ms), "15:30:08 UTC");
    }

    /// A real session directory plus its id, so tests read back the log the
    /// harness actually wrote rather than a hand-built stand-in.
    struct Fixture {
        dir: std::path::PathBuf,
        id: String,
    }

    impl Fixture {
        fn create(name: &str) -> (Self, Session) {
            let dir =
                std::env::temp_dir().join(format!("gc-recovery-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let session = Session::create(&dir).unwrap();
            let fixture = Self {
                dir,
                id: session.id.clone(),
            };
            (fixture, session)
        }

        fn open(&self) -> Session {
            Session::open_readonly(&self.dir, &self.id).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn every_known_failure_class_maps_to_exactly_one_bounded_action() {
        let classes = [
            (
                FailureClass::Authentication,
                SafeAction::CheckAuthentication,
            ),
            (FailureClass::RateLimit, SafeAction::Wait),
            (FailureClass::ContextLimit, SafeAction::RaiseContextWindow),
            (FailureClass::OutputLimit, SafeAction::FixInput),
            (FailureClass::ReasoningWithoutAnswer, SafeAction::Retry),
            (FailureClass::ProviderOrTransport, SafeAction::Retry),
            (FailureClass::RecoveredAfterRetry, SafeAction::Inspect),
            (FailureClass::Unclassified, SafeAction::Wait),
        ];
        for (class, expected) in classes {
            assert_eq!(class.action(), expected, "{class:?}");
            // Deterministic: the same class never yields two answers.
            assert_eq!(class.action(), class.action(), "{class:?}");
            assert!(!class.action().label().is_empty());
        }
        // Every class the classifier can return is covered by the table above.
        for text in [
            "output limit reached",
            "reasoning arrived without an answer",
            "http 429: slow down",
            "context length exceeded",
            "http 401: bad key",
            "http 403: forbidden",
            "http 503: overloaded",
            "transport: connection reset",
            "something never seen before",
        ] {
            let class = FailureClass::classify(text);
            assert!(
                classes.iter().any(|(known, _)| *known == class),
                "{text} -> {class:?}"
            );
        }
    }

    #[test]
    fn a_completed_mutation_is_never_marked_safe_to_replay() {
        for class in [
            FailureClass::Authentication,
            FailureClass::RateLimit,
            FailureClass::ContextLimit,
            FailureClass::OutputLimit,
            FailureClass::ReasoningWithoutAnswer,
            FailureClass::ProviderOrTransport,
            FailureClass::RecoveredAfterRetry,
            FailureClass::Unclassified,
        ] {
            for state in [MutationState::NoneRecorded, MutationState::Completed] {
                let recommendation = Recommendation::new(class, state);
                assert!(!recommendation.replays_completed_mutation());
                if state == MutationState::Completed {
                    assert!(
                        recommendation.detail().contains("does not run them again"),
                        "{class:?} detail did not state that recorded work is kept: {}",
                        recommendation.detail()
                    );
                }
            }
        }
    }

    #[test]
    fn a_completed_mutation_is_measured_from_the_log_not_assumed() {
        let (fixture, mut session) = Fixture::create("mutation");
        session.append(Record::User("go".into()));
        session.append(Record::Bash {
            call_id: Some("c1".into()),
            command: "true".into(),
            output: String::new(),
            exit_code: Some(0),
            truncated: false,
            spill: None,
        });
        session.append(Record::Meta("error: http 503: overloaded".into()));
        drop(session);

        let session = fixture.open();
        let (failures, _) = failures(&session).unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].mutation_state, MutationState::Completed);
        assert_eq!(failures[0].recommendation.action(), SafeAction::Retry);
        assert!(!failures[0].recommendation.replays_completed_mutation());
        assert!(
            failures[0]
                .recommendation
                .detail()
                .contains("does not run them again")
        );
    }

    #[test]
    fn a_turn_with_no_recorded_run_reports_no_mutation_rather_than_guessing() {
        let (fixture, mut session) = Fixture::create("nomutation");
        session.append(Record::User("go".into()));
        session.append(Record::Meta("error: http 429: slow down".into()));
        drop(session);

        let session = fixture.open();
        let (failures, _) = failures(&session).unwrap();
        assert_eq!(failures[0].mutation_state, MutationState::NoneRecorded);
        assert_eq!(failures[0].class, FailureClass::RateLimit);
        assert_eq!(failures[0].recommendation.action(), SafeAction::Wait);
    }

    #[test]
    fn an_unrecorded_clock_stays_unknown_and_an_absent_id_stays_absent() {
        let (fixture, mut session) = Fixture::create("unknown");
        session.append(Record::User("go".into()));
        session.append(Record::Meta("error: http 503: overloaded".into()));
        drop(session);

        let session = fixture.open();
        let (failures, _) = failures(&session).unwrap();
        assert_eq!(failures[0].timestamp_ms, None);
        assert_eq!(failures[0].provider_request_id, None);

        let entry = &inspector(&session)["failures"][0].clone();
        assert_eq!(entry["timestamp_known"], false);
        assert!(entry["timestamp_ms"].is_null());
        assert_eq!(entry["provider_request_id_known"], false);
        assert!(entry["provider_request_id"].is_null());
        assert_eq!(entry["cooldown"]["provider_retry_after_known"], false);
        assert!(entry["cooldown"]["provider_retry_after_s"].is_null());
        assert!(entry["last_applied_backoff_s"].is_null());
        assert_eq!(
            entry["recommended_action"]["replays_completed_mutation"],
            false
        );
    }

    #[test]
    fn each_retry_is_listed_with_its_own_attempt_number_delay_and_cause() {
        let (fixture, mut session) = Fixture::create("lineage");
        session.append(Record::User("go".into()));
        crate::attempt_log::append(&session.path, 1, 1, "http 503: overloaded", 1, None).unwrap();
        crate::attempt_log::append(&session.path, 2, 2, "http 503: overloaded", 1, None).unwrap();
        session.append(Record::Meta("error: http 503: overloaded".into()));
        drop(session);

        let session = fixture.open();
        let (failures, complete) = failures(&session).unwrap();
        assert!(complete);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].attempt_count, 2);
        assert!(failures[0].attempts_complete);
        assert_eq!(
            failures[0]
                .attempts
                .iter()
                .map(|a| (a.attempt, a.delay_s))
                .collect::<Vec<_>>(),
            vec![(1, 1), (2, 2)],
            "the lineage is chronological: attempt 1 happened first"
        );
        assert!(failures[0].attempts.iter().all(|a| a.cause.contains("503")));
        assert!(
            failures[0].timestamp_ms.is_some(),
            "a retry is a real clock reading"
        );
        assert_eq!(failures[0].last_applied_backoff_s(), Some(2));
    }

    #[test]
    fn a_retry_is_bound_to_the_failure_that_followed_it() {
        let (fixture, mut session) = Fixture::create("binding");
        session.append(Record::User("go".into()));
        // Recorded while the first turn was still in flight.
        crate::attempt_log::append(&session.path, 1, 1, "http 429: slow down", 1, None).unwrap();
        session.append(Record::Meta("error: http 429: slow down".into()));
        crate::attempt_log::append(&session.path, 1, 2, "http 503: overloaded", 2, None).unwrap();
        session.append(Record::Meta("error: http 503: overloaded".into()));
        drop(session);

        let session = fixture.open();
        let (failures, _) = failures(&session).unwrap();
        assert_eq!(failures.len(), 2, "no attempt is unclaimed");
        assert_eq!(failures[0].raw_record, 2);
        assert_eq!(failures[0].cause, "http 503: overloaded");
        assert_eq!(failures[0].attempt_count, 1);
        assert_eq!(failures[0].attempts[0].delay_s, 2);
        assert_eq!(failures[1].raw_record, 1);
        assert_eq!(failures[1].cause, "http 429: slow down");
        assert_eq!(failures[1].attempt_count, 1);
        assert_eq!(failures[1].attempts[0].delay_s, 1);
    }

    #[test]
    fn a_retry_no_failure_claimed_is_reported_as_recovered_not_dropped() {
        let (fixture, mut session) = Fixture::create("recovered");
        session.append(Record::User("go".into()));
        crate::attempt_log::append(&session.path, 1, 4, "http 503: overloaded", 1, None).unwrap();
        session.append(Record::Assistant("answered after the retry".into()));
        drop(session);

        let session = fixture.open();
        let (failures, _) = failures(&session).unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].class, FailureClass::RecoveredAfterRetry);
        assert_eq!(failures[0].severity, Severity::Notice);
        assert_eq!(failures[0].recommendation.action(), SafeAction::Inspect);
        assert_eq!(failures[0].attempt_count, 1);
    }

    #[test]
    fn the_inspector_shows_a_provider_id_while_the_shareable_bundle_omits_it() {
        let (fixture, mut session) = Fixture::create("ids");
        session.append(Record::User("PRIVATE_PROMPT_SENTINEL".into()));
        crate::attempt_log::append(
            &session.path,
            1,
            1,
            "stream diagnostics: events=9, finish=length, response=resp_01SENTINEL, usage=unknown",
            1,
            None,
        )
        .unwrap();
        session.append(Record::Meta(
            "error: stream diagnostics: events=9, finish=length, response=resp_01SENTINEL, usage=unknown".into(),
        ));
        drop(session);

        let session = fixture.open();
        let local = serde_json::to_string(&inspector(&session)).unwrap();
        assert!(
            local.contains("resp_01SENTINEL"),
            "the local inspector keeps the id the provider returned"
        );
        assert!(!local.contains("PRIVATE_PROMPT_SENTINEL"));

        let bundle =
            serde_json::to_string(&crate::diagnostic_bundle::snapshot(&session).unwrap()).unwrap();
        assert!(!bundle.contains("resp_01SENTINEL"), "{bundle}");
        assert!(!bundle.contains("PRIVATE_PROMPT_SENTINEL"), "{bundle}");
    }

    #[test]
    fn the_inspector_output_carries_no_prompt_credential_or_endpoint() {
        let (fixture, mut session) = Fixture::create("leaks");
        session.append(Record::User("LEAK_PROMPT_SENTINEL".into()));
        crate::attempt_log::append(
            &session.path,
            1,
            1,
            "transport failure calling https://LEAK_ENDPOINT_SENTINEL.invalid/v1 with authorization=Bearer sk-LEAK_KEY_SENTINEL",
            1,
            None,
        )
        .unwrap();
        session.append(Record::Meta(
            "error: transport failure calling https://LEAK_ENDPOINT_SENTINEL.invalid/v1 with authorization=Bearer sk-LEAK_KEY_SENTINEL".into(),
        ));
        drop(session);

        let session = fixture.open();
        let text = inspector(&session).to_string();
        for sentinel in [
            "LEAK_PROMPT_SENTINEL",
            "LEAK_ENDPOINT_SENTINEL",
            "LEAK_KEY_SENTINEL",
        ] {
            assert!(
                !text.contains(sentinel),
                "inspector leaked {sentinel}: {text}"
            );
        }
    }

    #[test]
    fn a_reset_is_offered_only_for_a_transport_failure_and_only_when_safe() {
        let safety = crate::transport::Safety::Safe;
        let blocked = crate::transport::Safety::Refused("a turn is streaming".into());
        let failure = |class: FailureClass| Failure {
            raw_record: 0,
            turn: 1,
            cause: "recorded".into(),
            class,
            severity: class.severity(),
            attempts: Vec::new(),
            attempt_count: 0,
            attempts_complete: true,
            timestamp_ms: None,
            provider_request_id: None,
            mutation_state: MutationState::NoneRecorded,
            recommendation: Recommendation::new(class, MutationState::NoneRecorded),
        };

        let transport = vec![failure(FailureClass::ProviderOrTransport)];
        assert!(reset_offer(&transport, &safety).0);
        assert!(!reset_offer(&transport, &blocked).0);
        assert!(!reset_offer(&[], &safety).0);

        for class in [
            FailureClass::Authentication,
            FailureClass::RateLimit,
            FailureClass::ContextLimit,
            FailureClass::OutputLimit,
            FailureClass::Unclassified,
        ] {
            assert!(!reset_offer(&[failure(class)], &safety).0, "{class:?}");
        }
    }

    #[test]
    fn the_failure_list_is_bounded_and_never_holds_a_transcript_body() {
        let (_fixture, mut session) = Fixture::create("bounds");
        for index in 0..MAX_FAILURES + 8 {
            session.append(Record::User(format!("LEAK_PROMPT_SENTINEL {index}")));
            session.append(Record::Assistant("BODY_SENTINEL".repeat(400)));
            session.append(Record::Meta(format!("error: http 503: overloaded {index}")));
        }
        let value = inspector(&session);
        let listed = value["failures"].as_array().unwrap();
        assert_eq!(listed.len(), MAX_FAILURES, "the bound is enforced");
        assert_eq!(value["bounds"]["max_failures"], MAX_FAILURES);
        assert_eq!(value["bounds"]["transcript_bodies_loaded"], false);
        let text = value.to_string();
        assert!(
            !text.contains("BODY_SENTINEL"),
            "no transcript body was loaded"
        );
        assert!(
            !text.contains("LEAK_PROMPT_SENTINEL"),
            "no user prompt was loaded"
        );
    }

    #[test]
    fn an_empty_session_reports_no_failures_rather_than_an_empty_class() {
        let (fixture, _session) = Fixture::create("empty");
        let session = fixture.open();
        let value = inspector(&session);
        assert!(value["failures"].as_array().unwrap().is_empty());
        assert_eq!(value["transport_reset"]["offered"], false);
        assert!(
            value["transport_reset"]["reason"]
                .as_str()
                .unwrap()
                .contains("no recorded provider failure")
        );
    }
}
