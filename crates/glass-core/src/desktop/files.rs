//! Moves files between the workspace and the managed desktop's private
//! Downloads folder, which the workspace-scoped file tools cannot reach.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{Desktop, process};

const MAX_BYTES: u64 = 100 * 1024 * 1024;
const MAX_LISTED: usize = 200;
pub(super) const EXPORT_DIRECTORY: &str = ".gc-artifacts/desktop/downloads";

fn file_name(value: &Value) -> Result<&str, String> {
    value["name"]
        .as_str()
        .filter(|name| {
            !name.is_empty()
                && name.len() <= 255
                && !matches!(*name, "." | "..")
                && !name.contains(['/', '\0'])
        })
        .ok_or_else(|| {
            "name must be a file name from desktop_files list, without slashes".to_owned()
        })
}

fn regular_file(path: &Path, label: &str) -> Result<u64, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| format!("{label} does not exist; call desktop_files with action list"))?;
    if !metadata.is_file() {
        return Err(format!("{label} is not a regular file"));
    }
    if metadata.len() > MAX_BYTES {
        return Err(format!("{label} is larger than 100 MiB"));
    }
    Ok(metadata.len())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| error.to_string())?;
    let partial = to.with_file_name(format!(".{:032x}.gc-partial", u128::from_ne_bytes(nonce)));
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .map_err(|error| format!("Could not stage the file: {error}"))?;
    let result = (|| {
        let mut input = std::fs::File::open(from)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        std::fs::rename(&partial, to)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result.map_err(|error| format!("Could not copy the file: {error}"))
}

fn export_directory(workspace: &Path) -> Result<PathBuf, String> {
    let root = workspace
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let mut directory = root.clone();
    for component in Path::new(EXPORT_DIRECTORY).components() {
        directory.push(component);
        match std::fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Could not create {EXPORT_DIRECTORY}: {error}")),
        }
        directory = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !directory.starts_with(&root) || !directory.is_dir() {
            return Err("export path escapes the workspace or is not a directory".into());
        }
    }
    Ok(directory)
}

pub(super) fn transfer(home: &Path, workspace: &Path, value: &Value) -> Result<Value, String> {
    let object = value
        .as_object()
        .ok_or("desktop_files arguments must be an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "action" | "sessionId" | "name" | "path"))
    {
        return Err("desktop_files accepts only action, sessionId, name, and path".into());
    }
    let downloads = home.join("Downloads");
    match value["action"].as_str() {
        Some("list") => {
            let mut files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&downloads) {
                for entry in entries.filter_map(Result::ok) {
                    let Ok(metadata) = entry.metadata() else {
                        continue;
                    };
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if metadata.is_file() && !name.ends_with(".gc-partial") {
                        files.push(json!({"name": name, "bytes": metadata.len()}));
                    }
                }
            }
            files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            let truncated = files.len() > MAX_LISTED;
            files.truncate(MAX_LISTED);
            Ok(json!({"downloads": downloads, "files": files, "truncated": truncated}))
        }
        Some("export") => {
            let name = file_name(value)?;
            let source = downloads.join(name);
            let bytes = regular_file(&source, &format!("Downloads/{name}"))?;
            let directory = export_directory(workspace)?;
            let target = directory.join(name);
            if std::fs::symlink_metadata(&target).is_ok() {
                let resolved = target.canonicalize().map_err(|error| error.to_string())?;
                let root = workspace
                    .canonicalize()
                    .map_err(|error| error.to_string())?;
                if !resolved.starts_with(root) {
                    return Err("export path escapes the workspace".into());
                }
            }
            copy(&source, &target)?;
            Ok(json!({
                "path": format!("{EXPORT_DIRECTORY}/{name}"),
                "bytes": bytes,
                "next": "Read it with your workspace file tools at that relative path."
            }))
        }
        Some("import") => {
            let relative = value["path"]
                .as_str()
                .filter(|path| !path.is_empty())
                .ok_or("import needs path, a workspace-relative file path")?;
            let root = workspace
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let source = root
                .join(relative)
                .canonicalize()
                .map_err(|_| format!("{relative} does not exist in the workspace"))?;
            if !source.starts_with(&root) {
                return Err("path escapes the workspace".into());
            }
            let bytes = regular_file(&source, relative)?;
            let name = source
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("path must name a UTF-8 file")?;
            std::fs::create_dir_all(&downloads).map_err(|error| {
                format!("Could not create the desktop Downloads folder: {error}")
            })?;
            let target: PathBuf = downloads.join(name);
            copy(&source, &target)?;
            Ok(json!({
                "desktopPath": target,
                "bytes": bytes,
                "next": "Open it from the desktop app's file dialog under Downloads, or type desktopPath into the dialog."
            }))
        }
        _ => Err("action must be `list`, `import`, or `export`".into()),
    }
}

impl Desktop {
    pub(super) fn files(&self, value: &Value, thread_workspace: &Path) -> Result<Value, String> {
        let session_id = value["sessionId"]
            .as_str()
            .ok_or("sessionId is required from desktop_status")?;
        let (home, workspace) = {
            let guard = self
                .session
                .lock()
                .map_err(|_| "Desktop state unavailable")?;
            let session = guard
                .as_ref()
                .ok_or("No managed desktop session is running")?;
            if session.id != session_id {
                return Err(
                    "The managed desktop session changed; call desktop_status and retry".into(),
                );
            }
            let home = match session.lifecycle {
                process::Lifecycle::Disposable => session.directory.join("home"),
                process::Lifecycle::Reusable => self.profile_dir().join("home"),
            };
            (home, session.workspace.clone())
        };
        let root = workspace
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let workspace = thread_workspace
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !workspace.starts_with(root) {
            return Err("thread workspace escapes the desktop workspace".into());
        }
        transfer(&home, &workspace, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_transfers_use_the_trusted_nested_thread_workspace() {
        let root = scratch("thread-root");
        let thread = root.join("project");
        std::fs::create_dir(&thread).unwrap();
        std::fs::write(root.join("report.csv"), "wrong").unwrap();
        std::fs::write(thread.join("report.csv"), "thread").unwrap();
        let desktop = super::super::tests::desktop_with_test_session(&root, "s");
        let imported = desktop
            .files(
                &json!({"action":"import","sessionId":"s","path":"report.csv"}),
                &thread,
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(imported["desktopPath"].as_str().unwrap()).unwrap(),
            "thread"
        );
        let exported = desktop
            .files(
                &json!({"action":"export","sessionId":"s","name":"report.csv"}),
                &thread,
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(thread.join(exported["path"].as_str().unwrap())).unwrap(),
            "thread"
        );
        assert!(!root.join(EXPORT_DIRECTORY).exists());
        assert!(
            desktop
                .files(
                    &json!({"action":"list","sessionId":"s"}),
                    root.parent().unwrap()
                )
                .unwrap_err()
                .contains("escapes")
        );
        assert!(
            desktop
                .files(
                    &json!({"action":"list","sessionId":"s","workspace":"/"}),
                    &thread
                )
                .unwrap_err()
                .contains("only action")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn staging_preserves_siblings_and_cleans_up_failed_copies() {
        let root = scratch("staging");
        let source = root.join("source");
        let target = root.join("report.csv");
        let sibling = root.join("report.gc-partial");
        std::fs::write(&source, "new").unwrap();
        std::fs::write(&sibling, "untouched").unwrap();
        copy(&source, &target).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(copy(&root.join("missing"), &target).is_err());
        assert_eq!(std::fs::read_to_string(&sibling).unwrap(), "untouched");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn export_rejects_directory_and_file_symlink_escapes() {
        use std::os::unix::fs::symlink;
        let home = scratch("confined-home");
        let workspace = scratch("confined-workspace");
        let outside = scratch("confined-outside");
        std::fs::create_dir(home.join("Downloads")).unwrap();
        std::fs::write(home.join("Downloads/report.csv"), "new").unwrap();
        std::fs::write(outside.join("report.csv"), "untouched").unwrap();
        symlink(&outside, workspace.join(".gc-artifacts")).unwrap();
        let request = json!({"action":"export","name":"report.csv"});
        assert!(
            transfer(&home, &workspace, &request)
                .unwrap_err()
                .contains("escapes")
        );
        assert!(!outside.join("desktop").exists());
        std::fs::remove_file(workspace.join(".gc-artifacts")).unwrap();
        let directory = export_directory(&workspace).unwrap();
        symlink(outside.join("report.csv"), directory.join("report.csv")).unwrap();
        assert!(
            transfer(&home, &workspace, &request)
                .unwrap_err()
                .contains("escapes")
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("report.csv")).unwrap(),
            "untouched"
        );
        for path in [home, workspace, outside] {
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    fn scratch(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "glass-desktop-files-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn files_move_between_workspace_and_desktop_downloads() {
        let (home, workspace) = (scratch("home"), scratch("workspace"));
        std::fs::write(workspace.join("report.csv"), "a,b\n").unwrap();
        let imported = transfer(
            &home,
            &workspace,
            &json!({"action":"import","sessionId":"s","path":"report.csv"}),
        )
        .unwrap();
        assert_eq!(imported["bytes"], 4);
        assert_eq!(
            std::fs::read_to_string(home.join("Downloads/report.csv")).unwrap(),
            "a,b\n"
        );
        std::fs::write(home.join("Downloads/invoice.pdf"), "%PDF").unwrap();
        let listed =
            transfer(&home, &workspace, &json!({"action":"list","sessionId":"s"})).unwrap();
        assert_eq!(
            listed["files"],
            json!([{"name":"invoice.pdf","bytes":4},{"name":"report.csv","bytes":4}])
        );
        let exported = transfer(
            &home,
            &workspace,
            &json!({"action":"export","sessionId":"s","name":"invoice.pdf"}),
        )
        .unwrap();
        assert_eq!(
            exported["path"],
            ".gc-artifacts/desktop/downloads/invoice.pdf"
        );
        assert_eq!(
            std::fs::read(workspace.join(".gc-artifacts/desktop/downloads/invoice.pdf")).unwrap(),
            b"%PDF"
        );
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn files_reject_escapes_symlinks_and_unknown_fields() {
        let (home, workspace) = (scratch("home-bad"), scratch("workspace-bad"));
        std::fs::create_dir_all(home.join("Downloads")).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", home.join("Downloads/link")).unwrap();
        let error = |value: Value| {
            transfer(&home, &workspace, &value)
                .err()
                .unwrap_or_default()
        };
        assert!(error(json!({"action":"export","name":"../secret"})).contains("without slashes"));
        assert!(error(json!({"action":"export","name":"link"})).contains("not a regular file"));
        assert!(error(json!({"action":"export","name":"missing"})).contains("action list"));
        assert!(error(json!({"action":"import","path":"../../etc/hostname"})).contains("escapes"));
        assert!(error(json!({"action":"import"})).contains("workspace-relative"));
        assert!(error(json!({"action":"list","dest":"x"})).contains("only action"));
        assert!(error(json!({"action":"move"})).contains("`export`"));
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(workspace);
    }
}
