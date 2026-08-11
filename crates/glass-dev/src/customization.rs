//! Governed project configuration, skills, hooks, commands, and custom tools.

use crate::WorkspaceTrust;
use glass_browser::development::{DevelopmentError, DevelopmentResult, ToolDescriptor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
    pub dap: BTreeMap<String, DapServerConfig>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DapServerConfig {
    pub command: String,
    pub args: Vec<String>,
    pub tcp_address: Option<String>,
    pub connect_timeout_ms: u64,
}

impl Default for DapServerConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
            tcp_address: None,
            connect_timeout_ms: 5_000,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CustomizationAuthority {
    GlassBuiltIn,
    UserGlobal,
    TrustedProject,
    UntrustedProject,
    ExternalClient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CustomizationRisk {
    Static,
    AgentContext,
    Executable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomizationInspectionItem {
    pub kind: String,
    pub name: String,
    pub source: PathBuf,
    pub authority: CustomizationAuthority,
    pub risk: CustomizationRisk,
    pub command: Option<String>,
    pub declared_mutating: Option<bool>,
    pub trust_required: bool,
    pub governance: Option<CustomizationGovernance>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomizationGovernance {
    pub event: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub failure_policy: Option<String>,
    pub input_schema: Option<Value>,
    pub effective_mutating: bool,
    pub latest_execution: Option<CustomizationExecutionEvidence>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomizationExecutionEvidence {
    pub actor: String,
    pub authority: CustomizationAuthority,
    pub started_at_ms: u64,
    pub duration_ms: u64,
    pub success: bool,
    pub ignored_failure: bool,
    pub result_bytes: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct Customization {
    root: PathBuf,
    config_path: Option<PathBuf>,
    config: GlassConfig,
    skills: BTreeMap<String, Skill>,
    latest_executions: Mutex<BTreeMap<String, CustomizationExecutionEvidence>>,
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
            latest_executions: Mutex::new(BTreeMap::new()),
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

    pub fn agent_instructions(&self, trust: WorkspaceTrust) -> Option<String> {
        let active = self
            .skills
            .values()
            .filter(|skill| !skill.project_scoped || trust.permits_project_execution())
            .collect::<Vec<_>>();
        if active.is_empty() {
            return None;
        }
        let mut output =
            String::from("\n\nGlass customization instructions with explicit authority:\n");
        for skill in active {
            let authority = if skill.project_scoped {
                "trusted-project"
            } else {
                "user-global"
            };
            output.push_str(&format!(
                "\n<skill name=\"{}\" authority=\"{}\" source=\"{}\">\n{}\n</skill>\n",
                skill.name,
                authority,
                skill.source.display(),
                skill.instructions
            ));
        }
        Some(output)
    }

    pub fn inspect(&self, trust: WorkspaceTrust) -> Vec<CustomizationInspectionItem> {
        let project_authority = if trust.permits_project_execution() {
            CustomizationAuthority::TrustedProject
        } else {
            CustomizationAuthority::UntrustedProject
        };
        let source = self
            .config_path
            .clone()
            .unwrap_or_else(|| self.root.join("glass.toml"));
        let latest = self
            .latest_executions
            .lock()
            .map(|latest| latest.clone())
            .unwrap_or_default();
        let governance =
            |key: &str,
             event: Option<String>,
             timeout_seconds: Option<u64>,
             failure_policy: Option<String>,
             input_schema: Option<Value>,
             effective_mutating: bool| CustomizationGovernance {
                event,
                timeout_seconds,
                failure_policy,
                input_schema,
                effective_mutating,
                latest_execution: latest.get(key).cloned(),
            };
        let mut items = vec![
            CustomizationInspectionItem {
                kind: "skill".into(),
                name: "glass-runtime-rules".into(),
                source: PathBuf::from("<glass-built-in>"),
                authority: CustomizationAuthority::GlassBuiltIn,
                risk: CustomizationRisk::AgentContext,
                command: None,
                declared_mutating: None,
                trust_required: false,
                governance: None,
            },
            CustomizationInspectionItem {
                kind: "authorityBoundary".into(),
                name: "external-client".into(),
                source: PathBuf::from("<runtime-connection>"),
                authority: CustomizationAuthority::ExternalClient,
                risk: CustomizationRisk::Static,
                command: None,
                declared_mutating: None,
                trust_required: false,
                governance: None,
            },
        ];
        if let Some(name) = self.config.project.name.as_ref() {
            items.push(CustomizationInspectionItem {
                kind: "setting".into(),
                name: "project.name".into(),
                source: source.clone(),
                authority: project_authority,
                risk: CustomizationRisk::Static,
                command: Some(name.clone()),
                declared_mutating: None,
                trust_required: false,
                governance: None,
            });
        }
        if let Some(url) = self.config.browser.url.as_ref() {
            items.push(CustomizationInspectionItem {
                kind: "setting".into(),
                name: "browser.url".into(),
                source: source.clone(),
                authority: project_authority,
                risk: CustomizationRisk::Static,
                command: Some(url.clone()),
                declared_mutating: None,
                trust_required: false,
                governance: None,
            });
        }
        for skill in self.skills.values() {
            items.push(CustomizationInspectionItem {
                kind: "skill".into(),
                name: skill.name.clone(),
                source: skill.source.clone(),
                authority: if skill.project_scoped {
                    project_authority
                } else {
                    CustomizationAuthority::UserGlobal
                },
                risk: CustomizationRisk::AgentContext,
                command: None,
                declared_mutating: None,
                trust_required: skill.project_scoped,
                governance: None,
            });
        }
        for (name, command) in &self.config.commands {
            items.push(executable_item(
                "command",
                name,
                command,
                &source,
                project_authority,
                None,
                governance(
                    &format!("command:{name}"),
                    None,
                    Some(15 * 60),
                    Some("fail".into()),
                    Some(serde_json::json!({"type":"object"})),
                    true,
                ),
            ));
        }
        for (name, tool) in &self.config.tools {
            items.push(executable_item(
                "customTool",
                name,
                &tool.command,
                &source,
                project_authority,
                Some(tool.mutating),
                governance(
                    &format!("customTool:{name}"),
                    None,
                    Some(tool.timeout_seconds),
                    Some("fail".into()),
                    Some(tool.input_schema.clone()),
                    true,
                ),
            ));
        }
        for (event, hooks) in &self.config.hooks {
            for (index, hook) in hooks.iter().enumerate() {
                items.push(executable_item(
                    "hook",
                    &format!("{event}[{index}]"),
                    &hook.command,
                    &source,
                    project_authority,
                    Some(true),
                    governance(
                        &format!("hook:{event}[{index}]"),
                        Some(event.clone()),
                        Some(hook.timeout_seconds),
                        Some(
                            if hook.fail_on_error {
                                "fail"
                            } else {
                                "continue"
                            }
                            .into(),
                        ),
                        Some(serde_json::json!({"type":"object"})),
                        true,
                    ),
                ));
            }
        }
        for (name, test) in &self.config.tests {
            items.push(executable_item(
                "test",
                name,
                &test.command,
                &source,
                project_authority,
                Some(true),
                governance(
                    &format!("test:{name}"),
                    None,
                    test.timeout_seconds,
                    Some("fail".into()),
                    None,
                    true,
                ),
            ));
        }
        for (name, server) in &self.config.lsp {
            let command = format!("{} {}", server.command, server.args.join(" "));
            items.push(executable_item(
                "lsp",
                name,
                command.trim(),
                &source,
                project_authority,
                Some(true),
                governance(
                    &format!("lsp:{name}"),
                    None,
                    None,
                    Some("fail".into()),
                    None,
                    true,
                ),
            ));
        }
        for (name, server) in &self.config.dap {
            let command = format!("{} {}", server.command, server.args.join(" "));
            items.push(executable_item(
                "dap",
                name,
                command.trim(),
                &source,
                project_authority,
                Some(true),
                governance(
                    &format!("dap:{name}"),
                    None,
                    None,
                    Some("fail".into()),
                    None,
                    true,
                ),
            ));
        }
        items
    }

    pub fn custom_tool(&self, name: &str) -> Option<&CustomToolConfig> {
        name.strip_prefix("glass.custom.")
            .and_then(|name| self.config.tools.get(name))
    }

    pub fn command(&self, name: &str) -> Option<&str> {
        self.config.commands.get(name).map(String::as_str)
    }

    pub fn descriptors(&self, trust: WorkspaceTrust) -> Vec<ToolDescriptor> {
        let available = trust.permits_project_execution();
        let unavailable_reason = (!available)
            .then(|| "repository shell commands require explicit workspace trust".to_string());
        self.config
            .tools
            .iter()
            .map(|(name, tool)| ToolDescriptor {
                name: format!("glass.custom.{name}"),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
                // Shell-backed project tools can mutate regardless of their
                // declaration, so the effective router policy always requires
                // mutation authority and confirmation.
                mutating: true,
                available,
                unavailable_reason: unavailable_reason.clone(),
            })
            .chain(self.config.commands.keys().map(|name| ToolDescriptor {
                name: format!("glass.command.{name}"),
                description: format!("Configured Glass project command {name}"),
                input_schema: serde_json::json!({"type":"object"}),
                mutating: true,
                available,
                unavailable_reason: unavailable_reason.clone(),
            }))
            .collect()
    }

    pub fn execute_tool(
        &self,
        name: &str,
        arguments: &Value,
        trust: WorkspaceTrust,
        actor: &str,
    ) -> DevelopmentResult<Value> {
        require_project_trust(trust)?;
        let tool = self
            .custom_tool(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("custom tool {name}")))?;
        validate_schema(&tool.input_schema, arguments)?;
        let started_at_ms = now_ms();
        let started = Instant::now();
        let result = run_bounded_command(
            &self.root,
            &tool.command,
            Duration::from_secs(tool.timeout_seconds),
            "GLASS_TOOL_INPUT",
            arguments,
        );
        self.record_execution(
            &format!("customTool:{}", name.trim_start_matches("glass.custom.")),
            actor,
            started_at_ms,
            started.elapsed(),
            &result,
            false,
        );
        result
    }

    pub fn execute_command(
        &self,
        name: &str,
        trust: WorkspaceTrust,
        actor: &str,
    ) -> DevelopmentResult<Value> {
        require_project_trust(trust)?;
        let command = self
            .command(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("project command {name}")))?;
        let started_at_ms = now_ms();
        let started = Instant::now();
        let result = run_bounded_command(
            &self.root,
            command,
            Duration::from_secs(15 * 60),
            "GLASS_COMMAND_INPUT",
            &Value::Null,
        );
        self.record_execution(
            &format!("command:{name}"),
            actor,
            started_at_ms,
            started.elapsed(),
            &result,
            false,
        );
        result
    }

    pub fn run_hooks(
        &self,
        event: &str,
        evidence: &Value,
        trust: WorkspaceTrust,
        actor: &str,
    ) -> DevelopmentResult<Vec<Value>> {
        require_project_trust(trust)?;
        validate_hook_event(event)?;
        let mut results = Vec::new();
        for (index, hook) in self
            .config
            .hooks
            .get(event)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let started_at_ms = now_ms();
            let started = Instant::now();
            let result = run_bounded_command(
                &self.root,
                &hook.command,
                Duration::from_secs(hook.timeout_seconds),
                "GLASS_HOOK_EVENT",
                &serde_json::json!({"event":event,"evidence":evidence}),
            );
            self.record_execution(
                &format!("hook:{event}[{index}]"),
                actor,
                started_at_ms,
                started.elapsed(),
                &result,
                result.is_err() && !hook.fail_on_error,
            );
            match result {
                Ok(result) => results.push(result),
                Err(error) if !hook.fail_on_error => results.push(serde_json::json!({
                    "ok":false,"ignored":true,"error":error.to_string()
                })),
                Err(error) => return Err(error),
            }
        }
        Ok(results)
    }

    fn record_execution(
        &self,
        key: &str,
        actor: &str,
        started_at_ms: u64,
        duration: Duration,
        result: &DevelopmentResult<Value>,
        ignored_failure: bool,
    ) {
        let (success, result_bytes, error) = match result {
            Ok(value) => (
                true,
                serde_json::to_vec(value).ok().map(|bytes| bytes.len()),
                None,
            ),
            Err(error) => (false, None, Some(error.to_string())),
        };
        if let Ok(mut latest) = self.latest_executions.lock() {
            latest.insert(
                key.to_string(),
                CustomizationExecutionEvidence {
                    actor: actor.to_string(),
                    authority: CustomizationAuthority::TrustedProject,
                    started_at_ms,
                    duration_ms: duration.as_millis() as u64,
                    success,
                    ignored_failure,
                    result_bytes,
                    error,
                },
            );
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn require_project_trust(trust: WorkspaceTrust) -> DevelopmentResult<()> {
    if trust.permits_project_execution() {
        Ok(())
    } else {
        Err(DevelopmentError::Conflict(
            "repository-controlled execution is blocked until the workspace is trusted".into(),
        ))
    }
}

fn executable_item(
    kind: &str,
    name: &str,
    command: &str,
    source: &Path,
    authority: CustomizationAuthority,
    declared_mutating: Option<bool>,
    governance: CustomizationGovernance,
) -> CustomizationInspectionItem {
    CustomizationInspectionItem {
        kind: kind.into(),
        name: name.into(),
        source: source.to_path_buf(),
        authority,
        risk: CustomizationRisk::Executable,
        command: Some(command.into()),
        declared_mutating,
        trust_required: true,
        governance: Some(governance),
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
            format!("{}:{name}", if project_scoped { "project" } else { "user" }),
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
        let echo_input = if cfg!(windows) {
            "echo %GLASS_TOOL_INPUT%"
        } else {
            "printf '%s' \"$GLASS_TOOL_INPUT\""
        };
        let echo_text = if cfg!(windows) {
            "echo command-ok"
        } else {
            "printf command-ok"
        };
        let configured_test = if cfg!(windows) {
            "echo configured-test"
        } else {
            "printf configured-test"
        };
        let hook = if cfg!(windows) {
            "echo hook-ok"
        } else {
            "printf hook-ok"
        };
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
            format!(
                r#"
[agent]
harness = "pi"
model = "provider/model"
reasoning = "high"
[commands]
hello = "{echo_text}"
[tests.smoke]
command = "{configured_test}"
timeout_seconds = 30
[tools.echo]
description = "Echo structured input"
command = '''{echo_input}'''
input_schema = {{ type = "object", required = ["text"] }}
[hooks]
"tool.before" = [{{ command = "{hook}" }}]
"#,
            ),
        )
        .unwrap();
        let customization = Customization::load(&root).unwrap();
        assert!(
            customization
                .agent_instructions(WorkspaceTrust::TrustedOnce)
                .unwrap()
                .contains("Always run tests")
        );
        assert_eq!(
            customization
                .execute_tool(
                    "glass.custom.echo",
                    &serde_json::json!({"text":"ok"}),
                    WorkspaceTrust::TrustedOnce,
                    "external:customization-test",
                )
                .unwrap()["text"],
            "ok"
        );
        assert_eq!(
            customization
                .run_hooks(
                    "tool.before",
                    &Value::Null,
                    WorkspaceTrust::TrustedOnce,
                    "external:customization-test"
                )
                .unwrap()
                .len(),
            1
        );
        assert!(
            customization
                .execute_command(
                    "hello",
                    WorkspaceTrust::TrustedOnce,
                    "external:customization-test"
                )
                .unwrap()["stdout"]
                .as_str()
                .unwrap()
                .contains("command-ok")
        );
        let mut workspace = crate::DevelopmentWorkspace::open(&root).unwrap();
        workspace
            .apply_local_trust_decision(crate::LocalTrustDecision::TrustOnce)
            .unwrap();
        assert!(workspace.tests().suites().any(|suite| {
            suite.id == "smoke"
                && suite.framework == crate::testing::TestFramework::Custom
                && suite
                    .arguments
                    .iter()
                    .any(|argument| argument == configured_test)
        }));
        assert!(
            workspace
                .tool_descriptors()
                .iter()
                .any(|descriptor| descriptor.name == "glass.custom.echo"
                    && descriptor.available
                    && descriptor.mutating)
        );
        let context = crate::DevelopmentToolContext {
            authorization: glass_browser::development::ToolAuthorization {
                actor: glass_browser::development::Actor::external("customization-test"),
                allow_mutation: true,
                confirmed: true,
            },
            initiator: None,
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
        let inspection = workspace.trust_inspection();
        let tool = inspection
            .iter()
            .find(|item| item.kind == "customTool" && item.name == "echo")
            .unwrap();
        assert_eq!(tool.declared_mutating, Some(false));
        let governance = tool.governance.as_ref().unwrap();
        assert!(governance.effective_mutating);
        assert_eq!(governance.timeout_seconds, Some(30));
        assert_eq!(governance.failure_policy.as_deref(), Some("fail"));
        assert!(governance.input_schema.is_some());
        let latest = governance.latest_execution.as_ref().unwrap();
        assert_eq!(latest.actor, "external:customization-test");
        assert!(latest.success);
        assert!(inspection.iter().any(|item| {
            item.kind == "skill"
                && item.authority == CustomizationAuthority::GlassBuiltIn
                && item.name == "glass-runtime-rules"
        }));
        assert!(inspection.iter().any(|item| {
            item.kind == "authorityBoundary"
                && item.authority == CustomizationAuthority::ExternalClient
        }));
        let hook = inspection
            .iter()
            .find(|item| item.kind == "hook" && item.name == "tool.before[0]")
            .unwrap();
        let hook_governance = hook.governance.as_ref().unwrap();
        assert_eq!(hook_governance.event.as_deref(), Some("tool.before"));
        assert_eq!(hook_governance.failure_policy.as_deref(), Some("fail"));
        assert_eq!(
            hook_governance.latest_execution.as_ref().unwrap().actor,
            "external:customization-test"
        );
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
