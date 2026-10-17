//! Typed, response-scoped Chat Completions reasoning replay. Display reasoning
//! records are deliberately not interpreted as wire payloads: Responses
//! summaries and Anthropic thinking have different provider contracts.
use crate::session::Record;
use glass_mod_api::Message;
use serde::{Deserialize, Serialize};
pub const KIND: &str = "provider/chat-reasoning";
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
struct Payload {
    content: Option<String>,
    assistant_hash: Option<String>,
    tool_call_ids: Vec<String>,
}
pub fn record(content: Option<&str>, text: &str, calls: &[glass_mod_api::ToolCall]) -> Record {
    let payload = Payload {
        content: content.map(str::to_owned),
        assistant_hash: (!text.trim().is_empty()).then(|| crate::workspace::hash(text.as_bytes())),
        tool_call_ids: calls.iter().map(|c| c.id.clone()).collect(),
    };
    Record::Custom {
        kind: KIND.into(),
        data: Some(serde_json::to_string(&payload).expect("string-only reasoning payload")),
    }
}
fn payload(record: &Record) -> Option<Payload> {
    let Record::Custom {
        kind,
        data: Some(data),
    } = record
    else {
        return None;
    };
    if kind != KIND || data.len() > MAX_BYTES * 6 + 65536 {
        return None;
    }
    let value: Payload = serde_json::from_str(data).ok()?;
    (value.content.as_ref().map_or(0, String::len) <= MAX_BYTES && value.tool_call_ids.len() <= 256)
        .then_some(value)
}
pub fn message(record: &Record) -> Option<Message> {
    let p = payload(record)?;
    Some(Message::ChatReasoning {
        content: p.content,
        assistant_hash: p.assistant_hash,
        tool_call_ids: p.tool_call_ids,
    })
}
/// Reinsert metadata whose precise source target survived a truncation.
/// Source matching walks backwards in order, so identical assistant texts in
/// different responses cannot cause a forward search into a later response.
/// Raw offsets/compaction coordinates remain untouched in durable records.
fn associations(source: &[Record]) -> std::collections::HashMap<usize, usize> {
    let mut associations = std::collections::HashMap::new();
    let mut active: Option<(usize, Payload, std::collections::HashSet<String>)> = None;
    for (index, record) in source.iter().enumerate() {
        if let Some(p) = payload(record) {
            let ids = p.tool_call_ids.iter().cloned().collect();
            active = Some((index, p, ids));
            continue;
        }
        match record {
            Record::User(_) => active = None,
            Record::Assistant(text) => {
                if let Some((marker, p, _)) = &active {
                    if *marker + 1 == index
                        && p.assistant_hash.as_deref()
                            == Some(crate::workspace::hash(text.as_bytes()).as_str())
                    {
                        associations.insert(index, *marker);
                        continue;
                    }
                }
                active = None;
            }
            Record::ToolCall { id, .. } => {
                if let Some((marker, _, ids)) = &mut active {
                    if ids.remove(id) {
                        associations.insert(index, *marker);
                    } else {
                        active = None;
                    }
                }
            }
            _ => {}
        }
    }
    associations
}
pub fn checkpoint_markers(
    source: &[Record],
    range: std::ops::Range<usize>,
    pending: &std::collections::HashSet<String>,
) -> std::collections::HashSet<usize> {
    associations(source)
        .into_iter()
        .filter_map(|(target, marker)| {
            (range.contains(&target)
                && matches!(&source[target],Record::ToolCall{id,..}if pending.contains(id)))
            .then_some(marker)
        })
        .collect()
}
pub fn retain(source: &[Record], mut kept: Vec<Record>) -> Vec<Record> {
    let associations = associations(source);
    let mut cursor = source.len();
    let mut positions = vec![None; kept.len()];
    for (i, record) in kept.iter().enumerate().rev() {
        if let Some(index) = source[..cursor].iter().rposition(|r| r == record) {
            positions[i] = Some(index);
            cursor = index;
        }
    }
    let present: std::collections::HashSet<usize> = positions.iter().flatten().copied().collect();
    let mut additions = std::collections::BTreeMap::new();
    let mut used = std::collections::HashSet::new();
    for (i, index) in positions.iter().enumerate() {
        let Some(marker) = index.and_then(|index| associations.get(&index)) else {
            continue;
        };
        if !present.contains(marker) && used.insert(*marker) {
            additions.insert(i, source[*marker].clone());
        }
    }
    for (index, record) in additions.into_iter().rev() {
        kept.insert(index, record);
    }
    kept
}

/// Private metadata is recognizable before a large JSON string is decoded.
pub fn private_value(value: &serde_json::Value) -> bool {
    value["k"] == "custom" && value["c"] == KIND
}
pub fn private_at(path: &std::path::Path, offset: u64) -> Result<bool, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut prefix = [0u8; 64];
    let n = file.read(&mut prefix).map_err(|e| e.to_string())?;
    Ok(prefix[..n].starts_with(b"{\"_private\":\"provider/chat-reasoning\""))
}
pub fn public_window(
    path: &std::path::Path,
    start: u64,
    bytes: &[u8],
) -> Result<(u64, String), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut boundary = start;
    if start > 0 {
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let floor = start.saturating_sub(16 * 1024 * 1024);
        let mut cursor = start;
        loop {
            let from = cursor.saturating_sub(32768).max(floor);
            file.seek(SeekFrom::Start(from))
                .map_err(|e| e.to_string())?;
            let mut chunk = vec![0; (cursor - from) as usize];
            file.read_exact(&mut chunk).map_err(|e| e.to_string())?;
            if let Some(n) = chunk.iter().rposition(|b| *b == b'\n') {
                boundary = from + n as u64 + 1;
                break;
            }
            if from == 0 {
                boundary = 0;
                break;
            }
            if from == floor {
                return Err("Transcript record exceeds the private reader bound".into());
            }
            cursor = from;
        }
    }
    let mut before = start;
    let mut position = start;
    let mut output = Vec::new();
    for (i, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        let record_start = if i == 0 { boundary } else { position };
        let private = if i == 0 {
            private_at(path, record_start)?
        } else {
            line.starts_with(b"{\"_private\":\"provider/chat-reasoning\"")
        };
        if private {
            if i == 0 {
                before = boundary;
            }
        } else {
            output.extend_from_slice(line);
        }
        position += line.len() as u64;
    }
    Ok((before, String::from_utf8_lossy(&output).into_owned()))
}
