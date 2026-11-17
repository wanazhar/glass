//! Durable conversation lifecycle over the Glass host runtime client.

use crate::{Approval, MarbleError, TURN_TIMEOUT, host::HostPolicy, state::PendingApproval};
use glass_runtime_client::{
    Incoming, RuntimeClient, SessionOptions, ToolDefinition, TurnInput, json,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const HOST_TOOL: &str = "glass.workspace.list";

/// Awaiting tool request plus the explicit host decision channel.
#[derive(Debug, Clone, Default)]
pub struct ApprovalInbox {
    pending: std::sync::Arc<std::sync::Mutex<BTreeMap<String, PendingApproval>>>,
    senders: std::sync::Arc<std::sync::Mutex<HashMap<String, mpsc::Sender<Approval>>>>,
}

impl ApprovalInbox {
    fn pending_map(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, PendingApproval>> {
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn senders_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, mpsc::Sender<Approval>>> {
        self.senders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Register one inbound tool request and park for the explicit decision.
    pub fn register(
        &self,
        request_id: &str,
        call_id: &str,
        tool: &str,
        arguments: Value,
    ) -> Receiver<Approval> {
        let (sender, receiver) = mpsc::channel();
        self.pending_map().insert(
            request_id.to_string(),
            PendingApproval {
                request_id: request_id.to_string(),
                call_id: call_id.to_string(),
                tool: tool.to_string(),
                arguments,
            },
        );
        self.senders_map().insert(request_id.to_string(), sender);
        receiver
    }

    /// Resolve one pending tool request with the host decision.
    pub fn resolve(&self, request_id: &str, approval: Approval) -> bool {
        let sender = self.senders_map().remove(request_id);
        if let Some(sender) = sender {
            let _ = sender.send(approval);
        }
        self.pending_map().remove(request_id).is_some()
    }

    /// Snapshot of tool requests still awaiting an explicit decision.
    pub fn pending(&self) -> Vec<PendingApproval> {
        self.pending_map().values().cloned().collect()
    }
}

/// Durable conversation handle over the local runtime.
pub struct Conversation {
    client: RuntimeClient,
    cwd: PathBuf,
    host: HostPolicy,
    approvals: ApprovalInbox,
}

impl Conversation {
    pub fn spawn(
        command: Command,
        cwd: &Path,
        storage_dir: &Path,
        approvals: ApprovalInbox,
    ) -> Result<Self, MarbleError> {
        Self::spawn_with_policy(command, cwd, storage_dir, HostPolicy::default(), approvals)
    }

    /// Spawn with an explicit host policy so deny paths stay testable.
    pub fn spawn_with_policy(
        mut command: Command,
        cwd: &Path,
        storage_dir: &Path,
        host: HostPolicy,
        approvals: ApprovalInbox,
    ) -> Result<Self, MarbleError> {
        command.current_dir(cwd);
        let mut client = RuntimeClient::spawn_command(command).map_err(MarbleError::Io)?;
        let init_id = client.initialize(storage_dir).map_err(MarbleError::Io)?;
        let __t0 = std::time::Instant::now();
        let init = Self::wait_response(&mut client, &host, &approvals, &init_id)?;
        eprintln!("PHASE init-done {:?}", __t0.elapsed());
        Self::require_durable_capabilities(&init)?;
        Ok(Self {
            client,
            cwd: cwd.to_path_buf(),
            host,
            approvals,
        })
    }

    /// Fail closed unless the runtime advertises the durable session contract.
    fn require_durable_capabilities(init: &Value) -> Result<(), MarbleError> {
        let capabilities = init.get("capabilities").ok_or_else(|| {
            MarbleError::Protocol("glass-core runtime returned no capability set".into())
        })?;
        for (capability, required) in [("durable", true), ("resume", true)] {
            if capabilities.get(capability).and_then(Value::as_bool) != Some(required) {
                return Err(MarbleError::Protocol(format!(
                    "glass-core runtime capability {capability} is unavailable"
                )));
            }
        }
        Ok(())
    }

    fn wait_response(
        client: &mut RuntimeClient,
        host: &HostPolicy,
        approvals: &ApprovalInbox,
        expected: &str,
    ) -> Result<Value, MarbleError> {
        loop {
            let incoming = client
                .recv_typed_timeout(TURN_TIMEOUT)
                .map_err(MarbleError::Io)?;
            let Some(incoming) = incoming else {
                return Err(MarbleError::Protocol(format!(
                    "timed out waiting for runtime response {expected}"
                )));
            };
            match incoming {
                Incoming::Response { id, result } => {
                    if id.as_str() != Some(expected) {
                        return Err(MarbleError::Protocol(format!(
                            "unexpected runtime response id {id:?}"
                        )));
                    }
                    return result.map_err(|error| {
                        MarbleError::Protocol(format!(
                            "runtime error {}: {}",
                            error.code, error.message
                        ))
                    });
                }
                Incoming::Event(_) => {}
                Incoming::Tool { request_id, call } => {
                    let key = request_id
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| request_id.to_string());
                    let decision = host.decision();
                    let approval = if decision == Approval::Approve {
                        Approval::Approve
                    } else {
                        approvals
                            .register(&key, &call.call_id, &call.name, call.arguments.clone())
                            .recv_timeout(TURN_TIMEOUT)
                            .unwrap_or(decision)
                    };
                    approvals.resolve(&key, approval);
                    crate::host::answer_tool_request(client, request_id, &call, approval)?;
                }
                Incoming::Unknown(_) => {}
            }
        }
    }

    /// Create a durable conversation and return its id.
    pub fn create(&mut self) -> Result<String, MarbleError> {
        let options = SessionOptions::host(self.cwd.clone(), vec![Self::host_tool()]);
        let create_id = self
            .client
            .durable_create(&options)
            .map_err(MarbleError::Io)?;
        let __t1 = std::time::Instant::now();
        let result =
            Self::wait_response(&mut self.client, &self.host, &self.approvals, &create_id)?;
        eprintln!("PHASE create-done {:?}", __t1.elapsed());
        Ok(result["conversationId"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Admit one durable request and return its task id.
    pub fn submit(
        &mut self,
        conversation_id: &str,
        request_id: &str,
        input: &TurnInput,
    ) -> Result<String, MarbleError> {
        let submit_id = self
            .client
            .durable_submit(conversation_id, request_id, input)
            .map_err(MarbleError::Io)?;
        let result =
            Self::wait_response(&mut self.client, &self.host, &self.approvals, &submit_id)?;
        Ok(result["submission"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string())
    }

    /// Start one durable task; returns the runtime response id to await.
    pub fn start(&mut self, task_id: &str) -> Result<String, MarbleError> {
        self.client
            .durable_run(Some(task_id))
            .map_err(MarbleError::Io)
    }

    pub fn recv_typed_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Incoming>, MarbleError> {
        self.client
            .recv_typed_timeout(timeout)
            .map_err(MarbleError::Io)
    }

    pub fn shutdown(&mut self) -> Result<(), MarbleError> {
        let shutdown_id = self
            .client
            .request("shutdown", json!({}))
            .map_err(MarbleError::Io)?;
        let _ = Self::wait_response(&mut self.client, &self.host, &self.approvals, &shutdown_id)?;
        Ok(())
    }

    pub fn host(&self) -> &HostPolicy {
        &self.host
    }

    pub fn client_mut(&mut self) -> &mut RuntimeClient {
        &mut self.client
    }

    pub fn approvals(&self) -> &ApprovalInbox {
        &self.approvals
    }

    fn host_tool() -> ToolDefinition {
        ToolDefinition {
            name: HOST_TOOL.into(),
            description: "List the current workspace files.".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities(durable: bool, resume: bool) -> serde_json::Value {
        serde_json::json!({
            "durable": durable,
            "resume": resume,
        })
    }

    #[test]
    fn initialize_capabilities_must_advertise_durable_sessions() {
        assert!(
            Conversation::require_durable_capabilities(&serde_json::json!({
                "capabilities": capabilities(true, true)
            }))
            .is_ok()
        );
        assert!(
            Conversation::require_durable_capabilities(&serde_json::json!({
                "capabilities": capabilities(false, true)
            }))
            .is_err()
        );
        assert!(
            Conversation::require_durable_capabilities(&serde_json::json!({
                "capabilities": capabilities(true, false)
            }))
            .is_err()
        );
        assert!(Conversation::require_durable_capabilities(&serde_json::json!({})).is_err());
    }
}
