//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

use super::config::Viewport;
use super::dom::{NativeDocument, NativePageScriptSource, NativePageScriptTiming};
use super::error::NativeEngineError;
use super::interaction::NativeEventKind;
use super::origin::NativeOrigin;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Context, Error, Module, Runtime, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::Url;

/// Maximum source accepted by the native script evaluator.
pub(crate) const MAX_NATIVE_SCRIPT_BYTES: usize = crate::browser_backend::MAX_TEXT_BYTES;
/// Maximum JSON representation returned to the semantic backend.
pub(crate) const MAX_NATIVE_SCRIPT_RESULT_BYTES: usize = crate::browser_backend::MAX_JSON_BYTES;
/// Maximum inline page scripts executed while committing one document.
pub(crate) const MAX_NATIVE_INLINE_SCRIPTS: usize = 32;
pub(crate) const MAX_NATIVE_MODULE_IMPORTS: usize = 128;
const NATIVE_SCRIPT_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const NATIVE_SCRIPT_STACK_BYTES: usize = 1024 * 1024;
const NATIVE_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_WEB_STORAGE_PROFILE_BYTES: usize = 4 * 1024 * 1024;
const WEB_STORAGE_PROFILE_VERSION: u64 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum NativeScriptCommand {
    Focus {
        node_index: u32,
    },
    Blur {
        node_index: u32,
    },
    Click {
        node_index: u32,
    },
    SubmitForm {
        node_index: u32,
    },
    RequestSubmitForm {
        node_index: u32,
        #[serde(default)]
        submitter_index: Option<u32>,
    },
    Fetch {
        request_id: u32,
        href: String,
        credentials: bool,
        method: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        content_type: Option<String>,
    },
    StorageSet {
        scope: String,
        key: String,
        value: String,
    },
    StorageRemove {
        scope: String,
        key: String,
    },
    StorageClear {
        scope: String,
    },
    SetValue {
        node_index: u32,
        value: String,
    },
    SetChecked {
        node_index: u32,
        checked: bool,
    },
    SetSelected {
        node_index: u32,
        selected: bool,
    },
    SetAttribute {
        node_index: u32,
        name: String,
        value: String,
    },
    RemoveAttribute {
        node_index: u32,
        name: String,
    },
    SetCustomValidity {
        node_index: u32,
        message: String,
    },
    CheckValidity {
        node_index: u32,
    },
    ReportValidity {
        node_index: u32,
    },
}

pub(crate) struct NativeScriptEvaluation {
    pub(crate) value: serde_json::Value,
    pub(crate) commands: Vec<NativeScriptCommand>,
}

/// Origin-keyed page storage retained by the native runtime owner.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct NativeWebStorageState {
    local: BTreeMap<String, BTreeMap<String, String>>,
    session: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct NativeWebStorageProfile {
    version: u64,
    local: BTreeMap<String, BTreeMap<String, String>>,
}

pub(crate) fn load_web_storage_profile(
    path: Option<&Path>,
) -> Result<NativeWebStorageState, NativeEngineError> {
    let Some(path) = path else {
        return Ok(NativeWebStorageState::default());
    };
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(NativeWebStorageState::default());
        }
        Err(_) => {
            return Err(NativeEngineError::Worker {
                operation: "load native Web Storage profile".into(),
                reason: "native Web Storage profile metadata is unavailable".into(),
            });
        }
    };
    let profile_bytes = usize::try_from(metadata.len()).map_err(|_| {
        NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            usize::MAX,
        )
    })?;
    if profile_bytes > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            profile_bytes,
        ));
    }
    let bytes = fs::read(path).map_err(|_| NativeEngineError::Worker {
        operation: "load native Web Storage profile".into(),
        reason: "native Web Storage profile cannot be read".into(),
    })?;
    let profile: NativeWebStorageProfile = serde_json::from_slice(&bytes).map_err(|_| {
        NativeEngineError::invalid(
            "storage profile",
            "must contain a valid native Web Storage profile",
        )
    })?;
    if profile.version != WEB_STORAGE_PROFILE_VERSION {
        return Err(NativeEngineError::invalid(
            "storage profile version",
            "is unsupported",
        ));
    }
    let state = NativeWebStorageState {
        local: profile.local,
        session: BTreeMap::new(),
    };
    validate_web_storage_state(&state)?;
    Ok(state)
}

pub(crate) fn save_web_storage_profile(
    path: Option<&Path>,
    state: &NativeWebStorageState,
) -> Result<(), NativeEngineError> {
    let Some(path) = path else {
        return Ok(());
    };
    validate_web_storage_state(state)?;
    let profile = NativeWebStorageProfile {
        version: WEB_STORAGE_PROFILE_VERSION,
        local: state.local.clone(),
    };
    let bytes = serde_json::to_vec(&profile).map_err(|_| NativeEngineError::Worker {
        operation: "save native Web Storage profile".into(),
        reason: "native Web Storage profile cannot be encoded".into(),
    })?;
    if bytes.len() > MAX_WEB_STORAGE_PROFILE_BYTES {
        return Err(NativeEngineError::limit(
            "native Web Storage profile",
            MAX_WEB_STORAGE_PROFILE_BYTES,
            bytes.len(),
        ));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|_| NativeEngineError::Worker {
            operation: "save native Web Storage profile".into(),
            reason: "native Web Storage profile directory cannot be created".into(),
        })?;
    }
    let temporary_path = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary_path, &bytes).map_err(|_| NativeEngineError::Worker {
        operation: "save native Web Storage profile".into(),
        reason: "native Web Storage profile cannot be written".into(),
    })?;
    if let Err(rename_error) = fs::rename(&temporary_path, path) {
        // Unix replaces an existing destination atomically. Windows refuses
        // that rename, so copy the already-complete bounded snapshot as a
        // portable fallback and keep the original until the copy starts.
        let expected_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let fallback = fs::copy(&temporary_path, path).and_then(|copied_bytes| {
            (copied_bytes == expected_bytes)
                .then_some(())
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::WriteZero))
        });
        if fallback.is_err() {
            let _ = fs::remove_file(&temporary_path);
            return Err(NativeEngineError::Worker {
                operation: "save native Web Storage profile".into(),
                reason: format!("native Web Storage profile cannot be committed: {rename_error}"),
            });
        }
        let _ = fs::remove_file(&temporary_path);
    }
    Ok(())
}

fn validate_web_storage_state(state: &NativeWebStorageState) -> Result<(), NativeEngineError> {
    for origins in [&state.local, &state.session] {
        if origins.len() > crate::browser_backend::MAX_STORAGE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native Web Storage origins",
                crate::browser_backend::MAX_STORAGE_ENTRIES,
                origins.len(),
            ));
        }
        for (origin, entries) in origins {
            if origin.len() > crate::browser_backend::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage origin",
                    crate::browser_backend::MAX_TEXT_BYTES,
                    origin.len(),
                ));
            }
            if entries.len() > crate::browser_backend::MAX_STORAGE_ENTRIES {
                return Err(NativeEngineError::limit(
                    "native Web Storage entries",
                    crate::browser_backend::MAX_STORAGE_ENTRIES,
                    entries.len(),
                ));
            }
            for (key, value) in entries {
                if key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Web Storage key",
                        crate::browser_backend::MAX_BACKEND_ID_BYTES,
                        key.len(),
                    ));
                }
                if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Web Storage value",
                        crate::browser_backend::MAX_TEXT_BYTES,
                        value.len(),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
struct NativeWebStorageView {
    local: BTreeMap<String, String>,
    session: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) enum NativePageScript {
    Classic { source: String },
    Module { name: String, source: String },
    ModuleDependency { name: String, source: String },
}

struct NativeModuleResolver;

impl Resolver for NativeModuleResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &rquickjs::Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        let base = Url::parse(base)
            .map_err(|_| Error::new_resolving_message(base, name, "module base is not a URL"))?;
        let target = Url::parse(name).or_else(|_| base.join(name)).map_err(|_| {
            Error::new_resolving_message(base.as_str(), name, "module URL is invalid")
        })?;
        if !target.username().is_empty() || target.password().is_some() {
            return Err(Error::new_resolving_message(
                base.as_str(),
                name,
                "module URL must not contain credentials",
            ));
        }
        let mut target = target;
        target.set_fragment(None);
        Ok(target.to_string())
    }
}

struct NativeModuleLoader {
    sources: Arc<Mutex<BTreeMap<String, String>>>,
}

impl Loader for NativeModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &rquickjs::Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js>> {
        let source = self
            .sources
            .lock()
            .ok()
            .and_then(|sources| sources.get(name).cloned())
            .ok_or_else(|| Error::new_loading_message(name, "module was not prefetched"))?;
        Module::declare(ctx.clone(), name, source)
    }
}

pub(crate) fn order_page_scripts(
    sources: Vec<(NativePageScriptTiming, NativePageScript)>,
) -> Vec<NativePageScript> {
    let mut parser_blocking = Vec::new();
    let mut asynchronous = Vec::new();
    let mut deferred = Vec::new();
    for (timing, source) in sources {
        match timing {
            NativePageScriptTiming::ParserBlocking => parser_blocking.push(source),
            NativePageScriptTiming::Async => asynchronous.push(source),
            NativePageScriptTiming::Defer => deferred.push(source),
        }
    }
    parser_blocking.extend(asynchronous);
    parser_blocking.extend(deferred);
    parser_blocking
}

/// Execute the bounded inline scripts discovered in one parsed document.
///
/// The caller owns the realm so local documents and the child content process
/// can both retain globals and listeners after the document commit. Script
/// navigation is intentionally rejected during parsing; navigation only has a
/// defined owner after the document has been committed.
pub(crate) fn execute_inline_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
) -> Result<(), NativeEngineError> {
    let sources = document
        .page_script_sources(MAX_NATIVE_INLINE_SCRIPTS, MAX_NATIVE_SCRIPT_BYTES)
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| match source {
            NativePageScriptSource::Inline { source, timing } => {
                Some((timing, NativePageScript::Classic { source }))
            }
            NativePageScriptSource::ModuleInline { source, timing } => Some((
                timing,
                NativePageScript::Module {
                    name: format!("{document_url}#glass-inline-module-{index}"),
                    source,
                },
            )),
            NativePageScriptSource::External { .. }
            | NativePageScriptSource::ModuleExternal { .. } => None,
        })
        .collect::<Vec<_>>();
    let sources = order_page_scripts(sources);
    execute_page_scripts(
        document,
        runtime,
        &sources,
        document_url,
        document_origin,
        viewport,
        storage_state,
        &[],
    )
    .map(|_| ())
}

pub(crate) fn execute_page_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    sources: &[NativePageScript],
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    storage_state: &NativeWebStorageState,
    resource_load_nodes: &[u32],
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    if runtime.is_none() {
        *runtime = Some(NativeJavaScriptRuntime::new()?);
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_storage_state(storage_state.clone());
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("loading");
    let module_sources = sources
        .iter()
        .filter_map(|source| match source {
            NativePageScript::Module { name, source }
            | NativePageScript::ModuleDependency { name, source } => {
                Some((name.clone(), source.clone()))
            }
            NativePageScript::Classic { .. } => None,
        })
        .collect::<BTreeMap<_, _>>();
    runtime
        .as_ref()
        .expect("page script runtime initialized")
        .set_module_sources(module_sources);
    let mut pending_fetches = Vec::new();
    for source in sources {
        let evaluation = {
            let script_runtime = runtime.as_ref().expect("page script runtime initialized");
            match source {
                NativePageScript::Classic { source } => script_runtime.evaluate(
                    source,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                )?,
                NativePageScript::Module { name, source } => script_runtime.evaluate_module(
                    name,
                    source,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                )?,
                NativePageScript::ModuleDependency { .. } => continue,
            }
        };
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
    }
    for node_index in resource_load_nodes {
        let Some(event_source) = host_event_script(&[(*node_index, NativeEventKind::Load)])? else {
            continue;
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate(
                &event_source,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("interactive");
    for (target, kind) in [
        (0, NativeEventKind::ReadyStateChange),
        (0, NativeEventKind::DomContentLoaded),
    ] {
        let Some(event_source) = host_event_script(&[(target, kind)])? else {
            continue;
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate(
                &event_source,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
    }
    runtime
        .as_mut()
        .expect("page script runtime initialized")
        .set_ready_state("complete");
    for (target, kind) in [
        (0, NativeEventKind::ReadyStateChange),
        (u32::MAX, NativeEventKind::Load),
    ] {
        let Some(event_source) = host_event_script(&[(target, kind)])? else {
            continue;
        };
        let evaluation = runtime
            .as_ref()
            .expect("page script runtime initialized")
            .evaluate(
                &event_source,
                document,
                document_url,
                document_origin,
                viewport,
            )?;
        apply_page_script_evaluation(document, evaluation, &mut pending_fetches)?;
    }
    Ok(pending_fetches)
}

fn apply_page_script_evaluation(
    document: &mut NativeDocument,
    evaluation: NativeScriptEvaluation,
    pending_fetches: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let mut commands = Vec::new();
    for command in evaluation.commands {
        if matches!(command, NativeScriptCommand::Fetch { .. }) {
            pending_fetches.push(command);
        } else {
            commands.push(command);
        }
    }
    if commands.is_empty() {
        return Ok(());
    }
    let mut next = document.clone();
    next.apply_script_commands(&commands)?;
    *document = next;
    Ok(())
}

/// Build the internal source used to deliver Rust-owned semantic events into
/// the persistent page realm. The source is generated from typed, bounded
/// event metadata and never contains page-provided strings.
pub(crate) fn host_event_script(
    events: &[(u32, NativeEventKind)],
) -> Result<Option<String>, NativeEngineError> {
    let events = events
        .iter()
        .map(|(node_index, kind)| (*node_index, *kind, None))
        .collect::<Vec<_>>();
    host_event_script_with_submitters(&events)
}

pub(crate) fn host_submit_event_script(
    form_index: u32,
    submitter_index: Option<u32>,
) -> Result<Option<String>, NativeEngineError> {
    host_event_script_with_submitters(&[(form_index, NativeEventKind::Submit, submitter_index)])
}

pub(crate) fn host_hash_change_event_script(
    old_url: &str,
    new_url: &str,
) -> Result<Option<String>, NativeEngineError> {
    let old_url = serde_json::to_string(old_url).map_err(|_| NativeEngineError::Worker {
        operation: "serialize hashchange event".into(),
        reason: "hashchange old URL could not be serialized".into(),
    })?;
    let new_url = serde_json::to_string(new_url).map_err(|_| NativeEngineError::Worker {
        operation: "serialize hashchange event".into(),
        reason: "hashchange new URL could not be serialized".into(),
    })?;
    let source = format!(
        "globalThis.__glassDispatchHostEvents([{{\"node_index\":4294967295,\"type\":\"hashchange\",\"bubbles\":false,\"cancelable\":false,\"old_url\":{old_url},\"new_url\":{new_url}}}])"
    );
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "hashchange event",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

fn host_event_script_with_submitters(
    events: &[(u32, NativeEventKind, Option<u32>)],
) -> Result<Option<String>, NativeEngineError> {
    if events.is_empty() {
        return Ok(None);
    }
    let descriptors = events
        .iter()
        .map(|(node_index, kind, submitter_index)| {
            let (event_type, bubbles, cancelable) = match kind {
                NativeEventKind::Blur => ("blur", false, false),
                NativeEventKind::Focus => ("focus", false, false),
                NativeEventKind::ReadyStateChange => ("readystatechange", false, false),
                NativeEventKind::DomContentLoaded => ("DOMContentLoaded", false, false),
                NativeEventKind::Load => ("load", false, false),
                NativeEventKind::PageHide => ("pagehide", false, false),
                NativeEventKind::Unload => ("unload", false, false),
                NativeEventKind::PageShow => ("pageshow", false, false),
                NativeEventKind::BeforeUnload => ("beforeunload", false, true),
                NativeEventKind::HashChange => ("hashchange", false, false),
                NativeEventKind::PopState => ("popstate", false, false),
                NativeEventKind::Invalid => ("invalid", false, true),
                NativeEventKind::KeyDown => ("keydown", true, true),
                NativeEventKind::KeyUp => ("keyup", true, false),
                NativeEventKind::Submit => ("submit", true, true),
                NativeEventKind::Click => ("click", true, true),
                NativeEventKind::Input => ("input", true, false),
                NativeEventKind::Change => ("change", true, false),
                NativeEventKind::Scroll => ("scroll", true, false),
            };
            serde_json::json!({
                "node_index": node_index,
                "type": event_type,
                "bubbles": bubbles,
                "cancelable": cancelable,
                "submitter_node_index": submitter_index,
            })
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_string(&descriptors).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native event dispatch".into(),
        reason: "native event dispatch metadata could not be serialized".into(),
    })?;
    let source = format!("globalThis.__glassDispatchHostEvents({encoded})");
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native event dispatch",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

pub(crate) fn host_key_event_script(
    node_index: u32,
    kind: NativeEventKind,
    key: &str,
) -> Result<Option<String>, NativeEngineError> {
    let (event_type, bubbles, cancelable) = match kind {
        NativeEventKind::KeyDown => ("keydown", true, true),
        NativeEventKind::KeyUp => ("keyup", true, false),
        _ => {
            return Err(NativeEngineError::invalid(
                "native key event",
                "key event dispatch requires keydown or keyup",
            ));
        }
    };
    let code = match key {
        " " => "Space".to_owned(),
        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Enter" | "Tab" | "Escape"
        | "Backspace" | "Delete" | "Home" | "End" | "PageUp" | "PageDown" => key.to_owned(),
        _ if key.chars().count() == 1 => {
            let character = key.chars().next().expect("single-character key");
            if character.is_ascii_alphabetic() {
                format!("Key{}", character.to_ascii_uppercase())
            } else if character.is_ascii_digit() {
                format!("Digit{character}")
            } else {
                key.to_owned()
            }
        }
        _ => key.to_owned(),
    };
    let descriptors = serde_json::json!([{
        "node_index": node_index,
        "type": event_type,
        "bubbles": bubbles,
        "cancelable": cancelable,
        "key": key,
        "code": code,
    }]);
    let encoded = serde_json::to_string(&descriptors).map_err(|_| NativeEngineError::Worker {
        operation: "serialize native key event dispatch".into(),
        reason: "native key event metadata could not be serialized".into(),
    })?;
    let source = format!("globalThis.__glassDispatchHostEvents({encoded})");
    if source.len() > MAX_NATIVE_SCRIPT_BYTES {
        return Err(NativeEngineError::limit(
            "native key event dispatch",
            MAX_NATIVE_SCRIPT_BYTES,
            source.len(),
        ));
    }
    Ok(Some(source))
}

/// One persistent ECMAScript realm. A full navigation creates a new value;
/// same-document navigation retains it, matching a page global object's
/// lifetime.
pub(crate) struct NativeJavaScriptRuntime {
    runtime: Runtime,
    context: Context,
    deadline: Arc<Mutex<Option<Instant>>>,
    module_sources: Arc<Mutex<BTreeMap<String, String>>>,
    storage: Arc<Mutex<NativeWebStorageState>>,
    ready_state: String,
    clock_origin: Instant,
}

impl NativeJavaScriptRuntime {
    pub(crate) fn new() -> Result<Self, NativeEngineError> {
        let runtime = Runtime::new().map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript runtime".into(),
            reason: "native JavaScript runtime could not be created".into(),
        })?;
        let module_sources = Arc::new(Mutex::new(BTreeMap::new()));
        runtime.set_loader(
            NativeModuleResolver,
            NativeModuleLoader {
                sources: Arc::clone(&module_sources),
            },
        );
        runtime.set_memory_limit(NATIVE_SCRIPT_MEMORY_BYTES);
        runtime.set_max_stack_size(NATIVE_SCRIPT_STACK_BYTES);
        let deadline = Arc::new(Mutex::new(None));
        let interrupt_deadline = Arc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt_deadline
                .lock()
                .ok()
                .and_then(|deadline| *deadline)
                .is_some_and(|deadline| Instant::now() >= deadline)
        })));
        let context = Context::full(&runtime).map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript context".into(),
            reason: "native JavaScript context could not be created".into(),
        })?;
        Ok(Self {
            runtime,
            context,
            deadline,
            module_sources,
            storage: Arc::new(Mutex::new(NativeWebStorageState::default())),
            ready_state: "complete".into(),
            clock_origin: Instant::now(),
        })
    }

    pub(crate) fn set_storage_state(&self, state: NativeWebStorageState) {
        if let Ok(mut current) = self.storage.lock() {
            *current = state;
        }
    }

    pub(crate) fn storage_state(&self) -> NativeWebStorageState {
        self.storage
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    fn now_ms(&self) -> u64 {
        self.clock_origin
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    pub(crate) fn set_ready_state(&mut self, ready_state: &str) {
        self.ready_state.clear();
        self.ready_state.push_str(ready_state);
    }

    fn storage_view(&self, document_url: &str, origin: &NativeOrigin) -> NativeWebStorageView {
        let key = storage_key(document_url, origin);
        let Ok(state) = self.storage.lock() else {
            return NativeWebStorageView::default();
        };
        NativeWebStorageView {
            local: state.local.get(&key).cloned().unwrap_or_default(),
            session: state.session.get(&key).cloned().unwrap_or_default(),
        }
    }

    fn apply_storage_command(
        &self,
        command: &NativeScriptCommand,
        document_url: &str,
        origin: &NativeOrigin,
    ) -> Result<bool, NativeEngineError> {
        let (scope, entry_key, value, operation) = match command {
            NativeScriptCommand::StorageSet { scope, key, value } => (
                scope.as_str(),
                Some(key.as_str()),
                Some(value.as_str()),
                "set",
            ),
            NativeScriptCommand::StorageRemove { scope, key } => {
                (scope.as_str(), Some(key.as_str()), None, "remove")
            }
            NativeScriptCommand::StorageClear { scope } => (scope.as_str(), None, None, "clear"),
            _ => return Ok(false),
        };
        let mut state = self.storage.lock().map_err(|_| NativeEngineError::Worker {
            operation: "native Web Storage".into(),
            reason: "native Web Storage state lock is unavailable".into(),
        })?;
        let storage = match scope {
            "local" => &mut state.local,
            "session" => &mut state.session,
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Web Storage scope",
                    "must be local or session",
                ));
            }
        };
        let origin_key = storage_key(document_url, origin);
        let entries = storage.entry(origin_key).or_default();
        if let Some(entry_key) = entry_key {
            if entry_key.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage key",
                    crate::browser_backend::MAX_BACKEND_ID_BYTES,
                    entry_key.len(),
                ));
            }
        }
        if let Some(value) = value {
            if value.len() > crate::browser_backend::MAX_TEXT_BYTES {
                return Err(NativeEngineError::limit(
                    "native Web Storage value",
                    crate::browser_backend::MAX_TEXT_BYTES,
                    value.len(),
                ));
            }
        }
        match operation {
            "set" => {
                if entries.len() >= crate::browser_backend::MAX_STORAGE_ENTRIES
                    && entry_key.is_some_and(|key| !entries.contains_key(key))
                {
                    return Err(NativeEngineError::limit(
                        "native Web Storage entries",
                        crate::browser_backend::MAX_STORAGE_ENTRIES,
                        entries.len().saturating_add(1),
                    ));
                }
                entries.insert(
                    entry_key.expect("storage set key").to_owned(),
                    value.expect("storage set value").to_owned(),
                );
            }
            "remove" => {
                entries.remove(entry_key.expect("storage remove key"));
            }
            "clear" => entries.clear(),
            _ => unreachable!("storage operation matched above"),
        }
        Ok(true)
    }

    fn set_module_sources(&self, sources: BTreeMap<String, String>) {
        if let Ok(mut current) = self.module_sources.lock() {
            *current = sources;
        }
    }

    pub(crate) fn evaluate(
        &self,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "script source",
                "must not be empty",
            ));
        }
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "script source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        let bootstrap = document_bootstrap(
            document,
            document_url,
            origin,
            viewport,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
        )?;
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let result = self.context.with(|ctx| {
            ctx.eval::<(), _>(bootstrap.as_str())
                .map_err(|_| NativeEngineError::Worker {
                    operation: "install JavaScript host view".into(),
                    reason: "native JavaScript host view could not be installed".into(),
                })?;
            let (value, async_evaluation): (Value, bool) = match ctx.eval::<Value, _>(source) {
                Ok(value) => (value, false),
                Err(_) if contains_await_token(source) => (
                    ctx.eval_promise(source)
                        .and_then(|promise| promise.finish::<Value>())
                        .map_err(|_| NativeEngineError::Worker {
                            operation: "evaluate JavaScript".into(),
                            reason: "JavaScript evaluation failed".into(),
                        })?,
                    true,
                ),
                Err(_) => {
                    return Err(NativeEngineError::Worker {
                        operation: "evaluate JavaScript".into(),
                        reason: "JavaScript evaluation failed".into(),
                    });
                }
            };
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut document_commands = Vec::with_capacity(commands.len());
            for command in commands {
                if self.apply_storage_command(&command, document_url, origin)? {
                    continue;
                }
                document_commands.push(command);
            }
            let commands = document_commands;
            let json = ctx
                .json_stringify(value)
                .map_err(|_| NativeEngineError::Worker {
                    operation: "serialize JavaScript result".into(),
                    reason: "JavaScript result could not be serialized".into(),
                })?;
            let Some(json) = json else {
                return Ok(NativeScriptEvaluation {
                    value: serde_json::Value::Null,
                    commands,
                });
            };
            let json = json.to_string().map_err(|_| NativeEngineError::Worker {
                operation: "serialize JavaScript result".into(),
                reason: "JavaScript result could not be converted to UTF-8".into(),
            })?;
            if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
                return Err(NativeEngineError::limit(
                    "script result",
                    MAX_NATIVE_SCRIPT_RESULT_BYTES,
                    json.len(),
                ));
            }
            let mut result: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                    operation: "decode JavaScript result".into(),
                    reason: "JavaScript result was not valid JSON".into(),
                })?;
            if async_evaluation
                && let Some(object) = result.as_object_mut()
                && object.len() == 1
                && let Some(value) = object.remove("value")
            {
                result = value;
            }
            Ok(NativeScriptEvaluation {
                value: result,
                commands,
            })
        });
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    pub(crate) fn resolve_fetch(
        &self,
        request_id: u32,
        payload: &serde_json::Value,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        let serialized = serde_json::to_string(payload).map_err(|_| NativeEngineError::Worker {
            operation: "serialize JavaScript fetch response".into(),
            reason: "native fetch response could not be serialized".into(),
        })?;
        if serialized.len() > MAX_NATIVE_SCRIPT_BYTES {
            return self.evaluate(
                &format!(
                    "globalThis.__glassResolveFetch({request_id}, {{ error: \"fetch response exceeded the script transfer limit\" }});"
                ),
                document,
                document_url,
                origin,
                viewport,
            );
        }
        self.evaluate(
            &format!("globalThis.__glassResolveFetch({request_id}, {serialized});"),
            document,
            document_url,
            origin,
            viewport,
        )
    }

    pub(crate) fn evaluate_module(
        &self,
        name: &str,
        source: &str,
        document: &NativeDocument,
        document_url: &str,
        origin: &NativeOrigin,
        viewport: Viewport,
    ) -> Result<NativeScriptEvaluation, NativeEngineError> {
        if name.is_empty() {
            return Err(NativeEngineError::invalid(
                "module name",
                "must not be empty",
            ));
        }
        if source.is_empty() {
            return Err(NativeEngineError::invalid(
                "module source",
                "must not be empty",
            ));
        }
        if source.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "module source",
                MAX_NATIVE_SCRIPT_BYTES,
                source.len(),
            ));
        }
        let bootstrap = document_bootstrap(
            document,
            document_url,
            origin,
            viewport,
            &self.ready_state,
            self.now_ms(),
            &self.storage_view(document_url, origin),
        )?;
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let result = self.context.with(|ctx| {
            ctx.eval::<(), _>(bootstrap.as_str())
                .map_err(|_| NativeEngineError::Worker {
                    operation: "install JavaScript host view".into(),
                    reason: "native JavaScript host view could not be installed".into(),
                })?;
            Module::evaluate(ctx.clone(), name, source)
                .and_then(|promise| promise.finish::<()>())
                .map_err(|_| NativeEngineError::Worker {
                    operation: "evaluate JavaScript module".into(),
                    reason: "JavaScript module evaluation failed".into(),
                })?;
            for _ in 0..MAX_NATIVE_MODULE_IMPORTS {
                if !ctx.execute_pending_job() {
                    break;
                }
            }
            let commands = read_script_commands(ctx.clone())?;
            let mut document_commands = Vec::with_capacity(commands.len());
            for command in commands {
                if self.apply_storage_command(&command, document_url, origin)? {
                    continue;
                }
                document_commands.push(command);
            }
            Ok(NativeScriptEvaluation {
                value: serde_json::Value::Null,
                commands: document_commands,
            })
        });
        if let Ok(mut current) = self.deadline.lock() {
            *current = None;
        }
        result
    }

    #[allow(dead_code)]
    pub(crate) fn has_pending_jobs(&self) -> bool {
        self.runtime.is_job_pending()
    }
}

fn read_script_commands<'js>(
    ctx: rquickjs::Ctx<'js>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let json: String = ctx
        .eval("JSON.stringify(globalThis.__glassHostCommands || [])")
        .map_err(|_| NativeEngineError::Worker {
            operation: "collect JavaScript host commands".into(),
            reason: "native JavaScript host commands could not be collected".into(),
        })?;
    if json.len() > MAX_NATIVE_SCRIPT_RESULT_BYTES {
        return Err(NativeEngineError::limit(
            "script host commands",
            MAX_NATIVE_SCRIPT_RESULT_BYTES,
            json.len(),
        ));
    }
    let commands = serde_json::from_str::<Vec<NativeScriptCommand>>(&json).map_err(|_| {
        NativeEngineError::Worker {
            operation: "decode JavaScript host commands".into(),
            reason: "native JavaScript host commands were invalid".into(),
        }
    })?;
    if commands.len() > super::interaction::MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "script host commands",
            super::interaction::MAX_NATIVE_EFFECTS,
            commands.len(),
        ));
    }
    Ok(commands)
}

fn contains_await_token(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' | b'`' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = index.saturating_add(2);
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'_'
                        || bytes[index] == b'$')
                {
                    index += 1;
                }
                if &bytes[start..index] == b"await" {
                    return true;
                }
            }
            _ => index += 1,
        }
    }
    false
}

/// Extract the bounded static import/export specifiers from a module source.
///
/// This is intentionally a lexical prefetch pass, not a replacement for the
/// JavaScript parser. QuickJS remains authoritative for module grammar and
/// evaluation; this pass only discovers URLs that the content process must
/// fetch before installing the in-memory module loader.
pub(crate) fn static_module_specifiers(source: &str) -> Result<Vec<String>, NativeEngineError> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut specifiers = Vec::new();
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if !is_javascript_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
            index += 1;
        }
        let keyword = &bytes[start..index];
        if keyword != b"import" && keyword != b"export" {
            continue;
        }
        if let Some(specifier) = module_specifier_after_keyword(bytes, index, keyword == b"import")
        {
            if specifier.is_empty() {
                return Err(NativeEngineError::invalid(
                    "module import",
                    "module specifier must not be empty",
                ));
            }
            specifiers.push(specifier);
            if specifiers.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module imports",
                    MAX_NATIVE_MODULE_IMPORTS,
                    specifiers.len(),
                ));
            }
        }
    }
    Ok(specifiers)
}

/// Extract literal dynamic-import specifiers. Computed expressions remain
/// unresolved and therefore fail through the bounded module loader instead of
/// receiving an implicit network capability.
pub(crate) fn literal_dynamic_module_specifiers(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut specifiers = Vec::new();
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() {
            break;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if !is_javascript_identifier_start(bytes[index]) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
            index += 1;
        }
        if &bytes[start..index] != b"import" {
            continue;
        }
        let argument = skip_javascript_space_and_comments(bytes, index);
        if bytes.get(argument) != Some(&b'(') {
            continue;
        }
        let specifier_start = skip_javascript_space_and_comments(bytes, argument + 1);
        if let Some((specifier, _)) = read_javascript_string(bytes, specifier_start) {
            specifiers.push(specifier);
            if specifiers.len() >= MAX_NATIVE_MODULE_IMPORTS {
                break;
            }
        }
    }
    specifiers
}

fn module_specifier_after_keyword(
    bytes: &[u8],
    keyword_end: usize,
    import_keyword: bool,
) -> Option<String> {
    let start = skip_javascript_space_and_comments(bytes, keyword_end);
    if import_keyword && bytes.get(start) == Some(&b'(') {
        return None;
    }
    if bytes
        .get(start)
        .is_some_and(|byte| matches!(byte, b'\'' | b'"'))
    {
        return read_javascript_string(bytes, start).map(|(value, _)| value);
    }
    let mut index = start;
    while index < bytes.len() {
        index = skip_javascript_space_and_comments(bytes, index);
        if index >= bytes.len() || bytes[index] == b';' {
            return None;
        }
        if matches!(bytes[index], b'\'' | b'"' | b'`') {
            index = skip_javascript_string(bytes, index);
            continue;
        }
        if is_javascript_identifier_start(bytes[index]) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_javascript_identifier_continue(bytes[index]) {
                index += 1;
            }
            if &bytes[start..index] == b"from" {
                let specifier_start = skip_javascript_space_and_comments(bytes, index);
                return read_javascript_string(bytes, specifier_start).map(|(value, _)| value);
            }
            continue;
        }
        index += 1;
    }
    None
}

fn skip_javascript_space_and_comments(bytes: &[u8], mut index: usize) -> usize {
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'/') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes.get(index) == Some(&b'/') && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        return index;
    }
}

fn skip_javascript_string(bytes: &[u8], mut index: usize) -> usize {
    let Some(&quote) = bytes.get(index) else {
        return index;
    };
    index += 1;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index = index.saturating_add(2);
        } else if bytes[index] == quote {
            return index + 1;
        } else {
            index += 1;
        }
    }
    index
}

fn read_javascript_string(bytes: &[u8], mut index: usize) -> Option<(String, usize)> {
    let quote = *bytes.get(index)?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    index += 1;
    let mut value = String::new();
    while index < bytes.len() {
        match bytes[index] {
            byte if byte == quote => return Some((value, index + 1)),
            b'\\' if index + 1 < bytes.len() => {
                value.push(bytes[index + 1] as char);
                index += 2;
            }
            byte => {
                value.push(byte as char);
                index += 1;
            }
        }
    }
    None
}

fn is_javascript_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

fn is_javascript_identifier_continue(byte: u8) -> bool {
    is_javascript_identifier_start(byte) || byte.is_ascii_digit()
}

fn storage_key(document_url: &str, origin: &NativeOrigin) -> String {
    if matches!(origin, NativeOrigin::Opaque) {
        return format!(
            "opaque:{}",
            document_url
                .split_once('#')
                .map_or(document_url, |(url, _)| url)
        );
    }
    origin.serialized()
}

fn document_bootstrap(
    document: &NativeDocument,
    document_url: &str,
    origin: &NativeOrigin,
    viewport: Viewport,
    ready_state: &str,
    now_ms: u64,
    storage: &NativeWebStorageView,
) -> Result<String, NativeEngineError> {
    let state = document.script_snapshot(crate::browser_backend::MAX_TEXT_BYTES);
    let serialized = serde_json::to_string(&serde_json::json!({
        "url": document_url,
        "origin": origin.serialized(),
        "state": state,
        "now_ms": now_ms,
        "storage": storage,
    }))
    .map_err(|_| NativeEngineError::Worker {
        operation: "serialize JavaScript host view".into(),
        reason: "native JavaScript host view could not be serialized".into(),
    })?;
    let ready_state =
        serde_json::to_string(ready_state).map_err(|_| NativeEngineError::Worker {
            operation: "serialize document ready state".into(),
            reason: "native document ready state could not be serialized".into(),
        })?;
    Ok(format!(
        r###"(() => {{
  const host = {serialized};
  const state = host.state;
  const commands = [];
  const activeCommands = () => Array.isArray(globalThis.__glassHostCommandBuffer)
    ? globalThis.__glassHostCommandBuffer
    : commands;
  const pushCommand = (command) => {{
    const target = activeCommands();
    if (target.length >= {max_commands}) throw new RangeError("native host command limit exceeded");
    target.push(command);
  }};
  const timers = globalThis.__glassTimers instanceof Map
    ? globalThis.__glassTimers
    : new Map();
  const runningTimers = globalThis.__glassRunningTimers instanceof Map
    ? globalThis.__glassRunningTimers
    : new Map();
  let nextTimerId = Number.isSafeInteger(globalThis.__glassNextTimerId)
    ? globalThis.__glassNextTimerId
    : 1;
  const scheduleTimer = (callback, delay, args, repeating) => {{
    if (typeof callback !== "function") throw new TypeError("timer callback must be callable");
    if (timers.size >= {max_timers}) throw new RangeError("native timer limit exceeded");
    const id = nextTimerId;
    nextTimerId += 1;
    globalThis.__glassNextTimerId = nextTimerId;
    const numericDelay = Number(delay);
    const normalizedDelay = Number.isFinite(numericDelay)
      ? Math.max(0, Math.min(2147483647, numericDelay))
      : 0;
    timers.set(id, {{
      callback,
      args,
      dueAt: host.now_ms + normalizedDelay,
      intervalMs: repeating ? Math.max(1, normalizedDelay) : 0,
      cancelled: false,
    }});
    return id;
  }};
  const setTimeoutNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, false);
  const setIntervalNative = (callback, delay, ...args) =>
    scheduleTimer(callback, delay, args, true);
  const clearTimer = (id) => {{
    const timerId = Number(id);
    const timer = timers.get(timerId) || runningTimers.get(timerId);
    if (timer) timer.cancelled = true;
    timers.delete(timerId);
  }};
  const storageEntryLimit = {storage_entry_limit};
  const storageKeyLimit = {storage_key_limit};
  const storageValueLimit = {storage_value_limit};
  const boundedStorageText = (value, limit, field) => {{
    const text = String(value);
    if (text.length > limit) throw new RangeError("native storage " + field + " exceeds its limit");
    return text;
  }};
  const createStorage = (mapSlot, objectSlot, initialValues) => {{
    const hasExistingValues = globalThis[mapSlot] instanceof Map;
    const values = hasExistingValues ? globalThis[mapSlot] : new Map();
    if (!hasExistingValues && initialValues && typeof initialValues === "object") {{
      for (const key of Object.keys(initialValues)) {{
        values.set(key, String(initialValues[key]));
      }}
    }}
    globalThis[mapSlot] = values;
    const existing = globalThis[objectSlot];
    if (existing && existing.__glassNativeStorage === true) return existing;
    const store = {{
      get length() {{ return values.size; }},
      key(index) {{
        const position = Number(index);
        if (!Number.isInteger(position) || position < 0) return null;
        return Array.from(values.keys())[position] ?? null;
      }},
      getItem(key) {{
        const value = values.get(String(key));
        return value === undefined ? null : value;
      }},
      setItem(key, value) {{
        const normalizedKey = boundedStorageText(key, storageKeyLimit, "key");
        const normalizedValue = boundedStorageText(value, storageValueLimit, "value");
        if (!values.has(normalizedKey) && values.size >= storageEntryLimit) {{
          throw new RangeError("native storage entry limit exceeded");
        }}
        values.set(normalizedKey, normalizedValue);
        pushCommand({{ kind: "storageSet", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session", key: normalizedKey, value: normalizedValue }});
      }},
      removeItem(key) {{
        const normalizedKey = String(key);
        if (!values.delete(normalizedKey)) return;
        pushCommand({{ kind: "storageRemove", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session", key: normalizedKey }});
      }},
      clear() {{
        if (values.size === 0) return;
        values.clear();
        pushCommand({{ kind: "storageClear", scope: mapSlot === "__glassLocalStorageValues" ? "local" : "session" }});
      }},
    }};
    Object.defineProperty(store, "__glassNativeStorage", {{
      value: true,
      enumerable: false,
      configurable: false,
    }});
    globalThis[objectSlot] = store;
    return store;
  }};
  globalThis.localStorage = createStorage("__glassLocalStorageValues", "__glassLocalStorageObject", host.storage.local);
  globalThis.sessionStorage = createStorage("__glassSessionStorageValues", "__glassSessionStorageObject", host.storage.session);
  globalThis.__glassTimers = timers;
  globalThis.__glassRunningTimers = runningTimers;
  globalThis.__glassNextTimerId = nextTimerId;
  globalThis.setTimeout = setTimeoutNative;
  globalThis.setInterval = setIntervalNative;
  globalThis.clearTimeout = clearTimer;
  globalThis.clearInterval = clearTimer;
  const fetchRequests = globalThis.__glassFetchRequests instanceof Map
    ? globalThis.__glassFetchRequests
    : new Map();
  let nextFetchRequestId = Number.isSafeInteger(globalThis.__glassNextFetchRequestId)
    ? globalThis.__glassNextFetchRequestId
    : 1;
  const formDataEntries = (form) => {{
    if (!form || form.tagName !== "FORM") {{
      throw new TypeError("FormData constructor requires a form element");
    }}
    const entries = [];
    for (const control of elements) {{
      if (control.formOwnerIndex !== form.nodeIndex || control.disabled) continue;
      const name = String(control.getAttribute("name") || "");
      if (!name) continue;
      if (control.tagName === "SELECT") {{
        const options = elements.filter(option =>
          option.tagName === "OPTION"
            && option.parentIndex === control.nodeIndex
            && option.selected
            && !option.disabled
        );
        const selectedOptions = control.getAttribute("multiple") === null
          ? options.slice(0, 1)
          : options;
        for (const option of selectedOptions) {{
          const optionValue = option.getAttribute("value");
          entries.push([name, optionValue === null ? String(option.textContent || "") : String(optionValue)]);
        }}
        continue;
      }}
      const type = String(control.getAttribute("type") || "text").toLowerCase();
      if (["button", "reset", "submit", "image"].includes(type)) continue;
      if (type === "file") throw new TypeError("native FormData file controls are unsupported");
      if (["checkbox", "radio"].includes(type) && !control.checked) continue;
      if (control.tagName === "OPTION") continue;
      entries.push([name, String(control.value)]);
    }}
    return entries;
  }};
  const blobPartText = (part) => {{
    if (part && part.__glassNativeBlob === true) return part._text;
    if (typeof part === "string") return part;
    throw new TypeError("native Blob supports only text or Blob parts");
  }};
  const boundedBlobText = (parts) => {{
    if (parts === undefined || parts === null) return "";
    if (!Array.isArray(parts)) throw new TypeError("native Blob parts must be an array");
    const text = parts.map(blobPartText).join("");
    if (text.length > storageValueLimit) throw new RangeError("native Blob size limit exceeded");
    return text;
  }};
  const normalizeBlobType = (options) => {{
    const type = options && typeof options.type === "string"
      ? options.type.toLowerCase()
      : "";
    return /^[\x20-\x7e]*$/.test(type) ? type : "";
  }};
  const BlobNative = function(parts, options) {{
    this.__glassNativeBlob = true;
    this._text = boundedBlobText(parts);
    this.size = this._text.length;
    this.type = normalizeBlobType(options);
  }};
  BlobNative.prototype.text = function() {{
    return Promise.resolve(this._text);
  }};
  BlobNative.prototype.slice = function(start, end, contentType) {{
    const length = this._text.length;
    const normalizePosition = (value, fallback) => {{
      if (value === undefined) return fallback;
      const number = Number(value);
      if (!Number.isFinite(number)) return fallback;
      return number < 0 ? Math.max(length + Math.trunc(number), 0) : Math.min(Math.trunc(number), length);
    }};
    const begin = normalizePosition(start, 0);
    const finish = normalizePosition(end, length);
    return new BlobNative([begin > finish ? "" : this._text.slice(begin, finish)], {{ type: contentType }});
  }};
  const FileNative = function(parts, name, options) {{
    if (name === undefined) throw new TypeError("native File requires a name");
    BlobNative.call(this, parts, options);
    this.__glassNativeFile = true;
    this.name = String(name);
    const modified = options && Number.isFinite(Number(options.lastModified))
      ? Number(options.lastModified)
      : 0;
    this.lastModified = Math.max(0, modified);
  }};
  FileNative.prototype = Object.create(BlobNative.prototype);
  FileNative.prototype.constructor = FileNative;
  globalThis.Blob = BlobNative;
  globalThis.File = FileNative;
  const formDataValue = (value, filename) => {{
    if (value && value.__glassNativeBlob === true) {{
      const defaultFilename = value.__glassNativeFile === true ? value.name : "blob";
      return {{
        kind: "file",
        value,
        filename: filename === undefined ? defaultFilename : String(filename),
      }};
    }}
    return String(value);
  }};
  const FormDataNative = function(form) {{
    this.__glassFormData = true;
    this._entries = [];
    if (form !== undefined && form !== null) this._entries = formDataEntries(form);
  }};
  FormDataNative.prototype.append = function(name, value, filename) {{
    this._entries.push([String(name), formDataValue(value, filename)]);
  }};
  FormDataNative.prototype.set = function(name, value, filename) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    this._entries.push([key, formDataValue(value, filename)]);
  }};
  FormDataNative.prototype.delete = function(name) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
  }};
  FormDataNative.prototype.get = function(name) {{
    const key = String(name);
    const entry = this._entries.find(candidate => candidate[0] === key);
    return entry ? (entry[1].kind === "file" ? entry[1].value : entry[1]) : null;
  }};
  FormDataNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries
      .filter(entry => entry[0] === key)
      .map(entry => entry[1].kind === "file" ? entry[1].value : entry[1]);
  }};
  FormDataNative.prototype.has = function(name) {{
    const key = String(name);
    return this._entries.some(entry => entry[0] === key);
  }};
  FormDataNative.prototype.entries = function() {{
    return this._entries.map(entry => [
      entry[0],
      entry[1].kind === "file" ? entry[1].value : entry[1],
    ]);
  }};
  FormDataNative.prototype.forEach = function(callback, thisArg) {{
    if (typeof callback !== "function") throw new TypeError("FormData callback must be callable");
    this._entries.forEach(entry => callback.call(
      thisArg,
      entry[1].kind === "file" ? entry[1].value : entry[1],
      entry[0],
      this,
    ));
  }};
  const escapeFormDataName = value => String(value)
    .replace(/\\/g, "\\\\")
    .replace(/"/g, "\\\"")
    .replace(/\r/g, "%0D")
    .replace(/\n/g, "%0A");
  const serializeFormData = (formData, requestId) => {{
    const boundary = "----GlassNativeForm" + requestId;
    let body = "";
    for (const entry of formData._entries) {{
      body += "--" + boundary + "\r\n";
      const file = entry[1].kind === "file" ? entry[1] : null;
      body += "Content-Disposition: form-data; name=\"" + escapeFormDataName(entry[0]) + "\"";
      if (file) body += "; filename=\"" + escapeFormDataName(file.filename) + "\"";
      body += "\r\n";
      if (file) body += "Content-Type: " + (file.value.type || "application/octet-stream") + "\r\n";
      body += "\r\n";
      body += file ? file.value._text : entry[1];
      body += "\r\n";
    }}
    body += "--" + boundary + "--\r\n";
    return {{ body, contentType: "multipart/form-data; boundary=" + boundary }};
  }};
  globalThis.FormData = FormDataNative;
  const URLSearchParamsNative = function(init) {{
    this.__glassUrlSearchParams = true;
    this._entries = [];
    if (init === undefined || init === null) return;
    if (typeof init === "string") {{
      for (const part of init.split("&")) {{
        if (!part) continue;
        const pieces = part.split("=");
        const decode = value => decodeURIComponent(String(value).replace(/\+/g, " "));
        this._entries.push([decode(pieces.shift()), decode(pieces.join("="))]);
      }}
      return;
    }}
    if (init.__glassUrlSearchParams === true) {{
      this._entries = init._entries.map(entry => [entry[0], entry[1]]);
      return;
    }}
    throw new TypeError("native URLSearchParams accepts only text or URLSearchParams");
  }};
  URLSearchParamsNative.prototype.append = function(name, value) {{
    this._entries.push([String(name), String(value)]);
  }};
  URLSearchParamsNative.prototype.set = function(name, value) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
    this._entries.push([key, String(value)]);
  }};
  URLSearchParamsNative.prototype.delete = function(name) {{
    const key = String(name);
    this._entries = this._entries.filter(entry => entry[0] !== key);
  }};
  URLSearchParamsNative.prototype.get = function(name) {{
    const key = String(name);
    const entry = this._entries.find(candidate => candidate[0] === key);
    return entry ? entry[1] : null;
  }};
  URLSearchParamsNative.prototype.getAll = function(name) {{
    const key = String(name);
    return this._entries.filter(entry => entry[0] === key).map(entry => entry[1]);
  }};
  URLSearchParamsNative.prototype.has = function(name) {{
    const key = String(name);
    return this._entries.some(entry => entry[0] === key);
  }};
  URLSearchParamsNative.prototype.entries = function() {{ return this._entries.slice(); }};
  URLSearchParamsNative.prototype.toString = function() {{
    const encode = value => encodeURIComponent(String(value)).replace(/%20/g, "+");
    return this._entries.map(entry => encode(entry[0]) + "=" + encode(entry[1])).join("&");
  }};
  globalThis.URLSearchParams = URLSearchParamsNative;
  const fetchNative = (input, options) => {{
    if (typeof input !== "string") throw new TypeError("native fetch requires a URL string");
    const settings = options && typeof options === "object" ? options : {{}};
    const method = settings.method === undefined ? "GET" : String(settings.method).toUpperCase();
    const rawBody = settings.body === undefined || settings.body === null
      ? null
      : String(settings.body);
    const formData = settings.body && settings.body.__glassFormData === true
      ? settings.body
      : null;
    const urlSearchParams = settings.body && settings.body.__glassUrlSearchParams === true
      ? settings.body
      : null;
    let body = rawBody;
    if (method !== "GET" && method !== "POST") {{
      return Promise.reject(new TypeError("native fetch supports only GET and POST requests"));
    }}
    const headers = settings.headers && typeof settings.headers === "object"
      ? settings.headers
      : {{}};
    let contentType = null;
    for (const name of Object.keys(headers)) {{
      if (String(name).toLowerCase() !== "content-type") {{
        return Promise.reject(new TypeError("native fetch only supports the Content-Type header"));
      }}
      contentType = String(headers[name]);
    }}
    const requestId = nextFetchRequestId;
    if (formData) {{
      if (contentType !== null) return Promise.reject(new TypeError("FormData chooses its own Content-Type boundary"));
      const serialized = serializeFormData(formData, requestId);
      body = serialized.body;
      contentType = serialized.contentType;
    }}
    if (urlSearchParams) {{
      if (contentType !== null) return Promise.reject(new TypeError("URLSearchParams chooses its own Content-Type"));
      body = urlSearchParams.toString();
      contentType = "application/x-www-form-urlencoded;charset=UTF-8";
    }}
    if (method === "GET" && body !== null) {{
      return Promise.reject(new TypeError("GET fetch requests must not have a body"));
    }}
    nextFetchRequestId += 1;
    globalThis.__glassNextFetchRequestId = nextFetchRequestId;
    const credentials = settings.credentials !== "omit";
    return new Promise((resolve, reject) => {{
      fetchRequests.set(requestId, {{ resolve, reject }});
      pushCommand({{ kind: "fetch", request_id: requestId, href: input, credentials, method, body, content_type: contentType }});
    }});
  }};
  const responseFromFetch = (payload) => Object.freeze({{
    ok: payload.status >= 200 && payload.status < 300,
    status: payload.status,
    url: payload.url,
    headers: Object.freeze({{
      get(name) {{
        return String(name).toLowerCase() === "content-type" ? payload.contentType : null;
      }}
    }}),
    text() {{ return Promise.resolve(payload.body); }},
    json() {{ return Promise.resolve(JSON.parse(payload.body)); }},
  }});
  const XMLHttpRequestNative = function() {{
    this.readyState = 0;
    this.status = 0;
    this.statusText = "";
    this.responseText = "";
    this.responseURL = "";
    this.response = "";
    this.withCredentials = false;
    this.onreadystatechange = null;
    this.onload = null;
    this.onerror = null;
    this._method = "GET";
    this._url = "";
    this._headers = {{}};
    this._responseContentType = null;
  }};
  XMLHttpRequestNative.prototype._notifyReadyState = function() {{
    if (typeof this.onreadystatechange === "function") this.onreadystatechange.call(this);
  }};
  XMLHttpRequestNative.prototype.open = function(method, url, async) {{
    if (async === false) throw new TypeError("native XMLHttpRequest requires async mode");
    const normalizedMethod = String(method).toUpperCase();
    if (normalizedMethod !== "GET" && normalizedMethod !== "POST")
      throw new TypeError("native XMLHttpRequest supports only GET and POST");
    if (typeof url !== "string") throw new TypeError("native XMLHttpRequest URL must be text");
    this._method = normalizedMethod;
    this._url = url;
    this._headers = {{}};
    this.readyState = 1;
    this._notifyReadyState();
  }};
  XMLHttpRequestNative.prototype.setRequestHeader = function(name, value) {{
    if (String(name).toLowerCase() !== "content-type")
      throw new TypeError("native XMLHttpRequest only supports the Content-Type header");
    this._headers["Content-Type"] = String(value);
  }};
  XMLHttpRequestNative.prototype.getResponseHeader = function(name) {{
    return String(name).toLowerCase() === "content-type" ? this._responseContentType : null;
  }};
  XMLHttpRequestNative.prototype.getAllResponseHeaders = function() {{
    return this._responseContentType
      ? "content-type: " + this._responseContentType + "\r\n"
      : "";
  }};
  XMLHttpRequestNative.prototype.send = function(body) {{
    if (this.readyState !== 1) throw new TypeError("native XMLHttpRequest is not open");
    const requestBody = body && (body.__glassFormData === true || body.__glassUrlSearchParams === true)
      ? body
      : body === undefined || body === null ? null : String(body);
    const request = fetchNative(this._url, {{
      method: this._method,
      body: requestBody,
      headers: this._headers,
      credentials: this.withCredentials ? "include" : "omit",
    }});
    request.then(response => {{
      this.status = response.status;
      this.statusText = String(response.status);
      this.responseURL = response.url;
      this._responseContentType = response.headers.get("content-type");
      return response.text();
    }}).then(text => {{
      this.responseText = text;
      this.response = text;
      this.readyState = 4;
      this._notifyReadyState();
      if (typeof this.onload === "function") this.onload.call(this, {{ type: "load", target: this }});
    }}).catch(error => {{
      this.readyState = 4;
      this._notifyReadyState();
      if (typeof this.onerror === "function") this.onerror.call(this, {{ type: "error", target: this, error }});
    }});
  }};
  globalThis.XMLHttpRequest = XMLHttpRequestNative;
  globalThis.__glassFetchRequests = fetchRequests;
  globalThis.__glassNextFetchRequestId = nextFetchRequestId;
  globalThis.fetch = fetchNative;
  globalThis.__glassResolveFetch = (requestId, payload) => {{
    const pending = fetchRequests.get(Number(requestId));
    if (!pending) return;
    fetchRequests.delete(Number(requestId));
    if (payload && payload.error) pending.reject(new Error(String(payload.error)));
    else pending.resolve(responseFromFetch(payload));
  }};
  globalThis.queueMicrotask = (callback) => {{
    if (typeof callback !== "function") throw new TypeError("microtask callback must be callable");
    Promise.resolve().then(callback);
  }};
  globalThis.__glassRunTimers = (currentNow) => {{
    const now = Number.isFinite(Number(currentNow)) ? Number(currentNow) : host.now_ms;
    const pending = Array.from(timers.entries())
      .filter(([, timer]) => Number(timer.dueAt === undefined ? 0 : timer.dueAt) <= now)
      .sort((left, right) => {{
        const due = Number(left[1].dueAt === undefined ? 0 : left[1].dueAt)
          - Number(right[1].dueAt === undefined ? 0 : right[1].dueAt);
        return due || left[0] - right[0];
    }});
    for (const [id, timer] of pending) {{
      if (!timers.has(id)) continue;
      timers.delete(id);
      runningTimers.set(id, timer);
      try {{
        timer.callback(...timer.args);
      }} finally {{
        runningTimers.delete(id);
        if (timer.intervalMs > 0 && !timer.cancelled) {{
          timer.dueAt = now + timer.intervalMs;
          timers.set(id, timer);
        }}
      }}
    }}
  }};
  const listeners = globalThis.__glassHostListeners instanceof Map
    ? globalThis.__glassHostListeners
    : new Map();
  globalThis.__glassHostListeners = listeners;
  const normalizeEventType = (type) => {{
    const value = String(type).toLowerCase();
    if (!value || value.length > 128) throw new TypeError("invalid native event type");
    return value;
  }};
  const listenerKey = (owner, type) => String(owner) + ":" + normalizeEventType(type);
  const listenerCount = () => {{
    let count = 0;
    for (const callbacks of listeners.values()) count += callbacks.length;
    return count;
  }};
  const listenerOptions = (options) => {{
    if (options === true) return {{ capture: true, once: false }};
    if (!options || typeof options !== "object") return {{ capture: false, once: false }};
    return {{ capture: Boolean(options.capture), once: Boolean(options.once) }};
  }};
  const addListener = (owner, type, callback, options) => {{
    if (typeof callback !== "function") throw new TypeError("event listener must be callable");
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key) || [];
    const settings = listenerOptions(options);
    if (callbacks.some((record) => record.callback === callback && record.capture === settings.capture)) return;
    if (listenerCount() >= {max_listeners}) throw new RangeError("native event listener limit exceeded");
    callbacks.push({{ callback, capture: settings.capture, once: settings.once }});
    listeners.set(key, callbacks);
  }};
  const removeListener = (owner, type, callback, options) => {{
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key);
    if (!callbacks) return;
    const capture = listenerOptions(options).capture;
    const index = callbacks.findIndex((record) => record.callback === callback && record.capture === capture);
    if (index < 0) return;
    callbacks.splice(index, 1);
    if (callbacks.length === 0) listeners.delete(key);
  }};
  const createEvent = (type, options) => {{
    const settings = options && typeof options === "object" ? options : {{}};
    const event = {{
      type: normalizeEventType(type),
      bubbles: Boolean(settings.bubbles),
      cancelable: Boolean(settings.cancelable),
      key: settings.key === undefined ? "" : String(settings.key),
      code: settings.code === undefined ? "" : String(settings.code),
      target: null,
      currentTarget: null,
      eventPhase: 0,
      defaultPrevented: false,
      returnValue: "",
      submitter: settings.submitter === undefined ? null : settings.submitter,
      oldURL: settings.oldURL === undefined ? "" : String(settings.oldURL),
      newURL: settings.newURL === undefined ? "" : String(settings.newURL),
      preventDefault() {{
        if (this.cancelable) this.defaultPrevented = true;
      }},
      stopPropagation() {{ eventState.stopped = true; }},
      stopImmediatePropagation() {{
        eventState.stopped = true;
        eventState.immediate = true;
      }},
    }};
    const eventState = {{ stopped: false, immediate: false, dispatching: false }};
    Object.defineProperty(event, "__glassState", {{
      value: eventState,
      enumerable: false,
      configurable: false,
    }});
    return event;
  }};
  const ownerFor = (target) => {{
    if (target === globalThis) return "window";
    if (target === document) return "document";
    return "node:" + target.nodeIndex;
  }};
  const invokeListeners = (target, event, capture, phase) => {{
    const owner = ownerFor(target);
    const callbacks = (listeners.get(listenerKey(owner, event.type)) || []).slice();
    const eventState = event.__glassState;
    event.currentTarget = target;
    event.eventPhase = phase;
    for (const record of callbacks) {{
      if (record.capture !== capture) continue;
      record.callback.call(target, event);
      if (record.once) removeListener(owner, event.type, record.callback, capture);
      if (eventState.immediate) break;
    }}
  }};
  const dispatchTarget = (target, event) => {{
    if (!event || typeof event.type !== "string") throw new TypeError("invalid native event");
    normalizeEventType(event.type);
    const eventState = event.__glassState;
    if (!eventState || eventState.dispatching) throw new TypeError("event is already being dispatched");
    eventState.dispatching = true;
    event.target = target;
    const path = [target];
    if (target !== globalThis && target !== document && typeof target.nodeIndex === "number") {{
      let parent = target.parentElement;
      while (parent) {{
        path.push(parent);
        parent = parent.parentElement;
      }}
      path.push(document, globalThis);
    }} else if (target === document) {{
      path.push(globalThis);
    }}
    for (let index = path.length - 1; index > 0; index -= 1) {{
      invokeListeners(path[index], event, true, 1);
      if (eventState.stopped || eventState.immediate) break;
    }}
    if (!eventState.immediate) {{
      invokeListeners(target, event, true, 2);
      if (!eventState.immediate) invokeListeners(target, event, false, 2);
    }}
    if (event.bubbles && !eventState.stopped && !eventState.immediate) {{
      for (let index = 1; index < path.length; index += 1) {{
        invokeListeners(path[index], event, false, 3);
        if (eventState.stopped || eventState.immediate) break;
      }}
    }}
    if (event.type === "beforeunload" && event.returnValue !== "") event.defaultPrevented = true;
    event.currentTarget = null;
    event.eventPhase = 0;
    eventState.dispatching = false;
    return !event.defaultPrevented;
  }};
  const validityFlags = (entry) => {{
    const source = entry.validity || {{}};
    const customError = String(entry.customValidity || "").length > 0;
    const validity = {{
      badInput: Boolean(source.badInput),
      customError,
      patternMismatch: Boolean(source.patternMismatch),
      rangeOverflow: Boolean(source.rangeOverflow),
      rangeUnderflow: Boolean(source.rangeUnderflow),
      stepMismatch: Boolean(source.stepMismatch),
      tooLong: Boolean(source.tooLong),
      tooShort: Boolean(source.tooShort),
      typeMismatch: Boolean(source.typeMismatch),
      valueMissing: Boolean(source.valueMissing),
      valid: false,
    }};
    validity.valid = String(entry.tagName).toUpperCase() === "FORM"
      ? Boolean(source.valid)
      : !validity.badInput && !validity.customError
        && !validity.patternMismatch && !validity.rangeOverflow
        && !validity.rangeUnderflow && !validity.stepMismatch
        && !validity.tooLong && !validity.tooShort
        && !validity.typeMismatch && !validity.valueMissing;
    return validity;
  }};
  const validationMessageFor = (entry, validity) => {{
    const custom = String(entry.customValidity || "");
    if (custom.length > 0) return custom;
    if (validity.valueMissing) return "Please fill out this field.";
    if (validity.typeMismatch || validity.badInput) return "Please enter a valid value.";
    if (validity.tooShort) return "Value is too short.";
    if (validity.tooLong) return "Value is too long.";
    if (validity.rangeUnderflow) return "Value is below the minimum.";
    if (validity.rangeOverflow) return "Value is above the maximum.";
    if (validity.stepMismatch) return "Value does not match the required step.";
    if (validity.patternMismatch) return "Please match the requested format.";
    return "";
  }};
  const makeElement = (initialEntry) => {{
    let entry = initialEntry;
    const element = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex,
      formOwnerIndex: entry.formOwnerIndex,
      tagName: entry.tagName.toUpperCase(),
      id: entry.attributes.id || "",
      className: entry.attributes.class || "",
      textContent: entry.text,
      innerText: entry.text,
      value: entry.value === null
        ? (entry.tagName.toLowerCase() === "option"
          ? (entry.attributes.value === undefined ? entry.text : entry.attributes.value)
          : "")
        : entry.value,
      checked: entry.checked,
      selected: entry.selected,
      multiple: Object.prototype.hasOwnProperty.call(entry.attributes, "multiple"),
      disabled: entry.disabled,
      hidden: entry.hidden,
      focused: entry.focused,
      get validity() {{ return validityFlags(entry); }},
      get validationMessage() {{
        return validationMessageFor(entry, validityFlags(entry));
      }},
      get willValidate() {{ return Boolean(entry.willValidate); }},
      getAttribute(name) {{
        const key = String(name).toLowerCase();
        for (const attr of Object.keys(entry.attributes)) {{
          if (attr.toLowerCase() === key) return entry.attributes[attr];
        }}
        return null;
      }},
      hasAttribute(name) {{ return this.getAttribute(name) !== null; }},
      addEventListener(type, callback, options) {{
        addListener("node:" + entry.nodeIndex, type, callback, options);
      }},
      removeEventListener(type, callback, options) {{
        removeListener("node:" + entry.nodeIndex, type, callback, options);
      }},
      dispatchEvent(event) {{
        return dispatchTarget(this, event);
      }},
      focus() {{
        if (this.disabled || this.hidden) return;
        setLocalFocus(this);
      }},
      blur() {{
        if (!this.focused) return;
        this.focused = false;
        pushCommand({{ kind: "blur", node_index: entry.nodeIndex }});
        dispatchTarget(this, createEvent("blur"));
      }},
      click() {{
        if (this.disabled || this.hidden) return;
        this.focus();
        const event = createEvent("click", {{ bubbles: true, cancelable: true }});
        if (!dispatchTarget(this, event)) return;
        if (this.tagName === "INPUT") {{
          const type = String(entry.attributes.type || "text").toLowerCase();
          if (type === "checkbox") checked = !checked;
          if (type === "radio") checked = true;
        }}
        pushCommand({{ kind: "click", node_index: entry.nodeIndex }});
      }},
      submit() {{
        if (this.tagName !== "FORM") throw new TypeError("submit requires a form");
        pushCommand({{ kind: "submitForm", node_index: entry.nodeIndex }});
      }},
      requestSubmit() {{
        if (this.tagName !== "FORM") throw new TypeError("requestSubmit requires a form");
        const submitter = arguments.length === 0 ? null : arguments[0];
        if (submitter !== null && (typeof submitter !== "object" || typeof submitter.nodeIndex !== "number")) {{
          throw new TypeError("requestSubmit submitter must be an element");
        }}
        pushCommand({{ kind: "requestSubmitForm", node_index: entry.nodeIndex, submitter_index: submitter === null ? null : submitter.nodeIndex }});
      }},
      checkValidity() {{
        const valid = validityFlags(entry).valid;
        if (!valid) pushCommand({{ kind: "checkValidity", node_index: entry.nodeIndex }});
        return valid;
      }},
      reportValidity() {{
        const valid = validityFlags(entry).valid;
        if (!valid) pushCommand({{ kind: "reportValidity", node_index: entry.nodeIndex }});
        return valid;
      }},
      setCustomValidity(message) {{
        const value = String(message);
        entry.customValidity = value;
        if (!entry.validity) entry.validity = {{}};
        entry.validity.customError = value.length > 0;
        entry.validity.valid = validityFlags(entry).valid;
        pushCommand({{ kind: "setCustomValidity", node_index: entry.nodeIndex, message: value }});
      }},
      setAttribute(name, value) {{
        const key = String(name).toLowerCase();
        const stringValue = String(value);
        entry.attributes[key] = stringValue;
        if (key === "id") this.id = stringValue;
        if (key === "class") this.className = stringValue;
        if (key === "disabled") this.disabled = true;
        if (key === "hidden") this.hidden = true;
        pushCommand({{ kind: "setAttribute", node_index: entry.nodeIndex, name: key, value: stringValue }});
      }},
      removeAttribute(name) {{
        const key = String(name).toLowerCase();
        delete entry.attributes[key];
        if (key === "id") this.id = "";
        if (key === "class") this.className = "";
        if (key === "disabled") this.disabled = false;
        if (key === "hidden") this.hidden = false;
        pushCommand({{ kind: "removeAttribute", node_index: entry.nodeIndex, name: key }});
      }}
    }};
    let value = element.value;
    Object.defineProperty(element, "value", {{
      enumerable: true,
      configurable: false,
      get() {{ return value; }},
      set(next) {{
        value = String(next);
        pushCommand({{ kind: "setValue", node_index: entry.nodeIndex, value }});
      }}
    }});
    let checked = element.checked;
    Object.defineProperty(element, "checked", {{
      enumerable: true,
      configurable: false,
      get() {{ return checked; }},
      set(next) {{
        checked = Boolean(next);
        pushCommand({{ kind: "setChecked", node_index: entry.nodeIndex, checked }});
      }}
    }});
    let selected = element.selected;
    Object.defineProperty(element, "selected", {{
      enumerable: true,
      configurable: false,
      get() {{ return selected; }},
      set(next) {{
        selected = Boolean(next);
        pushCommand({{ kind: "setSelected", node_index: entry.nodeIndex, selected }});
      }}
    }});
    Object.defineProperty(element, "__glassRefresh", {{
      enumerable: false,
      configurable: false,
      value(nextEntry) {{
        entry = nextEntry;
        element.nodeIndex = nextEntry.nodeIndex;
        element.parentIndex = nextEntry.parentIndex;
        element.tagName = nextEntry.tagName.toUpperCase();
        element.id = nextEntry.attributes.id || "";
        element.className = nextEntry.attributes.class || "";
        element.textContent = nextEntry.text;
        element.innerText = nextEntry.text;
        element.disabled = nextEntry.disabled;
        element.hidden = nextEntry.hidden;
        element.focused = nextEntry.focused;
        element.multiple = Object.prototype.hasOwnProperty.call(nextEntry.attributes, "multiple");
        value = nextEntry.value === null
          ? (nextEntry.tagName.toLowerCase() === "option"
            ? (nextEntry.attributes.value === undefined ? nextEntry.text : nextEntry.attributes.value)
            : "")
          : nextEntry.value;
        checked = nextEntry.checked;
        selected = nextEntry.selected;
      }}
    }});
    return element;
  }};
  const previousElements = globalThis.__glassHostElements instanceof Map
    ? globalThis.__glassHostElements
    : new Map();
  const elements = state.elements.map((entry) => {{
    const existing = previousElements.get(entry.nodeIndex);
    if (existing && typeof existing.__glassRefresh === "function") {{
      existing.__glassRefresh(entry);
      return existing;
    }}
    return makeElement(entry);
  }});
  const elementsByIndex = new Map(elements.map((element) => [element.nodeIndex, element]));
  globalThis.__glassHostElements = elementsByIndex;
  for (const element of elements) {{
    if (!Object.prototype.hasOwnProperty.call(element, "parentElement")) {{
      Object.defineProperty(element, "parentElement", {{
        enumerable: false,
        configurable: false,
        get() {{
          if (element.parentIndex === null) return null;
          const current = globalThis.__glassHostElements;
          return current instanceof Map ? current.get(element.parentIndex) || null : null;
        }},
      }});
      Object.defineProperty(element, "parentNode", {{
        enumerable: false,
        configurable: false,
        get() {{ return element.parentElement; }},
      }});
    }}
    if (element.tagName === "SELECT" && !Object.prototype.hasOwnProperty.call(element, "options")) {{
      Object.defineProperty(element, "options", {{
        enumerable: false,
        configurable: false,
        get() {{
          const current = globalThis.__glassHostElements;
          if (!(current instanceof Map)) return [];
          return Array.from(current.values()).filter(option =>
            option.tagName === "OPTION" && option.parentIndex === element.nodeIndex
          );
        }},
      }});
      Object.defineProperty(element, "selectedOptions", {{
        enumerable: false,
        configurable: false,
        get() {{ return element.options.filter(option => option.selected); }},
      }});
    }}
  }}
  const setLocalFocus = (target) => {{
    const current = elements.find((element) => element.focused && element !== target) || null;
    if (current) {{
      current.focused = false;
      dispatchTarget(current, createEvent("blur"));
    }}
    if (target.focused) return;
    target.focused = true;
    pushCommand({{ kind: "focus", node_index: target.nodeIndex }});
    dispatchTarget(target, createEvent("focus"));
  }};
  const matches = (element, selector) => {{
    const value = String(selector).trim();
    if (value.startsWith("#")) return element.id === value.slice(1);
    if (value.startsWith(".")) return element.className.split(/\s+/).includes(value.slice(1));
    return element.tagName.toLowerCase() === value.toLowerCase();
  }};
  const findAll = (selector) => elements.filter((element) => matches(element, selector));
  const body = elements.find((element) => element.tagName === "BODY") || null;
  const documentElement = elements.find((element) => element.tagName === "HTML") || null;
  const document = {{
    title: state.title,
    body,
    documentElement,
    get activeElement() {{ return elements.find((element) => element.focused) || null; }},
    readyState: {ready_state},
    addEventListener(type, callback, options) {{
      addListener("document", type, callback, options);
    }},
    removeEventListener(type, callback, options) {{
      removeListener("document", type, callback, options);
    }},
    dispatchEvent(event) {{
      return dispatchTarget(this, event);
    }},
    getElementById(id) {{ return elements.find((element) => element.id === String(id)) || null; }},
    querySelector(selector) {{ return findAll(selector)[0] || null; }},
    querySelectorAll(selector) {{ return findAll(selector); }},
    getElementsByTagName(name) {{
      const value = String(name).toLowerCase();
      return elements.filter((element) => value === "*" || element.tagName.toLowerCase() === value);
    }},
    getElementsByClassName(name) {{
      const value = String(name);
      return elements.filter((element) => element.className.split(/\s+/).includes(value));
    }}
  }};
  globalThis.__glassDispatchHostEvents = (events) => events.map((descriptor) => {{
    const target = descriptor.node_index === 0
      ? document
      : descriptor.node_index === 4294967295
        ? globalThis
        : elements.find((element) => element.nodeIndex === descriptor.node_index) || null;
    if (!target) throw new TypeError("native event target is detached");
    const event = createEvent(descriptor.type, {{
      bubbles: Boolean(descriptor.bubbles),
      cancelable: Boolean(descriptor.cancelable),
      key: descriptor.key,
      code: descriptor.code,
      submitter: descriptor.submitter_node_index == null
        ? null
        : elements.find((element) => element.nodeIndex === descriptor.submitter_node_index) || null,
      oldURL: descriptor.old_url,
      newURL: descriptor.new_url,
    }});
    return dispatchTarget(target, event);
  }});
  globalThis.window = globalThis;
  globalThis.__glassHostCommands = commands;
  globalThis.__glassHostCommandBuffer = commands;
  globalThis.document = document;
  globalThis.location = Object.freeze({{ href: host.url, origin: host.origin }});
  globalThis.innerWidth = {width};
  globalThis.innerHeight = {height};
  globalThis.navigator = globalThis.navigator || Object.freeze({{ userAgent: "GlassNative" }});
  globalThis.Event = globalThis.Event || function Event(type, options) {{
    return createEvent(type, options);
  }};
  globalThis.CustomEvent = globalThis.CustomEvent || function CustomEvent(type, options) {{
    const event = createEvent(type, options);
    event.detail = options && typeof options === "object" ? options.detail : undefined;
    return event;
  }};
  globalThis.addEventListener = (type, callback, options) => addListener("window", type, callback, options);
  globalThis.removeEventListener = (type, callback, options) => removeListener("window", type, callback, options);
  globalThis.dispatchEvent = (event) => dispatchTarget(globalThis, event);
  globalThis.console = globalThis.console || {{
    log() {{}}, info() {{}}, warn() {{}}, error() {{}}
  }};
  globalThis.__glassRunTimers(host.now_ms);
}})();"###,
        serialized = serialized,
        max_commands = super::interaction::MAX_NATIVE_EFFECTS,
        max_listeners = super::interaction::MAX_NATIVE_EFFECTS,
        max_timers = super::interaction::MAX_NATIVE_EFFECTS,
        storage_entry_limit = crate::browser_backend::MAX_STORAGE_ENTRIES,
        storage_key_limit = crate::browser_backend::MAX_BACKEND_ID_BYTES,
        storage_value_limit = crate::browser_backend::MAX_TEXT_BYTES,
        width = viewport.width,
        height = viewport.height,
        ready_state = ready_state,
    ))
}
