//! Durable adapter over the existing GC host runtime. No second agent loop.
use super::*;
use crate::durable::{Op, Store, Task};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
type Shared = Rc<RefCell<Store>>;
const MAX_BUFFERED_REASONING_RECORDS: usize = 256;
pub struct Host {
    pub store: Shared,
    definitions: BTreeMap<String, (u32, String)>,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn update(t: &Task, status: &str, outcome: Option<Value>) -> Op {
    Op::TaskUpdate {
        id: t.id.clone(),
        expected_revision: t.revision,
        status: status.into(),
        checkpoint: t.checkpoint.clone(),
        on: Vec::new(),
        policy: "all_settled".into(),
        due_at_ms: t.due_at_ms,
        outcome,
    }
}
fn token(parts: &[&str]) -> String {
    use ring::digest::{SHA256, digest};
    digest(&SHA256, parts.join("\0").as_bytes()).as_ref()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn commit(store: &Shared, ops: Vec<Op>) -> Result<u64, String> {
    let mut store = store.borrow_mut();
    let seq = store.state.sequence;
    store.commit(seq, ops)
}
fn finish_unsettled_after_runtime(
    store: &Shared,
    task_id: &str,
    context: Option<&Context>,
    disconnected: bool,
    result: &Result<(), String>,
    turn: &Value,
) -> Result<(), String> {
    let still_running = store
        .borrow()
        .state
        .tasks
        .get(task_id)
        .is_some_and(|task| task.status == "running");
    if still_running && let Some(context) = context {
        context.finish(
            if disconnected {
                "interrupted"
            } else {
                "failed"
            },
            result.as_ref().err().map(String::as_str),
            turn,
        )?;
    }
    Ok(())
}
impl Host {
    pub fn open(dir: &std::path::Path, fsync: bool) -> Result<Self, String> {
        let mut store = Store::open(&dir.join("durable"), fsync)?;
        store.recover()?;
        let store = Rc::new(RefCell::new(store));
        // Reconcile unfinished GC tool calls into the same authoritative commit
        // stream. An unsafe tool is never silently issued a second time.
        let mut ops = Vec::new();
        for cv in store.borrow().state.conversations.values() {
            let records = store.borrow().records(&cv.id)?;
            let mut calls = BTreeMap::new();
            let mut finished = std::collections::BTreeSet::new();
            for record in records {
                match record {
                    Record::ToolCall { id, name, .. } => {
                        calls.insert(id, name);
                    }
                    Record::ToolResult { call_id, .. } => {
                        finished.insert(call_id);
                    }
                    _ => {}
                }
            }
            for (id, name) in calls {
                if !finished.contains(&id) {
                    ops.push(Op::Entry{conversation:cv.id.clone(),record:Record::ToolResult{name,call_id:id,body:"error: interrupted; the external effect may have completed. Reconcile before retrying.".into()}.to_json()});
                }
            }
        }
        for live in store
            .borrow()
            .state
            .documents
            .values()
            .filter(|doc| doc.kind == "gc.live")
        {
            if let Some(task) = live.value["taskId"]
                .as_str()
                .and_then(|id| store.borrow().state.tasks.get(id).cloned())
                && !matches!(task.status.as_str(), "completed" | "failed" | "aborted")
                && let Some(text) = live.value["text"].as_str().filter(|text| !text.is_empty())
            {
                let records = store.borrow().records(&live.conversation)?;
                let start = records
                    .iter()
                    .rposition(|record| matches!(record, Record::User(_)))
                    .unwrap_or(0);
                if !records[start..].iter().any(
                    |record| matches!(record,Record::Assistant(answer) if answer.contains(text)),
                ) {
                    ops.push(Op::Entry {
                        conversation: live.conversation.clone(),
                        record: Record::Assistant(text.into()).to_json(),
                    });
                    ops.push(Op::Entry {
                        conversation: live.conversation.clone(),
                        record: Record::Meta(
                            "Recovered committed partial response; generation remains pending."
                                .into(),
                        )
                        .to_json(),
                    });
                }
            }
        }
        if !ops.is_empty() {
            commit(&store, ops)?;
        }
        Ok(Self {
            store,
            definitions: BTreeMap::from([("gc.generation".into(), (1, "safe".into()))]),
        })
    }
    fn eligible(&self, task: &Task) -> bool {
        let store = self.store.borrow();
        task.status == "pending"
            && !task.abort_requested
            && task.due_at_ms <= now()
            && self
                .definitions
                .get(&task.kind)
                .is_some_and(|(version, replay)| {
                    *version == task.version && (task.replay != "safe" || replay == "safe")
                })
            && !store.state.tasks.values().any(|child| {
                child.owner.as_deref() == Some(&task.id)
                    && !child.background
                    && !matches!(child.status.as_str(), "completed" | "failed" | "aborted")
            })
            && (task.kind != "gc.generation"
                || !store.state.tasks.values().any(|other| {
                    other.id != task.id
                        && other.kind == "gc.generation"
                        && other.conversation == task.conversation
                        && (matches!(other.status.as_str(), "running" | "waiting" | "completing")
                            || (other.status == "pending"
                                && (other.created_sequence, &other.id)
                                    < (task.created_sequence, &task.id)))
                }))
    }
    fn ready(&self, kind: Option<&str>) -> Option<Task> {
        self.store
            .borrow()
            .state
            .tasks
            .values()
            .filter(|task| {
                kind.map_or(task.kind != "gc.generation", |kind| task.kind == kind)
                    && self.eligible(task)
            })
            .min_by_key(|task| (task.created_sequence, &task.id))
            .cloned()
    }
    pub fn handle(
        &mut self,
        method: &str,
        p: &Value,
        dir: &std::path::Path,
        cfg: &config::Config,
        wire: &Rc<RefCell<Wire>>,
        request: &Value,
    ) -> Result<Value, String> {
        match method {
            "durable/create" => {
                let (settings, _, mode, _) = metadata(p, cfg)?;
                super::validate_tool_mode(wire.borrow().protocol_version, mode)?;
                let session =
                    Session::create(&dir.join("durable-projections")).map_err(|e| e.to_string())?;
                let id = session.id.clone();
                drop(session);
                let mut agent = p.clone();
                agent["cwd"] = json!(settings.workspace);
                agent["model"] = json!(settings.model);
                agent["provider"] = json!(settings.provider);
                agent["baseUrl"] = json!(settings.base_url);
                agent["maxTokens"] = json!(settings.max_tokens);
                agent["imageInput"] = json!(settings.image_input);
                commit(
                    &self.store,
                    vec![Op::ConversationCreate {
                        id: id.clone(),
                        agent,
                        provider_identity: crate::durable::identity()?,
                        owner: p["ownerTaskId"].as_str().map(str::to_owned),
                        parent: None,
                        fork_at: None,
                    }],
                )?;
                Ok(json!({"conversationId":id,"sequence":self.store.borrow().state.sequence}))
            }
            "durable/fork" => {
                let parent = string(p, "conversationId")?;
                let at = p["asOf"]
                    .as_u64()
                    .unwrap_or(self.store.borrow().state.sequence);
                let snapshot = self.store.borrow().snapshot_at(at)?;
                let original = snapshot
                    .conversations
                    .get(parent)
                    .ok_or("conversation does not exist at fork boundary")?;
                let session =
                    Session::create(&dir.join("durable-projections")).map_err(|e| e.to_string())?;
                let id = session.id.clone();
                drop(session);
                let mut ops = vec![Op::ConversationCreate {
                    id: id.clone(),
                    agent: original.agent.clone(),
                    provider_identity: crate::durable::identity()?,
                    owner: p["ownerTaskId"].as_str().map(str::to_owned),
                    parent: Some(parent.into()),
                    fork_at: Some(at),
                }];
                for document in self
                    .store
                    .borrow()
                    .state
                    .documents
                    .values()
                    .filter(|d| d.conversation == parent)
                {
                    let value = match document.fork.as_str() {
                        "initial" => document.initial.clone(),
                        "current" => document.value.clone(),
                        "as_of" => snapshot
                            .documents
                            .get(&format!("{parent}:{}", document.kind))
                            .map(|d| d.value.clone())
                            .ok_or("document absent at fork boundary")?,
                        _ => return Err("invalid document fork policy".into()),
                    };
                    ops.push(Op::Document {
                        conversation: id.clone(),
                        kind: document.kind.clone(),
                        version: document.version,
                        expected_revision: 0,
                        value,
                        history: document.history,
                        fork: document.fork.clone(),
                        initial: document.initial.clone(),
                    });
                }
                commit(&self.store, ops)?;
                Ok(json!({"conversationId":id,"parent":parent,"asOf":at}))
            }
            "durable/configure" => {
                let cv = string(p, "conversationId")?;
                let mut agent = self
                    .store
                    .borrow()
                    .state
                    .conversations
                    .get(cv)
                    .ok_or("unknown conversation")?
                    .agent
                    .clone();
                let patch = p["agent"]
                    .as_object()
                    .ok_or("agent patch must be an object")?;
                for (key, value) in patch {
                    agent[key] = value.clone();
                }
                metadata(&agent, cfg)?;
                commit(
                    &self.store,
                    vec![Op::Configure {
                        conversation: cv.into(),
                        agent,
                    }],
                )?;
                Ok(json!({"sequence":self.store.borrow().state.sequence}))
            }
            "durable/entries" => {
                let cv = string(p, "conversationId")?;
                let cursor = p["cursor"].as_u64().unwrap_or(0) as usize;
                let limit = p["limit"].as_u64().unwrap_or(25).clamp(1, 128) as usize;
                let records = self.store.borrow().records(cv)?;
                if cursor > records.len() {
                    return Err("entry cursor exceeds record count".into());
                }
                let mut end = cursor;
                let mut bytes = 0;
                let mut entries = Vec::new();
                for record in records.iter().skip(cursor).take(limit) {
                    let value = record.to_json();
                    let size = value.to_string().len();
                    if bytes + size > 768 * 1024 {
                        if entries.is_empty() {
                            return Err(
                                "entry exceeds response limit; use artifact range reads".into()
                            );
                        }
                        break;
                    }
                    bytes += size;
                    entries.push(value);
                    end += 1;
                }
                Ok(json!({"entries":entries,"cursor":end,"hasMore":end<records.len()}))
            }
            "durable/commit" => {
                let seq = p["expectedSequence"]
                    .as_u64()
                    .ok_or("expectedSequence required")?;
                let ops: Vec<Op> =
                    serde_json::from_value(p["operations"].clone()).map_err(|e| e.to_string())?;
                let seq = self.store.borrow_mut().commit(seq, ops)?;
                Ok(json!({"sequence":seq}))
            }
            "durable/inspect" => {
                let store = self.store.borrow();
                let cv = p["conversationId"].as_str();
                let cursor = p["cursor"].as_str().unwrap_or("");
                let limit = p["limit"].as_u64().unwrap_or(25).clamp(1, 128) as usize;
                enum Resource<'a> {
                    Conversation(&'a crate::durable::Conversation),
                    Document(&'a crate::durable::Document),
                    Task(&'a Task),
                }
                let mut resources = BTreeMap::new();
                for c in store
                    .state
                    .conversations
                    .values()
                    .filter(|c| cv.is_none_or(|id| c.id == id))
                {
                    resources.insert(format!("conversation:{}", c.id), Resource::Conversation(c));
                }
                for d in store
                    .state
                    .documents
                    .values()
                    .filter(|d| cv.is_none_or(|id| d.conversation == id))
                {
                    resources.insert(
                        format!("document:{}:{}", d.conversation, d.kind),
                        Resource::Document(d),
                    );
                }
                for t in store
                    .state
                    .tasks
                    .values()
                    .filter(|t| cv.is_none_or(|id| t.conversation == id))
                {
                    resources.insert(format!("task:{}", t.id), Resource::Task(t));
                }
                let mut items = Vec::new();
                let mut bytes = 0;
                let mut next = cursor.to_owned();
                let mut more = false;
                for (key, resource) in resources.range((
                    std::ops::Bound::Excluded(cursor.to_owned()),
                    std::ops::Bound::Unbounded,
                )) {
                    if items.len() >= limit {
                        more = true;
                        break;
                    }
                    let mut value = match resource {
                        Resource::Conversation(c) => json!({"type":"conversation","value":c}),
                        Resource::Document(d) => json!({"type":"document","value":d}),
                        Resource::Task(t) => {
                            let blocked = if t.status != "pending" {
                                None
                            } else {
                                match self.definitions.get(&t.kind) {
                                    None => Some("missing_definition"),
                                    Some((v, _)) if *v != t.version => Some("version_mismatch"),
                                    Some((_, r)) if t.replay == "safe" && r != "safe" => {
                                        Some("replay_policy_mismatch")
                                    }
                                    _ => None,
                                }
                            };
                            json!({"type":"task","value":t,"blocked":blocked})
                        }
                    };
                    let mut size = value.to_string().len();
                    if size > 768 * 1024 {
                        value = match resource {
                            Resource::Conversation(c) => {
                                json!({"type":"conversation","truncated":true,"value":{"id":c.id,"providerIdentity":c.provider_identity,"entries":c.entries}})
                            }
                            Resource::Document(d) => {
                                json!({"type":"document","truncated":true,"value":{"conversation":d.conversation,"kind":d.kind,"revision":d.revision,"version":d.version},"readMethod":"durable/document"})
                            }
                            Resource::Task(t) => {
                                json!({"type":"task","truncated":true,"value":{"id":t.id,"revision":t.revision,"status":t.status},"readMethod":"durable/task"})
                            }
                        };
                        size = value.to_string().len();
                    }
                    if bytes + size > 768 * 1024 {
                        more = true;
                        break;
                    }
                    bytes += size;
                    next = key.clone();
                    items.push(value);
                }
                Ok(
                    json!({"sequence":store.state.sequence,"items":items,"cursor":next,"hasMore":more}),
                )
            }
            "durable/changes" => self.store.borrow().changes(
                p["cursor"].as_u64().unwrap_or(0),
                p["limit"].as_u64().unwrap_or(25) as usize,
            ),
            "durable/task" => {
                let id = string(p, "taskId")?;
                Ok(json!({"task":self.store.borrow().state.tasks.get(id)}))
            }
            "durable/reset" => {
                let cv = string(p, "conversationId")?;
                if self.store.borrow().state.tasks.values().any(|t| {
                    t.conversation == cv
                        && !matches!(t.status.as_str(), "completed" | "failed" | "aborted")
                }) {
                    return Err("abort or settle conversation tasks before reset".into());
                }
                commit(
                    &self.store,
                    vec![Op::Reset {
                        conversation: cv.into(),
                        record: Record::Meta("Conversation reset".into()).to_json(),
                    }],
                )?;
                Ok(json!({"sequence":self.store.borrow().state.sequence}))
            }
            "durable/taskGraph" => Ok(self.store.borrow().task_graph()),
            "durable/document" => {
                let cv = string(p, "conversationId")?;
                let kind = string(p, "kind")?;
                if let Some(seq) = p["asOf"].as_u64() {
                    return Ok(
                        json!({"document":self.store.borrow().document_as_of(cv,kind,seq)?}),
                    );
                }
                Ok(
                    json!({"document":self.store.borrow().state.documents.get(&format!("{cv}:{kind}"))}),
                )
            }
            "durable/register" => {
                let definitions = p["tasks"].as_array().ok_or("tasks must be an array")?;
                if definitions.len() > 128 {
                    return Err("too many task definitions".into());
                }
                let mut next = self.definitions.clone();
                for definition in definitions {
                    let kind = string(definition, "kind")?;
                    if kind.starts_with("gc.") {
                        return Err("gc task definitions are reserved".into());
                    }
                    let version = definition["version"]
                        .as_u64()
                        .filter(|v| *v > 0 && *v <= u32::MAX as u64)
                        .ok_or("positive task version required")?;
                    let replay = definition["replay"].as_str().unwrap_or("unsafe");
                    if !matches!(replay, "safe" | "unsafe") {
                        return Err("invalid replay policy".into());
                    }
                    next.insert(kind.into(), (version as u32, replay.into()));
                }
                self.definitions = next;
                Ok(json!({"registered":true}))
            }
            "durable/submit" => {
                let cv = string(p, "conversationId")?;
                let id = string(p, "requestId")?;
                if let Some(existing) = self.store.borrow().submission(cv, id) {
                    if existing.input_digest != crate::durable::input_digest(&p["input"]) {
                        return Err("request ID already belongs to different input".into());
                    }
                    return Ok(json!({"submission":existing,"duplicate":true}));
                }
                let input = p["input"].clone();
                if !input.is_object() {
                    return Err("input must contain text and optional context/images".into());
                }
                let settings = metadata(
                    &self
                        .store
                        .borrow()
                        .state
                        .conversations
                        .get(cv)
                        .ok_or("unknown conversation")?
                        .agent,
                    cfg,
                )?
                .0;
                let task = format!("generation-{}", token(&[cv, id]));
                support::input(&input, settings.image_input)?;
                commit(
                    &self.store,
                    vec![Op::Submit {
                        conversation: cv.into(),
                        request_id: id.into(),
                        task_id: task.clone(),
                        input,
                    }],
                )?;
                Ok(json!({"submission":self.store.borrow().state.tasks[&task],"duplicate":false}))
            }
            "durable/submission" => {
                let cv = string(p, "conversationId")?;
                let id = string(p, "requestId")?;
                Ok(json!({"submission":self.store.borrow().submission(cv,id)}))
            }
            "durable/claim" => {
                let Some(task) = self.ready(p["kind"].as_str()) else {
                    return Ok(json!({"task":null}));
                };
                if task.kind == "gc.generation" {
                    return Err("use durable/run for GC generation tasks".into());
                }
                if task.replay == "safe" && self.definitions[&task.kind].1 != "safe" {
                    return Err("stored safe task cannot run under unsafe definition".into());
                }
                commit(&self.store, vec![update(&task, "running", None)])?;
                Ok(json!({"task":self.store.borrow().state.tasks[&task.id]}))
            }
            "durable/run" | "durable/resume" => {
                let limit = p["limit"].as_u64().unwrap_or(32).clamp(1, 32);
                let mut settled = Vec::new();
                for _ in 0..limit {
                    let task = if let Some(id) = p["taskId"].as_str() {
                        let task = self
                            .store
                            .borrow()
                            .state
                            .tasks
                            .get(id)
                            .cloned()
                            .ok_or("unknown task")?;
                        if !self.eligible(&task) {
                            return Err("task is not ready; inspect ownership, version, due time and earlier submissions".into());
                        }
                        task
                    } else {
                        let Some(task) = self.ready(Some("gc.generation")) else {
                            break;
                        };
                        task
                    };
                    if task.kind != "gc.generation" {
                        return Err("only generation tasks run inside GC".into());
                    }
                    if let Err(error) = self.run_task(&task, dir, cfg, wire, request) {
                        let current = self.store.borrow().state.tasks.get(&task.id).cloned();
                        if let Some(current) = current.filter(|t| t.status == "running") {
                            commit(
                                &self.store,
                                vec![update(&current, "failed", Some(json!({"message":error})))],
                            )?;
                        }
                        return Err(error);
                    }
                    settled.push(task.id.clone());
                    if p["taskId"].is_string() {
                        break;
                    }
                }
                Ok(json!({"settled":settled,"sequence":self.store.borrow().state.sequence}))
            }
            "durable/abort" => {
                let id = string(p, "taskId")?;
                commit(
                    &self.store,
                    vec![Op::Abort {
                        id: id.into(),
                        include_background: p["includeBackground"].as_bool().unwrap_or(false),
                    }],
                )?;
                Ok(json!({"accepted":true}))
            }
            _ => Err("unknown durable method".into()),
        }
    }
    fn run_task(
        &mut self,
        task: &Task,
        dir: &std::path::Path,
        cfg: &config::Config,
        wire: &Rc<RefCell<Wire>>,
        request: &Value,
    ) -> Result<(), String> {
        let cv = self
            .store
            .borrow()
            .state
            .conversations
            .get(&task.conversation)
            .cloned()
            .ok_or("unknown conversation")?;
        let (mut settings, host_tools, tool_mode, instructions) = metadata(&cv.agent, cfg)?;
        settings.provider_session = Some(cv.provider_identity.clone());
        let mut start = update(task, "running", None);
        let mut admission = Vec::new();
        if task.checkpoint["phase"] == "admitted" {
            admission.extend(
                support::input(&task.input, settings.image_input)?
                    .iter()
                    .map(|record| Op::Entry {
                        conversation: cv.id.clone(),
                        record: record.to_json(),
                    }),
            );
            if let Op::TaskUpdate { checkpoint, .. } = &mut start {
                *checkpoint = json!({"phase":"executing"});
            }
        }
        admission.push(start);
        if task.checkpoint["phase"] == "admitted" {
            admission.push(Op::Consume {
                id: task.id.clone(),
            });
        }
        commit(&self.store, admission)?;
        let records = self.store.borrow().records(&cv.id)?;
        let projection = dir.join("durable-projections");
        std::fs::create_dir_all(&projection).map_err(|e| e.to_string())?;
        let path = projection.join(format!("{}.jsonl", cv.id));
        let mut bytes = Vec::new();
        for record in &records {
            serde_json::to_writer(&mut bytes, &record.to_json()).map_err(|e| e.to_string())?;
            bytes.push(b'\n');
        }
        crate::auth::secure_write(&path, &bytes)?;
        let mut session = Session::open(&projection, &cv.id).map_err(|e| e.to_string())?;
        let restored = Session::raw_records(&session.path).map_err(|e| e.to_string())?;
        if restored.len() > records.len() {
            commit(
                &self.store,
                restored[records.len()..]
                    .iter()
                    .map(|record| Op::Entry {
                        conversation: cv.id.clone(),
                        record: record.to_json(),
                    })
                    .collect(),
            )?;
        }

        let context = Context::new(self.store.clone(), task.id.clone(), cv.id.clone());
        session.observe_commits(context.observer());
        let mut state = tools::State::new();
        state.restore_todo(&session.history_records())?;
        for record in &session.records {
            if let Record::State(items) = record {
                state.apply(&glass_mod_api::ToolCall {
                    id: "restore".into(),
                    name: "state".into(),
                    arguments: json!({"items":items}).to_string(),
                });
            }
        }
        let mut conversation = Conversation {
            session,
            cfg: settings,
            state,
            tools: host_tools,
            tool_mode,
            instructions,
            stats_cache: None,
            durable: Some(context),
        };
        wire.borrow_mut().durable = Some(self.store.clone());
        wire.borrow_mut().durable_task = Some(task.id.clone());
        let result = run(&mut conversation, wire, request, Vec::new());
        wire.borrow_mut().durable = None;
        wire.borrow_mut().durable_task = None;
        finish_unsettled_after_runtime(
            &self.store,
            &task.id,
            conversation.durable.as_ref(),
            wire.borrow().disconnected,
            &result,
            request,
        )?;
        result
    }
}
fn trim_tail(text: &mut String) {
    if text.len() > 64 * 1024 {
        let mut at = text.len() - 64 * 1024;
        while !text.is_char_boundary(at) {
            at += 1;
        }
        text.drain(..at);
    }
}
struct Partial {
    at: Instant,
    text: String,
    reasoning: String,
    pending_text: String,
    pending_reasoning: String,
}
pub struct Context {
    store: Shared,
    task: String,
    conversation: String,
    partial: Rc<RefCell<Partial>>,
    buffer: Rc<RefCell<Vec<Record>>>,
}
impl Context {
    fn new(store: Shared, task: String, conversation: String) -> Self {
        Self {
            store,
            task,
            conversation,
            buffer: Rc::new(RefCell::new(Vec::new())),
            partial: Rc::new(RefCell::new(Partial {
                at: Instant::now() - Duration::from_secs(1),
                text: String::new(),
                reasoning: String::new(),
                pending_text: String::new(),
                pending_reasoning: String::new(),
            })),
        }
    }
    fn observer(&self) -> Rc<crate::session::CommitObserver> {
        let store = self.store.clone();
        let task = self.task.clone();
        let cv = self.conversation.clone();
        let buffer = self.buffer.clone();
        let partial = self.partial.clone();
        Rc::new(move |records| {
            if records
                .iter()
                .all(|record| matches!(record, Record::Reasoning(_)))
            {
                let mut pending = buffer.borrow_mut();
                for record in records {
                    if let Record::Reasoning(text) = record {
                        if let Some(Record::Reasoning(previous)) = pending.last_mut() {
                            previous.push_str(text);
                        } else {
                            pending.push(record.clone());
                        }
                    }
                }
                drop(pending);
                let pending = buffer.borrow();
                let pending_bytes = pending
                    .iter()
                    .map(|record| match record {
                        Record::Reasoning(text) => text.len(),
                        _ => 0,
                    })
                    .sum::<usize>();
                if pending.len() < MAX_BUFFERED_REASONING_RECORDS && pending_bytes < 64 * 1024 {
                    return Ok(());
                }
                drop(pending);
            }
            let mut owned = std::mem::take(&mut *buffer.borrow_mut());
            if !records
                .iter()
                .all(|record| matches!(record, Record::Reasoning(_)))
            {
                owned.extend_from_slice(records);
            }
            let mut ops = Vec::new();
            for record in &owned {
                ops.push(Op::Entry {
                    conversation: cv.clone(),
                    record: record.to_json(),
                });
                match record {
                    Record::ToolCall {
                        id,
                        name,
                        arguments,
                    } => {
                        let tool_id = format!("tool-{}", token(&[&task, id]));
                        ops.push(Op::TaskCreate {
                            id: tool_id.clone(),
                            conversation: cv.clone(),
                            kind: "gc.tool".into(),
                            version: 1,
                            owner: Some(task.clone()),
                            background: false,
                            replay: "unsafe".into(),
                            input: json!({"callId":id,"name":name,"arguments":arguments}),
                            checkpoint: json!({"phase":"execute"}),
                        });
                        ops.push(Op::TaskUpdate {
                            id: tool_id,
                            expected_revision: 1,
                            status: "running".into(),
                            checkpoint: json!({"phase":"execute"}),
                            on: Vec::new(),
                            policy: "all_settled".into(),
                            due_at_ms: 0,
                            outcome: None,
                        });
                    }
                    Record::ToolResult { call_id, body, .. } => {
                        let id = format!("tool-{}", token(&[&task, call_id]));
                        if let Some(t) = store
                            .borrow()
                            .state
                            .tasks
                            .get(&id)
                            .filter(|t| t.status == "running")
                            .cloned()
                        {
                            ops.push(update(&t,if t.abort_requested{"aborted"}else if body.starts_with("error:"){"failed"}else{"completed"},Some(json!({"callId":call_id,"outputBytes":body.len(),"isError":body.starts_with("error:")}))));
                        }
                    }
                    _ => {}
                }
            }
            commit(&store, ops).map_err(std::io::Error::other)?;
            if owned
                .iter()
                .any(|record| matches!(record, Record::Assistant(_)))
            {
                partial.borrow_mut().text.clear();
            }
            Ok(())
        })
    }
    pub fn publish(&self, kind: &str, data: Value, turn: &Value) -> bool {
        let mut partial = self.partial.borrow_mut();
        if matches!(kind, "text" | "reasoning") {
            let chunk = data["text"].as_str().unwrap_or("");
            if kind == "text" {
                partial.text.push_str(chunk);
                partial.pending_text.push_str(chunk);
            } else {
                partial.reasoning.push_str(chunk);
                partial.pending_reasoning.push_str(chunk);
            }
            trim_tail(&mut partial.text);
            trim_tail(&mut partial.reasoning);
            if partial.at.elapsed() < Duration::from_millis(100) {
                return false;
            }
        }
        let current = self
            .store
            .borrow()
            .state
            .documents
            .get(&format!("{}:gc.live", self.conversation))
            .map_or(0, |d| d.revision);
        let text = std::mem::take(&mut partial.pending_text);
        let reasoning = std::mem::take(&mut partial.pending_reasoning);
        partial.at = Instant::now();
        let mut ops: Vec<_> = std::mem::take(&mut *self.buffer.borrow_mut())
            .iter()
            .map(|record| Op::Entry {
                conversation: self.conversation.clone(),
                record: record.to_json(),
            })
            .collect();
        ops.push(Op::Document{conversation:self.conversation.clone(),kind:"gc.live".into(),version:1,expected_revision:current,value:json!({"taskId":self.task,"kind":kind,"data":data,"text":partial.text,"reasoning":partial.reasoning}),history:false,fork:"initial".into(),initial:Value::Null});
        let result = commit(&self.store, ops);
        if result.is_err() {
            self.store.borrow_mut().poison();
            return false;
        }
        if !text.is_empty() {
            event(&self.conversation, turn, "text", json!({"text":text}));
        }
        if !reasoning.is_empty() {
            event(
                &self.conversation,
                turn,
                "reasoning",
                json!({"text":reasoning}),
            );
        }
        !matches!(kind, "text" | "reasoning")
    }
    pub fn finish(&self, reason: &str, message: Option<&str>, turn: &Value) -> Result<(), String> {
        let task = self
            .store
            .borrow()
            .state
            .tasks
            .get(&self.task)
            .cloned()
            .ok_or("generation task missing")?;
        let status = if task.abort_requested {
            "aborted"
        } else {
            match reason {
                "completed" => "completed",
                "cancelled" => "aborted",
                "interrupted" => "pending",
                _ => "failed",
            }
        };
        // Finalize the task, its live projection, and any buffered transcript
        // records in one journal commit. Previously `publish("settling")`
        // committed the live document separately before this terminal write;
        // a slow or failed second append left an ambiguous `settling` record
        // after usage had already been reported.
        let buffered = self.buffer.borrow().clone();
        let mut ops = buffered
            .iter()
            .map(|record| Op::Entry {
                conversation: self.conversation.clone(),
                record: record.to_json(),
            })
            .collect::<Vec<_>>();
        ops.push(update(
            &task,
            status,
            Some(json!({"status":status,"message":message})),
        ));
        let revision = self
            .store
            .borrow()
            .state
            .documents
            .get(&format!("{}:gc.live", self.conversation))
            .map_or(0, |d| d.revision);
        let partial = self.partial.borrow();
        let live_kind = if status == "pending" {
            "interrupted"
        } else {
            "settled"
        };
        let recovery_text = if status == "pending" {
            partial.text.as_str()
        } else {
            ""
        };
        let recovery_reasoning = if status == "pending" {
            partial.reasoning.as_str()
        } else {
            ""
        };
        ops.push(Op::Document {
            conversation: self.conversation.clone(),
            kind: "gc.live".into(),
            version: 1,
            expected_revision: revision,
            value: json!({
                "taskId": self.task,
                "kind": live_kind,
                "reason": reason,
                "text": recovery_text,
                "reasoning": recovery_reasoning,
            }),
            history: false,
            fork: "initial".into(),
            initial: Value::Null,
        });
        let pending_text = partial.pending_text.clone();
        let pending_reasoning = partial.pending_reasoning.clone();
        drop(partial);

        let started = Instant::now();
        trace_runtime_frame(
            "durable_finish",
            "journal_commit_begin",
            "task_settlement",
            0,
            None,
        );
        if let Err(error) = commit(&self.store, ops) {
            self.store.borrow_mut().poison();
            trace_runtime_frame(
                "durable_finish",
                "journal_commit_error",
                "task_settlement",
                0,
                Some(started.elapsed().as_micros()),
            );
            return Err(error);
        }
        trace_runtime_frame(
            "durable_finish",
            "journal_commit_complete",
            "task_settlement",
            0,
            Some(started.elapsed().as_micros()),
        );
        self.buffer.borrow_mut().clear();
        {
            let mut partial = self.partial.borrow_mut();
            partial.pending_text.clear();
            partial.pending_reasoning.clear();
        }
        if !pending_text.is_empty() {
            event(
                &self.conversation,
                turn,
                "text",
                json!({"text":pending_text}),
            );
        }
        if !pending_reasoning.is_empty() {
            event(
                &self.conversation,
                turn,
                "reasoning",
                json!({"text":pending_reasoning}),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (PathBuf, Host) {
        let dir = std::env::temp_dir().join(format!(
            "gc-host-durable-{}",
            crate::durable::identity().unwrap()
        ));
        let host = Host::open(&dir, false).unwrap();
        commit(
            &host.store,
            vec![Op::ConversationCreate {
                id: "root".into(),
                agent: json!({}),
                provider_identity: crate::durable::identity().unwrap(),
                owner: None,
                parent: None,
                fork_at: None,
            }],
        )
        .unwrap();
        (dir, host)
    }
    #[test]
    fn submissions_are_fifo_and_not_prematurely_visible() {
        let (dir, host) = setup();
        for (id, text) in [("z-first", "first"), ("a-second", "second")] {
            commit(
                &host.store,
                vec![Op::Submit {
                    conversation: "root".into(),
                    request_id: id.into(),
                    task_id: id.into(),
                    input: json!({"text":text}),
                }],
            )
            .unwrap();
        }
        assert!(host.store.borrow().records("root").unwrap().is_empty());
        assert_eq!(host.ready(Some("gc.generation")).unwrap().id, "z-first");
        let task = host.store.borrow().state.tasks["z-first"].clone();
        commit(&host.store, vec![update(&task, "running", None)]).unwrap();
        assert!(host.ready(Some("gc.generation")).is_none());
        let task = host.store.borrow().state.tasks["z-first"].clone();
        commit(&host.store, vec![update(&task, "completed", None)]).unwrap();
        assert_eq!(host.ready(Some("gc.generation")).unwrap().id, "a-second");
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn custom_claim_skips_generation_and_checks_registry() {
        let (dir, mut host) = setup();
        commit(
            &host.store,
            vec![
                Op::Submit {
                    conversation: "root".into(),
                    request_id: "input".into(),
                    task_id: "input".into(),
                    input: json!({"text":"hello"}),
                },
                Op::TaskCreate {
                    id: "custom".into(),
                    conversation: "root".into(),
                    kind: "app.step".into(),
                    version: 1,
                    owner: None,
                    background: false,
                    replay: "safe".into(),
                    input: Value::Null,
                    checkpoint: Value::Null,
                },
            ],
        )
        .unwrap();
        assert!(host.ready(None).is_none());
        host.definitions
            .insert("app.step".into(), (1, "unsafe".into()));
        assert!(host.ready(None).is_none());
        host.definitions
            .insert("app.step".into(), (1, "safe".into()));
        assert_eq!(host.ready(None).unwrap().id, "custom");
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn running_task(host: &Host, id: &str) {
        commit(
            &host.store,
            vec![
                Op::TaskCreate {
                    id: id.into(),
                    conversation: "root".into(),
                    kind: "gc.generation".into(),
                    version: 1,
                    owner: None,
                    background: false,
                    replay: "safe".into(),
                    input: json!({"text":"answer"}),
                    checkpoint: json!({"phase":"executing"}),
                },
                Op::TaskUpdate {
                    id: id.into(),
                    expected_revision: 1,
                    status: "running".into(),
                    checkpoint: json!({"phase":"executing"}),
                    on: Vec::new(),
                    policy: "all_settled".into(),
                    due_at_ms: 0,
                    outcome: None,
                },
            ],
        )
        .unwrap();
    }

    #[test]
    fn finish_atomically_settles_and_flushes_buffered_stream_state() {
        let (dir, host) = setup();
        running_task(&host, "generation");
        let context = Context::new(host.store.clone(), "generation".into(), "root".into());
        context
            .buffer
            .borrow_mut()
            .push(Record::Reasoning("buffered reasoning".into()));
        {
            let mut partial = context.partial.borrow_mut();
            partial.text = "final answer".into();
            partial.pending_text = " answer".into();
            partial.reasoning = "private reasoning".into();
            partial.pending_reasoning = " reasoning".into();
        }
        let before = host.store.borrow().state.sequence;

        context.finish("completed", None, &json!("turn-1")).unwrap();

        let store = host.store.borrow();
        assert_eq!(store.state.sequence, before + 1);
        assert_eq!(store.state.tasks["generation"].status, "completed");
        let live = &store.state.documents["root:gc.live"].value;
        assert_eq!(live["kind"], "settled");
        assert_eq!(live["reason"], "completed");
        assert_eq!(
            store.records("root").unwrap(),
            vec![Record::Reasoning("buffered reasoning".into())]
        );
        drop(store);
        drop(context);
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn interrupted_finish_keeps_recovery_text_without_stale_settling_state() {
        let (dir, host) = setup();
        running_task(&host, "generation");
        let context = Context::new(host.store.clone(), "generation".into(), "root".into());
        {
            let mut partial = context.partial.borrow_mut();
            partial.text = "unfinished response".into();
            partial.pending_text = "response".into();
        }

        context
            .finish("interrupted", Some("host disconnected"), &json!("turn-1"))
            .unwrap();

        let store = host.store.borrow();
        assert_eq!(store.state.tasks["generation"].status, "pending");
        let live = &store.state.documents["root:gc.live"].value;
        assert_eq!(live["kind"], "interrupted");
        assert_eq!(live["text"], "unfinished response");
        assert_eq!(live["reason"], "interrupted");
        drop(store);
        drop(context);
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn poisoned_journal_cannot_claim_terminal_settlement() {
        let (dir, host) = setup();
        running_task(&host, "generation");
        let context = Context::new(host.store.clone(), "generation".into(), "root".into());
        let before = host.store.borrow().state.sequence;
        host.store.borrow_mut().poison();

        assert!(context.finish("completed", None, &json!("turn-1")).is_err());

        let store = host.store.borrow();
        assert_eq!(store.state.sequence, before);
        assert_eq!(store.state.tasks["generation"].status, "running");
        assert!(!store.state.documents.contains_key("root:gc.live"));
        drop(store);
        drop(context);
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn early_disconnected_runtime_exit_is_checkpointed_as_interrupted() {
        let (dir, host) = setup();
        running_task(&host, "generation");
        let context = Context::new(host.store.clone(), "generation".into(), "root".into());
        {
            let mut partial = context.partial.borrow_mut();
            partial.text = "before disconnect".into();
        }
        let result = Ok(());

        finish_unsettled_after_runtime(
            &host.store,
            "generation",
            Some(&context),
            true,
            &result,
            &json!("turn-1"),
        )
        .unwrap();

        let store = host.store.borrow();
        assert_eq!(store.state.tasks["generation"].status, "pending");
        let live = &store.state.documents["root:gc.live"].value;
        assert_eq!(live["kind"], "interrupted");
        assert_eq!(live["text"], "before disconnect");
        drop(store);
        drop(context);
        drop(host);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
