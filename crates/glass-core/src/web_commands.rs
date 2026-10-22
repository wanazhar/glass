//! Portable human commands. Inspectors do not hold a session writer; mutations
//! execute in the served session actor, never in an HTTP handler or model tool.
use crate::{config, session::Session, tools::State};
use serde_json::{Value, json};

pub fn readonly(name: &str) -> bool {
    matches!(
        name,
        "usage"
            | "requests"
            | "history"
            | "recap"
            | "recovery"
            | "diagnostics"
            | "guidance"
            | "turns"
            | "review"
            | "skills"
            | "skill-doctor"
            | "prompts"
            | "attention"
            | "branches"
            | "checkpoints"
    )
}
pub fn supported(name: &str) -> bool {
    readonly(name)
        || matches!(
            name,
            "handoff"
                | "doctor"
                | "tasks"
                | "search"
                | "context"
                | "settings-info"
                | "status"
                | "name"
                | "forget"
                | "unforget"
                | "rewrite"
                | "compact"
                | "attach"
                | "file"
                | "quote"
                | "export"
                | "constraints"
                | "loop"
                | "send"
                | "branch"
                | "checkpoint"
                | "diff"
                | "undo"
                | "restore"
                | "todo"
                | "budget"
                | "pacing"
                | "model"
                | "prewalk"
                | "mods"
                | "jobs"
                | "job"
                | "bg"
                | "security"
                | "neuralnets"
                | "profiles"
                | "login"
                | "logout"
                | "plan"
                | "init"
                | "btw"
                | "code-review"
                | "advisor"
                | "pr"
                | "git"
                | "capture"
                | "rewind"
                | "extensions"
                | "agents"
                | "retry"
                | "queue"
                | "queue-run"
        )
}
fn result(name: &str, body: impl Into<Value>, format: &str) -> Value {
    json!({"title":format!("/{name}"),"body":body.into(),"format":format})
}
fn label(value: &Value, key: &str) -> Result<String, String> {
    let s = value[key].as_str().unwrap_or("").trim();
    if s.len() > 60000 {
        return Err("Command value exceeds 60000 bytes".into());
    }
    Ok(s.into())
}
fn checkpoint_dir(s: &Session) -> std::path::PathBuf {
    config::data_dir().join("checkpoints").join(&s.id)
}
pub fn inspect(name: &str, s: &Session, cfg: &config::Config) -> Result<Value, String> {
    let value=match name {
        "usage"=>crate::requests::usage_summary(s)?,
        "requests"=>json!(crate::requests::reports(&s.path)?.into_iter().rev().take(50).collect::<Vec<_>>()),
        "history"=>crate::history_inspector::receipts(s),
        "handoff"=>crate::history_inspector::handoff(s),
        "recap"=>return Ok(result(name,crate::recap::cached(s).unwrap_or_else(||"Your recap will appear after your first message. No extra tokens are used.".into()),"markdown")),
        "recovery"=>crate::recovery::inspector(s),
        "diagnostics"=>crate::diagnostic_bundle::snapshot(s)?,
        "guidance"=>crate::guidance::request(&json!({"action":"catalogue"}))?,
        "turns"=>json!(crate::rewind::turn_checkpoints(s)),
        "review"=>return Ok(result(name,crate::polish::review_report(s,None),"markdown")),
        "attention"=>json!(crate::attention::targets(s)),
        "tasks"=>serde_json::to_value(crate::workspace::load(&cfg.workspace)?).map_err(|e|e.to_string())?,
        "branches"=>crate::navigation::inspect(&cfg.workspace),
        "neuralnets"=>crate::neuralnets::request(&cfg.workspace,&json!({"action":"list"}),None,true)?,
        "context"=>{let instructions=crate::trust::gated(cfg,&crate::agents_md::load(&cfg.workspace),false);let skills=cfg.skills_dir.as_deref().map(crate::skills::load_dir).unwrap_or_default();return Ok(result(name,crate::polish::context_report(s,cfg,&instructions,&crate::skills::catalogue(&skills)),"text"));},
        "doctor"=>serde_json::to_value(crate::doctor::inspect(cfg,&config::default_path(),&config::data_dir())).map_err(|e|e.to_string())?,
        "settings-info"=>crate::settings::snapshot(cfg)?,
        "status"=>json!({"provider":cfg.provider,"model":cfg.model,"connection":crate::auth::status(cfg),"session":s.id,"security":crate::connect::security(cfg)}),
        "skills"=>json!(cfg.skills_dir.as_deref().map(crate::skills::load_dir).unwrap_or_default().iter().map(|skill|json!({"name":skill.name,"description":skill.description,"path":skill.path,"bytes":std::fs::metadata(&skill.path).map(|m|m.len()).unwrap_or(0)})).collect::<Vec<_>>()),
        "skill-doctor"=>return Ok(result(name,crate::skill_doctor(cfg),"text")),
        "prompts"=>json!(s.history_records().into_iter().rev().filter_map(|r|if let crate::session::Record::User(text)=r {Some(text)} else {None}).take(100).collect::<Vec<_>>()),
        "checkpoints"=>json!(std::fs::read_dir(checkpoint_dir(s)).ok().into_iter().flatten().filter_map(Result::ok).filter(|e|e.path().extension().is_some_and(|x|x=="json")).take(256).map(|e|e.path().file_stem().unwrap_or_default().to_string_lossy().into_owned()).collect::<Vec<_>>()),
        _=>return Err("No read-only inspector for this command".into()),
    };
    Ok(result(name, value, "json"))
}

/// Fields are rendered by the browser; values never become model prompts.
pub fn form(name: &str) -> Value {
    let fields: Vec<(&str, &str, &str)> = match name {
        "skills" => vec![("args", "Skill name to read (empty to list)", "text")],
        "doctor" => vec![("action", "inspect | probe", "text")],
        "context" => vec![
            (
                "action",
                "inspect | instructions | system | skills | requests | remove",
                "text",
            ),
            ("args", "Attachment path (remove)", "text"),
        ],
        "neuralnets" => vec![
            (
                "action",
                "mode | list | search | read | save | update | forget | restore | purge",
                "text",
            ),
            ("id", "Memory ID", "text"),
            ("query", "Search query", "text"),
            ("kind", "preference | fact | procedure | lesson", "text"),
            (
                "text",
                "Memory text; for mode: auto | recall | off",
                "textarea",
            ),
            ("expected", "Revision (update/remove/restore)", "number"),
        ],
        "profiles" => vec![
            (
                "action",
                "list | save | use | delete | instructions | global",
                "text",
            ),
            ("args", "Profile name", "text"),
            ("text", "Instructions (optional)", "textarea"),
        ],
        "login" => vec![
            ("action", "list | use | api | oauth | callback", "text"),
            ("args", "Saved connection ID / OAuth attempt ID", "text"),
            (
                "provider",
                "Provider (openai, openai-responses, anthropic)",
                "text",
            ),
            ("endpoint", "API endpoint", "url"),
            ("model", "Model ID", "text"),
            ("label", "Connection name", "text"),
            (
                "secret",
                "API key or complete OAuth callback URL",
                "password",
            ),
        ],
        "logout" => vec![("args", "Connection ID to remove", "text")],
        "plan" => vec![
            ("action", "inspect | generate | execute", "text"),
            ("args", "Task description", "textarea"),
        ],
        "init" => vec![
            ("action", "draft | write", "text"),
            ("args", "Notes for AGENTS.md", "textarea"),
            ("text", "Reviewed AGENTS.md draft (write)", "textarea"),
            ("expected", "Existing file revision (write)", "text"),
        ],
        "btw" => vec![("args", "Side question", "textarea")],
        "code-review" => vec![("args", "Maximum findings (1–50)", "number")],
        "advisor" => vec![
            (
                "action",
                "inspect | request | cadence | disposition",
                "text",
            ),
            ("args", "Cadence (N/off) or agent ID", "text"),
            ("text", "Review disposition", "textarea"),
        ],
        "pr" => vec![
            (
                "action",
                "status | checks | reviews | preview | create",
                "text",
            ),
            ("title", "PR title", "text"),
            ("body_file", "Body file on host", "text"),
            ("base", "Base branch (optional)", "text"),
            ("expected", "Reviewed preview fingerprint (create)", "text"),
        ],
        "git" => vec![
            (
                "action",
                "status | diff | stage | unstage | stage-patch | unstage-patch | revert-patch | revert-file | commit",
                "text",
            ),
            (
                "args",
                "File path (stage/unstage) or commit message",
                "text",
            ),
            ("expected", "Reviewed status fingerprint", "text"),
            (
                "patch",
                "Selected patch including its diff header",
                "textarea",
            ),
        ],
        "capture" => vec![
            ("action", "draft | save", "text"),
            ("args", "Skill name", "text"),
            ("first", "First turn", "number"),
            ("last", "Last turn", "number"),
            ("text", "Reviewed skill draft (save)", "textarea"),
        ],
        "rewind" => vec![
            ("args", "Through turn number", "number"),
            ("action", "keep | files", "text"),
            ("expected", "Reviewed rewind fingerprint (files)", "text"),
        ],
        "extensions" => vec![
            ("action", "list | enable | disable | reload", "text"),
            ("args", "Bundle name or extension command", "text"),
        ],
        "agents" => vec![
            (
                "action",
                "list | transcript | send | resume | interrupt | archive | create | budget | limits",
                "text",
            ),
            ("name", "Teammate name (create)", "text"),
            (
                "options",
                "Optional limits JSON (tokens, output, rounds, reasoning, role, worktree)",
                "textarea",
            ),
            ("id", "Agent ID", "text"),
            ("args", "Task or message", "textarea"),
            ("model", "Model (spawn)", "text"),
        ],
        "handoff" => vec![
            ("action", "inspect | save | preview-import | import", "text"),
            ("args", "Host file path", "text"),
            ("expected", "Reviewed handoff fingerprint", "text"),
        ],
        "queue" => vec![
            (
                "action",
                "list | remove | clear | saved-edit | saved-earlier | saved-later | saved-promote | saved-remove | saved-clear",
                "text",
            ),
            (
                "args",
                "Live request ID or saved index (starting at 1)",
                "text",
            ),
            ("text", "Replacement saved message", "textarea"),
        ],
        "tasks" => vec![
            (
                "action",
                "list | task-add | task-claim | task-finish | task-release | debug-add | debug-evidence | debug-instrument | debug-resolve | artifact-add | artifact-preview | artifact-read | findings-import | findings-show | findings-disposition | role-set | handoff-show | handoff-import | advanced",
                "text",
            ),
            (
                "args",
                "Title, path, note, or advanced arguments",
                "textarea",
            ),
            ("id", "Task / artifact / finding ID", "number"),
            ("owner", "Owner", "text"),
            ("acceptance", "Acceptance criteria", "textarea"),
            ("depends", "Dependency IDs (comma separated)", "text"),
            ("paths", "Owned paths (comma separated)", "text"),
            ("check", "Check performed", "textarea"),
            ("model", "Role model", "text"),
            ("tokens", "Role token allowance", "number"),
            ("offset", "Byte offset", "number"),
            ("limit", "Byte limit", "number"),
        ],
        "search" => vec![("args", "Search conversation text", "text")],
        "name" => vec![("args", "Conversation title", "text")],
        "forget" | "unforget" | "branch" => vec![("args", "Through turn number", "number")],
        "rewrite" => vec![
            ("turn", "Turn number", "number"),
            ("text", "Replacement text", "textarea"),
        ],
        "compact" => vec![(
            "args",
            "What should the summary preserve? (optional)",
            "textarea",
        )],
        "attach" => vec![("args", "File path", "text")],
        "quote" => vec![
            ("args", "Raw conversation record index", "number"),
            ("text", "Reviewed excerpt (optional)", "textarea"),
        ],
        "constraints" => vec![("args", "list | on | off | clear | add TEXT", "text")],
        "loop" => vec![("args", "Loop arguments (empty to list)", "text")],
        "send" => vec![("args", "Session ID followed by message", "textarea")],
        "diff" | "undo" | "restore" => vec![("args", "Checkpoint label", "text")],
        "todo" => vec![
            (
                "action",
                "add | complete | start | block | remove | clear",
                "text",
            ),
            ("args", "Task title (add) or task ID", "text"),
        ],
        "budget" => vec![("args", "USD limit or off (empty to inspect)", "text")],
        "pacing" => vec![("args", "adaptive | maximum (empty to inspect)", "text")],
        "model" => vec![
            ("action", "inspect | list | set", "text"),
            ("args", "Model ID (empty to inspect)", "text"),
        ],
        "prewalk" => vec![("args", "Target model or off (empty to inspect)", "text")],
        "mods" => vec![("args", "Mod name to toggle (empty to inspect)", "text")],
        "job" => vec![
            ("id", "Job ID", "number"),
            ("action", "poll | stop | input", "text"),
            ("text", "Input (optional)", "textarea"),
        ],
        "bg" => vec![("args", "Shell command", "textarea")],
        "security" => vec![
            ("action", "inspect | profile | approve", "text"),
            (
                "args",
                "Profile name or exact command to approve",
                "textarea",
            ),
        ],
        _ => vec![],
    };
    json!({"fields":fields.into_iter().map(|(key,label,kind)|json!({"key":key,"label":label,"kind":kind})).collect::<Vec<_>>(),"confirm":matches!(name,"handoff"|"undo"|"restore"|"forget"|"rewrite"|"bg"|"security"|"profiles"|"login"|"logout"|"git"|"pr"|"init"|"capture"|"rewind"|"extensions"|"agents"|"plan"),"model_request":matches!(name,"compact"|"plan"|"init"|"btw"|"code-review"|"advisor"|"retry"|"queue-run"|"doctor")})
}
pub fn execute(
    name: &str,
    v: &Value,
    s: &mut Session,
    state: &mut State,
    cfg: &mut config::Config,
    factory: &crate::warmer::ProviderFactory,
    ledger: &crate::budget::Shared,
) -> Result<Value, String> {
    if readonly(name) {
        return inspect_request(name, v, s, cfg);
    }
    if matches!(name, "settings-info" | "status") {
        return inspect(name, s, cfg);
    }
    if matches!(name, "quote" | "branch" | "rewrite") && v.get("source_record").is_some() {
        let index = v["source_record"].as_u64().ok_or("Invalid source record")? as usize;
        let records = s.history_records();
        let source = match records.get(index) {
            Some(crate::session::Record::User(t) | crate::session::Record::Assistant(t)) => t,
            Some(
                crate::session::Record::ToolResult { body: t, .. }
                | crate::session::Record::Bash { output: t, .. },
            ) => t,
            _ => return Err("Source activity changed; reload the message".into()),
        };
        if v["source_expected"].as_str() != Some(crate::workspace::hash(source.as_bytes()).as_str())
        {
            return Err("Source activity changed; reload before applying this action".into());
        }
        if name == "quote"
            && v["args"].as_str().and_then(|s| s.parse::<usize>().ok()) != Some(index)
        {
            return Err("Quote source does not match the selected activity".into());
        }
        let turn = records[..=index]
            .iter()
            .filter(|r| matches!(r, crate::session::Record::User(_)))
            .count();
        let chosen = if name == "branch" {
            v["args"].as_str()
        } else {
            v["turn"].as_str()
        };
        if name != "quote"
            && chosen.and_then(|s| s.parse::<usize>().ok())
                != Some(if name == "branch" && v["before"] == true {
                    turn.saturating_sub(1)
                } else {
                    turn
                })
        {
            return Err("Turn changed; reload the selected activity".into());
        }
    }
    let args = label(v, "args")?;
    if matches!(name, "undo" | "restore" | "forget" | "rewrite" | "bg") && v["confirmed"] != true {
        return Err("Explicit human confirmation is required".into());
    }
    let body = match name {
        "handoff" => {
            let action = label(v, "action")?;
            match action.as_str() {
                "" | "inspect" => return inspect(name, s, cfg),
                "save" => {
                    if v["confirmed"] != true {
                        return Err("Confirm saving project context to this path".into());
                    }
                    crate::history_inspector::save_new(
                        std::path::Path::new(&args),
                        &crate::history_inspector::handoff(s),
                    )?;
                    json!({"saved":args})
                }
                "preview-import" | "import" => {
                    let value =
                        crate::history_inspector::read_portable(std::path::Path::new(&args))?;
                    let expected = crate::workspace::hash(value.to_string().as_bytes());
                    if action == "preview-import" {
                        json!({"preview":value,"expected":expected})
                    } else {
                        if v["confirmed"] != true || label(v, "expected")? != expected {
                            return Err("Preview this handoff and confirm its current fingerprint before importing".into());
                        }
                        let imported = crate::history_inspector::import(&value, &cfg.workspace)?;
                        json!({"open_thread":imported.id,"executed":false})
                    }
                }
                _ => return Err("Unknown handoff action".into()),
            }
        }

        "doctor" => {
            if label(v, "action")? == "probe" {
                return Ok(result(
                    name,
                    crate::workflows::diagnostics(
                        cfg,
                        true,
                        &std::sync::atomic::AtomicBool::new(false),
                        Some(std::sync::Arc::clone(factory)),
                    )?,
                    "text",
                ));
            }
            return inspect(name, s, cfg);
        }
        "context" => {
            let action = label(v, "action")?;
            match action.as_str() {
                "" | "inspect" => return inspect(name, s, cfg),
                "remove" => {
                    let mut meta = crate::polish::metadata(s);
                    let before = meta.attachments.len();
                    meta.attachments
                        .retain(|a| a.path.to_string_lossy() != args);
                    if before == meta.attachments.len() {
                        return Err("Choose an attached path from the context report".into());
                    }
                    crate::polish::save(s, &meta)?;
                    return inspect(name, s, cfg);
                }
                "instructions" => {
                    return Ok(result(
                        name,
                        crate::trust::gated(cfg, &crate::agents_md::load(&cfg.workspace), false)
                            .unwrap_or_else(|| "No loaded project instructions".into()),
                        "text",
                    ));
                }
                "system" => return Ok(result(name, cfg.system_prompt.clone(), "text")),
                "skills" => {
                    let skills = cfg
                        .skills_dir
                        .as_deref()
                        .map(crate::skills::load_dir)
                        .unwrap_or_default();
                    return Ok(result(
                        name,
                        crate::skills::catalogue(&skills)
                            .unwrap_or_else(|| "No skills loaded".into()),
                        "text",
                    ));
                }
                "requests" => json!(crate::context_audit::reports(s)),
                _ => {
                    return Err(
                        "Choose inspect, instructions, system, skills, requests or remove".into(),
                    );
                }
            }
        }
        "neuralnets" => {
            let mut request = v.clone();
            let obj = request.as_object_mut().ok_or("Invalid memory request")?;
            obj.retain(|k, _| {
                ["action", "query", "kind", "text", "id", "expected"].contains(&k.as_str())
            });
            if request["action"].as_str().is_none_or(|s| s.is_empty()) {
                request["action"] = json!("list");
            }
            if let Some(n) = v["expected"].as_str().and_then(|s| s.parse::<u64>().ok()) {
                request["expected"] = json!(n);
            } else {
                request.as_object_mut().unwrap().remove("expected");
            }
            crate::neuralnets::request(&cfg.workspace, &request, None, false)?
        }
        "profiles" => crate::profiles::request(
            cfg,
            &label(v, "action")?,
            &args,
            &label(v, "text")?,
            v["confirmed"] == true,
        )?,
        "login" => login(cfg, s, &label(v, "action")?, v)?,
        "logout" => {
            if v["confirmed"] != true {
                return Err("Confirm removing this saved connection and credentials".into());
            }
            crate::auth::edit(|store| {
                let c = store
                    .connections
                    .iter()
                    .find(|c| c.id == args)
                    .cloned()
                    .ok_or("Unknown saved connection")?;
                store.connections.retain(|c| c.id != args);
                if let Some(id) = c.credential {
                    store.credentials.remove(&id);
                }
                if store.active.as_ref() == Some(&args) {
                    store.active = None;
                }
                Ok(())
            })?;
            if cfg.connection.as_ref() == Some(&args) {
                cfg.connection = None;
                cfg.api_key_env = Some("GLASS_CORE_SIGNED_OUT".into());
            }
            json!({"signed_out":args})
        }
        "plan" => {
            let action = label(v, "action")?;
            if action == "execute" {
                if v["confirmed"] != true {
                    return Err("Confirm executing the reviewed plan".into());
                }
                let plan = crate::workflows::portable_plan(
                    cfg,
                    s,
                    std::sync::Arc::clone(factory),
                    "",
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                )?;
                let task = plan["task"].as_str().unwrap_or("");
                let proposal = plan["proposal"]
                    .as_str()
                    .filter(|p| !p.is_empty())
                    .ok_or("Generate and review a plan first")?;
                crate::todo::apply_user(
                    state,
                    s,
                    json!([{"op":"init","items":crate::todo::plan_items(proposal,task)}]),
                )?;
                json!({"start_prompt":format!("Implement this approved plan for {task}:\n{proposal}\nUpdate the native checklist and verify the result.")})
            } else {
                let task = if action == "generate" {
                    if args.is_empty() {
                        return Err("Describe the task".into());
                    }
                    args.as_str()
                } else {
                    ""
                };
                crate::workflows::portable_plan(
                    cfg,
                    s,
                    std::sync::Arc::clone(factory),
                    task,
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                )?
            }
        }
        "init" => {
            if label(v, "action")? == "write" {
                if v["confirmed"] != true {
                    return Err("Confirm writing reviewed AGENTS.md".into());
                }
                let path = cfg.workspace.join("AGENTS.md");
                let old = std::fs::read(&path).unwrap_or_default();
                if label(v, "expected")? != crate::workspace::hash(&old) {
                    return Err(
                        "AGENTS.md changed or revision is missing; draft again before writing"
                            .into(),
                    );
                }
                let text = label(v, "text")?;
                if text.is_empty() {
                    return Err("Review and supply a nonempty draft".into());
                }
                if path.is_symlink() {
                    return Err("AGENTS.md is a symlink; use guarded file tools".into());
                }
                crate::auth::secure_write(&path, text.as_bytes())?;
                json!({"written":true})
            } else {
                let text = crate::workflows::init_draft(
                    cfg,
                    std::sync::Arc::clone(factory),
                    Some(&args),
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    |_| {},
                )?;
                json!({"draft":text,"expected":crate::workspace::hash(&std::fs::read(cfg.workspace.join("AGENTS.md")).unwrap_or_default())})
            }
        }
        "btw" => {
            if args.is_empty() {
                return Err("Enter a side question".into());
            }
            let provider = factory()?;
            let answer = crate::btw::ask(
                provider.as_ref(),
                &cfg.system_prompt,
                &s.messages(),
                &args,
                &cfg.model,
            )
            .map_err(|e| e.to_string())?;
            json!({"answer":answer.text,"input_tokens":answer.input_tokens,"output_tokens":answer.output_tokens,"main_context_changed":false})
        }
        "code-review" => {
            let max = args.parse::<usize>().unwrap_or(10).clamp(1, 50);
            let report = crate::code_review(
                &cfg.workspace,
                &config::data_dir().join("reviews"),
                factory,
                max,
                Some(cfg.system_prompt.clone()),
            )?;
            s.append_checked(&[crate::session::Record::System(format!(
                "code review findings:\n{report}"
            ))])
            .map_err(|e| e.to_string())?;
            return Ok(result(name, report, "markdown"));
        }
        "advisor" => {
            let action = label(v, "action")?;
            let parts = match action.as_str() {
                "" | "inspect" => vec![],
                "request" => vec![action],
                "cadence" => vec![action, args],
                "disposition" => vec![action, args, label(v, "text")?],
                _ => return Err("Choose inspect, request, cadence or disposition".into()),
            };
            crate::advisor::request(s, &parts)?
        }
        "pr" => {
            let action = label(v, "action")?;
            let action = if action.is_empty() { "status" } else { &action };
            let mut parts = vec![action.into()];
            if matches!(action, "preview" | "create") {
                parts.extend([
                    "--title".into(),
                    label(v, "title")?,
                    "--body-file".into(),
                    label(v, "body_file")?,
                ]);
                let base = label(v, "base")?;
                if !base.is_empty() {
                    parts.extend(["--base".into(), base]);
                }
                if action == "create" {
                    if v["confirmed"] != true {
                        return Err("Confirm publishing the reviewed draft PR".into());
                    }
                    parts.extend([
                        "--publish".into(),
                        "--expect-hash".into(),
                        label(v, "expected")?,
                    ]);
                }
            }
            crate::source_control::request(&cfg.workspace, &parts)?
        }
        "git" => git(cfg, v)?,
        "capture" => {
            if label(v, "action")? == "save" {
                if v["confirmed"] != true {
                    return Err("Confirm saving the reviewed skill".into());
                }
                let path = crate::capture::save(
                    &args,
                    &label(v, "text")?,
                    &cfg.skills_dir
                        .clone()
                        .unwrap_or_else(|| config::config_dir().join("skills")),
                )?;
                json!({"saved":path})
            } else {
                return Ok(result(
                    name,
                    crate::capture::draft(
                        s,
                        &args,
                        label(v, "first")?
                            .parse()
                            .map_err(|_| "First turn must be positive")?,
                        label(v, "last")?
                            .parse()
                            .map_err(|_| "Last turn must be positive")?,
                    )?,
                    "markdown",
                ));
            }
        }
        "rewind" => {
            let files = label(v, "action")? == "files";
            let expected = label(v, "expected")?;
            if files && !expected.is_empty() && v["confirmed"] != true {
                return Err("Confirm rewinding workspace files".into());
            }
            crate::rewind::portable(
                s,
                &cfg.workspace,
                args.parse().map_err(|_| "Choose a turn number")?,
                files,
                (!expected.is_empty()).then_some(expected.as_str()),
            )?
        }
        "agents" => {
            let action = label(v, "action")?;
            let action = if action.is_empty() { "list" } else { &action };
            let mut request = json!({"action":action});
            for key in ["id", "model", "name"] {
                let value = label(v, key)?;
                if !value.is_empty() {
                    request[key] = json!(value);
                }
            }
            if !args.is_empty() {
                request["text"] = json!(args);
            }
            request["from"] = json!("human");
            let options = label(v, "options")?;
            if !options.is_empty() {
                let options: Value =
                    serde_json::from_str(&options).map_err(|_| "Limits must be a JSON object")?;
                for (key, value) in options.as_object().ok_or("Limits must be a JSON object")? {
                    if ![
                        "tokens",
                        "output",
                        "rounds",
                        "reasoning",
                        "role",
                        "worktree",
                    ]
                    .contains(&key.as_str())
                    {
                        return Err("Unknown teammate limit".into());
                    }
                    request[key] = value.clone();
                }
            }
            if !matches!(action, "list" | "transcript") && v["confirmed"] != true {
                return Err("Confirm teammate task control".into());
            }
            if let Some(hub) = crate::team::get(&s.id) {
                hub.request(&request)?
            } else {
                crate::team::client(&s.id, &request)?
            }
        }
        "tasks" => {
            let action = label(v, "action")?;
            let id = label(v, "id")?;
            let owner = label(v, "owner")?;
            let parts = match action.as_str() {
                "" | "list" => {
                    if args.is_empty() {
                        vec![]
                    } else {
                        serde_json::from_str::<Vec<String>>(&args).map_err(
                            |_| "Choose a workspace action or supply a JSON argument array",
                        )?
                    }
                }
                "advanced" => serde_json::from_str::<Vec<String>>(&args)
                    .map_err(|_| "Use a JSON argument array")?,
                "task-add" => {
                    let mut parts = vec![
                        "task".into(),
                        "add".into(),
                        args,
                        "--accept".into(),
                        label(v, "acceptance")?,
                    ];
                    for (key, flag) in [("depends", "--depends"), ("paths", "--paths")] {
                        let value = label(v, key)?;
                        if !value.is_empty() {
                            parts.extend([flag.into(), value]);
                        }
                    }
                    parts
                }
                "task-claim" | "task-release" => vec![
                    "task".into(),
                    action.trim_start_matches("task-").into(),
                    id,
                    if owner.is_empty() {
                        "human".into()
                    } else {
                        owner
                    },
                ],
                "task-finish" => vec![
                    "task".into(),
                    "finish".into(),
                    id,
                    if owner.is_empty() {
                        "human".into()
                    } else {
                        owner
                    },
                    label(v, "check")?,
                ],
                "debug-add" => vec!["debug".into(), "add".into(), args],
                "debug-evidence" | "debug-instrument" => vec![
                    "debug".into(),
                    action.trim_start_matches("debug-").into(),
                    id,
                    args,
                ],
                "debug-resolve" => vec!["debug".into(), "resolve".into(), id],
                "artifact-add" => vec![
                    "artifact".into(),
                    "add".into(),
                    label(v, "acceptance")?,
                    args,
                ],
                "artifact-preview" => vec!["artifact".into(), "preview".into(), id],
                "artifact-read" => vec![
                    "artifact".into(),
                    "read".into(),
                    id,
                    label(v, "offset")?,
                    label(v, "limit")?,
                ],
                "findings-import" => vec!["findings".into(), "import".into(), args],
                "findings-show" => vec!["findings".into(), "show".into(), id],
                "findings-disposition" => vec!["findings".into(), "disposition".into(), id, args],
                "role-set" => vec![
                    "role".into(),
                    "set".into(),
                    args,
                    label(v, "model")?,
                    label(v, "tokens")?,
                ],
                "handoff-show" => vec!["handoff".into(), "show".into(), id],
                "handoff-import" => vec!["handoff".into(), args],
                _ => return Err("Choose a workspace action".into()),
            };
            crate::workspace::request(&cfg.workspace, &parts)?
        }
        "search" => {
            let query = args.to_lowercase();
            json!(
                s.history_records()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, r)| {
                        let (_, text) = r.to_entry();
                        if !query.is_empty() && text.to_lowercase().contains(&query) {
                            Some(
                                json!({"record":index,"excerpt":glass_tui::text::clip(&text,2000)}),
                            )
                        } else {
                            None
                        }
                    })
                    .take(100)
                    .collect::<Vec<_>>()
            )
        }
        "name" => {
            if args.is_empty() || args.len() > 200 {
                return Err("Use a title between 1 and 200 bytes".into());
            }
            let mut m = crate::polish::metadata(s);
            m.title = args.clone();
            crate::polish::save(s, &m)?;
            json!({"title":args})
        }
        "forget" | "unforget" => json!(s.context_command(name, &args)?),
        "rewrite" => json!(s.context_command(
            name,
            &format!("{} {}", label(v, "turn")?, label(v, "text")?)
        )?),
        "compact" => {
            let provider = factory()?;
            let (before, after) = crate::compact::run_manual(
                provider.as_ref(),
                s,
                state,
                (!args.is_empty()).then_some(args.as_str()),
            )?;
            json!({"before_estimate":before,"after_estimate":after,"saved":true})
        }
        "file" => {
            if v["confirmed"] != true {
                return Err("Confirm saving this file".into());
            }
            let path = v["path"].as_str().ok_or("Missing file path")?;
            let requested = std::path::Path::new(path);
            if requested.is_absolute()
                || requested
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Choose a relative path within the thread workspace".into());
            }
            let text = v["text"].as_str().ok_or("Missing file text")?;
            if text.len() > 32768 {
                return Err("Browser editing is limited to 32 KiB".into());
            }
            crate::files::execute(&cfg.workspace, "write", v)?
        }
        "attach" => {
            let upload = v["upload"].as_str().unwrap_or("");
            let args = if upload.is_empty() {
                args.clone()
            } else {
                if upload.len() != 64 || !upload.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Invalid upload token".into());
                }
                config::data_dir()
                    .join("web-uploads")
                    .join(&s.id)
                    .join(upload)
                    .to_string_lossy()
                    .into_owned()
            };
            crate::polish::attach(s, &cfg.workspace, &args)?;
            if !upload.is_empty() {
                let name: String = v["upload_name"]
                    .as_str()
                    .unwrap_or("attachment")
                    .chars()
                    .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
                    .take(120)
                    .collect();
                let mut meta = crate::polish::metadata(s);
                if let Some(attachment) = meta
                    .attachments
                    .iter_mut()
                    .find(|a| a.path == std::path::Path::new(&args))
                {
                    attachment.source = Some(format!(
                        "Uploaded {} · {}",
                        name,
                        attachment.source.as_deref().unwrap_or("snapshot")
                    ));
                }
                crate::polish::save(s, &meta)?;
            }
            // The native context/image snapshot is now durable. Staging bytes
            // have no remaining consumer and need not accumulate for the life
            // of this thread. Failed attaches remain bounded for retry.
            if !upload.is_empty() {
                let _ = std::fs::remove_file(&args);
            }
            json!({"attached":args})
        }
        "quote" => {
            if args.is_empty() {
                return Ok(result(
                    name,
                    json!(
                        s.history_records()
                            .iter()
                            .enumerate()
                            .filter_map(|(index, r)| match r {
                                crate::session::Record::User(t)
                                | crate::session::Record::Assistant(t) => Some(
                                    json!({"record":index,"excerpt":glass_tui::text::clip(t,500)})
                                ),
                                crate::session::Record::ToolResult { body: t, .. }
                                | crate::session::Record::Bash { output: t, .. } => Some(
                                    json!({"record":index,"excerpt":glass_tui::text::clip(t,500)})
                                ),
                                _ => None,
                            })
                            .rev()
                            .take(100)
                            .collect::<Vec<_>>()
                    ),
                    "json",
                ));
            }
            let index = args
                .parse::<usize>()
                .map_err(|_| "Choose a raw record index from /history")?;
            let records = s.history_records();
            let record = records.get(index).ok_or("Unknown record index")?;
            let (role, source) = match record {
                crate::session::Record::User(t) => ("You", t),
                crate::session::Record::Assistant(t) => ("Response", t),
                crate::session::Record::ToolResult { body, .. } => ("Tool", body),
                crate::session::Record::Bash { output, .. } => ("Command output", output),
                _ => return Err("Choose a message or tool output record".into()),
            };
            let text = label(v, "text")?;
            let text = if text.is_empty() {
                source.clone()
            } else {
                text
            };
            if text.trim().is_empty() || text.len() > 32768 {
                return Err("Choose nonempty text up to 32 KiB".into());
            }
            let mut meta = crate::polish::metadata(s);
            if meta.attachments.len() >= 128
                || meta.attachments.iter().map(|a| a.text.len()).sum::<usize>() + text.len()
                    > 131072
            {
                return Err("Attachment budget reached".into());
            }
            let revision = crate::workspace::hash(source.as_bytes());
            let path =
                std::path::PathBuf::from(format!("quote:{}:{index}:{}", s.id, &revision[..12]));
            meta.attachments.retain(|a| a.path != path);
            meta.attachments.push(crate::polish::Attachment{path,source:Some(format!("Conversation {} · raw record {index} · {role} · source revision {revision} · edited quote snapshot (data)",s.id)),text,media_type:None,data:None,citation:None});
            crate::polish::save(s, &meta)?;
            json!({"quoted_record":index,"role":role})
        }
        "export" => {
            json!({"download":format!("/api/export?id={}",s.id),"filename":format!("glass-{}.md",s.id)})
        }
        "constraints" => json!(crate::reminders::command(s, &args)?),
        "loop" => json!(crate::loops::command_for(s, cfg, &args)?),
        "send" => json!(crate::inbox::from_session(s, &args)?),
        "branch" => {
            let through = args.parse::<usize>().map_err(|_| "Enter a turn number")?;
            if through > s.raw_turns().map_err(|e| e.to_string())? {
                return Err("Choose an existing turn".into());
            }
            let child = if through == 0 && v["before"] == true && v.get("source_record").is_some() {
                Session::create(&config::data_dir()).map_err(|e| e.to_string())?
            } else {
                s.branch(&config::data_dir(), through)
                    .map_err(|e| e.to_string())?
            };
            crate::polish::save(&child, &crate::polish::metadata(s))?;
            crate::navigation::lineage(&child, &s.id, through)?;
            json!({"open_thread":child.id})
        }
        "checkpoint" => {
            let dir = checkpoint_dir(s);
            let mut c =
                crate::checkpoint::begin(&cfg.workspace, &dir, &format!("manual-{}", s.log_len()))?;
            c.finish(&dir)?;
            json!({"checkpoint":c.label})
        }
        "diff" | "undo" | "restore" => {
            if args.is_empty()
                || !args
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err("Choose a valid checkpoint label".into());
            }
            let dir = checkpoint_dir(s);
            let c = crate::checkpoint::Checkpoint::load(&dir, &args)?;
            json!(match name {
                "diff" => c.diff()?,
                "undo" => c.undo(&dir)?,
                _ => c.restore(&dir)?,
            })
        }
        "todo" => {
            state.restore_todo(&s.history_records())?;
            let action = label(v, "action")?;
            let ops = match action.as_str() {
                "" => return Ok(result(name, state.todo.markdown(), "markdown")),
                "add" => json!([{"op":"add","items":[{"title":args}]}]),
                "clear" => {
                    if v["confirmed"] != true {
                        return Err("Confirm clearing the checklist".into());
                    }
                    json!([{"op":"clear"}])
                }
                "remove" => json!([{"op":"remove","id":args}]),
                "complete" | "start" | "block" => {
                    json!([{"op":"update","id":args,"status":match action.as_str(){"complete"=>"completed","start"=>"in_progress",_=>"blocked"},"blocker":if action=="block"{"Blocked by human"}else{""}}])
                }
                _ => return Err("Choose add, complete, start, block, remove, or clear".into()),
            };
            crate::todo::apply_user(state, s, ops)?;
            return Ok(result(name, state.todo.markdown(), "markdown"));
        }
        "budget" => {
            let mut l = ledger.lock().map_err(|_| "Budget is unavailable")?;
            if !args.is_empty() {
                l.limit = if args == "off" {
                    None
                } else {
                    Some(
                        args.parse::<f64>()
                            .ok()
                            .filter(|n| n.is_finite() && *n > 0.0)
                            .ok_or("Use a positive finite USD amount or off")?,
                    )
                };
                crate::budget::save(&s.path.with_extension("cost.json"), &l)?;
            }
            json!({"spent":l.spent,"limit":l.limit})
        }
        "pacing" => {
            if !args.is_empty() {
                cfg.adaptive_pacing = match args.as_str() {
                    "adaptive" => true,
                    "maximum" => false,
                    _ => return Err("Choose adaptive or maximum".into()),
                };
                crate::profiles::update_active(cfg)?;
            }
            json!({"adaptive":cfg.adaptive_pacing,"server_deadlines":"Retry-After and provider reset deadlines always apply"})
        }
        "model" => {
            if label(v, "action")? == "list" {
                let key = crate::auth::resolve(cfg)?;
                let models = crate::onboarding::fetch_models(cfg, key)?;
                return Ok(result(
                    name,
                    json!({"models":models.iter().map(|m|json!({"id":m.id,"label":m.label})).collect::<Vec<_>>()}),
                    "json",
                ));
            }
            if !args.is_empty() {
                if args.len() > 256 {
                    return Err("Model ID is too long".into());
                }
                cfg.model = args.clone();
                if let Some(id) = &cfg.connection {
                    crate::auth::edit(|store| {
                        if let Some(c) = store.connections.iter_mut().find(|c| &c.id == id) {
                            c.model = args.clone();
                        }
                        Ok(())
                    })?;
                }
                crate::profiles::update_active(cfg)?;
            }
            json!({"model":cfg.model,"provider":cfg.provider})
        }
        "prewalk" => {
            if args == "off" {
                cfg.prewalk.clear();
            } else if !args.is_empty() {
                let from = cfg.model.clone();
                let note = crate::checkpoint_tools::prewalk_note(s, &from, &args)?;
                cfg.model = args.clone();
                return Ok(result(name, note, "text"));
            }
            json!({"target":cfg.prewalk,"model":cfg.model})
        }
        "mods" => {
            if !args.is_empty() {
                if !crate::mods::available().contains(&args) {
                    return Err("Unknown mod".into());
                }
                if let Some(i) = cfg.mods.iter().position(|m| m == &args) {
                    cfg.mods.remove(i);
                } else {
                    cfg.mods.push(args);
                }
                crate::profiles::update_active(cfg)?;
            }
            json!({"available":crate::mods::available(),"enabled":cfg.mods})
        }
        "jobs" | "job" | "bg" => {
            let action = if name == "jobs" {
                "list".into()
            } else if name == "bg" {
                "start".into()
            } else {
                label(v, "action")?
            };
            let id = label(v, "id")?.parse::<u32>().unwrap_or(0);
            let command = if name == "bg" {
                args.clone()
            } else {
                String::new()
            };
            let mut request = json!({"job":{"action":action,"id":id,"text":label(v,"text")?}});
            if name == "bg" {
                request["command"] = json!(command);
            }
            let call = glass_mod_api::ToolCall {
                id: "human-command".into(),
                name: "bash".into(),
                arguments: request.to_string(),
            };
            let approvals = crate::permissions::approvals_for_command(&command)?;
            crate::jobs::dispatch(&call, &cfg.workspace, &s.id, &approvals)?
        }
        "security" => {
            if label(v, "action")? == "approve" {
                if args.is_empty() || v["confirmed"] != true {
                    return Err("Confirm the exact command to approve for this session".into());
                }
                crate::permissions::approve_session_command(&args)?;
                return Ok(result(
                    name,
                    "Exact command exception approved for this session only.",
                    "text",
                ));
            }
            if args.is_empty() {
                crate::connect::security(cfg)
            } else {
                if v["confirmed"] != true {
                    return Err("Confirm changing the permission profile".into());
                }
                let profile = crate::sandbox::PermissionProfile::parse(&args)
                    .ok_or("Choose restricted, read-only, workspace-edit, or unrestricted")?;
                crate::permissions::persist_profile(&config::default_path(), profile)?;
                json!({"saved":args,"effective_after":"Restart GC Connect and the thread worker; the current policy is unchanged"})
            }
        }
        _ => return Err(format!("/{name} has no portable handler yet")),
    };
    Ok(result(name, body, "json"))
}

pub(crate) fn changes(cfg: &config::Config) -> Result<Value, String> {
    let mut value = git(cfg, &json!({"action":"status"}))?;
    let raw = crate::workflows::git(&cfg.workspace, &["status", "--porcelain=v1", "-z"], None)?;
    let mut records = raw.split(|b| *b == 0).filter(|r| !r.is_empty());
    let mut files = Vec::new();
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        let path =
            String::from_utf8(record[3..].to_vec()).map_err(|_| "Git paths must be UTF-8")?;
        let renamed = matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C');
        let old = if renamed {
            records
                .next()
                .map(|p| String::from_utf8_lossy(p).into_owned())
        } else {
            None
        };
        files.push(json!({"path":path,"index":(record[0] as char).to_string(),"worktree":(record[1] as char).to_string(),"oldPath":old}));
        if files.len() >= 512 {
            return Err(
                "Changes review is limited to 512 files; narrow the repository changes first"
                    .into(),
            );
        }
    }
    value["files"] = json!(files);
    // Preserve the actor's fingerprint over HEAD + index while presenting the
    // index-to-worktree patch required by stage/revert hunk operations.
    value["diff"] = json!(
        String::from_utf8(crate::workflows::git(
            &cfg.workspace,
            &["diff", "--no-ext-diff", "--no-textconv", "--binary"],
            None
        )?)
        .map_err(|e| e.to_string())?
    );
    if value.to_string().len() > 2_000_000 {
        return Err(
            "Changes preview exceeds 2 MB; use native Git review or narrow the changes".into(),
        );
    }
    Ok(value)
}

fn git(cfg: &config::Config, v: &Value) -> Result<Value, String> {
    let capture = |args: &[&str]| {
        String::from_utf8(crate::workflows::git(&cfg.workspace, args, None)?)
            .map_err(|e| e.to_string())
    };
    let status = capture(&["status", "--short"])?;
    let head =
        crate::workflows::git(&cfg.workspace, &["rev-parse", "--verify", "HEAD"], None).is_ok();
    let diff = if head {
        capture(&["diff", "--no-ext-diff", "--no-textconv", "HEAD", "--binary"])?
    } else {
        capture(&["diff", "--no-ext-diff", "--no-textconv", "--binary"])?
    };
    let staged = capture(&[
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--cached",
        "--binary",
    ])?;
    let expected = crate::workspace::hash(format!("{status}\0{diff}\0{staged}").as_bytes());
    let action = label(v, "action")?;
    let arg = label(v, "args")?;
    match action.as_str() {
        "" | "status" => {
            Ok(json!({"status":status,"diff":diff,"staged":staged,"expected":expected}))
        }
        "diff" => Ok(json!({"diff":diff,"expected":expected})),
        "stage" | "unstage" | "stage-patch" | "unstage-patch" | "revert-patch" | "revert-file"
        | "commit" => {
            if v["confirmed"] != true || label(v, "expected")? != expected {
                return Err("Review /git status and confirm its current fingerprint before changing the index or committing".into());
            }
            if arg.is_empty() && !action.ends_with("patch") {
                return Err("Supply a path or commit message".into());
            }
            let output = match action.as_str() {
                "stage" => capture(&["add", "--", &arg])?,
                "unstage" => capture(&["restore", "--staged", "--", &arg])?,
                "revert-file" => capture(&["restore", "--", &arg])?,
                "stage-patch" | "unstage-patch" | "revert-patch" => {
                    let patch = label(v, "patch")?;
                    if patch.is_empty() {
                        return Err("Supply the selected patch with its diff header".into());
                    }
                    let mut options = vec!["apply"];
                    if action != "revert-patch" {
                        options.push("--cached");
                    }
                    if action != "stage-patch" {
                        options.push("--reverse");
                    }
                    let mut check = options.clone();
                    check.extend(["--check", "-"]);
                    crate::workflows::git(&cfg.workspace, &check, Some(patch.as_bytes()))?;
                    options.push("-");
                    String::from_utf8(crate::workflows::git(
                        &cfg.workspace,
                        &options,
                        Some(patch.as_bytes()),
                    )?)
                    .map_err(|e| e.to_string())?
                }
                _ => capture(&["commit", "-m", &arg])?,
            };
            Ok(json!({"output":output,"status":capture(&["status","--short"])?}))
        }
        _ => Err("Choose status, diff, stage, unstage, or commit".into()),
    }
}
struct LoginAttempt {
    session: String,
    created: std::time::Instant,
    attempt: crate::auth::Attempt,
}
fn login_attempts() -> &'static std::sync::Mutex<std::collections::HashMap<String, LoginAttempt>> {
    static STORE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, LoginAttempt>>,
    > = std::sync::OnceLock::new();
    STORE.get_or_init(Default::default)
}
fn login(cfg: &mut config::Config, s: &Session, action: &str, v: &Value) -> Result<Value, String> {
    let connections = || -> Result<Value, String> {
        Ok(json!(
            crate::auth::load()?
                .connections
                .iter()
                .map(|c| json!({"id":c.id,"label":c.label,"provider":c.provider,"model":c.model}))
                .collect::<Vec<_>>()
        ))
    };
    match action {
        "" | "list" => Ok(json!({"connections":connections()?,"current":cfg.connection})),
        "use" => {
            if v["confirmed"] != true {
                return Err("Confirm switching the saved connection".into());
            }
            let id = label(v, "args")?;
            let c = crate::auth::edit(|store| {
                let c = store
                    .connections
                    .iter()
                    .find(|c| c.id == id)
                    .cloned()
                    .ok_or("Unknown saved connection")?;
                store.active = Some(id.clone());
                Ok(c)
            })?;
            crate::auth::configure(cfg, &c);
            Ok(json!({"connected":c.label,"model":c.model}))
        }
        "oauth" => {
            let mut attempts = login_attempts().lock().map_err(|_| "Sign-in unavailable")?;
            attempts.retain(|_, v| v.created.elapsed().as_secs() < 600);
            if attempts.len() >= 8 {
                return Err("Too many sign-in attempts; wait ten minutes".into());
            }
            let mut store = crate::auth::load()?;
            let attempt = crate::auth::Attempt::start(&mut store, None)?;
            let id = crate::auth::random(24)?;
            let url = attempt.url.clone();
            attempts.insert(
                id.clone(),
                LoginAttempt {
                    session: s.id.clone(),
                    created: std::time::Instant::now(),
                    attempt,
                },
            );
            Ok(
                json!({"attempt":id,"authorize_url":url,"instructions":"Open the sign-in link. After authorization, paste the complete final localhost callback URL here using action callback. The callback may not load on your phone; its URL is sufficient."}),
            )
        }
        "api" | "callback" => {
            let requested_model = label(v, "model")?;
            if requested_model.is_empty()
                || requested_model.len() > 256
                || requested_model.chars().any(char::is_control)
            {
                return Err("Enter a valid model ID up to 256 bytes before signing in".into());
            }
            if v["confirmed"] != true {
                return Err("Confirm saving this connection on the host".into());
            }
            let id = crate::auth::random(16)?;
            let (provider, endpoint, credential) = if action == "callback" {
                let attempt_id = label(v, "args")?;
                let attempt = {
                    let mut attempts =
                        login_attempts().lock().map_err(|_| "Sign-in unavailable")?;
                    let a = attempts
                        .get(&attempt_id)
                        .ok_or("Sign-in attempt missing or expired")?;
                    if a.session != s.id || a.created.elapsed().as_secs() >= 600 {
                        return Err(
                            "Sign-in attempt does not belong to this session or expired".into()
                        );
                    }
                    attempts.remove(&attempt_id).unwrap().attempt
                };
                (
                    "openai-responses".into(),
                    "https://api.openai.com/v1".into(),
                    attempt.exchange(label(v, "secret")?)?,
                )
            } else {
                let provider = label(v, "provider")?;
                if !matches!(
                    provider.as_str(),
                    "openai" | "openai-responses" | "anthropic"
                ) {
                    return Err("Choose openai, openai-responses, or anthropic".into());
                }
                let endpoint = label(v, "endpoint")?;
                crate::provider::validate_endpoint(&endpoint).map_err(|e| e.to_string())?;
                let key = label(v, "secret")?;
                if key.is_empty() {
                    return Err("Enter an API key".into());
                }
                let credential = crate::auth::Credential {
                    access_token: key,
                    refresh_token: String::new(),
                    expires_at: 0,
                    client_id: String::new(),
                    subject: String::new(),
                    email: String::new(),
                    id_token: String::new(),
                    scopes: Vec::new(),
                };
                (provider, endpoint, credential)
            };
            let model = label(v, "model")?;
            if model.is_empty() || model.len() > 256 {
                return Err("Enter a model ID up to 256 bytes".into());
            }
            let name = label(v, "label")?;
            let connection = crate::auth::Connection {
                id: id.clone(),
                label: if name.is_empty() {
                    provider.clone()
                } else {
                    name
                },
                provider,
                base_url: endpoint,
                model,
                api_key_env: None,
                credential: Some(id.clone()),
            };
            crate::auth::edit(|store| {
                store.credentials.insert(id.clone(), credential);
                store.connections.push(connection.clone());
                store.active = Some(id.clone());
                Ok(())
            })?;
            crate::auth::configure(cfg, &connection);
            Ok(json!({"connected":connection.label,"model":connection.model}))
        }
        _ => Err("Choose list, use, api, oauth, or callback".into()),
    }
}

pub fn catalogue(extensions: Option<&crate::ext::Registry>) -> Vec<Value> {
    let commands = extensions
        .map(crate::actions::commands)
        .unwrap_or_else(crate::actions::catalogue);
    commands.into_iter().map(|c|{
        let group=match crate::actions::group(&c.name){crate::actions::Group::Conversations=>"conversations",crate::actions::Group::Work=>"work",crate::actions::Group::Settings=>"settings"};
        let extension=extensions.is_some_and(|e|e.has_command(&c.name)) && !crate::actions::catalogue().iter().any(|builtin|builtin.name==c.name);
        let form=if extension{json!({"fields":[{"key":"args","label":"Extension arguments","kind":"text"}]})}else{form(&c.name)};
        json!({"name":c.name,"description":c.description,"hint":c.hint,"aliases":c.aliases,"keywords":c.keywords,"primary":c.primary,"group":group,"portable":supported(&c.name)||extension,"readonly":readonly(&c.name),"form":form})
    }).collect()
}

pub fn inspect_request(
    name: &str,
    values: &Value,
    s: &Session,
    cfg: &config::Config,
) -> Result<Value, String> {
    if name == "skills" {
        let selected = label(values, "args")?;
        if !selected.is_empty() {
            let skills = cfg
                .skills_dir
                .as_deref()
                .map(crate::skills::load_dir)
                .unwrap_or_default();
            let skill = skills
                .iter()
                .find(|skill| skill.name == selected)
                .ok_or("Unknown skill name")?;
            let bytes = crate::requests::bounded(&skill.path, 128 * 1024)?;
            return Ok(result(
                name,
                String::from_utf8(bytes).map_err(|_| "Skill must be UTF-8")?,
                "markdown",
            ));
        }
    }
    inspect(name, s, cfg)
}
pub fn inspect_only(name: &str, v: &Value) -> bool {
    if readonly(name) || matches!(name, "settings-info" | "status" | "export" | "search") {
        return true;
    }
    let action = v["action"].as_str().unwrap_or("");
    let args = v["args"].as_str().unwrap_or("");
    if matches!(
        name,
        "handoff"
            | "context"
            | "doctor"
            | "profiles"
            | "neuralnets"
            | "plan"
            | "advisor"
            | "git"
            | "pr"
            | "agents"
            | "extensions"
            | "queue"
    ) {
        return matches!(
            action,
            "" | "list"
                | "inspect"
                | "status"
                | "diff"
                | "checks"
                | "reviews"
                | "read"
                | "search"
                | "transcript"
                | "instructions"
                | "system"
                | "skills"
                | "requests"
        ) && !(name == "profiles" && action == "instructions");
    }
    matches!(
        name,
        "tasks"
            | "todo"
            | "budget"
            | "pacing"
            | "model"
            | "mods"
            | "jobs"
            | "security"
            | "prewalk"
            | "constraints"
            | "loop"
            | "login"
            | "quote"
    ) && args.is_empty()
        && matches!(action, "" | "inspect" | "list")
}

pub fn bounded_result(mut value: Value) -> Value {
    let bytes = value["body"].to_string();
    if bytes.len() > 384 * 1024 {
        let mut end = 384 * 1024;
        while !bytes.is_char_boundary(end) {
            end -= 1;
        }
        value["body"] = json!(format!(
            "{}\n\n[Display clipped at 384 KiB. Open a narrower view or download the transcript.]",
            &bytes[..end]
        ));
        value["format"] = json!("text");
        value["truncated"] = json!(true);
    }
    value
}
pub fn heavy(name: &str, v: &Value) -> bool {
    match name {
        "compact" | "btw" | "code-review" => true,
        "plan" => v["action"] == "generate",
        "init" => v["action"] != "write",
        "doctor" => v["action"] == "probe",
        _ => false,
    }
}
struct CancelProvider {
    inner: Box<dyn crate::provider::Provider>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl crate::provider::Provider for CancelProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn context_window(&self) -> u32 {
        self.inner.context_window()
    }
    fn rate_card(&self) -> Option<crate::pricing::RateCard> {
        self.inner.rate_card()
    }
    fn estimate_tokens(&self, messages: &[glass_mod_api::Message]) -> u32 {
        self.inner.estimate_tokens(messages)
    }
    fn stream(
        &self,
        request: &crate::provider::Request<'_>,
    ) -> Result<Box<dyn Iterator<Item = crate::provider::Event> + '_>, crate::provider::Error> {
        crate::provider::with_request_cancel(std::sync::Arc::clone(&self.cancel), || {
            self.inner.stream(request)
        })
    }
}
pub fn start_heavy(
    name: String,
    v: Value,
    path: std::path::PathBuf,
    cfg: config::Config,
    factory: crate::warmer::ProviderFactory,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::sync::mpsc::Receiver<Result<Value, String>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let token = std::sync::Arc::clone(&cancel);
        let cancel_factory = std::sync::Arc::clone(&cancel);
        let wrapped: crate::warmer::ProviderFactory = std::sync::Arc::new(move || {
            Ok(Box::new(CancelProvider {
                inner: factory()?,
                cancel: std::sync::Arc::clone(&cancel_factory),
            }))
        });
        let outcome = crate::provider::with_request_cancel(token, || -> Result<Value, String> {
            let s = Session::open_readonly(
                &config::data_dir(),
                path.to_str().ok_or("Invalid session path")?,
            )
            .map_err(|e| e.to_string())?;
            let args = label(&v, "args")?;
            let body = match name.as_str() {
                "plan" => crate::workflows::portable_plan(
                    &cfg,
                    &s,
                    std::sync::Arc::clone(&wrapped),
                    &args,
                    std::sync::Arc::clone(&cancel),
                )?,
                "init" => {
                    let draft = crate::workflows::init_draft(
                        &cfg,
                        std::sync::Arc::clone(&wrapped),
                        Some(&args),
                        std::sync::Arc::clone(&cancel),
                        |_| {},
                    )?;
                    json!({"draft":draft,"expected":crate::workspace::hash(&std::fs::read(cfg.workspace.join("AGENTS.md")).unwrap_or_default())})
                }
                "btw" => {
                    if args.is_empty() {
                        return Err("Enter a side question".into());
                    }
                    let provider = wrapped()?;
                    let answer = crate::btw::ask(
                        provider.as_ref(),
                        &cfg.system_prompt,
                        &s.messages(),
                        &args,
                        &cfg.model,
                    )
                    .map_err(|e| e.to_string())?;
                    json!({"answer":answer.text,"input_tokens":answer.input_tokens,"output_tokens":answer.output_tokens,"main_context_changed":false})
                }
                "code-review" => {
                    let report = crate::code_review(
                        &cfg.workspace,
                        &config::data_dir().join("reviews"),
                        &wrapped,
                        args.parse::<usize>().unwrap_or(10).clamp(1, 50),
                        Some(cfg.system_prompt.clone()),
                    )?;
                    json!({"review_report":report})
                }
                "doctor" => {
                    json!({"text":crate::workflows::diagnostics(&cfg,true,&cancel,Some(std::sync::Arc::clone(&wrapped)))?})
                }
                "compact" => {
                    let provider = wrapped()?;
                    let mut state = State::new();
                    state.restore_todo(&s.history_records())?;
                    for r in &s.records {
                        if let crate::session::Record::State(items) = r {
                            state.apply(&glass_mod_api::ToolCall {
                                id: "restore".into(),
                                name: "state".into(),
                                arguments: json!({"items":items}).to_string(),
                            });
                        }
                    }
                    let (before, summary) = crate::compact::manual_summary(
                        provider.as_ref(),
                        &s,
                        &state,
                        (!args.is_empty()).then_some(args.as_str()),
                    )?;
                    json!({"summary":summary,"before_estimate":before})
                }
                _ => return Err("Unknown model command".into()),
            };
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err(
                    "Command cancelled. Completed model usage remains accounted for.".into(),
                );
            }
            Ok(result(&name, body, "json"))
        });
        let _ = tx.send(outcome);
    });
    rx
}
pub fn finish_heavy(
    name: &str,
    outcome: Result<Value, String>,
    s: &mut Session,
    state: &State,
    cancelled: bool,
) -> Result<Value, String> {
    if cancelled {
        return Err("Command cancelled. Completed model usage remains accounted for.".into());
    }
    let mut value = match outcome {
        Ok(value) => value,
        Err(error) if name == "compact" => {
            crate::compact::fallback(s, state, &error)?;
            return Ok(result(
                name,
                json!({"saved":true,"method":"local extractive checkpoint","model_error":crate::live::sanitize_provider_detail(&error)}),
                "json",
            ));
        }
        Err(error) => return Err(error),
    };
    if name == "compact" {
        let summary = value["body"]["summary"]
            .as_str()
            .ok_or("Summary is missing")?;
        crate::compact::commit_manual_summary(s, summary)?;
        value["body"].as_object_mut().unwrap().remove("summary");
        value["body"]["saved"] = json!(true);
    } else if name == "code-review" {
        let report = value["body"]["review_report"]
            .as_str()
            .ok_or("Review report is missing")?
            .to_owned();
        s.append_checked(&[crate::session::Record::System(format!(
            "code review findings:\n{report}"
        ))])
        .map_err(|e| e.to_string())?;
        value = result(name, report, "markdown");
    }
    Ok(value)
}

#[cfg(test)]
mod mutation_tests {
    use super::*;
    #[test]
    fn naming_is_local_and_unconfirmed_destructive_commands_do_nothing() {
        let path = std::env::temp_dir().join(format!("gc-web-commands-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let mut session = Session::create(&path).unwrap();
        let mut state = State::new();
        let mut cfg = config::Config {
            workspace: path.clone(),
            ..Default::default()
        };
        let factory: crate::warmer::ProviderFactory =
            std::sync::Arc::new(|| panic!("Local command contacted a model"));
        let ledger = crate::budget::load(&session.path.with_extension("cost.json"), None);
        execute(
            "name",
            &json!({"args":"Browser command fixture"}),
            &mut session,
            &mut state,
            &mut cfg,
            &factory,
            &ledger,
        )
        .unwrap();
        assert_eq!(
            crate::polish::metadata(&session).title,
            "Browser command fixture"
        );
        for name in ["undo", "restore", "forget", "rewrite", "bg"] {
            assert!(
                execute(
                    name,
                    &json!({"args":"x"}),
                    &mut session,
                    &mut state,
                    &mut cfg,
                    &factory,
                    &ledger
                )
                .unwrap_err()
                .contains("confirmation")
            );
        }
        assert_eq!(session.log_len(), 0);
        drop(session);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_native_command_has_a_browser_route() {
        let local = [
            "conversations",
            "work",
            "settings",
            "help",
            "outputs",
            "thinking",
            "theme",
            "redraw",
            "keys",
            "animation",
            "tools",
            "clear",
            "editor",
        ];
        for c in crate::actions::catalogue() {
            assert!(
                supported(&c.name) || local.contains(&c.name.as_str()),
                "Missing /{}",
                c.name
            );
            for alias in c.aliases {
                assert_eq!(crate::actions::canonical(&alias), c.name);
            }
        }
    }
    #[test]
    fn observers_cannot_change_state_through_inspector_names() {
        for name in [
            "profiles",
            "git",
            "pr",
            "neuralnets",
            "plan",
            "advisor",
            "agents",
            "extensions",
            "queue",
            "context",
            "doctor",
        ] {
            assert!(inspect_only(name, &json!({})), "{name}");
        }
        for (name, action) in [
            ("handoff", "preview-import"),
            ("handoff", "import"),
            ("queue", "saved-edit"),
            ("profiles", "instructions"),
            ("profiles", "use"),
            ("git", "commit"),
            ("pr", "create"),
            ("neuralnets", "save"),
            ("plan", "execute"),
            ("advisor", "request"),
            ("agents", "create"),
            ("extensions", "enable"),
            ("queue", "clear"),
            ("context", "remove"),
            ("doctor", "probe"),
        ] {
            assert!(
                !inspect_only(name, &json!({"action":action,"args":"value"})),
                "{name}/{action}"
            );
        }
        assert!(!inspect_only("unknown", &json!({})));
    }
    #[test]
    fn large_results_are_explicitly_bounded_at_unicode_boundaries() {
        let result = bounded_result(json!({"body":"界".repeat(200000),"format":"markdown"}));
        assert_eq!(result["truncated"], true);
        assert!(result.to_string().len() < 400 * 1024);
        assert!(result["body"].as_str().unwrap().contains("Display clipped"));
    }
    #[test]
    fn login_secrets_are_password_fields_and_not_catalogue_values() {
        let fields = form("login")["fields"].as_array().unwrap().clone();
        assert!(
            fields
                .iter()
                .any(|f| f["key"] == "secret" && f["kind"] == "password")
        );
        assert!(catalogue(None).iter().all(|c| {
            c["form"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f.get("value").is_none())
        }));
    }
}
