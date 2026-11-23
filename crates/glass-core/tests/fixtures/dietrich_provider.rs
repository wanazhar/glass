use serde_json::{Value, json};

pub const PLAN: &str = "The user is asking why I am leaking my reasoning. I need to respond as Dietrich, being direct and quick. I should acknowledge the issue, explain that I should keep reasoning private, and apologize. I should not include any reasoning in my response. I'll output only the final answer.";
pub const ANSWER: &str =
    "My bad — I'll keep my reasoning private from now on. Just the final answer.";

pub struct Case {
    pub name: &'static str,
    pub provider: &'static str,
    pub frames: Vec<Value>,
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for (name, delta) in [
        (
            "chat-thought",
            json!({"content":[{"type":"text","thought":true,"text":PLAN}]}),
        ),
        (
            "chat-thinking-part",
            json!({"content":[{"type":"thinking","thinking":PLAN}]}),
        ),
        ("chat-thinking-field", json!({"thinking":PLAN})),
        (
            "chat-reasoning-details",
            json!({"reasoning_details":[{"type":"reasoning.text","text":PLAN},{"type":"reasoning.encrypted","data":"SECRET"}]}),
        ),
        (
            "chat-structured-reasoning",
            json!({"reasoning":[{"type":"text","text":PLAN}]}),
        ),
        (
            "chat-gemini-compatible-parts",
            json!({"content":[{"thought":true,"text":PLAN}]}),
        ),
        ("chat-reasoning-content", json!({"reasoning_content":PLAN})),
        ("chat-reasoning-string", json!({"reasoning":PLAN})),
    ] {
        cases.push(Case {
            name,
            provider: "openai",
            frames: vec![
                json!({"choices":[{"delta":delta}]}),
                json!({"choices":[{"delta":{"content":ANSWER},"finish_reason":"stop"}]}),
            ],
        });
    }
    cases.push(Case {
        name: "chat-text-parts",
        provider: "openai",
        frames: vec![
            json!({"choices":[{"delta":{"content":[{"type":"text","index":0,"text":PLAN}]}}]}),
            json!({"choices":[{"delta":{"content":[{"type":"text","index":1,"text":ANSWER}]},"finish_reason":"stop"}]}),
        ],
    });
    cases.push(Case {
        name: "responses-reasoning-text",
        provider: "openai-responses",
        frames: vec![
            json!({"type":"response.reasoning_text.delta","item_id":"private","content_index":0,"delta":PLAN}),
            json!({"type":"response.reasoning_text.done","item_id":"private","content_index":0,"text":PLAN}),
            json!({"type":"response.output_text.delta","item_id":"answer","content_index":0,"delta":ANSWER}),
            json!({"type":"response.completed","response":{"usage":{"input_tokens":1,"output_tokens":1}}}),
        ],
    });
    cases.push(Case {
        name: "responses-text-parts",
        provider: "openai-responses",
        frames: vec![
            json!({"type":"response.output_text.delta","item_id":"message","content_index":0,"delta":PLAN}),
            json!({"type":"response.output_text.delta","item_id":"message","content_index":1,"delta":ANSWER}),
            json!({"type":"response.completed","response":{"usage":{"input_tokens":1,"output_tokens":1}}}),
        ],
    });
    cases.push(Case {
        name: "anthropic-thinking",
        provider: "anthropic",
        frames: vec![
            json!({"type":"message_start","message":{"usage":{"input_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":PLAN}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":ANSWER}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}),
            json!({"type":"message_stop"}),
        ],
    });
    cases.push(Case {
        name: "anthropic-text-blocks",
        provider: "anthropic",
        frames: vec![
            json!({"type":"message_start","message":{"usage":{"input_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":PLAN}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":ANSWER}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":1}}),
            json!({"type":"message_stop"}),
        ],
    });
    cases
}
