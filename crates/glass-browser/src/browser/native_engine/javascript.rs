//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

use super::config::Viewport;
use super::dom::NativeDocument;
use super::error::NativeEngineError;
use super::origin::NativeOrigin;
use rquickjs::{Context, Runtime, Value};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Maximum source accepted by the native script evaluator.
pub(crate) const MAX_NATIVE_SCRIPT_BYTES: usize = crate::browser_backend::MAX_TEXT_BYTES;
/// Maximum JSON representation returned to the semantic backend.
pub(crate) const MAX_NATIVE_SCRIPT_RESULT_BYTES: usize = crate::browser_backend::MAX_JSON_BYTES;
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

/// One persistent ECMAScript realm. A full navigation creates a new value;
/// same-document navigation retains it, matching a page global object's
/// lifetime.
pub(crate) struct NativeJavaScriptRuntime {
    runtime: Runtime,
    context: Context,
    deadline: Arc<Mutex<Option<Instant>>>,
}

impl NativeJavaScriptRuntime {
    pub(crate) fn new() -> Result<Self, NativeEngineError> {
        let runtime = Runtime::new().map_err(|_| NativeEngineError::Worker {
            operation: "create JavaScript runtime".into(),
            reason: "native JavaScript runtime could not be created".into(),
        })?;
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
        })
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
  const pushCommand = (command) => {{
    if (commands.length >= {max_commands}) throw new RangeError("native host command limit exceeded");
    commands.push(command);
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
  const addListener = (owner, type, callback) => {{
    if (typeof callback !== "function") throw new TypeError("event listener must be callable");
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key) || [];
    if (callbacks.some((current) => current === callback)) return;
    if (listenerCount() >= {max_listeners}) throw new RangeError("native event listener limit exceeded");
    callbacks.push(callback);
    listeners.set(key, callbacks);
  }};
  const removeListener = (owner, type, callback) => {{
    const key = listenerKey(owner, type);
    const callbacks = listeners.get(key);
    if (!callbacks) return;
    const index = callbacks.findIndex((current) => current === callback);
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
      stopPropagation() {{}},
      stopImmediatePropagation() {{}},
    }};
    return event;
  }};
  const dispatchOwner = (owner, target, event) => {{
    if (!event || typeof event.type !== "string") throw new TypeError("invalid native event");
    const eventType = normalizeEventType(event.type);
    if (event.target !== null && event.target !== undefined && event.target !== target) {{
      throw new TypeError("event target is already assigned");
    }}
    event.target = target;
    event.currentTarget = target;
    event.eventPhase = 2;
    const callbacks = (listeners.get(listenerKey(owner, eventType)) || []).slice();
    for (const callback of callbacks) callback.call(target, event);
    event.currentTarget = null;
    event.eventPhase = 0;
    return !event.defaultPrevented;
  }};
  const makeElement = (entry) => {{
    const element = {{
      nodeIndex: entry.nodeIndex,
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
      addEventListener(type, callback) {{
        addListener("node:" + entry.nodeIndex, type, callback);
      }},
      removeEventListener(type, callback) {{
        removeListener("node:" + entry.nodeIndex, type, callback);
      }},
      dispatchEvent(event) {{
        return dispatchOwner("node:" + entry.nodeIndex, this, event);
      }},
      focus() {{
        if (this.disabled || this.hidden) return;
        setLocalFocus(this);
      }},
      blur() {{
        if (!this.focused) return;
        this.focused = false;
        pushCommand({{ kind: "blur", node_index: entry.nodeIndex }});
        dispatchOwner("node:" + entry.nodeIndex, this, createEvent("blur"));
      }},
      click() {{
        if (this.disabled || this.hidden) return;
        this.focus();
        const event = createEvent("click", {{ bubbles: true, cancelable: true }});
        if (!dispatchOwner("node:" + entry.nodeIndex, this, event)) return;
        if (this.tagName === "INPUT") {{
          const type = String(entry.attributes.type || "text").toLowerCase();
          if (type === "checkbox") checked = !checked;
          if (type === "radio") checked = true;
        }}
        pushCommand({{ kind: "click", node_index: entry.nodeIndex }});
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
    return element;
  }};
  const elements = state.elements.map(makeElement);
  const setLocalFocus = (target) => {{
    const current = elements.find((element) => element.focused && element !== target) || null;
    if (current) {{
      current.focused = false;
      dispatchOwner("node:" + current.nodeIndex, current, createEvent("blur"));
    }}
    if (target.focused) return;
    target.focused = true;
    pushCommand({{ kind: "focus", node_index: target.nodeIndex }});
    dispatchOwner("node:" + target.nodeIndex, target, createEvent("focus"));
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
    addEventListener(type, callback) {{
      addListener("document", type, callback);
    }},
    removeEventListener(type, callback) {{
      removeListener("document", type, callback);
    }},
    dispatchEvent(event) {{
      return dispatchOwner("document", this, event);
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
  globalThis.window = globalThis;
  globalThis.__glassHostCommands = commands;
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
  globalThis.addEventListener = (type, callback) => addListener("window", type, callback);
  globalThis.removeEventListener = (type, callback) => removeListener("window", type, callback);
  globalThis.dispatchEvent = (event) => dispatchOwner("window", globalThis, event);
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
