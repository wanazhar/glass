//! One durable Glass marble turn: submit, stream, settle.

use crate::{
    MarbleError,
    conversation::{CancelToken, Conversation},
    live::LiveTurn,
};
use glass_runtime_client::TurnInput;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long one durable turn may run before it is reported as timed out.
pub const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(900);

/// Durable turn submitted to a Glass conversation.
#[derive(Debug, Clone)]
pub struct MarbleTurn {
    /// Durable conversation to submit to; `None` creates a fresh one.
    pub conversation_id: Option<String>,
    /// Unique per turn; the durable request id is scoped to the conversation.
    pub request_id: String,
    pub input: TurnInput,
}

/// Final delivered reply for one marble turn.
#[derive(Debug, Clone, PartialEq)]
pub struct MarbleReply {
    pub conversation_id: String,
    pub task_id: String,
    pub text: String,
    pub reasoning: String,
    pub usage: Option<Value>,
    pub cancelled: bool,
}

impl MarbleReply {
    /// Short transcript preview for product surfaces and logs.
    pub fn preview(&self, limit: usize) -> String {
        let text = self.text.trim();
        if text.chars().count() <= limit {
            return text.to_string();
        }
        let mut preview: String = text.chars().take(limit).collect();
        preview.push('…');
        preview
    }
}

/// Durable identity callback: conversation id, then task id.
pub type AcceptedHook<'a> = &'a mut dyn FnMut(&str, &str);

/// Per-turn execution controls.
pub struct TurnOptions<'a> {
    pub cancel: Option<&'a AtomicBool>,
    pub timeout: Duration,
    /// Called once the durable task id exists, before the model runs.
    pub on_accepted: Option<AcceptedHook<'a>>,
    /// Where to journal the turn while it runs, so a crash keeps the partial
    /// reply instead of losing everything written since admission.
    pub live: Option<&'a mut LiveTurn>,
}

impl<'a> Default for TurnOptions<'a> {
    fn default() -> Self {
        Self {
            cancel: None,
            timeout: DEFAULT_TURN_TIMEOUT,
            on_accepted: None,
            live: None,
        }
    }
}

/// Run one durable turn, streaming every runtime event to the product bus.
pub fn run_turn(
    turn: &MarbleTurn,
    conversation: &mut Conversation,
    options: TurnOptions<'_>,
) -> Result<MarbleReply, MarbleError> {
    let conversation_id = match turn.conversation_id.as_deref() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => conversation.create()?,
    };
    // Durable request ids are scoped to the real durable conversation so a
    // resumed conversation keeps its submission identity, while each new turn
    // still carries a distinct id.
    let request_id = format!("marble-{conversation_id}-{}", turn.request_id);
    let task_id = conversation.submit(&conversation_id, &request_id, &turn.input)?;
    conversation.set_active(&conversation_id, &task_id);
    if let Some(on_accepted) = options.on_accepted {
        on_accepted(&conversation_id, &task_id);
    }
    let mut live = options.live;
    if let Some(turn) = live.as_deref_mut() {
        turn.note_task(&conversation_id, &task_id);
    }
    let run_id = match conversation.start(&task_id) {
        Ok(run_id) => run_id,
        Err(error) => {
            if let Some(turn) = live.as_deref_mut() {
                turn.finish();
            }
            return Err(error);
        }
    };
    let deadline = Instant::now() + options.timeout;
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut usage: Option<Value> = None;
    let cancel = options.cancel.map(|requested| CancelToken {
        requested,
        task_id: &task_id,
    });
    let outcome = conversation.pump(&run_id, deadline, cancel, &mut |event| match event
        .kind
        .as_str()
    {
        "text" => {
            if let Some(delta) = event.data.get("text").and_then(Value::as_str) {
                text.push_str(delta);
                if let Some(turn) = live.as_deref_mut() {
                    turn.progress(&text, &reasoning);
                }
            }
        }
        "reasoning" => {
            if let Some(delta) = event.data.get("text").and_then(Value::as_str) {
                reasoning.push_str(delta);
                if let Some(turn) = live.as_deref_mut() {
                    turn.progress(&text, &reasoning);
                }
            }
        }
        "usage" => usage = Some(event.data.clone()),
        _ => {}
    });
    if let Err(error) = outcome {
        if let Some(turn) = live.as_deref_mut() {
            turn.finish();
        }
        conversation.publish("error", serde_json::json!({ "message": error.to_string() }));
        return Err(error);
    }
    // The turn settled, so the journal entry has nothing left to recover; the
    // product document records the outcome instead.
    if let Some(turn) = live {
        turn.finish();
    }
    let cancelled = options
        .cancel
        .map(|requested| requested.load(Ordering::Relaxed))
        .unwrap_or(false);
    let reply = MarbleReply {
        conversation_id,
        task_id,
        text,
        reasoning,
        usage,
        cancelled,
    };
    conversation.publish(
        "settled",
        serde_json::json!({
            "taskId": reply.task_id,
            "cancelled": reply.cancelled,
            "usage": reply.usage,
        }),
    );
    Ok(reply)
}
