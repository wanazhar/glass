//! Opt-in milestone review reuses persistent teammates; no second agent runtime.
use crate::{config, session::Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    every_turns: Option<usize>,
    last_requested_turn: usize,
    dispositions: BTreeMap<String, String>,
}
fn load(s: &Session) -> Result<Settings, String> {
    let path = s.path.with_extension("advisor.json");
    if !path.try_exists().map_err(|e| e.to_string())? {
        return Ok(Settings::default());
    }
    let v: Settings = serde_json::from_slice(&crate::requests::bounded(&path, 65536)?)
        .map_err(|e| format!("Advisor settings: {e}"))?;
    if v.every_turns.is_some_and(|n| n == 0 || n > 1000)
        || v.dispositions.len() > 128
        || v.dispositions.values().any(|s| s.len() > 1024)
    {
        return Err("Advisor settings exceed bounds".into());
    }
    Ok(v)
}
fn save(s: &Session, v: &Settings) -> Result<(), String> {
    crate::auth::secure_write(
        &s.path.with_extension("advisor.json"),
        &serde_json::to_vec(v).map_err(|e| e.to_string())?,
    )
}
fn turn(s: &Session) -> usize {
    s.history_records()
        .iter()
        .filter(|r| matches!(r, crate::session::Record::User(_)))
        .count()
}
fn launch(s: &Session) -> Result<Value, String> {
    let request = json!({"action":"checkpoint","trigger":"changes","from":"human"});
    if let Some(hub) = crate::team::get(&s.id) {
        hub.request(&request)
    } else {
        crate::team::client(&s.id, &request)
    }
}
pub fn milestone(s: &Session) -> Result<(), String> {
    let mut settings = load(s)?;
    let Some(every) = settings.every_turns else {
        return Ok(());
    };
    let at = turn(s);
    if at.saturating_sub(settings.last_requested_turn) < every {
        return Ok(());
    }
    // The hub deduplicates unchanged patches and only starts a configured read-only
    // reviewer. Persist cadence after acceptance; failed launch stays retryable.
    launch(s)?;
    settings.last_requested_turn = at;
    save(s, &settings)?;
    Ok(())
}
fn snapshot(s: &Session) -> Result<Value, String> {
    let settings = load(s)?;
    let agents = if let Some(hub) = crate::team::get(&s.id) {
        serde_json::to_value(hub.snapshot()).map_err(|e| e.to_string())?
    } else {
        crate::team::client(&s.id, &json!({"action":"list"})).unwrap_or_else(|_| json!([]))
    };
    let remote=crate::team::get(&s.id).is_none();
    let reviewers=agents.as_array().into_iter().flatten().filter(|a|a["name"].as_str().is_some_and(|name|name.starts_with("review-"))).take(32).map(|a| {
        let detail=a;
        let result=detail["result"].as_str().unwrap_or("");
        let mut end=result.len().min(8192);while !result.is_char_boundary(end){end-=1;}
        json!({"id":detail["id"],"model":detail["model"],"status":detail["status"],"note":&result[..end],"note_truncated":remote||result.len()>end,"note_scope":if remote {"Roster preview; inspect the exact agent or transcript for full notes"} else {"Local result preview"},
            "transcript_session":detail["session"],"usage":{"input":detail["input"],"output":detail["output"],"reported":detail["usage_known"],"estimated_tokens":detail["estimated_tokens"],"cost_usd":detail["cost_usd"],"cost_complete":detail["cost_complete"]},
            "disposition":detail["id"].as_str().and_then(|id|settings.dispositions.get(id))})
    }).collect::<Vec<_>>();
    Ok(
        json!({"cadence":settings.every_turns.map(|n|format!("Every {n} completed user turns with changes")).unwrap_or("Manual only; no automatic requests".into()),
        "last_requested_turn":settings.last_requested_turn,"reviews":reviewers,
        "role":"Configure reviewer model and cumulative token allowance with gc workspace role set reviewer MODEL TOKENS.",
        "cost":"Each new patch review consumes the separate reviewer allowance. UI navigation does not request the model."}),
    )
}
pub fn view(keys: &crate::input::KeyReader, s: &Session) {
    loop {
        let actions = [
            "Review notes and separate usage",
            "Request review of current changes",
            "Set milestone cadence…",
            "Manual reviews only",
            "Record disposition…",
            "Open teammate transcripts",
        ]
        .map(String::from);
        let Some(i) = crate::modal::choose(
            keys,
            "Advisor",
            "Read-only persistent reviewers · explicit opt-in cadence",
            &actions,
        ) else {
            return;
        };
        let result: Result<(), String> = (|| {
            match i {
                0 => crate::structured::view(keys, "Advisor notes", &snapshot(s)?.to_string()),
                1 => crate::structured::view(keys, "Review requested", &launch(s)?.to_string()),
                2 => {
                    if let Some(value) = crate::modal::validated_field(
                        keys,
                        "Milestone cadence",
                        "Review every N completed user turns (1–1000)",
                        "1",
                        false,
                        |s| {
                            if s.parse::<usize>().is_ok_and(|n| (1..=1000).contains(&n)) {
                                Ok(())
                            } else {
                                Err("Enter 1–1000".into())
                            }
                        },
                    ) {
                        let mut settings = load(s)?;
                        settings.every_turns = Some(value.parse().map_err(|_| "Invalid cadence")?);
                        settings.last_requested_turn = turn(s);
                        save(s, &settings)?;
                    }
                }
                3 => {
                    let mut settings = load(s)?;
                    settings.every_turns = None;
                    save(s, &settings)?;
                }
                4 => {
                    let reviewers = crate::team::get(&s.id)
                        .map(|h| h.snapshot())
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|a| a.name.starts_with("review-"))
                        .collect::<Vec<_>>();
                    let labels = reviewers
                        .iter()
                        .map(|a| format!("{} · {} · {}", a.name, a.model, a.status))
                        .collect::<Vec<_>>();
                    if let Some(i) = crate::modal::choose(
                        keys,
                        "Review disposition",
                        "Choose the exact reviewer",
                        &labels,
                    ) && let Some(note) = crate::modal::validated_field(
                        keys,
                        "Record disposition",
                        "Accepted / rejected / follow-up and evidence",
                        "",
                        false,
                        |s| {
                            if !s.trim().is_empty() && s.len() <= 1024 {
                                Ok(())
                            } else {
                                Err("Enter a note up to 1024 bytes".into())
                            }
                        },
                    ) {
                        let mut settings = load(s)?;
                        settings.dispositions.insert(reviewers[i].id.clone(), note);
                        save(s, &settings)?;
                    }
                }
                5 => crate::team::view(keys, &s.id),
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = result {
            crate::modal::info(keys, "Advisor unavailable", &e);
        }
    }
}
pub fn command(args: &[String]) -> Result<(), String> {
    let usage = "gc advisor SESSION [--json] | gc advisor SESSION request | gc advisor SESSION cadence N|off | gc advisor SESSION disposition AGENT NOTE";
    let id = args.first().ok_or(usage)?;
    let session = Session::open_readonly(&config::data_dir(), id).map_err(|e| e.to_string())?;
    let value = match args.get(1).map(String::as_str) {
        None | Some("--json") if args.len() <= 2 => snapshot(&session)?,
        Some("request") if args.len() == 2 => launch(&session)?,
        Some("cadence") if args.len() == 3 => {
            let mut s = load(&session)?;
            s.every_turns = if args[2] == "off" {
                None
            } else {
                Some(
                    args[2]
                        .parse::<usize>()
                        .ok()
                        .filter(|n| (1..=1000).contains(n))
                        .ok_or("Cadence must be 1–1000 or off")?,
                )
            };
            s.last_requested_turn = turn(&session);
            save(&session, &s)?;
            snapshot(&session)?
        }
        Some("disposition") if args.len() == 4 => {
            let mut s = load(&session)?;
            if args[2].is_empty()
                || args[2].len() > 128
                || args[3].trim().is_empty()
                || args[3].len() > 1024
                || (!s.dispositions.contains_key(&args[2]) && s.dispositions.len() >= 128)
            {
                return Err("Disposition limit reached".into());
            }
            s.dispositions.insert(args[2].clone(), args[3].clone());
            save(&session, &s)?;
            snapshot(&session)?
        }
        _ => return Err(usage.into()),
    };
    if args.iter().any(|s| s == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("Advisor", &value);
    }
    Ok(())
}
