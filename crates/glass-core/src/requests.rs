//! Request diagnostics: bounded metadata, one write per request, no prompt copies.
use crate::{
    provider::{Event, Request},
    session::Session,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_TRACE_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_trace_id() -> u64 {
    NEXT_TRACE_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn diagnostic_phase(
    trace_id: u64,
    workflow: &str,
    phase: &str,
    elapsed_us: Option<u128>,
    runtime_turn_hash: Option<&str>,
) {
    let Some(trace_path) = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE") else {
        return;
    };
    let trace_path = PathBuf::from(trace_path).with_extension("provider.jsonl");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_path)
    else {
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = file.metadata() {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = file.set_permissions(permissions);
        }
    }
    let record = serde_json::json!({
        "component": "core_provider",
        "phase": phase,
        "trace_id": trace_id,
        "elapsed_us": elapsed_us,
        "workflow": workflow
    });
    let mut record = record;
    if let Some(runtime_turn_hash) = runtime_turn_hash {
        record["runtime_turn_hash"] = serde_json::json!(runtime_turn_hash);
    }
    let _ = writeln!(file, "{record}");
}
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    model: String,
    system: String,
    tools: String,
    prefix: String,
    messages: Vec<String>,
    #[serde(default)]
    sources: std::collections::BTreeMap<String, String>,
    estimated_tokens: u32,
}
fn hash(bytes: &[u8]) -> String {
    crate::hashing::sha256(bytes)
}
fn message_hash(m: &glass_mod_api::Message) -> String {
    use glass_mod_api::Message;
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let (tag, fields, count): (&str, [&str; 3], usize) = match m {
        Message::User(t) => ("user", [t, "", ""], 1),
        Message::Assistant(t) => ("assistant", [t, "", ""], 1),
        Message::ChatReasoning {
            content,
            assistant_hash,
            ..
        } => (
            "chat_reasoning",
            [
                content.as_deref().unwrap_or(""),
                assistant_hash.as_deref().unwrap_or(""),
                "",
            ],
            2,
        ),
        Message::System(t) => ("system", [t, "", ""], 1),
        Message::ToolCall {
            id,
            name,
            arguments,
        } => ("call", [id, name, arguments], 3),
        Message::ToolResult {
            call_id,
            name,
            body,
        } => ("result", [call_id, name, body], 3),
        Message::Image { media_type, data } => ("image", [media_type, data, ""], 2),
    };
    digest.update(tag.as_bytes());
    for field in fields.into_iter().take(count) {
        digest.update(&(field.len() as u64).to_le_bytes());
        digest.update(field.as_bytes());
    }
    if let Message::ChatReasoning { tool_call_ids, .. } = m {
        for id in tool_call_ids {
            digest.update(&(id.len() as u64).to_le_bytes());
            digest.update(id.as_bytes());
        }
    }
    crate::hashing::hex(digest.finish().as_ref())
}
impl Snapshot {
    pub fn capture(req: &Request<'_>) -> Self {
        let system = hash(req.system.as_bytes());
        let tools = hash(&serde_json::to_vec(req.tools).unwrap_or_default());
        // A fingerprint, not a prediction of a provider's cache key.
        let prefix = hash(format!("{}:{system}:{tools}", req.model).as_bytes());
        let messages = req.messages.iter().map(message_hash).collect();
        let mut sources = std::collections::BTreeMap::new();
        for tag in [
            "project_instructions",
            "attached_file",
            "project_memory",
            "neuralnets",
        ] {
            for rest in req.system.split(&format!("<{tag} path=")).skip(1).take(128) {
                if let Some((path, body)) = rest.split_once(">\n")
                    && let Some((body, _)) = body.split_once(&format!("</{tag}>"))
                {
                    sources.insert(
                        format!(
                            "{tag}:{}",
                            glass_tui::text::clip(&glass_tui::markdown::sanitize(path), 512)
                        ),
                        hash(body.as_bytes()),
                    );
                }
            }
        }
        Self {
            model: req.model.into(),
            system,
            tools,
            prefix,
            messages,
            sources,
            estimated_tokens: crate::provider::default_estimate(req.messages).saturating_add(
                (req.system.len()
                    + req
                        .tools
                        .iter()
                        .map(|(n, d, s)| n.len() + d.len() + s.len())
                        .sum::<usize>())
                .div_ceil(4)
                .try_into()
                .unwrap_or(u32::MAX),
            ),
        }
    }
    fn delta(&self, before: Option<&Self>) -> serde_json::Value {
        let Some(before) = before else {
            return serde_json::json!({"baseline":true,"message_count":self.messages.len(),"estimated_tokens":self.estimated_tokens});
        };
        let unchanged = self
            .messages
            .iter()
            .zip(&before.messages)
            .take_while(|(a, b)| a == b)
            .count();
        let changed_sources = self
            .sources
            .iter()
            .filter(|(key, value)| before.sources.get(*key) != Some(*value))
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let removed_sources = before
            .sources
            .keys()
            .filter(|key| !self.sources.contains_key(*key))
            .collect::<Vec<_>>();
        serde_json::json!({"baseline":false,"added_or_changed_sources":changed_sources,"removed_sources":removed_sources,"model_changed":self.model!=before.model,"system_changed":self.system!=before.system,
            "tools_changed":self.tools!=before.tools,"prefix_changed":self.prefix!=before.prefix,"unchanged_message_prefix":unchanged,
            "new_or_changed_messages":self.messages.len()-unchanged,"previous_tail_messages":before.messages.len()-unchanged,
            "estimated_token_delta":i64::from(self.estimated_tokens)-i64::from(before.estimated_tokens)})
    }
}
pub struct Trace {
    trace_id: u64,
    runtime_turn_hash: Option<String>,
    path: PathBuf,
    background: bool,
    workflow: &'static str,
    raw_record: Option<usize>,
    rate: Option<crate::pricing::RateCard>,
    serving_model: Option<String>,
    started: Instant,
    first_event: Option<u128>,
    text_bytes: usize,
    reasoning_bytes: usize,
    calls: usize,
    retries: usize,
    usage: Option<crate::provider::Usage>,
    outcome: String,
    snapshot: Snapshot,
    delta: serde_json::Value,
}
impl Trace {
    pub fn begin(session: &Session, req: &Request<'_>) -> Self {
        Self::at(&session.path, req, false, "agent", next_trace_id(), None)
    }
    pub(crate) fn begin_correlated(
        session: &Session,
        req: &Request<'_>,
        trace_id: u64,
        runtime_turn_hash: Option<String>,
    ) -> Self {
        Self::at(
            &session.path,
            req,
            false,
            "agent",
            trace_id,
            runtime_turn_hash,
        )
    }
    pub fn compaction(session: &Session, req: &Request<'_>) -> Self {
        let mut trace = Self::at(
            &session.path,
            req,
            false,
            "compaction",
            next_trace_id(),
            None,
        );
        trace.raw_record = Some(session.log_len());
        trace
    }
    pub fn background(path: &Path, req: &Request<'_>) -> Self {
        Self::at(path, req, true, "cache_warmer", next_trace_id(), None)
    }
    fn at(
        path: &Path,
        req: &Request<'_>,
        background: bool,
        workflow: &'static str,
        trace_id: u64,
        runtime_turn_hash: Option<String>,
    ) -> Self {
        let snapshot = Snapshot::capture(req);
        let previous = bounded(&path.with_extension("request-last.json"), 256 * 1024)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok());
        let delta = snapshot.delta(previous.as_ref());
        Self {
            trace_id,
            runtime_turn_hash,
            path: path.to_owned(),
            background,
            workflow,
            raw_record: None,
            rate: None,
            serving_model: None,
            started: Instant::now(),
            first_event: None,
            text_bytes: 0,
            reasoning_bytes: 0,
            calls: 0,
            retries: 0,
            usage: None,
            outcome: "interrupted or request setup failed".into(),
            snapshot,
            delta,
        }
        .with_diagnostic_start()
    }
    fn with_diagnostic_start(self) -> Self {
        diagnostic_phase(
            self.trace_id,
            self.workflow,
            "provider_request_begin",
            None,
            self.runtime_turn_hash.as_deref(),
        );
        self
    }
    pub fn trace_id(&self) -> u64 {
        self.trace_id
    }
    pub fn pricing(&mut self, rate: Option<crate::pricing::RateCard>) {
        self.rate = rate;
    }
    pub fn serving(&mut self, model: &str, rate: Option<crate::pricing::RateCard>) {
        self.serving_model = Some(model.to_owned());
        self.rate = rate;
    }
    pub fn retry(&mut self) {
        self.retries += 1;
    }
    pub fn event(&mut self, event: &Event) {
        self.stream_event_received(event);
        match event {
            Event::Text(t) => self.text_bytes += t.len(),
            Event::Reasoning(t) => self.reasoning_bytes += t.len(),
            Event::ToolCall { .. } => self.calls += 1,
            Event::Usage(u) => self.usage = Some(*u),
            Event::Done if self.outcome == "interrupted or request setup failed" => {
                self.outcome = "completed".into()
            }
            Event::Error(e) => self.outcome = crate::live::sanitize_provider_detail(e),
            _ => {}
        }
        if matches!(event, Event::Done) {
            diagnostic_phase(
                self.trace_id,
                self.workflow,
                "provider_stream_complete",
                Some(self.started.elapsed().as_micros()),
                self.runtime_turn_hash.as_deref(),
            );
        }
    }
    pub fn stream_event_received(&mut self, _event: &Event) {
        if self.first_event.is_none() {
            self.first_event = Some(self.started.elapsed().as_millis());
            diagnostic_phase(
                self.trace_id,
                self.workflow,
                "provider_first_stream_event",
                Some(self.started.elapsed().as_micros()),
                self.runtime_turn_hash.as_deref(),
            );
        }
    }
    pub fn outcome(&mut self, outcome: &str) {
        self.outcome = outcome.into();
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        diagnostic_phase(
            self.trace_id,
            self.workflow,
            "provider_request_complete",
            Some(self.started.elapsed().as_micros()),
            self.runtime_turn_hash.as_deref(),
        );
        let usage=self.usage.map(|u|serde_json::json!({"input":u.input,"cache_read":u.cache_read,"cache_write":u.cache_write,"output":u.output}));
        let report = serde_json::json!({"kind":self.workflow,"raw_record_at_request":self.raw_record,"cost_usd":self.rate.zip(self.usage).map(|(rate,usage)|rate.cost(&usage)),"model_requested":self.snapshot.model,"model_served":self.serving_model,"elapsed_ms":self.started.elapsed().as_millis(),"first_event_ms":self.first_event,
            "text_bytes":self.text_bytes,"reasoning_bytes":self.reasoning_bytes,"tool_calls":self.calls,"transport_retries":self.retries,"outcome":self.outcome,
            "usage":usage,"context_delta":self.delta,"prefix_fingerprint":self.snapshot.prefix,
            "cache_note":"Prefix fingerprint is not a provider cache key. Missing usage cannot establish cache behavior."});
        let result = (|| -> Result<(), String> {
            append(&self.path.with_extension("requests.jsonl"), &report)?;
            if self.background || self.workflow == "compaction" {
                return Ok(());
            }
            crate::auth::secure_write(
                &self.path.with_extension("request-last.json"),
                &serde_json::to_vec(&self.snapshot).map_err(|e| e.to_string())?,
            )
        })();
        if result.is_err() {
            eprintln!("request diagnostics could not be saved; session output is unaffected");
        }
    }
}
pub fn bounded(path: &Path, cap: usize) -> Result<Vec<u8>, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose a regular file".into());
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > cap {
        return Err(format!("File exceeds {cap} byte limit"));
    }
    Ok(bytes)
}
fn append(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Diagnostics path is not a regular file".into());
    }
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    file.write_all(&bytes).map_err(|e| e.to_string())
}
pub fn reports(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    tail(&path.with_extension("requests.jsonl"))
}
pub fn tail(path: &Path) -> Result<Vec<serde_json::Value>, String> {
    tail_window(path).map(|(rows, _)| rows)
}

pub fn tail_window(path: &Path) -> Result<(Vec<serde_json::Value>, bool), String> {
    // Tail-only reads keep long sessions cheap; a partial first row is skipped.
    use std::io::{Seek, SeekFrom};
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = match options.open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), true)),
        Err(e) => return Err(e.to_string()),
    };
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Invalid diagnostics file".into());
    }
    let offset = metadata.len().saturating_sub(256 * 1024);
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(file.take(256 * 1024));
    if offset > 0 {
        let mut partial = Vec::new();
        reader
            .read_until(b'\n', &mut partial)
            .map_err(|e| e.to_string())?;
    }
    let mut rows = reader
        .split(b'\n')
        .map_while(Result::ok)
        .filter_map(|s| serde_json::from_slice(&s).ok())
        .collect::<Vec<_>>();
    let complete = offset == 0 && rows.len() <= 100;
    if rows.len() > 100 {
        rows.drain(..rows.len() - 100);
    }
    Ok((rows, complete))
}
pub fn view(keys: &crate::input::KeyReader, session: &Session) {
    match reports(&session.path) {
        Ok(rows) => {
            let labels = rows
                .iter()
                .enumerate()
                .rev()
                .map(|(i, v)| {
                    format!(
                        "Recent item {} · {} · {} ms · {}",
                        i + 1,
                        v["model_requested"].as_str().unwrap_or("?"),
                        v["elapsed_ms"],
                        v["outcome"].as_str().unwrap_or("?")
                    )
                })
                .collect::<Vec<_>>();
            if labels.is_empty() {
                crate::modal::info(keys, "Requests", "No request diagnostics recorded yet.");
                return;
            }
            while let Some(i) = crate::modal::choose(
                keys,
                "Requests",
                "Recent 100 requests · metadata only · Esc back",
                &labels,
            ) {
                crate::structured::view(
                    keys,
                    "Request · context and cache",
                    &serde_json::to_string_pretty(&rows[rows.len() - 1 - i]).unwrap_or_default(),
                );
            }
        }
        Err(e) => crate::modal::info(keys, "Requests", &e),
    }
}

/// Aggregate only the bounded recent request window; absent usage is unknown.
pub fn usage_summary(session: &Session) -> Result<serde_json::Value, String> {
    let rows = reports(&session.path)?;
    let (mut input, mut output, mut read, mut write) = (0u64, 0u64, 0u64, 0u64);
    let (mut known, mut unknown, mut priced) = (0usize, 0usize, 0usize);
    let mut cost = 0.0;
    let mut failures = 0;
    let mut invalidations = 0;
    for row in &rows {
        if let Some(usage) = row["usage"].as_object() {
            known += 1;
            input = input.saturating_add(
                usage
                    .get("input")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            );
            output = output.saturating_add(
                usage
                    .get("output")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            );
            read = read.saturating_add(
                usage
                    .get("cache_read")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            );
            write = write.saturating_add(
                usage
                    .get("cache_write")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0),
            );
        } else {
            unknown += 1;
        }
        if let Some(value) = row["cost_usd"].as_f64() {
            cost += value;
            priced += 1;
        }
        if row["outcome"].as_str().is_some_and(|s| s != "completed") {
            failures += 1;
        }
        if row["context_delta"]["prefix_changed"] == true {
            invalidations += 1;
        }
    }
    let prompt = input.saturating_add(read).saturating_add(write);
    let teammates = crate::team::get(&session.id).map(|hub| hub.snapshot().iter().map(|info|serde_json::json!({"name":info.name,"status":info.status,"input":info.input,"output":info.output,"usage_known":info.usage_known,"estimated_tokens":info.estimated_tokens,"cost_usd":info.cost_usd,"cost_complete":info.cost_complete})).collect::<Vec<_>>()).unwrap_or_default();
    Ok(
        serde_json::json!({"session":session.id,"scope":"Recent request diagnostics only; includes cache warmer and correction requests when present. Not a lifetime session total.","requests":rows.len(),"requests_with_usage":known,"requests_without_usage":unknown,
        "reported_uncached_input":input,"reported_cache_read":read,"reported_cache_write":write,
        "reported_prompt_total":prompt,"reported_output":output,
        "reported_cache_hit_ratio":if prompt>0 {Some(read as f64/prompt as f64)} else {None},
        "known_cost_subtotal_usd":if priced>0 {Some(cost)} else {None},"cost_complete":priced==rows.len() && !rows.is_empty(),
        "non_completed_requests":failures,"observed_prefix_changes":invalidations,
        "fs_scan":crate::fs_scan::diagnostics(),
        "quota":null,"quota_note":"No authoritative provider quota is available from request usage.","teammates":teammates,
        "cache_note":"Prefix changes explain local invalidation boundaries, not a provider cache-miss cause. Missing usage is never assumed to be zero."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn allocation_changes_preserve_legacy_message_fingerprints() {
        use glass_mod_api::Message;
        let samples = [
            Message::User("界".into()),
            Message::Assistant("answer".into()),
            Message::System("instructions".into()),
            Message::ToolCall {
                id: "".into(),
                name: "read".into(),
                arguments: "{}".into(),
            },
            Message::ToolResult {
                call_id: "1".into(),
                name: "read".into(),
                body: "".into(),
            },
            Message::Image {
                media_type: "image/png".into(),
                data: "YQ==".into(),
            },
        ];
        for message in samples {
            let (tag, fields): (&str, Vec<&str>) = match &message {
                Message::User(text) => ("user", vec![text]),
                Message::Assistant(text) => ("assistant", vec![text]),
                Message::System(text) => ("system", vec![text]),
                Message::ToolCall {
                    id,
                    name,
                    arguments,
                } => ("call", vec![id, name, arguments]),
                Message::ToolResult {
                    call_id,
                    name,
                    body,
                } => ("result", vec![call_id, name, body]),
                Message::Image { media_type, data } => ("image", vec![media_type, data]),
                Message::ChatReasoning {
                    content,
                    assistant_hash,
                    ..
                } => (
                    "chat_reasoning",
                    vec![
                        content.as_deref().unwrap_or(""),
                        assistant_hash.as_deref().unwrap_or(""),
                    ],
                ),
            };
            let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
            digest.update(tag.as_bytes());
            for field in fields {
                digest.update(&(field.len() as u64).to_le_bytes());
                digest.update(field.as_bytes());
            }
            let previous: String = digest
                .finish()
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(message_hash(&message), previous);
        }
    }

    #[test]
    fn byte_tail_skips_partial_unicode_and_reports_byte_truncation() {
        let path =
            std::env::temp_dir().join(format!("gc-tail-unicode-{}.jsonl", std::process::id()));
        let mut bytes =
            serde_json::to_vec(&serde_json::json!({"long":"€".repeat(100_000)})).unwrap();
        bytes.extend_from_slice(b"\n{\"last\":true}\n");
        std::fs::write(&path, &bytes).unwrap();
        let (rows, complete) = tail_window(&path).unwrap();
        assert!(!complete);
        assert_eq!(rows, vec![serde_json::json!({"last":true})]);
        std::fs::write(&path, b"{\"first\":true}\n\xff\n{\"last\":true}\n").unwrap();
        assert_eq!(
            tail(&path).unwrap().len(),
            2,
            "corrupt UTF-8 must not hide later rows"
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn delta_tracks_changes_without_contents() {
        let messages = vec![glass_mod_api::Message::User("PRIVATE_MESSAGE".into())];
        let tools = vec![];
        let req = Request {
            model: "mock",
            system: "PRIVATE_SYSTEM",
            messages: &messages,
            tools: &tools,
            max_tokens: 10,
            reasoning: None,
        };
        let a = Snapshot::capture(&req);
        let mut b = a.clone();
        b.messages.push(hash(b"new"));
        let delta = b.delta(Some(&a));
        assert_eq!(delta["new_or_changed_messages"], 1);
        assert_eq!(delta["prefix_changed"], false);
        assert!(!serde_json::to_string(&a).unwrap().contains("PRIVATE"));
        b.model = "other".into();
        assert_eq!(b.delta(Some(&a))["model_changed"], true);
    }
}
