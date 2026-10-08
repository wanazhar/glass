//! Daily-use flows built on the existing log, snapshots and shared modal input.
use crate::{
    agents_md, auth,
    checkpoint::Checkpoint,
    config,
    input::KeyReader,
    modal,
    session::{Record, Session},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub struct Metadata {
    pub workspace: PathBuf,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub path: PathBuf,
    pub text: String,
}
fn meta_path(s: &Session) -> PathBuf {
    s.path.with_extension("meta.json")
}
pub fn metadata(s: &Session) -> Metadata {
    std::fs::read(meta_path(s))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn save(s: &Session, meta: &Metadata) -> Result<(), String> {
    auth::secure_write(
        &meta_path(s),
        &serde_json::to_vec_pretty(meta).map_err(|e| e.to_string())?,
    )
}
pub fn bind(s: &Session, workspace: &Path) -> Result<(), String> {
    let path = meta_path(s);
    if path.exists() {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        serde_json::from_slice::<Metadata>(&bytes)
            .map_err(|_| "Session preferences are invalid; they have not been overwritten.")?;
    }
    let mut meta = metadata(s);
    let root = workspace.canonicalize().map_err(|e| e.to_string())?;
    if meta.workspace.as_os_str().is_empty() {
        meta.workspace = root;
        save(s, &meta)?;
    } else if meta.workspace != root {
        return Err(format!(
            "Session belongs to {}. Run glass-core there to resume it.",
            meta.workspace.display()
        ));
    }
    Ok(())
}
pub fn title(s: &Session) -> String {
    let m = metadata(s);
    if !m.title.is_empty() {
        return m.title;
    }
    s.records
        .iter()
        .find_map(|r| {
            if let Record::User(t) = r {
                Some(glass_tui::text::clip(&t.replace('\n', " "), 60))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "New conversation".into())
}
pub fn sessions(workspace: &Path) -> Vec<Session> {
    let root = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.into());
    let mut paths = std::fs::read_dir(config::data_dir())
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .collect::<Vec<_>>();
    paths.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));
    paths
        .into_iter()
        .filter_map(|e| Session::open(&config::data_dir(), &e.path().to_string_lossy()).ok())
        .filter(|s| metadata(s).workspace == root)
        .collect()
}
pub fn browser(
    keys: &KeyReader,
    current: &Session,
    workspace: &Path,
) -> Result<Option<Session>, String> {
    loop {
        let all = sessions(workspace);
        let rows = all
            .iter()
            .map(|s| {
                format!(
                    "{} · {}{}",
                    title(s),
                    s.id,
                    if s.id == current.id { " (current)" } else { "" }
                )
            })
            .collect::<Vec<_>>();
        let Some(i) = modal::choose(
            keys,
            "Sessions",
            "Search conversations in this project",
            &rows,
        ) else {
            return Ok(None);
        };
        let s = &all[i];
        let preview = s
            .records
            .iter()
            .filter_map(|r| match r {
                Record::User(t) => Some(format!("You: {t}")),
                Record::Assistant(t) => Some(format!("Assistant: {t}")),
                _ => None,
            })
            .take(8)
            .collect::<Vec<_>>()
            .join("\n\n");
        let actions = vec![
            "Resume conversation".into(),
            "Preview conversation".into(),
            "Rename conversation".into(),
            "Branch conversation".into(),
            "Delete conversation…".into(),
        ];
        match modal::choose(
            keys,
            &title(s),
            &format!("{} turns · {}", s.turn, s.id),
            &actions,
        ) {
            Some(0) => {
                return Session::open(&config::data_dir(), &s.id)
                    .map(Some)
                    .map_err(|e| e.to_string());
            }
            Some(1) => modal::info(keys, "Conversation preview", &preview),
            Some(2) => {
                if let Some(t) = modal::field(keys, "Rename session", "Title", &title(s), false) {
                    let mut m = metadata(s);
                    m.title = t;
                    save(s, &m)?;
                }
            }
            Some(3) => {
                let count = s.raw_turns().map_err(|e| e.to_string())?;
                if count == 0 {
                    modal::info(keys, "Branch session", "This session has no turns yet.");
                    continue;
                }
                let choices = (1..=count)
                    .rev()
                    .map(|n| format!("Through turn {n}"))
                    .collect::<Vec<_>>();
                if let Some(n) = modal::choose(
                    keys,
                    "Branch session",
                    "Choose the last turn to include",
                    &choices,
                ) {
                    let branch = s
                        .branch(&config::data_dir(), count - n)
                        .map_err(|e| e.to_string())?;
                    let mut m = metadata(s);
                    m.title = format!("{} (branch)", title(s));
                    save(&branch, &m)?;
                    return Ok(Some(branch));
                }
            }
            Some(4) => {
                if s.id == current.id {
                    modal::info(
                        keys,
                        "Delete session",
                        "Switch to another session before deleting the current one.",
                    );
                    continue;
                }
                if modal::choose(
                    keys,
                    "Delete conversation?",
                    "Deletes transcript and preferences. Workspace files and checkpoint snapshots stay.",
                    &[
                        "Keep conversation".into(),
                        "Delete conversation permanently".into(),
                    ],
                ) == Some(1)
                {
                    std::fs::remove_file(&s.path).map_err(|e| e.to_string())?;
                    for ext in ["meta.json", "cost.json"] {
                        let p = s.path.with_extension(ext);
                        if p.exists() {
                            std::fs::remove_file(p).map_err(|e| e.to_string())?;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
pub fn attach(s: &Session, workspace: &Path, path: &str) -> Result<(), String> {
    let p = workspace
        .join(path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    if bytes.len() > 32_768 {
        return Err("File exceeds 32 KiB. Attach a smaller excerpt.".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "Only UTF-8 text files can be attached.")?;
    let mut m = metadata(s);
    m.attachments.retain(|a| a.path != p);
    if m.attachments.iter().map(|a| a.text.len()).sum::<usize>() + text.len() > 131_072 {
        return Err("Attachments exceed 128 KiB. Remove an attachment in /context.".into());
    }
    m.attachments.push(Attachment { path: p, text });
    save(s, &m)
}
pub fn prompt_context(s: &Session, instructions: &Option<String>) -> Option<String> {
    let mut text = instructions.clone().unwrap_or_default();
    for a in metadata(s).attachments {
        text.push_str(&format!(
            "\n<attached_file path={:?}>\n{}\n</attached_file>",
            a.path, a.text
        ));
    }
    (!text.is_empty()).then_some(text)
}
pub fn context_report(
    s: &Session,
    cfg: &config::Config,
    instructions: &Option<String>,
    catalogue: &Option<String>,
) -> String {
    let messages = s.messages();
    let instruction_files = agents_md::load(&cfg.workspace)
        .into_iter()
        .map(|i| {
            format!(
                "{}{}",
                i.path.display(),
                if i.truncated { " (truncated)" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let m = metadata(s);
    let files = m
        .attachments
        .iter()
        .map(|a| {
            format!(
                "{} · {} bytes (saved snapshot)",
                a.path.display(),
                a.text.len()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = cfg.system_prompt.len()
        + instructions.as_ref().map_or(0, String::len)
        + catalogue.as_ref().map_or(0, String::len)
        + m.attachments.iter().map(|a| a.text.len()).sum::<usize>();
    let estimate = crate::provider::default_estimate(&messages) + bytes.div_ceil(4) as u32;
    let compaction = s
        .records
        .iter()
        .filter_map(|r| {
            if let Record::Compacted {
                kept,
                dropped,
                summary,
                ..
            } = r
            {
                Some(format!(
                    "Kept {kept}, dropped {dropped}\n{}",
                    summary
                        .as_deref()
                        .unwrap_or("Deterministic compaction; no generated summary.")
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!(
        "Model: {}\nEstimated context: {estimate} tokens (includes prompt text; excludes tool schemas)\nMessages: {}\nSystem prompt: {} bytes\nSkill catalogue: {} bytes\n\nLoaded instructions:\n{}\n\nAttached files:\n{}\n\nCompaction:\n{}",
        cfg.model,
        messages.len(),
        cfg.system_prompt.len(),
        catalogue.as_ref().map_or(0, String::len),
        if instruction_files.is_empty() {
            "None"
        } else {
            &instruction_files
        },
        if files.is_empty() { "None" } else { &files },
        if compaction.is_empty() {
            "No compaction recorded"
        } else {
            &compaction
        }
    )
}
pub fn context(
    keys: &KeyReader,
    s: &Session,
    cfg: &config::Config,
    instructions: &Option<String>,
    catalogue: &Option<String>,
) -> Result<(), String> {
    loop {
        let mut rows = vec![
            "Inspect context and compaction".into(),
            "Attach a text file…".into(),
            "Read loaded instructions".into(),
            "Read system prompt".into(),
            "Read skill catalogue".into(),
        ];
        let m = metadata(s);
        rows.extend(
            m.attachments
                .iter()
                .map(|a| format!("Remove {}", a.path.display())),
        );
        match modal::choose(
            keys,
            "Context",
            "Attachments are saved snapshots; remove them before the next turn",
            &rows,
        ) {
            Some(0) => modal::info(
                keys,
                "Context details",
                &context_report(s, cfg, instructions, catalogue),
            ),
            Some(1) => {
                if let Some(path) = path_picker(keys, &cfg.workspace) {
                    attach(s, &cfg.workspace, &path)?;
                }
            }
            Some(2) => modal::info(
                keys,
                "Loaded instructions",
                instructions
                    .as_deref()
                    .unwrap_or("No project instructions loaded."),
            ),
            Some(3) => modal::info(keys, "System prompt", &cfg.system_prompt),
            Some(4) => modal::info(
                keys,
                "Skill catalogue",
                catalogue.as_deref().unwrap_or("No skill catalogue loaded."),
            ),
            Some(i) => {
                let mut m = metadata(s);
                m.attachments.remove(i - 5);
                save(s, &m)?;
            }
            None => return Ok(()),
        }
    }
}
pub fn path_picker(keys: &KeyReader, workspace: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(workspace)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
        .ok();
    let mut paths = output
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split('\0')
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    paths.sort();
    paths.dedup();
    paths.insert(0, "Enter a file path…".into());
    match modal::choose(keys, "Files", "Search project files", &paths)? {
        0 => modal::field(
            keys,
            "File path",
            "Relative or absolute text file path",
            "",
            false,
        ),
        i => Some(paths[i].clone()),
    }
}
pub(crate) fn raw_records(s: &Session) -> Vec<Record> {
    std::fs::read_to_string(&s.path)
        .ok()
        .map(|text| {
            text.lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter_map(|v| Record::from_json(&v))
                .collect()
        })
        .unwrap_or_else(|| s.records.clone())
}
pub fn completion(s: &Session, checkpoint: Option<&Checkpoint>, width: usize) -> String {
    let records = raw_records(s);
    let start = records
        .iter()
        .rposition(|r| matches!(r, Record::User(_)))
        .unwrap_or(0);
    let commands = records[start..]
        .iter()
        .filter(|r| matches!(r, Record::Bash { .. }))
        .count();
    let changed = checkpoint
        .and_then(|c| c.changed_files().ok())
        .map(|paths| format!("{} changed files", paths.len()))
        .unwrap_or_else(|| "diff unavailable".into());
    if width < 80 {
        let changed = checkpoint
            .and_then(|c| c.changed_files().ok())
            .map(|paths| format!("{} files", paths.len()))
            .unwrap_or_else(|| "no diff".into());
        glass_tui::text::wrap_words(
            &format!("{commands} tools · {changed}"),
            width.saturating_sub(1),
        )
        .join("\n")
    } else {
        format!("{commands} commands · {changed} · /review · /details")
    }
}
struct ToolDetail {
    label: String,
    command: Option<String>,
    output: String,
    spill: Option<String>,
    truncated: bool,
}
fn tool_details(records: &[Record]) -> Vec<ToolDetail> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Bash {
                command,
                output,
                exit_code,
                spill,
                truncated,
                ..
            } => Some(ToolDetail {
                label: format!(
                    "bash · exit {} · {}",
                    exit_code
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| "signal".into()),
                    command.replace('\n', " ")
                ),
                command: Some(command.clone()),
                output: output.clone(),
                spill: spill.clone(),
                truncated: *truncated,
            }),
            Record::ToolResult { name, body, .. } if name != "bash" => Some(ToolDetail {
                label: format!("{name} · {}", glass_tui::text::clip(body, 50)),
                command: None,
                output: body.clone(),
                spill: None,
                truncated: false,
            }),
            _ => None,
        })
        .rev()
        .collect()
}
pub fn details(keys: &KeyReader, s: &Session) {
    let entries = tool_details(&raw_records(s));
    if entries.is_empty() {
        modal::info(
            keys,
            "Tool details",
            "No tool activity in this session yet.",
        );
        return;
    }
    let labels: Vec<_> = entries
        .iter()
        .map(|e| glass_tui::markdown::sanitize(&e.label))
        .collect();
    let searchable: Vec<_> = entries
        .iter()
        .map(|e| {
            format!(
                "{}\n{}\n{}",
                e.label,
                e.command.as_deref().unwrap_or(""),
                e.output
            )
        })
        .collect();
    while let Some(i) = modal::choose_search(
        keys,
        "Tool details",
        "Search names, commands and recorded inline output; no execution",
        &labels,
        &searchable,
    ) {
        let entry = &entries[i];
        let mut actions = vec!["Read command and recorded output".into()];
        if entry.command.is_some() {
            actions.push("Copy command".into());
        }
        actions.push("Copy recorded output".into());
        while let Some(action) = modal::choose(
            keys,
            "Tool actions",
            "Explicit clipboard copy only · Esc back",
            &actions,
        ) {
            if action == 0 {
                let command = entry
                    .command
                    .as_deref()
                    .map(|c| format!("Command\n$ {c}\n\n"))
                    .unwrap_or_default();
                let location = if let Some(spill) = &entry.spill {
                    format!(
                        "Recorded inline preview only. Full output saved at: {spill}\nCopy recorded output copies this preview; the spill file is not loaded."
                    )
                } else if entry.truncated {
                    "Recorded inline output is truncated; no full output path was recorded.".into()
                } else {
                    "Complete recorded inline output. Copy preserves these original bytes.".into()
                };
                modal::info(
                    keys,
                    "Command / recorded output",
                    &format!("{command}Recorded output\n{}\n\n{location}", entry.output),
                );
            } else {
                let copied = if actions[action] == "Copy command" {
                    entry.command.as_deref().unwrap()
                } else {
                    &entry.output
                };
                match crate::workflows::copy(copied) {
                    Ok(()) => modal::info(
                        keys,
                        "Copied",
                        "Exact original bytes copied through OSC52. Your SSH terminal must support clipboard forwarding.",
                    ),
                    Err(e) => modal::info(keys, "Copy error", &e),
                }
            }
        }
    }
}

pub fn keys(keys: &KeyReader) {
    modal::info(
        keys,
        "Keyboard shortcuts",
        "Draft\nEnter: send · Shift/Ctrl-Enter: newline · Alt-Enter: queue\nUp/Down: prompt history · Ctrl-R: searchable history\nCtrl-A/E: start/end · Ctrl-U/K: delete before/after cursor · Ctrl-W: delete word\nCtrl-P or /: command palette · Ctrl-X or /editor: external editor\nCtrl-D: quit · Ctrl-C: clear draft\n\nActive turn\nEnter: steer current work · Alt-Enter: queue a follow-up\nCtrl-C: cancel active provider/tool work, including from a viewer\nPageUp/Ctrl-B: scroll conversation inline · PageDown/Ctrl-F: toward newest\nEnd/Esc: return to bottom · mouse wheel: scroll when supported\nCtrl-G: fullscreen transcript · Ctrl-T: provider reasoning\nTyping keeps an unsent draft; viewers and turn completion preserve it\n\nViewers\nCtrl-G/T: close matching view or switch views · Esc/q: close\nUp/Down, PageUp/PageDown, Home/End: scroll · End/f: follow new text\nTranscript Enter: fold/unfold · t: first turn (idle viewer)\nReasoning uses only provider-exposed text or public summaries\n\nModal selections\nType to search · Up/Down: select · Enter: choose · Esc: back\nF1 or Right: selection details · info views: Up/Down or PageUp/PageDown to scroll\n\nThe app owns one screen and restores your shell on exit. Full recorded conversation is reachable through Ctrl-G or inline scrolling.",
    );
}

pub fn review_report(s: &Session, checkpoint: Option<&Checkpoint>) -> String {
    let records = raw_records(s);
    let start = records
        .iter()
        .rposition(|r| matches!(r, Record::User(_)))
        .unwrap_or(0);
    let commands = records[start..]
        .iter()
        .filter_map(|r| {
            if let Record::Bash {
                command, exit_code, ..
            } = r
            {
                Some(format!(
                    "[{}] {command}",
                    exit_code
                        .map(|c| format!("exit {c}"))
                        .unwrap_or_else(|| "signal".into())
                ))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let summary = records[start..]
        .iter()
        .rev()
        .find_map(|r| {
            if let Record::Assistant(t) = r {
                Some(t.as_str())
            } else {
                None
            }
        })
        .unwrap_or("No completed answer.");
    let diff = checkpoint
        .map(|c| {
            c.diff()
                .unwrap_or_else(|e| format!("Diff unavailable: {e}"))
        })
        .unwrap_or_else(|| "No checkpoint available (requires Git).".into());
    format!(
        "Task summary\n{summary}\n\nCommands run (exit status only; test success is not inferred)\n{}\n\nWorkspace changes\n{}\n\n/checkpoints browse · /undo revert task changes · /restore restore baseline",
        if commands.is_empty() {
            "No shell commands"
        } else {
            &commands
        },
        if diff.is_empty() {
            "No file changes"
        } else {
            &diff
        }
    )
}
pub fn external_editor(draft: &str) -> Result<String, String> {
    let command = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());
    let path = std::env::temp_dir().join(format!(
        "glass-core-draft-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    auth::secure_write(&path, draft.as_bytes())?;
    crate::surface::suspend().map_err(|e| e.to_string())?;
    glass_tui::term::leave().map_err(|e| e.to_string())?;
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{command} \"$1\""))
        .arg("glass-core-editor")
        .arg(&path)
        .status();
    // Resume raw input without purging the conversation's scrollback.
    let enter = glass_tui::term::enable_raw_input();
    let _ = glass_tui::crossterm::execute!(
        std::io::stdout(),
        glass_tui::crossterm::event::EnableBracketedPaste
    );
    let result = match status {
        Ok(s) if s.success() => std::fs::read_to_string(&path).map_err(|e| e.to_string()),
        Ok(_) => Err("Editor exited unsuccessfully; your draft was kept.".into()),
        Err(e) => Err(e.to_string()),
    };
    let _ = std::fs::remove_file(path);
    enter.map_err(|e| e.to_string())?;
    crate::surface::resume().map_err(|e| e.to_string())?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scratch() -> PathBuf {
        let p = std::env::temp_dir().join(format!("glass-polish-{}", auth::random(8).unwrap()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn project_binding_and_titles_are_persistent() {
        let root = scratch();
        let other = scratch();
        let mut s = Session::create(&root).unwrap();
        bind(&s, &root).unwrap();
        s.append(Record::User("A useful task".into()));
        assert_eq!(title(&s), "A useful task");
        let mut m = metadata(&s);
        m.title = "Renamed".into();
        save(&s, &m).unwrap();
        assert_eq!(title(&Session::open(&root, &s.id).unwrap()), "Renamed");
        assert!(bind(&s, &other).is_err());
        std::fs::remove_dir_all(root).ok();
        std::fs::remove_dir_all(other).ok();
    }
    #[test]
    fn attachment_is_a_snapshot_and_can_be_removed() {
        let root = scratch();
        let s = Session::create(&root).unwrap();
        bind(&s, &root).unwrap();
        std::fs::write(root.join("note"), "first version").unwrap();
        attach(&s, &root, "note").unwrap();
        std::fs::write(root.join("note"), "second version").unwrap();
        assert!(prompt_context(&s, &None).unwrap().contains("first version"));
        let mut m = metadata(&s);
        m.attachments.clear();
        save(&s, &m).unwrap();
        assert!(prompt_context(&s, &None).is_none());
        std::fs::write(root.join("huge"), vec![b'x'; 32769]).unwrap();
        assert!(attach(&s, &root, "huge").is_err());
        std::fs::remove_dir_all(root).ok();
    }
    #[test]
    fn task_review_reports_actual_commands_without_claiming_tests_passed() {
        let root = scratch();
        let mut s = Session::create(&root).unwrap();
        s.append(Record::User("test".into()));
        s.append(Record::Bash {
            command: "false".into(),
            output: "".into(),
            exit_code: Some(1),
            truncated: false,
            spill: None,
        });
        let report = review_report(&s, None);
        assert!(report.contains("[exit 1] false"));
        assert!(report.contains("No completed answer"));
        std::fs::remove_dir_all(root).ok();
    }
}
