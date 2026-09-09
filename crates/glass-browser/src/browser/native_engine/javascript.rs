//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

use super::config::Viewport;
use super::dom::{NativeDocument, NativePageScriptSource};
use super::error::NativeEngineError;
use super::interaction::NativeEventKind;
use super::origin::NativeOrigin;
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Context, Error, Module, Runtime, Value};
use serde::Deserialize;
use std::collections::BTreeMap;
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
}

pub(crate) struct NativeScriptEvaluation {
    pub(crate) value: serde_json::Value,
    pub(crate) commands: Vec<NativeScriptCommand>,
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
) -> Result<(), NativeEngineError> {
    let sources = document
        .page_script_sources(MAX_NATIVE_INLINE_SCRIPTS, MAX_NATIVE_SCRIPT_BYTES)
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| match source {
            NativePageScriptSource::Inline(source) => Some(NativePageScript::Classic { source }),
            NativePageScriptSource::ModuleInline(source) => Some(NativePageScript::Module {
                name: format!("{document_url}#glass-inline-module-{index}"),
                source,
            }),
            NativePageScriptSource::External(_) | NativePageScriptSource::ModuleExternal(_) => None,
        })
        .collect::<Vec<_>>();
    execute_page_scripts(
        document,
        runtime,
        &sources,
        document_url,
        document_origin,
        viewport,
    )
}

pub(crate) fn execute_page_scripts(
    document: &mut NativeDocument,
    runtime: &mut Option<NativeJavaScriptRuntime>,
    sources: &[NativePageScript],
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<(), NativeEngineError> {
    if sources.is_empty() {
        return Ok(());
    }
    if runtime.is_none() {
        *runtime = Some(NativeJavaScriptRuntime::new()?);
    }
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
        if evaluation.commands.is_empty() {
            continue;
        }
        let mut next = document.clone();
        next.apply_script_commands(&evaluation.commands)?;
        *document = next;
    }
    Ok(())
}

/// Build the internal source used to deliver Rust-owned semantic events into
/// the persistent page realm. The source is generated from typed, bounded
/// event metadata and never contains page-provided strings.
pub(crate) fn host_event_script(
    events: &[(u32, NativeEventKind)],
) -> Result<Option<String>, NativeEngineError> {
    if events.is_empty() {
        return Ok(None);
    }
    let descriptors = events
        .iter()
        .map(|(node_index, kind)| {
            let (event_type, bubbles, cancelable) = match kind {
                NativeEventKind::Blur => ("blur", false, false),
                NativeEventKind::Focus => ("focus", false, false),
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

/// One persistent ECMAScript realm. A full navigation creates a new value;
/// same-document navigation retains it, matching a page global object's
/// lifetime.
pub(crate) struct NativeJavaScriptRuntime {
    runtime: Runtime,
    context: Context,
    deadline: Arc<Mutex<Option<Instant>>>,
    module_sources: Arc<Mutex<BTreeMap<String, String>>>,
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
        })
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
        let bootstrap = document_bootstrap(document, document_url, origin, viewport)?;
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
            let commands = read_script_commands(ctx.clone())?;
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
        let bootstrap = document_bootstrap(document, document_url, origin, viewport)?;
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
            Ok(NativeScriptEvaluation {
                value: serde_json::Value::Null,
                commands,
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

fn document_bootstrap(
    document: &NativeDocument,
    document_url: &str,
    origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<String, NativeEngineError> {
    let state = document.script_snapshot(crate::browser_backend::MAX_TEXT_BYTES);
    let serialized = serde_json::to_string(&serde_json::json!({
        "url": document_url,
        "origin": origin.serialized(),
        "state": state,
    }))
    .map_err(|_| NativeEngineError::Worker {
        operation: "serialize JavaScript host view".into(),
        reason: "native JavaScript host view could not be serialized".into(),
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
      target: null,
      currentTarget: null,
      eventPhase: 0,
      defaultPrevented: false,
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
    event.currentTarget = null;
    event.eventPhase = 0;
    eventState.dispatching = false;
    return !event.defaultPrevented;
  }};
  const makeElement = (entry) => {{
    const element = {{
      nodeIndex: entry.nodeIndex,
      parentIndex: entry.parentIndex,
      tagName: entry.tagName.toUpperCase(),
      id: entry.attributes.id || "",
      className: entry.attributes.class || "",
      textContent: entry.text,
      innerText: entry.text,
      value: entry.value === null ? "" : entry.value,
      checked: entry.checked,
      selected: entry.selected,
      disabled: entry.disabled,
      hidden: entry.hidden,
      focused: entry.focused,
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
      requestSubmit() {{ this.submit(); }},
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
    return element;
  }};
  const elements = state.elements.map(makeElement);
  const elementsByIndex = new Map(elements.map((element) => [element.nodeIndex, element]));
  for (const element of elements) {{
    Object.defineProperty(element, "parentElement", {{
      enumerable: false,
      configurable: false,
      get() {{ return element.parentIndex === null ? null : elementsByIndex.get(element.parentIndex) || null; }},
    }});
    Object.defineProperty(element, "parentNode", {{
      enumerable: false,
      configurable: false,
      get() {{ return element.parentElement; }},
    }});
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
    readyState: "complete",
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
      : elements.find((element) => element.nodeIndex === descriptor.node_index) || null;
    if (!target) throw new TypeError("native event target is detached");
    const event = createEvent(descriptor.type, {{
      bubbles: Boolean(descriptor.bubbles),
      cancelable: Boolean(descriptor.cancelable),
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
}})();"###,
        serialized = serialized,
        max_commands = super::interaction::MAX_NATIVE_EFFECTS,
        max_listeners = super::interaction::MAX_NATIVE_EFFECTS,
        width = viewport.width,
        height = viewport.height,
    ))
}
