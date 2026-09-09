//! Bounded JavaScript execution for the native browser realm.
//!
//! QuickJS supplies the ECMAScript implementation. Glass owns the host
//! objects and Web APIs, which are added in separate slices so every exposed
//! capability has an explicit resource and security contract.

use super::error::NativeEngineError;
use rquickjs::{Context, Runtime, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Maximum source accepted by the native script evaluator.
pub(crate) const MAX_NATIVE_SCRIPT_BYTES: usize = crate::browser_backend::MAX_TEXT_BYTES;
/// Maximum JSON representation returned to the semantic backend.
pub(crate) const MAX_NATIVE_SCRIPT_RESULT_BYTES: usize = crate::browser_backend::MAX_JSON_BYTES;
const NATIVE_SCRIPT_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const NATIVE_SCRIPT_STACK_BYTES: usize = 1024 * 1024;
const NATIVE_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);

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

    pub(crate) fn evaluate(&self, source: &str) -> Result<serde_json::Value, NativeEngineError> {
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
        let deadline = Instant::now() + NATIVE_SCRIPT_TIMEOUT;
        if let Ok(mut current) = self.deadline.lock() {
            *current = Some(deadline);
        }
        let result = self.context.with(|ctx| {
            let value: Value = ctx.eval(source).map_err(|_| NativeEngineError::Worker {
                operation: "evaluate JavaScript".into(),
                reason: "JavaScript evaluation failed".into(),
            })?;
            let json = ctx
                .json_stringify(value)
                .map_err(|_| NativeEngineError::Worker {
                    operation: "serialize JavaScript result".into(),
                    reason: "JavaScript result could not be serialized".into(),
                })?;
            let Some(json) = json else {
                return Ok(serde_json::Value::Null);
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
            serde_json::from_str(&json).map_err(|_| NativeEngineError::Worker {
                operation: "decode JavaScript result".into(),
                reason: "JavaScript result was not valid JSON".into(),
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
