//! Session-local, short-lived secret handles for explicitly authorized host tools.
//!
//! Secret values never become tool arguments or transcript records. A consumer
//! must present the exact session, tool, operation, and HTTPS origin that were
//! shown to the human, and each handle can be resolved once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use zeroize::Zeroizing;

const MAX_ENTRIES: usize = 32;
pub(crate) const MAX_SECRET_BYTES: usize = 4096;
const SECRET_TTL: Duration = Duration::from_secs(5 * 60);
const WEB_SECRET_TOOL: &str = "web_secret_request";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SecretRequest {
    pub purpose: String,
    pub destination: String,
    pub tool_name: String,
    pub operation: String,
}

impl SecretRequest {
    pub(crate) fn parse(value: &Value) -> Result<Self, String> {
        let text = |key: &str, max: usize| -> Result<String, String> {
            let value = value
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{key} is required"))?;
            if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
                return Err(format!("{key} is invalid or exceeds its size limit"));
            }
            Ok(value.to_owned())
        };
        let purpose = text("purpose", 512)?;
        let destination = canonical_origin(&text("destination", 2048)?)?;
        let tool_name = text("tool", 128)?;
        if tool_name != WEB_SECRET_TOOL {
            return Err("Secrets are currently limited to the HTTPS web request tool".into());
        }
        let operation = text("operation", 256)?;
        Ok(Self {
            purpose,
            destination,
            tool_name,
            operation,
        })
    }

    pub(crate) fn scope(&self, session_id: &str) -> SecretScope {
        SecretScope {
            session_id: session_id.to_owned(),
            tool_name: self.tool_name.clone(),
            operation: self.operation.clone(),
            destination: self.destination.clone(),
        }
    }
}

pub(crate) fn request_schema() -> (String, String, String) {
    (
        "request_secret".into(),
        "Ask the human for one secret needed by a registered host tool. The dedicated prompt masks the value and binds its short-lived handle to this session, tool, operation, and HTTPS origin. The value is never returned to you; pass only the returned secretRef to that exact host tool. Call this alone and do not request secrets for ordinary questions or unregistered tools.".into(),
        json!({
            "type":"object",
            "additionalProperties":false,
            "required":["purpose","destination","tool","operation"],
            "properties":{
                "purpose":{"type":"string","minLength":1,"maxLength":512},
                "destination":{"type":"string","format":"uri","maxLength":2048},
                "tool":{"type":"string","minLength":1,"maxLength":128},
                "operation":{"type":"string","minLength":1,"maxLength":256}
            }
        })
        .to_string(),
    )
}

pub(crate) fn revoke_schema() -> (String, String, String) {
    (
        "revoke_secret".into(),
        "Revoke an unused secretRef from this session before its HTTPS request is sent. This invalidates the reference immediately; it cannot be used again.".into(),
        json!({
            "type":"object",
            "additionalProperties":false,
            "required":["secretRef"],
            "properties":{"secretRef":{"type":"string","minLength":16,"maxLength":96}}
        }).to_string(),
    )
}

pub(crate) fn prompt(
    request: &SecretRequest,
    keys: &crate::input::KeyReader,
) -> Option<Zeroizing<String>> {
    let label = format!(
        "{}\nDestination: {}\nTool: {} · {}",
        request.purpose, request.destination, request.tool_name, request.operation
    );
    crate::modal::validated_secret_field(keys, "Secret requested", &label, "", |value| {
        if value.trim().is_empty() || value.len() > MAX_SECRET_BYTES {
            Err(format!("Enter a value up to {MAX_SECRET_BYTES} bytes"))
        } else {
            Ok(())
        }
    })
}

fn canonical_origin(value: &str) -> Result<String, String> {
    let url = url::Url::parse(value).map_err(|_| "Destination must be an HTTPS origin")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || (!url.path().is_empty() && url.path() != "/")
    {
        return Err("Destination must be an HTTPS origin without credentials or a path".into());
    }
    Ok(url.origin().ascii_serialization())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SecretScope {
    pub session_id: String,
    pub tool_name: String,
    pub operation: String,
    pub destination: String,
}

struct Entry {
    value: Zeroizing<String>,
    scope: SecretScope,
    expires_at: Instant,
    used: bool,
    revoked: bool,
}

/// An in-memory vault owned by one GC session. References are random handles,
/// not encoded or derived from the secret value. Expired, revoked and consumed
/// entries remain bounded redactors until vault drop; they never regain use.
#[derive(Clone, Default)]
pub(crate) struct SecretVault {
    // Runtime subagent observers may execute on worker threads. Cloned vault
    // handles share the live session registry so secrets requested earlier
    // in the same turn are still redacted by child-event observers.
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    // Parent-session values copied into a child session for redaction only.
    // They have no handles/scopes in this vault and cannot be consumed.
    inherited_redactors: Vec<Zeroizing<String>>,
}

/// Stateful redaction for provider text that may split one known value across
/// multiple stream events.
#[derive(Default)]
pub(crate) struct SecretTextStream {
    pending: Zeroizing<String>,
}

impl SecretTextStream {
    pub(crate) fn push(&mut self, secrets: &SecretVault, chunk: &str) -> String {
        self.pending.push_str(chunk);
        self.pending = Zeroizing::new(secrets.redact(&self.pending));
        let safe_end = secrets.stream_safe_prefix_len(&self.pending);
        let safe = self.pending[..safe_end].to_string();
        self.pending.drain(..safe_end);
        safe
    }

    /// Flush remaining text, replacing any incomplete secret prefix at the
    /// stream boundary instead of exposing a fragment when a response stops.
    pub(crate) fn finish(&mut self, secrets: &SecretVault) -> String {
        self.pending = Zeroizing::new(secrets.redact(&self.pending));
        let safe_end = secrets.stream_safe_prefix_len(&self.pending);
        let mut output = self.pending[..safe_end].to_string();
        if safe_end < self.pending.len() {
            output.push_str("[redacted secret]");
        }
        self.pending = Zeroizing::default();
        output
    }
}

impl SecretVault {
    fn entries(&self) -> MutexGuard<'_, HashMap<String, Entry>> {
        match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => {
                let entries = poisoned.into_inner();
                self.entries.clear_poison();
                entries
            }
        }
    }

    pub(crate) fn has_redactors(&self) -> bool {
        !self.entries().is_empty() || !self.inherited_redactors.is_empty()
    }

    /// Copy another session's current values as redactors only. Child tasks
    /// must not retain parent handles or gain authority to consume them.
    pub(crate) fn inherit_redactors(&mut self, parent: &SecretVault) {
        let parent_entries = parent.entries();
        self.inherited_redactors.extend(
            parent_entries
                .values()
                .map(|entry| Zeroizing::new(entry.value.to_string())),
        );
        self.inherited_redactors.extend(
            parent
                .inherited_redactors
                .iter()
                .map(|value| Zeroizing::new(value.to_string())),
        );
    }

    /// Largest prefix that can be safely emitted now. The caller first
    /// replaces complete matches; this holds only a trailing partial value
    /// that a later chunk could complete.
    pub(crate) fn stream_safe_prefix_len(&self, text: &str) -> usize {
        let entries = self.entries();
        let values = entries
            .values()
            .map(|entry| entry.value.as_str())
            .chain(self.inherited_redactors.iter().map(|value| value.as_str()));
        let mut held = 0;
        for value in values {
            let max_prefix = value.len().saturating_sub(1).min(text.len());
            for prefix_len in (1..=max_prefix).rev() {
                if !value.is_char_boundary(prefix_len)
                    || !text.is_char_boundary(text.len() - prefix_len)
                {
                    continue;
                }
                if text.ends_with(&value[..prefix_len]) {
                    held = held.max(prefix_len);
                    break;
                }
            }
        }
        text.len() - held
    }

    pub(crate) fn issue_zeroizing(
        &mut self,
        scope: SecretScope,
        value: Zeroizing<String>,
    ) -> Result<String, String> {
        self.issue_sensitive(scope, value, Instant::now())
    }

    #[cfg(test)]
    fn issue_at(
        &mut self,
        scope: SecretScope,
        value: String,
        now: Instant,
    ) -> Result<String, String> {
        self.issue_sensitive(scope, Zeroizing::new(value), now)
    }

    fn issue_sensitive(
        &mut self,
        scope: SecretScope,
        value: Zeroizing<String>,
        now: Instant,
    ) -> Result<String, String> {
        if !valid_scope(&scope)
            || value.trim().is_empty()
            || value.len() > MAX_SECRET_BYTES
            || value.chars().any(char::is_control)
        {
            return Err("Secret request is invalid or outside supported limits".into());
        }
        let mut entries = self.entries();
        if entries.len() >= MAX_ENTRIES {
            return Err(
                "Session secret limit reached; start a new session for additional secrets".into(),
            );
        }
        let handle = random_handle()?;
        entries.insert(
            handle.clone(),
            Entry {
                value,
                scope,
                expires_at: now + SECRET_TTL,
                used: false,
                revoked: false,
            },
        );
        Ok(handle)
    }

    /// Consume a handle for the exact HTTPS request operation it authorized.
    /// This is deliberately the only value extraction path; callers pass the
    /// returned zeroizing value directly to the bounded web consumer.
    pub(crate) fn take_web_request_secret(
        &mut self,
        handle: &str,
        session_id: &str,
        operation: &str,
    ) -> Result<(Zeroizing<String>, String), String> {
        self.take_web_request_secret_at(handle, session_id, operation, Instant::now())
    }

    fn take_web_request_secret_at(
        &mut self,
        handle: &str,
        session_id: &str,
        operation: &str,
        now: Instant,
    ) -> Result<(Zeroizing<String>, String), String> {
        let mut entries = self.entries();
        let entry = entries
            .get_mut(handle)
            .ok_or_else(|| "Secret handle is missing, expired, or revoked".to_owned())?;
        if entry.revoked || now >= entry.expires_at {
            return Err("Secret handle is missing, expired, or revoked".into());
        }
        if entry.scope.session_id != session_id
            || entry.scope.tool_name != WEB_SECRET_TOOL
            || entry.scope.operation != operation
        {
            return Err("Secret handle is not authorized for this operation".into());
        }
        if entry.used {
            return Err("Secret handle has already been used".into());
        }
        entry.used = true;
        Ok((
            Zeroizing::new(entry.value.to_string()),
            entry.scope.destination.clone(),
        ))
    }

    #[cfg(test)]
    fn with_secret_at<T>(
        &mut self,
        handle: &str,
        scope: &SecretScope,
        now: Instant,
        use_value: impl FnOnce(&str) -> T,
    ) -> Result<T, String> {
        let mut entries = self.entries();
        let entry = entries
            .get_mut(handle)
            .ok_or_else(|| "Secret handle is missing, expired, or revoked".to_owned())?;
        if entry.revoked || now >= entry.expires_at {
            return Err("Secret handle is missing, expired, or revoked".into());
        }
        if entry.scope != *scope {
            return Err("Secret handle is not authorized for this operation".into());
        }
        if entry.used {
            return Err("Secret handle has already been used".into());
        }
        entry.used = true;
        Ok(use_value(&entry.value))
    }

    pub(crate) fn revoke(&mut self, handle: &str) -> bool {
        let mut entries = self.entries();
        let Some(entry) = entries.get_mut(handle) else {
            return false;
        };
        if entry.used || entry.revoked || Instant::now() >= entry.expires_at {
            return false;
        }
        entry.revoked = true;
        true
    }

    #[cfg(test)]
    pub(crate) fn revoke_session(&mut self, session_id: &str) {
        for entry in self.entries().values_mut() {
            if entry.scope.session_id == session_id {
                entry.revoked = true;
            }
        }
    }

    /// Remove exact secret values before recording tool output or sending it
    /// to the provider. Handle expiry/revocation limits use, not redaction:
    /// values remain zeroizing and bounded until this session vault is dropped.
    pub(crate) fn redact(&self, text: &str) -> String {
        let entries = self.entries();
        let mut values = entries
            .values()
            .map(|entry| entry.value.as_str())
            .chain(self.inherited_redactors.iter().map(|value| value.as_str()))
            .collect::<Vec<_>>();
        values.sort_unstable_by(|left, right| {
            right.len().cmp(&left.len()).then_with(|| left.cmp(right))
        });
        values.dedup();
        values.into_iter().fold(text.to_owned(), |output, value| {
            output.replace(value, "[redacted secret]")
        })
    }
}

fn valid_scope(scope: &SecretScope) -> bool {
    !scope.session_id.is_empty()
        && scope.session_id.len() <= 128
        && !scope.tool_name.is_empty()
        && scope.tool_name.len() <= 128
        && !scope.operation.trim().is_empty()
        && scope.operation.len() <= 256
        && scope.operation.chars().all(|ch| !ch.is_control())
        && canonical_origin(&scope.destination)
            .is_ok_and(|destination| destination == scope.destination)
}

fn random_handle() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .map_err(|_| "Secure randomness is unavailable".to_owned())?;
    Ok(format!(
        "secret_{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> SecretScope {
        SecretScope {
            session_id: "session-a".into(),
            tool_name: "web_secret_request".into(),
            operation: "create staging token".into(),
            destination: "https://api.example.test".into(),
        }
    }

    #[test]
    fn secret_request_accepts_only_bounded_https_origins() {
        let request = SecretRequest::parse(&json!({
            "purpose":"Connect to the staging API",
            "destination":"https://api.example.test/",
            "tool":"web_secret_request",
            "operation":"create staging token"
        }))
        .unwrap();
        assert_eq!(request.destination, "https://api.example.test");
        for destination in [
            "http://api.example.test",
            "https://user:pass@api.example.test",
            "https://api.example.test/path",
            "https://api.example.test/?token=private",
        ] {
            assert!(
                SecretRequest::parse(&json!({
                    "purpose":"Connect",
                    "destination":destination,
                    "tool":"web_secret_request",
                    "operation":"create token"
                }))
                .is_err()
            );
        }
    }

    #[test]
    fn tui_secret_prompt_accepts_pasted_value_through_masked_input() {
        use glass_tui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        let keys = crate::input::KeyReader::from_test_events([
            Event::Paste("tui-secret-sentinel".into()),
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ]);
        let request = SecretRequest {
            purpose: "Connect to staging".into(),
            destination: "https://api.example.test".into(),
            tool_name: WEB_SECRET_TOOL.into(),
            operation: "create token".into(),
        };
        let value = prompt(&request, &keys);
        assert!(value.as_ref().map(|value| value.as_str()) == Some("tui-secret-sentinel"));
    }

    #[test]
    fn secret_handle_is_random_scoped_and_single_use() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        let handle = vault
            .issue_at(scope(), "secret-sentinel".into(), now)
            .unwrap();
        assert!(!handle.contains("secret-sentinel"));
        let value = vault
            .with_secret_at(&handle, &scope(), now, |secret| secret.to_owned())
            .unwrap();
        assert_eq!(value, "secret-sentinel");
        assert!(
            vault
                .with_secret_at(&handle, &scope(), now, |_| ())
                .is_err()
        );
    }

    #[test]
    fn web_secret_resolver_pins_destination_session_operation_and_single_use() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        let handle = vault
            .issue_at(scope(), "secret-sentinel".into(), now)
            .unwrap();
        assert!(
            vault
                .take_web_request_secret_at(&handle, "session-b", "create staging token", now)
                .is_err()
        );
        assert!(
            vault
                .take_web_request_secret_at(&handle, "session-a", "other operation", now)
                .is_err()
        );
        let (value, destination) = vault
            .take_web_request_secret_at(&handle, "session-a", "create staging token", now)
            .unwrap();
        assert_eq!(value.as_str(), "secret-sentinel");
        assert_eq!(destination, "https://api.example.test");
        assert!(!vault.revoke(&handle));
        assert_eq!(
            vault.redact("echo secret-sentinel"),
            "echo [redacted secret]"
        );
        assert!(
            vault
                .take_web_request_secret_at(&handle, "session-a", "create staging token", now,)
                .is_err()
        );
    }

    #[test]
    fn secret_handle_rejects_cross_session_tool_operation_and_destination() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        let handle = vault
            .issue_at(scope(), "secret-sentinel".into(), now)
            .unwrap();
        for changed in [
            SecretScope {
                session_id: "session-b".into(),
                ..scope()
            },
            SecretScope {
                tool_name: "other.tool".into(),
                ..scope()
            },
            SecretScope {
                operation: "different operation".into(),
                ..scope()
            },
            SecretScope {
                destination: "https://other.example.test".into(),
                ..scope()
            },
        ] {
            assert!(
                vault
                    .with_secret_at(&handle, &changed, now, |_| ())
                    .is_err()
            );
        }
        assert!(vault.with_secret_at(&handle, &scope(), now, |_| ()).is_ok());
    }

    #[test]
    fn expiry_revoke_and_session_close_remove_secret_handles() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        let expired = vault
            .issue_at(scope(), "expired-secret".into(), now)
            .unwrap();
        let scope_b = SecretScope {
            session_id: "session-b".into(),
            ..scope()
        };
        let revoked = vault
            .issue_at(scope_b.clone(), "revoked-secret".into(), now)
            .unwrap();
        let session = vault
            .issue_at(scope(), "session-secret".into(), now)
            .unwrap();
        assert!(vault.revoke(&revoked));
        vault.revoke_session("session-a");
        let later = now + SECRET_TTL;
        assert!(
            vault
                .with_secret_at(&expired, &scope(), later, |_| ())
                .is_err()
        );
        assert!(
            vault
                .with_secret_at(&revoked, &scope_b, now, |_| ())
                .is_err()
        );
        assert!(
            vault
                .with_secret_at(&session, &scope(), now, |_| ())
                .is_err()
        );
    }

    #[test]
    fn redaction_keeps_consumed_secret_out_of_tool_output() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        let handle = vault
            .issue_at(scope(), "secret-sentinel".into(), now)
            .unwrap();
        drop(
            vault
                .take_web_request_secret_at(&handle, "session-a", "create staging token", now)
                .unwrap(),
        );
        assert_eq!(
            vault.redact("server echoed secret-sentinel"),
            "server echoed [redacted secret]"
        );
    }

    #[test]
    fn expired_and_revoked_handles_remain_redacted_for_the_session() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        let expired = vault
            .issue_at(scope(), "expired-sentinel".into(), now - SECRET_TTL)
            .unwrap();
        let revoked = vault
            .issue_at(scope(), "revoked-sentinel".into(), now)
            .unwrap();
        let consumed = vault
            .issue_at(scope(), "consumed-sentinel".into(), now)
            .unwrap();
        drop(
            vault
                .take_web_request_secret_at(&consumed, "session-a", "create staging token", now)
                .unwrap(),
        );
        assert!(vault.revoke(&revoked));
        assert!(!vault.revoke(&revoked));
        for handle in [&expired, &revoked, &consumed] {
            assert!(
                vault
                    .take_web_request_secret_at(
                        handle,
                        "session-a",
                        "create staging token",
                        now + SECRET_TTL
                    )
                    .is_err()
            );
        }
        assert_eq!(
            vault.redact("later echo: expired-sentinel revoked-sentinel consumed-sentinel"),
            "later echo: [redacted secret] [redacted secret] [redacted secret]"
        );
    }

    #[test]
    fn session_redaction_limit_never_evicts_expired_or_revoked_values() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        for index in 0..MAX_ENTRIES {
            let handle = vault
                .issue_at(scope(), format!("retained-{index:02}-sentinel"), now)
                .unwrap();
            assert!(vault.revoke(&handle));
        }
        assert!(
            vault
                .issue_at(scope(), "new-sentinel".into(), now + SECRET_TTL)
                .is_err()
        );
        assert_eq!(vault.entries().len(), MAX_ENTRIES);
        for index in 0..MAX_ENTRIES {
            assert_eq!(
                vault.redact(&format!("retained-{index:02}-sentinel")),
                "[redacted secret]"
            );
        }
    }

    #[test]
    fn overlapping_secret_values_redact_the_longest_value_first() {
        let now = Instant::now();
        let mut vault = SecretVault::default();
        vault
            .issue_at(scope(), "shared-prefix".into(), now)
            .unwrap();
        vault
            .issue_at(scope(), "shared-prefix-sensitive-suffix".into(), now)
            .unwrap();
        assert_eq!(
            vault.redact("shared-prefix-sensitive-suffix shared-prefix"),
            "[redacted secret] [redacted secret]"
        );
    }

    #[test]
    fn cloned_live_handle_redacts_secrets_added_after_the_clone() {
        let mut session_vault = SecretVault::default();
        let observer_vault = session_vault.clone();
        session_vault
            .issue_at(
                scope(),
                "parent-secret-added-mid-turn".into(),
                Instant::now(),
            )
            .unwrap();

        assert_eq!(
            observer_vault.redact("echo parent-secret-added-mid-turn"),
            "echo [redacted secret]"
        );
    }

    #[test]
    fn inherited_values_are_redaction_only_and_stream_boundaries_hold_partial_values() {
        let mut parent = SecretVault::default();
        parent
            .issue_at(scope(), "parent-secret-sentinel".into(), Instant::now())
            .unwrap();
        let mut child = SecretVault::default();
        child.inherit_redactors(&parent);

        assert!(child.has_redactors());
        assert_eq!(child.redact("parent-secret-sentinel"), "[redacted secret]");
        assert_eq!(
            child.stream_safe_prefix_len("before parent-secret-"),
            "before ".len()
        );
        assert_eq!(
            child.redact(
                &"before parent-secret-"[..child.stream_safe_prefix_len("before parent-secret-")]
            ),
            "before "
        );
        let mut stream = SecretTextStream::default();
        assert_eq!(stream.push(&child, "before parent-secret-"), "before ");
        assert_eq!(stream.finish(&child), "[redacted secret]");
        let complete = child.redact("parent-secret-sentinel after");
        assert_eq!(child.stream_safe_prefix_len(&complete), complete.len());

        // Inherited values do not create handles that the child can consume.
        assert!(
            child
                .take_web_request_secret("unknown", "child-session", "create staging token")
                .is_err()
        );
    }
}
