//! Server-sent-event decoding.
//!
//! Both providers stream over SSE and differ only in which event names and
//! JSON fields they use. Decoding is therefore one small state machine shared
//! by both adapters rather than a second dependency.

/// A decoded SSE record: the `event:` name and its `data:` payload.
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Incremental SSE decoder fed arbitrary byte-boundary chunks.
pub struct SseDecoder {
    buf: String,
    bytes: Vec<u8>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            bytes: Vec::new(),
        }
    }

    /// Preserve UTF-8 characters split across socket reads.
    pub fn push_bytes(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.bytes.extend_from_slice(chunk);
        let mut out = Vec::new();
        loop {
            let length = match std::str::from_utf8(&self.bytes) {
                Ok(_) => self.bytes.len(),
                Err(e) => e.valid_up_to() + e.error_len().unwrap_or(0),
            };
            if length == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&self.bytes[..length]).into_owned();
            self.bytes.drain(..length);
            out.extend(self.push(&text));
            if self.bytes.is_empty() {
                break;
            }
        }
        out
    }

    /// Feed a chunk and drain any complete events it produced.
    ///
    /// Events are separated by a blank line. `data:` lines accumulate and are
    /// joined with newlines, which is what the SSE spec requires and what
    /// both providers rely on for multi-line payloads.
    pub fn push(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buf.push_str(chunk);
        let mut out = Vec::new();

        // A chunk can end on a lone `\r` that is the first half of a `\r\n`
        // split across reads. Hold it back rather than normalizing a prefix
        // we have not fully seen yet.
        let held = self.buf.ends_with('\r');
        if held {
            self.buf.pop();
        }
        self.buf = self.buf.replace("\r\n", "\n");

        while let Some(idx) = self.buf.find("\n\n") {
            let raw: String = self.buf.drain(..idx + 2).collect();
            if let Some(ev) = parse_record(&raw) {
                out.push(ev);
            }
        }

        if held {
            self.buf.push('\r');
        }
        out
    }

    /// Flush a trailing record that arrived without its final blank line.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if self.buf.trim().is_empty() {
            return None;
        }
        let raw = std::mem::take(&mut self.buf);
        parse_record(&raw)
    }
}

fn parse_record(raw: &str) -> Option<SseEvent> {
    let mut event = None;
    let mut data: Vec<&str> = Vec::new();

    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        // Comment / keepalive.
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        // A single optional space after the colon is part of the framing.
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => event = Some(value.to_string()),
            "data" => data.push(value),
            _ => {}
        }
    }

    if data.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_single_event() {
        let mut d = SseDecoder::new();
        let evs = d.push("event: delta\ndata: {\"a\":1}\n\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].event.as_deref(), Some("delta"));
        assert_eq!(evs[0].data, "{\"a\":1}");
    }

    #[test]
    fn decodes_multiple_events_in_one_chunk() {
        let mut d = SseDecoder::new();
        let evs = d.push("data: one\n\ndata: two\n\n");
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].data, "one");
        assert_eq!(evs[1].data, "two");
    }

    #[test]
    fn handles_split_across_chunks() {
        let mut d = SseDecoder::new();
        assert!(d.push("event: mess").is_empty());
        assert!(d.push("age\ndata: hi").is_empty());
        let evs = d.push("\n\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].event.as_deref(), Some("message"));
        assert_eq!(evs[0].data, "hi");
    }

    #[test]
    fn joins_multiline_data() {
        let mut d = SseDecoder::new();
        let evs = d.push("data: line1\ndata: line2\n\n");
        assert_eq!(evs[0].data, "line1\nline2");
    }

    #[test]
    fn ignores_keepalives_and_comments() {
        let mut d = SseDecoder::new();
        let evs = d.push(": keepalive\n\ndata: real\n\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].data, "real");
    }

    #[test]
    fn strips_carriage_returns() {
        let mut d = SseDecoder::new();
        let evs = d.push("data: x\r\n\r\n");
        assert_eq!(evs[0].data, "x");
    }

    #[test]
    fn finish_flushes_unterminated_tail() {
        let mut d = SseDecoder::new();
        assert!(d.push("data: tail\n").is_empty());
        let ev = d.finish().expect("tail should flush");
        assert_eq!(ev.data, "tail");
        assert!(d.finish().is_none());
    }

    #[test]
    fn tolerates_missing_space_after_colon() {
        let mut d = SseDecoder::new();
        let evs = d.push("data:{\"tight\":true}\n\n");
        assert_eq!(evs[0].data, "{\"tight\":true}");
    }
}

#[cfg(test)]
mod utf8_tests {
    use super::*;
    #[test]
    fn every_utf8_split_preserves_streamed_text() {
        let wire = "data: {\"text\":\"héllo 世界 🦀\"}\n\n";
        for split in 0..wire.len() {
            let mut d = SseDecoder::new();
            let mut events = d.push_bytes(&wire.as_bytes()[..split]);
            events.extend(d.push_bytes(&wire.as_bytes()[split..]));
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].data, "{\"text\":\"héllo 世界 🦀\"}");
        }
    }
}
