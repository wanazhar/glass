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
    const OPEN_TAGS: [&'static str; 4] = ["<thinking>", "<think>", "<thought>", "<reasoning>"];
    const CLOSE_TAGS: [&'static str; 4] = ["</thinking>", "</think>", "</thought>", "</reasoning>"];

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
    mut options: TurnOptions<'_>,
) -> Result<MarbleReply, MarbleError> {
    let result = run_with_retry(turn, conversation, &mut options);
    if let Some(live) = options.live {
        live.finish();
    }
    result
}

fn run_with_retry(
    turn: &MarbleTurn,
    conversation: &mut Conversation,
    options: &mut TurnOptions<'_>,
) -> Result<MarbleReply, MarbleError> {
    let product = conversation.product_state();
    let mut next = turn.clone();
    let mut logical_task: Option<String> = None;
    for attempt in 0..=1 {
        let mut accepted = None;
        let mut on_accepted = |conversation_id: &str, task_id: &str| {
            accepted = Some((conversation_id.to_string(), task_id.to_string()));
            if let Some(hook) = options.on_accepted.as_deref_mut() {
                hook(conversation_id, task_id);
            }
        };
        let result = run_attempt(
            &next,
            conversation,
            logical_task.as_deref(),
            TurnOptions {
                cancel: options.cancel,
                timeout: options.timeout,
                on_accepted: Some(&mut on_accepted),
                live: options.live.as_deref_mut(),
            },
        );
        let Err(error) = &result else {
            return result;
        };
        if attempt == 1
            || !crate::failure::transient(&error.to_string())
            || error.to_string().contains("waiting for runtime response")
            || options
                .cancel
                .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            return result;
        }
        let (Some(product), Some((conversation_id, task_id))) = (&product, accepted) else {
            return result;
        };
        {
            let mut state = product.lock().unwrap_or_else(|e| e.into_inner());
            if !state
                .everyone_pending
                .contains_key(&format!("task:{task_id}"))
                || state
                    .transcript
                    .iter()
                    .any(|e| e.task_id == task_id && e.kind == "tool")
                || !state
                    .conversation_mut(&conversation_id)
                    .checkpoints
                    .contains_key(&task_id)
            {
                return result;
            }
            state.conversation_mut(&conversation_id).retry_task_id = task_id.clone();
        }
        conversation.publish("task_retry", serde_json::json!({"attempt":1,"phase":"generation","reason":"transient_failure","taskId":task_id}));
        conversation.ensure_runtime()?;
        next.conversation_id = Some(conversation_id);
        next.request_id = format!("{}-retry-1", turn.request_id);
        logical_task = Some(task_id);
    }
    unreachable!()
}

fn run_attempt(
    turn: &MarbleTurn,
    conversation: &mut Conversation,
    logical_task: Option<&str>,
    options: TurnOptions<'_>,
) -> Result<MarbleReply, MarbleError> {
    let mut live = options.live;
    // Every path that returns finishes the journal entry: a leftover file is
    // read on the next start as "this turn was running", which a turn that
    // never reached the engine was not. A panic is the exception — dropping
    // without finish is exactly the crash the journal exists for.
    let conversation_id = match turn.conversation_id.as_deref() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => conversation.create().or_else(|error| {
            if !crate::failure::transient(&error.to_string()) {
                return Err(error);
            }
            conversation.publish(
                "task_retry",
                serde_json::json!({"attempt":1,"phase":"admission","reason":"transient_failure"}),
            );
            conversation.ensure_runtime()?;
            conversation.create()
        })?,
    };
    // Durable request ids are scoped to the real durable conversation so a
    // resumed conversation keeps its submission identity, while each new turn
    // still carries a distinct id.
    let request_id = format!("marble-{conversation_id}-{}", turn.request_id);
    let engine_task_id = conversation.submit(&conversation_id, &request_id, &turn.input)?;
    let task_id = logical_task.unwrap_or(&engine_task_id).to_string();
    if logical_task.is_some()
        && let Some(product) = conversation.product_state()
    {
        let mut state = product.lock().unwrap_or_else(|e| e.into_inner());
        let record = state.conversation_mut(&conversation_id);
        if let Some(checkpoint) = record.checkpoints.get(&engine_task_id).copied() {
            record.checkpoints.insert(task_id.clone(), checkpoint);
        }
    }
    conversation.set_active(&conversation_id, &task_id);
    if let Some(on_accepted) = options.on_accepted {
        on_accepted(&conversation_id, &task_id);
    }
    if let Some(turn) = live.as_deref_mut() {
        turn.note_task(&conversation_id, &task_id);
    }
    let run_id = conversation.start(&engine_task_id)?;
    let deadline = Instant::now() + options.timeout;
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut reasoning_boundary = false;
    let mut splitter = ThinkSplitter::default();
    let mut last_segment = String::new();
    let mut usage: Option<Value> = None;
    let events = conversation.events().clone();
    let streaming = conversation.streams_text_blocks();
    let publish_text = |delta: &str| {
        if streaming && !delta.is_empty() {
            events.publish(
                &conversation_id,
                &task_id,
                "text",
                serde_json::json!({"text":delta}),
            );
        }
    };
    let reset_text = || {
        if streaming {
            events.publish(
                &conversation_id,
                &task_id,
                "text_reset",
                serde_json::json!({"text":""}),
            );
        }
    };
    let publish_reasoning = |delta: &str| {
        if !delta.is_empty() {
            events.publish(
                &conversation_id,
                &task_id,
                "reasoning",
                serde_json::json!({"text": delta}),
            );
        }
    };
    let cancel = options.cancel.map(|requested| CancelToken {
        requested,
        task_id: &engine_task_id,
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
                publish_reasoning(&append_reasoning(
                    &mut reasoning,
                    &mut reasoning_boundary,
                    &thought,
                ));
                text.push_str(&visible);
                publish_text(&visible);
                if let Some(turn) = live.as_deref_mut() {
                    turn.progress(if streaming { &text } else { "" }, &reasoning);
                }
            }
        }
        "reasoning" => {
            if let Some(delta) = event.data.get("text").and_then(Value::as_str) {
                publish_reasoning(&append_reasoning(
                    &mut reasoning,
                    &mut reasoning_boundary,
                    delta,
                ));
                if let Some(turn) = live.as_deref_mut() {
                    turn.progress(if streaming { &text } else { "" }, &reasoning);
                }
            }
        }
        "text_block_start" | "tool_start" => {
            if event.kind == "text_block_start"
                && (!streaming
                    || event
                        .data
                        .get("blockId")
                        .and_then(Value::as_str)
                        .is_none_or(str::is_empty))
            {
                return;
            }
            let (visible, thought) = splitter.flush();
            text.push_str(&visible);
            publish_reasoning(&append_reasoning(
                &mut reasoning,
                &mut reasoning_boundary,
                &thought,
            ));
            let commentary = take_commentary(&mut text, &mut reasoning, &mut last_segment);
            if event.kind == "text_block_start" {
                splitter = ThinkSplitter::default();
                last_segment.clear();
            }
            reset_text();
            publish_reasoning(&commentary);
            reasoning_boundary = true;
            if let Some(turn) = live.as_deref_mut() {
                turn.progress("", &reasoning);
            }
        }
        "usage" => usage = Some(event.data.clone()),
        _ => {}
    });
    if let Err(error) = outcome {
        reset_text();
        conversation.publish("error", serde_json::json!({ "message": error.to_string() }));
        return Err(error);
    }
    let (visible, thought) = splitter.flush();
    publish_reasoning(&append_reasoning(
        &mut reasoning,
        &mut reasoning_boundary,
        &thought,
    ));
    text.push_str(&visible);
    publish_text(&visible);
    if !streaming && text.trim().is_empty() {
        text = last_segment;
    }
    text = conversation.clean_reply(&text);
    if text.trim_start().starts_with("[Generation failed]") {
        reset_text();
        return Err(MarbleError::Protocol(
            "provider_rejected: model returned a generation failure".into(),
        ));
    }
    if !streaming && !text.is_empty() {
        conversation.publish("text", serde_json::json!({"text": text}));
    }
    let cancelled = options
        .cancel
        .map(|requested| requested.load(Ordering::Relaxed))
        .unwrap_or(false);
    if !cancelled && text.trim().is_empty() {
        return Err(MarbleError::Protocol(
            "provider returned an empty reply".into(),
        ));
    }
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

fn append_reasoning(reasoning: &mut String, boundary: &mut bool, delta: &str) -> String {
    if delta.is_empty() {
        return String::new();
    }
    let delta = if *boundary && !reasoning.is_empty() {
        format!("\n\n{delta}")
    } else {
        delta.to_string()
    };
    reasoning.push_str(&delta);
    *boundary = false;
    delta
}

fn take_commentary(text: &mut String, reasoning: &mut String, last: &mut String) -> String {
    let segment = std::mem::take(text);
    if segment.trim().is_empty() {
        return String::new();
    }
    last.clone_from(&segment);
    let delta = if reasoning.is_empty() {
        segment
    } else {
        format!("\n\n{segment}")
    };
    reasoning.push_str(&delta);
    delta
}

#[cfg(test)]
mod tests {
    use super::ThinkSplitter;

    #[test]
    fn tool_boundaries_separate_commentary_and_keep_a_fallback() {
        let mut text = "Research info.".to_string();
        let mut reasoning = String::new();
        let mut last = String::new();
        super::take_commentary(&mut text, &mut reasoning, &mut last);
        assert!(text.is_empty());
        text.push_str("Checking sources.");
        super::take_commentary(&mut text, &mut reasoning, &mut last);
        assert_eq!(reasoning, "Research info.\n\nChecking sources.");
        assert_eq!(last, "Checking sources.");
        assert!(super::take_commentary(&mut text, &mut reasoning, &mut last).is_empty());
        assert_eq!(last, "Checking sources.");
        text.push_str("The answer.");
        assert_eq!(text, "The answer.");
        let mut boundary = true;
        super::append_reasoning(&mut reasoning, &mut boundary, "Private ");
        super::append_reasoning(&mut reasoning, &mut boundary, "thought.");
        assert_eq!(
            reasoning,
            "Research info.\n\nChecking sources.\n\nPrivate thought."
        );
    }

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
