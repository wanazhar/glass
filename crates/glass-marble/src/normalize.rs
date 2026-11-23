//! Inbound text becomes the minimal typed turn input.

use glass_runtime_client::TurnInput;

pub(crate) fn engine_input(
    state: &crate::ProductState,
    conversation_id: Option<&str>,
    owner: &str,
    text: &str,
) -> TurnInput {
    let mut input = to_turn_input(&InboundMessage::new(text));
    let mut context = vec![current_datetime(
        crate::worker::now_millis(),
        state.routines.offset_minutes,
    )];
    context.push(crate::everyone::addressing(state, text, owner));
    context.push(crate::conversation::REPLY_PRIVACY_GUIDANCE.into());
    if let Some(previous) = conversation_id.filter(|id| !id.is_empty()).and_then(|id| {
        state.tasks.iter().rev().find(|task| {
            task.conversation_id == id
                && task.status != "queued"
                && !(task.status == "running" && task.id.starts_with("queued-"))
        })
    }) && matches!(
        previous.status.as_str(),
        "cancelled" | "failed" | "interrupted"
    ) && !state.transcript.iter().any(|entry| {
        entry.task_id == previous.id && entry.kind == "assistant" && !entry.text.trim().is_empty()
    }) {
        context.push("Host note: the earlier unanswered request was withdrawn. Do not answer it; answer only the newest message.".into());
    }
    if let Some(text) = state.memory.context_block_for(owner) {
        context.push(text);
    }
    input.text = format!(
        "[Marble host context]\n{}\n[/Marble host context]\n\n[User message]\n{text}",
        context.join("\n\n")
    );
    input
}

fn current_datetime(now: u64, offset_minutes: i32) -> String {
    let offset = offset_minutes.clamp(-840, 840);
    let seconds = (now / 1000) as i64 + i64::from(offset) * 60;
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let weekday = [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ][(days + 4).rem_euclid(7) as usize];
    let month = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ][(month - 1) as usize];
    let sign = if offset < 0 { '-' } else { '+' };
    format!(
        "Current date and time: {weekday}, {day} {month} {year}, {:02}:{:02} (UTC{sign}{:02}:{:02}).",
        time / 3600,
        time / 60 % 60,
        offset.abs() / 60,
        offset.abs() % 60
    )
}

/// One human-or-agent message entering a Glass marble turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    pub text: String,
}

impl InboundMessage {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Marble v1 carries text only; context and images stay empty.
pub fn to_turn_input(message: &InboundMessage) -> TurnInput {
    TurnInput {
        text: message.text.clone(),
        context: Vec::new(),
        images: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProductState, TaskRecord, TranscriptEntry};

    fn has_withdrawal(input: TurnInput) -> bool {
        input.text.contains("withdrawn")
    }

    #[test]
    fn resumed_conversations_receive_the_reply_privacy_instruction_each_turn() {
        let mut state = ProductState::default();
        state.conversation_mut("existing");
        for owner in ["marble", "ada"] {
            for text in ["private question", "@everyone question"] {
                let input = engine_input(&state, Some("existing"), owner, text);
                assert!(
                    input
                        .text
                        .contains(crate::conversation::REPLY_PRIVACY_GUIDANCE)
                );
                assert!(input.text.ends_with(text));
            }
        }
    }

    #[test]
    fn each_turn_carries_the_configured_local_clock() {
        assert_eq!(
            current_datetime(0, 480),
            "Current date and time: Thursday, 1 January 1970, 08:00 (UTC+08:00)."
        );
        assert_eq!(
            current_datetime(0, -330),
            "Current date and time: Wednesday, 31 December 1969, 18:30 (UTC-05:30)."
        );
        assert_eq!(
            current_datetime(951_782_400_000, 0),
            "Current date and time: Tuesday, 29 February 2000, 00:00 (UTC+00:00)."
        );
        let mut state = ProductState::default();
        state.routines.offset_minutes = 480;
        for owner in ["marble", "ada"] {
            let input = engine_input(&state, None, owner, "hello");
            assert!(
                input.text.contains("Current date and time:") && input.text.contains("UTC+08:00")
            );
        }
    }

    #[test]
    fn withdrawn_unanswered_turn_is_not_the_next_request() {
        let mut state = ProductState::default();
        assert!(!has_withdrawal(engine_input(
            &state,
            Some("chat"),
            "marble",
            "new"
        )));
        for status in ["cancelled", "failed", "interrupted"] {
            state.tasks = vec![TaskRecord {
                id: "old".into(),
                conversation_id: "chat".into(),
                status: status.into(),
                ..Default::default()
            }];
            let input = engine_input(&state, Some("chat"), "marble", "new");
            assert!(input.text.ends_with("[User message]\nnew"));
            assert!(input.context.is_empty());
            assert!(has_withdrawal(input));
            assert!(!has_withdrawal(engine_input(
                &state,
                Some("other"),
                "marble",
                "new"
            )));
            assert!(!has_withdrawal(engine_input(&state, None, "marble", "new")));
        }
        state.transcript.push(TranscriptEntry {
            task_id: "old".into(),
            kind: "assistant".into(),
            text: "visible reply".into(),
            ..Default::default()
        });
        assert!(!has_withdrawal(engine_input(
            &state,
            Some("chat"),
            "marble",
            "new"
        )));
        state.transcript.clear();
        state.tasks[0].status = "completed".into();
        assert!(!has_withdrawal(engine_input(
            &state,
            Some("chat"),
            "marble",
            "new"
        )));
    }

    #[test]
    fn engine_input_serializes_as_exactly_one_user_message() {
        let mut state = ProductState::default();
        state.tasks.push(TaskRecord {
            id: "old".into(),
            conversation_id: "chat".into(),
            status: "failed".into(),
            ..Default::default()
        });
        state
            .memory
            .remember("Shared fact", "fact", "chat", true, "")
            .unwrap();
        for text in ["hello", "", "<script>user text</script>"] {
            let input = engine_input(&state, Some("chat"), "marble", text);
            let wire = serde_json::to_value(&input).unwrap();
            assert_eq!(wire.as_object().unwrap().len(), 1);
            assert!(wire.get("context").is_none());
            assert!(wire.get("images").is_none());
            assert!(input.text.contains("Shared fact"));
            assert!(input.text.contains("withdrawn"));
            assert!(input.text.ends_with(&format!("[User message]\n{text}")));
            assert!(state.transcript.is_empty());
        }
    }
}
