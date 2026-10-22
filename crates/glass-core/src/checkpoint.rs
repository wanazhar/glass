//! Git object snapshots with a private index; the user's index is never changed.
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub before: String,
    pub after: Option<String>,
    pub workspace: PathBuf,
    pub label: String,
}
fn git(workspace: &Path, args: &[&str], index: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut c = Command::new("git");
    c.current_dir(workspace).args(args).env("LC_ALL", "C");
    if let Some(i) = index {
        c.env("GIT_INDEX_FILE", i);
    }
    let out = c.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().into());
    }
    Ok(out.stdout)
}
fn root(w: &Path) -> Result<PathBuf, String> {
    let out = git(w, &["rev-parse", "--show-toplevel"], None)?;
    Ok(PathBuf::from(String::from_utf8_lossy(&out).trim()))
}
pub fn snapshot(w: &Path, dir: &Path, tag: &str) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let w = root(w)?;
    let index = dir.join(format!("index-{}", std::process::id()));
    let _ = std::fs::remove_file(&index);
    let result = (|| {
        let user_index = git(&w, &["rev-parse", "--git-path", "index"], None)?;
        let user_index = w.join(String::from_utf8_lossy(&user_index).trim());
        if user_index.exists() {
            std::fs::copy(user_index, &index).map_err(|e| e.to_string())?;
        } else {
            git(&w, &["read-tree", "--empty"], Some(&index))?;
        }
        // Only tracked and nonignored files; large generated trees stay excluded.
        git(&w, &["add", "-A", "--", "."], Some(&index))?;
        // A copied index can have a newer timestamp than unchanged-size files,
        // hiding rapid edits behind Git's stat cache. Rehash remaining entries
        // after applying deletions and renames; never mutate the user's index.
        git(&w, &["add", "--renormalize", "--", "."], Some(&index))?;
        let tree = String::from_utf8_lossy(&git(&w, &["write-tree"], Some(&index))?)
            .trim()
            .to_string();
        git(
            &w,
            &[
                "update-ref",
                &format!(
                    "refs/glass-core/{}/{tag}",
                    dir.file_name()
                        .ok_or("missing session directory")?
                        .to_string_lossy()
                ),
                &tree,
            ],
            None,
        )?;
        Ok(tree)
    })();
    let _ = std::fs::remove_file(index);
    result
}
/// Automatic snapshots are optional outside Git; explicit Git actions still explain
/// their requirements. Never initialize or modify a repository implicitly.
pub fn begin_if_repository(
    w: &Path,
    dir: &Path,
    label: &str,
) -> Result<Option<Checkpoint>, String> {
    match root(w) {
        Ok(_) => begin(w, dir, label).map(Some),
        Err(error) if error.contains("not a git repository") => Ok(None),
        Err(error) => Err(error),
    }
}

fn valid_label(label: &str) -> Result<(), String> {
    if label.is_empty()
        || label.len() > 128
        || matches!(label, "." | "..")
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err("Invalid checkpoint label".into());
    }
    Ok(())
}

pub fn record_turn(
    session: &mut crate::session::Session,
    checkpoint: &Checkpoint,
) -> Result<(), String> {
    let turn = session.raw_turns().map_err(|e| e.to_string())?;
    session.append_checked(&[crate::session::Record::Custom {
        kind:"turn/checkpoint".into(),
        data:Some(serde_json::json!({"turn":turn,"label":checkpoint.label,"before":checkpoint.before,"after":checkpoint.after,
            "attribution":"Observed changes during this turn, not proof of exclusive authorship; concurrent external changes may be included."}).to_string()),
    }]).map_err(|e|e.to_string())
}

pub fn begin(w: &Path, dir: &Path, label: &str) -> Result<Checkpoint, String> {
    valid_label(label)?;
    let c = Checkpoint {
        before: snapshot(w, dir, &format!("{label}/before"))?,
        after: None,
        workspace: root(w)?,
        label: label.into(),
    };
    c.save(dir)?;
    Ok(c)
}
impl Checkpoint {
    pub fn finish(&mut self, dir: &Path) -> Result<(), String> {
        self.after = Some(snapshot(
            &self.workspace,
            dir,
            &format!("{}/after", self.label),
        )?);
        self.save(dir)
    }
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        valid_label(&self.label)?;
        crate::auth::secure_write(
            &dir.join(format!("{}.json", self.label)),
            &serde_json::to_vec(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn load(dir: &Path, label: &str) -> Result<Self, String> {
        valid_label(label)?;
        let checkpoint: Self = serde_json::from_slice(&crate::requests::bounded(
            &dir.join(format!("{label}.json")),
            65536,
        )?)
        .map_err(|e| e.to_string())?;
        valid_label(&checkpoint.label)?;
        if checkpoint.label != label {
            return Err("Checkpoint label differs from its receipt".into());
        }
        Ok(checkpoint)
    }
    pub fn changed_files(&self) -> Result<Vec<String>, String> {
        let after = self.after.as_deref().ok_or("checkpoint is still running")?;
        let paths = git(
            &self.workspace,
            &["diff", "--name-only", "-z", &self.before, after],
            None,
        )?;
        Ok(String::from_utf8_lossy(&paths)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect())
    }
    pub fn diff(&self) -> Result<String, String> {
        let after = self.after.as_deref().ok_or("checkpoint is still running")?;
        let summary = git(
            &self.workspace,
            &["diff", "--stat", &self.before, after],
            None,
        )?;
        let patch = git(
            &self.workspace,
            &["diff", "--no-ext-diff", "--no-color", &self.before, after],
            None,
        )?;
        Ok(format!(
            "{}{}",
            String::from_utf8_lossy(&summary),
            String::from_utf8_lossy(&patch)
        ))
    }
    /// Explicit baseline restore; snapshot current files first for recovery.
    pub fn restore(&self, dir: &Path) -> Result<String, String> {
        let current = snapshot(
            &self.workspace,
            dir,
            &format!("{}/restore-backup", self.label),
        )?;
        let backup = Checkpoint {
            before: current.clone(),
            after: Some(self.before.clone()),
            workspace: self.workspace.clone(),
            label: format!("restore-{}", self.label),
        };
        // Save before mutation so a partial restoration can still be recovered.
        backup.save(dir)?;
        let restoration = Checkpoint {
            before: self.before.clone(),
            after: Some(current),
            workspace: self.workspace.clone(),
            label: self.label.clone(),
        };
        let result = restoration.undo(dir)?;
        Ok(format!("{result}; recover with /restore {}", backup.label))
    }

    pub fn undo(&self, dir: &Path) -> Result<String, String> {
        let after = self.after.as_deref().ok_or("checkpoint is still running")?;
        let current = snapshot(&self.workspace, dir, &format!("{}/undo-backup", self.label))?;
        Checkpoint {
            before: current.clone(),
            after: Some(self.before.clone()),
            workspace: self.workspace.clone(),
            label: format!("undo-backup-{}", self.label),
        }
        .save(dir)?;
        let changed = git(
            &self.workspace,
            &[
                "diff",
                "--no-renames",
                "--name-only",
                "-z",
                &self.before,
                after,
            ],
            None,
        )?;
        let later = git(
            &self.workspace,
            &["diff", "--no-renames", "--name-only", "-z", after, &current],
            None,
        )?;
        let paths: Vec<_> = changed
            .split(|b| *b == 0)
            .filter(|b| !b.is_empty())
            .collect();
        for p in &paths {
            if later.split(|b| *b == 0).filter(|q| !q.is_empty()).any(|q| {
                q == *p
                    || (q.starts_with(p) && q.get(p.len()) == Some(&b'/'))
                    || (p.starts_with(q) && p.get(q.len()) == Some(&b'/'))
            }) {
                return Err(format!(
                    "undo refused: {} changed after the task",
                    String::from_utf8_lossy(p)
                ));
            }
            let path = String::from_utf8(p.to_vec()).map_err(|_| "undo requires UTF-8 paths")?;
            let mut parent = self.workspace.join(&path);
            while let Some(p) = parent.parent() {
                if p == self.workspace {
                    break;
                }
                if p.symlink_metadata()
                    .is_ok_and(|m| m.file_type().is_symlink())
                {
                    return Err("undo refused: symlink parent".into());
                }
                parent = p.to_path_buf();
            }
        }
        // Delete task-created leaves before restoring old leaves, including directory transitions.
        let mut restores = Vec::new();
        let mut removes = Vec::new();
        for p in &paths {
            let path = String::from_utf8(p.to_vec()).map_err(|_| "undo requires UTF-8 paths")?;
            let entry = git(
                &self.workspace,
                &["ls-tree", &self.before, "--", &path],
                None,
            )?;
            if entry.starts_with(b"160000 ") {
                return Err("undo does not restore submodule contents".into());
            }
            if entry.is_empty() || entry.starts_with(b"040000 ") {
                removes.push(path);
            } else {
                restores.push(path);
            }
        }
        removes.sort_by_key(|p| std::cmp::Reverse(p.matches('/').count()));
        for path in &removes {
            let target = self.workspace.join(path);
            if target.symlink_metadata().is_ok_and(|m| m.is_dir()) {
                std::fs::remove_dir(&target).map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_file(&target).map_err(|e| e.to_string())?;
            }
            let mut parent = target.parent();
            while let Some(dir) = parent {
                if dir == self.workspace || std::fs::remove_dir(dir).is_err() {
                    break;
                }
                parent = dir.parent();
            }
        }
        for path in &restores {
            let target = self.workspace.join(path);
            if target.symlink_metadata().is_ok_and(|m| m.is_dir()) {
                std::fs::remove_dir(&target).map_err(|e| e.to_string())?;
            }
            git(
                &self.workspace,
                &[
                    "restore",
                    "--source",
                    &self.before,
                    "--worktree",
                    "--",
                    path,
                ],
                None,
            )?;
        }
        Ok(format!(
            "restored {} paths; pre-task edits preserved",
            paths.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn repo() -> (PathBuf, PathBuf) {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base =
            std::env::temp_dir().join(format!("glass-checkpoint-{}-{id}", std::process::id()));
        let w = base.join("repo");
        let d = base.join("snapshots");
        std::fs::create_dir_all(&w).unwrap();
        git(&w, &["init", "-q"], None).unwrap();
        std::fs::write(w.join("file"), "original\n").unwrap();
        git(&w, &["add", "file"], None).unwrap();
        git(
            &w,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@example.com",
                "commit",
                "-qm",
                "initial",
            ],
            None,
        )
        .unwrap();
        (w, d)
    }
    #[test]
    fn automatic_checkpoint_is_optional_in_plain_directories() {
        let (w, d) = repo();
        std::fs::remove_dir_all(w.join(".git")).unwrap();
        assert!(begin_if_repository(&w, &d, "task").unwrap().is_none());
        assert!(!w.join(".git").exists());
        assert!(!d.exists());
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }

    #[test]
    fn undo_preserves_dirty_baseline_index_and_unrelated_later_edits() {
        let (w, d) = repo();
        std::fs::write(w.join("file"), "user's staged edit\n").unwrap();
        git(&w, &["add", "file"], None).unwrap();
        let index = std::fs::read(w.join(".git/index")).unwrap();
        std::fs::write(w.join("file"), "user's unstaged edit\n").unwrap();
        std::fs::write(w.join("untracked"), "user data").unwrap();
        let mut c = begin(&w, &d, "task").unwrap();
        std::fs::write(w.join("file"), "agent edit\n").unwrap();
        std::fs::remove_file(w.join("untracked")).unwrap();
        std::fs::write(w.join("new"), "agent new file").unwrap();
        c.finish(&d).unwrap();
        assert!(c.diff().unwrap().contains("agent edit"));
        std::fs::write(w.join("later"), "later user data").unwrap();
        c.undo(&d).unwrap();
        assert_eq!(
            std::fs::read_to_string(w.join("file")).unwrap(),
            "user's unstaged edit\n"
        );
        assert_eq!(
            std::fs::read_to_string(w.join("untracked")).unwrap(),
            "user data"
        );
        assert!(!w.join("new").exists());
        assert!(w.join("later").exists());
        assert_eq!(std::fs::read(w.join(".git/index")).unwrap(), index);
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }
    #[test]
    fn undo_refuses_later_edits_before_touching_any_file() {
        let (w, d) = repo();
        let mut c = begin(&w, &d, "task").unwrap();
        std::fs::write(w.join("file"), "agent").unwrap();
        std::fs::write(w.join("new"), "new").unwrap();
        c.finish(&d).unwrap();
        std::fs::write(w.join("file"), "later user edit").unwrap();
        assert!(c.undo(&d).unwrap_err().contains("changed after"));
        assert!(w.join("new").exists());
        assert_eq!(
            std::fs::read_to_string(w.join("file")).unwrap(),
            "later user edit"
        );
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }
    #[test]
    fn undo_restores_renames_as_two_paths() {
        let (w, d) = repo();
        let mut c = begin(&w, &d, "task").unwrap();
        std::fs::rename(w.join("file"), w.join("renamed")).unwrap();
        c.finish(&d).unwrap();
        c.undo(&d).unwrap();
        assert!(w.join("file").exists());
        assert!(!w.join("renamed").exists());
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }

    #[test]
    fn undo_handles_file_directory_transitions() {
        let (w, d) = repo();
        let mut c = begin(&w, &d, "task").unwrap();
        std::fs::remove_file(w.join("file")).unwrap();
        std::fs::create_dir(w.join("file")).unwrap();
        std::fs::create_dir(w.join("file/nested")).unwrap();
        std::fs::write(w.join("file/nested/child"), "child").unwrap();
        c.finish(&d).unwrap();
        c.undo(&d).unwrap();
        assert_eq!(
            std::fs::read_to_string(w.join("file")).unwrap(),
            "original\n"
        );
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }

    #[test]
    fn undo_refuses_later_descendants_before_removing_task_files() {
        let (w, d) = repo();
        let mut c = begin(&w, &d, "task").unwrap();
        std::fs::remove_file(w.join("file")).unwrap();
        std::fs::create_dir(w.join("file")).unwrap();
        std::fs::write(w.join("file/task"), "task").unwrap();
        c.finish(&d).unwrap();
        std::fs::write(w.join("file/user"), "later user data").unwrap();
        assert!(c.undo(&d).is_err());
        assert!(w.join("file/task").exists());
        assert!(w.join("file/user").exists());
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }

    #[test]
    fn session_snapshot_refs_do_not_overwrite_another_sessions_objects() {
        let (w, d) = repo();
        let a = begin(&w, &d.join("session-a"), "task-1").unwrap();
        let metadata = std::fs::metadata(w.join("file")).unwrap();
        std::fs::write(w.join("file"), "different").unwrap();
        // Equal size and mtime must not make a copied index hide the edit.
        std::fs::File::options()
            .write(true)
            .open(w.join("file"))
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        let b = begin(&w, &d.join("session-b"), "task-1").unwrap();
        assert_ne!(a.before, b.before);
        let kept = git(
            &w,
            &["rev-parse", "refs/glass-core/session-a/task-1/before"],
            None,
        )
        .unwrap();
        assert_eq!(String::from_utf8_lossy(&kept).trim(), a.before);
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }

    #[test]
    fn manual_restore_has_a_recoverable_backup() {
        let (w, d) = repo();
        let mut c = begin(&w, &d, "manual").unwrap();
        c.finish(&d).unwrap();
        std::fs::write(w.join("file"), "later").unwrap();
        c.restore(&d).unwrap();
        assert_eq!(
            std::fs::read_to_string(w.join("file")).unwrap(),
            "original\n"
        );
        Checkpoint::load(&d, "restore-manual")
            .unwrap()
            .restore(&d)
            .unwrap();
        assert_eq!(std::fs::read_to_string(w.join("file")).unwrap(), "later");
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn undo_restores_symlink_and_executable_mode() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (w, d) = repo();
        symlink("file", w.join("link")).unwrap();
        std::fs::set_permissions(w.join("file"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut c = begin(&w, &d, "task").unwrap();
        std::fs::remove_file(w.join("link")).unwrap();
        symlink("missing", w.join("link")).unwrap();
        std::fs::set_permissions(w.join("file"), std::fs::Permissions::from_mode(0o644)).unwrap();
        c.finish(&d).unwrap();
        c.undo(&d).unwrap();
        assert_eq!(
            std::fs::read_link(w.join("link")).unwrap(),
            PathBuf::from("file")
        );
        assert_ne!(
            std::fs::metadata(w.join("file"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }
    #[test]
    fn checkpoint_labels_reject_paths_and_mismatched_receipts() {
        for label in ["", ".", "..", "../escape", "a/b", "a\\b", "with space"] {
            assert!(valid_label(label).is_err());
        }
        assert!(valid_label("task-123.before").is_ok());
        let (w, d) = repo();
        let c = begin(&w, &d, "original").unwrap();
        std::fs::write(d.join("different.json"), serde_json::to_vec(&c).unwrap()).unwrap();
        assert!(Checkpoint::load(&d, "different").is_err());
        std::fs::remove_dir_all(w.parent().unwrap()).unwrap();
    }
}
