//! Request diagnostics: bounded metadata, one write per request, no prompt copies.
use crate::{
    provider::{Event, Request},
    session::Session,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
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
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn message_hash(m: &glass_mod_api::Message) -> String {
    use glass_mod_api::Message;
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    let (tag, fields): (&str, Vec<&str>) = match m {
        Message::User(t) => ("user", vec![t]),
        Message::Assistant(t) => ("assistant", vec![t]),
        Message::System(t) => ("system", vec![t]),
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
    };
    digest.update(tag.as_bytes());
    for field in fields {
        digest.update(&(field.len() as u64).to_le_bytes());
        digest.update(field.as_bytes());
    }
    digest
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl Snapshot {
    pub fn capture(req: &Request<'_>) -> Self {
        let system = hash(req.system.as_bytes());
        let tools = hash(&serde_json::to_vec(req.tools).unwrap_or_default());
        // A fingerprint, not a prediction of a provider's cache key.
        let prefix = hash(format!("{}:{system}:{tools}", req.model).as_bytes());
        let messages = req.messages.iter().map(message_hash).collect();
        let mut sources = std::collections::BTreeMap::new();
        for tag in ["project_instructions", "attached_file", "project_memory"] {
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
                .min(u32::MAX as usize) as u32,
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
    path: PathBuf,
    background: bool,
    rate: Option<crate::pricing::RateCard>,
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
        Self::at(&session.path, req, false)
    }
    pub fn background(path: &Path, req: &Request<'_>) -> Self {
        Self::at(path, req, true)
    }
    fn at(path: &Path, req: &Request<'_>, background: bool) -> Self {
        let snapshot = Snapshot::capture(req);
        let previous = bounded(&path.with_extension("request-last.json"), 256 * 1024)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok());
        let delta = snapshot.delta(previous.as_ref());
        Self {
            path: path.to_owned(),
            background,
            rate: None,
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
    }
    pub fn pricing(&mut self, rate: Option<crate::pricing::RateCard>) {
        self.rate = rate;
    }
    pub fn retry(&mut self) {
        self.retries += 1;
    }
    pub fn event(&mut self, event: &Event) {
        self.first_event
            .get_or_insert_with(|| self.started.elapsed().as_millis());
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
    }
    pub fn outcome(&mut self, outcome: &str) {
        self.outcome = outcome.into();
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        let usage=self.usage.map(|u|serde_json::json!({"input":u.input,"cache_read":u.cache_read,"cache_write":u.cache_write,"output":u.output}));
        let report = serde_json::json!({"kind":if self.background{"cache_warmer"}else{"agent"},"cost_usd":self.rate.zip(self.usage).map(|(rate,usage)|rate.cost(&usage)),"model_requested":self.snapshot.model,"elapsed_ms":self.started.elapsed().as_millis(),"first_event_ms":self.first_event,
            "text_bytes":self.text_bytes,"reasoning_bytes":self.reasoning_bytes,"tool_calls":self.calls,"transport_retries":self.retries,"outcome":self.outcome,
            "usage":usage,"context_delta":self.delta,"prefix_fingerprint":self.snapshot.prefix,
            "cache_note":"Fingerprint describes the requested model/system/tools, not the provider cache key. Zero/absent usage cannot establish why caching missed. Provider response identity and finish reason are in /context when exposed. Cache warmer requests have kind=cache_warmer and consume provider usage; unknown pricing is not zero cost."});
        let result = (|| -> Result<(), String> {
            append(&self.path.with_extension("requests.jsonl"), &report)?;
            if self.background {
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
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
        let mut partial = String::new();
        reader.read_line(&mut partial).map_err(|e| e.to_string())?;
    }
    let mut rows = reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect::<Vec<_>>();
    if rows.len() > 100 {
        rows.drain(..rows.len() - 100);
    }
    Ok(rows)
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
    let (mut input, mut output, mut read, mut write) = (0u64,0u64,0u64,0u64);
    let (mut known, mut unknown, mut priced) = (0usize,0usize,0usize);
    let mut cost = 0.0;
    let mut failures = 0;
    let mut invalidations = 0;
    for row in &rows {
        if let Some(usage) = row["usage"].as_object() {
            known += 1;
            input = input.saturating_add(usage.get("input").and_then(serde_json::Value::as_u64).unwrap_or(0));
            output = output.saturating_add(usage.get("output").and_then(serde_json::Value::as_u64).unwrap_or(0));
            read = read.saturating_add(usage.get("cache_read").and_then(serde_json::Value::as_u64).unwrap_or(0));
            write = write.saturating_add(usage.get("cache_write").and_then(serde_json::Value::as_u64).unwrap_or(0));
        } else { unknown += 1; }
        if let Some(value) = row["cost_usd"].as_f64() { cost += value; priced += 1; }
        if row["outcome"].as_str().is_some_and(|s| s != "completed") { failures += 1; }
        if row["context_delta"]["prefix_changed"] == true { invalidations += 1; }
    }
    let prompt = input.saturating_add(read).saturating_add(write);
    let teammates = crate::team::get(&session.id).map(|hub| hub.snapshot().iter().map(|info|serde_json::json!({"name":info.name,"status":info.status,"input":info.input,"output":info.output,"usage_known":info.usage_known,"estimated_tokens":info.estimated_tokens,"cost_usd":info.cost_usd,"cost_complete":info.cost_complete})).collect::<Vec<_>>()).unwrap_or_default();
    Ok(serde_json::json!({"session":session.id,"scope":"Recent request diagnostics only; includes cache warmer and correction requests when present. Not a lifetime session total.","requests":rows.len(),"requests_with_usage":known,"requests_without_usage":unknown,
        "reported_uncached_input":input,"reported_cache_read":read,"reported_cache_write":write,
        "reported_prompt_total":prompt,"reported_output":output,
        "reported_cache_hit_ratio":if prompt>0 {Some(read as f64/prompt as f64)} else {None},
        "known_cost_subtotal_usd":if priced>0 {Some(cost)} else {None},"cost_complete":priced==rows.len() && !rows.is_empty(),
        "non_completed_requests":failures,"observed_prefix_changes":invalidations,
        "fs_scan":crate::fs_scan::diagnostics(),
        "quota":null,"quota_note":"No authoritative provider quota is available from request usage.","teammates":teammates,
        "cache_note":"Prefix changes explain local invalidation boundaries, not a provider cache-miss cause. Missing usage is never assumed to be zero."}))
}

#[cfg(test)]
mod tests {
    use super::*;
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
