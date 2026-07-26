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
        let method = bounded_identity_text(method.to_ascii_lowercase(), "method")?;
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
        let parameters = format!(
            "{components};created={created};keyid=\"{}\";alg=\"ed25519\";tag=\"glass\"",
            self.key_id
        );
        let signature = STANDARD.encode(self.signing_key.sign(covered.as_bytes()).to_bytes());
        Ok(SignedHttpRequest {
            signature_agent: format!("\"{}\"", self.agent),
            signature_input: format!("sig1={parameters}"),
            signature: format!("sig1=:{signature}:"),
        })
    }
}

impl BrowserSession {
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
    if value.is_empty() || value.len() > MAX_IDENTITY_TEXT_BYTES {
        return Err(
            format!("agent identity {field} must be 1..={MAX_IDENTITY_TEXT_BYTES} bytes").into(),
        );
    }
    Ok(value)
}
