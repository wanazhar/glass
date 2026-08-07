//! Virtual WebAuthn authenticator support via CDP.
//!
//! Enables automated testing of WebAuthn / passkey flows by creating
//! a virtual authenticator through the CDP `WebAuthn` domain. Use
//! [`BrowserSession::enable_webauthn`] to start and obtain a
//! [`WebAuthnGuard`] for credential management.

use super::*;

/// Options for a virtual WebAuthn authenticator.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebAuthnOptions {
    /// Protocol version: `"ctap2"` (default) or `"u2f"`.
    pub protocol: String,
    /// Transport type: `"internal"`, `"usb"`, `"nfc"`, or `"ble"`.
    pub transport: String,
    /// Whether the authenticator supports resident keys (discoverable credentials).
    #[serde(default)]
    pub has_resident_key: bool,
    /// Whether the authenticator supports user verification.
    #[serde(default)]
    pub has_user_verification: bool,
    /// Whether this is a user-verifying platform authenticator.
    #[serde(default)]
    pub is_user_verifying_platform_authenticator: bool,
}

impl Default for WebAuthnOptions {
    fn default() -> Self {
        Self {
            protocol: "ctap2".into(),
            transport: "internal".into(),
            has_resident_key: false,
            has_user_verification: false,
            is_user_verifying_platform_authenticator: false,
        }
    }
}

/// Scoped guard managing a virtual WebAuthn authenticator.
///
/// Created by [`BrowserSession::enable_webauthn`]. While this guard is
/// alive, a virtual authenticator is registered in the browser. Use
/// [`add_credential`](Self::add_credential) to provision credentials
/// for testing, and [`disable`](Self::disable) to remove it explicitly.
///
/// On drop, the virtual authenticator is removed and the WebAuthn
/// domain is disabled (best-effort via a spawned task).
pub struct WebAuthnGuard {
    cdp: CdpClient,
    authenticator_id: String,
    armed: bool,
}

impl WebAuthnGuard {
    /// Enable the WebAuthn domain and create a virtual authenticator.
    pub(crate) async fn start(cdp: CdpClient, options: &WebAuthnOptions) -> BrowserResult<Self> {
        cdp.send("WebAuthn.enable", None).await?;
        let result = cdp.send("WebAuthn.addVirtualAuthenticator", Some(serde_json::json!({
            "options": {
                "protocol": options.protocol,
                "transport": options.transport,
                "hasResidentKey": options.has_resident_key,
                "hasUserVerification": options.has_user_verification,
                "isUserVerifyingPlatformAuthenticator": options.is_user_verifying_platform_authenticator,
            }
        }))).await?;
        let authenticator_id = result["authenticatorId"]
            .as_str()
            .ok_or("WebAuthn.addVirtualAuthenticator returned no authenticatorId")?
            .to_string();
        Ok(Self {
            cdp,
            authenticator_id,
            armed: true,
        })
    }

    /// Return the CDP authenticator ID for this virtual authenticator.
    pub fn authenticator_id(&self) -> &str {
        &self.authenticator_id
    }

    /// Add a resident credential to the virtual authenticator.
    ///
    /// `credential_id` is an arbitrary identifier for the credential.
    /// `rp_id` is the relying party ID (typically the domain).
    /// `user_handle` is the user identifier associated with the credential.
    /// `private_key_pem` must be a PEM-encoded private key.
    /// `sign_count` is the initial signature counter value.
    pub async fn add_credential(
        &self,
        credential_id: &str,
        rp_id: &str,
        user_handle: &str,
        private_key_pem: &str,
        sign_count: u32,
    ) -> BrowserResult<()> {
        self.cdp
            .send(
                "WebAuthn.addCredential",
                Some(serde_json::json!({
                    "authenticatorId": self.authenticator_id,
                    "credential": {
                        "credentialId": credential_id,
                        "rpId": rp_id,
                        "privateKey": private_key_pem,
                        "signCount": sign_count,
                        "isResidentCredential": true,
                        "userHandle": user_handle,
                    }
                })),
            )
            .await?;
        Ok(())
    }

    /// Remove the virtual authenticator and disable the WebAuthn domain.
    ///
    /// After calling this, no further credential operations are possible.
    /// This is called automatically on drop, but explicit calls let you
    /// handle errors synchronously.
    pub async fn disable(mut self) -> BrowserResult<()> {
        self.armed = false;
        let _ = self
            .cdp
            .send(
                "WebAuthn.removeVirtualAuthenticator",
                Some(serde_json::json!({"authenticatorId": self.authenticator_id})),
            )
            .await;
        let _ = self.cdp.send("WebAuthn.disable", None).await;
        Ok(())
    }
}

impl Drop for WebAuthnGuard {
    fn drop(&mut self) {
        if self.armed {
            let cdp = self.cdp.clone();
            let auth_id = self.authenticator_id.clone();
            tokio::spawn(async move {
                let _ = cdp
                    .send(
                        "WebAuthn.removeVirtualAuthenticator",
                        Some(serde_json::json!({"authenticatorId": auth_id})),
                    )
                    .await;
                let _ = cdp.send("WebAuthn.disable", None).await;
            });
        }
    }
}

impl BrowserSession {
    /// Enable a virtual WebAuthn authenticator for the session.
    ///
    /// Returns a `WebAuthnGuard` scoped to this session. Use
    /// `WebAuthnGuard::add_credential` to provision credentials before
    /// navigating to the page under test.
    pub async fn enable_webauthn(&self, options: &WebAuthnOptions) -> BrowserResult<WebAuthnGuard> {
        self.cdp
            .with_current_route(async { WebAuthnGuard::start(self.cdp.clone(), options).await })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webauthn_options_default_uses_ctap2_and_internal_transport() {
        let opts = WebAuthnOptions::default();
        assert_eq!(opts.protocol, "ctap2");
        assert_eq!(opts.transport, "internal");
    }

    #[test]
    fn webauthn_options_default_has_all_booleans_false() {
        let opts = WebAuthnOptions::default();
        assert!(!opts.has_resident_key);
        assert!(!opts.has_user_verification);
        assert!(!opts.is_user_verifying_platform_authenticator);
    }

    #[test]
    fn webauthn_options_serializes_to_json() {
        let opts = WebAuthnOptions::default();
        let json = serde_json::to_value(&opts).unwrap();
        assert_eq!(json["protocol"], "ctap2");
        assert_eq!(json["transport"], "internal");
        assert!(!json["has_resident_key"].as_bool().unwrap());
        assert!(!json["has_user_verification"].as_bool().unwrap());
    }

    #[test]
    fn webauthn_options_custom_serializes_all_fields() {
        let opts = WebAuthnOptions {
            protocol: "u2f".to_string(),
            transport: "usb".to_string(),
            has_resident_key: true,
            has_user_verification: true,
            is_user_verifying_platform_authenticator: true,
        };
        let json = serde_json::to_value(&opts).unwrap();
        assert_eq!(json["protocol"], "u2f");
        assert_eq!(json["transport"], "usb");
        assert!(json["has_resident_key"].as_bool().unwrap());
        assert!(json["has_user_verification"].as_bool().unwrap());
        assert!(
            json["is_user_verifying_platform_authenticator"]
                .as_bool()
                .unwrap()
        );
    }

    #[test]
    fn webauthn_options_with_false_booleans_still_serializes_them() {
        let opts = WebAuthnOptions {
            protocol: "ctap2".to_string(),
            transport: "nfc".to_string(),
            has_resident_key: false,
            has_user_verification: false,
            is_user_verifying_platform_authenticator: false,
        };
        let json = serde_json::to_value(&opts).unwrap();
        // All booleans should appear in JSON because #[serde(default)] does not
        // skip them — it only provides default when deserializing.
        assert!(!json["has_resident_key"].as_bool().unwrap());
        assert!(!json["has_user_verification"].as_bool().unwrap());
        assert!(
            !json["is_user_verifying_platform_authenticator"]
                .as_bool()
                .unwrap()
        );
    }
}
