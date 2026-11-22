//! Saved browser workflows: the agent tool calls that changed the page are
//! recorded, saved under a name and replayed later without the model.
//!
//! Snapshot refs are only valid for one page revision, so a recorded target
//! that is a ref becomes `role=<role>;name=<name>` from the snapshot or find
//! result that produced it. Arguments are stored before secret injection, so
//! `{{secret:NAME}}` placeholders stay placeholders on disk.

use super::agent_tools::{config_dir, named_path, write_private};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

const TOOL: &str = "browser_workflow";
const MAX_STEPS: usize = 100;
const MAX_REFERENCES: usize = 4096;

static REPLAYING: AtomicBool = AtomicBool::new(false);

/// Marks a replay in progress so replayed steps are not recorded again.
pub(crate) fn set_replaying(on: bool) {
    REPLAYING.store(on, Ordering::SeqCst);
}

#[derive(Default)]
struct Recorder {
    steps: Vec<Value>,
    references: HashMap<String, String>,
}

static RECORDER: LazyLock<Mutex<Recorder>> = LazyLock::new(Mutex::default);

fn recorder() -> std::sync::MutexGuard<'static, Recorder> {
    RECORDER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Agent tools whose successful calls are replayed by a saved workflow.
pub(crate) fn records(tool: &str, arguments: &Value) -> bool {
    match tool {
        "browser_navigate"
        | "browser_history"
        | "browser_click"
        | "browser_type"
        | "browser_fill_form"
        | "browser_select_option"
        | "browser_check"
        | "browser_press_key"
        | "browser_scroll"
        | "browser_wait_for"
        | "browser_drag"
        | "browser_hover"
        | "browser_handle_dialog"
        | "browser_upload_file"
        | "browser_dismiss_consent"
        | "browser_verify" => true,
        "browser_logins" => arguments["action"] == "load",
        _ => false,
    }
}

/// Remember the role and name behind each ref in a snapshot or find result.
pub(crate) fn learn_references(raw: &str) {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return;
    };
    let mut found = Vec::new();
    collect(&value, &mut found);
    let mut recorder = recorder();
    if recorder.references.len() + found.len() > MAX_REFERENCES {
        recorder.references.clear();
    }
    recorder.references.extend(found);
}

fn collect(value: &Value, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(object) => {
            let field = |key: &str| object.get(key).and_then(Value::as_str);
            if let (Some(reference), Some(role), Some(name)) =
                (field("reference"), field("role"), field("name"))
                && !role.is_empty()
                && !name.trim().is_empty()
                && !name.contains(';')
            {
                out.push((reference.to_owned(), format!("role={role};name={name}")));
            }
            object.values().for_each(|value| collect(value, out));
        }
        Value::Array(items) => items.iter().for_each(|value| collect(value, out)),
        _ => {}
    }
}

/// Record a successful call. `arguments` are the model's, before secrets.
pub(crate) fn record(tool: &str, arguments: &Value) {
    if REPLAYING.load(Ordering::SeqCst) {
        return;
    }
    let mut recorder = recorder();
    let mut arguments = arguments.clone();
    let references = &recorder.references;
    let stable = |target: &mut Value| {
        if let Some(locator) = target.as_str().and_then(|key| references.get(key)) {
            *target = json!(locator);
        }
    };
    for key in ["target", "source", "destination"] {
        if let Some(target) = arguments.get_mut(key) {
            stable(target);
        }
    }
    if let Some(fields) = arguments.get_mut("fields").and_then(Value::as_array_mut) {
        for field in fields {
            if let Some(target) = field.get_mut("target") {
                stable(target);
            }
        }
    }
    recorder
        .steps
        .push(json!({"tool": tool, "arguments": arguments}));
    if recorder.steps.len() > MAX_STEPS {
        recorder.steps.remove(0);
    }
}

fn is_reference(target: &str) -> bool {
    let snapshot_ref = target.split_once(':').is_some_and(|(revision, _)| {
        revision.len() > 1
            && revision.starts_with('r')
            && revision[1..].chars().all(|c| c.is_ascii_digit())
    });
    let short_ref = target.len() > 1
        && target.starts_with('e')
        && target[1..].chars().all(|c| c.is_ascii_digit());
    snapshot_ref || short_ref
}

fn unstable_steps(steps: &[Value]) -> Vec<usize> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, step)| {
            let arguments = &step["arguments"];
            let mut targets: Vec<&Value> = ["target", "source", "destination"]
                .iter()
                .filter_map(|key| arguments.get(*key))
                .collect();
            if let Some(fields) = arguments["fields"].as_array() {
                targets.extend(fields.iter().filter_map(|field| field.get("target")));
            }
            targets
                .iter()
                .any(|target| target.as_str().is_some_and(is_reference))
        })
        .map(|(index, _)| index + 1)
        .collect()
}

pub(crate) fn start() -> String {
    let mut recorder = recorder();
    let dropped = recorder.steps.len();
    recorder.steps.clear();
    format!(
        "Recording a new workflow ({dropped} earlier steps dropped). Do the task with browser_* tools, then call {{\"action\":\"save\",\"name\":\"<name>\"}}."
    )
}

pub(crate) fn save(name: &str) -> Result<String, String> {
    let path = named_path("workflows", TOOL, name)?;
    let steps = recorder().steps.clone();
    if steps.is_empty() {
        return Err(format!(
            "{TOOL}: nothing recorded yet; do the task with browser_* tools first, then save."
        ));
    }
    let workflow = json!({"version": 1, "steps": steps});
    write_private(&path, workflow.to_string().as_bytes())
        .map_err(|e| format!("{TOOL}: cannot write {}: {e}", path.display()))?;
    let unstable = unstable_steps(&steps);
    let warning = if unstable.is_empty() {
        String::new()
    } else {
        format!(
            " Steps {unstable:?} use snapshot refs with no role and name, so they will fail on replay; redo them with \"role=<role>;name=<name>\" targets and save again."
        )
    };
    Ok(format!(
        "Saved workflow \"{name}\" with {} steps: {}. Replay it with {{\"action\":\"run\",\"name\":\"{name}\"}}.{warning}",
        steps.len(),
        outline(&steps)
    ))
}

fn outline(steps: &[Value]) -> String {
    let mut names: Vec<String> = Vec::new();
    for step in steps {
        let tool = step["tool"]
            .as_str()
            .unwrap_or_default()
            .trim_start_matches("browser_");
        match names.last_mut() {
            Some(last) if last.split(" x").next() == Some(tool) => {
                let count = last
                    .split(" x")
                    .nth(1)
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(1);
                *last = format!("{tool} x{}", count + 1);
            }
            _ => names.push(tool.to_owned()),
        }
    }
    names.join(", ")
}

/// The steps of the saved workflow `name`.
pub(crate) fn load(name: &str) -> Result<Vec<Value>, String> {
    let path = named_path("workflows", TOOL, name)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("{TOOL}: no saved workflow named \"{name}\"."))?;
    let workflow: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{TOOL}: {} is not valid: {e}", path.display()))?;
    let steps = workflow["steps"].as_array().cloned().unwrap_or_default();
    if steps.len() > MAX_STEPS {
        return Err(format!(
            "{TOOL}: {} has {} steps; at most {MAX_STEPS} replay.",
            path.display(),
            steps.len()
        ));
    }
    if let Some(step) = steps.iter().find(|step| {
        !step["tool"]
            .as_str()
            .is_some_and(|tool| records(tool, &step["arguments"]))
    }) {
        return Err(format!(
            "{TOOL}: {} has a step this tool does not replay: {}.",
            path.display(),
            step["tool"]
        ));
    }
    Ok(steps)
}

pub(crate) fn list() -> Result<String, String> {
    let mut names: Vec<String> = std::fs::read_dir(config_dir("workflows"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry
                .file_name()
                .into_string()
                .ok()?
                .strip_suffix(".json")?
                .to_owned();
            let steps = load(&name).ok()?;
            Some(format!(
                "- {name} ({} steps: {})",
                steps.len(),
                outline(&steps)
            ))
        })
        .collect();
    names.sort();
    Ok(if names.is_empty() {
        "No saved workflows. Do a task with browser_* tools, then call {\"action\":\"save\",\"name\":\"<name>\"}.".into()
    } else {
        format!("Saved workflows:\n{}", names.join("\n"))
    })
}

pub(crate) fn delete(name: &str) -> Result<String, String> {
    std::fs::remove_file(named_path("workflows", TOOL, name)?)
        .map(|()| format!("Deleted workflow \"{name}\"."))
        .map_err(|_| format!("{TOOL}: no saved workflow named \"{name}\"."))
}

/// The result of replaying `steps`: `last` is the final step's result text,
/// `failure` the index and error of the step that stopped the run.
pub(crate) fn run_report(
    name: &str,
    steps: &[Value],
    last: &str,
    failure: Option<(usize, String)>,
) -> Result<String, String> {
    match failure {
        None => Ok(format!(
            "Replayed workflow \"{name}\": {} of {} steps, no model steps needed.\nLast step ({}): {}\nNext: browser_verify or browser_snapshot to confirm the result before reporting it.",
            steps.len(),
            steps.len(),
            steps
                .last()
                .map_or("", |step| step["tool"].as_str().unwrap_or_default()),
            clip(last, 1500)
        )),
        Some((index, error)) => Err(format!(
            "Workflow \"{name}\" stopped at step {} of {} ({} {}): {}\n{}",
            index + 1,
            steps.len(),
            steps[index]["tool"].as_str().unwrap_or_default(),
            steps[index]["arguments"],
            clip(&error, 1500),
            if index == 0 {
                "No steps completed.".to_owned()
            } else {
                format!("Steps 1-{index} completed.")
            }
        )),
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_steps_replace_snapshot_refs_with_role_and_name() {
        let snapshot = json!({"regions":[{"targets":[
            {"reference":"r10:c1:b3","role":"textbox","name":"Full name"},
            {"reference":"r10:c1:b9","role":"button","name":"Create account"},
            {"reference":"r10:c1:b10","role":"generic","name":""}
        ]}]});
        learn_references(&snapshot.to_string());
        let start = recorder().steps.len();
        record(
            "browser_fill_form",
            &json!({"fields":[{"target":"r10:c1:b3","value":"{{secret:AGENT_NAME}}"}]}),
        );
        record("browser_click", &json!({"target":"r10:c1:b9"}));
        record("browser_click", &json!({"target":"r10:c1:b10"}));
        let steps = recorder().steps[start..].to_vec();
        assert_eq!(
            steps[0]["arguments"]["fields"][0],
            json!({"target":"role=textbox;name=Full name","value":"{{secret:AGENT_NAME}}"})
        );
        assert_eq!(
            steps[1]["arguments"]["target"],
            "role=button;name=Create account"
        );
        assert_eq!(unstable_steps(&steps), vec![3]);
        assert_eq!(outline(&steps), "fill_form, click x2");
    }

    #[test]
    fn workflow_records_only_replayable_calls() {
        assert!(records("browser_click", &json!({})));
        assert!(records("browser_logins", &json!({"action":"load"})));
        assert!(!records("browser_logins", &json!({"action":"save"})));
        for tool in [
            "browser_snapshot",
            "browser_click_at",
            "browser_tabs",
            "browser_workflow",
        ] {
            assert!(!records(tool, &json!({})), "{tool}");
        }
        assert!(is_reference("e12") && is_reference("r10:c1:b3"));
        assert!(!is_reference("role=button;name=Save") && !is_reference("email"));
    }

    #[test]
    fn workflow_run_report_names_the_failed_step() {
        let steps = vec![
            json!({"tool":"browser_navigate","arguments":{"url":"http://a"}}),
            json!({"tool":"browser_click","arguments":{"target":"text=Pay"}}),
        ];
        let error = run_report("pay", &steps, "", Some((1, "not found".into()))).unwrap_err();
        assert!(
            error.contains("stopped at step 2 of 2 (browser_click"),
            "{error}"
        );
        assert!(error.contains("Steps 1-1 completed."), "{error}");
        let done = run_report("pay", &steps, "Clicked", None).unwrap();
        assert!(
            done.starts_with("Replayed workflow \"pay\": 2 of 2 steps"),
            "{done}"
        );
    }
}
