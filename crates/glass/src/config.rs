//! Configuration.
//!
//! One TOML file, one optional env var per secret. Anything omitted falls back
//! to an environment variable, because a config file that has to hold an API
//! key is a config file that ends up committed.

use std::path::{Path, PathBuf};

use crate::notify;
use crate::pricing;
use crate::sandbox;

/// Whether workspace instructions need consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Trust {
    /// Ask once per workspace, keyed by content hash.
    #[default]
    Ask,
    /// Load project instructions without asking (pre-gate behavior).
    Always,
}

impl Trust {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ask" | "prompt" => Some(Trust::Ask),
            "always" => Some(Trust::Always),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Env var holding the key. Read at startup, never stored in the file.
    pub api_key_env: Option<String>,
    /// Selected glass-core account; never inferred from another application.
    pub connection: Option<String>,
    pub workspace: PathBuf,
    pub max_tokens: u32,
    pub reasoning_budget: Option<u32>,
    pub cost_limit: Option<f64>,
    /// Per-million token prices. `None` falls back to the built-in
    /// table, which covers the Anthropic models; an unknown model
    /// simply shows tokens, never a guessed price.
    pub pricing: Option<pricing::RateCard>,
    /// Extra directory scanned for `skills/*.md`.
    pub skills_dir: Option<PathBuf>,
    /// Mods to load, by name.
    pub mods: Vec<String>,
    /// External extension executables to spawn, by path or PATH name.
    pub extensions: Vec<String>,
    /// How a finished turn gets attention: auto, osc, bell, or off.
    pub notify: notify::Mode,
    /// Model ids to try, in order, when the primary fails before any event.
    /// The same connection and endpoint serve every fallback.
    pub fallback: Vec<String>,
    /// Whether project instruction files need consent.
    pub trust: Trust,
    /// OS-level confinement for shell commands.
    pub sandbox: sandbox::Policy,
    /// Model shell write profile. Defaults to legacy unrestricted execution.
    pub permission_profile: sandbox::PermissionProfile,
    /// Extension confinement: off (legacy) or strict (declaration required).
    pub extension_isolation: String,
    /// Outbound TCP ports a confined shell may connect to (Landlock ABI 4+).
    /// An empty list denies all outbound TCP; binds are never restricted.
    pub sandbox_ports: Vec<u16>,
    /// Credential env scrubbing for shell children: "", "scrub", or "inherit".
    /// Empty means "scrub whenever the sandbox is on".
    pub sandbox_env: String,
    /// Per-record fsync policy for session logs: "off" (default) or "always".
    pub log_fsync: String,
    /// How often streamed partial output is committed to the sidecar, in ms.
    /// 0 disables the sidecar; a crash otherwise loses at most this window.
    pub live_flush_ms: u32,
    /// OSC 7501 program status: "on" (default; interactive terminals only)
    /// or "off".
    pub program_status: String,
    pub system_prompt: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-opus-4-5".into(),
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            connection: None,
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_tokens: 16_000,
            reasoning_budget: None,
            cost_limit: None,
            pricing: None,
            skills_dir: None,
            mods: Vec::new(),
            extensions: Vec::new(),
            notify: notify::Mode::default(),
            fallback: Vec::new(),
            trust: Trust::default(),
            sandbox: sandbox::Policy::default(),
            permission_profile: sandbox::PermissionProfile::default(),
            extension_isolation: "off".into(),
            sandbox_ports: vec![443, 80],
            sandbox_env: String::new(),
            log_fsync: "off".into(),
            live_flush_ms: 250,
            program_status: "on".into(),
            system_prompt: BASE_SYSTEM_PROMPT.to_string(),
        }
    }
}

/// The default system prompt.
///
/// Kept short on purpose. A large prompt is paid on every request and, past a
/// few hundred tokens, most of it stops changing the model's behaviour. What
/// survives here is the discipline that a two-tool surface requires.
pub const BASE_SYSTEM_PROMPT: &str = "\
You are a coding agent working directly in the user's workspace.

You have exactly two tools.

`bash` runs a shell command in the workspace. This is your only way to read,
write, search, edit, build, test, and use version control. There is no separate
read, write, edit, or search tool. Prefer one command that does the job over
several that each do a piece of it.
Native file helpers: run `\"$GC_BIN\" file --help` for bounded reads, revision-checked
range/exact edits, atomic writes, and syntax diagnostics. Prefer them for edits.

Tasks, evidence, artifacts, handoffs and budgets: `\"$GC_BIN\" workspace --help`. Claim before edits; report checks.

Persistent teammates: `\"$GC_BIN\" agent --help` provides create/list/send/wait,
transcripts and interrupt/resume. Prefer doing the work yourself with one
read-only reviewer when useful. Reuse named agents with focused follow-ups;
send goals, relevant paths and changes, never the entire conversation. Delegate
only independent work. Check reports and preserve the original task.

`state` records durable notes that survive context compaction. Call it with a
list of specific facts, file paths, decisions, and next steps whenever you
reach a milestone on a long task, and always before you expect context to run
short. Be concrete: \"auth check now lives in src/auth/verify.rs\" is useful,
\"worked on auth\" is not. Call it with no arguments to read the current state
back.

Working style:
- Treat greetings, thanks, and ordinary conversation as conversation. Reply
  directly without inspecting the workspace or using tools unless the user asks
  for information or an action that needs them.
- For coding and workspace tasks, inspect the relevant files, then act and
  verify. Use a tool only when it helps answer or carry out the user's request.
- Investigate before changing. Read the code you are about to edit.
- Make the change, then verify it compiles and passes tests. Report what you ran.
- Do not describe what you are about to do. Run it.
- Keep going until the task is actually done. If something fails, diagnose it and
  try a different approach rather than stopping at the first error.
- When you report completion, state what you verified, not what you intended.

If a task is ambiguous in a way that changes the outcome, pick the most
reasonable reading, do it, and say which reading you took.";

/// Configuration belongs exclusively to glass-core. The separate Glass
/// project owns `glass`; never read or migrate its configuration or sessions.
pub fn config_dir() -> PathBuf {
    if let Some(root) = xdg_root("XDG_CONFIG_HOME", &[".config"]) {
        return root.join("glass-core");
    }
    PathBuf::from(".glass-core").join("config")
}

/// Session data, isolated from the separate Glass project's data tree.
pub fn data_dir() -> PathBuf {
    if let Some(root) = xdg_root("XDG_DATA_HOME", &[".local", "share"]) {
        return root.join("glass-core");
    }
    PathBuf::from(".glass-core").join("data")
}

fn xdg_root(variable: &str, home_suffix: &[&str]) -> Option<PathBuf> {
    if let Some(x) = std::env::var_os(variable).filter(|x| !x.is_empty()) {
        return Some(PathBuf::from(x));
    }
    let mut root = PathBuf::from(std::env::var_os("HOME").filter(|h| !h.is_empty())?);
    for part in home_suffix {
        root.push(part);
    }
    Some(root)
}

impl Config {
    /// Read the API key from the configured env var.
    pub fn api_key(&self) -> Option<String> {
        let name = self.api_key_env.as_deref()?;
        std::env::var(name).ok().filter(|s| !s.trim().is_empty())
    }

    /// The rate card in force: an explicit `[pricing]` section, else
    /// the built-in table for the current model, else nothing.
    pub fn rate_card(&self, model: &str) -> Option<pricing::RateCard> {
        // API-key prices do not describe ChatGPT plan usage.
        if crate::auth::is_chatgpt(self) {
            return None;
        }
        self.pricing.or_else(|| {
            (self.provider == "anthropic"
                && self.base_url.trim_end_matches('/') == "https://api.anthropic.com")
                .then(|| pricing::builtin(model))
                .flatten()
        })
    }
}

/// Default config path.
pub fn default_path() -> PathBuf {
    config_dir().join("config.toml")
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(toml::de::Error),
    /// A required field was missing or unusable.
    Invalid(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "config: {e}"),
            LoadError::Parse(e) => write!(f, "config parse: {e}"),
            LoadError::Invalid(m) => write!(f, "config: {m}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Raw TOML shape. Every field optional so a partial file is valid.
#[derive(Debug, Default, serde::Deserialize)]
struct Raw {
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    api_key_env: Option<String>,
    workspace: Option<String>,
    max_tokens: Option<u32>,
    reasoning_budget: Option<u32>,
    cost_limit: Option<f64>,
    pricing: Option<PricingRaw>,
    system_prompt: Option<String>,
    mods: Option<Vec<String>>,
    extensions: Option<Vec<String>>,
    notify: Option<String>,
    fallback: Option<Vec<String>>,
    trust: Option<String>,
    sandbox: Option<String>,
    permission_profile: Option<String>,
    extension_isolation: Option<String>,
    sandbox_ports: Option<Vec<u16>>,
    sandbox_env: Option<String>,
    log_fsync: Option<String>,
    live_flush_ms: Option<u32>,
    program_status: Option<String>,

    skills_dir: Option<String>,
    skills: Option<Vec<String>>,
}

/// `[pricing]` in config. All four fields are required: a partial
/// card would misprice every request silently, which is worse than
/// no card at all.
#[derive(Debug, Default, serde::Deserialize)]
struct PricingRaw {
    input: Option<f64>,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
    output: Option<f64>,
}

/// Load config from `path`, falling back to defaults for absent fields.
///
/// A missing file is not an error: glass should run with zero configuration.
pub fn load(path: &Path) -> Result<Config, LoadError> {
    let mut cfg = Config::default();

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(cfg),
        Err(e) => return Err(LoadError::Io(e)),
    };

    let raw: Raw = toml::from_str(&text).map_err(LoadError::Parse)?;

    if let Some(v) = raw.provider {
        cfg.provider = v;
    }
    if let Some(v) = raw.base_url {
        cfg.base_url = v;
    }
    if let Some(v) = raw.model {
        cfg.model = v;
    }
    // An explicit empty string means "no key needed", which is how local
    // runtimes like ollama are configured.
    if let Some(v) = raw.api_key_env {
        cfg.api_key_env = (!v.trim().is_empty()).then_some(v);
    }
    if let Some(v) = raw.workspace {
        let p = PathBuf::from(&v);
        let p = if p.is_absolute() {
            p
        } else {
            cfg.workspace.join(p)
        };
        if !p.is_dir() {
            return Err(LoadError::Invalid(format!(
                "workspace `{v}` is not a directory"
            )));
        }
        cfg.workspace = p;
    }
    if let Some(v) = raw.max_tokens {
        if v == 0 {
            return Err(LoadError::Invalid(
                "max_tokens must be greater than zero".into(),
            ));
        }
        cfg.max_tokens = v;
    }
    if let Some(v) = raw.reasoning_budget {
        cfg.reasoning_budget = Some(v);
    }
    if let Some(v) = raw.cost_limit {
        if !v.is_finite() || v <= 0.0 {
            return Err(LoadError::Invalid(
                "cost_limit must be finite and positive".into(),
            ));
        }
        cfg.cost_limit = Some(v);
    }
    if let Some(p) = raw.pricing {
        let card = || {
            Some(pricing::RateCard {
                input: p.input?,
                cache_read: p.cache_read?,
                cache_write: p.cache_write?,
                output: p.output?,
            })
        };
        match card() {
            Some(c) => {
                if [c.input, c.cache_read, c.cache_write, c.output]
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0.0)
                {
                    return Err(LoadError::Invalid(
                        "pricing rates must be finite and nonnegative".into(),
                    ));
                }
                cfg.pricing = Some(c);
            }
            None => {
                return Err(LoadError::Invalid(
                    "pricing needs input, cache_read, cache_write and output".into(),
                ));
            }
        }
    }
    if let Some(v) = raw.system_prompt {
        if v.trim().is_empty() {
            return Err(LoadError::Invalid("system_prompt must not be empty".into()));
        }
        cfg.system_prompt = v;
    }
    if let Some(v) = raw.mods {
        cfg.mods = v;
    }
    if let Some(v) = raw.extensions {
        cfg.extensions = v;
    }
    if let Some(v) = raw.notify {
        cfg.notify = notify::Mode::parse(&v).ok_or_else(|| {
            LoadError::Invalid(format!(
                "notify must be auto, osc, osc777, bell, or off (got `{v}`)"
            ))
        })?;
    }
    if let Some(v) = raw.fallback {
        // Empty entries are dropped rather than rejected: a trailing comma
        // in a hand-written list should not stop the session starting.
        cfg.fallback = v
            .into_iter()
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect();
    }
    if let Some(v) = raw.trust {
        cfg.trust = Trust::parse(&v).ok_or_else(|| {
            LoadError::Invalid(format!("trust must be ask or always (got `{v}`)"))
        })?;
    }
    if let Some(v) = raw.sandbox {
        cfg.sandbox = sandbox::Policy::parse(&v).ok_or_else(|| {
            LoadError::Invalid(format!(
                "sandbox must be off, workspace, or strict (got `{v}`)"
            ))
        })?;
    }
    if let Some(v) = raw.permission_profile {
        cfg.permission_profile = sandbox::PermissionProfile::parse(&v).ok_or_else(|| {
            LoadError::Invalid(format!(
                "permission_profile must be unrestricted, workspace-edit, or read-only (got `{v}`)"
            ))
        })?;
    }
    if let Some(v) = raw.extension_isolation {
        let v = v.trim().to_ascii_lowercase();
        if !matches!(v.as_str(), "off" | "strict") {
            return Err(LoadError::Invalid(format!(
                "extension_isolation must be off or strict (got `{v}`)"
            )));
        }
        cfg.extension_isolation = v;
    }
    if let Some(ports) = raw.sandbox_ports {
        cfg.sandbox_ports = ports;
    }
    if let Some(v) = raw.sandbox_env {
        let v = v.trim().to_ascii_lowercase();
        if !matches!(v.as_str(), "" | "scrub" | "inherit") {
            return Err(LoadError::Invalid(format!(
                "sandbox_env must be scrub, inherit, or empty (got `{v}`)"
            )));
        }
        cfg.sandbox_env = v;
    }
    if let Some(v) = raw.log_fsync {
        let v = v.trim().to_ascii_lowercase();
        if !matches!(v.as_str(), "off" | "always") {
            return Err(LoadError::Invalid(format!(
                "log_fsync must be off or always (got `{v}`)"
            )));
        }
        cfg.log_fsync = v;
    }
    if let Some(v) = raw.live_flush_ms {
        if v > 60_000 {
            return Err(LoadError::Invalid(format!(
                "live_flush_ms is capped at 60000 (got {v}); 0 disables the sidecar"
            )));
        }
        cfg.live_flush_ms = v;
    }
    if let Some(v) = raw.program_status {
        let v = v.trim().to_ascii_lowercase();
        if !matches!(v.as_str(), "on" | "off") {
            return Err(LoadError::Invalid(format!(
                "program_status must be on or off (got `{v}`)"
            )));
        }
        cfg.program_status = v;
    }

    // `skills = [...]` in the default location narrows the set, so an empty
    // list is meaningful: it disables skills without needing a skills_dir.
    let skills_list = raw.skills.unwrap_or_default();
    cfg.skills_dir = match (raw.skills_dir, skills_list.is_empty()) {
        (Some(d), _) if !d.trim().is_empty() => {
            let p = PathBuf::from(&d);
            Some(if p.is_absolute() {
                p
            } else {
                config_dir().join(p)
            })
        }
        (_, true) => None,
        (_, false) => {
            let d = config_dir().join("skills");
            d.is_dir().then_some(d)
        }
    };

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-config-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_yields_defaults() {
        let d = scratch("missing");
        let cfg = load(&d.join("nope.toml")).unwrap();
        assert_eq!(cfg.provider, "anthropic");
        assert!(cfg.system_prompt.contains("`bash`"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn reads_a_partial_file() {
        let d = scratch("partial");
        let p = write(&d, "c.toml", "model = \"gpt-5\"\nprovider = \"openai\"\n");
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.model, "gpt-5");
        assert_eq!(cfg.provider, "openai");
        // Untouched fields keep their defaults.
        assert_eq!(cfg.max_tokens, Config::default().max_tokens);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn empty_api_key_env_means_no_key_required() {
        let d = scratch("nokey");
        let p = write(&d, "c.toml", "api_key_env = \"\"\n");
        let cfg = load(&p).unwrap();
        assert!(
            cfg.api_key_env.is_none(),
            "empty string must disable the key, not look up \"\""
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rejects_zero_max_tokens() {
        let d = scratch("zero");
        let p = write(&d, "c.toml", "max_tokens = 0\n");
        assert!(matches!(load(&p), Err(LoadError::Invalid(_))));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rejects_empty_system_prompt() {
        let d = scratch("emptyprompt");
        let p = write(&d, "c.toml", "system_prompt = \"  \"\n");
        assert!(matches!(load(&p), Err(LoadError::Invalid(_))));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rejects_missing_workspace() {
        let d = scratch("nows");
        let p = write(&d, "c.toml", "workspace = \"/definitely/not/here\"\n");
        assert!(matches!(load(&p), Err(LoadError::Invalid(_))));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rejects_malformed_toml() {
        let d = scratch("bad");
        let p = write(&d, "c.toml", "model = \n");
        assert!(matches!(load(&p), Err(LoadError::Parse(_))));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn notify_defaults_to_auto_and_parses_values() {
        let d = scratch("notify");
        let cfg = load(&d.join("none.toml")).unwrap();
        assert_eq!(cfg.notify, notify::Mode::Auto);
        let p = write(&d, "c.toml", "notify = \"bell\"\n");
        assert_eq!(load(&p).unwrap().notify, notify::Mode::Bell);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rejects_an_unknown_notify_value() {
        let d = scratch("notify-bad");
        let p = write(&d, "c.toml", "notify = \"loud\"\n");
        let err = load(&p).unwrap_err().to_string();
        assert!(err.contains("notify"), "{err}");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn fallback_defaults_empty_and_drops_blank_entries() {
        let d = scratch("fallback");
        assert!(load(&d.join("none.toml")).unwrap().fallback.is_empty());
        let p = write(
            &d,
            "c.toml",
            "fallback = [\" model-b \", \"\", \"model-c\"]\n",
        );
        assert_eq!(load(&p).unwrap().fallback, vec!["model-b", "model-c"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn trust_defaults_to_ask_and_parses_values() {
        let d = scratch("trust");
        assert_eq!(load(&d.join("none.toml")).unwrap().trust, Trust::Ask);
        let p = write(&d, "c.toml", "trust = \"always\"\n");
        assert_eq!(load(&p).unwrap().trust, Trust::Always);
        let p = write(&d, "bad.toml", "trust = \"maybe\"\n");
        assert!(load(&p).unwrap_err().to_string().contains("trust"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn sandbox_defaults_to_off_and_parses_values() {
        let d = scratch("sandbox");
        assert_eq!(
            load(&d.join("none.toml")).unwrap().sandbox,
            sandbox::Policy::Off
        );
        let p = write(&d, "c.toml", "sandbox = \"workspace\"\n");
        assert_eq!(load(&p).unwrap().sandbox, sandbox::Policy::Workspace);
        let p = write(&d, "strict.toml", "sandbox = \"strict\"\n");
        assert_eq!(load(&p).unwrap().sandbox, sandbox::Policy::Strict);
        let p = write(&d, "bad.toml", "sandbox = \"jail\"\n");
        assert!(load(&p).unwrap_err().to_string().contains("sandbox"));
    }

    #[test]
    fn sandbox_network_and_env_knobs_validate() {
        let d = scratch("sandbox-knobs");
        let plain = load(&d.join("none.toml")).unwrap();
        assert_eq!(plain.sandbox_ports, vec![443, 80], "sane egress default");
        assert_eq!(plain.sandbox_env, "", "empty means scrub-with-sandbox");

        let p = write(
            &d,
            "c.toml",
            "sandbox = \"workspace\"\nsandbox_ports = [22, 443]\nsandbox_env = \"inherit\"\n",
        );
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.sandbox_ports, vec![22, 443]);
        assert_eq!(cfg.sandbox_env, "inherit");

        let p = write(&d, "empty.toml", "sandbox_ports = []\n");
        assert!(load(&p).unwrap().sandbox_ports.is_empty(), "deny all TCP");

        let p = write(&d, "bad.toml", "sandbox_env = \"leak\"\n");
        assert!(load(&p).unwrap_err().to_string().contains("sandbox_env"));
    }

    #[test]
    fn permission_profiles_and_extension_isolation_validate_with_legacy_defaults() {
        let d = scratch("permission-profiles");
        let defaults = load(&d.join("none.toml")).unwrap();
        assert_eq!(
            defaults.permission_profile,
            sandbox::PermissionProfile::Unrestricted
        );
        assert_eq!(defaults.extension_isolation, "off");
        let p = write(
            &d,
            "profiles.toml",
            "permission_profile = \"read-only\"\nextension_isolation = \"strict\"\n",
        );
        let cfg = load(&p).unwrap();
        assert_eq!(cfg.permission_profile, sandbox::PermissionProfile::ReadOnly);
        assert_eq!(cfg.extension_isolation, "strict");
        let p = write(
            &d,
            "bad-profile.toml",
            "permission_profile = \"mostly-safe\"\n",
        );
        assert!(
            load(&p)
                .unwrap_err()
                .to_string()
                .contains("permission_profile")
        );
        let p = write(
            &d,
            "bad-extension.toml",
            "extension_isolation = \"best-effort\"\n",
        );
        assert!(
            load(&p)
                .unwrap_err()
                .to_string()
                .contains("extension_isolation")
        );
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn log_fsync_parses_off_by_default_and_rejects_typos() {
        let d = scratch("log-fsync");
        assert_eq!(load(&d.join("none.toml")).unwrap().log_fsync, "off");
        let p = write(&d, "always.toml", "log_fsync = \"always\"\n");
        assert_eq!(load(&p).unwrap().log_fsync, "always");
        let p = write(&d, "bad.toml", "log_fsync = \"sometimes\"\n");
        assert!(load(&p).unwrap_err().to_string().contains("log_fsync"));
    }

    #[test]
    fn live_flush_ms_defaults_and_is_capped() {
        let d = scratch("live-flush");
        assert_eq!(load(&d.join("none.toml")).unwrap().live_flush_ms, 250);
        let p = write(&d, "off.toml", "live_flush_ms = 0\n");
        assert_eq!(load(&p).unwrap().live_flush_ms, 0, "0 disables the sidecar");
        let p = write(&d, "huge.toml", "live_flush_ms = 60001\n");
        assert!(load(&p).unwrap_err().to_string().contains("live_flush_ms"));
    }

    #[test]
    fn program_status_defaults_on_and_rejects_typos() {
        let d = scratch("program-status");
        assert_eq!(load(&d.join("none.toml")).unwrap().program_status, "on");
        let p = write(&d, "off.toml", "program_status = \"off\"\n");
        assert_eq!(load(&p).unwrap().program_status, "off");
        let p = write(&d, "bad.toml", "program_status = \"maybe\"\n");
        assert!(load(&p).unwrap_err().to_string().contains("program_status"));
    }

    #[test]
    fn base_prompt_names_both_tools() {
        assert!(BASE_SYSTEM_PROMPT.contains("`bash`"));
        assert!(BASE_SYSTEM_PROMPT.contains("`state`"));
    }

    #[test]
    fn base_prompt_keeps_conversation_separate_from_workspace_work() {
        assert!(
            BASE_SYSTEM_PROMPT
                .contains("Treat greetings, thanks, and ordinary conversation as conversation")
        );
        assert!(BASE_SYSTEM_PROMPT.contains("without inspecting the workspace or using tools"));
        assert!(
            BASE_SYSTEM_PROMPT
                .contains("For coding and workspace tasks, inspect the relevant files")
        );
        assert!(
            BASE_SYSTEM_PROMPT
                .contains("Use a tool only when it helps answer or carry out the user's request.")
        );
    }

    #[test]
    fn base_prompt_is_deliberately_small() {
        // Roughly 3.5 chars/token. A prompt that crept past a few hundred
        // tokens would be paid on every single request.
        let approx = BASE_SYSTEM_PROMPT.len() / 4;
        assert!(
            approx < 600,
            "base prompt is ~{approx} tokens, expected under 600"
        );
    }

    #[test]
    fn a_complete_pricing_section_loads() {
        let d = scratch("pricing");
        let p = write(
            &d,
            "c.toml",
            "model = \"gpt-5\"\n[pricing]\ninput = 1.25\ncache_read = 0.125\ncache_write = 1.5\noutput = 10.0\n",
        );
        let cfg = load(&p).unwrap();
        let card = cfg.pricing.expect("card must load");
        assert_eq!(card.input, 1.25);
        assert_eq!(card.output, 10.0);
        // The explicit card wins over any built-in lookup.
        assert!(cfg.rate_card("gpt-5").is_some());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_partial_pricing_section_is_rejected() {
        let d = scratch("pricing-partial");
        let p = write(&d, "c.toml", "[pricing]\ninput = 1.25\noutput = 10.0\n");
        assert!(
            matches!(load(&p), Err(LoadError::Invalid(_))),
            "a partial card must be rejected, not silently mispriced"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_opus_model_gets_a_builtin_card() {
        let d = scratch("pricing-builtin");
        let p = write(&d, "c.toml", "model = \"claude-opus-4-5\"\n");
        let cfg = load(&p).unwrap();
        let card = cfg.rate_card(&cfg.model).expect("opus has a built-in card");
        assert_eq!(card.input, 5.0);
        assert_eq!(card.output, 25.0);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn an_unknown_model_has_no_card() {
        let d = scratch("pricing-none");
        let p = write(&d, "c.toml", "model = \"some-local-llm\"\n");
        let cfg = load(&p).unwrap();
        assert!(cfg.rate_card(&cfg.model).is_none());
        std::fs::remove_dir_all(&d).ok();
    }
}
