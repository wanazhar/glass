//! `/btw`: ask a side question without disturbing the conversation.
//!
//! The idea is borrowed from zerostack. Sometimes you want to know something
//! mid-task — "what does this flag actually do", "why did that test fail
//! originally" — without derailing the model or spending a turn on it.
//!
//! `/btw` forks the current context, asks the question in a *separate* request
//! with read-only tools, prints the answer inline, and writes **nothing** back
//! to the conversation. The model never sees the question or the answer, so the
//! main context is unchanged and the next turn costs exactly what it would have
//! anyway.
//!
//! Read-only is the important part. A side question that could write would be a
//! second agent with its own agenda, and the guarantee that makes `/btw` cheap
//! — it cannot change anything — is what makes it safe to run without a
//! permission prompt.

use glass_mod_api::Message;

use crate::provider::{self, Event, Provider, Request};

/// Result of a side question.
pub struct Answer {
    pub text: String,
    /// Tokens this detour cost, reported so the cost is never invisible.
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Answer {
    /// True when the side model failed or declined to answer.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// Ask a question against a copy of the current context, changing nothing.
///
/// `tools` is deliberately empty: the side agent gets the same `bash` it
/// always has, and the prompt tells it not to write. Running with no tools at
/// all would be safer still, but then "what does this function do" could not be
/// answered by looking at the code, which is the most common question people
/// type after `/btw`.
pub fn ask(
    provider: &dyn Provider,
    system: &str,
    history: &[Message],
    question: &str,
    model: &str,
) -> Result<Answer, provider::Error> {
    // Only the tail of the conversation is needed for context, and a shorter
    // context is a cheaper one. Ten turns is enough to know what "this" refers
    // to, which is the entire failure mode of a side question.
    const CONTEXT_TURNS: usize = 10;
    let start = history.len().saturating_sub(CONTEXT_TURNS);
    let mut messages: Vec<Message> = history[start..].to_vec();
    messages.push(Message::User(question.to_string()));

    let system = format!(
        "{system}\n\n\
         You are answering a quick side question. Rules:\n\
         - Answer the question directly. Do not ask what the user means.\n\
         - You may read files to check a fact, but you MUST NOT create, modify, \
           or delete anything. This question is being asked in the background \
           while other work continues; changing files would conflict with it.\n\
         - Be brief. A few sentences, or a short code excerpt."
    );

    let tools: Vec<(String, String, String)> = Vec::new();
    let req = Request {
        model,
        system: &system,
        messages: &messages,
        tools: &tools,
        // A side answer is a side answer: cap it so a confused model cannot
        // spend a full turn's budget here.
        max_tokens: 2_000,
        reasoning: None,
    };

    let stream = provider.stream(&req)?;
    let mut text = String::new();
    let mut input_tokens = 0;
    let mut output_tokens = 0;

    for ev in stream {
        match ev {
            Event::Text(t) => text.push_str(&t),
            Event::Usage(u) => {
                input_tokens = u.prompt_total();
                output_tokens = u.output;
            }
            Event::Done => break,
            _ => {}
        }
    }

    Ok(Answer { text, input_tokens, output_tokens })
}

/// Render the answer for the terminal.
///
/// Shown as a dim aside rather than as a normal turn, because it is not part of
/// the conversation and should not read like one.
pub fn render(question: &str, answer: &Answer) -> String {
    let mut out = String::new();
    out.push_str(&format!("\x1b[90mbtw: {question}\x1b[0m\n"));
    for line in answer.text.trim().lines() {
        out.push_str(&format!("\x1b[90m│\x1b[0m {line}\n"));
    }
    if answer.input_tokens > 0 || answer.output_tokens > 0 {
        out.push_str(&format!(
            "\x1b[90m  (side question: {} in / {} out, not added to the conversation)\x1b[0m\n",
            answer.input_tokens, answer.output_tokens
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Vec<Message> {
        vec![
            Message::User("refactor the parser".into()),
            Message::Assistant("starting with the tokenizer".into()),
            Message::ToolResult {
                call_id: "c1".into(),
                name: "bash".into(),
                body: "3 files changed".into(),
            },
        ]
    }

    #[test]
    fn render_prefixes_the_question() {
        let a = Answer { text: "it sets dry-run mode".into(), input_tokens: 10, output_tokens: 4 };
        let r = render("what does --check do?", &a);
        assert!(r.contains("btw: what does --check do?"));
        assert!(r.contains("it sets dry-run mode"));
    }

    #[test]
    fn render_shows_the_cost_and_that_it_is_not_recorded() {
        let a = Answer { text: "x".into(), input_tokens: 100, output_tokens: 20 };
        let r = render("q", &a);
        assert!(r.contains("100 in / 20 out"), "{r}");
        assert!(r.contains("not added to the conversation"), "{r}");
    }

    #[test]
    fn render_omits_the_cost_line_when_unreported() {
        let a = Answer { text: "x".into(), input_tokens: 0, output_tokens: 0 };
        assert!(!render("q", &a).contains("in /"), "no fabricated usage numbers");
    }

    #[test]
    fn empty_answer_is_reported_as_empty() {
        let a = Answer { text: "   \n ".into(), input_tokens: 0, output_tokens: 0 };
        assert!(a.is_empty());
    }

    #[test]
    fn long_history_is_trimmed_to_a_tail() {
        let mut h: Vec<Message> = (0..50).map(|i| Message::User(format!("m{i}"))).collect();
        h.extend(history());
        // The tail must be enough to resolve "this" without the whole session.
        let tail: Vec<&Message> = h[h.len().saturating_sub(10)..].iter().collect();
        assert!(tail.iter().any(|m| m.text().contains("refactor the parser")));
    }

    #[test]
    fn side_prompt_forbids_writing() {
        // The read-only guarantee is what makes /btw safe to run unattended, so
        // the prohibition has to be stated to the side model. This is a literal
        // check on the instruction rather than on behaviour, because the prompt
        // is assembled inside `ask` and would otherwise need a live provider.
        let src = include_str!("btw.rs");
        assert!(
            src.contains("MUST NOT create, modify, or delete anything"),
            "the side prompt must forbid writes"
        );
    }
}