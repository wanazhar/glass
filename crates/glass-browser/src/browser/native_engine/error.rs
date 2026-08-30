use super::lifecycle::NativeLifecycleState;
use std::fmt;

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
