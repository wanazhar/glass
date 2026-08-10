//! Resident browser and workflow ownership for the Glass development workspace.

use glass_browser::browser::policy::BrowserPolicy;
use glass_browser::browser::session::{
    BrowserSession, SessionOptions, WorkflowCheckpoint, WorkflowDefinition, WorkflowRunResult,
};
use glass_browser::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender};
use std::time::Duration;

const COMMAND_QUEUE: usize = 32;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrowserStartConfig {
    pub port: u16,
    pub attach: bool,
    pub incognito: bool,
    pub headed: bool,
    pub profile: String,
    pub chrome_path: Option<PathBuf>,
}

impl Default for BrowserStartConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            attach: false,
            incognito: true,
            headed: false,
            profile: default_profile(),
            chrome_path: None,
        }
    }
}

fn default_port() -> u16 {
    9222
}

fn default_profile() -> String {
    "default".into()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeState {
    pub connected: bool,
    pub browser_revision: Option<u64>,
    pub workflow_state: String,
    pub active_workflow: Option<String>,
}

enum BrowserCommand {
    Start(BrowserStartConfig),
    Stop,
    State,
    Observe,
    Targets,
    SelectTarget(String),
    Navigate {
        url: String,
        expected_revision: u64,
        timeout: Duration,
    },
    Click {
        target: String,
        expected_revision: u64,
    },
    Type {
        text: String,
        target: Option<String>,
        expected_revision: u64,
    },
    Scroll {
        dx: f64,
        dy: f64,
        expected_revision: u64,
    },
    Screenshot,
    RunWorkflow {
        definition: Value,
        inputs: BTreeMap<String, Value>,
    },
    PauseWorkflow,
    ResumeWorkflow {
        definition: Value,
        inputs: BTreeMap<String, Value>,
        checkpoint: Value,
    },
}

type Reply = SyncSender<DevelopmentResult<Value>>;

/// Cloneable command handle for the one authoritative browser worker.
#[derive(Clone)]
pub struct BrowserService {
    commands: SyncSender<(BrowserCommand, Reply)>,
}

impl BrowserService {
    pub fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = root.as_ref().to_path_buf();
        let (commands, receiver) = mpsc::sync_channel::<(BrowserCommand, Reply)>(COMMAND_QUEUE);
        std::thread::Builder::new()
            .name("glass-browser-workspace".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    return;
                };
                let mut worker = BrowserWorker::new(root);
                while let Ok((command, reply)) = receiver.recv() {
                    let result = runtime.block_on(worker.execute(command));
                    let _ = reply.send(result);
                }
            })
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        Ok(Self { commands })
    }

    fn call(&self, command: BrowserCommand) -> DevelopmentResult<Value> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.commands
            .send((command, reply))
            .map_err(|_| DevelopmentError::Process("resident browser worker stopped".into()))?;
        receiver.recv_timeout(COMMAND_TIMEOUT).map_err(|error| {
            DevelopmentError::Process(format!("resident browser command timed out: {error}"))
        })?
    }

    pub fn start(&self, config: BrowserStartConfig) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Start(config))
    }

    pub fn stop(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Stop)
    }

    pub fn state(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::State)
    }

    pub fn observe(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Observe)
    }

    pub fn targets(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Targets)
    }

    pub fn select_target(&self, target: String) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::SelectTarget(target))
    }

    pub fn navigate(
        &self,
        url: String,
        expected_revision: u64,
        timeout: Duration,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Navigate {
            url,
            expected_revision,
            timeout,
        })
    }

    pub fn click(&self, target: String, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Click {
            target,
            expected_revision,
        })
    }

    pub fn type_text(
        &self,
        text: String,
        target: Option<String>,
        expected_revision: u64,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Type {
            text,
            target,
            expected_revision,
        })
    }

    pub fn scroll(&self, dx: f64, dy: f64, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Scroll {
            dx,
            dy,
            expected_revision,
        })
    }

    pub fn screenshot(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Screenshot)
    }

    pub fn run_workflow(
        &self,
        definition: Value,
        inputs: BTreeMap<String, Value>,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::RunWorkflow { definition, inputs })
    }

    pub fn pause_workflow(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::PauseWorkflow)
    }

    pub fn resume_workflow(
        &self,
        definition: Value,
        inputs: BTreeMap<String, Value>,
        checkpoint: Value,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::ResumeWorkflow {
            definition,
            inputs,
            checkpoint,
        })
    }
}

struct BrowserWorker {
    root: PathBuf,
    session: Option<BrowserSession>,
    revision: Option<u64>,
    workflow_state: String,
    active_workflow: Option<String>,
    last_workflow: Option<(WorkflowDefinition, WorkflowRunResult)>,
}

impl BrowserWorker {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            session: None,
            revision: None,
            workflow_state: "idle".into(),
            active_workflow: None,
            last_workflow: None,
        }
    }

    fn state(&self) -> BrowserRuntimeState {
        BrowserRuntimeState {
            connected: self.session.is_some(),
            browser_revision: self.revision,
            workflow_state: self.workflow_state.clone(),
            active_workflow: self.active_workflow.clone(),
        }
    }

    fn session(&self) -> DevelopmentResult<&BrowserSession> {
        self.session.as_ref().ok_or_else(|| {
            DevelopmentError::Conflict(
                "browser is not connected; call glass.browser.start first".into(),
            )
        })
    }

    async fn execute(&mut self, command: BrowserCommand) -> DevelopmentResult<Value> {
        match command {
            BrowserCommand::Start(config) => {
                if self.session.is_some() {
                    return Err(DevelopmentError::Conflict(
                        "browser workspace already has a connected session".into(),
                    ));
                }
                let policy = BrowserPolicy::development(&self.root)
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                let mut builder = SessionOptions::builder()
                    .port(config.port)
                    .attach(config.attach)
                    .incognito(config.incognito)
                    .headed(config.headed)
                    .profile(config.profile)
                    .policy(policy);
                if let Some(path) = config.chrome_path {
                    builder = builder.chrome_path(path);
                }
                let options = builder
                    .build()
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                let session = BrowserSession::start(&options)
                    .await
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                self.revision = Some(1);
                self.session = Some(session);
                serde_json::to_value(self.state()).map_err(Into::into)
            }
            BrowserCommand::Stop => {
                self.session = None;
                self.revision = None;
                self.workflow_state = "idle".into();
                self.active_workflow = None;
                self.last_workflow = None;
                serde_json::to_value(self.state()).map_err(Into::into)
            }
            BrowserCommand::State => serde_json::to_value(self.state()).map_err(Into::into),
            BrowserCommand::Observe => {
                let context = self
                    .session()?
                    .observe_fresh()
                    .await
                    .map_err(browser_error)?;
                self.revision = Some(context.consistency.end_revision);
                serde_json::to_value(context).map_err(Into::into)
            }
            BrowserCommand::Targets => {
                let targets = self
                    .session()?
                    .list_targets()
                    .await
                    .map_err(browser_error)?;
                serde_json::to_value(targets).map_err(Into::into)
            }
            BrowserCommand::SelectTarget(target) => {
                let selected = self
                    .session()?
                    .select_target(&target)
                    .await
                    .map_err(browser_error)?;
                let observation = self
                    .session()?
                    .observe_fresh()
                    .await
                    .map_err(browser_error)?;
                self.revision = Some(observation.consistency.end_revision);
                Ok(serde_json::json!({"target":selected,"observation":observation}))
            }
            BrowserCommand::Navigate {
                url,
                expected_revision,
                timeout,
            } => {
                let outcome = self
                    .session()?
                    .navigate_with_revision(&url, timeout, expected_revision)
                    .await
                    .map_err(browser_error)?;
                let value = serde_json::to_value(outcome)?;
                self.revision = value.get("currentRevision").and_then(Value::as_u64);
                Ok(value)
            }
            BrowserCommand::Click {
                target,
                expected_revision,
            } => {
                let outcome = self
                    .session()?
                    .click_with_revision(&target, expected_revision)
                    .await
                    .map_err(browser_error)?;
                self.revision = Some(outcome.current_revision);
                serde_json::to_value(outcome).map_err(Into::into)
            }
            BrowserCommand::Type {
                text,
                target,
                expected_revision,
            } => {
                let outcome = self
                    .session()?
                    .type_text_with_expected_revision(
                        &text,
                        target.as_deref(),
                        Some(expected_revision),
                    )
                    .await
                    .map_err(browser_error)?;
                self.revision = Some(outcome.current_revision);
                serde_json::to_value(outcome).map_err(Into::into)
            }
            BrowserCommand::Scroll {
                dx,
                dy,
                expected_revision,
            } => {
                let outcome = self
                    .session()?
                    .scroll_with_revision(dx, dy, Some(expected_revision))
                    .await
                    .map_err(browser_error)?;
                self.revision = Some(outcome.current_revision);
                serde_json::to_value(outcome).map_err(Into::into)
            }
            BrowserCommand::Screenshot => {
                let png = self
                    .session()?
                    .screenshot_png()
                    .await
                    .map_err(browser_error)?;
                Ok(serde_json::json!({
                    "mimeType":"image/png",
                    "bytes":png.len(),
                    "base64":base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
                }))
            }
            BrowserCommand::RunWorkflow { definition, inputs } => {
                let workflow = WorkflowDefinition::from_value(definition)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                self.workflow_state = "running".into();
                self.active_workflow = Some(workflow.name.clone());
                let result = self.session()?.run_workflow(&workflow, &inputs).await;
                self.workflow_state = if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
                .into();
                let result = result.map_err(browser_error)?;
                self.revision = Some(result.final_revision);
                self.last_workflow = Some((workflow, result.clone()));
                serde_json::to_value(result).map_err(Into::into)
            }
            BrowserCommand::PauseWorkflow => {
                let (workflow, result) = self.last_workflow.as_ref().ok_or_else(|| {
                    DevelopmentError::Conflict(
                        "no workflow result is available to checkpoint".into(),
                    )
                })?;
                let checkpoint = self
                    .session()?
                    .export_workflow_checkpoint(workflow, result)
                    .await
                    .map_err(browser_error)?;
                self.workflow_state = "paused".into();
                serde_json::to_value(checkpoint).map_err(Into::into)
            }
            BrowserCommand::ResumeWorkflow {
                definition,
                inputs,
                checkpoint,
            } => {
                let workflow = WorkflowDefinition::from_value(definition)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                let checkpoint: WorkflowCheckpoint = serde_json::from_value(checkpoint)?;
                self.workflow_state = "running".into();
                self.active_workflow = Some(workflow.name.clone());
                let result = self
                    .session()?
                    .resume_workflow(&workflow, &inputs, &checkpoint)
                    .await;
                self.workflow_state = if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
                .into();
                let result = result.map_err(browser_error)?;
                self.revision = Some(result.final_revision);
                self.last_workflow = Some((workflow, result.clone()));
                serde_json::to_value(result).map_err(Into::into)
            }
        }
    }
}

fn browser_error(error: Box<dyn std::error::Error>) -> DevelopmentError {
    DevelopmentError::Process(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn disconnected_service_reports_authoritative_state() {
        let service = BrowserService::new(std::env::temp_dir()).unwrap();
        let state = service.state().unwrap();
        assert_eq!(state["connected"], false);
        assert_eq!(state["workflowState"], "idle");
        let error = service.observe().unwrap_err();
        assert!(error.to_string().contains("glass.browser.start"));
    }

    #[test]
    fn resident_browser_executes_revision_safe_fixture_flow() {
        if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
            return;
        }
        let chrome_path =
            std::env::var("GLASS_CHROME_PATH").expect("GLASS_E2E=1 requires GLASS_CHROME_PATH");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let server = std::thread::spawn(move || {
            let html = br#"<!doctype html><title>Resident Glass</title><label>Name<input id="name"></label><button id="save" onclick="document.querySelector('p').textContent='Saved '+document.querySelector('#name').value">Save</button><p></p>"#;
            while server_running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0; 4096];
                        let _ = stream.read(&mut request);
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            html.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(html);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        let port_probe = TcpListener::bind("127.0.0.1:0").unwrap();
        let browser_port = port_probe.local_addr().unwrap().port();
        drop(port_probe);

        let service = BrowserService::new(std::env::temp_dir()).unwrap();
        service
            .start(BrowserStartConfig {
                port: browser_port,
                chrome_path: Some(chrome_path.into()),
                ..BrowserStartConfig::default()
            })
            .unwrap();
        let initial = service.observe().unwrap();
        let initial_revision = initial["consistency"]["end_revision"]
            .as_u64()
            .or_else(|| initial["consistency"]["endRevision"].as_u64())
            .unwrap();
        service
            .navigate(
                format!("http://{address}/fixture.html"),
                initial_revision,
                Duration::from_secs(30),
            )
            .unwrap();
        assert!(
            service
                .navigate(
                    format!("http://{address}/stale"),
                    initial_revision,
                    Duration::from_secs(30),
                )
                .unwrap_err()
                .to_string()
                .contains("revision")
        );
        let observed = service.observe().unwrap();
        assert_eq!(observed["page"]["title"], "Resident Glass");
        let revision = observed["consistency"]["end_revision"]
            .as_u64()
            .or_else(|| observed["consistency"]["endRevision"].as_u64())
            .unwrap();
        let typed = service
            .type_text("Ada".into(), Some("Name".into()), revision)
            .unwrap();
        let revision = typed["currentRevision"].as_u64().unwrap();
        service.click("Save".into(), revision).unwrap();
        assert!(
            service.observe().unwrap()["text"]
                .as_str()
                .unwrap()
                .contains("Saved Ada")
        );
        service.stop().unwrap();
        running.store(false, Ordering::Relaxed);
        server.join().unwrap();
    }
}
