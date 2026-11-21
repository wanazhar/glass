//! The workspace's own crates, and how this host may reach them.
//!
//! Glass ships as several crates that only mean something together: `glass-core`
//! owns the model loop, `glass-runtime-client` speaks its protocol,
//! `glass-browser` owns the browser, `glass-dev` owns the development workspace,
//! and `glass-mod-api` is the extension boundary. A marble asked to change
//! behaviour reads file paths, and a path does not say which crate owns what it
//! found. So the manifests themselves are the source: every workspace member
//! with its package identity, the binaries it ships, the siblings it depends on,
//! and the reach this host has into it.
//!
//! Nothing here executes another crate. The map answers "which crate owns this,
//! and how would I get to it" so the marble stops guessing; the actual call is
//! either a library dependency at build time, a binary on `PATH`, or a tool.

use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The read-only governed tool that serves this map.
pub const CRATES_TOOL: &str = "glass.crates.list";
/// The product crate whose own dependencies are reported as reachable.
const MARBLE_CRATE: &str = "glass-marble";
/// A personal host's workspace, not a registry.
const MAX_MEMBERS: usize = 64;
const MAX_BINARIES: usize = 16;
const MAX_EDGES: usize = 512;
const MAX_DESCRIPTION: usize = 240;
/// Crates named in the session briefing before it says "and N more".
const BRIEFING_NAMES: usize = 12;

/// One executable a crate ships, and whether this host can run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryReach {
    pub name: String,
    /// Absolute path when the binary is on `PATH`. `None` is worth reporting:
    /// a crate that declares a binary this machine has not installed is not the
    /// same thing as a crate without one.
    pub on_path: Option<PathBuf>,
}

/// One member of the workspace, as its manifest describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCrate {
    pub name: String,
    /// The name Rust code uses: dashes become underscores.
    pub crate_name: String,
    pub version: String,
    pub description: String,
    /// Path relative to the workspace root.
    pub path: String,
    pub binaries: Vec<BinaryReach>,
    /// Sibling crates this one depends on, restricted to the workspace.
    pub depends_on: Vec<String>,
    /// `library` when the marble links this crate, `binary` when it ships an
    /// executable this host has on `PATH`.
    pub reach: Vec<&'static str>,
}

impl WorkspaceCrate {
    pub fn document(&self) -> Value {
        json!({
            "name": self.name,
            "crateName": self.crate_name,
            "version": self.version,
            "description": self.description,
            "path": self.path,
            "binaries": self
                .binaries
                .iter()
                .map(|binary| {
                    json!({
                        "name": binary.name,
                        "onPath": binary.on_path.as_ref().map(|path| path.display().to_string()),
                    })
                })
                .collect::<Vec<_>>(),
            "dependsOn": self.depends_on,
            "reach": self.reach,
        })
    }

    /// Whether this crate answers to the name, path, or binary a caller used.
    fn matches(&self, query: &str) -> bool {
        self.name == query
            || self.crate_name == query
            || self.path == query
            || self.binaries.iter().any(|binary| binary.name == query)
    }
}

/// Every crate in the workspace the marble is working inside.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrateMap {
    crates: Vec<WorkspaceCrate>,
    version: String,
    truncated: bool,
}

impl CrateMap {
    /// Read the workspace from `root`. A missing or unreadable root manifest
    /// yields an empty map rather than an error: a marble can still work in a
    /// directory that is not a Rust workspace, it just has nothing to report.
    pub fn load(root: &Path) -> Self {
        let root_manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
        let workspace = parse_workspace(&root_manifest);
        let mut members = expand_members(root, &workspace.members);
        let mut truncated = members.len() > MAX_MEMBERS;
        members.truncate(MAX_MEMBERS);
        // A single-crate project has no `[workspace]` table: the root is the crate.
        if members.is_empty() && !root_manifest.is_empty() {
            members.push(".".to_string());
        }

        let mut parsed = Vec::new();
        for member in &members {
            let path = normalize_member(member);
            let manifest = fs::read_to_string(root.join(member).join("Cargo.toml"))
                .map(|text| parse_manifest(&text))
                .unwrap_or_default();
            if manifest.name.is_some() {
                parsed.push((path, manifest));
            }
            if parsed.len() >= MAX_MEMBERS {
                truncated = true;
                break;
            }
        }

        let known: Vec<String> = parsed
            .iter()
            .filter_map(|(_, manifest)| manifest.name.clone())
            .collect();
        let mut crates: Vec<WorkspaceCrate> = parsed
            .into_iter()
            .map(|(path, manifest)| {
                let name = manifest.name.clone().unwrap_or_default();
                let mut depends_on: Vec<String> = Vec::new();
                for dependency in &manifest.dependencies {
                    if known.contains(dependency) && !depends_on.contains(dependency) {
                        depends_on.push(dependency.clone());
                    }
                }
                depends_on.truncate(MAX_MEMBERS);
                let binaries = manifest
                    .binaries
                    .iter()
                    .take(MAX_BINARIES)
                    .map(|binary| BinaryReach {
                        name: binary.clone(),
                        on_path: find_binary(binary),
                    })
                    .collect();
                WorkspaceCrate {
                    crate_name: name.replace('-', "_"),
                    name,
                    version: manifest
                        .version
                        .clone()
                        .or_else(|| workspace.version.clone())
                        .unwrap_or_default(),
                    description: bounded(&manifest.description.clone().unwrap_or_default()),
                    path,
                    binaries,
                    depends_on,
                    reach: Vec::new(),
                }
            })
            .collect();
        crates.sort_by(|left, right| left.name.cmp(&right.name));

        // A crate is reachable as a library only when the marble itself links
        // it; that is a fact about the manifest, not about what would be nice.
        let libraries = crates
            .iter()
            .find(|entry| entry.name == MARBLE_CRATE)
            .map(|entry| entry.depends_on.clone())
            .unwrap_or_default();
        for entry in &mut crates {
            if libraries.contains(&entry.name) {
                entry.reach.push("library");
            }
            if entry.binaries.iter().any(|binary| binary.on_path.is_some()) {
                entry.reach.push("binary");
            }
        }

        Self {
            crates,
            version: workspace.version.unwrap_or_default(),
            truncated,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.crates.is_empty()
    }

    pub fn find(&self, query: &str) -> Option<&WorkspaceCrate> {
        let query = query.trim();
        if query.is_empty() {
            return None;
        }
        self.crates.iter().find(|entry| entry.matches(query))
    }

    pub fn marble(&self) -> Option<&WorkspaceCrate> {
        self.crates.iter().find(|entry| entry.name == MARBLE_CRATE)
    }

    /// The map as a surface document, used by `/api/crates` and the tool.
    pub fn document(&self) -> Value {
        let binaries: usize = self.crates.iter().map(|entry| entry.binaries.len()).sum();
        let reachable = self
            .crates
            .iter()
            .filter(|entry| !entry.reach.is_empty())
            .count();
        json!({
            "tool": CRATES_TOOL,
            "workspace": {
                "version": self.version,
                "members": self.crates.len(),
                "marble": self.marble().map(|entry| entry.name.clone()),
            },
            "crates": self
                .crates
                .iter()
                .map(WorkspaceCrate::document)
                .collect::<Vec<_>>(),
            "edges": self.edges(),
            "counts": {
                "crates": self.crates.len(),
                "binaries": binaries,
                "reachable": reachable,
            },
            "truncated": self.truncated,
            "note": "Read from the workspace manifests: package identity, shipped binaries, and \
                     sibling dependencies. `reach` says how this host can touch a crate: \
                     `library` when the marble links it, `binary` when it ships an executable on \
                     PATH. An empty reach means another crate in the map owns the way to it.",
        })
    }

    /// One crate's document, for a tool call that names it.
    pub fn crate_document(&self, entry: &WorkspaceCrate) -> Value {
        let mut document = entry.document();
        if let Value::Object(map) = &mut document {
            map.insert("tool".into(), json!(CRATES_TOOL));
            map.insert(
                "dependedOnBy".into(),
                self.depended_on_by(&entry.name).into(),
            );
        }
        document
    }

    /// Direct dependents inside the workspace, so a caller can see what a change
    /// to one crate would touch.
    fn depended_on_by(&self, name: &str) -> Vec<String> {
        self.crates
            .iter()
            .filter(|entry| entry.depends_on.iter().any(|dependency| dependency == name))
            .map(|entry| entry.name.clone())
            .collect()
    }

    fn edges(&self) -> Vec<Value> {
        let mut edges = Vec::new();
        for entry in &self.crates {
            for dependency in &entry.depends_on {
                if edges.len() >= MAX_EDGES {
                    return edges;
                }
                edges.push(json!({"from": entry.name, "to": dependency}));
            }
        }
        edges
    }

    /// One short paragraph for the session instructions.
    ///
    /// A marble that has to discover the workspace crate by crate spends the
    /// opening of every conversation doing it, so the names are given up front
    /// and the map itself stays behind the tool.
    pub fn briefing(&self) -> Option<String> {
        if self.crates.is_empty() {
            return None;
        }
        let names: Vec<&str> = self
            .crates
            .iter()
            .take(BRIEFING_NAMES)
            .map(|entry| entry.name.as_str())
            .collect();
        let more = self.crates.len().saturating_sub(names.len());
        let more = if more == 0 {
            String::new()
        } else {
            format!(" (and {more} more)")
        };
        Some(format!(
            "This workspace is a Rust workspace of {} crates: {}{more}. Before changing \
             behaviour, ask `{CRATES_TOOL}` which crate owns it, which binaries that crate \
             ships, and which siblings depend on it, rather than inferring ownership from a \
             file path.",
            self.crates.len(),
            names.join(", ")
        ))
    }
}

/// A member list can use a one-level glob such as `crates/*`.
fn expand_members(root: &Path, members: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for member in members {
        let Some(parent) = member.strip_suffix("/*") else {
            expanded.push(member.clone());
            continue;
        };
        let Ok(entries) = fs::read_dir(root.join(parent)) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
            .map(|entry| format!("{parent}/{}", entry.file_name().to_string_lossy()))
            .collect();
        names.sort();
        expanded.extend(names);
    }
    expanded
}

fn normalize_member(member: &str) -> String {
    let member = member.replace('\\', "/");
    let member = member.trim_start_matches("./").trim_end_matches('/');
    if member.is_empty() {
        ".".to_string()
    } else {
        member.to_string()
    }
}

fn bounded(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_DESCRIPTION {
        return text.to_string();
    }
    let short: String = text.chars().take(MAX_DESCRIPTION).collect();
    format!("{short}…")
}

/// Resolve one binary the way a shell would, without running anything.
fn find_binary(name: &str) -> Option<PathBuf> {
    let suffix = env::consts::EXE_SUFFIX;
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths).find_map(|directory| {
        let candidate = directory.join(format!("{name}{suffix}"));
        is_executable(&candidate).then_some(candidate)
    })
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// One member manifest, as far as the map needs it.
#[derive(Debug, Default)]
struct Manifest {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    binaries: Vec<String>,
    dependencies: Vec<String>,
}

/// `[workspace.package]` and the member list from the root manifest.
#[derive(Debug, Default)]
struct WorkspaceManifest {
    members: Vec<String>,
    version: Option<String>,
}

fn parse_manifest(text: &str) -> Manifest {
    let mut manifest = Manifest::default();
    let mut section = String::new();
    let mut binary_table = false;
    let mut depth = 0i32;
    for raw in text.lines() {
        let line = without_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if depth == 0
            && let Some((name, array)) = header(line)
        {
            section = name;
            binary_table = array;
            continue;
        }
        // Only entries at table depth count: a multi-line `{ … }` value or a
        // feature array must not be read as a dependency of its own.
        if depth == 0
            && let Some((key, value)) = entry(line)
        {
            let key = unquote(key);
            if binary_table && section == "bin" {
                if key == "name" {
                    manifest.binaries.push(unquote(value).to_string());
                }
            } else if section == "package" {
                // `version.workspace = true` is inherited, not a literal.
                if !key.contains('.') {
                    match key {
                        "name" => manifest.name = Some(unquote(value).to_string()),
                        "version" => manifest.version = Some(unquote(value).to_string()),
                        "description" => manifest.description = Some(unquote(value).to_string()),
                        _ => {}
                    }
                }
            } else if is_dependency_table(&section)
                && let Some(name) = dependency_name(key)
            {
                manifest.dependencies.push(name.to_string());
            }
        }
        depth = (depth + brace_delta(line)).max(0);
    }
    manifest
}

fn parse_workspace(text: &str) -> WorkspaceManifest {
    let mut workspace = WorkspaceManifest::default();
    let mut section = String::new();
    let mut depth = 0i32;
    let mut lines = text.lines();
    while let Some(raw) = lines.next() {
        let line = without_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if depth == 0
            && let Some((name, _)) = header(line)
        {
            section = name;
            continue;
        }
        if depth == 0
            && let Some((key, value)) = entry(line)
        {
            let key = unquote(key);
            if section == "workspace" && key == "members" {
                let mut array = value.to_string();
                while !array.contains(']') {
                    let Some(next) = lines.next() else {
                        break;
                    };
                    array.push_str(without_comment(next).trim());
                }
                workspace.members = quoted(&array);
            } else if section == "workspace.package" && key == "version" {
                workspace.version = Some(unquote(value).to_string());
            }
        }
        depth = (depth + brace_delta(line)).max(0);
    }
    workspace
}

fn is_dependency_table(section: &str) -> bool {
    section == "dependencies" || section.ends_with(".dependencies")
}

/// The dependency name from `name = …`, `name.workspace = true`, or a quoted key.
fn dependency_name(key: &str) -> Option<&str> {
    let name = unquote(key).split('.').next()?.trim();
    (!name.is_empty()).then_some(name)
}

/// A table header, and whether it was an array of tables (`[[bin]]`).
fn header(line: &str) -> Option<(String, bool)> {
    if let Some(inner) = line.strip_prefix("[[") {
        return Some((inner.strip_suffix("]]")?.trim().to_string(), true));
    }
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    Some((inner.trim().to_string(), false))
}

fn entry(line: &str) -> Option<(&str, &str)> {
    let index = line.find('=')?;
    let key = line[..index].trim();
    (!key.is_empty()).then(|| (key, line[index + 1..].trim()))
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    text
}

fn brace_delta(text: &str) -> i32 {
    let open = text.chars().filter(|character| *character == '{').count();
    let close = text.chars().filter(|character| *character == '}').count();
    open as i32 - close as i32
}

/// Remove a trailing comment without cutting a `#` inside a quoted value.
fn without_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '#' => return &line[..index],
            None => {}
        }
    }
    line
}

fn quoted(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '"' && character != '\'' {
            continue;
        }
        let mut value = String::new();
        while let Some(next) = characters.next() {
            if next == '\\' {
                if let Some(escaped) = characters.next() {
                    value.push(escaped);
                }
                continue;
            }
            if next == character {
                break;
            }
            value.push(next);
        }
        values.push(value);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("glass-marble-crates-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn crate_manifest(root: &Path, member: &str, text: &str) {
        let directory = root.join(member);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("Cargo.toml"), text).unwrap();
    }

    #[test]
    fn members_are_read_with_binaries_versions_and_sibling_edges() {
        let root = workspace("members");
        fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
resolver = "3"
members = [
    "crates/alpha",
    "crates/beta",
]
exclude = ["fuzz"]

[workspace.package]
version = "1.2.3"
"#,
        )
        .unwrap();
        crate_manifest(
            &root,
            "crates/alpha",
            r#"
[package]
name = "alpha"
version.workspace = true
description = "The crate that ships a binary"

[[bin]]
name = "alpha-cli"
path = "src/main.rs"

[dependencies]
beta = { path = "../beta", features = ["std"] }
serde = { version = "1", features = ["derive"] }

[target.'cfg(unix)'.dependencies]
gamma = { path = "../gamma" }
"#,
        );
        crate_manifest(
            &root,
            "crates/beta",
            r#"
[package]
name = "beta"
version = "0.2.0"
description = "A library"

[dependencies]
serde = "1"
"#,
        );

        let map = CrateMap::load(&root);
        assert_eq!(map.crates.len(), 2);
        let alpha = map.find("alpha").expect("alpha is a member");
        assert_eq!(alpha.version, "1.2.3", "workspace version is inherited");
        assert_eq!(alpha.crate_name, "alpha");
        assert_eq!(alpha.description, "The crate that ships a binary");
        assert_eq!(alpha.binaries.len(), 1);
        assert_eq!(alpha.binaries[0].name, "alpha-cli");
        assert_eq!(alpha.depends_on, vec!["beta"], "only workspace siblings");
        assert_eq!(map.find("beta").unwrap().version, "0.2.0");

        let document = map.document();
        assert_eq!(document["workspace"]["members"], json!(2));
        assert_eq!(document["workspace"]["version"], json!("1.2.3"));
        assert_eq!(document["edges"], json!([{"from": "alpha", "to": "beta"}]));
        assert_eq!(document["crates"][0]["path"], json!("crates/alpha"));
        assert!(!map.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_marble_dependencies_are_the_library_reach() {
        let root = workspace("reach");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/glass-marble\", \"crates/support\"]\n",
        )
        .unwrap();
        crate_manifest(
            &root,
            "crates/glass-marble",
            "[package]\nname = \"glass-marble\"\nversion = \"0.1.0\"\n\n[dependencies]\nsupport = { path = \"../support\" }\n",
        );
        crate_manifest(
            &root,
            "crates/support",
            "[package]\nname = \"support\"\nversion = \"0.1.0\"\n",
        );

        let map = CrateMap::load(&root);
        assert_eq!(map.marble().unwrap().name, "glass-marble");
        assert_eq!(map.find("support").unwrap().reach, vec!["library"]);
        assert!(map.find("glass-marble").unwrap().reach.is_empty());
        assert_eq!(
            map.crate_document(map.find("support").unwrap())["dependedOnBy"],
            json!(["glass-marble"])
        );
        let briefing = map.briefing().unwrap();
        assert!(briefing.contains("2 crates"), "{briefing}");
        assert!(briefing.contains(CRATES_TOOL), "{briefing}");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_single_crate_project_is_its_own_map() {
        let root = workspace("single");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"solo\"\nversion = \"0.1.0\"\ndescription = \"solo crate\"\n",
        )
        .unwrap();

        let map = CrateMap::load(&root);
        assert_eq!(map.crates.len(), 1);
        let entry = map.find("solo").unwrap();
        assert_eq!(entry.path, ".");
        assert_eq!(entry.version, "0.1.0");
        assert!(map.find("").is_none());
        assert!(map.find("nothing-here").is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_directory_without_manifests_is_an_empty_map() {
        let root = workspace("empty");
        let map = CrateMap::load(&root);
        assert!(map.is_empty());
        assert!(map.briefing().is_none());
        assert_eq!(map.document()["counts"]["crates"], json!(0));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn comments_and_inline_tables_do_not_become_members_or_dependencies() {
        let root = workspace("comments");
        fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
# a comment with [brackets] and "quotes"
members = [
    "crates/one", # the first
    "crates/two",
]
"#,
        )
        .unwrap();
        crate_manifest(
            &root,
            "crates/one",
            r#"
[package]
name = "one" # the name
version = "3.0.0"
description = "Has a # inside a string"

[dependencies]
two = {
    path = "../two",
    features = ["a", "b"],
}
"#,
        );
        crate_manifest(
            &root,
            "crates/two",
            "[package]\nname = \"two\"\nversion = \"1.0.0\"\n",
        );

        let map = CrateMap::load(&root);
        assert_eq!(map.crates.len(), 2);
        let one = map.find("one").unwrap();
        assert_eq!(one.version, "3.0.0");
        assert_eq!(one.description, "Has a # inside a string");
        assert_eq!(
            one.depends_on,
            vec!["two"],
            "a multi-line dependency is still one edge"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
