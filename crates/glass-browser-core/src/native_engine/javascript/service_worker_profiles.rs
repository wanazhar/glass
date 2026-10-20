use super::*;

pub(crate) fn validate_service_worker_client_states(
    states: &[NativeServiceWorkerClientState],
) -> Result<(), NativeEngineError> {
    if states.len() > MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES {
        return Err(NativeEngineError::limit(
            "native service worker clients",
            MAX_NATIVE_SERVICE_WORKER_CLIENT_LEASES,
            states.len(),
        ));
    }
    let mut ids = BTreeSet::new();
    for state in states {
        validate_context_id(&state.id)?;
        if state.id.is_empty() {
            return Err(NativeEngineError::invalid(
                "native service worker client id",
                "must not be empty",
            ));
        }
        if !ids.insert(state.id.clone()) {
            return Err(NativeEngineError::invalid(
                "native service worker client id",
                "must be unique within the client projection",
            ));
        }
        validate_url_text("native service worker client URL", &state.url)?;
        if state.url.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker client URL",
                MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                state.url.len(),
            ));
        }
        if !matches!(
            state.client_type.as_str(),
            "window" | "worker" | "sharedworker"
        ) {
            return Err(NativeEngineError::invalid(
                "native service worker client type",
                "must be window, worker, or sharedworker",
            ));
        }
        if !matches!(
            state.frame_type.as_str(),
            "top-level" | "nested" | "auxiliary" | "none"
        ) {
            return Err(NativeEngineError::invalid(
                "native service worker client frame type",
                "must be top-level, nested, auxiliary, or none",
            ));
        }
        if !matches!(
            state.visibility_state.as_str(),
            "visible" | "hidden" | "prerender"
        ) {
            return Err(NativeEngineError::invalid(
                "native service worker client visibility state",
                "must be visible, hidden, or prerender",
            ));
        }
    }
    Ok(())
}

/// Persistent metadata for one activated Service Worker registration. The
/// worker source remains owned by the network/resource policy and is loaded
/// again when a content-process owner is restored.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeServiceWorkerRegistrationProfile {
    pub(crate) script_url: String,
    pub(crate) scope: String,
    #[serde(default = "default_classic_worker_type")]
    pub(crate) worker_type: String,
    #[serde(default)]
    pub(crate) navigation_preload_enabled: bool,
    #[serde(default = "default_navigation_preload_header_value")]
    pub(crate) navigation_preload_header_value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) waiting: Option<NativeServiceWorkerWorkerProfile>,
}

/// Persistent metadata for an installed Service Worker waiting to replace the
/// activated registration. The runtime and pending event state remain
/// process-owned and are recreated from this descriptor after restart.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct NativeServiceWorkerWorkerProfile {
    pub(crate) script_url: String,
    #[serde(default = "default_classic_worker_type")]
    pub(crate) worker_type: String,
}

impl NativeServiceWorkerRegistrationProfile {
    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        if self.script_url.is_empty() || self.script_url.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker registration script URL",
                MAX_NATIVE_SCRIPT_BYTES,
                self.script_url.len(),
            ));
        }
        if self.scope.is_empty() || self.scope.len() > MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES {
            return Err(NativeEngineError::limit(
                "native service worker registration scope",
                MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES,
                self.scope.len(),
            ));
        }
        validate_navigation_preload_header_value(&self.navigation_preload_header_value)?;
        let script = Url::parse(without_fragment(&self.script_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "native service worker registration script URL is invalid".into(),
            }
        })?;
        let scope = Url::parse(without_fragment(&self.scope)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "native service worker registration scope URL is invalid".into(),
            }
        })?;
        if !matches!(script.scheme(), "http" | "https")
            || !matches!(scope.scheme(), "http" | "https")
            || !script.username().is_empty()
            || script.password().is_some()
            || !scope.username().is_empty()
            || scope.password().is_some()
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native service worker registration URLs must be credential-free HTTP(S)"
                    .into(),
            });
        }
        if NativeOrigin::from_url(&script)? != NativeOrigin::from_url(&scope)? {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native service worker registration script and scope must be same-origin"
                    .into(),
            });
        }
        let script_directory = script
            .path()
            .rfind('/')
            .map(|index| &script.path()[..=index])
            .unwrap_or("/");
        if !scope.path().starts_with(script_directory)
            || scope.query().is_some()
            || scope.fragment().is_some()
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native service worker registration scope exceeds its script directory"
                    .into(),
            });
        }
        let canonical_scope = format!("{}{}", scope.origin().ascii_serialization(), scope.path());
        if self.scope != canonical_scope {
            return Err(NativeEngineError::invalid(
                "native service worker registration scope",
                "must be a canonical origin and path",
            ));
        }
        if !self.worker_type.eq_ignore_ascii_case("classic")
            && !self.worker_type.eq_ignore_ascii_case("module")
        {
            return Err(NativeEngineError::invalid(
                "native service worker registration type",
                "must be classic or module",
            ));
        }
        if let Some(waiting) = self.waiting.as_ref() {
            if waiting.script_url.is_empty() || waiting.script_url.len() > MAX_NATIVE_SCRIPT_BYTES {
                return Err(NativeEngineError::limit(
                    "native waiting service worker script URL",
                    MAX_NATIVE_SCRIPT_BYTES,
                    waiting.script_url.len(),
                ));
            }
            let waiting_script =
                Url::parse(without_fragment(&waiting.script_url)).map_err(|_| {
                    NativeEngineError::UnsupportedUrl {
                        reason: "native waiting service worker script URL is invalid".into(),
                    }
                })?;
            if !matches!(waiting_script.scheme(), "http" | "https")
                || !waiting_script.username().is_empty()
                || waiting_script.password().is_some()
                || NativeOrigin::from_url(&waiting_script)? != NativeOrigin::from_url(&scope)?
            {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "native waiting service worker script URL must be same-origin and credential-free"
                        .into(),
                });
            }
            let waiting_directory = waiting_script
                .path()
                .rfind('/')
                .map(|index| &waiting_script.path()[..=index])
                .unwrap_or("/");
            if !scope.path().starts_with(waiting_directory) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "native waiting service worker scope exceeds its script directory"
                        .into(),
                });
            }
            if !waiting.worker_type.eq_ignore_ascii_case("classic")
                && !waiting.worker_type.eq_ignore_ascii_case("module")
            {
                return Err(NativeEngineError::invalid(
                    "native waiting service worker registration type",
                    "must be classic or module",
                ));
            }
        }
        Ok(())
    }
}
