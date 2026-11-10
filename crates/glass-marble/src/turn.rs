//! One durable Glass marble turn: submit, host tool, final text.

use crate::{Approval, MarbleError, TURN_TIMEOUT, conversation::Conversation, host};
use glass_runtime_client::{Incoming, TurnInput};
use serde_json::Value;

/// Durable turn submitted to a Glass conversation.
#[derive(Debug, Clone)]
pub struct MarbleTurn {
    /// Durable conversation to submit to; `None` creates a fresh one.
    pub conversation_id: Option<String>,
    pub request_id: String,
    pub input: TurnInput,
}

/// Final delivered reply for one marble turn.
#[derive(Debug, Clone)]
pub struct MarbleReply {
    pub conversation_id: String,
    pub task_id: String,
    pub text: String,
}

pub fn run_turn(
    turn: &MarbleTurn,
    conversation: &mut Conversation,
) -> Result<MarbleReply, MarbleError> {
    let conversation_id = match &turn.conversation_id {
        Some(id) => id.clone(),
        None => conversation.create()?,
    };
    // Durable request ids are scoped to the real durable conversation so a
    // resumed conversation keeps its submission identity.
    let request_id = format!("marble-{}-{}", conversation_id, turn.request_id);
    eprintln!("PHASE submit-start");
    let task_id = conversation.submit(&conversation_id, &request_id, &turn.input)?;
    eprintln!("PHASE submit-done task={task_id}");
    let run_id = conversation.start(&task_id)?;
    eprintln!("PHASE run-start");
    let mut text = String::new();
    let response = loop {
        let incoming = conversation
            .recv_typed_timeout(TURN_TIMEOUT)?
            .ok_or_else(|| {
                MarbleError::Protocol(format!("timed out waiting for response {run_id}"))
            })?;
        match incoming {
            Incoming::Response { id, result } => {
                if id.as_str() != Some(run_id.as_str()) {
                    return Err(MarbleError::Protocol(format!(
                        "unexpected runtime response id {id:?}"
                    )));
                }
                eprintln!("PHASE response-break");
                break result.map_err(|error| {
                    MarbleError::Protocol(format!(
                        "runtime error {}: {}",
                        error.code, error.message
                    ))
                })?;
            }
            Incoming::Event(event) => {
                if event.kind == "text"
                    && let Some(delta) = event.data.get("text").and_then(Value::as_str)
                {
                    text.push_str(delta);
                }
            }
            Incoming::Tool { request_id, call } => {
                let key = request_id
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| request_id.to_string());
                let decision = conversation.host().decision();
                let approval = if decision == Approval::Approve {
                    Approval::Approve
                } else {
                    conversation
                        .approvals()
                        .register(&key, &call.call_id, &call.name, call.arguments.clone())
                        .recv_timeout(TURN_TIMEOUT)
                        .unwrap_or(decision)
                };
                conversation.approvals().resolve(&key, approval);
                host::answer_tool_request(conversation.client_mut(), request_id, &call, approval)?;
            }
            Incoming::Unknown(_) => {}
        }
    };
    eprintln!("PHASE response-done");
    let _ = response;
    conversation.shutdown()?;
    eprintln!("PHASE shutdown-done");
    eprintln!("PHASE before-Ok");
    Ok(MarbleReply {
        conversation_id,
        task_id,
        text,
    })
}
