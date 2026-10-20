//! Native service-worker ownership, lifecycle, and request interception.
//!
//! Registrations live with the process-backed browsing context so a reload can
//! be controlled by the worker that the page registered before it. Worker
//! JavaScript remains isolated in its own QuickJS realm; only bounded request,
//! response, and message envelopes cross back to the content-process owner.

#[path = "service_worker/operations.rs"]
mod operations;
use super::config::{is_network_url, validate_context_id, validate_url_text, without_fragment};
use super::content_process::{
    NativeContentFetchBroker, is_parent_owned_network_target, missing_parent_network_authority,
};
use super::error::NativeEngineError;
use super::fetch_stream::{
    MAX_NATIVE_FETCH_UPLOAD_CHUNKS, NativeFetchUploadCommand, NativeFetchUploadConnection,
    NativeFetchUploadEvent, spawn_native_fetch_upload_source, spawn_native_fetch_upload_stream,
};
use super::interaction::{MAX_NATIVE_EFFECTS, MAX_NATIVE_FORM_BODY_BYTES};
use super::javascript::{
    MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_SERVICE_WORKER_CACHE_BODY_BYTES,
    MAX_NATIVE_SERVICE_WORKER_CACHE_ENTRIES, MAX_NATIVE_SERVICE_WORKER_CACHE_KEY_BYTES,
    MAX_NATIVE_SERVICE_WORKER_CACHE_NAME_BYTES, MAX_NATIVE_SERVICE_WORKER_CACHES,
    MAX_NATIVE_SERVICE_WORKER_SCOPE_BYTES, MAX_NATIVE_SERVICE_WORKERS, MAX_NATIVE_WORKER_MESSAGES,
    NativeJavaScriptRuntime, NativeMessagePortPageMessage, NativeMessagePortTransfer,
    NativeScriptCommand, NativeScriptEvaluation, NativeServiceWorkerCacheBatchEntry,
    NativeServiceWorkerCacheEntry, NativeServiceWorkerCacheState, NativeServiceWorkerClientMessage,
    NativeServiceWorkerClientState, NativeServiceWorkerMessageEvent,
    NativeServiceWorkerOpenWindowRequest, NativeServiceWorkerRegistrationProfile,
    NativeServiceWorkerRegistrationState, NativeServiceWorkerWorkerProfile,
    NativeServiceWorkerWorkerState, NativeWorkerModuleGraph,
    default_navigation_preload_header_value, load_service_worker_entry, load_service_worker_source,
    resolve_module_request_url, validate_message_port_transfers, validate_native_message_payload,
    validate_native_object_url_transfers, validate_native_service_worker_cache_request_headers,
    validate_navigation_preload_header_value, validate_service_worker_client_states,
};
use super::origin::NativeOrigin;
use super::resource_loader::{
    NativeCorsMode, NativeFetchCacheMode, NativeFetchCredentialsMode, NativeFetchMethod,
    NativeFetchRedirectMode, NativeFetchReferrerPolicy, NativeFetchRequest, NativeFetchResponse,
    NativeNavigationRequest, NativeRequestBody, NativeResource, NativeResourceLoader,
    referrer_for_navigation_with_policy,
};
use base64::Engine as _;
pub(crate) use operations::native_service_worker_client_id;
use operations::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;
use tokio::task::{AbortHandle, JoinSet};
use url::Url;

const MAX_NATIVE_SERVICE_WORKER_CLIENTS: usize =
    crate::limits::TOPOLOGY_MAX_TARGETS * crate::limits::TOPOLOGY_MAX_FRAMES;

pub(super) struct NativeServiceWorkerRegistrationRequest<'a> {
    pub(super) owner_url: &'a str,
    pub(super) script_url: Option<&'a str>,
    pub(super) scope: &'a str,
    pub(super) worker_type: Option<&'a str>,
    pub(super) referrer_url: &'a str,
    pub(super) referrer_policy: NativeFetchReferrerPolicy,
    pub(super) request_id: u32,
}

struct NativeServiceWorkerInstantiation {
    script_url: String,
    scope: String,
    is_module: bool,
    source: String,
    import_script_counts: BTreeMap<String, usize>,
    module_graph: Option<NativeWorkerModuleGraph>,
    worker_referrer_policy: NativeFetchReferrerPolicy,
}

struct NativeServiceWorkerRestoreRequest<'a> {
    owner_url: &'a str,
    script_url: &'a str,
    scope: &'a str,
    worker_type: &'a str,
    request_id: u32,
}

struct NativeServiceWorker {
    id: u32,
    script_url: String,
    scope: String,
    is_module: bool,
    runtime: NativeJavaScriptRuntime,
    import_script_counts: BTreeMap<String, usize>,
    skip_waiting_requested: bool,
    clients_claim_requested: bool,
    fetch_upload_connections: BTreeMap<u32, NativeFetchUploadConnection>,
}

const MAX_NATIVE_SERVICE_WORKER_FETCH_UPLOADS: usize = MAX_NATIVE_WORKER_MESSAGES;

pub(crate) enum NativeServiceWorkerNavigationOutcome {
    NotHandled,
    Handled(NativeResource),
    Suspended,
}

pub(crate) enum NativeServiceWorkerFetchOutcome {
    NotHandled,
    Handled(NativeFetchResponse),
    Suspended,
}

pub(crate) struct NativeServiceWorkerFetchCompletion {
    pub(crate) worker_id: u32,
    pub(crate) response: Option<NativeFetchResponse>,
}

pub(crate) enum NativeServiceWorkerNavigationPreloadResult {
    State { enabled: bool, header_value: String },
    InvalidState,
}

pub(super) struct NativeServiceWorkerNavigationPreloadRequest {
    request_id: u32,
    scope: String,
    header_value: String,
}

struct NativeServiceWorkerFetchContinuation {
    worker_id: u32,
    open_window_request_id: u32,
    fallback_url: String,
    pending: VecDeque<NativeScriptCommand>,
    value: Value,
    awaiting: bool,
}

pub(crate) enum NativeServiceWorkerFetchTaskResult {
    Completed(Box<NativeServiceWorkerFetchTaskCompletion>),
    ParentBrokered(Box<NativeServiceWorkerParentFetchTask>),
}

pub(crate) struct NativeServiceWorkerFetchTaskCompletion {
    worker_id: u32,
    request_id: u32,
    fallback_url: String,
    payload: Value,
    loader: Option<NativeResourceLoader>,
}

pub(crate) struct NativeServiceWorkerParentFetchTask {
    worker_id: u32,
    request_id: u32,
    fallback_url: String,
    request: NativeServiceWorkerFetchTaskRequest,
}

struct NativeServiceWorkerFetchTaskRequest {
    request_id: u32,
    worker_id: u32,
    href: String,
    credentials: bool,
    credentials_mode: NativeFetchCredentialsMode,
    referrer_url: Option<String>,
    referrer_policy: Option<String>,
    method: NativeFetchMethod,
    headers: BTreeMap<String, String>,
    body: Option<NativeRequestBody>,
    content_type: Option<String>,
    cors_mode: NativeCorsMode,
    redirect_mode: NativeFetchRedirectMode,
    cache_mode: NativeFetchCacheMode,
    timeout: Option<Duration>,
    upload_stream_id: Option<u32>,
}

enum NativeServiceWorkerFetchSettlement {
    Complete {
        value: Value,
        client_messages: Vec<NativeServiceWorkerClientMessage>,
    },
    Suspended {
        continuation: NativeServiceWorkerFetchContinuation,
        client_messages: Vec<NativeServiceWorkerClientMessage>,
    },
}

fn completed_lifecycle_states() -> Vec<String> {
    ["installing", "installed", "activating", "activated"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn waiting_lifecycle_states() -> Vec<String> {
    ["installing", "installed"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn worker_state(worker: &NativeServiceWorker, state: &str) -> NativeServiceWorkerWorkerState {
    NativeServiceWorkerWorkerState {
        script_url: worker.script_url.clone(),
        state: state.to_owned(),
    }
}

fn worker_type_name(worker: &NativeServiceWorker) -> String {
    if worker.is_module {
        "module".into()
    } else {
        "classic".into()
    }
}

fn registration_state_for_workers(
    scope: &str,
    active: Option<&NativeServiceWorker>,
    waiting: Option<&NativeServiceWorker>,
    controlled: bool,
    lifecycle: Vec<String>,
) -> Result<NativeServiceWorkerRegistrationState, NativeEngineError> {
    let active_state = active.map(|worker| worker_state(worker, "activated"));
    let waiting_state = waiting.map(|worker| worker_state(worker, "installed"));
    let representative = active_state
        .as_ref()
        .or(waiting_state.as_ref())
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "native service worker registration state",
                "must contain an active or waiting worker",
            )
        })?;
    Ok(NativeServiceWorkerRegistrationState {
        script_url: representative.script_url.clone(),
        scope: scope.to_owned(),
        state: representative.state.clone(),
        controlled,
        lifecycle,
        active: active_state,
        waiting: waiting_state,
    })
}

pub(crate) struct NativeServiceWorkerRegistry {
    registrations: BTreeMap<String, NativeServiceWorker>,
    waiting_workers: BTreeMap<String, NativeServiceWorker>,
    next_worker_id: u32,
    next_navigation_preload_request_id: u32,
    next_timer_worker_id: u32,
    cache_state: NativeServiceWorkerCacheState,
    registration_profiles: Vec<NativeServiceWorkerRegistrationProfile>,
    registration_changes: BTreeMap<String, Option<NativeServiceWorkerRegistrationProfile>>,
    pending_message_port_messages: VecDeque<NativeMessagePortPageMessage>,
    pending_client_messages: VecDeque<NativeServiceWorkerClientMessage>,
    pending_open_windows: VecDeque<NativeServiceWorkerOpenWindowRequest>,
    pending_fetches: BTreeMap<(u32, u32), NativeServiceWorkerFetchContinuation>,
    pending_parent_lifetime_fetches: VecDeque<NativeServiceWorkerParentFetchTask>,
    lifetime_fetch_tasks: JoinSet<NativeServiceWorkerFetchTaskResult>,
    lifetime_fetch_abort_handles: BTreeMap<(u32, u32), AbortHandle>,
    completed_fetches: VecDeque<NativeServiceWorkerFetchCompletion>,
    announced_open_windows: BTreeSet<(u32, u32)>,
    message_port_routes: BTreeMap<String, u32>,
    current_client_id: String,
    current_client_url: Option<String>,
    current_client_scope: Option<String>,
    client_states: Vec<NativeServiceWorkerClientState>,
}

impl Default for NativeServiceWorkerRegistry {
    fn default() -> Self {
        Self {
            registrations: BTreeMap::new(),
            waiting_workers: BTreeMap::new(),
            next_worker_id: 1,
            next_navigation_preload_request_id: 1,
            next_timer_worker_id: 0,
            cache_state: NativeServiceWorkerCacheState::default(),
            registration_profiles: Vec::new(),
            registration_changes: BTreeMap::new(),
            pending_message_port_messages: VecDeque::new(),
            pending_client_messages: VecDeque::new(),
            pending_open_windows: VecDeque::new(),
            pending_fetches: BTreeMap::new(),
            pending_parent_lifetime_fetches: VecDeque::new(),
            lifetime_fetch_tasks: JoinSet::new(),
            lifetime_fetch_abort_handles: BTreeMap::new(),
            completed_fetches: VecDeque::new(),
            announced_open_windows: BTreeSet::new(),
            message_port_routes: BTreeMap::new(),
            current_client_id: String::new(),
            current_client_url: None,
            current_client_scope: None,
            client_states: Vec::new(),
        }
    }
}

mod registry_lifecycle;
mod registry_lifetime;
mod registry_messaging;
mod registry_navigation;
mod registry_ports;

#[cfg(test)]
mod navigation_preload_tests {
    use super::*;
    use crate::native_engine::config::{NativeEngineConfig, NativeFixture};

    fn test_worker(script_url: &str, scope: &str) -> NativeServiceWorker {
        NativeServiceWorker {
            id: 1,
            script_url: script_url.into(),
            scope: scope.into(),
            is_module: false,
            runtime: NativeJavaScriptRuntime::new_with_context_id("navigation-preload-test")
                .expect("test worker runtime constructs"),
            import_script_counts: BTreeMap::new(),
            skip_waiting_requested: false,
            clients_claim_requested: false,
            fetch_upload_connections: BTreeMap::new(),
        }
    }

    #[test]
    fn navigation_preload_state_requires_active_worker_and_survives_replacement() {
        let document_url = "https://navigation-preload.test/page";
        let scope = "https://navigation-preload.test/";
        let mut registry = NativeServiceWorkerRegistry::default();
        registry
            .registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: "https://navigation-preload.test/sw.js".into(),
                scope: scope.into(),
                worker_type: "classic".into(),
                navigation_preload_enabled: false,
                navigation_preload_header_value: "true".into(),
                waiting: None,
            });

        assert!(matches!(
            registry
                .apply_navigation_preload_operation(document_url, scope, "enable", None)
                .expect("inactive registration reports a Web API state failure"),
            NativeServiceWorkerNavigationPreloadResult::InvalidState
        ));
        assert!(matches!(
            registry
                .apply_navigation_preload_operation(document_url, scope, "getState", None)
                .expect("state can be read without an active worker"),
            NativeServiceWorkerNavigationPreloadResult::State {
                enabled: false,
                header_value,
            } if header_value == "true"
        ));

        registry.registrations.insert(
            scope.into(),
            test_worker("https://navigation-preload.test/sw.js", scope),
        );
        assert!(matches!(
            registry
                .apply_navigation_preload_operation(document_url, scope, "enable", None)
                .expect("active registration enables preloading"),
            NativeServiceWorkerNavigationPreloadResult::State {
                enabled: true,
                header_value,
            } if header_value == "true"
        ));
        registry
            .apply_navigation_preload_operation(
                document_url,
                scope,
                "setHeaderValue",
                Some("release-preview"),
            )
            .expect("active registration updates its header value");

        registry.remember_registration(&test_worker(
            "https://navigation-preload.test/sw-v2.js",
            scope,
        ));
        let profile = registry
            .registration_profiles()
            .into_iter()
            .find(|profile| profile.scope == scope)
            .expect("replacement keeps its registration profile");
        assert_eq!(
            profile.script_url,
            "https://navigation-preload.test/sw-v2.js"
        );
        assert!(profile.navigation_preload_enabled);
        assert_eq!(profile.navigation_preload_header_value, "release-preview");
        assert_eq!(
            registry.registration_changes().get(scope),
            Some(&Some(profile))
        );

        assert!(
            registry
                .apply_navigation_preload_operation(
                    "https://other.test/page",
                    scope,
                    "getState",
                    None,
                )
                .is_err()
        );
    }

    #[test]
    fn navigation_preload_requires_enabled_scope_and_fetch_listener() {
        let scope = "https://navigation-preload.test/";
        let target =
            Url::parse("https://navigation-preload.test/page").expect("navigation target parses");
        let mut registry = NativeServiceWorkerRegistry::default();
        registry
            .registration_profiles
            .push(NativeServiceWorkerRegistrationProfile {
                script_url: "https://navigation-preload.test/sw.js".into(),
                scope: scope.into(),
                worker_type: "classic".into(),
                navigation_preload_enabled: false,
                navigation_preload_header_value: "true".into(),
                waiting: None,
            });
        let worker = test_worker("https://navigation-preload.test/sw.js", scope);
        assert!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("no active worker does not start a preload")
                .is_none()
        );
        registry.registrations.insert(scope.into(), worker);
        assert!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("an active worker without a fetch listener does not preload")
                .is_none()
        );
        let worker = registry
            .registrations
            .get_mut(scope)
            .expect("test registration retains its active worker");
        worker
            .runtime
            .evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                None,
                "self.addEventListener('fetch', () => {});",
                &BTreeMap::new(),
            )
            .expect("fetch listener installs");
        assert!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("disabled navigation preload does not start a request")
                .is_none()
        );
        registry
            .registration_profiles
            .iter_mut()
            .find(|profile| profile.scope == scope)
            .expect("test registration has a profile")
            .navigation_preload_enabled = true;
        assert!(
            registry
                .navigation_preload_configuration(&target, "POST")
                .expect("non-GET navigation does not start a preload")
                .is_none()
        );
        assert_eq!(
            registry
                .navigation_preload_configuration(&target, "GET")
                .expect("enabled registration with a fetch listener starts a preload"),
            Some((scope.into(), "true".into()))
        );
        assert!(
            registry
                .navigation_preload_configuration(
                    &Url::parse("https://other.test/page").expect("cross-origin URL parses"),
                    "GET",
                )
                .expect("cross-origin navigation cannot match this registration")
                .is_none()
        );
    }

    #[tokio::test]
    async fn wait_until_timer_runs_after_fetch_response_settles() {
        let worker_url = "fixture://worker-timer.test/sw.js";
        let scope = "fixture://worker-timer.test/";
        let target_url = "fixture://worker-timer.test/page";
        let worker = test_worker(worker_url, scope);
        worker
            .runtime
            .evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                None,
                r#"self.addEventListener('fetch', event => {
  event.waitUntil(new Promise(resolve => setTimeout(() => {
    globalThis.__waitUntilTimerSettled = true;
    resolve();
  }, 0)));
  event.respondWith(new Response('response ready'));
});"#,
                &BTreeMap::new(),
            )
            .expect("worker installs fetch handler");

        let mut registry = NativeServiceWorkerRegistry::default();
        registry.registrations.insert(scope.into(), worker);
        let mut loader = NativeResourceLoader::new(&NativeEngineConfig::default())
            .expect("test resource loader constructs");
        let evaluation = registry
            .registrations
            .get(scope)
            .expect("active worker is registered")
            .runtime
            .evaluate_service_worker_fetch(
                1,
                worker_url,
                &json!({
                    "url": target_url,
                    "method": "GET",
                    "headers": [],
                    "bodyNull": true,
                    "mode": "navigate",
                    "redirect": "follow",
                    "credentialsMode": "include",
                    "destination": "document",
                }),
                false,
            )
            .expect("fetch response settles independently of waitUntil timer");
        let NativeServiceWorkerFetchSettlement::Complete { value, .. } =
            settle_service_worker_fetch(
                registry
                    .registrations
                    .get_mut(scope)
                    .expect("active worker remains registered"),
                &mut loader,
                evaluation,
                &mut registry.cache_state,
                NativeServiceWorkerFetchTestContext {
                    current_client_id: "",
                    fallback_url: target_url,
                    pending_open_windows: &mut registry.pending_open_windows,
                    lifetime_fetch_tasks: &mut registry.lifetime_fetch_tasks,
                    lifetime_fetch_abort_handles: &mut registry.lifetime_fetch_abort_handles,
                },
            )
            .await
            .expect("independent response settles")
        else {
            panic!("independent response must not suspend");
        };
        assert_eq!(value["handled"], true);
        assert_eq!(registry.next_timer_delay_ms().unwrap(), Some(0));

        registry
            .run_due_timers(&mut loader)
            .await
            .expect("worker timer runs as a background event-loop turn");
        let result = registry
            .registrations
            .get(scope)
            .expect("active worker remains registered")
            .runtime
            .evaluate_service_worker_source(
                1,
                worker_url,
                None,
                "globalThis.__waitUntilTimerSettled",
                &BTreeMap::new(),
            )
            .expect("completed lifetime callback remains observable");
        assert_eq!(result.value, true);
        assert_eq!(registry.next_timer_delay_ms().unwrap(), None);
    }

    #[tokio::test]
    async fn independent_response_survives_and_retains_lifetime_fetch_work() {
        let worker_url = "fixture://worker-lifetime.test/sw.js";
        let target_url = "fixture://worker-lifetime.test/page";
        let fetched_url = "fixture://worker-lifetime.test/lifetime.txt";
        let mut config = NativeEngineConfig::default();
        config.fixtures.push(
            NativeFixture::new(fetched_url, "lifetime body")
                .expect("lifetime fetch fixture is valid"),
        );
        let mut loader = NativeResourceLoader::new(&config).expect("test loader constructs");
        let mut worker = test_worker(worker_url, "fixture://worker-lifetime.test/");
        worker
            .runtime
            .evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                None,
                r#"self.addEventListener('fetch', event => {
  event.waitUntil(fetch('fixture://worker-lifetime.test/lifetime.txt', {
    mode: 'same-origin',
  }).then(response => response.text()).then(text => {
    globalThis.__lifetimeFetchText = text;
  }));
  event.respondWith(new Response('response is ready'));
});"#,
                &BTreeMap::new(),
            )
            .expect("worker fetch handler installs");
        let evaluation = worker
            .runtime
            .evaluate_service_worker_fetch(
                worker.id,
                &worker.script_url,
                &json!({
                    "url": target_url,
                    "method": "GET",
                    "headers": [],
                    "bodyNull": true,
                    "mode": "navigate",
                    "redirect": "follow",
                    "credentialsMode": "include",
                    "destination": "document",
                }),
                false,
            )
            .expect("FetchEvent returns its independent response");
        let mut cache_state = NativeServiceWorkerCacheState::default();
        let mut pending_open_windows = VecDeque::new();
        let mut lifetime_fetch_tasks = JoinSet::new();
        let mut lifetime_fetch_abort_handles = BTreeMap::new();
        let settlement = settle_service_worker_fetch(
            &mut worker,
            &mut loader,
            evaluation,
            &mut cache_state,
            NativeServiceWorkerFetchTestContext {
                current_client_id: "",
                fallback_url: target_url,
                pending_open_windows: &mut pending_open_windows,
                lifetime_fetch_tasks: &mut lifetime_fetch_tasks,
                lifetime_fetch_abort_handles: &mut lifetime_fetch_abort_handles,
            },
        )
        .await
        .expect("settled response preserves unrelated fetch lifetime work");
        let NativeServiceWorkerFetchSettlement::Complete { value, .. } = settlement else {
            panic!("independent response must not suspend");
        };
        assert_eq!(value["handled"], true);
        let response_body = base64::engine::general_purpose::STANDARD
            .decode(
                value["response"]["bodyBase64"]
                    .as_str()
                    .expect("response body is encoded"),
            )
            .expect("response body encoding is valid");
        assert_eq!(response_body, b"response is ready");
        assert_eq!(lifetime_fetch_tasks.len(), 1);

        let task = lifetime_fetch_tasks
            .join_next()
            .await
            .expect("lifetime fetch task remains owned")
            .expect("lifetime fetch task completes");
        let NativeServiceWorkerFetchTaskResult::Completed(completion) = task else {
            panic!("standalone lifetime fetch uses its local loader");
        };
        loader
            .merge_fetch_task_state(
                completion
                    .loader
                    .expect("local fetch completion retains loader state"),
            )
            .expect("lifetime fetch state merges into the worker loader");
        let resolved = worker
            .runtime
            .resolve_service_worker_fetch(
                worker.id,
                &worker.script_url,
                completion.request_id,
                &completion.payload,
                worker.is_module,
            )
            .expect("lifetime fetch promise settles in the worker realm");
        assert!(resolved.commands.is_empty());
        let settled = worker
            .runtime
            .evaluate_service_worker_source(
                worker.id,
                &worker.script_url,
                None,
                "globalThis.__lifetimeFetchText;",
                &BTreeMap::new(),
            )
            .expect("lifetime result is observable in the worker realm");
        assert_eq!(settled.value, "lifetime body");
    }
}
