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
/// Splits `<think>…</think>` / `<thinking>…</thinking>` blocks out of a text
/// stream into reasoning. Providers sometimes send their reasoning as plain
/// text deltas (no separate `reasoning` event), and a tag can straddle chunk
/// boundaries, so this keeps a pending tail that might be half a tag.
#[derive(Debug, Default)]
struct ThinkSplitter {
    pending: String,
    in_think: bool,
}

impl ThinkSplitter {
    const OPEN_TAGS: [&'static str; 2] = ["<thinking>", "<think>"];
    const CLOSE_TAGS: [&'static str; 2] = ["</thinking>", "</think>"];

    /// Feed one delta; returns (text, reasoning) produced by it.
    fn feed(&mut self, delta: &str) -> (String, String) {
        self.pending.push_str(delta);
        self.drain(false)
    }

    /// End of stream: unterminated input goes where the parser stood —
    /// inside an open think block it is reasoning, outside it is text.
    fn flush(&mut self) -> (String, String) {
        self.drain(true)
    }

    fn drain(&mut self, eof: bool) -> (String, String) {
        let mut text = String::new();
        let mut reasoning = String::new();
        loop {
            if self.in_think {
                match Self::find_tag(&self.pending, &Self::CLOSE_TAGS) {
                    Some((start, len)) => {
                        reasoning.push_str(&self.pending[..start]);
                        self.pending.drain(..start + len);
                        self.in_think = false;
                    }
                    None => {
                        let keep = Self::partial_tag_tail(&self.pending, &Self::CLOSE_TAGS);
                        if eof || keep == 0 {
                            reasoning.push_str(&self.pending);
                            self.pending.clear();
                        } else {
                            let cut = self.pending.len() - keep;
                            reasoning.push_str(&self.pending[..cut]);
                            self.pending.drain(..cut);
                        }
                        break;
                    }
                }
            } else {
                match Self::find_tag(&self.pending, &Self::OPEN_TAGS) {
                    Some((start, len)) => {
                        text.push_str(&self.pending[..start]);
                        self.pending.drain(..start + len);
                        self.in_think = true;
                    }
                    None => {
                        let keep = Self::partial_tag_tail(&self.pending, &Self::OPEN_TAGS);
                        if eof || keep == 0 {
                            text.push_str(&self.pending);
                            self.pending.clear();
                        } else {
                            let cut = self.pending.len() - keep;
                            text.push_str(&self.pending[..cut]);
                            self.pending.drain(..cut);
                        }
                        break;
                    }
                }
            }
        }
        (text, reasoning)
    }

    /// Earliest occurrence of any tag in `haystack` as (offset, length).
    fn find_tag(haystack: &str, tags: &[&str]) -> Option<(usize, usize)> {
        tags.iter()
            .filter_map(|tag| haystack.find(tag).map(|at| (at, tag.len())))
            .min_by_key(|(at, _)| *at)
    }

    /// Length of a trailing run that could still grow into a full tag, so it
    /// must be held for the next delta.
    fn partial_tag_tail(pending: &str, tags: &[&str]) -> usize {
        for cut in (1..=pending.len().min(10)).rev() {
            if !pending.is_char_boundary(pending.len() - cut) {
                continue;
            }
            let tail = &pending[pending.len() - cut..];
            if !tail.starts_with('<') {
                continue;
            }
            if tags.iter().any(|tag| tag.starts_with(tail)) {
                return cut;
            }
        }
        0
    }
}

pub fn run_turn(
    turn: &MarbleTurn,
    conversation: &mut Conversation,
    options: TurnOptions<'_>,
) -> Result<MarbleReply, MarbleError> {
    let mut live = options.live;
    // Every path that returns finishes the journal entry: a leftover file is
    // read on the next start as "this turn was running", which a turn that
    // never reached the engine was not. A panic is the exception — dropping
    // without finish is exactly the crash the journal exists for.
    let conversation_id = match turn.conversation_id.as_deref() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => match conversation.create() {
            Ok(id) => id,
            Err(error) => {
                if let Some(turn) = live.as_deref_mut() {
                    turn.finish();
                }
                return Err(error);
            }
        },
    };
    // Durable request ids are scoped to the real durable conversation so a
    // resumed conversation keeps its submission identity, while each new turn
    // still carries a distinct id.
    let request_id = format!("marble-{conversation_id}-{}", turn.request_id);
    let task_id = match conversation.submit(&conversation_id, &request_id, &turn.input) {
        Ok(task_id) => task_id,
        Err(error) => {
            if let Some(turn) = live.as_deref_mut() {
                turn.finish();
            }
            return Err(error);
        }
    };
    conversation.set_active(&conversation_id, &task_id);
    if let Some(on_accepted) = options.on_accepted {
        on_accepted(&conversation_id, &task_id);
    }
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
    let mut splitter = ThinkSplitter::default();
    let mut usage: Option<Value> = None;
    let events = conversation.events().clone();
    let publish_split = |visible: &str, thought: &str| {
        for (kind, delta) in [("text", visible), ("reasoning", thought)] {
            if !delta.is_empty() {
                events.publish(
                    &conversation_id,
                    &task_id,
                    kind,
                    serde_json::json!({"text": delta}),
                );
            }
        }
    };
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
                // Providers may inline `<think>` blocks inside ordinary text
                // deltas; split them out so reasoning never lands in replies.
                let (visible, thought) = splitter.feed(delta);
                publish_split(&visible, &thought);
                text.push_str(&visible);
                reasoning.push_str(&thought);
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
    let (visible, thought) = splitter.flush();
    publish_split(&visible, &thought);
    text.push_str(&visible);
    reasoning.push_str(&thought);
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

#[cfg(test)]
mod tests {
    use super::ThinkSplitter;

    fn split(deltas: &[&str]) -> (String, String) {
        let mut splitter = ThinkSplitter::default();
        let mut text = String::new();
        let mut reasoning = String::new();
        for delta in deltas {
            let (t, r) = splitter.feed(delta);
            text.push_str(&t);
            reasoning.push_str(&r);
        }
        let (t, r) = splitter.flush();
        text.push_str(&t);
        reasoning.push_str(&r);
        (text, reasoning)
    }

    #[test]
    fn plain_text_passes_through() {
        let (text, reasoning) = split(&["Hello ", "world."]);
        assert_eq!(text, "Hello world.");
        assert!(reasoning.is_empty());
    }

    #[test]
    fn think_block_becomes_reasoning() {
        let (text, reasoning) = split(&["ok <think>let me reason</think> done"]);
        assert_eq!(text, "ok  done");
        assert_eq!(reasoning, "let me reason");
    }

    #[test]
    fn thinking_block_becomes_reasoning() {
        let (text, reasoning) = split(&["a<thinking>plan</thinking>b"]);
        assert_eq!(text, "ab");
        assert_eq!(reasoning, "plan");
    }

    #[test]
    fn tags_split_across_deltas() {
        let (text, reasoning) = split(&["hel", "<th", "ink>rea", "soning</thi", "nk>", " lo"]);
        assert_eq!(text, "hel lo");
        assert_eq!(reasoning, "reasoning");
    }

    #[test]
    fn unterminated_block_is_reasoning_not_text() {
        let (text, reasoning) = split(&["start <think>never closed"]);
        assert_eq!(text, "start ");
        assert_eq!(reasoning, "never closed");
    }

    #[test]
    fn lone_angle_brackets_survive() {
        let (text, reasoning) = split(&["a < b and x < y <think>r</think>"]);
        assert_eq!(text, "a < b and x < y ");
        assert_eq!(reasoning, "r");
    }
}
