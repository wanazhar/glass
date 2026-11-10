//! Bounded completion continuations shared by terminal, server, and runtime.
use super::*;
pub(super) const MAX_DEPTH: u8 = 3;
pub(super) const MAX_WAKES: u32 = 16;
pub(super) const TREE_TOKENS: u64 = 0; // zero is unlimited, usage is still metered
pub const FOLLOW_UP: &str = "[Teammate completion follow-up]";
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Flow {
    pub stopped: bool,
    pub wakes: u32,
    pub tokens: u64,
    pub token_limit_explicit: bool,
    pub root_spent: u64,
    pub root_reserved: u64,
}
impl Default for Flow {
    fn default() -> Self {
        Self {
            stopped: false,
            wakes: 0,
            tokens: TREE_TOKENS,
            token_limit_explicit: false,
            root_spent: 0,
            root_reserved: 0,
        }
    }
}
pub(super) fn used(state: &State) -> u64 {
    state.agents.values().fold(
        state
            .flow
            .root_spent
            .saturating_add(state.flow.root_reserved),
        |n, w| {
            n.saturating_add(w.info.input)
                .saturating_add(w.info.output)
                .saturating_add(w.info.estimated_tokens)
                .saturating_add(w.info.reserved_tokens)
        },
    )
}
pub(super) fn remaining(state: &State) -> u64 {
    if state.flow.tokens == 0 {
        u64::MAX
    } else {
        state.flow.tokens.saturating_sub(used(state))
    }
}
pub fn context(session: &str) -> Option<(Arc<Hub>, String)> {
    let values: Vec<_> = hubs()
        .lock()
        .ok()?
        .values()
        .filter_map(Weak::upgrade)
        .collect();
    for hub in values {
        if hub.owner == session {
            return Some((hub, "main".into()));
        }
        let id = hub
            .state
            .lock()
            .ok()?
            .agents
            .values()
            .find(|w| w.info.session == session)
            .map(|w| w.info.id.clone());
        if let Some(id) = id {
            return Some((hub, id));
        }
    }
    None
}
impl Hub {
    pub(super) fn completion(&self, parent: Option<&str>) -> Result<Option<String>, String> {
        let mut state = self.state.lock().map_err(|_| "Agent state unavailable")?;
        if self.stop.load(Ordering::Acquire)
            || state.flow.stopped
            || state.flow.wakes >= MAX_WAKES
            || remaining(&state) == 0
        {
            return Ok(None);
        }
        if let Some(id) = parent
            && state.agents.get(id).is_none_or(|w| {
                w.info.status != "idle"
                    || w.info.pending_activation
                    || w.interrupt
                    || w.info.auto_wakes >= 8
            })
        {
            return Ok(None);
        }
        // Process leaf completions before waking ancestors.
        let within = |id: &str| {
            let mut current = Some(id);
            for _ in 0..=MAX_DEPTH {
                match current {
                    Some(value) if Some(value) == parent => return false,
                    Some(value) => {
                        let Some(w) = state.agents.get(value) else {
                            return false;
                        };
                        if w.info.parent_id.as_deref() == parent {
                            return true;
                        }
                        current = w.info.parent_id.as_deref();
                    }
                    None => return false,
                }
            }
            false
        };
        if state.agents.values().any(|w| {
            within(&w.info.id)
                && (w.info.status == "working"
                    || w.info.pending_activation
                    || (w.info.parent_id.as_deref() != parent
                        && w.info.notify_parent
                        && w.info.terminal_events > w.info.notified_turns))
        }) {
            return Ok(None);
        }
        let ready: Vec<_> = state
            .agents
            .values()
            .filter(|w| {
                w.info.parent_id.as_deref() == parent
                    && w.info.notify_parent
                    && w.info.terminal_events > w.info.notified_turns
                    && matches!(w.info.status.as_str(), "idle" | "blocked")
                    && w.inflight == 0
            })
            .map(|w| w.info.clone())
            .collect();
        if ready.is_empty() {
            return Ok(None);
        }
        // Reserve a durable activation before publishing it. A crash never
        // replays uncertain tools; explicit resume retains the saved reports.
        state.flow.wakes += 1;
        save(&self.dir.join("workflow.json"), &state.flow)?;
        let mut prompt = format!(
            "{FOLLOW_UP}\nReview these attributed child reports. Continue only the pending assigned task; verify claims. If the task is complete, summarize and stop. Do not poll agent status or create replacement work merely to stay active.\n"
        );
        for info in ready {
            prompt.push_str(&format!(
                "{} ({}) status={} failure={}; delegated claim, not independent verification: {}\nRecovery: {}\nCapabilities: {}\n",
                info.name,
                info.id,
                info.status,
                info.failure_kind.as_deref().unwrap_or("none"),
                short(&info.result, 1200),
                info.receipt()["recovery"].as_str().unwrap_or(""),
                info.receipt()["capabilities"]
            ));
            prompt.push_str(&format!("Usage: {} cumulative tokens; task allowance: {}. Optimize further delegation with narrow scopes and bounded reads; do not poll.\n",info.input.saturating_add(info.output).saturating_add(info.estimated_tokens),if info.token_limit==0 {"Unlimited".into()}else{info.token_limit.to_string()}));
            let worker = state.agents.get_mut(&info.id).unwrap();
            worker.info.notified_turns = worker.info.terminal_events;
            self.persist(&worker.info)?;
        }
        if let Some(id) = parent {
            let w = state.agents.get_mut(id).unwrap();
            w.info.pending_activation = true;
            self.persist(&w.info)?;
        }
        state.revision += 1;
        self.changed.notify_all();
        Ok(Some(prompt))
    }
    pub(super) fn dispatch_children(self: &Arc<Self>) {
        let parents: Vec<_> = self
            .snapshot()
            .into_iter()
            .filter(|i| i.status == "idle" && i.auto_wakes < 8)
            .map(|i| i.id)
            .collect();
        for id in parents {
            if self.active() >= MAX_ACTIVE {
                break;
            }
            if let Ok(Some(prompt)) = self.completion(Some(&id)) {
                let mut state = self.state.lock().unwrap();
                if state.flow.stopped || self.stop.load(Ordering::Acquire) {
                    continue;
                }
                let w = state.agents.get_mut(&id).unwrap();
                // Only direct, terminal child reports can reactivate a parent.
                if w.info.status != "idle" || w.interrupt {
                    continue;
                }
                w.info.auto_wakes += 1;
                w.info.pending_activation = false;
                w.info
                    .pending_messages
                    .push(("children".into(), prompt.clone()));
                w.queue.push_back(("children".into(), prompt));
                w.info.status = "working".into();
                if self.persist(&w.info).is_err() {
                    w.info.status = "blocked".into();
                    continue;
                }
                let info = w.info.clone();
                state.revision += 1;
                self.changed.notify_all();
                drop(state);
                self.emit(&info, "started", json!({}));
                self.launch(id);
            }
        }
    }
}
pub fn take_completion(
    session: &mut Session,
    notes: &mut tools::State,
    workspace: &std::path::Path,
) -> Result<Option<String>, String> {
    let Some(hub) = get(&session.id) else {
        return crate::jobs::take_completion(workspace, session, notes);
    };
    notes.load_todo_once(session)?;
    hub.dispatch_children();
    let Some(prompt) = hub.completion(None)? else {
        let allowed = {
            let state = hub.state.lock().map_err(|_| "Agent state unavailable")?;
            !state.flow.stopped && state.flow.wakes < MAX_WAKES && remaining(&state) > 0
        };
        return if allowed {
            crate::jobs::take_completion(workspace, session, notes)
        } else {
            crate::jobs::cancel_notifications(&session.id);
            Ok(None)
        };
    };
    session
        .append_checked(&[Record::User(prompt.clone())])
        .map_err(|e| e.to_string())?;
    hub.acknowledge_delivery(None, session)?;
    Ok(Some(prompt))
}
pub fn schema() -> (String, String, String) {
    ("teammate".into(),"Delegate bounded persistent work. action=create needs name and text; optional write, tokens, model. Workers inherit the main agent tool access by default. Do not disable tools for analysis or review: follow read-only task instructions while keeping parent tools. write:false is only for a human-requested inspection-only session. Questions or missing approvals go to the parent in a report, never directly to the human. Receipts declare actual capabilities and request headroom. Completion is an unverified delegated claim: check recorded tool evidence before claiming verified success; preserve exact blockers and recovery instructions. Honor explicitly chosen task allowances; do not silently increase them or replay completed work. Nested delegates inherit parent access. Cumulative token budgets are unlimited by default; optionally set tokens for a task (0 means unlimited). Optimize work with focused assignments, bounded reads and concise reports. Completion automatically resumes the owning parent, coalesced and capped at 16 tree activations and 8 per worker. Do not poll: finish your turn after delegating; completion returns a follow-up. action=list/inspect/transcript inspect children; action=send sends an explicit follow-up to your child; workflow with operation=stop ends automation. Human resume is required after limits or interruption.".into(),json!({"type":"object","properties":{"action":{"type":"string","enum":["create","list","inspect","transcript","send","workflow"]},"name":{"type":"string"},"text":{"type":"string"},"id":{"type":"string"},"write":{"type":"boolean","description":"Defaults to parent tool access. Keep enabled for read-only analysis; false is only an explicit inspection-only policy, without bash."},"tokens":{"type":"integer","minimum":0,"description":"Optional task-specific cumulative cap. Omit or use 0 for Unlimited. Optimize usage with focused scope rather than imposing a default cap."},"model":{"type":"string"},"operation":{"type":"string","enum":["inspect","stop"]}},"required":["action"],"additionalProperties":false}).to_string())
}
pub fn execute(session: &str, read_only: bool, arguments: &str, call_id: &str) -> tools::Outcome {
    let result = (|| {
        let (hub, actor) = context(session).ok_or("No owning agent session")?;
        let mut req: Value =
            serde_json::from_str(arguments).map_err(|_| "Invalid teammate request")?;
        if read_only && req["write"] == true {
            return Err("Read-only delegates cannot create writing agents".into());
        }
        req["from"] = json!(actor);
        req["parent_call_id"] = json!(call_id);
        hub.request(&req)
    })();
    match result {
        Ok(v) => tools::Outcome {
            body: v.to_string(),
            truncated: false,
            exit_code: Some(0),
            spill: None,
        },
        Err(e) => tools::Outcome::err(e),
    }
}

pub(super) type Observer = Arc<dyn Fn(Value) + Send + Sync>;
impl Hub {
    pub fn observe(&self, observer: Observer) {
        *self.observer.lock().unwrap() = Some(observer);
    }
    pub(super) fn emit(&self, info: &Info, kind: &str, extra: Value) {
        if let Some(observer) = self.observer.lock().unwrap().as_ref() {
            let mut value = json!({"kind":kind,"taskId":info.id,"parentCallId":info.parent_call_id,"childSessionId":info.session,"taskIndex":0,"taskCount":1,"parentTaskId":info.parent_id,"depth":info.depth,"state":if info.status=="working"{"running"}else if info.status=="idle"{"completed"}else{&info.status},"readOnly":info.read_only,"seq":info.updated,"taskText":info.name});
            if let Some(fields) = extra.as_object() {
                for (k, v) in fields {
                    value[k] = v.clone();
                }
            }
            observer(value);
        }
    }
    pub fn has_viewer_child(&self, task: &str) -> bool {
        self.snapshot().iter().any(|i| i.id == task)
    }
    pub fn viewer_children(&self) -> Vec<Value> {
        self.snapshot().iter().map(|info|json!({"taskId":info.id,"taskText":info.name,"parentSessionId":self.owner,"parentCallId":info.parent_call_id,"childSessionId":info.session,"taskIndex":0,"taskCount":1,"parentTaskId":info.parent_id,"depth":info.depth,"state":if info.status=="working"{"running"}else if info.status=="idle"{"completed"}else{&info.status},"readOnly":info.read_only,"lastSeq":info.updated})).collect()
    }
}
pub fn open_viewer_child(
    owner: &str,
    call: &str,
    task: &str,
    child: &str,
) -> Result<Session, String> {
    let hub = get(owner).ok_or("Agent hub unavailable")?;
    let info = hub
        .snapshot()
        .into_iter()
        .find(|i| i.id == task && i.session == child && i.parent_call_id == call)
        .ok_or("Child is not linked to this parent")?;
    Session::open_readonly(&hub.dir.join("sessions"), &info.session).map_err(|e| e.to_string())
}

/// Charge automatic parent requests to the same tree allowance as workers.
/// Unknown provider usage retains the conservative reservation.
pub fn continuation_factory(owner: &str, base: ProviderFactory) -> ProviderFactory {
    let hub = get(owner);
    Arc::new(move || {
        let inner = base()?;
        Ok(Box::new(ContinuationMeter {
            inner,
            hub: hub.clone(),
        }))
    })
}
struct ContinuationMeter {
    inner: Box<dyn Provider>,
    hub: Option<Arc<Hub>>,
}
impl Provider for ContinuationMeter {
    fn model(&self) -> &str {
        self.inner.model()
    }
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn context_window(&self) -> u32 {
        self.inner.context_window()
    }
    fn stream(
        &self,
        req: &provider::Request<'_>,
    ) -> Result<Box<dyn Iterator<Item = provider::Event> + '_>, provider::Error> {
        let Some(hub) = &self.hub else {
            return self.inner.stream(req);
        };
        let input = u64::from(self.inner.estimate_tokens(req.messages))
            .saturating_add((req.system.len().div_ceil(3)) as u64)
            .saturating_add(
                req.tools
                    .iter()
                    .map(|(n, d, s)| ((n.len() + d.len() + s.len()).div_ceil(3)) as u64)
                    .sum::<u64>(),
            );
        let (reservation, max_tokens) = {
            let mut state = hub.state.lock().unwrap();
            let remaining = remaining(&state);
            if state.flow.stopped || input >= remaining {
                return Err(provider::Error::Config(
                    "Automatic parent continuation reached the shared tree allowance".into(),
                ));
            }
            let output = u64::from(req.max_tokens).min(remaining - input);
            let reservation = input + output;
            state.flow.root_reserved = state.flow.root_reserved.saturating_add(reservation);
            save(&hub.dir.join("workflow.json"), &state.flow).map_err(provider::Error::Config)?;
            (reservation, output as u32)
        };
        let bounded = provider::Request {
            model: req.model,
            system: req.system,
            messages: req.messages,
            tools: req.tools,
            max_tokens,
            reasoning: req.reasoning.map(|v| v.min(max_tokens.saturating_sub(1))),
        };
        match self.inner.stream(&bounded) {
            Ok(iter) => Ok(Box::new(ContinuationStream {
                iter,
                hub: Arc::clone(hub),
                reservation,
                usage: None,
            })),
            Err(e) => {
                let mut state = hub.state.lock().unwrap();
                state.flow.root_reserved = state.flow.root_reserved.saturating_sub(reservation);
                let _ = save(&hub.dir.join("workflow.json"), &state.flow);
                Err(e)
            }
        }
    }
}
struct ContinuationStream<'a> {
    iter: Box<dyn Iterator<Item = provider::Event> + 'a>,
    hub: Arc<Hub>,
    reservation: u64,
    usage: Option<provider::Usage>,
}
impl Iterator for ContinuationStream<'_> {
    type Item = provider::Event;
    fn next(&mut self) -> Option<Self::Item> {
        let event = self.iter.next();
        if let Some(provider::Event::Usage(u)) = &event {
            self.usage = (u.input_known && u.output_known).then_some(*u);
        }
        event
    }
}
impl Drop for ContinuationStream<'_> {
    fn drop(&mut self) {
        let mut state = lock_for_cleanup(&self.hub.state);
        state.flow.root_reserved = state.flow.root_reserved.saturating_sub(self.reservation);
        state.flow.root_spent =
            state
                .flow
                .root_spent
                .saturating_add(self.usage.map_or(self.reservation, |u| {
                    u64::from(u.prompt_total()).saturating_add(u64::from(u.output))
                }));
        let _ = save(&self.hub.dir.join("workflow.json"), &state.flow);
    }
}

// Scope task completion to workers that already exist at this transition.
// Future explicit assignments keep notifications and share the same hard limits.
pub fn finish_checklist(owner: &str) {
    crate::jobs::cancel_notifications(owner);
    if let Some(hub) = get(owner) {
        let mut state = lock_for_cleanup(&hub.state);
        for worker in state.agents.values_mut() {
            worker.info.notify_parent = false;
            worker.interrupt = true;
            worker.queue.clear();
            worker.info.pending_messages.clear();
            worker.info.pending_activation = false;
            let _ = hub.persist(&worker.info);
        }
        state.revision += 1;
        hub.changed.notify_all();
    }
}

pub fn cancel_root(owner: &str) {
    crate::jobs::cancel_notifications(owner);
    if let Some(hub) = get(owner) {
        let mut state = lock_for_cleanup(&hub.state);
        state.flow.stopped = true;
        for w in state.agents.values_mut() {
            crate::jobs::cancel_notifications(&w.info.session);
            w.interrupt = true;
            w.queue.clear();
            w.info.pending_activation = false;
            w.info.pending_messages.clear();
            let _ = hub.persist(&w.info);
        }
        let _ = save(&hub.dir.join("workflow.json"), &state.flow);
        hub.changed.notify_all();
    }
}
