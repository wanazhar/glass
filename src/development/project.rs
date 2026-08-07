use super::{
    Actor, DevelopmentError, DevelopmentEventKind, DevelopmentGraph, DevelopmentResult,
    MAX_BUFFER_BYTES, MAX_FILE_BYTES, MAX_FILE_ENTRIES, ProcessManager, SourceLocation, Timeline,
};
use crate::development::diff::{ProjectDiff, build_diff};
use crate::development::graph::{LinkEvidence, LinkProvenance, RuntimeLink};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlassProjectConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub commands: CommandConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandConfig {
    #[serde(default)]
    pub dev: Option<String>,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub lint: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserConfig {
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorConfig {
    #[serde(default)]
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfig {
    #[serde(default)]
    pub harness: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetection {
    pub root: PathBuf,
    pub languages: Vec<String>,
    pub package_manager: Option<String>,
    pub framework: Option<String>,
    pub git_branch: Option<String>,
    pub dev_command: Option<String>,
    pub test_command: Option<String>,
    pub lint_command: Option<String>,
    pub build_command: Option<String>,
    pub browser_url: Option<String>,
    pub editor_engine: Option<String>,
    pub agent_harness: Option<String>,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub kind: FileKind,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditorBuffer {
    pub path: String,
    pub content: String,
    pub original_hash: String,
    pub dirty: bool,
    pub cursor_line: u32,
    pub cursor_column: u32,
    pub actor: Actor,
}

pub struct ProjectWorkspace {
    root: PathBuf,
    detection: ProjectDetection,
    config: GlassProjectConfig,
    buffers: BTreeMap<String, EditorBuffer>,
    processes: ProcessManager,
    timeline: Timeline,
    graph: DevelopmentGraph,
    actor: Actor,
    revision: u64,
}

impl std::fmt::Debug for ProjectWorkspace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectWorkspace")
            .field("root", &self.root)
            .field("detection", &self.detection)
            .field("buffers", &self.buffers.keys().collect::<Vec<_>>())
            .field("revision", &self.revision)
            .finish()
    }
}

impl ProjectWorkspace {
    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = canonical_root(root.as_ref())?;
        let detection = detect_project(&root)?;
        let config = detection
            .config_path
            .as_deref()
            .map(fs::read_to_string)
            .transpose()?
            .map(|text| {
                toml::from_str::<GlassProjectConfig>(&text)
                    .map_err(|error| DevelopmentError::Config(error.to_string()))
            })
            .transpose()?
            .unwrap_or_default();
        let timeline = Timeline::for_project(&root)?;
        let mut workspace = Self {
            root: root.clone(),
            detection,
            config,
            buffers: BTreeMap::new(),
            processes: ProcessManager::new(&root),
            timeline,
            graph: DevelopmentGraph::default(),
            actor: Actor::local(),
            revision: 0,
        };
        let _ = workspace.record(
            DevelopmentEventKind::WorkspaceOpened,
            serde_json::json!({"root": root}),
        );
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn detection(&self) -> &ProjectDetection {
        &self.detection
    }

    pub fn config(&self) -> &GlassProjectConfig {
        &self.config
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    pub fn graph(&self) -> &DevelopmentGraph {
        &self.graph
    }

    pub fn processes(&mut self) -> &mut ProcessManager {
        &mut self.processes
    }

    pub fn list_files(&self) -> DevelopmentResult<Vec<FileEntry>> {
        let mut entries = Vec::new();
        visit_files(&self.root, &self.root, &mut entries)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    pub fn read_file(&mut self, path: &str) -> DevelopmentResult<String> {
        let (absolute, relative) = self.resolve_path(path, false)?;
        let metadata = fs::metadata(&absolute)?;
        if !metadata.is_file() {
            return Err(DevelopmentError::InvalidInput(format!(
                "not a regular file: {path}"
            )));
        }
        if metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(DevelopmentError::InvalidInput(format!(
                "file exceeds the {} byte read limit: {path}",
                MAX_FILE_BYTES
            )));
        }
        let content = fs::read_to_string(&absolute)?;
        self.record(
            DevelopmentEventKind::FileOpened,
            serde_json::json!({"path": relative}),
        )?;
        Ok(content)
    }

    pub fn open_buffer(&mut self, path: &str, actor: Actor) -> DevelopmentResult<EditorBuffer> {
        let content = self.read_file(path)?;
        let (_, relative) = self.resolve_path(path, false)?;
        let buffer = EditorBuffer {
            path: relative.clone(),
            original_hash: hash(&content),
            content,
            dirty: false,
            cursor_line: 1,
            cursor_column: 1,
            actor,
        };
        self.buffers.insert(relative, buffer.clone());
        Ok(buffer)
    }

    pub fn buffers(&self) -> impl Iterator<Item = &EditorBuffer> {
        self.buffers.values()
    }

    pub fn buffer(&self, path: &str) -> Option<&EditorBuffer> {
        self.buffers.get(path)
    }

    pub fn edit_buffer(
        &mut self,
        path: &str,
        content: String,
        actor: Actor,
    ) -> DevelopmentResult<()> {
        if content.len() > MAX_BUFFER_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "buffer exceeds the {} byte limit",
                MAX_BUFFER_BYTES
            )));
        }
        if !self.buffers.contains_key(path) {
            self.open_buffer(path, actor.clone())?;
        }
        let existing = read_existing(&self.root, path)?;
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        buffer.content = content;
        buffer.dirty = buffer.content != existing;
        buffer.actor = actor;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn save_buffer(&mut self, path: &str) -> DevelopmentResult<EditorBuffer> {
        let buffer = self
            .buffers
            .get(path)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        self.write_file(path, &buffer.content, buffer.actor.clone())?;
        let mut saved = buffer;
        saved.original_hash = hash(&saved.content);
        saved.dirty = false;
        self.buffers.insert(path.into(), saved.clone());
        Ok(saved)
    }

    pub fn write_file(&mut self, path: &str, content: &str, actor: Actor) -> DevelopmentResult<()> {
        if content.len() > MAX_FILE_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "file exceeds the {} byte write limit",
                MAX_FILE_BYTES
            )));
        }
        let (absolute, relative) = self.resolve_path(path, true)?;
        let parent = absolute
            .parent()
            .ok_or_else(|| DevelopmentError::InvalidInput("file has no parent".into()))?;
        fs::create_dir_all(parent)?;
        let temporary = absolute.with_extension(format!("glass-tmp-{}", std::process::id()));
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }
        if let Err(error) = fs::rename(&temporary, &absolute) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::FileSaved,
            serde_json::json!({"path": relative, "bytes": content.len()}),
        )?;
        Ok(())
    }

    pub fn start_process(
        &mut self,
        name: &str,
        command: &str,
    ) -> DevelopmentResult<super::ProcessSnapshot> {
        let snapshot = self.processes.start(name, command)?;
        self.revision = self.revision.saturating_add(1);
        self.record(
            DevelopmentEventKind::ProcessStarted,
            serde_json::json!({"name": name, "command": command, "pid": snapshot.pid}),
        )?;
        Ok(snapshot)
    }

    pub fn stop_process(&mut self, name: &str) -> DevelopmentResult<super::ProcessSnapshot> {
        let snapshot = self.processes.stop(name)?;
        self.revision = self.revision.saturating_add(1);
        self.record(
            DevelopmentEventKind::ProcessExited,
            serde_json::json!({"name": name, "state": snapshot.state}),
        )?;
        Ok(snapshot)
    }

    pub fn link_runtime_source(
        &mut self,
        entity_id: &str,
        path: &str,
        start_line: u32,
        end_line: u32,
        provenance: LinkProvenance,
        detail: &str,
        confidence: f32,
        actor: Actor,
    ) -> DevelopmentResult<RuntimeLink> {
        let (_, relative) = self.resolve_path(path, false)?;
        let link = RuntimeLink {
            entity_id: entity_id.into(),
            source: SourceLocation::new(relative, start_line, end_line)?,
            evidence: LinkEvidence {
                provenance,
                detail: detail.into(),
                confidence,
            },
        };
        self.graph.link(link.clone())?;
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::SourceRuntimeLinked,
            serde_json::to_value(&link)?,
        )?;
        Ok(link)
    }

    pub fn diff(&mut self) -> DevelopmentResult<ProjectDiff> {
        build_diff(&self.root, &self.timeline, &self.graph, &mut self.processes)
    }

    pub fn record(
        &mut self,
        kind: DevelopmentEventKind,
        payload: serde_json::Value,
    ) -> DevelopmentResult<()> {
        self.record_as(self.actor.clone(), kind, payload)
    }

    pub fn record_as(
        &mut self,
        actor: Actor,
        kind: DevelopmentEventKind,
        payload: serde_json::Value,
    ) -> DevelopmentResult<()> {
        self.timeline
            .record(actor, kind, self.root.display().to_string(), payload)?;
        Ok(())
    }

    fn resolve_path(
        &self,
        input: &str,
        allow_missing: bool,
    ) -> DevelopmentResult<(PathBuf, String)> {
        if input.is_empty() || input.len() > 512 {
            return Err(DevelopmentError::InvalidInput(
                "relative path must be 1-512 bytes".into(),
            ));
        }
        let path = Path::new(input);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(DevelopmentError::PathOutsideWorkspace(path.to_path_buf()));
        }
        let candidate = self.root.join(path);
        let parent = candidate
            .parent()
            .ok_or_else(|| DevelopmentError::InvalidInput("path has no parent".into()))?;
        let canonical_parent = fs::canonicalize(parent)?;
        if !canonical_parent.starts_with(&self.root) {
            return Err(DevelopmentError::PathOutsideWorkspace(candidate));
        }
        if candidate.exists() {
            let canonical = fs::canonicalize(&candidate)?;
            if !canonical.starts_with(&self.root) {
                return Err(DevelopmentError::PathOutsideWorkspace(candidate));
            }
        } else if !allow_missing {
            return Err(DevelopmentError::NotFound(input.into()));
        }
        Ok((candidate, path.to_string_lossy().into_owned()))
    }
}

pub fn detect_project(root: impl AsRef<Path>) -> DevelopmentResult<ProjectDetection> {
    let root = canonical_root(root.as_ref())?;
    let config_path = [root.join("glass.toml"), root.join(".glass.toml")]
        .into_iter()
        .find(|path| path.is_file());
    let mut languages = Vec::new();
    if root.join("Cargo.toml").is_file() {
        languages.push("Rust".into());
    }
    if root.join("package.json").is_file() {
        languages.push("JavaScript/TypeScript".into());
    }
    if root.join("pyproject.toml").is_file() || root.join("requirements.txt").is_file() {
        languages.push("Python".into());
    }
    if root.join("go.mod").is_file() {
        languages.push("Go".into());
    }
    let package_manager = if root.join("pnpm-lock.yaml").is_file() {
        Some("pnpm".into())
    } else if root.join("yarn.lock").is_file() {
        Some("yarn".into())
    } else if root.join("bun.lockb").is_file() || root.join("bun.lock").is_file() {
        Some("bun".into())
    } else if root.join("package-lock.json").is_file() {
        Some("npm".into())
    } else if root.join("Cargo.lock").is_file() {
        Some("cargo".into())
    } else {
        None
    };
    let framework = detect_framework(&root);
    let config = config_path
        .as_deref()
        .map(fs::read_to_string)
        .transpose()?
        .map(|text| {
            toml::from_str::<GlassProjectConfig>(&text)
                .map_err(|error| DevelopmentError::Config(error.to_string()))
        })
        .transpose()?
        .unwrap_or_default();
    let defaults = default_commands(&root, package_manager.as_deref());
    Ok(ProjectDetection {
        root: root.clone(),
        languages,
        package_manager,
        framework,
        git_branch: git_branch(&root),
        dev_command: config.commands.dev.or(defaults.0),
        test_command: config.commands.test.or(defaults.1),
        lint_command: config.commands.lint.or(defaults.2),
        build_command: config.commands.build.or(defaults.3),
        browser_url: config.browser.url,
        editor_engine: config.editor.engine,
        agent_harness: config.agent.harness,
        config_path,
    })
}

fn canonical_root(root: &Path) -> DevelopmentResult<PathBuf> {
    let root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root
    };
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(DevelopmentError::InvalidInput(
            "project root must be a directory".into(),
        ));
    }
    Ok(root)
}

fn visit_files(root: &Path, current: &Path, entries: &mut Vec<FileEntry>) -> DevelopmentResult<()> {
    if entries.len() >= MAX_FILE_ENTRIES {
        return Ok(());
    }
    let mut children =
        fs::read_dir(current).map(|entries| entries.collect::<Result<Vec<_>, _>>())??;
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if entries.len() >= MAX_FILE_ENTRIES {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "target" || name == "node_modules" || name == ".glass" {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| DevelopmentError::PathOutsideWorkspace(path.clone()))?
            .to_string_lossy()
            .into_owned();
        if metadata.is_dir() {
            entries.push(FileEntry {
                path: relative,
                kind: FileKind::Directory,
                bytes: None,
            });
            visit_files(root, &path, entries)?;
        } else if metadata.is_file() {
            entries.push(FileEntry {
                path: relative,
                kind: FileKind::File,
                bytes: Some(metadata.len()),
            });
        }
    }
    Ok(())
}

fn read_existing(root: &Path, path: &str) -> DevelopmentResult<String> {
    let absolute = root.join(path);
    let metadata = fs::metadata(&absolute)?;
    if metadata.len() > MAX_BUFFER_BYTES as u64 {
        return Err(DevelopmentError::InvalidInput(
            "existing file exceeds buffer limit".into(),
        ));
    }
    Ok(fs::read_to_string(absolute)?)
}

fn hash(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn git_branch(root: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty())
}

fn detect_framework(root: &Path) -> Option<String> {
    let package = fs::read_to_string(root.join("package.json")).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&package).ok()?;
    let dependencies = value
        .get("dependencies")
        .and_then(serde_json::Value::as_object);
    let dev_dependencies = value
        .get("devDependencies")
        .and_then(serde_json::Value::as_object);
    ["next", "vite", "react", "vue", "svelte"]
        .into_iter()
        .find(|framework| {
            dependencies.is_some_and(|values| values.contains_key(*framework))
                || dev_dependencies.is_some_and(|values| values.contains_key(*framework))
        })
        .map(str::to_string)
}

fn default_commands(
    root: &Path,
    package_manager: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if root.join("Cargo.toml").is_file() {
        return (
            None,
            Some("cargo test".into()),
            Some("cargo clippy --all-targets --all-features -- -D warnings".into()),
            Some("cargo build".into()),
        );
    }
    if root.join("package.json").is_file() {
        let manager = package_manager.unwrap_or("npm");
        return (
            Some(format!("{manager} run dev")),
            Some(format!("{manager} test")),
            Some(format!("{manager} run lint")),
            Some(format!("{manager} run build")),
        );
    }
    if root.join("pyproject.toml").is_file() {
        return (
            None,
            Some("python -m pytest".into()),
            Some("ruff check .".into()),
            None,
        );
    }
    if root.join("go.mod").is_file() {
        return (
            None,
            Some("go test ./...".into()),
            Some("go vet ./...".into()),
            Some("go build ./...".into()),
        );
    }
    (None, None, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("glass-project-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        root
    }

    #[test]
    fn detects_cargo_project_and_defaults() {
        let root = fixture();
        let detection = detect_project(&root).unwrap();
        assert_eq!(detection.languages, vec!["Rust"]);
        assert_eq!(detection.test_command.as_deref(), Some("cargo test"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_writes_are_confined_and_atomic() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project
            .write_file("src/lib.rs", "pub fn ok() {}\n", Actor::local())
            .unwrap();
        assert!(
            project
                .read_file("src/lib.rs")
                .unwrap()
                .contains("pub fn ok")
        );
        assert!(matches!(
            project.write_file("../escape", "bad", Actor::local()),
            Err(DevelopmentError::PathOutsideWorkspace(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_save_records_actor_and_clears_dirty_state() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project.open_buffer("src/main.rs", Actor::local()).unwrap();
        project
            .edit_buffer(
                "src/main.rs",
                "fn main() { println!(\"ok\"); }\n".into(),
                Actor::local(),
            )
            .unwrap();
        assert!(project.buffer("src/main.rs").unwrap().dirty);
        project.save_buffer("src/main.rs").unwrap();
        assert!(!project.buffer("src/main.rs").unwrap().dirty);
        assert!(
            project
                .timeline()
                .events()
                .any(|event| matches!(event.kind, DevelopmentEventKind::FileSaved))
        );
        let _ = fs::remove_dir_all(root);
    }
}
