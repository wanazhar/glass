//! Single on-demand output inspector for replies, tools, code and spill ranges.
use crate::{
    input::KeyReader,
    modal,
    session::{Record, Session},
};
#[derive(Clone, Copy)]
pub enum Filter {
    All,
    Replies,
    Tools,
}
fn matches(record: &Record, filter: Filter) -> bool {
    match record {
        Record::Assistant(_) => !matches!(filter, Filter::Tools),
        Record::Bash { .. } => !matches!(filter, Filter::Replies),
        Record::ToolResult { .. } => !matches!(filter, Filter::Replies),
        _ => false,
    }
}
fn body(record: &Record) -> &str {
    match record {
        Record::Assistant(t) => t,
        Record::Bash { output, .. } => output,
        Record::ToolResult { body, .. } => body,
        _ => "",
    }
}
fn label(record: &Record) -> String {
    match record {
        Record::Assistant(t) => format!(
            "Reply · {}",
            glass_tui::text::clip(t, 70).replace(['\n', '\r'], " ")
        ),
        Record::Bash {
            command, exit_code, ..
        } => format!(
            "Shell · exit {} · {}",
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".into()),
            glass_tui::text::clip(command, 60)
        ),
        Record::ToolResult { name, .. } => format!("Tool · {name}"),
        _ => String::new(),
    }
}
pub fn view(keys: &KeyReader, session: &Session, filter: Filter) {
    let records = crate::polish::raw_records(session);
    let shell_ids: std::collections::HashSet<_> = records
        .iter()
        .filter_map(|record| match record {
            Record::Bash {
                call_id: Some(id), ..
            } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    let entries: Vec<_> = records.iter().enumerate().rev().filter(|(_,r)|matches(r,filter))
        .filter(|(_,record)| !matches!(record,Record::ToolResult {call_id,name,..} if name=="bash" && shell_ids.contains(call_id.as_str())))
        .collect();
    if entries.is_empty() {
        modal::info(keys, "Outputs", "No matching replies or tool outputs yet.");
        return;
    }
    let mut page = 0usize;
    loop {
        let visible: Vec<_> = entries.iter().skip(page * 100).take(100).collect();
        let mut labels: Vec<_> = visible.iter().map(|(_, r)| label(r)).collect();
        let next = (page + 1) * 100 < entries.len();
        if next {
            labels.push("Older outputs →".into());
        }
        if page > 0 {
            labels.push("← Newer outputs".into());
        }
        let Some(index) = modal::choose(
            keys,
            "Outputs",
            "Select to inspect · c copy · b code · r raw · l full output",
            &labels,
        ) else {
            return;
        };
        if index == visible.len() && next {
            page += 1;
            continue;
        }
        if index >= visible.len() {
            page = page.saturating_sub(1);
            continue;
        }
        inspect(keys, visible[index].1);
    }
}
pub fn inspect(keys: &KeyReader, record: &Record) {
    let raw = body(record);
    let title = label(record);
    let mut loaded: Option<(String, bool)> = None;
    let mut raw_mode = false;
    loop {
        let content = loaded
            .as_ref()
            .map(|(text, _)| text.as_str())
            .unwrap_or(raw);
        let preview = if content.len() > 64 * 1024 {
            let mut end = 64 * 1024;
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            &content[..end]
        } else {
            content
        };
        let mut shown = if !raw_mode {
            serde_json::from_str::<serde_json::Value>(preview)
                .ok()
                .map(|v| crate::structured::render(&v))
                .unwrap_or_else(|| preview.into())
        } else {
            format!("```text\n{preview}\n```")
        };
        if preview.len() < content.len() {
            shown.push_str(
                "\n\n*Preview capped at 64 KiB; copy preserves the complete recorded text.*",
            );
        }
        if let Record::Bash {
            command,
            truncated,
            spill,
            ..
        } = record
        {
            let command_preview = glass_tui::text::clip(command, 4096);
            shown = format!("**Command**\n\n```sh\n{command_preview}\n```\n\n{shown}");
            if loaded.as_ref().is_some_and(|(_, cut)| *cut) {
                shown.push_str("\n\n*Loaded range capped at 64 KiB.*");
            } else if *truncated && loaded.is_none() {
                shown.push_str(if spill.is_some() {
                    "\n\n*Recorded preview is truncated · l loads the saved range.*"
                } else {
                    "\n\n*Recorded preview is truncated; no saved full output.*"
                });
            }
        }
        // Parse/layout only for a selected bounded item, never for the list.
        if shown.len() > 128 * 1024 {
            shown = glass_tui::text::clip(&shown, 64 * 1024);
            shown.push_str("\n\nPreview capped; copy retains exact recorded text.");
        }
        match modal::rich_actions(
            keys,
            &title,
            &shown,
            &[
                ('c', "copy"),
                ('b', "code"),
                ('r', "raw"),
                ('l', "full"),
                ('k', "command"),
            ],
        ) {
            Some('c') => {
                if let Err(e) = crate::workflows::copy(content) {
                    modal::info(keys, "Copy unavailable", &e);
                }
            }
            Some('k') => {
                if let Record::Bash { command, .. } = record
                    && let Err(e) = crate::workflows::copy(command)
                {
                    modal::info(keys, "Copy unavailable", &e);
                }
            }
            Some('r') => raw_mode = !raw_mode,
            Some('b') => {
                if content.len() > 64 * 1024 {
                    modal::info(
                        keys,
                        "Code extraction limit",
                        "Code extraction is capped at 64 KiB. Use c to copy the complete original output; no partial code is presented as complete.",
                    );
                    continue;
                }
                let blocks = glass_tui::markdown::code_blocks(content);
                let labels: Vec<_> = blocks
                    .iter()
                    .enumerate()
                    .map(|(i, (lang, body))| {
                        format!("{} · {} · {} lines", i + 1, lang, body.lines().count())
                    })
                    .collect();
                if labels.is_empty() {
                    modal::info(keys, "Code", "No fenced code blocks in this output.");
                } else if let Some(index) = modal::choose(
                    keys,
                    "Code blocks",
                    "Select a block · c copies exact code",
                    &labels,
                ) {
                    let (lang, code) = &blocks[index];
                    let shown = format!("```{lang}\n{code}\n```");
                    if modal::rich_actions(keys, &labels[index], &shown, &[('c', "copy")])
                        == Some('c')
                        && let Err(e) = crate::workflows::copy(code)
                    {
                        modal::info(keys, "Copy unavailable", &e);
                    }
                }
            }
            Some('l') => match record {
                Record::Bash {
                    spill: Some(path), ..
                } => match crate::polish::load_spill(path) {
                    Ok(value) => loaded = Some(value),
                    Err(e) => modal::info(keys, "Saved output unavailable", &e),
                },
                _ => modal::info(
                    keys,
                    "Saved output",
                    "This item has no recorded spill file.",
                ),
            },
            _ => return,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_and_identity_keep_exact_content() {
        let reply = Record::Assistant("line\n界面".into());
        assert!(matches(&reply, Filter::Replies));
        assert!(!matches(&reply, Filter::Tools));
        assert_eq!(body(&reply), "line\n界面");
        let shell = Record::Bash {
            call_id: Some("id".into()),
            command: "printf test".into(),
            output: "exact\n".into(),
            exit_code: Some(1),
            truncated: false,
            spill: None,
        };
        assert!(matches(&shell, Filter::Tools));
        assert!(!matches(&shell, Filter::Replies));
        assert!(label(&shell).contains("exit 1"));
    }
}
