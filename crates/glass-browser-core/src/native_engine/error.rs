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
}
