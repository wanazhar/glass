use super::{DevelopmentError, DevelopmentResult, ProcessManager, ProcessSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Stdio},
};
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
    let rpc = prove_embedded_rpc().is_ok();
    Ok(NeovimCapability {
        executable: "nvim".into(),
        version,
        pty_mode: true,
        rpc_prototype: rpc,
        architecture_decision: "Ship PTY compatibility as Mode A. Use Neovim's embedded Msgpack-RPC as an optional editing engine behind Glass-owned buffers and events; never make it the surrounding UI or runtime authority.".into(),
    })
}

struct EmbeddedNeovim {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: i64,
}

impl EmbeddedNeovim {
    fn spawn() -> DevelopmentResult<Self> {
        let mut child = Command::new("nvim")
            .args(["--embed", "--headless", "--clean"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| DevelopmentError::Process(format!("Neovim embed failed: {error}")))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| DevelopmentError::Process("Neovim RPC stdin unavailable".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| DevelopmentError::Process("Neovim RPC stdout unavailable".into()))?;
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            next_id: 1,
        })
    }

    fn call(&mut self, method: &str, params: Vec<rmpv::Value>) -> DevelopmentResult<rmpv::Value> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = rmpv::Value::Array(vec![
            0.into(),
            id.into(),
            method.into(),
            rmpv::Value::Array(params),
        ]);
        rmpv::encode::write_value(&mut self.input, &request)
            .map_err(|error| DevelopmentError::Serialization(error.to_string()))?;
        self.input.flush()?;
        loop {
            let response = rmpv::decode::read_value(&mut self.output)
                .map_err(|error| DevelopmentError::Serialization(error.to_string()))?;
            let Some(values) = response.as_array() else {
                continue;
            };
            if values.len() != 4 || values[0].as_i64() != Some(1) || values[1].as_i64() != Some(id)
            {
                continue;
            }
            if !values[2].is_nil() {
                return Err(DevelopmentError::Process(format!(
                    "Neovim RPC {method} failed: {}",
                    values[2]
                )));
            }
            return Ok(values[3].clone());
        }
    }
}

impl Drop for EmbeddedNeovim {
    fn drop(&mut self) {
        let _ = self.call("nvim_command", vec!["qa!".into()]);
        let _ = self.child.wait();
    }
}

fn prove_embedded_rpc() -> DevelopmentResult<()> {
    let mut nvim = EmbeddedNeovim::spawn()?;
    let buffer = nvim.call("nvim_create_buf", vec![false.into(), true.into()])?;
    nvim.call(
        "nvim_buf_set_lines",
        vec![
            buffer.clone(),
            0.into(),
            (-1).into(),
            true.into(),
            rmpv::Value::Array(vec!["glass-rpc-proof".into()]),
        ],
    )?;
    let lines = nvim.call(
        "nvim_buf_get_lines",
        vec![buffer, 0.into(), (-1).into(), true.into()],
    )?;
    if lines
        .as_array()
        .and_then(|lines| lines.first())
        .and_then(rmpv::Value::as_str)
        != Some("glass-rpc-proof")
    {
        return Err(DevelopmentError::Process(
            "Neovim RPC buffer round trip did not preserve text".into(),
        ));
    }
    Ok(())
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
