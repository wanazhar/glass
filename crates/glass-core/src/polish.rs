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

const SESSION_METADATA_MAX_BYTES: usize = 4 * 1024 * 1024;

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
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub text: String,
    /// `"image/png"` etc. for image attachments; `text` is empty and `data`
    /// holds the base64 payload instead.
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    /// Range citation state (task six-wave-e-001): set only for line-range
    /// excerpts. `consumed` flips once the marker has been written into a
    /// user message, so each citation enters the log exactly once.
    #[serde(default)]
    pub citation: Option<Citation>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Citation {
    pub first: usize,
    pub last: usize,
    /// sha256 of the excerpted bytes at citation time.
    pub revision: String,
    #[serde(default)]
    pub consumed: bool,
}
fn meta_path(s: &Session) -> PathBuf {
    s.path.with_extension("meta.json")
}
pub fn metadata(s: &Session) -> Metadata {
    crate::requests::bounded(&meta_path(s), SESSION_METADATA_MAX_BYTES)
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
        let bytes = crate::requests::bounded(&path, SESSION_METADATA_MAX_BYTES)
            .map_err(|_| "Session preferences are invalid or exceed the 4 MiB limit; they have not been overwritten.")?;
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
        .filter_map(|e| {
            Session::open_readonly(&config::data_dir(), &e.path().to_string_lossy()).ok()
        })
        .filter(|s| metadata(s).workspace == root)
        .collect()
}
pub fn browser(
    keys: &KeyReader,
    current: &Session,
    workspace: &Path,
) -> Result<Option<Session>, String> {
    match crate::navigation::choose(keys, workspace, Some(current), false, false, false)? {
        Some(pick) if pick.id != current.id => Session::open(&config::data_dir(), &pick.id)
            .map(Some)
            .map_err(|e| e.to_string()),
        _ => Ok(None),
    }
}
/// Magic-byte sniff for the four image formats every provider accepts.
///
/// No codec: the bytes are relayed as base64, never decoded, which is what
/// keeps the "no image codecs" size argument intact.
pub(crate) fn image_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

/// Sniff and attach `path`.
///
/// Images become session records (`Record::Image`) so every wire format
/// replays them without special cases; text files keep the old snapshot
/// behaviour. The `Session` is mutable because attaching an image is
/// appending to the conversation, not just file-handling bookkeeping.
pub fn attach(s: &mut Session, workspace: &Path, path: &str) -> Result<(), String> {
    let p = workspace
        .join(path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let bytes = crate::requests::bounded(&p, 8 * 1024 * 1024)?;
    let mut m = metadata(s);
    m.attachments.retain(|a| a.path != p);
    if m.attachments.len() >= 128 {
        return Err("Attachment limit reached (128); remove an attachment in /context".into());
    }
    if image_media_type(&bytes).is_some() {
        let prepared = crate::images::prepare(&bytes, path, || false)?;
        let reference = prepared["reference"]
            .as_str()
            .ok_or("missing image reference")?;
        let data_bytes = crate::files::read_workspace_bytes(
            &crate::config::data_dir().join("images"),
            &format!("{reference}.jpg"),
        )?;
        let existing: usize = m
            .attachments
            .iter()
            .filter_map(|a| a.data.as_ref().map(|d| d.len() * 3 / 4))
            .sum();
        if existing + data_bytes.len() > 2_097_152 {
            return Err("Image attachments exceed 2 MiB total. Remove one in /context.".into());
        }
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD.encode(&data_bytes);
        let duplicate = metadata(s)
            .attachments
            .iter()
            .any(|a| a.data.as_deref() == Some(data.as_str()))
            && matches!(s.records.last(), Some(Record::Image {data: old, ..}) if old == &data);
        if !duplicate {
            s.append(Record::Image {
                media_type: "image/jpeg".into(),
                data: data.clone(),
            });
            if let Some(error) = s.write_error() {
                return Err(error.into());
            }
        }
        m.attachments.push(Attachment {
            path: p,
            source: Some(format!(
                "image · {}×{} → {}×{} · {} KiB",
                prepared["width"],
                prepared["height"],
                prepared["sent_width"],
                prepared["sent_height"],
                data_bytes.len().div_ceil(1024)
            )),
            text: String::new(),
            media_type: Some("image/jpeg".into()),
            data: Some(data),
            citation: None,
        });
        save(s, &m)?;
        return Ok(());
    }
    if bytes.len() > 32_768 {
        return Err("File exceeds 32 KiB. Attach a smaller excerpt.".into());
    }
    let text = String::from_utf8(bytes).map_err(|_| "Only UTF-8 text files can be attached.")?;
    if m.attachments.iter().map(|a| a.text.len()).sum::<usize>() + text.len() > 131_072 {
        return Err("Attachments exceed 128 KiB. Remove an attachment in /context.".into());
    }
    m.attachments.push(Attachment {
        path: p,
        source: Some("whole file snapshot".into()),
        text,
        media_type: None,
        data: None,
        citation: None,
    });
    save(s, &m)
}
/// Build an attributed bounded excerpt without running a model or project code.
pub fn attach_excerpt(
    s: &Session,
    workspace: &Path,
    path: &str,
    first: usize,
    count: usize,
) -> Result<(), String> {
    if first == 0 || count == 0 {
        return Err("Line numbers and count must be positive".into());
    }
    let path = workspace
        .join(path)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let bytes = crate::requests::bounded(&path, 8 * 1024 * 1024)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Select a UTF-8 text file")?;
    let mut excerpt = String::new();
    let mut lines = 0;
    for line in text.lines().skip(first - 1).take(count) {
        if excerpt.len().saturating_add(line.len()).saturating_add(1) > 32768 {
            return Err("Excerpt exceeds 32 KiB; narrow the range".into());
        }
        excerpt.push_str(line);
        excerpt.push('\n');
        lines += 1;
    }
    if lines == 0 {
        return Err("No lines at this range".into());
    }
    let revision = crate::workspace::hash(&bytes);
    let mut meta = metadata(s);
    meta.attachments.retain(|a| a.path != path);
    if meta.attachments.iter().map(|a| a.text.len()).sum::<usize>() + excerpt.len() > 131072 {
        return Err("Context attachments exceed 128 KiB".into());
    }
    meta.attachments.push(Attachment {
        path,
        source: Some(format!(
            "lines {first}–{} · revision {revision}",
            first + lines - 1
        )),
        text: excerpt,
        media_type: None,
        data: None,
        citation: Some(Citation {
            first,
            last: first + lines - 1,
            revision: revision.clone(),
            consumed: false,
        }),
    });
    save(s, &meta)
}

/// The in-message citation marker for one attachment (task six-wave-e-001,
/// t3 assistant-citations): `@<file>#L<first>-L<last>#<8-hex revision>`.
/// It is a plain line of the user message — no new Record variant, so old
/// logs, exports and resumptions are untouched — and it survives its
/// source changing or disappearing because the revision hash names the
/// exact bytes that were quoted.
pub fn citation_marker(a: &Attachment) -> Option<String> {
    let c = a.citation.as_ref()?;
    Some(format!(
        "@{}#L{}-L{}#{}",
        a.path.display(),
        c.first,
        c.last,
        &c.revision[..c.revision.len().min(8)]
    ))
}

/// Markers for every range attachment that has not yet been cited in a
/// message, marking them consumed for this session. `session_marker_only`
/// attachments (whole-file) never produce a marker.
pub fn pending_citation_markers(s: &Session) -> Vec<String> {
    let mut meta = metadata(s);
    let mut markers = Vec::new();
    for a in meta.attachments.iter_mut() {
        if let Some(marker) = a
            .citation
            .as_ref()
            .map(|c| !c.consumed)
            .unwrap_or(false)
            .then(|| citation_marker(a))
            .flatten()
        {
            if let Some(c) = a.citation.as_mut() {
                c.consumed = true;
            }
            markers.push(marker);
        }
    }
    if !markers.is_empty()
        && let Err(error) = save(s, &meta)
    {
        eprintln!("citation state not saved: {error}");
        return Vec::new();
    }
    markers
}

/// Append one marker line per uncited range attachment to a submitted
/// prompt, deduplicated against markers already present in the text.
pub fn with_citation_markers(s: &Session, text: &str) -> String {
    let markers = pending_citation_markers(s);
    if markers.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    for m in markers {
        if !out.contains(&m) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&m);
        }
    }
    crate::session::dedupe_citation_markers(&out)
}

pub fn attach_picker(
    keys: &KeyReader,
    s: &mut Session,
    workspace: &Path,
    path: &str,
) -> Result<bool, String> {
    let choices = [
        "Whole file or image",
        "Line range",
        "Preview current attachment",
    ]
    .map(String::from);
    let Some(choice) = modal::choose(keys, "Attach context", path, &choices) else {
        return Ok(false);
    };
    match choice {
        0 => attach(s, workspace, path).map(|_| true),
        1 => {
            let Some(range) = modal::field(
                keys,
                "Attach line range",
                "FIRST:COUNT · e.g. 20:40",
                "",
                false,
            ) else {
                return Ok(false);
            };
            let (first, count) = range.split_once(':').ok_or("Use FIRST:COUNT")?;
            attach_excerpt(
                s,
                workspace,
                path,
                first.trim().parse().map_err(|_| "Invalid first line")?,
                count.trim().parse().map_err(|_| "Invalid line count")?,
            )
            .map(|_| true)
        }
        _ => {
            if let Some(item) = metadata(s)
                .attachments
                .iter()
                .find(|a| a.path.ends_with(path))
            {
                modal::markdown(
                    keys,
                    "Context snapshot",
                    &format!(
                        "{} · ~{} tokens\n\n{}",
                        item.source.as_deref().unwrap_or("saved attachment"),
                        item.text.len().div_ceil(4),
                        item.text
                    ),
                );
                Ok(false)
            } else {
                Err("No snapshot attached for this path yet".into())
            }
        }
    }
}

/// Quote a durable conversation entry as removable, attributed context.
/// The quote is a snapshot and never gains the authority of an instruction file.
pub fn quote_picker(keys: &KeyReader, s: &Session) -> Result<(), String> {
    let records = s.history_records();
    let candidates = records
        .iter()
        .enumerate()
        .filter_map(|(index, r)| match r {
            Record::User(t) => Some((index, "You", t)),
            Record::Assistant(t) => Some((index, "Response", t)),
            Record::ToolResult { name, body, .. } if name != "bash" => Some((index, "Tool", body)),
            Record::Bash { output, .. } => Some((index, "Command output", output)),
            _ => None,
        })
        .rev()
        .take(256)
        .collect::<Vec<_>>();
    let labels = candidates
        .iter()
        .map(|(i, role, t)| {
            format!(
                "{role} · record {i} · {}",
                glass_tui::text::clip(&t.replace('\n', " "), 100)
            )
        })
        .collect::<Vec<_>>();
    let Some(i) = modal::choose(
        keys,
        "Quote conversation context",
        "Newest first · source record stays retained",
        &labels,
    ) else {
        return Ok(());
    };
    let (record, role, text) = candidates[i];
    let preview = glass_tui::text::clip(text, 8000);
    let Some(quote) = modal::validated_field(
        keys,
        "Review quote",
        "Edit this excerpt (up to 32 KiB); source record remains attributed",
        &preview,
        false,
        |s| {
            if s.trim().is_empty() || s.len() > 32768 {
                Err("Choose nonempty text up to 32 KiB".into())
            } else {
                Ok(())
            }
        },
    ) else {
        return Ok(());
    };
    let mut meta = metadata(s);
    if meta.attachments.len() >= 128
        || meta.attachments.iter().map(|a| a.text.len()).sum::<usize>() + quote.len() > 131072
    {
        return Err("Attachment budget reached; remove context items first".into());
    }
    let fingerprint = crate::workspace::hash(text.as_bytes());
    let path = PathBuf::from(format!("quote:{}:{record}:{}", s.id, &fingerprint[..12]));
    meta.attachments.retain(|a| a.path != path);
    meta.attachments.push(Attachment{path,source:Some(format!("Conversation {} · raw record {record} · {role} · source revision {fingerprint} · edited quote snapshot (data)",s.id)),text:quote,media_type:None,data:None,citation:None});
    save(s, &meta)
}

pub fn prompt_context(s: &Session, instructions: &Option<String>) -> Option<String> {
    let mut text = instructions.clone().unwrap_or_default();
    let m = metadata(s);
    match crate::neuralnets::context(&m.workspace) {
        Ok(Some(memory)) => text.push_str(&memory),
        Ok(None) => {}
        Err(error) => eprintln!("Neuralnets was not loaded: {error}"),
    }
    // Images ride as conversation records, not prompt text; only text
    // snapshots belong in the system prompt.
    for a in m.attachments.iter().filter(|a| a.data.is_none()) {
        text.push_str(&format!(
            "\n<attached_file path={:?}>\nSource: {}\n{}\n</attached_file>",
            a.path,
            a.source.as_deref().unwrap_or("saved file snapshot"),
            a.text
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
        .filter(|i| {
            instructions
                .as_ref()
                .is_some_and(|text| text.contains(&format!("path=\"{}\"", i.path.display())))
        })
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
        .map(|a| match &a.data {
            Some(data) => format!(
                "{} · {} · {} KB (sent as an image message)",
                a.path.display(),
                a.media_type.as_deref().unwrap_or("image"),
                data.len() * 3 / 4 / 1024
            ),
            None => format!(
                "{} · {} bytes (saved snapshot)",
                a.path.display(),
                a.text.len()
            ),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = m.attachments.iter().fold(
        cfg.system_prompt
            .len()
            .saturating_add(instructions.as_ref().map_or(0, String::len))
            .saturating_add(catalogue.as_ref().map_or(0, String::len)),
        |total, attachment| total.saturating_add(attachment.text.len()),
    );
    let estimate = add_byte_estimate(crate::provider::default_estimate(&messages), bytes);
    let compaction = s
        .records
        .iter()
        .filter_map(|r| {
            if let Record::Compacted {
                kept,
                dropped,
                summary,
                system,
                ..
            } = r
            {
                let checkpoint = system
                    .as_deref()
                    .map(|sys| {
                        format!(
                            " · prompt checkpoint {}B (replay keeps it until the next compaction)",
                            sys.len()
                        )
                    })
                    .unwrap_or_default();
                Some(format!(
                    "Kept {kept}, dropped {dropped}{checkpoint}\n{}",
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
    let edits = {
        let mut latest: std::collections::HashMap<usize, (String, Option<String>)> =
            std::collections::HashMap::new();
        for record in &s.records {
            if let Record::ContextEdit { turn, mode, text } = record {
                if mode == "restore" {
                    latest.remove(turn);
                } else {
                    latest.insert(*turn, (mode.clone(), text.clone()));
                }
            }
        }
        if latest.is_empty() {
            "None".to_string()
        } else {
            let mut rows: Vec<(usize, String)> = latest
                .into_iter()
                .map(|(turn, (mode, text))| {
                    let detail = match (mode.as_str(), text) {
                        ("replace", Some(text)) => format!("rewritten as: {text}"),
                        ("replace", None) => "rewritten".into(),
                        _ => "hidden".into(),
                    };
                    (turn, detail)
                })
                .collect();
            rows.sort();
            rows.into_iter()
                .map(|(turn, detail)| format!("turn {turn}: {detail}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
    };
    let failure = s
        .records
        .iter()
        .rev()
        .find_map(|r| match r {
            Record::Meta(t) if t.starts_with("error:") => Some(t.as_str()),
            _ => None,
        })
        .unwrap_or("None recorded");
    let historical = crate::context_audit::reports(s);
    let latest = historical
        .last()
        .map(String::as_str)
        .unwrap_or("No request context recorded yet (older sessions may lack manifests).");
    format!(
        "Latest provider failure:\n{failure}\n\nLast request context:\n{latest}\n\nModel: {}\nCurrent configuration (not a historical request):\nEstimated context: {estimate} tokens (includes prompt text; excludes tool schemas)\nMessages: {}\nSystem prompt: {} bytes\nSkill catalogue: {} bytes\n\nLoaded instructions:\n{}\n\nAttached files:\n{}\n\nContext edits:\n{edits}\n\nCompaction:\n{}",
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

fn add_byte_estimate(base: u32, bytes: usize) -> u32 {
    let extra = u32::try_from(bytes.div_ceil(4)).unwrap_or(u32::MAX);
    base.saturating_add(extra)
}
pub fn context(
    keys: &KeyReader,
    s: &mut Session,
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
            "Inspect past model requests".into(),
            "Quote a conversation entry…".into(),
        ];
        let m = metadata(s);
        rows.extend(m.attachments.iter().map(|a| {
            format!(
                "Remove {} · {} · ~{} tokens",
                a.path.display(),
                a.source.as_deref().unwrap_or("snapshot"),
                if a.data.is_some() {
                    2300
                } else {
                    a.text.len().div_ceil(4)
                }
            )
        }));
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
                    attach_picker(keys, s, &cfg.workspace, &path)?;
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
            Some(5) => {
                let reports = crate::context_audit::reports(s);
                if reports.is_empty() {
                    modal::info(keys, "Model requests", "No request context recorded yet.");
                } else {
                    let rows = reports
                        .iter()
                        .enumerate()
                        .rev()
                        .map(|(i, _)| format!("Request {}", i + 1))
                        .collect::<Vec<_>>();
                    if let Some(i) = modal::choose(
                        keys,
                        "Model requests",
                        "Newest first · captured before dispatch",
                        &rows,
                    ) {
                        modal::info(keys, "Request context", &reports[reports.len() - 1 - i]);
                    }
                }
            }
            Some(6) => quote_picker(keys, s)?,
            Some(i) => {
                let mut m = metadata(s);
                m.attachments.remove(i - 7);
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
    s.history_records()
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
pub(crate) fn load_spill(path: &str) -> Result<(String, bool), String> {
    // Content-addressed spill (task six-wave-d-001): `blob:<sha256>` paths
    // read through the blob store, ranged at the same 64 KiB cap. Plain
    // paths keep the file read; a missing plain file is not a blob problem.
    if crate::blobs::hex_of(path).is_some() {
        let mut bytes = crate::blobs::get(path, 0, Some(64 * 1024 + 1))
            .map_err(|error| format!("cannot read recorded blob: {error}"))?;
        let truncated = bytes.len() > 64 * 1024;
        bytes.truncate(64 * 1024);
        return Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated));
    }
    std::fs::File::open(path)
        .map_err(|error| format!("cannot open recorded spill: {error}"))
        .and_then(|file| {
            use std::io::Read;
            let mut bytes = Vec::with_capacity(64 * 1024);
            file.take(64 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("cannot read recorded spill: {error}"))?;
            let truncated = bytes.len() > 64 * 1024;
            bytes.truncate(64 * 1024);
            Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
        })
}

#[derive(Clone, Copy)]
pub(crate) enum ActivityAction {
    Tool,
    Subagent,
    Job(u32),
    Loop(usize),
}

#[derive(Clone)]
pub(crate) struct ActivityItem {
    pub label: String,
    pub status: String,
    pub detail: String,
    pub action: ActivityAction,
}

/// Read-only snapshot assembled from the existing session, Jobs and Loops
/// owners; this view intentionally owns no lifecycle state of its own.
pub(crate) fn activity_items(s: &Session, job_list: &str) -> Vec<ActivityItem> {
    let mut items = Vec::new();
    for entry in s.history_entries() {
        let Some(card) = entry.tool_card else {
            continue;
        };
        let subagent = card.command.starts_with("run_subagents");
        items.push(ActivityItem {
            label: format!(
                "{} · {}",
                if subagent { "subagent" } else { "tool" },
                glass_tui::text::clip(&card.command.replace('\n', " "), 56)
            ),
            status: card.status.clone(),
            detail: crate::session::tool_card_text(&card, true),
            action: if subagent {
                ActivityAction::Subagent
            } else {
                ActivityAction::Tool
            },
        });
    }
    for row in job_list.lines() {
        let (head, rest) = row.split_once(':').unwrap_or((row, "unknown"));
        let Ok(id) = head.parse::<u32>() else {
            continue;
        };
        let status = rest
            .split_once(" — ")
            .map(|(s, _)| s)
            .unwrap_or("unknown")
            .trim()
            .to_string();
        items.push(ActivityItem {
            label: format!("job {id} · {}", glass_tui::text::clip(rest.trim(), 52)),
            status,
            detail: row.to_string(),
            action: ActivityAction::Job(id),
        });
    }
    for (index, entry) in crate::loops::load(s).entries.iter().enumerate() {
        items.push(ActivityItem {
            label: format!(
                "loop {} · {}",
                index + 1,
                glass_tui::text::clip(&entry.prompt, 52)
            ),
            status: format!(
                "scheduled · next in {}s · ran {}×",
                entry.next_at.saturating_sub(crate::loops::now()),
                entry.runs
            ),
            detail: format!(
                "Prompt: {}\nEvery: {} seconds\nNext at: {}\nRuns: {}",
                entry.prompt, entry.every_s, entry.next_at, entry.runs
            ),
            action: ActivityAction::Loop(index),
        });
    }
    items.reverse();
    items
}
pub fn details(keys: &KeyReader, session: &Session) {
    crate::output_view::view(keys, session, crate::output_view::Filter::Tools);
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

    #[test]
    fn byte_token_estimate_saturates_at_u32_max() {
        assert_eq!(add_byte_estimate(10, usize::MAX), u32::MAX);
        assert_eq!(add_byte_estimate(10, 40), 20);
    }

    fn scratch() -> PathBuf {
        let p = std::env::temp_dir().join(format!("glass-polish-{}", auth::random(8).unwrap()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn spill_load_reads_only_the_explicit_64kib_cap() {
        let root = scratch();
        let path = root.join("output.log");
        let content = format!("{}SPILL_TAIL_SENTINEL", "x".repeat(64 * 1024));
        std::fs::write(&path, content).unwrap();
        let (loaded, truncated) = load_spill(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.len(), 64 * 1024);
        assert!(truncated);
        assert!(!loaded.contains("SPILL_TAIL_SENTINEL"));
        std::fs::remove_dir_all(root).ok();
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
        assert_eq!(
            title(&Session::open_readonly(&root, &s.id).unwrap()),
            "Renamed"
        );
        assert!(bind(&s, &other).is_err());
        std::fs::remove_dir_all(root).ok();
        std::fs::remove_dir_all(other).ok();
    }

    #[test]
    fn oversized_session_preferences_are_not_parsed_or_overwritten() {
        let root = scratch();
        let session = Session::create(&root).unwrap();
        let path = meta_path(&session);
        let oversized = vec![b' '; SESSION_METADATA_MAX_BYTES + 1];
        std::fs::write(&path, &oversized).unwrap();

        let error = bind(&session, &root).unwrap_err();
        assert!(error.contains("4 MiB limit"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            oversized.len() as u64
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn attachment_is_a_snapshot_and_can_be_removed() {
        let root = scratch();
        let mut s = Session::create(&root).unwrap();
        bind(&s, &root).unwrap();
        std::fs::write(root.join("note"), "first version").unwrap();
        attach(&mut s, &root, "note").unwrap();
        std::fs::write(root.join("note"), "second version").unwrap();
        assert!(prompt_context(&s, &None).unwrap().contains("first version"));
        let mut m = metadata(&s);
        m.attachments.clear();
        save(&s, &m).unwrap();
        assert!(prompt_context(&s, &None).is_none());
        std::fs::write(root.join("huge"), vec![b'x'; 32769]).unwrap();
        assert!(attach(&mut s, &root, "huge").is_err());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn excerpt_marker_is_stamped_once_into_the_next_message() {
        let root = scratch();
        let s = Session::create(&root).unwrap();
        bind(&s, &root).unwrap();
        std::fs::write(root.join("src.txt"), "alpha\nbeta\ngamma\ndelta\n").unwrap();
        attach_excerpt(&s, &root, "src.txt", 2, 2).unwrap();
        let m = metadata(&s);
        let marker = citation_marker(&m.attachments[0]).unwrap();
        assert!(
            crate::session::citation_marker_of_test(&marker),
            "marker uses the documented @path#L..#hash8 shape: {marker}"
        );
        assert!(marker.contains("@"));
        assert!(marker.contains("#L2-L3#"));
        assert_eq!(marker.matches('#').count(), 2);

        // First submit consumes the pending citation.
        let text = with_citation_markers(&s, "what changed here?");
        assert!(text.starts_with("what changed here?\n"));
        assert!(text.ends_with(&marker));
        assert!(
            with_citation_markers(&s, "next question").ends_with("next question"),
            "consumed citations do not re-stamp later turns"
        );
        // Marker survives the source changing: hash was taken at attach time.
        std::fs::write(root.join("src.txt"), "rewritten").unwrap();
        assert_eq!(
            citation_marker(&metadata(&s).attachments[0]).unwrap(),
            marker
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn image_replacement_deduplicates_and_removal_changes_future_context() {
        let root = scratch();
        let mut session = Session::create(&root).unwrap();
        bind(&session, &root).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3, 3)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        std::fs::write(root.join("shot.png"), png.into_inner()).unwrap();
        attach(&mut session, &root, "shot.png").unwrap();
        session.append(Record::User("later turn".into()));
        attach(&mut session, &root, "shot.png").unwrap();
        let messages = session.messages();
        assert!(
            matches!(messages.last(), Some(glass_mod_api::Message::Image { .. })),
            "reattach must occur after the later user turn"
        );
        assert_eq!(
            session
                .messages()
                .iter()
                .filter(|m| matches!(m, glass_mod_api::Message::Image { .. }))
                .count(),
            1
        );
        let mut meta = metadata(&session);
        meta.attachments.clear();
        save(&session, &meta).unwrap();
        assert!(
            !session
                .messages()
                .iter()
                .any(|m| matches!(m, glass_mod_api::Message::Image { .. }))
        );
        assert!(
            session
                .records
                .iter()
                .any(|r| matches!(r, Record::Image { .. }))
        );
        attach(&mut session, &root, "shot.png").unwrap();
        assert_eq!(
            session
                .messages()
                .iter()
                .filter(|m| matches!(m, glass_mod_api::Message::Image { .. }))
                .count(),
            1
        );
        // Simulate a compacted retained window that no longer contains pixels.
        session
            .records
            .retain(|r| !matches!(r, Record::Image { .. }));
        assert_eq!(
            session
                .messages()
                .iter()
                .filter(|m| matches!(m, glass_mod_api::Message::Image { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn image_attachments_become_records_without_reaching_the_prompt() {
        let root = scratch();
        let mut s = Session::create(&root).unwrap();
        bind(&s, &root).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let png = png.into_inner();
        std::fs::write(root.join("shot.png"), png).unwrap();
        attach(&mut s, &root, "shot.png").unwrap();
        assert!(
            s.records.iter().any(
                |r| matches!(r, Record::Image { media_type, .. } if media_type == "image/jpeg")
            ),
            "image must be recorded so every wire format replays it"
        );
        assert!(prompt_context(&s, &None).is_none(), "not prompt text");
        let entry = s
            .records
            .iter()
            .find(|r| matches!(r, Record::Image { .. }))
            .map(|r| r.to_entry())
            .unwrap();
        assert_eq!(entry.0, glass_tui::Role::User);
        assert!(entry.1.starts_with("[image: jpeg"), "{}", entry.1);

        // Sniffing itself.
        assert_eq!(image_media_type(b"GIF89a-anything"), Some("image/gif"));
        assert_eq!(
            image_media_type(&[0xFF, 0xD8, 0xFF, 0x00]),
            Some("image/jpeg")
        );
        assert_eq!(
            image_media_type(b"RIFF\0\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(image_media_type(b"plain text"), None);

        // A >1 MiB image is refused before anything is written.
        let mut huge = vec![0x89u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        huge.resize(1_100_000, 0);
        std::fs::write(root.join("big.png"), huge).unwrap();
        assert!(attach(&mut s, &root, "big.png").is_err());
        std::fs::remove_dir_all(root).ok();
    }
    #[test]
    fn task_review_reports_actual_commands_without_claiming_tests_passed() {
        let root = scratch();
        let mut s = Session::create(&root).unwrap();
        s.append(Record::User("test".into()));
        s.append(Record::Bash {
            call_id: None,
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

    #[test]
    fn unified_activity_snapshots_tools_subagents_jobs_and_loops_from_existing_owners() {
        let root = scratch();
        let mut s = Session::create(&root).unwrap();
        s.append(Record::User("start work".into()));
        s.append(Record::ToolCall {
            id: "tool-a".into(),
            name: "state".into(),
            arguments: r#"{"items":["ready"]}"#.into(),
        });
        s.append(Record::ToolResult {
            name: "state".into(),
            call_id: "tool-a".into(),
            body: "state updated".into(),
        });
        s.append(Record::ToolCall {
            id: "agent-a".into(),
            name: "run_subagents".into(),
            arguments: r#"{"tasks":["inspect"]}"#.into(),
        });
        s.append(Record::ToolResult {
            name: "run_subagents".into(),
            call_id: "agent-a".into(),
            body: "inspection done".into(),
        });
        crate::loops::command(&s, "1h check status").unwrap();
        let mut jobs = crate::jobs::Jobs::default();
        jobs.start("echo activity-job", &root, &root.join("jobs"))
            .unwrap();
        let items = activity_items(&s, &jobs.list());
        assert!(
            items
                .iter()
                .any(|item| matches!(item.action, ActivityAction::Tool)
                    && item.detail.contains("state updated"))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item.action, ActivityAction::Subagent)
                    && item.detail.contains("inspection done"))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item.action, ActivityAction::Job(_)))
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item.action, ActivityAction::Loop(_))
                    && item.status.contains("scheduled"))
        );
        drop(jobs);
        std::fs::remove_dir_all(root).ok();
    }
}
