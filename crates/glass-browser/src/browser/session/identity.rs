//! Opt-in RFC 9421-style declared agent identity signing.
//!
//! This module identifies an agent honestly; it does not spoof browser
//! fingerprints, bypass bot protection, or alter the default navigation path.

use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};

const MAX_IDENTITY_TEXT_BYTES: usize = 256;

/// An Ed25519 identity used for explicit HTTP Message Signature requests.
/// The private key is retained only in this value and is never serialized.
pub struct AgentIdentity {
    pub agent: String,
    pub key_id: String,
    pub directory_url: String,
    signing_key: SigningKey,
}

impl std::fmt::Debug for AgentIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentIdentity")
            .field("agent", &self.agent)
            .field("key_id", &self.key_id)
            .field("directory_url", &self.directory_url)
            .finish_non_exhaustive()
    }
}

/// Headers needed to send one signed HTTP request.
#[derive(Debug, Clone, Serialize)]
pub struct SignedHttpRequest {
    #[serde(rename = "signatureAgent")]
    pub signature_agent: String,
    #[serde(rename = "signatureInput")]
    pub signature_input: String,
    pub signature: String,
}

impl AgentIdentity {
    /// Load a 32-byte Ed25519 private key from standard base64.
    pub fn from_base64(
        agent: impl Into<String>,
        key_id: impl Into<String>,
        directory_url: impl Into<String>,
        private_key_base64: &str,
    ) -> BrowserResult<Self> {
        let bytes = STANDARD.decode(private_key_base64.trim())?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "agent identity private key must decode to exactly 32 bytes")?;
        let agent = bounded_identity_text(agent.into(), "agent")?;
        let key_id = bounded_identity_text(key_id.into(), "key_id")?;
        let directory_url = bounded_identity_text(directory_url.into(), "directory_url")?;
        Ok(Self {
            agent,
            key_id,
            directory_url,
            signing_key: SigningKey::from_bytes(&key_bytes),
        })
    }

    /// Sign the RFC 9421 covered components for one explicit request.
    pub fn sign_request(
        &self,
        method: &str,
        target_uri: &str,
        content_digest: Option<&str>,
        created: i64,
    ) -> BrowserResult<SignedHttpRequest> {
        let method = bounded_identity_text(method.to_string(), "method")?;
        if !method.bytes().all(is_http_token_byte) {
            return Err("signature method must be a valid HTTP token".into());
        }
        let target_uri = bounded_identity_text(target_uri.to_string(), "target_uri")?;
        if created < 0 {
            return Err("signature created timestamp must be non-negative".into());
        }
        let mut covered = format!("\"@method\": {method}\n\"@target-uri\": {target_uri}");
        let mut components = "(\"@method\" \"@target-uri\")".to_string();
        if let Some(content_digest) = content_digest {
            let digest = bounded_identity_text(content_digest.to_string(), "content_digest")?;
            covered.push_str(&format!("\n\"content-digest\": {digest}"));
            components = "(\"@method\" \"@target-uri\" \"content-digest\")".to_string();
        }
        let key_id = structured_field_string(&self.key_id, "key_id")?;
        let agent = structured_field_string(&self.agent, "agent")?;
        let parameters =
            format!("{components};created={created};keyid={key_id};alg=\"ed25519\";tag=\"glass\"");
        let signature = STANDARD.encode(self.signing_key.sign(covered.as_bytes()).to_bytes());
        Ok(SignedHttpRequest {
            signature_agent: agent,
            signature_input: format!("sig1={parameters}"),
            signature: format!("sig1=:{signature}:"),
        })
    }
}

impl CdpBrowserSession {
    /// Sign an explicitly supplied request after policy approval.
    pub fn sign_http_request(
        &self,
        identity: &AgentIdentity,
        method: &str,
        target_uri: &str,
        content_digest: Option<&str>,
        created: i64,
    ) -> BrowserResult<SignedHttpRequest> {
        self.policy
            .require(PolicyCapability::DeclaredAgentIdentity)?;
        identity.sign_request(method, target_uri, content_digest, created)
    }
}

fn bounded_identity_text(value: String, field: &str) -> BrowserResult<String> {
    if value.is_empty()
        || value.len() > MAX_IDENTITY_TEXT_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(
            format!(
                "agent identity {field} must be 1..={MAX_IDENTITY_TEXT_BYTES} bytes without control characters"
            )
            .into(),
        );
    }
    Ok(value)
}

fn structured_field_string(value: &str, field: &str) -> BrowserResult<String> {
    if !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) {
        return Err(format!("agent identity {field} must contain only visible ASCII").into());
    }
    let mut escaped = String::with_capacity(value.len().saturating_add(2));
    escaped.push('"');
    for character in value.chars() {
        if matches!(character, '"' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push('"');
    Ok(escaped)
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(agent: &str, key_id: &str) -> BrowserResult<AgentIdentity> {
        AgentIdentity::from_base64(
            agent,
            key_id,
            "https://identity.example/agent",
            &STANDARD.encode([7; 32]),
        )
    }

    #[test]
    fn signed_identity_rejects_control_characters_and_invalid_methods() {
        assert!(identity("agent\r\nInjected: yes", "key").is_err());
        assert!(identity("agent", "key\nInjected").is_err());

        let identity = identity("agent", "key").unwrap();
        assert!(
            identity
                .sign_request(
                    "GET\n\"@target-uri\": https://attacker.test",
                    "https://example.test/",
                    None,
                    1
                )
                .is_err()
        );
        assert!(
            identity
                .sign_request("GET", "https://example.test/\n\"@method\": DELETE", None, 1)
                .is_err()
        );
        assert!(
            identity
                .sign_request(
                    "GET",
                    "https://example.test/",
                    Some("sha-256=:abc:\nInjected: yes"),
                    1
                )
                .is_err()
        );
    }

    #[test]
    fn signed_identity_escapes_structured_field_strings() {
        let identity = identity("agent \"one\"\\two", "key \"id\"\\suffix").unwrap();

        let signed = identity
            .sign_request("GET", "https://example.test/path", None, 1)
            .unwrap();

        assert_eq!(signed.signature_agent, "\"agent \\\"one\\\"\\\\two\"");
        assert!(
            signed
                .signature_input
                .contains("keyid=\"key \\\"id\\\"\\\\suffix\"")
        );
    }

    #[test]
    fn signed_identity_preserves_method_case_and_rejects_non_tokens() {
        let identity = identity("agent", "key").unwrap();

        let signed = identity
            .sign_request("GET", "https://example.test/", None, 1)
            .unwrap();
        let expected_signature = STANDARD.encode(
            identity
                .signing_key
                .sign(b"\"@method\": GET\n\"@target-uri\": https://example.test/")
                .to_bytes(),
        );

        assert_eq!(signed.signature, format!("sig1=:{expected_signature}:"));
        assert_ne!(
            signed.signature,
            identity
                .sign_request("get", "https://example.test/", None, 1)
                .unwrap()
                .signature
        );
        assert!(
            identity
                .sign_request("GET TEST", "https://example.test/", None, 1)
                .is_err()
        );
    }
}
