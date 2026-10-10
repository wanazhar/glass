//! Lightweight session index: no transcript is loaded until a user selects it.
use serde::de::{Deserializer, MapAccess, Visitor};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Meta {
    workspace: PathBuf,
    title: String,
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
    let mut de = serde_json::Deserializer::from_reader(BufReader::new(file));
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
    stamp: Option<std::time::SystemTime>,
}
pub(crate) fn index(dir: &Path, workspace: &Path) -> Vec<Thread> {
    index_scoped(dir, Some(workspace))
}
pub(crate) fn all(dir:&Path)->Vec<Thread> { index_scoped(dir,None) }
fn index_scoped(dir: &Path, workspace: Option<&Path>) -> Vec<Thread> {
    let root = workspace
        .unwrap_or(Path::new("/"))
        .canonicalize()
        .unwrap_or_else(|_| workspace.unwrap_or(Path::new("/")).into());
    let mut threads = Vec::new();
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
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let meta = std::fs::File::open(path.with_extension("meta.json"))
            .ok()
            .and_then(read_meta);
        let Some(meta) = meta.filter(|m| workspace.is_none() || m.workspace == root) else {
            continue;
        };
        let title = if meta.title.trim().is_empty() {
            std::fs::File::open(&path)
                .ok()
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
        });
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
}
pub fn choose(workspace: &Path, all: bool, browse: bool) -> Result<Option<Pick>, String> {
    if !glass_tui::term::stdin_is_tty() || !glass_tui::term::stdout_is_tty() {
        return Err("gc resume needs a terminal to choose a conversation. Use gc sessions --json, then gc run --resume ID <prompt> for headless use.".into());
    }
    let threads = if all {
        index_scoped(&crate::config::data_dir(), None)
    } else {
        index(&crate::config::data_dir(), workspace)
    };
    if threads.is_empty() {
        crate::cli::text(
            "Resume",
            "No conversations in this project yet. Run gc, or use gc resume --all to browse other projects.",
        );
        return Ok(None);
    }
    let _terminal = Terminal;
    glass_tui::term::enter().map_err(|e| e.to_string())?;
    let keys = crate::input::KeyReader::spawn();
    let labels = threads
        .iter()
        .map(|s| {
            format!(
                "{} · {} · {}",
                glass_tui::text::clip(&s.title, 60),
                s.modified,
                s.id
            )
        })
        .collect::<Vec<_>>();
    let search = threads
        .iter()
        .map(|s| {
            format!(
                "{} {} {} {}",
                s.title,
                s.modified,
                s.id,
                s.workspace.display()
            )
        })
        .collect::<Vec<_>>();
    while let Some(index) = crate::modal::choose_search(
        &keys,
        "Resume conversation",
        if all {
            "All projects · newest first · choose to preview/resume"
        } else {
            "Current project · newest first · type to search"
        },
        &labels,
        &search,
    ) {
        let thread = &threads[index];
        if !all && !browse {
            return Ok(Some(Pick {
                id: thread.id.clone(),
                workspace: thread.workspace.clone(),
            }));
        }
        loop {
            let options = [
                "Resume conversation".into(),
                "Preview conversation".into(),
                "Fork through a selected turn…".into(),
                "Back".into(),
            ];
            match crate::modal::choose(
                &keys,
                &thread.title,
                &format!("{}\n{}", thread.workspace.display(), thread.modified),
                &options,
            ) {
                Some(0) => {
                    return Ok(Some(Pick {
                        id: thread.id.clone(),
                        workspace: thread.workspace.clone(),
                    }));
                }
                Some(1) => {
                    let path = crate::config::data_dir().join(format!("{}.jsonl", thread.id));
                    // A bounded tail preview never opens the session for writing or recovers it.
                    let rows = crate::requests::tail(&path)?;
                    let text = rows
                        .iter()
                        .filter_map(|v| match v["k"].as_str() {
                            Some("user") => {
                                Some(format!("**You**\n\n{}", v["t"].as_str().unwrap_or("")))
                            }
                            Some("assistant") => Some(v["t"].as_str().unwrap_or("").to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n");
                    crate::modal::rich_info(
                        &keys,
                        "Conversation preview",
                        if text.is_empty() {
                            "No user or response text in this bounded tail preview."
                        } else {
                            &text
                        },
                    );
                }
                Some(2) => {
                    let Some(through) = crate::modal::validated_field(
                        &keys,
                        "Fork conversation",
                        "Through turn number (positive)",
                        "1",
                        false,
                        |s| {
                            if s.parse::<usize>().is_ok_and(|n| n > 0) {
                                Ok(())
                            } else {
                                Err("Enter a positive turn number".into())
                            }
                        },
                    ) else {
                        continue;
                    };
                    let session = crate::session::Session::open_readonly(
                        &crate::config::data_dir(),
                        &thread.id,
                    )
                    .map_err(|e| e.to_string())?;
                    let meta: crate::polish::Metadata =
                        serde_json::from_slice(&crate::requests::bounded(
                            &session.path.with_extension("meta.json"),
                            16 * 1024 * 1024,
                        )?)
                        .map_err(|_| "Invalid session metadata; nothing forked")?;
                    if meta.workspace != thread.workspace {
                        return Err("Session metadata changed; reopen the picker".into());
                    }
                    let fork = session
                        .branch(&crate::config::data_dir(), through.parse().unwrap())
                        .map_err(|e| e.to_string())?;
                    crate::polish::save(&fork, &meta)?;
                    return Ok(Some(Pick {
                        id: fork.id.clone(),
                        workspace: thread.workspace.clone(),
                    }));
                }
                _ => break,
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
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
}
