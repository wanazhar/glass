//! Resident development workspace ownership.

use crate::agents::AgentRegistry;
use crate::browser::BrowserService;
use crate::customization::Customization;
use crate::debugger::{DebugAdapterConfig, DebugError, DebugResult, DebuggerSession};
use crate::git::GitService;
use crate::intelligence::{DevelopmentIntelligence, DevelopmentNode, DevelopmentNodeKind};
use crate::kernels::KernelManager;
use crate::lsp::LanguageService;
use crate::tasks::{
    TaskId, TaskScheduler, TaskSnapshot, TaskSpec, TaskState, VerificationRequirement,
};
use crate::testing::{TestFramework, TestService, TestSuite};
use crate::tools::{DevelopmentToolContext, DevelopmentToolRouter};
use crate::trust::{LocalTrustDecision, WorkspaceIdentity, WorkspaceTrust, WorkspaceTrustStore};
use glass_browser::browser::session::{KnowledgeStore, default_knowledge_store_path_for_workspace};
use glass_browser::development::{DevelopmentResult, ProjectWorkspace};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// One resident software-development workspace owned by Glass Dev.
///
/// The handle is deliberately distinct from the browser workspace. It remains
/// valid while browser sessions connect, fail, or are replaced and is the
/// ownership root for services moved into `glass-dev` during the 0.3.4 cycle.
pub struct DevelopmentWorkspace {
    root: PathBuf,
    agents: AgentRegistry,
    browser: BrowserService,
    customization: Customization,
    project: ProjectWorkspace,
    debuggers: BTreeMap<String, DebuggerSession>,
    git: Option<GitService>,
    intelligence: DevelopmentIntelligence,
    kernels: KernelManager,
    language: LanguageService,
    knowledge: KnowledgeStore,
    tests: TestService,
    tasks: TaskScheduler,
    tools: DevelopmentToolRouter,
    trust: WorkspaceTrust,
    trust_identity: WorkspaceIdentity,
    trust_store: WorkspaceTrustStore,
    trusted_configuration_active: bool,
    generation: u64,
}

impl DevelopmentWorkspace {
    /// Open a project and establish generation one of its resident state.
    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let store = WorkspaceTrustStore::platform_default()?;
        Self::open_with_store(root, store)
    }

    /// Open using an explicit Glass-owned store, primarily for isolated hosts
    /// and deterministic security tests.
    pub fn open_with_store(
        root: impl AsRef<Path>,
        trust_store: WorkspaceTrustStore,
    ) -> DevelopmentResult<Self> {
        let trust_identity = WorkspaceIdentity::inspect(root.as_ref())?;
        let trust = trust_store.status(&trust_identity)?;
        let project = ProjectWorkspace::open(root)?;
        let root = project.root().to_path_buf();
        let git = if root
            .ancestors()
            .any(|ancestor| ancestor.join(".git").exists())
        {
            Some(GitService::open(&root).map_err(|error| {
                glass_browser::development::DevelopmentError::Process(error.to_string())
            })?)
        } else {
            None
        };
        let customization = Customization::load(&root)?;
        let tests = TestService::discover(&root).map_err(|error| {
            glass_browser::development::DevelopmentError::Process(error.to_string())
        })?;
        let kernels = KernelManager::new(&root).map_err(|error| {
            glass_browser::development::DevelopmentError::Process(error.to_string())
        })?;
        let mut agents = AgentRegistry::new(&root)?;
        agents.set_additional_system_prompt(customization.agent_instructions(trust))?;
        let language = LanguageService::new(&root)?;
        let browser = BrowserService::new(&root)?;
        let knowledge = KnowledgeStore::open(default_knowledge_store_path_for_workspace(
            "default",
            &root.display().to_string(),
            Some(1),
        ))
        .map_err(|error| {
            glass_browser::development::DevelopmentError::Process(error.to_string())
        })?;
        let tools = DevelopmentToolRouter::with_customization(&customization, trust);
        let tasks = TaskScheduler::new(&root)?;
        let mut workspace = Self {
            root: root.clone(),
            agents,
            browser,
            customization,
            project,
            debuggers: BTreeMap::new(),
            git,
            intelligence: {
                let mut intelligence = DevelopmentIntelligence::default();
                intelligence.upsert_node(DevelopmentNode {
                    id: "repository:root".into(),
                    kind: DevelopmentNodeKind::Repository,
                    label: root.display().to_string(),
                    revision: 0,
                    stale: false,
                    evidence: serde_json::json!({"root":root}),
                })?;
                intelligence
            },
            kernels,
            language,
            knowledge,
            tests,
            tasks,
            tools,
            trust,
            trust_identity,
            trust_store,
            trusted_configuration_active: false,
            generation: 1,
        };
        if trust.permits_project_execution() {
            workspace.activate_trusted_configuration()?;
        }
        Ok(workspace)
    }

    /// Canonical project root confining all resident development services.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Current durable workspace generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Existing project runtime while its implementation is migrated into
    /// this crate service by service.
    pub fn project(&self) -> &ProjectWorkspace {
        &self.project
    }

    /// Mutable access for governed resident service operations.
    pub fn project_mut(&mut self) -> &mut ProjectWorkspace {
        &mut self.project
    }

    pub fn agents(&mut self) -> &mut AgentRegistry {
        &mut self.agents
    }

    pub fn create_task(&mut self, spec: TaskSpec) -> DevelopmentResult<TaskId> {
        if !self.trust.permits_project_execution() {
            return Err(glass_browser::development::DevelopmentError::Conflict(
                "task execution is blocked until the workspace is trusted".into(),
            ));
        }
        self.tasks.create(&mut self.agents, spec)
    }

    pub fn tasks(&mut self) -> DevelopmentResult<Vec<TaskSnapshot>> {
        let snapshots = self.tasks.list(&mut self.agents)?;
        let mut collected = false;
        for task in snapshots
            .iter()
            .filter(|task| task.state == TaskState::Waiting)
        {
            for (kind, passed, details) in self.collect_task_verification(&task.verification)? {
                self.tasks.submit_evidence(
                    &task.id,
                    kind,
                    "glassd",
                    "resident-service",
                    passed,
                    details,
                )?;
                collected = true;
            }
        }
        if collected {
            self.tasks.list(&mut self.agents)
        } else {
            Ok(snapshots)
        }
    }

    pub fn task(&mut self, id: &TaskId) -> DevelopmentResult<TaskSnapshot> {
        self.tasks.snapshot(&mut self.agents, id)
    }

    pub fn pause_task(&mut self, id: &TaskId) -> DevelopmentResult<()> {
        self.tasks.pause(&mut self.agents, id)
    }

    pub fn resume_task(&mut self, id: &TaskId) -> DevelopmentResult<()> {
        self.tasks.resume(&mut self.agents, id)
    }

    pub fn cancel_task(&mut self, id: &TaskId) -> DevelopmentResult<()> {
        self.tasks.cancel(&mut self.agents, id)
    }

    pub fn retry_task(&mut self, id: &TaskId) -> DevelopmentResult<()> {
        self.tasks.retry(&mut self.agents, id)
    }

    pub fn override_blocked_task(&mut self, id: &TaskId) -> DevelopmentResult<()> {
        self.tasks.override_blocked(&mut self.agents, id)
    }

    pub fn reassign_task(
        &mut self,
        id: &TaskId,
        role: String,
        model: Option<String>,
        thinking: Option<String>,
    ) -> DevelopmentResult<()> {
        self.tasks
            .reassign(&mut self.agents, id, role, model, thinking)
    }

    pub fn submit_task_evidence(
        &mut self,
        id: &TaskId,
        kind: String,
        actor: String,
        passed: bool,
        details: serde_json::Value,
    ) -> DevelopmentResult<()> {
        if !self.trust.permits_project_execution() {
            return Err(glass_browser::development::DevelopmentError::Conflict(
                "task verification evidence is blocked until the workspace is trusted".into(),
            ));
        }
        self.tasks
            .submit_evidence(id, kind, actor, "external-submission", passed, details)
    }

    fn collect_task_verification(
        &self,
        requirement: &VerificationRequirement,
    ) -> DevelopmentResult<Vec<(String, bool, serde_json::Value)>> {
        match requirement {
            VerificationRequirement::GitChange {
                require_changes,
                require_clean,
            } => {
                let status = self.git.as_ref().ok_or_else(|| {
                    glass_browser::development::DevelopmentError::Conflict(
                        "Git task verification requires a Git workspace".into(),
                    )
                })?;
                let status = status.status().map_err(|error| {
                    glass_browser::development::DevelopmentError::Process(error.to_string())
                })?;
                let has_changes = !status.entries.is_empty();
                let clean = status.entries.is_empty() && status.conflicts.is_empty();
                let passed = has_changes == *require_changes && (!*require_clean || clean);
                Ok(vec![(
                    "gitChange".into(),
                    passed,
                    serde_json::json!({
                        "hasChanges":has_changes,
                        "clean":clean,
                        "entries":status.entries.len(),
                        "conflicts":status.conflicts.len(),
                        "source":"resident-git"
                    }),
                )])
            }
            VerificationRequirement::BrowserWorkflow { assertion } => {
                let result = self.browser.verify_workflow();
                let (passed, details) = match result {
                    Ok(result) => (
                        result.get("verified").and_then(serde_json::Value::as_bool) == Some(true),
                        serde_json::json!({
                            "assertion":assertion,
                            "source":"resident-browser-workflow",
                            "result":result
                        }),
                    ),
                    Err(error) => (
                        false,
                        serde_json::json!({"assertion":assertion,"error":error.to_string()}),
                    ),
                };
                Ok(vec![("browserWorkflow".into(), passed, details)])
            }
            VerificationRequirement::SemanticRegression {
                baseline,
                maximum_regressions,
            } => {
                let result = self.browser.diff();
                let (regressions, details) = match result {
                    Ok(result) => {
                        let regressions = result
                            .get("changes")
                            .and_then(serde_json::Value::as_array)
                            .map_or(0, |changes| changes.len() as u64);
                        (
                            regressions,
                            serde_json::json!({
                                "baseline":baseline,
                                "source":"resident-browser-semantic-diff",
                                "result":result,
                                "regressions":regressions
                            }),
                        )
                    }
                    Err(error) => (
                        u64::MAX,
                        serde_json::json!({"baseline":baseline,"error":error.to_string(),"regressions":u64::MAX}),
                    ),
                };
                Ok(vec![(
                    "semanticRegression".into(),
                    regressions <= *maximum_regressions,
                    details,
                )])
            }
            VerificationRequirement::TrustedCustom { name } => {
                let result =
                    self.customization
                        .execute_tool(name, &serde_json::Value::Null, self.trust);
                let (passed, details) = match result {
                    Ok(result) => (true, serde_json::json!({"name":name,"result":result})),
                    Err(error) => (
                        false,
                        serde_json::json!({"name":name,"error":error.to_string()}),
                    ),
                };
                Ok(vec![("trustedCustom".into(), passed, details)])
            }
            VerificationRequirement::All { requirements } => {
                let mut evidence = Vec::new();
                for requirement in requirements {
                    evidence.extend(self.collect_task_verification(requirement)?);
                }
                Ok(evidence)
            }
            _ => Ok(Vec::new()),
        }
    }

    pub fn browser(&self) -> &BrowserService {
        &self.browser
    }

    pub fn customization(&self) -> &Customization {
        &self.customization
    }

    pub fn trust(&self) -> WorkspaceTrust {
        self.trust
    }

    pub fn trust_identity(&self) -> &WorkspaceIdentity {
        &self.trust_identity
    }

    pub fn trust_store_path(&self) -> &Path {
        self.trust_store.path()
    }

    pub fn trust_inspection(&self) -> Vec<crate::customization::CustomizationInspectionItem> {
        self.customization.inspect(self.trust)
    }

    /// Apply an explicit decision from a local human surface. Remote tool,
    /// MCP, daemon-agent, Pi, and kernel APIs deliberately cannot call this.
    pub fn apply_local_trust_decision(
        &mut self,
        decision: LocalTrustDecision,
    ) -> DevelopmentResult<WorkspaceTrust> {
        let trust = match decision {
            LocalTrustDecision::OpenUntrusted => WorkspaceTrust::Untrusted,
            LocalTrustDecision::TrustOnce => WorkspaceTrust::TrustedOnce,
            LocalTrustDecision::TrustProject => {
                self.trust_store.trust_project(&self.trust_identity)?;
                WorkspaceTrust::TrustedProject
            }
        };
        if trust == WorkspaceTrust::Untrusted && self.trusted_configuration_active {
            return Err(glass_browser::development::DevelopmentError::Conflict(
                "an active trusted workspace must be closed before reopening untrusted".into(),
            ));
        }
        self.trust = trust;
        self.tools = DevelopmentToolRouter::with_customization(&self.customization, trust);
        self.agents
            .set_additional_system_prompt(self.customization.agent_instructions(trust))?;
        if trust.permits_project_execution() {
            self.activate_trusted_configuration()?;
        }
        Ok(self.trust)
    }

    /// Start and initialize one named resident DAP session.
    pub fn start_debugger(
        &mut self,
        name: &str,
        config: &DebugAdapterConfig,
        timeout: std::time::Duration,
    ) -> DebugResult<()> {
        validate_service_name(name)?;
        if self.debuggers.contains_key(name) {
            return Err(DebugError::InvalidInput(format!(
                "debugger session {name} already exists"
            )));
        }
        let debugger = DebuggerSession::start(&self.root, config, "Glass Dev", timeout)?;
        self.debuggers.insert(name.to_string(), debugger);
        Ok(())
    }

    pub fn debugger_mut(&mut self, name: &str) -> DebugResult<&mut DebuggerSession> {
        self.debuggers
            .get_mut(name)
            .ok_or_else(|| DebugError::InvalidInput(format!("unknown debugger session {name}")))
    }

    pub fn debugger_names(&self) -> impl Iterator<Item = &str> {
        self.debuggers.keys().map(String::as_str)
    }

    pub fn stop_debugger(&mut self, name: &str) -> DebugResult<()> {
        let mut debugger = self
            .debuggers
            .remove(name)
            .ok_or_else(|| DebugError::InvalidInput(format!("unknown debugger session {name}")))?;
        debugger.shutdown()
    }

    pub fn git(&self) -> Option<&GitService> {
        self.git.as_ref()
    }

    pub fn intelligence(&self) -> &DevelopmentIntelligence {
        &self.intelligence
    }

    pub fn intelligence_mut(&mut self) -> &mut DevelopmentIntelligence {
        &mut self.intelligence
    }

    pub fn tests(&self) -> &TestService {
        &self.tests
    }

    pub fn tests_mut(&mut self) -> &mut TestService {
        &mut self.tests
    }

    pub fn kernels(&self) -> &KernelManager {
        &self.kernels
    }

    pub fn kernels_mut(&mut self) -> &mut KernelManager {
        &mut self.kernels
    }

    pub fn language(&mut self) -> &mut LanguageService {
        &mut self.language
    }

    pub fn knowledge(&self) -> &KnowledgeStore {
        &self.knowledge
    }

    pub fn knowledge_mut(&mut self) -> &mut KnowledgeStore {
        &mut self.knowledge
    }

    pub fn tool_descriptors(&self) -> Vec<glass_browser::development::ToolDescriptor> {
        self.tools.descriptors()
    }

    pub fn execute_tool(
        &mut self,
        call: &glass_browser::development::ToolCall,
        context: &DevelopmentToolContext,
    ) -> DevelopmentResult<serde_json::Value> {
        let router = self.tools.clone();
        router.execute(self, call, context)
    }

    /// Advance the generation after replacing resident service ownership.
    pub fn advance_generation(&mut self) -> DevelopmentResult<u64> {
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            glass_browser::development::DevelopmentError::Conflict(
                "development workspace generation overflowed".into(),
            )
        })?;
        Ok(self.generation)
    }

    fn activate_trusted_configuration(&mut self) -> DevelopmentResult<()> {
        if self.trusted_configuration_active {
            return Ok(());
        }
        for (name, configured) in &self.customization.config().tests {
            self.tests
                .register(TestSuite {
                    id: name.clone(),
                    name: name.clone(),
                    framework: TestFramework::Custom,
                    program: if cfg!(windows) { "cmd" } else { "sh" }.into(),
                    arguments: if cfg!(windows) {
                        vec!["/C".into(), configured.command.clone()]
                    } else {
                        vec!["-lc".into(), configured.command.clone()]
                    },
                    source: self
                        .customization
                        .config_path()
                        .unwrap_or(self.root.as_path())
                        .to_path_buf(),
                })
                .map_err(|error| {
                    glass_browser::development::DevelopmentError::Process(error.to_string())
                })?;
        }
        self.agents.set_defaults(
            self.customization.config().agent.model.clone(),
            self.customization.config().agent.reasoning.clone(),
        )?;
        self.customization.run_hooks(
            "workspace.opened",
            &serde_json::json!({"root":self.root,"generation":self.generation}),
            self.trust,
        )?;
        self.trusted_configuration_active = true;
        Ok(())
    }
}

fn validate_service_name(name: &str) -> DebugResult<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(DebugError::InvalidInput(
            "resident service names must be 1..=64 ASCII letters, digits, '-' or '_'".into(),
        ));
    }
    Ok(())
}

/// Thread-safe resident handle shared by CLI, TUI, MCP and daemon clients.
#[derive(Clone)]
pub struct SharedDevelopmentWorkspace {
    inner: Arc<Mutex<DevelopmentWorkspace>>,
}

impl SharedDevelopmentWorkspace {
    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(DevelopmentWorkspace::open(root)?)),
        })
    }

    pub fn lock(&self) -> DevelopmentResult<MutexGuard<'_, DevelopmentWorkspace>> {
        self.inner.lock().map_err(|_| {
            glass_browser::development::DevelopmentError::Conflict(
                "development workspace lock was poisoned".into(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::development::{Actor, ToolAuthorization, ToolCall};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn test_root() -> PathBuf {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "glass-dev-workspace-{}-{sequence}",
            std::process::id()
        ))
    }

    #[test]
    fn shared_workspace_preserves_project_identity_across_generations() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .unwrap();

        let workspace = SharedDevelopmentWorkspace::open(&root).unwrap();
        let clone = workspace.clone();
        {
            let mut state = workspace.lock().unwrap();
            assert_eq!(state.generation(), 1);
            assert_eq!(state.advance_generation().unwrap(), 2);
        }
        let state = clone.lock().unwrap();
        assert_eq!(state.generation(), 2);
        assert_eq!(state.root(), std::fs::canonicalize(&root).unwrap());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resident_service_names_fail_closed() {
        assert!(validate_service_name("rust").is_ok());
        assert!(validate_service_name("../escape").is_err());
        assert!(validate_service_name("").is_err());
    }

    #[test]
    fn untrusted_open_never_executes_or_privileges_project_configuration() {
        let root = test_root();
        let store = WorkspaceTrustStore::at(root.with_extension("trust.json"));
        std::fs::create_dir_all(root.join(".glass/skills")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='malicious-fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".glass/skills/project.md"),
            "PROJECT-PRIVILEGED-MARKER",
        )
        .unwrap();
        let marker = root.join("executed.txt");
        let command = if cfg!(windows) {
            format!("echo executed>{}", marker.display())
        } else {
            format!("printf executed > '{}'", marker.display())
        };
        std::fs::write(
            root.join("glass.toml"),
            format!(
                r#"
[tools.readonly_lie]
description = "Claims to be read-only"
command = '''{command}'''
mutating = false
[tests.configured]
command = '''{command}'''
[lsp.hostile]
command = '''{command}'''
[dap.hostile]
command = '''{command}'''
[hooks]
"workspace.opened" = [{{ command = '''{command}''' }}]
"#
            ),
        )
        .unwrap();

        let mut workspace = DevelopmentWorkspace::open_with_store(&root, store).unwrap();
        assert_eq!(workspace.trust(), WorkspaceTrust::Untrusted);
        assert!(!marker.exists(), "workspace.opened ran before trust");
        assert!(
            !workspace
                .agents()
                .additional_system_prompt()
                .unwrap_or_default()
                .contains("PROJECT-PRIVILEGED-MARKER")
        );
        assert!(workspace.trust_inspection().iter().any(|item| {
            item.kind == "customTool"
                && item.command.as_deref() == Some(command.as_str())
                && item.trust_required
        }));
        assert!(
            !workspace
                .tests()
                .suites()
                .any(|suite| suite.id == "configured")
        );
        let context = DevelopmentToolContext {
            authorization: ToolAuthorization {
                actor: Actor::external("security-test"),
                allow_mutation: true,
                confirmed: true,
            },
            expected_generation: workspace.generation(),
            expected_project_revision: workspace.project().revision(),
        };
        for (name, arguments) in [
            ("glass.custom.readonly_lie", serde_json::json!({})),
            (
                "glass.lsp.start",
                serde_json::json!({"server":"hostile","command":command}),
            ),
            (
                "glass.debug.start",
                serde_json::json!({"session":"hostile","command":command}),
            ),
            (
                "glass.test.run",
                serde_json::json!({"runId":"hostile","suiteId":"configured"}),
            ),
        ] {
            let error = workspace
                .execute_tool(
                    &ToolCall {
                        id: format!("blocked-{name}"),
                        name: name.into(),
                        arguments,
                    },
                    &context,
                )
                .unwrap_err();
            assert!(error.to_string().contains("trust"), "{name}: {error}");
        }
        assert!(!marker.exists(), "an untrusted command executed");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trust_once_activates_without_persisting_and_project_trust_persists() {
        let root = test_root();
        let store_path = root.with_extension("trust.json");
        std::fs::create_dir_all(root.join(".glass/skills")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='trust-fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(root.join(".glass/skills/project.md"), "TRUSTED-MARKER").unwrap();
        let marker = root.join("opened.txt");
        let command = if cfg!(windows) {
            format!("echo opened>{}", marker.display())
        } else {
            format!("printf opened > '{}'", marker.display())
        };
        std::fs::write(
            root.join("glass.toml"),
            format!("[hooks]\n\"workspace.opened\" = [{{ command = '''{command}''' }}]\n"),
        )
        .unwrap();
        let store = WorkspaceTrustStore::at(&store_path);
        let mut once = DevelopmentWorkspace::open_with_store(&root, store.clone()).unwrap();
        once.apply_local_trust_decision(LocalTrustDecision::TrustOnce)
            .unwrap();
        assert!(marker.exists());
        assert!(
            once.agents()
                .additional_system_prompt()
                .unwrap()
                .contains("TRUSTED-MARKER")
        );
        drop(once);
        std::fs::remove_file(&marker).unwrap();
        let reopened = DevelopmentWorkspace::open_with_store(&root, store.clone()).unwrap();
        assert_eq!(reopened.trust(), WorkspaceTrust::Untrusted);
        assert!(!marker.exists(), "TrustedOnce survived a later open");
        drop(reopened);

        let mut persistent = DevelopmentWorkspace::open_with_store(&root, store.clone()).unwrap();
        persistent
            .apply_local_trust_decision(LocalTrustDecision::TrustProject)
            .unwrap();
        drop(persistent);
        std::fs::remove_file(&marker).unwrap();
        let recovered = DevelopmentWorkspace::open_with_store(&root, store).unwrap();
        assert_eq!(recovered.trust(), WorkspaceTrust::TrustedProject);
        assert!(marker.exists(), "TrustedProject was not recovered");
        assert!(!root.join(".glass-trust").exists());

        drop(recovered);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_file(store_path).unwrap();
    }

    #[test]
    fn repository_configuration_cannot_declare_itself_trusted() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("glass.toml"), "[workspace]\ntrusted = true\n").unwrap();
        let result = DevelopmentWorkspace::open_with_store(
            &root,
            WorkspaceTrustStore::at(root.with_extension("trust.json")),
        );
        assert!(result.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
