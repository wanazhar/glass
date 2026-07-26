use super::*;

/// Options for a virtual WebAuthn authenticator.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebAuthnOptions {
    pub protocol: String,
    pub transport: String,
    #[serde(default)]
    pub has_resident_key: bool,
    #[serde(default)]
    pub has_user_verification: bool,
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

pub struct WebAuthnGuard {
    cdp: CdpClient,
    authenticator_id: String,
    armed: bool,
}

impl WebAuthnGuard {
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

    pub fn authenticator_id(&self) -> &str {
        &self.authenticator_id
    }

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
    pub async fn enable_webauthn(&self, options: &WebAuthnOptions) -> BrowserResult<WebAuthnGuard> {
        self.cdp
            .with_current_route(async { WebAuthnGuard::start(self.cdp.clone(), options).await })
            .await
    }
}
