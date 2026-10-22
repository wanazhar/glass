//! Durable request manifests. Metadata only: never copy prompts or tool results.
use crate::{provider::Request, session::Session};
use glass_mod_api::Message;

pub fn reports(s: &Session) -> Vec<String> {
    use std::io::Read;
    let read = || -> std::io::Result<String> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = options.open(s.path.with_extension("context"))?;
        if !file.metadata()?.is_file() {
            return Err(std::io::Error::other(
                "request context path is not a regular file",
            ));
        }
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        Ok(text)
    };
    match read() {
        Ok(text) => text
            .lines()
            .filter_map(|line| serde_json::from_str::<String>(line).ok())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(_) => vec!["Saved request context could not be read safely.".into()],
    }
}

pub fn save(s: &Session, report: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(s.path.with_extension("context"))?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other(
            "request context path is not a regular file",
        ));
    }
    let start = file.metadata()?.len();
    let mut bytes = serde_json::to_vec(report).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_data()) {
        file.set_len(start)?;
        file.sync_data()?;
        return Err(error);
    }
    Ok(())
}

fn sources(system: &str, tag: &str) -> Vec<String> {
    let prefix = format!("<{tag} path=");
    system
        .split(&prefix)
        .skip(1)
        .filter_map(|rest| {
            let (path, body) = rest.split_once(">\n")?;
            let body = body.split_once(&format!("</{tag}>"))?.0;
            Some(format!(
                "{} · {} bytes{}",
                path,
                body.trim_end_matches('\n').len(),
                if body.contains("[truncated at ") {
                    " · truncated"
                } else {
                    ""
                }
            ))
        })
        .collect()
}

pub fn capture(
    req: &Request<'_>,
    catalogue: Option<&str>,
    extension: &str,
    checkpoint: bool,
) -> String {
    let mut instructions = sources(req.system, "project_instructions");
    instructions.extend(sources(req.system, "neuralnets").into_iter().map(|s|format!("Neuralnets: {s}")));
    let attachments = sources(req.system, "attached_file");
    let listed = catalogue.filter(|c| !c.is_empty() && req.system.contains(c));
    let skills = listed
        .map(|c| {
            c.lines()
                .filter(|l| l.starts_with("- "))
                .map(|l| l.split_once("): ").map_or(l, |(path, _)| path).to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut users = 0;
    let mut assistants = 0;
    let mut results = 0;
    let mut calls = 0;
    let mut images = 0;
    for m in req.messages {
        match m {
            Message::User(_) => users += 1,
            Message::Assistant(_) => assistants += 1,
            Message::ToolResult { .. } => results += 1,
            Message::ToolCall { .. } => calls += 1,
            Message::Image { .. } => images += 1,
            Message::System(_) | Message::ChatReasoning{..} => {}
        }
    }
    let schema_bytes = req
        .tools
        .iter()
        .map(|(n, d, s)| n.len() + d.len() + s.len())
        .sum::<usize>();
    let estimate = crate::provider::default_estimate(req.messages) as usize
        + (req.system.len() + schema_bytes).div_ceil(4);
    let list = |v: Vec<String>| {
        if v.is_empty() {
            "None".into()
        } else {
            v.join("\n")
        }
    };
    format!(
        "Model request · {}\nCaptured before dispatch; transport retries reuse this context. Background cache warming and other sessions are separate.\nApprox. input: ~{estimate} tokens (text estimate including tool schemas; images use a fixed rough allowance; transport overhead excluded). Provider usage is authoritative.\n\nSystem instructions: {} bytes{}\nExtension prompt: {}\n\nProject instruction snapshots included:\n{}\n\nText attachment snapshots included:\n{}\n\nSkill catalogue: {} bytes · descriptions only, not skill bodies\n{}\nSkill/file bodies enter context only when attached or returned by a tool; inspect /details for commands and results.\n\nConversation: {} messages · {users} user · {assistants} assistant · {calls} tool calls · {results} tool results · {images} images\n\nAvailable tools: {} · {schema_bytes} bytes of names, descriptions and schemas\n{}\n\nThis manifest stores metadata only. It shows what was supplied, not proof that the model read or followed it.",
        req.model,
        req.system.len(),
        if checkpoint {
            " · retained compaction checkpoint"
        } else {
            ""
        },
        if !extension.is_empty() && req.system.contains(extension) {
            format!("{} bytes included", extension.len())
        } else {
            "No current extension prompt identified (older checkpoint may differ)".into()
        },
        list(instructions),
        list(attachments),
        listed.map_or(0, str::len),
        list(skills),
        req.messages.len(),
        req.tools.len(),
        list(
            req.tools
                .iter()
                .map(|(n, d, s)| format!("{n} · {} bytes", n.len() + d.len() + s.len()))
                .collect()
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_is_durable_and_does_not_change_model_or_compaction_records() {
        let dir = std::env::temp_dir().join(format!("gc-context-audit-{}", std::process::id()));
        let mut session = Session::create(&dir).unwrap();
        session.append(crate::session::Record::User("hello".into()));
        let before = session.records.len();
        save(&session, "request one").unwrap();
        save(&session, "request two").unwrap();
        assert_eq!(session.records.len(), before);
        assert_eq!(session.log_len(), before);
        assert_eq!(session.messages().len(), 1);
        assert_eq!(reports(&session), ["request one", "request two"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{PermissionsExt, symlink};
            let path = session.path.with_extension("context");
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            std::fs::remove_file(&path).unwrap();
            let target = dir.join("private");
            std::fs::write(&target, "untouched").unwrap();
            symlink(&target, &path).unwrap();
            assert!(save(&session, "must not write").is_err());
            assert_eq!(std::fs::read_to_string(target).unwrap(), "untouched");
            assert!(reports(&session)[0].contains("could not be read safely"));
        }
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn manifest_describes_actual_request_without_copying_contents() {
        let system = "<project_instructions path=\"/tmp/AGENTS.md\">\nSECRET_INSTRUCTION\n</project_instructions>\n<attached_file path=\"/tmp/file\">\nPRIVATE_FILE\n</attached_file>\n- /skills/build/SKILL.md (build): Build things";
        let messages = [Message::User("PRIVATE_USER".into())];
        let tools = [("bash".into(), "shell".into(), "{}".into())];
        let req = Request {
            model: "mock",
            system,
            messages: &messages,
            tools: &tools,
            max_tokens: 1,
            reasoning: None,
        };
        let report = capture(
            &req,
            Some("- /skills/build/SKILL.md (build): Build things"),
            "",
            false,
        );
        assert!(report.contains("/tmp/AGENTS.md"));
        assert!(report.contains("descriptions only"));
        assert!(report.contains("1 user"));
        assert!(report.contains("bash"));
        for secret in [
            "SECRET_INSTRUCTION",
            "PRIVATE_FILE",
            "PRIVATE_USER",
            "Build things",
        ] {
            assert!(!report.contains(secret));
        }
        assert!(
            !capture(&req, Some("unloaded catalogue"), "", true).contains("unloaded catalogue")
        );
    }
}
