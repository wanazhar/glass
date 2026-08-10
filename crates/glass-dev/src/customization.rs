//! Governed project configuration, skills, hooks, commands, and custom tools.

use glass_browser::development::{DevelopmentError, DevelopmentResult, ToolDescriptor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_CONFIG_BYTES: u64 = 512 * 1024;
const MAX_SKILLS: usize = 32;
const MAX_SKILL_BYTES: u64 = 64 * 1024;
const MAX_COMMAND_BYTES: usize = 4096;
const MAX_OUTPUT_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlassConfig {
    pub project: ProjectConfig,
    pub commands: BTreeMap<String, String>,
    pub browser: BrowserConfig,
    pub agent: AgentConfig,
    pub lsp: BTreeMap<String, ServerConfig>,
    pub dap: BTreeMap<String, ServerConfig>,
    pub tests: BTreeMap<String, TestConfig>,
    pub hooks: BTreeMap<String, Vec<HookConfig>>,
    pub tools: BTreeMap<String, CustomToolConfig>,
    pub workspace: WorkspaceConfig,
    pub editor: EditorConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BrowserConfig {
    pub url: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentConfig {
    pub model: Option<String>,
    pub reasoning: Option<String>,
    /// Retained only to reject non-Pi embedded runtimes explicitly.
    pub harness: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EditorConfig {
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub persistent: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TestConfig {
    pub command: String,
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HookConfig {
    pub command: String,
    pub timeout_seconds: u64,
    pub fail_on_error: bool,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            timeout_seconds: 30,
            fail_on_error: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomToolConfig {
    pub description: String,
    pub command: String,
    pub mutating: bool,
    pub timeout_seconds: u64,
    pub input_schema: Value,
}

impl Default for CustomToolConfig {
    fn default() -> Self {
        Self {
            description: String::new(),
            command: String::new(),
            mutating: false,
            timeout_seconds: 30,
            input_schema: serde_json::json!({"type":"object"}),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub source: PathBuf,
    pub instructions: String,
    pub project_scoped: bool,
}

#[derive(Debug, Clone)]
pub struct Customization {
    root: PathBuf,
    config_path: Option<PathBuf>,
    config: GlassConfig,
    skills: BTreeMap<String, Skill>,
}

impl Customization {
    pub fn load(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = std::fs::canonicalize(root)?;
        let config_path = [root.join("glass.toml"), root.join(".glass.toml")]
            .into_iter()
            .find(|path| path.is_file());
        let config = match config_path.as_ref() {
            Some(path) => {
                if path.metadata()?.len() > MAX_CONFIG_BYTES {
                    return Err(DevelopmentError::Config(format!(
                        "{} exceeds the {MAX_CONFIG_BYTES} byte limit",
                        path.display()
                    )));
                }
                toml::from_str::<GlassConfig>(&std::fs::read_to_string(path)?)
                    .map_err(|error| DevelopmentError::Config(error.to_string()))?
            }
            None => GlassConfig::default(),
        };
        validate_config(&config)?;
        let mut skills = BTreeMap::new();
        if let Some(config_dir) = dirs::config_dir() {
            load_skill_dir(&config_dir.join("glass/skills"), false, &mut skills)?;
        }
        load_skill_dir(&root.join(".glass/skills"), true, &mut skills)?;
        Ok(Self {
            root,
            config_path,
            config,
            skills,
        })
    }

    pub fn config(&self) -> &GlassConfig {
        &self.config
    }

    pub fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }

    pub fn skills(&self) -> impl Iterator<Item = &Skill> {
        self.skills.values()
    }

    pub fn agent_instructions(&self) -> Option<String> {
        if self.skills.is_empty() {
            return None;
        }
        let mut output = String::from("\n\nProject and user skills supplied by Glass authority:\n");
        for skill in self.skills.values() {
            output.push_str(&format!(
                "\n<skill name=\"{}\">\n{}\n</skill>\n",
                skill.name, skill.instructions
            ));
        }
        Some(output)
    }

    pub fn custom_tool(&self, name: &str) -> Option<&CustomToolConfig> {
        name.strip_prefix("glass.custom.")
            .and_then(|name| self.config.tools.get(name))
    }

    pub fn command(&self, name: &str) -> Option<&str> {
        self.config.commands.get(name).map(String::as_str)
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.config
            .tools
            .iter()
            .map(|(name, tool)| ToolDescriptor {
                name: format!("glass.custom.{name}"),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                mutating: tool.mutating,
                available: true,
                unavailable_reason: None,
            })
            .chain(self.config.commands.keys().map(|name| ToolDescriptor {
                name: format!("glass.command.{name}"),
                description: format!("Configured Glass project command {name}"),
                input_schema: serde_json::json!({"type":"object"}),
                mutating: true,
                available: true,
                unavailable_reason: None,
            }))
            .collect()
    }

    pub fn execute_tool(&self, name: &str, arguments: &Value) -> DevelopmentResult<Value> {
        let tool = self
            .custom_tool(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("custom tool {name}")))?;
        validate_schema(&tool.input_schema, arguments)?;
        run_bounded_command(
            &self.root,
            &tool.command,
            Duration::from_secs(tool.timeout_seconds),
            "GLASS_TOOL_INPUT",
            arguments,
        )
    }

    pub fn execute_command(&self, name: &str) -> DevelopmentResult<Value> {
        let command = self
            .command(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("project command {name}")))?;
        run_bounded_command(
            &self.root,
            command,
            Duration::from_secs(15 * 60),
            "GLASS_COMMAND_INPUT",
            &Value::Null,
        )
    }

    pub fn run_hooks(&self, event: &str, evidence: &Value) -> DevelopmentResult<Vec<Value>> {
        validate_hook_event(event)?;
        let mut results = Vec::new();
        for hook in self.config.hooks.get(event).into_iter().flatten() {
            match run_bounded_command(
                &self.root,
                &hook.command,
                Duration::from_secs(hook.timeout_seconds),
                "GLASS_HOOK_EVENT",
                &serde_json::json!({"event":event,"evidence":evidence}),
            ) {
                Ok(result) => results.push(result),
                Err(error) if !hook.fail_on_error => results.push(serde_json::json!({
                    "ok":false,"ignored":true,"error":error.to_string()
                })),
                Err(error) => return Err(error),
            }
        }
        Ok(results)
    }
}

fn validate_config(config: &GlassConfig) -> DevelopmentResult<()> {
    if config
        .agent
        .harness
        .as_deref()
        .is_some_and(|harness| harness != "pi")
    {
        return Err(DevelopmentError::Config(
            "Pi is the sole embedded Glass Agent runtime; agent.harness must be 'pi' or omitted"
                .into(),
        ));
    }
    for (name, command) in &config.commands {
        validate_name(name)?;
        validate_command(command)?;
    }
    for (name, tool) in &config.tools {
        validate_name(name)?;
        validate_command(&tool.command)?;
        if tool.description.is_empty() || tool.description.len() > 1024 {
            return Err(DevelopmentError::Config(format!(
                "custom tool {name} requires a bounded description"
            )));
        }
        validate_timeout(tool.timeout_seconds)?;
        if serde_json::to_vec(&tool.input_schema)?.len() > 64 * 1024 {
            return Err(DevelopmentError::Config(format!(
                "custom tool {name} schema exceeds 64 KiB"
            )));
        }
    }
    for (event, hooks) in &config.hooks {
        validate_hook_event(event)?;
        if hooks.len() > 16 {
            return Err(DevelopmentError::Config(format!(
                "hook event {event} exceeds 16 commands"
            )));
        }
        for hook in hooks {
            validate_command(&hook.command)?;
            validate_timeout(hook.timeout_seconds)?;
        }
    }
    Ok(())
}

fn validate_name(name: &str) -> DevelopmentResult<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(DevelopmentError::Config(format!(
            "invalid customization name {name}"
        )));
    }
    Ok(())
}

fn validate_command(command: &str) -> DevelopmentResult<()> {
    if command.trim().is_empty() || command.len() > MAX_COMMAND_BYTES || command.contains('\0') {
        return Err(DevelopmentError::Config(
            "customization commands must be 1..=4096 bytes without NUL".into(),
        ));
    }
    Ok(())
}

fn validate_timeout(seconds: u64) -> DevelopmentResult<()> {
    if !(1..=3600).contains(&seconds) {
        return Err(DevelopmentError::Config(
            "customization timeout must be 1..=3600 seconds".into(),
        ));
    }
    Ok(())
}

fn validate_hook_event(event: &str) -> DevelopmentResult<()> {
    const EVENTS: &[&str] = &[
        "workspace.opened",
        "workspace.closed",
        "agent.started",
        "agent.settled",
        "agent.failed",
        "tool.before",
        "tool.after",
        "file.saved",
        "process.started",
        "process.failed",
        "test.completed",
        "debugger.stopped",
        "browser.revision",
        "workflow.completed",
        "git.changed",
        "experiment.completed",
    ];
    EVENTS
        .contains(&event)
        .then_some(())
        .ok_or_else(|| DevelopmentError::Config(format!("unsupported hook event {event}")))
}

fn load_skill_dir(
    directory: &Path,
    project_scoped: bool,
    skills: &mut BTreeMap<String, Skill>,
) -> DevelopmentResult<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    let mut entries = std::fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        if skills.len() >= MAX_SKILLS || path.metadata()?.len() > MAX_SKILL_BYTES {
            return Err(DevelopmentError::Config(
                "skills exceed the 32 file or 64 KiB per-file limit".into(),
            ));
        }
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| DevelopmentError::Config("skill name is not UTF-8".into()))?;
        validate_name(name)?;
        let instructions = std::fs::read_to_string(&path)?;
        if instructions.contains('\0') {
            return Err(DevelopmentError::Config(format!(
                "skill {name} contains NUL"
            )));
        }
        skills.insert(
            name.into(),
            Skill {
                name: name.into(),
                source: path,
                instructions,
                project_scoped,
            },
        );
    }
    Ok(())
}

fn validate_schema(schema: &Value, arguments: &Value) -> DevelopmentResult<()> {
    if schema.get("type").and_then(Value::as_str) != Some("object") || !arguments.is_object() {
        return Err(DevelopmentError::InvalidInput(
            "custom tool schema and arguments must be objects".into(),
        ));
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required {
            let field = field.as_str().ok_or_else(|| {
                DevelopmentError::Config("custom tool required fields must be strings".into())
            })?;
            if arguments.get(field).is_none() {
                return Err(DevelopmentError::InvalidInput(format!(
                    "custom tool requires argument {field}"
                )));
            }
        }
    }
    if serde_json::to_vec(arguments)?.len() > 256 * 1024 {
        return Err(DevelopmentError::InvalidInput(
            "custom tool arguments exceed 256 KiB".into(),
        ));
    }
    Ok(())
}

fn run_bounded_command(
    root: &Path,
    command: &str,
    timeout: Duration,
    input_name: &str,
    input: &Value,
) -> DevelopmentResult<Value> {
    validate_command(command)?;
    if timeout.is_zero() || timeout > Duration::from_secs(3600) {
        return Err(DevelopmentError::InvalidInput(
            "command timeout must be between 1 ms and 1 hour".into(),
        ));
    }
    let mut process = if cfg!(windows) {
        let mut process = Command::new("cmd.exe");
        process.args(["/D", "/Q", "/C", command]);
        process
    } else {
        let mut process = Command::new("/bin/sh");
        process.args(["-c", command]);
        process
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: this async-signal-safe call only creates an owned process group.
        unsafe {
            process.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = process
        .current_dir(root)
        .env(input_name, serde_json::to_string(input)?)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| DevelopmentError::Process("custom command stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| DevelopmentError::Process("custom command stderr unavailable".into()))?;
    let stdout = thread::spawn(move || read_bounded(stdout));
    let stderr = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            #[cfg(unix)]
            // SAFETY: the child was placed in a fresh process group above.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            #[cfg(not(unix))]
            child.kill()?;
            let _ = child.wait();
            return Err(DevelopmentError::Process(format!(
                "custom command exceeded {} seconds",
                timeout.as_secs()
            )));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let (stdout, stdout_truncated) = stdout
        .join()
        .map_err(|_| DevelopmentError::Process("custom stdout reader panicked".into()))??;
    let (stderr, stderr_truncated) = stderr
        .join()
        .map_err(|_| DevelopmentError::Process("custom stderr reader panicked".into()))??;
    if !status.success() {
        return Err(DevelopmentError::Process(format!(
            "custom command exited {:?}: {}",
            status.code(),
            stderr.trim()
        )));
    }
    if let Ok(value) = serde_json::from_str::<Value>(&stdout) {
        return Ok(value);
    }
    Ok(serde_json::json!({
        "exitCode":status.code(),
        "stdout":stdout,
        "stderr":stderr,
        "truncated":stdout_truncated || stderr_truncated
    }))
}

fn read_bounded(mut reader: impl Read) -> DevelopmentResult<(String, bool)> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(MAX_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > MAX_OUTPUT_BYTES;
    bytes.truncate(MAX_OUTPUT_BYTES as usize);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customization_loads_skills_and_executes_governed_tools_and_hooks() {
        let root = std::env::temp_dir().join(format!("glass-customization-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".glass/skills")).unwrap();
        std::fs::write(root.join(".glass/skills/review.md"), "Always run tests.").unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='customization-fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("glass.toml"),
            r#"
[agent]
harness = "pi"
[commands]
hello = "printf command-ok"
[tools.echo]
description = "Echo structured input"
command = "printf '%s' \"$GLASS_TOOL_INPUT\""
input_schema = { type = "object", required = ["text"] }
[hooks]
"tool.before" = [{ command = "printf hook-ok" }]
"#,
        )
        .unwrap();
        let customization = Customization::load(&root).unwrap();
        assert!(
            customization
                .agent_instructions()
                .unwrap()
                .contains("Always run tests")
        );
        assert_eq!(
            customization
                .execute_tool("glass.custom.echo", &serde_json::json!({"text":"ok"}))
                .unwrap()["text"],
            "ok"
        );
        assert_eq!(
            customization
                .run_hooks("tool.before", &Value::Null)
                .unwrap()
                .len(),
            1
        );
        assert!(
            customization.execute_command("hello").unwrap()["stdout"]
                .as_str()
                .unwrap()
                .contains("command-ok")
        );
        let mut workspace = crate::DevelopmentWorkspace::open(&root).unwrap();
        assert!(
            workspace
                .tool_descriptors()
                .iter()
                .any(|descriptor| descriptor.name == "glass.custom.echo" && descriptor.available)
        );
        let context = crate::DevelopmentToolContext {
            authorization: glass_browser::development::ToolAuthorization {
                actor: glass_browser::development::Actor::external("customization-test"),
                allow_mutation: false,
                confirmed: false,
            },
            expected_generation: workspace.generation(),
            expected_project_revision: workspace.project().revision(),
        };
        let result = workspace
            .execute_tool(
                &glass_browser::development::ToolCall {
                    id: "custom-1".into(),
                    name: "glass.custom.echo".into(),
                    arguments: serde_json::json!({"text":"resident"}),
                },
                &context,
            )
            .unwrap();
        assert_eq!(result["text"], "resident");
        drop(workspace);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn customization_rejects_alternate_embedded_harnesses() {
        let mut config = GlassConfig::default();
        config.agent.harness = Some("omp".into());
        assert!(validate_config(&config).is_err());
    }
}
