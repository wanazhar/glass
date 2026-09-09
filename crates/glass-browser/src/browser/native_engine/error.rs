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
    pub(crate) fn invalid(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::InvalidConfiguration {
            field: field.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn limit(resource: impl Into<String>, limit: usize, actual: usize) -> Self {
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
