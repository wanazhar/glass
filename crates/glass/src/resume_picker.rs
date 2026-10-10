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
    stamp: Option<std::time::SystemTime>,
}
pub(crate) fn index(dir: &Path, workspace: &Path) -> Vec<Thread> {
    let root = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.into());
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
        let Some(meta) = meta.filter(|m| m.workspace == root) else {
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
pub fn choose(workspace: &Path) -> Result<Option<String>, String> {
    if !glass_tui::term::stdin_is_tty() || !glass_tui::term::stdout_is_tty() {
        return Err("gc resume needs a terminal to choose a conversation. Use gc sessions --json, then gc run --resume ID <prompt> for headless use.".into());
    }
    let threads = index(&crate::config::data_dir(), workspace);
    if threads.is_empty() {
        crate::cli::text(
            "Resume",
            "No conversations in this project yet.\nRun gc to start one, or change to the project containing your conversation.\n",
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
        .map(|s| format!("{} {} {}", s.title, s.modified, s.id))
        .collect::<Vec<_>>();
    let picked = crate::modal::choose_search(
        &keys,
        "Resume conversation",
        "Current project · newest first · type to search",
        &labels,
        &search,
    );
    Ok(picked.map(|i| threads[i].id.clone()))
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
