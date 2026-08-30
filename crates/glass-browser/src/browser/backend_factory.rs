//! Deterministic backend registration, selection, and startup.
//!
//! The factory is the runtime seam between profile selection and concrete
//! adapters.  It owns no CDP assumptions: callers provide typed candidates and
//! receive one selected semantic backend.

use super::backend_adapter::CdpBrowserBackend;
use super::bidi_backend::{BidiBackendConfig, BidiBrowserBackend};
#[cfg(feature = "native-engine")]
use super::native_backend::{NATIVE_ENGINE_BACKEND_ID, NativeEngineBackend};
#[cfg(feature = "native-engine")]
use super::native_engine::NativeEngineConfig;
use super::proof_backend::ProofBackend;
use super::webdriver_backend::{WebDriverBackendConfig, WebDriverBrowserBackend};
use crate::browser_backend::{
    BackendFuture, BackendOperation, BackendProfile, BackendRequest, BackendResponse,
    BackendSelectionRequest, BackendSelectionResult, BrowserBackend, BrowserBackendError,
    select_backend,
};

/// A registered backend waiting to be selected and started.
pub enum BackendStartup {
    Cdp(Box<CdpBrowserBackend>),
    Bidi(Box<BidiBrowserBackend>),
    WebDriver(Box<WebDriverBrowserBackend>),
    Proof(Box<ProofBackend>),
    #[cfg(feature = "native-engine")]
    Native(Box<NativeEngineBackend>),
}

impl BackendStartup {
    pub fn profile(&self) -> &BackendProfile {
        match self {
            Self::Cdp(backend) => backend.profile(),
            Self::Bidi(backend) => backend.profile(),
            Self::WebDriver(backend) => backend.profile(),
            Self::Proof(backend) => backend.profile(),
            #[cfg(feature = "native-engine")]
            Self::Native(backend) => backend.profile(),
        }
    }

    pub fn backend_id(&self) -> &str {
        &self.profile().identity.backend_id
    }
}

impl BrowserBackend for BackendStartup {
    fn profile(&self) -> &BackendProfile {
        self.profile()
    }

    fn dispatch<'a>(
        &'a self,
        operation: BackendOperation,
        request: BackendRequest,
    ) -> BackendFuture<'a, BackendResponse> {
        match self {
            Self::Cdp(backend) => backend.dispatch(operation, request),
            Self::Bidi(backend) => backend.dispatch(operation, request),
            Self::WebDriver(backend) => backend.dispatch(operation, request),
            Self::Proof(backend) => backend.dispatch(operation, request),
            #[cfg(feature = "native-engine")]
            Self::Native(backend) => backend.dispatch(operation, request),
        }
    }
}

/// Selected backend plus its machine-readable selection evidence.
pub struct StartedBackend {
    pub selection: BackendSelectionResult,
    pub backend: BackendStartup,
}

impl StartedBackend {
    pub fn profile(&self) -> &BackendProfile {
        self.backend.profile()
    }

    pub fn identity(&self) -> &crate::browser_backend::BackendIdentity {
        &self.profile().identity
    }

    pub fn certification(&self) -> &crate::browser_backend::CertificationProfile {
        &self.profile().identity.certification
    }

    pub fn capabilities(
        &self,
    ) -> &std::collections::BTreeMap<
        crate::browser_backend::BrowserCapability,
        crate::browser_backend::CapabilityDescriptor,
    > {
        &self.profile().capabilities
    }
}

/// Registry/factory for explicit, deterministic backend startup.
pub struct BackendFactory;

impl BackendFactory {
    /// Select a candidate by profile and return the owned typed backend.
    ///
    /// Selection is strict for an explicit preference and otherwise follows the
    /// stable profile ordering implemented by [`select_backend`].
    pub fn start(
        request: &BackendSelectionRequest,
        candidates: Vec<BackendStartup>,
    ) -> Result<StartedBackend, BrowserBackendError> {
        if candidates.is_empty() {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "no backend candidates were registered".into(),
            });
        }
        let candidates = candidates
            .into_iter()
            .filter(|_candidate| {
                #[cfg(feature = "native-engine")]
                {
                    _candidate.backend_id() != NATIVE_ENGINE_BACKEND_ID
                        || request.preferred_backend_id.as_deref() == Some(NATIVE_ENGINE_BACKEND_ID)
                }
                #[cfg(not(feature = "native-engine"))]
                {
                    let _ = request;
                    true
                }
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "no backend candidates were eligible for automatic selection".into(),
            });
        }
        let profiles = candidates
            .iter()
            .map(BackendStartup::profile)
            .cloned()
            .collect::<Vec<_>>();
        let selection = select_backend(request, &profiles)?;
        let selected_id = selection.selected.identity.backend_id.as_str();
        let position = candidates
            .iter()
            .position(|candidate| candidate.backend_id() == selected_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "selected backend was not present in registry".into(),
            })?;
        let backend = candidates
            .into_iter()
            .nth(position)
            .expect("position checked above");
        Ok(StartedBackend { selection, backend })
    }

    pub fn proof() -> Result<BackendStartup, BrowserBackendError> {
        Ok(BackendStartup::Proof(Box::new(ProofBackend::new()?)))
    }

    pub async fn bidi(config: BidiBackendConfig) -> Result<BackendStartup, BrowserBackendError> {
        Ok(BackendStartup::Bidi(Box::new(
            BidiBrowserBackend::connect_with_config(config).await?,
        )))
    }

    pub fn webdriver(
        config: WebDriverBackendConfig,
    ) -> Result<BackendStartup, BrowserBackendError> {
        Ok(BackendStartup::WebDriver(Box::new(
            WebDriverBrowserBackend::new(config)?,
        )))
    }

    pub fn safari_webdriver(
        endpoint: impl Into<String>,
    ) -> Result<BackendStartup, BrowserBackendError> {
        Self::webdriver(WebDriverBackendConfig::for_safari(endpoint))
    }

    pub fn cdp(
        session: super::session::BrowserSession,
    ) -> Result<BackendStartup, BrowserBackendError> {
        Ok(BackendStartup::Cdp(Box::new(CdpBrowserBackend::new(
            session,
        )?)))
    }

    #[cfg(feature = "native-engine")]
    pub fn native(config: NativeEngineConfig) -> Result<BackendStartup, BrowserBackendError> {
        Ok(BackendStartup::Native(Box::new(NativeEngineBackend::new(
            config,
        )?)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_backend::{
        BROWSER_BACKEND_SCHEMA_VERSION, BackendSelectionRequest, BrowserCapability,
        CertificationLevel, SupportLevel,
    };

    #[test]
    fn explicit_selection_is_not_iteration_order_dependent() {
        let proof = BackendFactory::proof().unwrap();
        let request = BackendSelectionRequest {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            glass_version: env!("CARGO_PKG_VERSION").into(),
            preferred_backend_id: Some("semantic-proof".into()),
            browser_family: None,
            browser_version: None,
            required_capabilities: vec![crate::browser_backend::CapabilityRequirement {
                capability: BrowserCapability::Contexts,
                minimum: SupportLevel::Available,
            }],
            minimum_certification: CertificationLevel::Partial,
        };
        let started = BackendFactory::start(&request, vec![proof]).unwrap();
        assert_eq!(started.profile().identity.backend_id, "semantic-proof");
    }

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_backend_is_explicit_only() {
        let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
        let automatic = BackendSelectionRequest {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            glass_version: env!("CARGO_PKG_VERSION").into(),
            preferred_backend_id: None,
            browser_family: None,
            browser_version: None,
            required_capabilities: vec![],
            minimum_certification: CertificationLevel::Experimental,
        };
        assert!(BackendFactory::start(&automatic, vec![native]).is_err());

        let native = BackendFactory::native(NativeEngineConfig::default()).unwrap();
        let explicit = BackendSelectionRequest {
            preferred_backend_id: Some(NATIVE_ENGINE_BACKEND_ID.into()),
            ..automatic
        };
        let started = BackendFactory::start(&explicit, vec![native]).unwrap();
        assert_eq!(
            started.profile().identity.backend_id,
            NATIVE_ENGINE_BACKEND_ID
        );
    }
}
