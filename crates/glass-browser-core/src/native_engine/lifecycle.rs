use std::fmt;

/// Lifecycle state for one native engine instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeLifecycleState {
    New,
    Running,
    Closed,
}

impl NativeLifecycleState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Running => "running",
            Self::Closed => "closed",
        }
    }
}

impl fmt::Display for NativeLifecycleState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
