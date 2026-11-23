//! Identity of the host binary, fixed when it is built.

use serde_json::{Value, json};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_COMMIT: &str = env!("MARBLE_GIT_COMMIT");
pub const GIT_COMMIT_SHORT: &str = env!("MARBLE_GIT_COMMIT_SHORT");
pub const CLI_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git ",
    env!("MARBLE_GIT_COMMIT_SHORT"),
    "; commit ",
    env!("MARBLE_GIT_COMMIT"),
    ")"
);

pub fn document() -> Value {
    json!({"version": VERSION, "gitCommit": GIT_COMMIT, "gitCommitShort": GIT_COMMIT_SHORT})
}
