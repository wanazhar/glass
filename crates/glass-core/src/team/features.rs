//! Parent-owned capabilities shared with persistent children; human input stays with the parent.
use super::*;
#[derive(Clone, Default)]
pub(super) struct Features {
    pub config: Option<crate::config::Config>,
    pub hosted: Vec<(String, String, String)>,
    pub exclusive: bool,
}
pub(crate) struct HostedCall {
    pub call: glass_mod_api::ToolCall,
    pub child: String,
    pub sequence: u64,
    pub reply: std::sync::mpsc::SyncSender<crate::agent::HostToolOutcome>,
}
impl Hub {
    pub(crate) fn parent_config(&self) -> Option<crate::config::Config> {
        self.features.lock().unwrap().config.clone()
    }
    pub(crate) fn inherit_hosted<'a>(
        self: &Arc<Self>,
        agent: crate::agent::Agent<'a>,
        child: String,
        read_only: bool,
    ) -> crate::agent::Agent<'a> {
        let features = self.features.lock().unwrap().clone();
        if read_only || features.hosted.is_empty() {
            return agent;
        }
        let hub = Arc::clone(self);
        let execute: crate::agent::HostExecutor<'a> =
            Box::new(move |call, _, sequence| hub.execute_hosted(&child, call, sequence));
        if features.exclusive {
            agent.with_host_tools(features.hosted, execute)
        } else {
            agent.with_mixed_host_tools(features.hosted, execute)
        }
    }
    pub(crate) fn configure(&self, cfg: &crate::config::Config) {
        self.features.lock().unwrap().config = Some(cfg.clone());
    }
    pub(crate) fn configure_hosted(&self, tools: Vec<(String, String, String)>, exclusive: bool) {
        let mut features = self.features.lock().unwrap();
        features.hosted = tools;
        features.exclusive = exclusive;
    }
    pub(crate) fn pop_hosted(&self) -> Option<HostedCall> {
        let mut queue = self.hosted_calls.lock().unwrap();
        while let Some(call) = queue.pop_front() {
            let cancelled = self.stop.load(Ordering::Acquire)
                || self
                    .state
                    .lock()
                    .unwrap()
                    .agents
                    .values()
                    .any(|w| w.info.session == call.child && w.interrupt);
            if !cancelled {
                return Some(call);
            }
            let _ = call
                .reply
                .send(crate::agent::HostToolOutcome::plain(tools::Outcome::err(
                    "Delegated host tool cancelled",
                )));
        }
        None
    }
    pub(super) fn execute_hosted(
        &self,
        child: &str,
        call: &glass_mod_api::ToolCall,
        sequence: u64,
    ) -> crate::agent::HostToolOutcome {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.hosted_calls.lock().unwrap().push_back(HostedCall {
            call: call.clone(),
            child: child.into(),
            sequence,
            reply: tx,
        });
        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(outcome) => return outcome,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return crate::agent::HostToolOutcome::plain(tools::Outcome::err(
                        "Parent host connection closed",
                    ));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.stop.load(Ordering::Acquire)
                        || self
                            .state
                            .lock()
                            .unwrap()
                            .agents
                            .values()
                            .any(|w| w.info.session == child && w.interrupt)
                    {
                        return crate::agent::HostToolOutcome::plain(tools::Outcome::err(
                            "Delegated host tool cancelled",
                        ));
                    }
                }
            }
        }
    }
    pub(crate) fn extension_runner(self: &Arc<Self>, parent: &str) -> crate::ext::host::SubagentFn {
        let hub = Arc::clone(self);
        let parent = parent.to_string();
        Box::new(move |tasks| {
            crate::subagent::run_batch(tasks, crate::subagent::MAX_CONCURRENT, |task| {
                let name = format!("child-{}", crate::subagent::new_task_id());
                let mut request = json!({"action":"create","from":parent,"name":name,"text":task.task,"write":!task.read_only,"worktree":task.worktree,"notify_parent":false});
                if let Some(model) = &task.model {
                    request["model"] = json!(model);
                }
                let info = match hub.request(&request) {
                    Ok(value) => value,
                    Err(error) => return format!("subagent failed: {error}"),
                };
                let id = info["id"].as_str().unwrap_or("").to_string();
                let mut state = hub.state.lock().unwrap();
                loop {
                    let Some(worker) = state.agents.get(&id) else {
                        return "subagent unavailable".into();
                    };
                    if worker.info.status != "working" {
                        return worker.info.result.clone();
                    }
                    if hub.stop.load(Ordering::Acquire) {
                        return "subagent cancelled".into();
                    }
                    state = hub.changed.wait(state).unwrap();
                }
            })
        })
    }
}
pub fn start_with_config(
    owner: &str,
    workspace: &Path,
    factory: ProviderFactory,
    models: ProviderForModel,
    model: &str,
    cfg: &crate::config::Config,
) -> Result<Guard, String> {
    let guard = super::start(owner, workspace, factory, models, model)?;
    guard.hub.configure(cfg);
    Ok(guard)
}
