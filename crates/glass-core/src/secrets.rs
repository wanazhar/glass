//! Session-local, short-lived secret handles for explicitly authorized host tools.
//!
//! Secret values never become tool arguments or transcript records. A consumer
//! must present the exact session, tool, operation, and HTTPS origin that were
//! shown to the human, and each handle can be resolved once.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use zeroize::Zeroizing;

const MAX_ENTRIES: usize = 32;
const MAX_SECRET_BYTES: usize = 4096;
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

pub(crate) fn prompt(request: &SecretRequest, keys: &crate::input::KeyReader) -> Option<String> {
    let label = format!(
        "{}\nDestination: {}\nTool: {} · {}",
        request.purpose, request.destination, request.tool_name, request.operation
    );
    crate::modal::validated_field(keys, "Secret requested", &label, "", true, |value| {
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
}

/// An in-memory vault owned by one GC session. References are random handles,
/// not encoded or derived from the secret value.
#[derive(Default)]
pub(crate) struct SecretVault {
    entries: HashMap<String, Entry>,
}

impl SecretVault {
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
        self.purge_at(now);
        if !valid_scope(&scope)
            || value.trim().is_empty()
            || value.len() > MAX_SECRET_BYTES
            || value.chars().any(char::is_control)
        {
            return Err("Secret request is invalid or outside supported limits".into());
        }
        if self.entries.len() >= MAX_ENTRIES {
            return Err(
                "Too many unexpired secret requests; revoke an existing handle first".into(),
            );
        }
        let handle = random_handle()?;
        self.entries.insert(
            handle.clone(),
            Entry {
                value,
                scope,
                expires_at: now + SECRET_TTL,
                used: false,
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
        self.purge_at(now);
        let entry = self
            .entries
            .get_mut(handle)
            .ok_or_else(|| "Secret handle is missing, expired, or revoked".to_owned())?;
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
        self.purge_at(now);
        let entry = self
            .entries
            .get_mut(handle)
            .ok_or_else(|| "Secret handle is missing, expired, or revoked".to_owned())?;
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
        self.entries.remove(handle).is_some()
    }

    #[cfg(test)]
    pub(crate) fn revoke_session(&mut self, session_id: &str) {
        self.entries
            .retain(|_, entry| entry.scope.session_id != session_id);
    }

    /// Remove exact secret values from tool output before it is recorded or
    /// sent to the provider. Used values remain available for redaction until
    /// their short TTL ends, but can no longer be resolved.
    pub(crate) fn redact(&mut self, text: &str) -> String {
        self.purge_at(Instant::now());
        self.entries
            .values()
            .fold(text.to_owned(), |output, entry| {
                output.replace(entry.value.as_str(), "[redacted secret]")
            })
    }

    fn purge_at(&mut self, now: Instant) {
        self.entries.retain(|_, entry| entry.expires_at > now);
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
}
