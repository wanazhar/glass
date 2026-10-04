//! Configuration.
//!
//! One TOML file, one optional env var per secret. Anything omitted falls back
//! to an environment variable, because a config file that has to hold an API
//! key is a config file that ends up committed.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Env var holding the key. Read at startup, never stored in the file.
    pub api_key_env: Option<String>,
    pub workspace: PathBuf,
    pub max_tokens: u32,
    pub reasoning_budget: Option<u32>,
    /// Extra directory scanned for `skills/*.md`.
    pub skills_dir: Option<PathBuf>,
    /// Mods to load, by name.
    pub mods: Vec<String>,
    pub system_prompt: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-opus-4-5".into(),
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            max_tokens: 16_000,
            reasoning_budget: None,
            skills_dir: None,
            mods: Vec::new(),
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

`state` records durable notes that survive context compaction. Call it with a
list of specific facts, file paths, decisions, and next steps whenever you
reach a milestone on a long task, and always before you expect context to run
short. Be concrete: \"auth check now lives in src/auth/verify.rs\" is useful,
\"worked on auth\" is not. Call it with no arguments to read the current state
back.

Working style:
- Investigate before changing. Read the code you are about to edit.
- Make the change, then verify it compiles and passes tests. Report what you ran.
- Do not describe what you are about to do. Run it.
- Keep going until the task is actually done. If something fails, diagnose it and
  try a different approach rather than stopping at the first error.
- When you report completion, state what you verified, not what you intended.

If a task is ambiguous in a way that changes the outcome, pick the most
reasonable reading, do it, and say which reading you took.";

/// Where config lives, respecting `XDG_CONFIG_HOME`.
pub fn config_dir() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CONFIG_HOME")
        && !x.is_empty()
    {
        return PathBuf::from(x).join("glass");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".config").join("glass");
    }
    PathBuf::from(".glass")
}

/// Where sessions are written, respecting `XDG_DATA_HOME`.
pub fn data_dir() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_DATA_HOME")
        && !x.is_empty()
    {
        return PathBuf::from(x).join("glass");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local").join("share").join("glass");
    }
    PathBuf::from(".glass")
}

impl Config {
    /// Read the API key from the configured env var.
    pub fn api_key(&self) -> Option<String> {
        let name = self.api_key_env.as_deref()?;
        std::env::var(name).ok().filter(|s| !s.trim().is_empty())
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
    system_prompt: Option<String>,
    mods: Option<Vec<String>>,

    skills_dir: Option<String>,
    skills: Option<Vec<String>>,
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
        let p = if p.is_absolute() { p } else { cfg.workspace.join(p) };
        if !p.is_dir() {
            return Err(LoadError::Invalid(format!("workspace `{v}` is not a directory")));
        }
        cfg.workspace = p;
    }
    if let Some(v) = raw.max_tokens {
        if v == 0 {
            return Err(LoadError::Invalid("max_tokens must be greater than zero".into()));
        }
        cfg.max_tokens = v;
    }
    if let Some(v) = raw.reasoning_budget {
        cfg.reasoning_budget = Some(v);
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

    // `skills = [...]` in the default location narrows the set, so an empty
    // list is meaningful: it disables skills without needing a skills_dir.
    let skills_list = raw.skills.unwrap_or_default();
    cfg.skills_dir = match (raw.skills_dir, skills_list.is_empty()) {
        (Some(d), _) if !d.trim().is_empty() => {
            let p = PathBuf::from(&d);
            Some(if p.is_absolute() { p } else { config_dir().join(p) })
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
        assert!(cfg.api_key_env.is_none(), "empty string must disable the key, not look up \"\"");
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
    fn base_prompt_names_both_tools() {
        assert!(BASE_SYSTEM_PROMPT.contains("`bash`"));
        assert!(BASE_SYSTEM_PROMPT.contains("`state`"));
    }

    #[test]
    fn base_prompt_is_deliberately_small() {
        // Roughly 3.5 chars/token. A prompt that crept past a few hundred
        // tokens would be paid on every single request.
        let approx = BASE_SYSTEM_PROMPT.len() / 4;
        assert!(approx < 600, "base prompt is ~{approx} tokens, expected under 600");
    }
}
