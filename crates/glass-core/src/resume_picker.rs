//! Lightweight session index: no transcript is loaded until a user selects it.
use serde::de::{Deserializer, MapAccess, Visitor};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

const SHARED_PROJECT_LIMIT: usize = 256;

/// Register one trusted project store so Connect can discover its Glass
/// conversations when the gateway is started from a different workspace.
pub(crate) fn register_project(workspace: &Path) -> Result<(), String> {
    let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
    if !workspace.is_dir() {
        return Err("shared session workspace is not a directory".into());
    }
    let store = workspace.join(".glass/agent-sessions");
    std::fs::create_dir_all(&store).map_err(|e| e.to_string())?;
    let store = store.canonicalize().map_err(|e| e.to_string())?;
    if store.parent().and_then(Path::parent) != Some(workspace.as_path()) {
        return Err("shared session store is outside its project".into());
    }
    let data = crate::config::data_dir();
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let lock_path = data.join("shared-projects.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::lock_exclusive(&lock).map_err(|e| e.to_string())?;
    let index_path = data.join("shared-projects.json");
    let mut projects = crate::requests::bounded_optional(&index_path, 64 * 1024)?
        .map(|bytes| serde_json::from_slice::<Vec<PathBuf>>(&bytes).map_err(|e| e.to_string()))
        .transpose()?
        .unwrap_or_default();
    if !projects.contains(&workspace) {
        if projects.len() >= SHARED_PROJECT_LIMIT {
            return Err("shared project registry is full".into());
        }
        projects.push(workspace);
        projects.sort();
        crate::auth::secure_write(
            &index_path,
            &serde_json::to_vec(&projects).map_err(|e| e.to_string())?,
        )?;
    }
    fs2::FileExt::unlock(&lock).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn registered_project_stores() -> Vec<PathBuf> {
    let path = crate::config::data_dir().join("shared-projects.json");
    let Ok(Some(bytes)) = crate::requests::bounded_optional(&path, 64 * 1024) else {
        return Vec::new();
    };
    let Ok(projects) = serde_json::from_slice::<Vec<PathBuf>>(&bytes) else {
        return Vec::new();
    };
    projects
        .into_iter()
        .take(SHARED_PROJECT_LIMIT)
        .filter_map(|project| {
            let canonical = project.canonicalize().ok()?;
            let store = canonical
                .join(".glass/agent-sessions")
                .canonicalize()
                .ok()?;
            (store.parent().and_then(Path::parent) == Some(canonical.as_path())).then_some(store)
        })
        .collect()
}

#[derive(Default)]
struct Meta {
    workspace: PathBuf,
    title: String,
    origin: Option<crate::polish::SessionOrigin>,
}
fn regular_file(path: &Path) -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}
fn read_meta(file: std::fs::File) -> Option<Meta> {
    // Serde validates the closing brace after a map visitor returns. Signal a
    // deliberate early stop once index fields are read, before attachments.
    const READY: &str = "session index fields ready";
    struct Fields<'a>(&'a mut Meta);
    impl<'de> Visitor<'de> for Fields<'_> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("session metadata")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
            let mut workspace = false;
            let mut title = false;
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    "workspace" => {
                        self.0.workspace = map.next_value()?;
                        workspace = true;
                    }
                    "title" => {
                        self.0.title = map.next_value()?;
                        title = true;
                    }
                    "origin" => {
                        self.0.origin = match map.next_value::<Option<String>>()?.as_deref() {
                            Some("gc") => Some(crate::polish::SessionOrigin::Gc),
                            Some("glass") => Some(crate::polish::SessionOrigin::Glass),
                            _ => None,
                        };
                    }
                    _ => {
                        map.next_value::<serde::de::IgnoredAny>()?;
                    }
                }
                if workspace && title {
                    return Err(serde::de::Error::custom(READY));
                }
            }
            Ok(())
        }
    }
    let mut meta = Meta::default();
    let mut de = serde_json::Deserializer::from_reader(BufReader::new(file.take(64 * 1024)));
    let result = de.deserialize_map(Fields(&mut meta));
    match result {
        Ok(()) => Some(meta),
        Err(error) if error.to_string().starts_with(READY) => Some(meta),
        _ => None,
    }
}
#[derive(Debug)]
pub(crate) struct Thread {
    pub id: String,
    pub title: String,
    pub modified: String,
    pub workspace: PathBuf,
    pub path: PathBuf,
    pub origin: Option<crate::polish::SessionOrigin>,
    stamp: Option<std::time::SystemTime>,
}
pub(crate) fn index(dir: &Path, workspace: &Path) -> Vec<Thread> {
    let local = workspace.join(".glass").join("agent-sessions");
    index_roots(&[dir.to_path_buf(), local], Some(workspace), false)
}
pub(crate) fn all(dir: &Path) -> Vec<Thread> {
    index_roots(&[dir.to_path_buf()], None, false)
}
pub(crate) fn all_for_web(dir: &Path, workspace: &Path) -> Vec<Thread> {
    let mut dirs = vec![dir.to_path_buf(), workspace.join(".glass/agent-sessions")];
    dirs.extend(registered_project_stores());
    index_roots(&dirs, None, true)
}
fn index_roots(dirs: &[PathBuf], workspace: Option<&Path>, unassigned: bool) -> Vec<Thread> {
    let root = workspace
        .unwrap_or(Path::new("/"))
        .canonicalize()
        .unwrap_or_else(|_| workspace.unwrap_or(Path::new("/")).into());
    let mut threads = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (dir, inferred_origin) in dirs
        .iter()
        .map(|dir| (dir, crate::polish::origin_for_store(dir)))
    {
        for entry in std::fs::read_dir(dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.extension().is_none_or(|s| s != "jsonl")
                || !entry.file_type().is_ok_and(|t| t.is_file())
            {
                continue;
            }
            let Ok(canonical_path) = path.canonicalize() else {
                continue;
            };
            if !seen.insert(canonical_path.clone()) {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let meta_path = path.with_extension("meta.json");
            let meta = regular_file(&meta_path).and_then(read_meta).or_else(|| {
                (!meta_path.exists()
                    && (unassigned || inferred_origin == Some(crate::polish::SessionOrigin::Glass)))
                .then(|| Meta {
                    workspace: if inferred_origin == Some(crate::polish::SessionOrigin::Glass) {
                        dir.parent()
                            .and_then(Path::parent)
                            .unwrap_or(Path::new(""))
                            .to_path_buf()
                    } else {
                        PathBuf::new()
                    },
                    ..Meta::default()
                })
            });
            let Some(mut meta) = meta.filter(|m| workspace.is_none() || m.workspace == root) else {
                continue;
            };
            if meta.origin.is_none() {
                meta.origin = inferred_origin;
            }
            let title = if meta.title.trim().is_empty() {
                regular_file(&path)
                    .and_then(|f| {
                        BufReader::new(f.take(64 * 1024))
                            .lines()
                            .map_while(Result::ok)
                            .find_map(|line| {
                                let v: serde_json::Value = serde_json::from_str(&line).ok()?;
                                (v["k"] == "user")
                                    .then(|| v["t"].as_str().map(str::to_owned))
                                    .flatten()
                            })
                    })
                    .unwrap_or_else(|| "Untitled conversation".into())
            } else {
                meta.title
            };
            let stamp = entry.metadata().ok().and_then(|m| m.modified().ok());
            threads.push(Thread {
                id: id.into(),
                title: glass_tui::markdown::sanitize(&title).replace(['\n', '\r'], " "),
                modified: stamp
                    .map(crate::format_time)
                    .unwrap_or_else(|| "Unknown time".into()),
                stamp,
                workspace: meta.workspace,
                path: canonical_path,
                origin: meta.origin,
            });
        }
    }
    threads.sort_by_key(|s| std::cmp::Reverse(s.stamp));
    threads
}
struct Terminal;
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = glass_tui::term::leave();
    }
}
pub struct Pick {
    pub id: String,
    pub workspace: PathBuf,
    pub path: PathBuf,
}
pub fn choose(workspace: &Path, all: bool, browse: bool) -> Result<Option<Pick>, String> {
    if !glass_tui::term::stdin_is_tty() || !glass_tui::term::stdout_is_tty() {
        return Err("gc resume needs a terminal to choose a conversation. Use gc sessions --json, then gc run --resume ID <prompt> for headless use.".into());
    }
    let _terminal = Terminal;
    glass_tui::term::enter().map_err(|e| e.to_string())?;
    let keys = crate::input::KeyReader::spawn();
    crate::navigation::choose(&keys, workspace, None, all, false, !browse)
}

#[cfg(test)]
mod tests {
    #[test]
    fn web_index_includes_unassigned_without_guessing_a_workspace() {
        let root =
            std::env::temp_dir().join(format!("gc-web-index-{}", crate::auth::random(8).unwrap()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("bound.jsonl"),
            "{\"k\":\"user\",\"t\":\"Bound\"}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("bound.meta.json"),
            serde_json::json!({"workspace":root,"title":"Bound"}).to_string(),
        )
        .unwrap();
        std::fs::write(
            root.join("legacy.jsonl"),
            "{\"k\":\"user\",\"t\":\"Legacy\"}\n",
        )
        .unwrap();
        assert_eq!(super::all(&root).len(), 1);
        let rows = super::all_for_web(&root);
        assert_eq!(rows.len(), 2);
        let legacy = rows.iter().find(|r| r.id == "legacy").unwrap();
        assert!(legacy.workspace.as_os_str().is_empty());
        assert_eq!(legacy.title, "Legacy");
        std::fs::remove_dir_all(root).unwrap();
    }

    use super::*;
    #[test]
    fn index_ignores_attachment_payload_and_other_projects() {
        let dir = std::env::temp_dir().join(format!("gc-resume-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let workspace = dir.canonicalize().unwrap();
        std::fs::write(dir.join("a.jsonl"), "{\"k\":\"user\",\"t\":\"Hello\"}\n").unwrap();
        // Deliberately truncated attachment payload: index must not parse it.
        std::fs::write(
            dir.join("a.meta.json"),
            format!(
                "{{\"workspace\":{},\"title\":\"Pretty thread\",\"attachments\":[",
                serde_json::to_string(&workspace).unwrap()
            ),
        )
        .unwrap();
        let rows = index(&dir, &workspace);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Pretty thread");
        assert!(index(&dir, &dir.join("other")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn index_rejects_symlink_metadata_and_oversized_prefix() {
        let dir = std::env::temp_dir().join(format!("gc-index-safe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let workspace = dir.canonicalize().unwrap();
        std::fs::write(dir.join("a.jsonl"), "{}\n").unwrap();
        std::fs::write(
            dir.join("source"),
            serde_json::json!({"workspace":workspace,"title":"hidden"}).to_string(),
        )
        .unwrap();
        std::os::unix::fs::symlink(dir.join("source"), dir.join("a.meta.json")).unwrap();
        assert!(index(&dir, &workspace).is_empty());
        std::fs::remove_file(dir.join("a.meta.json")).unwrap();
        std::fs::write(
            dir.join("a.meta.json"),
            format!(
                "{{\"unknown\":\"{}\",\"workspace\":{},\"title\":\"late\"}}",
                "x".repeat(65536),
                serde_json::to_string(&workspace).unwrap()
            ),
        )
        .unwrap();
        assert!(index(&dir, &workspace).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
