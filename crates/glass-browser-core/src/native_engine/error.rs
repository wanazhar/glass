use super::lifecycle::NativeLifecycleState;
use std::fmt;

/// Failure classes reported by the out-of-process native content worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeWorkerFailureKind {
    Spawn,
    Exited,
    Transport,
    Timeout,
    Protocol,
    Rejected,
    InvalidTransfer,
    SandboxUnavailable,
}

/// Typed failures raised by the native engine kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeEngineError {
    InvalidConfiguration {
        field: String,
        reason: String,
    },
    Lifecycle {
        operation: String,
        state: NativeLifecycleState,
        reason: String,
    },
    UnsupportedUrl {
        reason: String,
    },
    Network {
        operation: String,
        reason: String,
    },
    Worker {
        operation: String,
        reason: String,
    },
    WorkerFailure {
        operation: String,
        kind: NativeWorkerFailureKind,
        reason: String,
    },
    NavigationCancelled,
    StorageProfileLocked {
        path: String,
    },
    Parse {
        offset: usize,
        reason: String,
    },
    LimitExceeded {
        resource: String,
        limit: usize,
        actual: usize,
    },
    Scheduler {
        reason: String,
    },
    TargetNotFound,
    AmbiguousTarget {
        matches: usize,
    },
    DetachedTarget,
    TargetNotActionable {
        reason: String,
    },
    DisabledTarget,
    ReadOnlyTarget,
}

impl NativeEngineError {
    /// Return a privacy-safe static classification for opt-in worker diagnostics.
    pub fn diagnostic_class(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration { .. } => "invalid_configuration",
            Self::Lifecycle { .. } => "lifecycle",
            Self::UnsupportedUrl { .. } => "unsupported_url",
            Self::Network { .. } => "network",
            Self::Worker { .. } => "worker",
            Self::WorkerFailure { .. } => "worker_failure",
            Self::NavigationCancelled => "navigation_cancelled",
            Self::StorageProfileLocked { .. } => "storage_profile_locked",
            Self::Parse { .. } => "parse",
            Self::LimitExceeded { .. } => "limit_exceeded",
            Self::Scheduler { .. } => "scheduler",
            Self::TargetNotFound => "target_not_found",
            Self::AmbiguousTarget { .. } => "ambiguous_target",
            Self::DetachedTarget => "detached_target",
            Self::TargetNotActionable { .. } => "target_not_actionable",
            Self::DisabledTarget => "disabled_target",
            Self::ReadOnlyTarget => "read_only_target",
        }
    }

    /// Return a coarse operation family without exposing the operation or its
    /// error details, which may include page or profile data.
    pub fn diagnostic_operation_class(&self) -> Option<&'static str> {
        let operation = match self {
            Self::Worker { operation, .. } | Self::WorkerFailure { operation, .. } => operation,
            _ => return None,
        };
        if operation.contains("content process") {
            return Some(if operation.contains("script") {
                "content_process_script"
            } else {
                "content_process"
            });
        }
        let prefix = operation
            .split_ascii_whitespace()
            .next()
            .unwrap_or_default();
        Some(match prefix {
            "dispatch" | "stage" | "evaluate" | "dynamic" | "script" | "javascript"
            | "page-load" => "script_runtime",
            "parse" | "serialize" | "encode" | "decode" => "serialization",
            "read" | "write" | "download" | "file" => "filesystem",
            "fetch" | "network" | "request" | "route" | "origin" | "HTTP" | "WebSocket" => {
                "network"
            }
            "compile" | "configure" | "create" | "initialize" | "install" | "prepare" | "spawn"
            | "activate" | "register" => "runtime_setup",
            _ => "other",
        })
    }

    /// Return a safe class for known worker failures without exposing their
    /// error text, which may contain page, URL, or profile data.
    pub fn diagnostic_reason_class(&self) -> Option<&'static str> {
        let Self::Worker { reason, .. } = self else {
            return None;
        };
        if reason.contains("content process exited or closed its pipe")
            || reason.contains("content process pipe is unavailable")
        {
            return Some("content_process_pipe_closed");
        }
        if reason.contains("content process returned a truncated frame") {
            return Some("content_process_truncated_frame");
        }
        let lower = reason.to_ascii_lowercase();
        if lower.contains("maximum call stack size exceeded") || lower.contains("stack overflow") {
            return Some("javascript_stack_overflow");
        }
        if lower.contains("out of memory") || lower.contains("quickjs_out_of_memory") {
            return Some("javascript_out_of_memory");
        }
        if lower.contains("exception generated by quickjs")
            || reason.starts_with("JavaScript evaluation failed:")
        {
            return Some("javascript_exception");
        }
        Some(match reason.as_str() {
            "content process has no committed document" => "missing_document",
            "content process has no committed URL" => "missing_url",
            "content process has no committed origin" => "missing_origin",
            _ => "other",
        })
    }

    /// Return bounded resource-limit diagnostics without exposing page data.
    pub fn diagnostic_limit(&self) -> Option<(String, usize, usize)> {
        let Self::LimitExceeded {
            resource,
            limit,
            actual,
        } = self
        else {
            return None;
        };

        // Limit names are supplied by native implementation call sites. Keep
        // opt-in diagnostics safe if a future caller accidentally passes
        // externally sourced text.
        let safe_resource = resource
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric()
                    || *character == ' '
                    || *character == '_'
                    || *character == '-'
            })
            .take(80)
            .collect();
        Some((safe_resource, *limit, *actual))
    }

    pub fn invalid(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidConfiguration {
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub fn limit(resource: impl Into<String>, limit: usize, actual: usize) -> Self {
        Self::LimitExceeded {
            resource: resource.into(),
            limit,
            actual,
        }
    }

    pub(crate) fn worker_failure(
        operation: impl Into<String>,
        kind: NativeWorkerFailureKind,
        reason: impl Into<String>,
    ) -> Self {
        Self::WorkerFailure {
            operation: operation.into(),
            kind,
            reason: reason.into(),
        }
    }
}

impl fmt::Display for NativeEngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration { field, reason } => {
                write!(formatter, "invalid {field}: {reason}")
            }
            Self::Lifecycle {
                operation,
                state,
                reason,
            } => write!(
                formatter,
                "lifecycle failure during {operation} ({state}): {reason}"
            ),
            Self::UnsupportedUrl { reason } => write!(formatter, "unsupported URL: {reason}"),
            Self::Network { operation, reason } => {
                write!(formatter, "network failure during {operation}: {reason}")
            }
            Self::Worker { operation, reason } => {
                write!(
                    formatter,
                    "runtime worker failure during {operation}: {reason}"
                )
            }
            Self::WorkerFailure {
                operation,
                kind,
                reason,
            } => write!(
                formatter,
                "native content worker {kind:?} failure during {operation}: {reason}"
            ),
            Self::NavigationCancelled => {
                formatter.write_str("native navigation cancelled by stop-loading")
            }
            Self::StorageProfileLocked { path } => {
                write!(
                    formatter,
                    "native Web Storage profile is already owned: {path}"
                )
            }
            Self::Parse { offset, reason } => {
                write!(formatter, "HTML parse failure at byte {offset}: {reason}")
            }
            Self::LimitExceeded {
                resource,
                limit,
                actual,
            } => write!(
                formatter,
                "{resource} exceeds limit {limit} (actual {actual})"
            ),
            Self::Scheduler { reason } => write!(formatter, "scheduler failure: {reason}"),
            Self::TargetNotFound => write!(formatter, "native action target was not found"),
            Self::AmbiguousTarget { matches } => write!(
                formatter,
                "native action target matched {matches} elements; exactly one is required"
            ),
            Self::DetachedTarget => write!(formatter, "native action target is stale or detached"),
            Self::TargetNotActionable { reason } => {
                write!(
                    formatter,
                    "native action target is not actionable: {reason}"
                )
            }
            Self::DisabledTarget => write!(formatter, "native action target is disabled"),
            Self::ReadOnlyTarget => write!(formatter, "native action target is read-only"),
        }
    }
}

impl std::error::Error for NativeEngineError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_class_is_static_and_excludes_error_details() {
        let error = NativeEngineError::Network {
            operation: "private page request".into(),
            reason: "secret response text".into(),
        };
        assert_eq!(error.diagnostic_class(), "network");
        assert!(!error.diagnostic_class().contains("private"));
        assert!(!error.diagnostic_class().contains("secret"));
        let limit = NativeEngineError::limit("native script bytes", 10, 11);
        assert_eq!(
            limit.diagnostic_limit(),
            Some(("native script bytes".to_owned(), 10, 11))
        );
        assert_eq!(
            NativeEngineError::limit("private page text", 10, 11).diagnostic_limit(),
            Some(("private page text".to_owned(), 10, 11))
        );
    }

    #[test]
    fn worker_operation_diagnostic_is_coarse_and_excludes_dynamic_details() {
        let error = NativeEngineError::Worker {
            operation: "dispatch private-event-name".into(),
            reason: "private page payload".into(),
        };
        assert_eq!(error.diagnostic_operation_class(), Some("script_runtime"));
        assert_eq!(error.diagnostic_class(), "worker");
        assert!(
            !format!("{:?}", error.diagnostic_operation_class()).contains("private-event-name")
        );
        assert_eq!(
            NativeEngineError::Network {
                operation: "request".into(),
                reason: "secret".into(),
            }
            .diagnostic_operation_class(),
            None
        );
        assert_eq!(
            NativeEngineError::Worker {
                operation: "page-load private-resource-key".into(),
                reason: "private payload".into(),
            }
            .diagnostic_operation_class(),
            Some("script_runtime")
        );
        let content_script_failure = NativeEngineError::Worker {
            operation: "content process script private-event".into(),
            reason: "private page payload".into(),
        };
        assert_eq!(
            content_script_failure.diagnostic_operation_class(),
            Some("content_process_script")
        );
        assert!(
            !format!("{:?}", content_script_failure.diagnostic_operation_class())
                .contains("private-event")
        );
        assert_eq!(
            NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: "content process has no committed URL".into(),
            }
            .diagnostic_reason_class(),
            Some("missing_url")
        );
        assert_eq!(
            NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: "private URL and query".into(),
            }
            .diagnostic_reason_class(),
            Some("other")
        );
        assert_eq!(
            NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: "connection failed: content process exited or closed its pipe; private URL"
                    .into(),
            }
            .diagnostic_reason_class(),
            Some("content_process_pipe_closed")
        );
        assert_eq!(
            NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: "JavaScript evaluation failed: Error: Maximum call stack size exceeded at https://private.example/user-data".into(),
            }
            .diagnostic_reason_class(),
            Some("javascript_stack_overflow")
        );
        assert_eq!(
            NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: "Exception generated by quickjs: TypeError: secret page text".into(),
            }
            .diagnostic_reason_class(),
            Some("javascript_exception")
        );
    }
}
