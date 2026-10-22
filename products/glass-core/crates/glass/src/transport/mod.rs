//! Provider transport identity and reset.
//!
//! Two things live here because they are properties of the wire, not of the
//! conversation: the identity a provider reports for a request, and the
//! ability to re-establish the transport without touching local history.
//!
//! Neither invents data. `request_id` returns the id the provider actually put
//! in the text it returned, or nothing at all — never a synthesised one, never
//! the endpoint, never a counter. `reset` re-establishes the transport and
//! appends a marker to the session log; it never rewrites or truncates it.

use crate::provider::Provider;
use crate::session::{Record, Session};
use crate::warmer::ProviderFactory;

/// Longest provider identity accepted. Providers truncate their own ids well
/// below this; anything longer is not an identity we should render.
const MAX_ID_CHARS: usize = 160;

/// Durable kind of the marker a transport reset leaves in the session log.
pub const RESET_KIND: &str = "recovery/transport-reset";

/// Values providers use to say "there is no id", which must not become one.
const NOT_REPORTED: [&str; 6] = [
    "not reported",
    "not_reported",
    "unknown",
    "none",
    "null",
    "not available",
];

/// The provider request/response id carried by provider-returned text.
///
/// Providers expose their identity inconsistently: some put `response=<id>` in
/// a diagnostic, some echo `request-id`, many send nothing at all. This reads
/// an id only when the provider wrote one, and rejects the shapes that are not
/// identities — a URL, a bearer token, a sentence fragment, or one of the "not
/// reported" placeholders an adapter substitutes when a stream carried no id.
/// An id that is absent stays absent.
pub fn request_id(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        let (key, inline) = word
            .split_once('=')
            .or_else(|| word.split_once(':'))
            .unwrap_or((word, ""));
        if !is_key(key) {
            continue;
        }
        // The value runs to the next `key=` token, so a multi-word placeholder
        // such as `not reported` is read whole and rejected rather than being
        // truncated into the identity "not".
        let candidate = value_after(&words, index, inline);
        let candidate = candidate.trim();
        if is_identity(candidate) {
            return Some(candidate.trim_matches(IDENTITY_PUNCTUATION).to_string());
        }
    }
    None
}

/// The key names providers use for a request identity.
fn is_key(key: &str) -> bool {
    matches!(
        key.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-'),
        "response" | "response_id" | "request_id" | "request-id" | "response-id"
    )
}

/// Punctuation that surrounds a value in prose rather than belonging to it.
const IDENTITY_PUNCTUATION: [char; 6] = ['"', '\'', ',', ';', '.', ':'];

fn value_after(words: &[&str], index: usize, inline: &str) -> String {
    let mut parts = vec![inline.trim_matches(IDENTITY_PUNCTUATION).to_string()];
    for word in words.iter().skip(index + 1).take(3) {
        if word.contains('=') || word.contains(':') {
            break;
        }
        parts.push((*word).to_string());
    }
    parts.join(" ")
}

/// Whether a captured token is shaped like a provider identity.
///
/// Deliberately narrow. An endpoint, a bearer token or a sentence fragment
/// must never be promoted to a request id just because it followed a key.
fn is_identity(value: &str) -> bool {
    let value = value.trim_matches(IDENTITY_PUNCTUATION).trim();
    if value.is_empty() || value.chars().count() > MAX_ID_CHARS {
        return false;
    }
    if NOT_REPORTED
        .iter()
        .any(|placeholder| value.eq_ignore_ascii_case(placeholder))
    {
        return false;
    }
    value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Why a reset may or may not run right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Safety {
    /// A reset re-establishes the transport and appends to the session log.
    Safe,
    /// Refused, with the reason a user can act on.
    Refused(String),
}

impl Safety {
    pub fn is_safe(&self) -> bool {
        matches!(self, Safety::Safe)
    }

    pub fn reason(&self) -> &str {
        match self {
            Safety::Safe => "the provider is rebuilt from the current configuration and the session log is only appended to",
            Safety::Refused(reason) => reason,
        }
    }
}

/// Whether a reset is actually safe right now.
///
/// A reset re-establishes the transport. It is refused when it could tear the
/// connection out from under a running turn, or when the session log cannot
/// accept the marker that makes the reset auditable.
pub fn reset_safety(session: &Session, turn_in_flight: bool) -> Safety {
    if turn_in_flight {
        return Safety::Refused("a turn is streaming; wait for it to finish".into());
    }
    if let Some(error) = session.write_error() {
        return Safety::Refused(format!(
            "the session log is not writable, so the reset could not be recorded: {error}"
        ));
    }
    Safety::Safe
}

/// What a reset did, and what it deliberately left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// Durable kind of the marker appended to the session log.
    pub marker_kind: &'static str,
    /// Records in the session log before the reset.
    pub records_before: usize,
    /// Records in the session log after it. Never smaller: a reset appends.
    pub records_after: usize,
    /// Provider identity of the rebuilt transport, when it reports one.
    pub provider: Option<String>,
}

/// Re-establish the provider transport, preserving the local session log.
///
/// What this does: builds a fresh provider from the current configuration, so
/// the HTTP agent, its connection pool and the resolved credential are new.
///
/// What this does not do: touch, truncate or reorder any recorded record. The
/// only write is one appended marker, which is what makes the reset visible in
/// the inspector afterwards. If the marker cannot be written the provider is
/// still returned, but the caller is told the reset is unrecorded rather than
/// being handed a silent success.
pub fn reset(
    session: &mut Session,
    factory: &ProviderFactory,
) -> Result<(Box<dyn Provider>, Receipt), String> {
    let records_before = session.log_len();
    let provider = factory()?;
    let provider_id = Some(provider.id().to_string());
    session
        .append_checked(&[Record::Custom {
            kind: RESET_KIND.to_string(),
            data: Some(
                serde_json::json!({
                    "kind": RESET_KIND,
                    "provider_rebuilt": true,
                    "session_log": "appended only; no record was removed or reordered",
                })
                .to_string(),
            ),
        }])
        .map_err(|error| {
            format!("transport rebuilt but the reset marker could not be recorded: {error}")
        })?;
    Ok((
        provider,
        Receipt {
            marker_kind: RESET_KIND,
            records_before,
            records_after: session.log_len(),
            provider: provider_id,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_reported_id_is_captured_and_a_missing_one_is_not_invented() {
        assert_eq!(
            request_id("events=12, finish=length, response=resp_01H9Z, usage=unknown"),
            Some("resp_01H9Z".into())
        );
        assert_eq!(
            request_id("request-id: abc-123"),
            Some("abc-123".into())
        );
        // Adapters substitute a placeholder when a stream carried no id.
        assert_eq!(request_id("response=not reported"), None);
        assert_eq!(request_id("events=3, finish=stop, model=m"), None);
        assert_eq!(request_id(""), None);
        // Never promote a non-identity to an identity.
        assert_eq!(request_id("response=https://endpoint.invalid/v1"), None);
        assert_eq!(request_id("authorization=Bearer sk-secret-value"), None);
        assert_eq!(request_id("response=\u{1b}]52;c;EVIL\u{7}"), None);
    }

    #[test]
    fn an_over_long_token_is_refused_rather_than_truncated_into_an_identity() {
        let long = "a".repeat(MAX_ID_CHARS + 1);
        assert_eq!(request_id(&format!("response={long}")), None);
    }

    #[test]
    fn a_reset_rebuilds_the_transport_and_preserves_the_session_log() {
        let dir = std::env::temp_dir().join(format!("gc-transport-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut session = Session::create(&dir).unwrap();
        session.append(Record::User("first turn".into()));
        session.append(Record::Assistant("first answer".into()));
        session.append(Record::Meta("error: http 503: overloaded".into()));
        let before = Session::raw_records(&session.path).unwrap();
        assert_eq!(before.len(), 3);

        let factory: ProviderFactory = std::sync::Arc::new(|| {
            Err("no provider configured in this fixture".to_string())
        });
        // A factory that cannot build anything must not touch the log.
        let refused = reset(&mut session, &factory);
        assert!(refused.is_err());
        assert_eq!(Session::raw_records(&session.path).unwrap(), before);

        let factory: ProviderFactory = std::sync::Arc::new(|| {
            Ok(Box::new(crate::provider::OpenAiCompat::new(
                "https://example.invalid",
                None,
                "m",
            )) as Box<dyn Provider>)
        });
        let (provider, receipt) = reset(&mut session, &factory).unwrap();
        assert_eq!(provider.id(), "openai");

        let after = Session::raw_records(&session.path).unwrap();
        assert_eq!(
            after.len(),
            before.len() + 1,
            "a reset appends one marker and removes nothing"
        );
        assert_eq!(&after[..before.len()], &before[..], "history is untouched");
        assert!(matches!(
            after.last(),
            Some(Record::Custom { kind, .. }) if kind == RESET_KIND
        ));
        assert_eq!(receipt.records_before, before.len());
        assert_eq!(receipt.records_after, before.len() + 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_reset_is_refused_while_a_turn_is_streaming_or_the_log_is_unwritable() {
        let dir = std::env::temp_dir().join(format!("gc-transport-safe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session = Session::create(&dir).unwrap();
        assert_eq!(reset_safety(&session, true), Safety::Refused("a turn is streaming; wait for it to finish".into()));
        assert!(reset_safety(&session, false).is_safe());
        drop(session);
        let _ = std::fs::remove_dir_all(dir);
    }
}