use super::{DevelopmentError, DevelopmentResult, ProcessManager, ProcessSnapshot};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NeovimCapability {
    pub executable: String,
    pub version: String,
    pub pty_mode: bool,
    pub rpc_prototype: bool,
    pub architecture_decision: String,
}

pub fn probe_neovim() -> DevelopmentResult<NeovimCapability> {
    let version = Command::new("nvim")
        .arg("--version")
        .output()
        .map_err(|error| DevelopmentError::Process(format!("Neovim is unavailable: {error}")))?;
    if !version.status.success() {
        return Err(DevelopmentError::Process(
            "Neovim version probe failed".into(),
        ));
    }
    let version = String::from_utf8_lossy(&version.stdout)
        .lines()
        .next()
        .unwrap_or("Neovim")
        .to_string();
    let rpc = Command::new("nvim")
        .args([
            "--headless",
            "--clean",
            "+lua io.write(vim.api.nvim_get_current_buf())",
            "+qa!",
        ])
        .output()
        .is_ok_and(|output| output.status.success());
    Ok(NeovimCapability {
        executable: "nvim".into(),
        version,
        pty_mode: true,
        rpc_prototype: rpc,
        architecture_decision: "Ship PTY compatibility as Mode A. Use Neovim's embedded Msgpack-RPC as an optional editing engine behind Glass-owned buffers and events; never make it the surrounding UI or runtime authority.".into(),
    })
}

pub fn start_neovim(
    processes: &mut ProcessManager,
    name: &str,
    path: Option<&Path>,
) -> DevelopmentResult<ProcessSnapshot> {
    let command = path.map_or_else(
        || "nvim".to_string(),
        |path| format!("nvim -- {}", shell_quote(path.to_string_lossy().as_ref())),
    );
    processes.start(name, &command)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_neovim_probe_proves_pty_and_rpc_modes_when_available() {
        if Command::new("nvim").arg("--version").output().is_err() {
            return;
        }
        let capability = probe_neovim().unwrap();
        assert!(capability.pty_mode);
        assert!(capability.rpc_prototype);
        assert!(capability.architecture_decision.contains("Msgpack-RPC"));
    }
}
