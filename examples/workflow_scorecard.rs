use base64::{Engine, engine::general_purpose::STANDARD};
use glass::browser::chrome::detect_chrome;
use glass::browser::session::{
    BrowserResult, BrowserSession, InteractionMode, SessionOptions, WorkflowDefinition,
    WorkflowRunStatus, WorkflowStepState,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::TcpListener;

const DEFAULT_ITERATIONS: usize = 10;

#[derive(Deserialize)]
struct Corpus {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    corpus: String,
    fixture: String,
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    id: String,
    #[serde(rename = "expectedStatus")]
    expected_status: WorkflowRunStatus,
    #[serde(rename = "expectedStepStates")]
    expected_step_states: Vec<WorkflowStepState>,
    workflow: WorkflowDefinition,
}

#[tokio::main]
async fn main() -> BrowserResult<()> {
    let corpus: Corpus =
        serde_json::from_str(include_str!("../benchmarks/scenarios/workflow-v1.json"))?;
    if corpus.schema_version != 1 {
        return Err(format!(
            "unsupported workflow corpus schema {}",
            corpus.schema_version
        )
        .into());
    }
    let chrome_path = std::env::var_os("CHROME_PATH")
        .map(PathBuf::from)
        .or_else(detect_chrome)
        .ok_or("Chrome/Chromium is required for the workflow scorecard")?;
    if !chrome_path.is_file() {
        return Err(format!(
            "CHROME_PATH does not name a file: {}",
            chrome_path.display()
        )
        .into());
    }
    let iterations = positive_env("GLASS_WORKFLOW_SCORECARD_ITERATIONS", DEFAULT_ITERATIONS)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let fixture = include_str!("../tests/fixtures/scorecard.html");
    let url = format!("data:text/html;base64,{}", STANDARD.encode(fixture));
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "workflow-scorecard".into(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
        audit: false,
        policy: None,
    })
    .await?;
    session.navigate(&url).await?;

    let started = Instant::now();
    let mut outcomes = Vec::with_capacity(corpus.scenarios.len() * iterations);
    for iteration in 0..iterations {
        for scenario in &corpus.scenarios {
            reset(&session).await?;
            let run_started = Instant::now();
            let result = session
                .run_workflow(&scenario.workflow, &BTreeMap::new())
                .await?;
            let states: Vec<_> = result.steps.iter().map(|step| step.state).collect();
            let status_match = result.status == scenario.expected_status;
            let states_match = states == scenario.expected_step_states;
            outcomes.push(json!({
                "id": scenario.id,
                "iteration": iteration + 1,
                "expected_status": scenario.expected_status,
                "actual_status": result.status,
                "expected_step_states": scenario.expected_step_states,
                "actual_step_states": states,
                "status": if status_match && states_match { "success" } else { "failure" },
                "run_id": result.run_id,
                "trace_events": result.trace.events.len(),
                "latency_ms": run_started.elapsed().as_secs_f64() * 1_000.0,
            }));
        }
    }
    session.close().await?;
    let failures = outcomes
        .iter()
        .filter(|outcome| outcome["status"] == "failure")
        .count();
    let report = json!({
        "schema_version": 1,
        "tool": {"name": "glass", "version": env!("CARGO_PKG_VERSION")},
        "run": {"corpus": corpus.corpus, "corpus_fixture": corpus.fixture, "iterations": iterations},
        "summary": {
            "scenarios": outcomes.len(),
            "failures": failures,
            "workflow_success_rate": (outcomes.len() - failures) as f64 / outcomes.len() as f64,
            "hard_gate_passed": failures == 0,
            "elapsed_ms": started.elapsed().as_secs_f64() * 1_000.0,
        },
        "scenarios": outcomes,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    if failures > 0 {
        return Err("workflow scorecard hard gate failed".into());
    }
    Ok(())
}

async fn reset(session: &BrowserSession) -> BrowserResult<()> {
    session
        .evaluate("window.resetFixture(); document.querySelector('#name').value=''")
        .await?;
    Ok(())
}

fn positive_env(name: &str, default: usize) -> BrowserResult<usize> {
    let value = std::env::var(name).unwrap_or_else(|_| default.to_string());
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{name} must be a positive integer").into());
    }
    Ok(parsed)
}
