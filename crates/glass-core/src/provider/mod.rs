//! Provider abstraction: one trait, two wire formats.
//!
//! The trait is deliberately tiny. Everything a provider needs beyond these
//! methods arrives in [`Request`]. A new provider is a base URL and a model
//! id, not a new implementation.

mod anthropic;
mod cancellable;
mod chain;
pub(crate) use cancellable::scoped as with_request_cancel;
mod openai;
mod pacing;
mod pacing_shared;
pub(crate) use pacing::{rate_limited, stream_status, with_notice as with_pacing_notice};

pub use anthropic::Anthropic;
pub use chain::Chain;
pub use openai::{Flavour, OpenAiCompat};

use std::io::{self, Read};

use glass_mod_api::Message;

/// Token accounting for one request.
///
/// Providers disagree about what "input tokens" means, so the parts are kept
/// apart and summed only for display. Anthropic charges `cache_read` at 0.1x,
/// `cache_write` at 1.25x, and `input` at 1.0x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    /// Prompt tokens that were neither read from nor written to cache.
    pub input: u32,
    /// Prompt tokens served from an existing cache entry.
    pub cache_read: u32,
    /// Prompt tokens written to cache this request.
    pub cache_write: u32,
    pub output: u32,
    /// Presence is tracked independently so a priced cache bucket omitted by
    /// the provider cannot be mistaken for zero usage.
    pub input_known: bool,
    pub cache_read_known: bool,
    pub cache_write_known: bool,
    pub output_known: bool,
}

impl Usage {
    /// Whether all buckets with a nonzero rate have authoritative counts.
    pub fn priced_complete(&self, card: crate::pricing::RateCard) -> bool {
        (card.input == 0.0 || self.input_known)
            && (card.cache_read == 0.0 || self.cache_read_known)
            && (card.cache_write == 0.0 || self.cache_write_known)
            && (card.output == 0.0 || self.output_known)
    }

    /// Total prompt tokens actually sent, however they were billed.
    pub fn prompt_total(&self) -> u32 {
        self.input
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write)
    }
}

/// Provider APIs expose token counts as unsigned 64-bit integers while the
/// public usage event keeps its historical 32-bit fields. Clamp oversized
/// counts so malformed or unusually large provider values cannot wrap into a
/// misleading small number.
pub(super) fn bounded_usage_tokens(tokens: Option<u64>) -> u32 {
    tokens
        .map(|tokens| u32::try_from(tokens).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

/// Provider streams number content/tool blocks with `u64` indices. Preserve
/// the historical default for a missing index, but reject values that cannot
/// be represented by this target instead of wrapping them onto another slot.
pub(super) fn checked_stream_index(index: Option<u64>) -> Option<usize> {
    index.map_or(Some(0), |index| usize::try_from(index).ok())
}

#[cfg(test)]
mod usage_tests {
    use super::{Usage, checked_stream_index, estimate_token_count};

    #[test]
    fn prompt_total_saturates_at_public_counter_limit() {
        let usage = Usage {
            input: u32::MAX,
            cache_read: 1,
            cache_write: u32::MAX,
            output: 0,
            input_known: true,
            cache_read_known: true,
            cache_write_known: true,
            output_known: true,
        };
        assert_eq!(usage.prompt_total(), u32::MAX);
    }

    #[test]
    fn provider_stream_indices_do_not_wrap_on_narrow_targets() {
        assert_eq!(checked_stream_index(None), Some(0));
        assert_eq!(
            checked_stream_index(Some(u64::MAX)),
            usize::try_from(u64::MAX).ok()
        );
    }

    #[test]
    fn token_estimates_saturate_at_public_counter_limit() {
        assert_eq!(estimate_token_count(usize::MAX), u32::MAX);
    }
}

/// A piece of a streamed model response.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Visible assistant text.
    Text(String),
    /// An opt-in transition to another structurally identified text block.
    TextBlockStart(String),
    /// Reasoning content, when the provider exposes it.
    Reasoning(String),
    /// Exact plaintext Chat Completions reasoning delta, separate from public
    /// summaries returned by Responses or another provider's thinking format.
    ChatReasoning(String),
    /// Successful Chat Completions response boundary, even without thinking.
    ChatResponse,
    /// The model wants a tool run.
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    /// Token counts reported by the provider, when available.
    ///
    /// `input` is the *uncached* portion. On Anthropic that is only the
    /// post-breakpoint remainder, so total prompt tokens is
    /// `cache_read + cache_write + input`. Getting this wrong makes every cost
    /// estimate wrong, which is why the fields are separate rather than
    /// pre-summed.
    Usage(Usage),
    /// Terminal event for this response.
    Done,
    /// Non-fatal problem worth surfacing.
    Warning(String),
    /// Fatal stream failure; partial tool calls must not execute.
    Error(String),
}

/// Provider diagnostics can echo a credential; never persist or render it.
pub fn redact_event(event: Event, key: Option<&str>) -> Event {
    let Some(key) = key.filter(|s| !s.is_empty()) else {
        return event;
    };
    match event {
        Event::Error(s) => Event::Error(s.replace(key, "[redacted]")),
        Event::Warning(s) => Event::Warning(s.replace(key, "[redacted]")),
        other => other,
    }
}

/// Everything needed to produce one model turn.
pub struct Request<'a> {
    /// Model id, for adapters whose endpoint carries it per request rather
    /// than in configuration.
    #[allow(dead_code)]
    pub model: &'a str,
    pub system: &'a str,
    pub messages: &'a [Message],
    /// `(name, description, json_schema)` triples.
    pub tools: &'a [(String, String, String)],
    /// Upper bound on generated tokens.
    pub max_tokens: u32,
    /// Provider-native reasoning budget, when the adapter supports one.
    pub reasoning: Option<u32>,
}

pub trait Provider {
    fn set_session_identity(&mut self, _identity: Option<&str>) {}
    /// Exact billing card supplied by the provider wrapper, when available.
    fn rate_card(&self) -> Option<crate::pricing::RateCard> {
        None
    }

    /// Stable identifier used in config and the status bar.
    fn id(&self) -> &str;

    /// Configured request model, distinct from the transport/provider identifier.
    fn model(&self) -> &str {
        self.id()
    }

    /// Stream one turn. Implementations must emit `Done` before returning.
    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error>;

    fn stream_text_blocks(
        &self,
        req: &Request<'_>,
        _enabled: bool,
    ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        self.stream(req)
    }

    /// Context window in tokens, used to decide when to compact.
    fn context_window(&self) -> u32;

    /// Approximate prompt tokens. Providers with a real count endpoint should
    /// override this; the default is a deliberately conservative estimate.
    fn estimate_tokens(&self, messages: &[Message]) -> u32 {
        default_estimate(messages)
    }
}

/// ~3.6 bytes per token is the usual English-with-code ratio. We round up and
/// add headroom, because under-counting triggers a context overflow while
/// over-counting only compacts slightly early.
pub fn default_estimate(messages: &[Message]) -> u32 {
    default_estimate_for(messages, true)
}
pub fn default_estimate_for(messages: &[Message], chat_reasoning: bool) -> u32 {
    let mut chars: usize = 0;
    for m in messages {
        if matches!(m,Message::ChatReasoning{content,..} if !chat_reasoning || content.is_none()) {
            continue;
        }
        if matches!(m, Message::Image { .. }) {
            // Images are billed by pixels, not bytes; a flat allowance keeps
            // a base64 data URI from reading as millions of phantom tokens.
            chars = chars.saturating_add(8_000);
            continue;
        }
        chars = chars.saturating_add(m.text().len());
        chars = chars.saturating_add(8); // role and framing overhead per message
    }
    estimate_token_count(chars)
}

fn estimate_token_count(chars: usize) -> u32 {
    let tokens = chars.div_ceil(4);
    let estimate = u128::try_from(tokens)
        .unwrap_or(u128::MAX)
        .saturating_mul(115)
        .div_ceil(100);
    u32::try_from(estimate).unwrap_or(u32::MAX)
}

#[derive(Debug)]
pub enum Error {
    /// Transport failure, with the provider message when one was returned.
    Transport(String),
    /// Provider returned a non-success status.
    Status(u16, String),
    /// Configuration is unusable.
    Config(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Every caller (TTY, headless, ACP, serve and durable error records)
        // must see the same redacted diagnostic, not the provider's raw body.
        let safe = crate::live::sanitize_provider_detail;
        match self {
            Error::Transport(m) => write!(f, "transport: {}", safe(m)),
            Error::Status(c @ (401 | 403), m) => write!(
                f,
                "http {c}: {} · use /login to reconnect or /model to choose a permitted model",
                safe(m)
            ),
            Error::Status(c, m) => write!(f, "http {c}: {}", safe(m)),
            Error::Config(m) => write!(f, "config: {}", safe(m)),
        }
    }
}

impl std::error::Error for Error {}

/// Read a response body as UTF-8, replacing invalid sequences.
///
/// Model output can contain partial multi-byte characters at chunk edges;
/// decoding lossily keeps the stream alive instead of erroring mid-turn.
pub fn read_lossy(mut r: impl Read) -> io::Result<String> {
    let mut out = Vec::new();
    r.read_to_end(&mut out)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Build an HTTP agent with sane timeouts and no automatic error statuses,
/// so callers can read the body of a 429 or 500 for the error message.
pub fn http_agent() -> ureq::Agent {
    cancellable::agent(
        ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(600)))
            .http_status_as_error(false)
            .build(),
    )
}

/// Stream one turn, retrying eligible HTTP status responses.
///
/// Only explicit 429/5xx responses enter bounded backoff. A transport failure
/// may occur after a provider accepted the POST; without idempotency support,
/// automatically repeating it can duplicate a billable generation. Such failures
/// require an explicit retry. HTTP retries also carry no billing guarantee.
///
/// `delays` is the backoff in seconds per retry, passed in so a
/// test does not have to sit through the production delays.
/// `on_retry` is notified before each sleep, which is how the
/// turn reports that it is retrying rather than hanging.
pub fn stream_with_retry<'a>(
    provider: &'a dyn Provider,
    req: &Request<'_>,
    attempts: u8,
    delays: &[u64],
    on_retry: impl FnMut(u8, u64, &str),
) -> Result<Box<dyn Iterator<Item = Event> + 'a>, Error> {
    stream_with_retry_text_blocks(provider, req, attempts, delays, false, on_retry)
}

pub(crate) fn stream_with_retry_text_blocks<'a>(
    provider: &'a dyn Provider,
    req: &Request<'_>,
    attempts: u8,
    delays: &[u64],
    text_blocks: bool,
    mut on_retry: impl FnMut(u8, u64, &str),
) -> Result<Box<dyn Iterator<Item = Event> + 'a>, Error> {
    let mut attempt = 0u8;
    loop {
        if cancellable::is_cancelled() {
            return Err(Error::Transport("provider request cancelled".into()));
        }
        match provider.stream_text_blocks(req, text_blocks) {
            Ok(stream) => return Ok(stream),
            Err(e) if is_transient(&e) && attempt + 1 < attempts => {
                attempt += 1;
                let delay = delays.get((attempt - 1) as usize).copied().unwrap_or(0);
                on_retry(attempt, delay, &e.to_string());
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(delay);
                loop {
                    if cancellable::is_cancelled() {
                        return Err(Error::Transport("provider retry cancelled".into()));
                    }
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    std::thread::sleep(remaining.min(std::time::Duration::from_millis(25)));
                }
            }
            Err(e) => return Err(e),
        }
    }
}

#[derive(Default)]
pub(crate) struct TextBlocks {
    pub enabled: bool,
    last: Option<String>,
}

impl TextBlocks {
    fn start(&mut self, id: String) -> Option<Event> {
        if !self.enabled {
            return None;
        }
        let previous = self.last.replace(id.clone());
        previous
            .filter(|previous| previous != &id)
            .map(|_| Event::TextBlockStart(id))
    }
}

/// Preserve structured quota classifications even when the human message is vague.
fn http_error_message(body: &str) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["message"].as_str())
        })
        .map(str::to_owned)
        .unwrap_or_else(|| body.chars().take(400).collect());
    if let Some(value) = parsed {
        for object in [&value["error"], &value] {
            for field in ["code", "type"] {
                if let Some(code) = object[field]
                    .as_str()
                    .filter(|code| pacing::quota_limited(code))
                {
                    return format!("{code}: {message}");
                }
            }
        }
    }
    message
}

/// Whether policy permits automatic retries before a stream is obtained.
pub(crate) fn is_transient(e: &Error) -> bool {
    match e {
        Error::Status(code, message) => {
            (*code == 429 && !pacing::quota_limited(message)) || *code >= 500
        }
        // Transport errors do not prove that a generation was never accepted.
        Error::Transport(_) => false,
        Error::Config(_) => false,
    }
}

/// How many times one request is attempted before the turn fails.
pub const RETRY_ATTEMPTS: u8 = 5;

/// Backoff in seconds between attempts: 1s, 2s, 4s.
pub const RETRY_DELAYS: [u64; 5] = [1, 2, 4, 8, 16];

pub fn validate_endpoint(base_url: &str) -> Result<(), Error> {
    let url = url::Url::parse(base_url)
        .map_err(|_| Error::Config("provider endpoint is not a valid URL".into()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Config(
            "provider endpoint must be HTTP(S), without embedded credentials, query, or fragment"
                .into(),
        ));
    }
    Ok(())
}

/// Build the provider named in config.
///
/// The dispatch is on a string in config rather than on inferred capability,
/// so adding a provider is a config edit and never a code change.
pub fn build(
    kind: &str,
    base_url: &str,
    api_key: Option<String>,
    model: &str,
    adaptive_pacing: bool,
) -> Result<Box<dyn Provider>, Error> {
    crate::policy::provider(kind, base_url).map_err(Error::Config)?;
    validate_endpoint(base_url)?;
    match kind {
        "anthropic" => {
            let mut provider = Anthropic::new(base_url, api_key.unwrap_or_default(), model);
            provider.adaptive_pacing = adaptive_pacing;
            Ok(Box::new(provider))
        }
        "openai" | "openai-responses" => {
            let mut provider = OpenAiCompat::new(base_url, api_key, model);
            provider.adaptive_pacing = adaptive_pacing;
            if kind == "openai-responses" {
                provider = provider.with_flavour(Flavour::Responses);
            }
            Ok(Box::new(provider))
        }
        other => Err(Error::Config(format!("unknown provider `{other}`"))),
    }
}

/// Compact core schemas; file operations share the native CLI engine.
pub fn builtin_tools() -> Vec<(String, String, String)> {
    let mut tools = vec![
        (
            "bash".to_string(),
            "Search, pipelines, builds, tests, processes; prefer read/edit/write. \
             Foreground timeout_ms defaults to 120000; background timeout_ms is optional. Output: 16 KiB/stream inline; 4 MiB/stream overflow; \
             report discarded bytes and tail. Redirect to capture full logs. \
             For builds, installs and other finite slow work, call once with command and job:{action:start}. \
             Completion notification is automatic: finish your turn; the host resumes this same conversation on exit/failure/timeout. \
             DO NOT wait, sleep, loop or poll for completion. watch(id,notify) registers/disables an existing job once. \
             poll(id,offset) is only for an actionable diagnostic; input(id,text,eof), stop(id), list are also available.".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "permission":{"type":"string","enum":["use_default","require_escalated"],"description":"Request explicit human approval for this exact command outside the sandbox; do not bypass denials."},
                    "justification":{"type":"string","maxLength":2048},
                    "command": { "type": "string", "description": "Shell command to run" },
                    "cwd": { "type": "string", "description": "Working directory, relative to the workspace root" },
                    "timeout_ms": { "type": "integer", "description": "Kill the command after this many milliseconds" },
                    "job": {"type":"object","properties":{"action":{"type":"string","enum":["start","watch","poll","input","stop","list"]},"id":{"type":"integer"},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":24576},"text":{"type":"string"},"eof":{"type":"boolean"},"notify":{"type":"boolean","description":"Default true on start. Host resumes once on terminal status; false disables notification."},"task_id":{"type":"string","maxLength":128,"description":"Optional existing checklist task ID; only resumes while this task remains pending/in_progress."}},"required":["action"]}
                },
                "anyOf": [{"required":["command"]},{"required":["job"]}]
            })
            .to_string(),
        ),
        (
            "state".to_string(),
            "Durable notes and native checklist. {} reads notes; items replaces them ([] clears). \
             Save facts, paths and next steps before compaction. Multi-step work uses todo.ops: \
             init/add items(title,phase), update by ID(status,blocker,owner), remove/clear/configure. \
             todo:{} reads tasks/revision without reopening a plan. For distinct new work, init a fresh phased plan; continue existing IDs only for unfinished work on the same task. Finished plans are history. Batch real progress; completed is not verified. \
             request_id makes note writes idempotent: same ID/payload returns its receipt; \
             changed payload fails. Discover recipes with guidance and configured tools with capabilities.".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "Replace notes. Omit to read; optional expected revision guards stale updates.",
                        "items": { "type": "string" }
                    },
                    "expected":{"type":"string","description":"Optional state revision from revision:true read"},
                    "request_id":{"type":"string","description":"Idempotency key: same ID/payload reuses receipt; changed payload fails."},
                    "revision":{"type":"boolean"},
                    "todo":crate::todo::schema(),
                    "guidance":{"type":"object","additionalProperties":false,"properties":{"action":{"type":"string","enum":["search","catalogue","read"]},"query":{"type":"string"},"topic":{"type":"string"}}},
                    "neuralnets":{"type":"object","additionalProperties":false,"required":["action"],"properties":{"action":{"type":"string","enum":["list","search","read","save","update","forget","restore","purge","propose"]},"expected":{"type":"integer","minimum":0},"offset":{"type":"integer","minimum":0},"query":{"type":"string"},"id":{"type":"string"},"kind":{"type":"string","enum":["preference","fact","procedure","lesson"]},"text":{"type":"string"}}},
                    "capabilities":{"type":"object","properties":{"search":{"type":"string"},"activate":{"type":"array","items":{"type":"string"}}}}
                }
            })
            .to_string(),
        ),
        (
            "web_search".to_string(),
            "Search current web pages through Exa's hosted MCP search. Returns provider-supplied source URLs and content; no browser is launched and no extra model summary call is made. Default keyless service is rate-limited; optional EXA_API_KEY or GC web config may authenticate.".to_string(),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "query":{"type":"string","minLength":1,"maxLength":2048,"description":"Natural-language description of the pages to find"},
                    "count":{"type":"integer","minimum":1,"maximum":25,"default":8}
                },
                "required":["query"],"additionalProperties":false
            }).to_string(),
        ),
        (
            "web_fetch".to_string(),
            "Fetch one HTTP(S) page directly and return bounded readable text with final URL, status and content type. Does not launch a browser; use browser tools for visual or interactive pages.".to_string(),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "url":{"type":"string","format":"uri","description":"Absolute HTTP or HTTPS page URL"},
                    "max_chars":{"type":"integer","minimum":1,"maximum":60000,"default":20000}
                },
                "required":["url"],"additionalProperties":false
            }).to_string(),
        ),
    ];
    tools.extend(crate::files::schemas());
    tools
}

/// Optional tools for a live Connect desktop. Kept outside the default catalog
/// so standalone GC sessions pay no schema cost when Connect is not running.
pub fn desktop_tool_schemas(include_input: bool) -> Vec<(String, String, String)> {
    let mut tools = vec![
        (
            "desktop_status".to_string(),
            "Read the live status of the managed desktop shared with GC, Glass resident agents, and the human through Connect. Call this first for any task in a desktop application. Reports its sessionId, exact display, size (width, height), workspace, frame freshness, current frame timestamp (frameTimeMs), and input lease. displayServer is always \"x11\" (a private Xvfb display, never the human's own screen); hostSession is the human's session type (x11, wayland, or none). Does not start or stop a desktop. Not for web pages in the agent browser: use the browser_* tools for those.".to_string(),
            serde_json::json!({"type":"object","properties":{},"additionalProperties":false}).to_string(),
        ),
        (
            "desktop_screenshot".to_string(),
            "Capture a fresh frame from the managed session identified by the latest desktop_status.sessionId. Returns a workspace-relative screenshot path; call view_image with that path to inspect the pixels. This observation is available in read-only mode and saves only a generated artifact under `.gc-artifacts/desktop`; it does not edit project files or change the input lease. A pixel (px, py) in this image maps to desktop_input x=px/(width - 1), y=py/(height - 1) using desktop_status width and height. To read small text or check a detail, pass `region` to zoom: that part of the screen is returned at full resolution, enlarged up to 4x, and the result says how to map its pixels back to desktop_input coordinates. Example: {\"sessionId\":\"<from desktop_status>\",\"region\":{\"x\":0.5,\"y\":0,\"width\":0.5,\"height\":0.25}} zooms on the top-right quarter. For web pages use browser_screenshot.".to_string(),
            serde_json::json!({"type":"object","properties":{
                "sessionId":{"type":"string","maxLength":64},
                "region":{"type":"object","description":"Optional zoom area as fractions of the screen; at least 0.02 wide and tall.","properties":{
                    "x":{"type":"number","minimum":0,"maximum":1},
                    "y":{"type":"number","minimum":0,"maximum":1},
                    "width":{"type":"number","minimum":0.02,"maximum":1},
                    "height":{"type":"number","minimum":0.02,"maximum":1}
                },"required":["x","y","width","height"],"additionalProperties":false}
            },"required":["sessionId"],"additionalProperties":false}).to_string(),
        ),
        (
            "desktop_observe".to_string(),
            "Read a compact, bounded accessibility snapshot in the managed session identified by the latest desktop_status.sessionId. Nodes cover visible apps, windows, and controls. Password fields never return values. Use a visible `windowId` from the `windows` list or a node's `owningWindow` to scope the next query to that exact window. A windowId is a session-scoped selector, not an action token; it is revalidated on each observation and stops working when the session changes, the window closes, or its title changes. The selector may change. If navigation changes a window title, use the current selector from the latest action's returned `observation.windows`; if no fresh list is available, call `desktop_observe` without `windowId`, then use its current selector. Without windowId, focused-search terms match app names, control names, roles, supported actions, and owningWindow titles. Broad snapshots omit `onscreen` and action tokens; issue a focused query before acting. An actionable `onscreen:false` match is descriptive only and has no `targetId` or `pointerTarget`. Scroll with `desktop_input` in small increments, then repeat the same scoped query using the latest windowId; stop if the candidate disappears or stops progressing. After following a link, query the current address and page heading before choosing another target. After switching tabs, observe the tab labels and active address. Act only on a fresh `onscreen:true` result with visible and showing states, a targetId, `queryAmbiguous:false`, and `truncated:false`. A visible non-actionable label with `pointerFallback:true` has no targetId; use its fresh onscreen pointerTarget for one pointer-click only when the query is complete and unique. If desktop_act rejects a click because the control has no accessibility action, repeat the exact query in the same window scope and use only the fresh onscreen pointerTarget from a complete, unique result. Never reuse coordinates or target IDs from an earlier observation. If results are ambiguous or truncated, use an observed windowId or refine the control label; if a complete query finds no result, broaden it once or inspect a fresh screenshot rather than repeating the same search or guessing. Queries never search editable values. Focused unique results return revision-bound target IDs. The observation returned by an action is for checking the UI state and current window selectors; broad action observations do not include actionable target IDs. Run a new focused `desktop_observe` before acting again.".to_string(),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from the latest desktop_status. Rejected if the managed session changed."},
                    "query":{"type":"string","maxLength":120,"description":"Optional 1–8 word search across app/window names, control names, roles, and supported actions. Use a visible `windowId` for direct scoped search; without one, include app and distinctive owningWindow title terms. Broad snapshots have no action tokens; a focused query is required before acting. An offscreen match has onscreen=false and no action tokens; scroll and repeat the same query in the same window scope, then act only on fresh onscreen=true or pointerFallback=true results when complete and unique. Refine ambiguous or truncated results; never search editable values."},
                    "windowId":{"type":"string","maxLength":96,"description":"Optional visible window selector from the latest snapshot's windows list or owningWindow. Scopes this observation to that exact session window; stale, closed, or title-changed windows are rejected. If navigation changes the title, use the latest action observation's windows list or refresh with an unscoped desktop_observe; the selector may differ."}
                },
                "required":["sessionId"],"additionalProperties":false
            }).to_string(),
        ),
    ];
    if include_input {
        tools.push((
            "desktop_act".to_string(),
            "Act on one exact target from the latest focused desktop_observe result using its targetId, sessionId, and revision. Supports accessible click, focus, and bounded text replacement; rejects stale targets and respects the human/agent control lease. Broad observations do not issue target IDs; use a fresh focused query to find an actionable target. Act only on a fresh result with `onscreen:true`, `visible` and `showing` states, `queryAmbiguous:false`, and `truncated:false`; use set_text only when editable is true. A stale target, revision, or session means observe again and choose from the new snapshot. The action result may include a refreshed bounded observation for checking the UI state and current window selectors, but it does not contain actionable target IDs. Verify the expected UI change before continuing, then run a fresh focused desktop_observe before the next action. If desktop_act rejects a click because the target has no supported accessibility action, call desktop_observe again with the exact app/window/control query and use a one-time pointer-click only from a fresh onscreen pointerTarget in a complete, unique result. If a click is performed but unverified and the UI did not change, follow the same reobserve rule. Do not use coordinates from an offscreen result or reuse coordinates from an older observation. If the new query has no pointerTarget, use desktop_screenshot and explicit desktop_input coordinates only when needed.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["sessionId","revision","targetId","action"],
                "properties":{
                    "sessionId":{"type":"string","maxLength":64},
                    "revision":{"type":"integer","minimum":1},
                    "targetId":{"type":"string","maxLength":96},
                    "action":{"type":"string","enum":["click","focus","set_text"]},
                    "text":{"type":"string","maxLength":512}
                },
                "allOf":[{"if":{"properties":{"action":{"const":"set_text"}}},"then":{"required":["text"]}}]
            }).to_string(),
        ));
        tools.push((
            "desktop_input".to_string(),
            "Fallback for custom-drawn surfaces when desktop_observe exposes no suitable target. Sends one bounded raw input event to the managed session identified by the latest desktop_status.sessionId, and is rejected while the human holds the control lease. Prefer desktop_act on accessible targets. Use `key-tap` to press and release one key; use `key-chord` with modifiers first and one key last. The exact modifier names are `Control`, `Alt`, `Shift`, and `Meta` (use `Control`, not `Ctrl`). Special key spellings include `Enter`, `Escape`, `Tab`, `BackSpace`, `Delete`, `Home`, `End`, `PageUp`, `PageDown`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Space`, and `F1` through `F12`; one ASCII letter or digit is also accepted. Examples: Control+L is `keys:[\"Control\",\"L\"]`; Alt+F2 is `keys:[\"Alt\",\"F2\"]`; Enter is `key:\"Enter\"`; Page Down is `key:\"PageDown\"`. `key` is a held-key down/up event and requires `pressed`. For raw pointer interaction, use one `pointer-click` with button and normalized x/y instead of separate down/up events. Use `pointer-double-click` at x/y to open a file or select a word, and `pointer-drag` from x/y to toX/toY (left button held) to move a window, slider or selection in one call. Pointer x/y use normalized 0..1 coordinates. Both scroll and pointer-move require x and y; scrolling acts on the pane under that point, so use a freshly observed point inside the intended scrollable pane rather than assuming the page body. Scroll deltas use browser pixels, and text accepts up to 512 UTF-8 bytes with newline and tab allowed; other control characters are rejected. `pointer-triple-click` selects a whole line or paragraph. To enter more than a short phrase (up to 4096 UTF-8 bytes), use `paste`: it puts the text on the clipboard and presses Control+V, which is faster and safer than `text` for long input.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["action","sessionId"],
                "properties":{
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from desktop_status; the manager rejects stale sessions."},
                    "action":{"type":"string","enum":["pointer-move","pointer-button","pointer-click","pointer-double-click","pointer-triple-click","pointer-drag","key","key-tap","key-chord","text","paste","scroll"]},
                    "x":{"type":"number","minimum":0,"maximum":1,"description":"Fraction of the screen width: 0 is the left edge, 1 the right, 0.5 the center. From a desktop_screenshot pixel, x = px / (desktop_status.width - 1)."},
                    "y":{"type":"number","minimum":0,"maximum":1,"description":"Fraction of the screen height: 0 is the top edge, 1 the bottom. From a desktop_screenshot pixel, y = py / (desktop_status.height - 1)."},
                    "toX":{"type":"number","minimum":0,"maximum":1,"description":"pointer-drag only: where to drop, as a fraction of the screen width."},
                    "toY":{"type":"number","minimum":0,"maximum":1,"description":"pointer-drag only: where to drop, as a fraction of the screen height."},
                    "button":{"type":"string","enum":["left","middle","right"]},
                    "pressed":{"type":"boolean"},
                    "key":{"type":"string","maxLength":16},
                    "keys":{"type":"array","minItems":2,"maxItems":4,"items":{"type":"string","maxLength":16},"description":"Modifiers first, then one key; for example [\"Alt\",\"F2\"]."},
                    "text":{"type":"string","maxLength":4096,"description":"For `text`: at most 512 UTF-8 bytes; newline and tab are allowed, other control characters are rejected. For `paste`: at most 4096 UTF-8 bytes."},
                    "deltaX":{"type":"number","minimum":-800,"maximum":800},
                    "deltaY":{"type":"number","minimum":-800,"maximum":800}
                }
            ,"allOf":[
                {"if":{"properties":{"action":{"const":"pointer-click"}}},"then":{"required":["x","y","button"]}},
                {"if":{"properties":{"action":{"enum":["pointer-double-click","pointer-triple-click"]}}},"then":{"required":["x","y"]}},
                {"if":{"properties":{"action":{"enum":["text","paste"]}}},"then":{"required":["text"]}},
                {"if":{"properties":{"action":{"const":"pointer-drag"}}},"then":{"required":["x","y","toX","toY"]}},
                {"if":{"properties":{"action":{"enum":["pointer-move","scroll"]}}},"then":{"required":["x","y"]}},
                {"if":{"properties":{"action":{"const":"pointer-button"}}},"then":{"required":["button","pressed"]}}
                ,{"if":{"properties":{"action":{"const":"key"}}},"then":{"required":["key","pressed"]}}
                ,{"if":{"properties":{"action":{"const":"key-tap"}}},"then":{"required":["key"]}}
                ,{"if":{"properties":{"action":{"const":"key-chord"}}},"then":{"required":["keys"]}}
            ]}).to_string(),
        ));
        tools.push((
            "desktop_launch_app".to_string(),
            "List installed desktop applications with their exact desktopEntryId, then launch one by that ID in the same managed session. The ID must come from the current list and the sessionId from desktop_status. This does not accept command lines, URIs, or arguments. Launch is bounded and reports verified=false; call desktop_observe to confirm the application window opened. Launch requires the agent input lease.".to_string(),
            serde_json::json!({
                "type":"object",
                "properties":{
                    "action":{"type":"string","enum":["list","launch"]},
                    "sessionId":{"type":"string","maxLength":64},
                    "desktopEntryId":{"type":"string","maxLength":128}
                },
                "required":["action","sessionId"],
                "additionalProperties":false,
                "allOf":[{"if":{"properties":{"action":{"const":"launch"}}},"then":{"required":["desktopEntryId"]}}]
            }).to_string(),
        ));
        tools.push((
            "desktop_clipboard".to_string(),
            "Read or replace the clipboard of the managed desktop identified by the latest desktop_status.sessionId. Use it to move text between desktop applications, or to paste text that is too long or awkward to type: write it here, then send Control+V with desktop_input key-chord. Writing requires the agent input lease; reading returns at most 4096 UTF-8 bytes with `truncated` set when there was more. Not for the agent browser: use browser_clipboard for web pages. Examples: {\"action\":\"read\",\"sessionId\":\"<from desktop_status>\"}; {\"action\":\"write\",\"sessionId\":\"<from desktop_status>\",\"text\":\"Quarterly report\"}.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["action","sessionId"],
                "properties":{
                    "action":{"type":"string","enum":["read","write"]},
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from the latest desktop_status."},
                    "text":{"type":"string","maxLength":4096,"description":"Text to place on the clipboard; required for write. Maximum 4096 UTF-8 bytes, not characters. Newline and tab are allowed."}
                },
                "allOf":[{"if":{"properties":{"action":{"const":"write"}}},"then":{"required":["text"]}}]
            }).to_string(),
        ));
        tools.push((
            "desktop_windows".to_string(),
            "List the application windows of the managed desktop identified by the latest desktop_status.sessionId, with title, app and position as screen fractions, or bring one to the front (`focus`) or close it (`close`) by its id. Use it to find a window hidden behind another, to check which apps are open, or to close an app or dialog you are done with. Ids are X11 window ids such as \"0x01a00007\" from the latest list; they are not desktop_observe windowId selectors. focus and close need the agent input lease; close asks the app to close, so it may show a save prompt. Not for browser tabs: use browser_tabs. Examples: {\"action\":\"list\",\"sessionId\":\"<from desktop_status>\"}; {\"action\":\"focus\",\"sessionId\":\"<from desktop_status>\",\"id\":\"0x01a00007\"}.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["action","sessionId"],
                "properties":{
                    "action":{"type":"string","enum":["list","focus","close"]},
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from the latest desktop_status."},
                    "id":{"type":"string","maxLength":10,"pattern":"^0x[0-9a-fA-F]+$","description":"Window id from desktop_windows list; required for focus and close."}
                },
                "allOf":[{"if":{"properties":{"action":{"enum":["focus","close"]}}},"then":{"required":["id"]}}]
            }).to_string(),
        ));
        tools.push((
            "desktop_files".to_string(),
            "Move files between the workspace and the managed desktop's private Downloads folder, for the desktop identified by the latest desktop_status.sessionId. Desktop apps save downloads there, and your workspace file tools cannot reach it. `list` shows what is in Downloads; `export` copies one file (by name) into the workspace at .gc-artifacts/desktop/downloads/<name>; `import` copies a workspace file (path, relative to the workspace) into Downloads so an app's file dialog can open it. Files up to 100 MiB; existing files with the same name are replaced. Not for browser downloads in the agent browser: browser_* tools save those in the workspace. Examples: {\"action\":\"export\",\"sessionId\":\"<from desktop_status>\",\"name\":\"invoice.pdf\"}; {\"action\":\"import\",\"sessionId\":\"<from desktop_status>\",\"path\":\"data/report.csv\"}.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["action","sessionId"],
                "properties":{
                    "action":{"type":"string","enum":["list","import","export"]},
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from the latest desktop_status."},
                    "name":{"type":"string","maxLength":255,"description":"export only: a file name from desktop_files list, without slashes."},
                    "path":{"type":"string","maxLength":4096,"description":"import only: a workspace-relative file path."}
                },
                "allOf":[
                    {"if":{"properties":{"action":{"const":"export"}}},"then":{"required":["name"]}},
                    {"if":{"properties":{"action":{"const":"import"}}},"then":{"required":["path"]}}
                ]
            }).to_string(),
        ));
    }
    if include_input {
        tools.push((
            "desktop_wait".to_string(),
            "Wait until the managed desktop identified by the latest desktop_status.sessionId changes, instead of taking repeated screenshots. `until:\"change\"` returns as soon as the screen differs from when the call started (a window opened, a page loaded, a progress bar finished). `until:\"text\"` returns once a visible accessible element's name contains `text`, case-insensitive. Use it after an action whose result appears slowly. On timeout it returns an error; then take desktop_screenshot to see what happened. Not for web pages in the agent browser: use browser_wait_for. Examples: {\"until\":\"change\",\"sessionId\":\"<from desktop_status>\"}; {\"until\":\"text\",\"sessionId\":\"<from desktop_status>\",\"text\":\"Saved\",\"timeoutMs\":10000}.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["until","sessionId"],
                "properties":{
                    "until":{"type":"string","enum":["change","text"]},
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from the latest desktop_status."},
                    "text":{"type":"string","minLength":1,"maxLength":120,"description":"until=text only: words a window, dialog or control name should contain."},
                    "timeoutMs":{"type":"integer","minimum":250,"maximum":30000,"default":5000}
                },
                "allOf":[{"if":{"properties":{"until":{"const":"text"}}},"then":{"required":["text"]}}]
            }).to_string(),
        ));
        tools.push((
            "desktop_record".to_string(),
            "Record the managed desktop identified by the latest desktop_status.sessionId to an MP4 video, for when the user asks for a recording or to show how a task was done. `start` begins recording at 10 frames per second and stops by itself after maxSeconds (default 300, at most 600); `stop` finishes the file and returns its workspace path under .gc-artifacts/desktop/recordings/; `status` says whether one is running. Only one recording runs at a time. Not for looking at the screen: use desktop_screenshot. Examples: {\"action\":\"start\",\"sessionId\":\"<from desktop_status>\"}; {\"action\":\"stop\",\"sessionId\":\"<from desktop_status>\"}.".to_string(),
            serde_json::json!({
                "type":"object","additionalProperties":false,"required":["action","sessionId"],
                "properties":{
                    "action":{"type":"string","enum":["start","stop","status"]},
                    "sessionId":{"type":"string","maxLength":64,"description":"Required sessionId from the latest desktop_status."},
                    "maxSeconds":{"type":"integer","minimum":1,"maximum":600,"default":300,"description":"start only: stop recording automatically after this many seconds."}
                }
            }).to_string(),
        ));
    }
    if let Some(secrets) = crate::secret_placeholders::description() {
        for tool in &mut tools {
            if matches!(tool.0.as_str(), "desktop_act" | "desktop_input") {
                tool.1.push_str(&secrets);
            }
        }
    }
    tools
}

/// Schemas for the opt-in model-invoked context tools (wave C).
/// Appended to the tool list only when `checkpoint_tools = true`, so the
/// core default costs nothing extra and the byte-stable frozen list
/// absorbs them at session start.
pub fn checkpoint_tool_schemas() -> Vec<(String, String, String)> {
    vec![
        (
            "checkpoint".to_string(),
            "Mark the current conversation state before exploring. Call this BEFORE an \
             exploratory change whose full history you may want to discard from context \
             later (a risky refactor attempt, a speculative search). Give it a short \
             descriptive label.".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "label": { "type": "string", "maxLength": 80, "description": "Short name for the state being preserved" }
                },
                "required": ["label"]
            })
            .to_string(),
        ),
        (
            "rewind".to_string(),
            "End the checkpoint: everything after it leaves your context and a short \
             report of what should be kept replaces it. Files and history are NOT \
             changed — report file changes rather than reverting; the user's /rewind \
             restores files. Requires an open checkpoint.".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "label": { "type": "string", "description": "The checkpoint to rewind (defaults to the only open one)" },
                    "report": { "type": "string", "maxLength": 4100, "description": "Bounded summary of what the exploration concluded; this is what survives in context" }
                }
            })
            .to_string(),
        ),
        (
            "wait".to_string(),
            "Block until a background result, teammate message, or scheduled event lands, \
             instead of polling with no-op calls. Nothing arrives within the timeout → \
             you get a plain no-event answer; do other work or wait again. Default 60s, \
             cap 300s.".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "seconds": { "type": "integer", "minimum": 1, "maximum": 300, "description": "How long to block (seconds)" }
                }
            })
            .to_string(),
        ),
    ]
}

/// Merge mod-provided tools with the built-ins.
///
/// Mods may add but never remove: a prompt written against the base surface
/// has to keep working when someone enables a mod.
pub fn collect_tools(mods: &[Box<dyn glass_mod_api::Mod_>]) -> Vec<(String, String, String)> {
    let mut tools = builtin_tools();
    for m in mods {
        for (name, desc, schema) in m.tools() {
            if tools.iter().any(|(n, _, _)| *n == name) {
                continue;
            }
            tools.push((name, desc, schema));
        }
    }
    tools
}

/// Count how much of the context window a request would occupy, as a fraction.
pub fn pressure(provider: &dyn Provider, messages: &[glass_mod_api::Message]) -> f32 {
    let window = provider.context_window().max(1);
    provider.estimate_tokens(messages) as f32 / window as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_mod_api::Message;
    use serde_json::Value;

    #[test]
    fn builtin_surface_has_nine_compact_tools() {
        let tools = builtin_tools();
        assert_eq!(tools.len(), 9);
        let schema_bytes = tools
            .iter()
            .map(|(name, description, schema)| name.len() + description.len() + schema.len())
            .sum::<usize>();
        assert!(
            schema_bytes <= 9000,
            "core schema byte budget exceeded: {schema_bytes}"
        );
        assert_eq!(tools[0].0, "bash");
        assert_eq!(tools[1].0, "state");
        assert_eq!(tools[2].0, "web_search");
        assert_eq!(tools[3].0, "web_fetch");
    }

    #[test]
    fn an_image_counts_a_flat_allowance_not_its_bytes() {
        let image = Message::Image {
            media_type: "image/png".into(),
            data: "A".repeat(1_000_000),
        };
        let estimate = default_estimate(&[image]);
        assert!(estimate > 1_500, "too low: {estimate}");
        assert!(
            estimate < 5_000,
            "base64 bytes must not be counted as text: {estimate}"
        );
    }

    #[test]
    fn tool_schemas_are_valid_json_objects() {
        for (_, _, schema) in builtin_tools() {
            let v: Value = serde_json::from_str(&schema).expect("schema must parse");
            assert_eq!(v["type"], "object");
        }
    }

    #[test]
    fn desktop_tools_are_optional_and_input_can_be_omitted() {
        let read_only = desktop_tool_schemas(false);
        let full = desktop_tool_schemas(true);
        assert_eq!(
            read_only
                .iter()
                .map(|(name, _, _)| name.as_str())
                .collect::<Vec<_>>(),
            vec!["desktop_status", "desktop_screenshot", "desktop_observe"]
        );
        assert_eq!(full.len(), 11);
        assert_eq!(full[10].0, "desktop_record");
        assert_eq!(full[9].0, "desktop_wait");
        let wait: Value = serde_json::from_str(&full[9].2).unwrap();
        assert_eq!(wait["additionalProperties"], false);
        let input: Value = serde_json::from_str(&full[4].2).unwrap();
        let actions = input["properties"]["action"]["enum"].to_string();
        assert!(actions.contains("pointer-triple-click") && actions.contains("paste"));
        assert_eq!(full[8].0, "desktop_files");
        assert_eq!(full[7].0, "desktop_windows");
        let windows: Value = serde_json::from_str(&full[7].2).unwrap();
        assert_eq!(
            windows["properties"]["action"]["enum"],
            serde_json::json!(["list", "focus", "close"])
        );
        let screenshot: Value = serde_json::from_str(&full[1].2).unwrap();
        assert_eq!(
            screenshot["properties"]["region"]["additionalProperties"],
            false
        );
        assert_eq!(full[6].0, "desktop_clipboard");
        let clipboard: Value = serde_json::from_str(&full[6].2).unwrap();
        assert_eq!(clipboard["additionalProperties"], false);
        assert_eq!(
            clipboard["properties"]["action"]["enum"],
            serde_json::json!(["read", "write"])
        );
        for (name, _, _) in &full {
            assert!(name.starts_with("desktop_"));
        }
        assert!(full[0].1.contains("frameTimeMs"));
        for name in ["desktop_screenshot", "desktop_observe"] {
            let schema: Value =
                serde_json::from_str(&full.iter().find(|tool| tool.0 == name).unwrap().2).unwrap();
            assert!(
                schema["required"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::String("sessionId".into()))
            );
        }
        let action: Value = serde_json::from_str(&full[3].2).unwrap();
        assert_eq!(action["properties"]["action"]["enum"][0], "click");
        let input: Value = serde_json::from_str(&full[4].2).unwrap();
        assert_eq!(input["properties"]["action"]["enum"][0], "pointer-move");
        assert!(
            input["properties"]["action"]["enum"]
                .as_array()
                .unwrap()
                .contains(&Value::String("pointer-click".into()))
        );
        assert!(
            input["properties"]["action"]["enum"]
                .as_array()
                .unwrap()
                .contains(&Value::String("key-chord".into()))
        );
        assert!(full[4].1.contains("[\"Alt\",\"F2\"]"));
        assert_eq!(input["properties"]["deltaX"]["maximum"], 800);
        assert!(
            input["required"]
                .as_array()
                .unwrap()
                .contains(&Value::String("sessionId".into()))
        );
        let coordinate_rule = input["allOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|rule| {
                rule["if"]["properties"]["action"]["enum"]
                    == serde_json::json!(["pointer-move", "scroll"])
            })
            .expect("move and scroll must require the coordinates used by the input backend");
        assert_eq!(
            coordinate_rule["then"]["required"],
            serde_json::json!(["x", "y"])
        );
        assert!(full[4].1.contains("pane under that point"));
        assert_eq!(full[5].0, "desktop_launch_app");
        assert!(full[5].1.contains("verified=false"));
        let launch: Value = serde_json::from_str(&full[5].2).unwrap();
        assert_eq!(launch["properties"]["action"]["enum"][0], "list");
        assert_eq!(launch["properties"]["action"]["enum"][1], "launch");
        assert_eq!(launch["additionalProperties"], false);
        let observe = full
            .iter()
            .find(|tool| tool.0 == "desktop_observe")
            .expect("desktop observe schema should be present");
        assert!(observe.1.contains("owningWindow"));
        assert!(observe.1.contains("queryAmbiguous"));
        assert!(observe.1.contains("truncated:false"));
        assert!(observe.1.contains("onscreen:false"));
        assert!(observe.1.contains("Broad snapshots omit `onscreen`"));
        assert!(observe.1.contains("repeat the same scoped query"));
        assert!(observe.1.contains("pointerFallback:true"));
        assert!(observe.1.contains("query is complete and unique"));
        let observe_schema: Value = serde_json::from_str(&observe.2).unwrap();
        assert!(observe_schema["properties"]["windowId"].is_object());
        assert!(observe.1.contains("session-scoped selector"));
        assert!(
            observe
                .1
                .contains("the window closes, or its title changes")
        );
        assert!(
            observe
                .1
                .to_ascii_lowercase()
                .contains("the selector may change")
        );
        assert!(
            observe
                .1
                .contains("latest action's returned `observation.windows`")
        );
        assert!(
            observe
                .1
                .contains("broad action observations do not include actionable target IDs")
        );
        let query_description = observe_schema["properties"]["query"]["description"]
            .as_str()
            .unwrap();
        assert!(query_description.contains("owningWindow title"));
        assert!(query_description.contains("onscreen=true"));
        assert!(query_description.contains("Broad snapshots have no action tokens"));
        assert!(query_description.contains("pointerFallback=true"));
        assert!(
            observe_schema["properties"]["windowId"]["description"]
                .as_str()
                .unwrap()
                .contains("unscoped desktop_observe")
        );
        assert!(full[3].1.contains("onscreen:true"));
        assert!(full[3].1.contains("call desktop_observe again"));
        assert!(full[3].1.contains("no supported accessibility action"));
        assert!(full[3].1.contains("does not contain actionable target IDs"));
        assert!(
            full[3]
                .1
                .contains("fresh focused desktop_observe before the next action")
        );
        assert!(
            observe
                .1
                .contains("query the current address and page heading")
        );
        assert!(
            observe
                .1
                .contains("observe the tab labels and active address")
        );
        assert!(observe.1.contains("small increments"));
    }

    #[test]
    fn bash_accepts_command_or_owned_job_but_state_requires_nothing() {
        let tools = builtin_tools();
        let bash: Value = serde_json::from_str(&tools[0].2).unwrap();
        let state: Value = serde_json::from_str(&tools[1].2).unwrap();
        assert_eq!(tools.len(), 9);
        assert_eq!(bash["anyOf"][0]["required"][0], "command");
        assert_eq!(bash["anyOf"][1]["required"][0], "job");
        assert_eq!(bash["properties"]["job"]["required"][0], "action");
        // `state` must be callable with zero arguments, or the read-back path
        // the compaction flow depends on becomes awkward to express.
        assert!(state.get("required").is_none());
    }

    #[test]
    fn mods_can_add_but_not_shadow_builtin_tools() {
        struct Greedy;
        impl glass_mod_api::Mod_ for Greedy {
            fn tools(&self) -> Vec<(String, String, String)> {
                vec![
                    ("bash".to_string(), "hijack".to_string(), "{}".to_string()),
                    ("read".to_string(), "hijack".to_string(), "{}".to_string()),
                    ("edit".to_string(), "hijack".to_string(), "{}".to_string()),
                    ("write".to_string(), "hijack".to_string(), "{}".to_string()),
                    ("custom".to_string(), "mine".to_string(), "{}".to_string()),
                ]
            }
        }
        let mods: Vec<Box<dyn glass_mod_api::Mod_>> = vec![Box::new(Greedy)];
        let tools = collect_tools(&mods);
        let names: Vec<_> = tools.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "bash",
                "state",
                "web_search",
                "web_fetch",
                "display_image",
                "view_image",
                "read",
                "edit",
                "write",
                "custom"
            ]
        );
    }

    #[test]
    fn unknown_provider_is_a_config_error() {
        match build("mystery", "https://x", None, "m", true) {
            Err(Error::Config(m)) => assert!(m.contains("mystery")),
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("expected a config error"),
        }
    }

    #[test]
    fn provider_dispatch_covers_all_three_flavours() {
        assert_eq!(
            build("anthropic", "https://example.invalid", None, "m", true)
                .unwrap()
                .id(),
            "anthropic"
        );
        assert_eq!(
            build("openai", "https://example.invalid", None, "m", true)
                .unwrap()
                .id(),
            "openai"
        );
        assert_eq!(
            build(
                "openai-responses",
                "https://example.invalid",
                None,
                "m",
                true
            )
            .unwrap()
            .id(),
            "openai"
        );
    }

    #[test]
    fn pressure_is_a_fraction_between_zero_and_one() {
        let p: Box<dyn Provider> = Box::new(OpenAiCompat::new("u", None, "gpt-5"));
        let msgs: Vec<Message> = (0..50)
            .map(|i| Message::User(format!("message {i}")))
            .collect();
        let f = pressure(p.as_ref(), &msgs);
        assert!(f > 0.0 && f < 1.0, "{f}");
    }

    /// Counts calls and fails the first `fail` of them.
    struct Flaky {
        calls: std::sync::Arc<std::sync::atomic::AtomicU8>,
        fail: u8,
        code: u16,
    }

    impl Provider for Flaky {
        fn id(&self) -> &str {
            "flaky"
        }
        fn stream(
            &self,
            _req: &Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < self.fail {
                return Err(Error::Status(self.code, "overloaded".into()));
            }
            Ok(Box::new(
                vec![Event::Text("ok".into()), Event::Done].into_iter(),
            ))
        }
        fn context_window(&self) -> u32 {
            1_000
        }
    }

    fn flaky_req() -> Request<'static> {
        Request {
            model: "m",
            system: "s",
            messages: &[],
            tools: &[],
            max_tokens: 10,
            reasoning: None,
        }
    }

    #[test]
    fn a_transient_failure_is_retried() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let p = Flaky {
            calls: std::sync::Arc::clone(&calls),
            fail: 2,
            code: 429,
        };
        let mut saw: Vec<(u8, u64)> = Vec::new();
        let out = stream_with_retry(&p, &flaky_req(), 3, &[0, 0], |attempt, delay, _| {
            saw.push((attempt, delay))
        });
        assert!(out.is_ok(), "the third attempt must succeed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert_eq!(saw, vec![(1, 0), (2, 0)], "both retries are reported");
    }

    #[test]
    fn cancelling_a_retry_does_not_wait_or_send_again() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicU8, Ordering},
        };
        let calls = Arc::new(AtomicU8::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let p = Flaky {
            calls: calls.clone(),
            fail: 1,
            code: 429,
        };
        let start = std::time::Instant::now();
        cancellable::scoped(cancelled.clone(), || {
            let result = stream_with_retry(&p, &flaky_req(), 3, &[10], |_, _, _| {
                cancelled.store(true, Ordering::Release)
            });
            assert!(
                matches!(result, Err(Error::Transport(message)) if message.contains("cancelled"))
            );
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn already_cancelled_requests_do_not_send() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, AtomicU8, Ordering},
        };
        let calls = Arc::new(AtomicU8::new(0));
        let p = Flaky {
            calls: calls.clone(),
            fail: 0,
            code: 429,
        };
        cancellable::scoped(Arc::new(AtomicBool::new(true)), || {
            assert!(stream_with_retry(&p, &flaky_req(), 3, &[0], |_, _, _| {}).is_err());
        });
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_server_error_is_transient() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let p = Flaky {
            calls: std::sync::Arc::clone(&calls),
            fail: 1,
            code: 503,
        };
        let out = stream_with_retry(&p, &flaky_req(), 3, &[0, 0], |_, _, _| {});
        assert!(out.is_ok());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn a_client_error_is_not_retried() {
        // A 400 is a request problem. Retrying would spend the
        // same money to get the same answer.
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let p = Flaky {
            calls: std::sync::Arc::clone(&calls),
            fail: 9,
            code: 400,
        };
        let out = stream_with_retry(&p, &flaky_req(), 3, &[0, 0], |_, _, _| {});
        assert!(out.is_err());
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "no retry on a 4xx"
        );
    }

    #[test]
    fn quota_refusals_are_not_transient_and_keep_their_classification() {
        for detail in [
            "insufficient_quota",
            "quota exceeded",
            "credits exhausted",
            "billing required",
            "usage limit reached",
        ] {
            assert!(!is_transient(&Error::Status(429, detail.into())));
        }
        for field in ["code", "type"] {
            for code in [
                "insufficient_quota",
                "quota_exceeded",
                "usage_limit_reached",
            ] {
                let value = serde_json::json!({field:code, "message":"Provider returned error"});
                for body in [
                    value.to_string(),
                    serde_json::json!({"error":value}).to_string(),
                ] {
                    let message = http_error_message(&body);
                    assert!(message.contains(code));
                    assert!(!is_transient(&Error::Status(429, message)));
                }
            }
        }
        assert!(is_transient(&Error::Status(
            429,
            "too many requests".into()
        )));
    }

    #[test]
    fn retries_stop_at_the_attempt_budget() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let p = Flaky {
            calls: std::sync::Arc::clone(&calls),
            fail: 9,
            code: 500,
        };
        let out = stream_with_retry(&p, &flaky_req(), 3, &[0, 0], |_, _, _| {});
        assert!(out.is_err(), "persistent failure still fails the turn");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn ambiguous_transport_failure_is_not_replayed_automatically() {
        struct Accepted(std::cell::Cell<u32>);
        impl Provider for Accepted {
            fn id(&self) -> &str {
                "accepted"
            }
            fn context_window(&self) -> u32 {
                100_000
            }
            fn stream(
                &self,
                _: &Request<'_>,
            ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
                self.0.set(self.0.get() + 1);
                Err(Error::Transport(
                    "connection closed after POST acceptance".into(),
                ))
            }
        }
        let provider = Accepted(std::cell::Cell::new(0));
        let mut retries = 0;
        assert!(
            stream_with_retry(&provider, &flaky_req(), 3, &[0, 0], |_, _, _| retries += 1).is_err()
        );
        assert_eq!(provider.0.get(), 1);
        assert_eq!(retries, 0);
    }

    #[test]
    fn displayed_errors_redact_credentials_for_every_caller() {
        let detail = "Authorization: Bearer sk-provider-secret-sentinel https://example.test/?token=private\x1b]52;c;EVIL\x07";
        for error in [
            Error::Transport(detail.into()),
            Error::Status(401, detail.into()),
            Error::Config(detail.into()),
        ] {
            let text = error.to_string();
            assert!(!text.contains("sk-provider-secret-sentinel"));
            assert!(!text.contains("example.test"));
            assert!(!text.contains('\x1b'));
            assert!(!text.contains('\x07'));
        }
    }
}
