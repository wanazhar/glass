//! Optional durable runtime substrate. One checked journal frame is one atomic
//! commit across documents, entries, submissions and checkpointed tasks.
use fs2::FileExt;
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};
const FRAME: usize = 1024 * 1024;
const OBJECTS: usize = 16384;
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub sequence: u64,
    pub conversations: BTreeMap<String, Conversation>,
    pub documents: BTreeMap<String, Document>,
    pub tasks: BTreeMap<String, Task>,
    pub submissions: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub agent: Value,
    pub provider_identity: String,
    pub owner: Option<String>,
    pub parent: Option<String>,
    pub fork_at: Option<u64>,
    pub entries: u64,
    pub epoch: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Document {
    pub conversation: String,
    pub kind: String,
    pub version: u32,
    pub revision: u64,
    pub value: Value,
    pub history: bool,
    pub fork: String,
    pub initial: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub conversation: String,
    pub kind: String,
    pub version: u32,
    pub owner: Option<String>,
    pub background: bool,
    pub created_sequence: u64,
    pub replay: String,
    pub input: Value,
    pub input_digest: String,
    pub checkpoint: Value,
    pub status: String,
    pub revision: u64,
    pub on: Vec<String>,
    pub policy: String,
    pub due_at_ms: u64,
    pub outcome: Option<Value>,
    pub abort_requested: bool,
}
impl Task {
    fn terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed" | "aborted")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum Op {
    #[serde(rename = "conversation.create")]
    ConversationCreate {
        id: String,
        agent: Value,
        provider_identity: String,
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        fork_at: Option<u64>,
    },
    #[serde(rename = "conversation.configure")]
    Configure { conversation: String, agent: Value },
    #[serde(rename = "entry.append")]
    Entry { conversation: String, record: Value },
    #[serde(rename = "conversation.reset")]
    Reset { conversation: String, record: Value },
    #[serde(rename = "document.put")]
    Document {
        conversation: String,
        kind: String,
        version: u32,
        expected_revision: u64,
        value: Value,
        #[serde(default)]
        history: bool,
        #[serde(default = "fork_current")]
        fork: String,
        #[serde(default)]
        initial: Value,
    },
    #[serde(rename = "task.create")]
    TaskCreate {
        id: String,
        conversation: String,
        kind: String,
        version: u32,
        #[serde(default)]
        owner: Option<String>,
        #[serde(default)]
        background: bool,
        #[serde(default = "unsafe_replay")]
        replay: String,
        #[serde(default)]
        input: Value,
        #[serde(default)]
        checkpoint: Value,
    },
    #[serde(rename = "task.update")]
    TaskUpdate {
        id: String,
        expected_revision: u64,
        status: String,
        #[serde(default)]
        checkpoint: Value,
        #[serde(default)]
        on: Vec<String>,
        #[serde(default = "all_settled")]
        policy: String,
        #[serde(default)]
        due_at_ms: u64,
        #[serde(default)]
        outcome: Option<Value>,
    },
    #[serde(rename = "submission.consume")]
    Consume { id: String },
    #[serde(rename = "task.migrate")]
    TaskMigrate {
        id: String,
        expected_revision: u64,
        version: u32,
        checkpoint: Value,
    },
    #[serde(rename = "document.migrate")]
    DocumentMigrate {
        conversation: String,
        kind: String,
        expected_revision: u64,
        version: u32,
        value: Value,
    },
    #[serde(rename = "submission.create")]
    Submit {
        conversation: String,
        request_id: String,
        task_id: String,
        input: Value,
    },
    #[serde(rename = "task.abort")]
    Abort {
        id: String,
        #[serde(default)]
        include_background: bool,
    },
}
fn fork_current() -> String {
    "current".into()
}
fn unsafe_replay() -> String {
    "unsafe".into()
}
fn all_settled() -> String {
    "all_settled".into()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Commit {
    sequence: u64,
    previous: String,
    ops: Vec<Op>,
    hash: String,
}
pub struct Store {
    pub state: State,
    path: PathBuf,
    file: File,
    _lock: File,
    last_hash: String,
    poisoned: bool,
    fsync: bool,
}
pub fn input_digest(value: &Value) -> String {
    digest(&SHA256, value.to_string().as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn valid_kind(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/'))
}
fn valid_request(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn doc_key(cv: &str, kind: &str) -> String {
    format!("{cv}:{kind}")
}
fn request_key(cv: &str, key: &str) -> String {
    format!("{cv}:{key}")
}
fn hash(sequence: u64, previous: &str, ops: &[Op]) -> Result<String, String> {
    let data = serde_json::to_vec(&(sequence, previous, ops)).map_err(|e| e.to_string())?;
    Ok(digest(&SHA256, &data)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn identity() -> Result<String, String> {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "cannot create provider identity")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn require_cv(state: &State, id: &str) -> Result<(), String> {
    if state.conversations.contains_key(id) {
        Ok(())
    } else {
        Err("unknown durable conversation".into())
    }
}
fn active_children(state: &State, id: &str) -> bool {
    state
        .tasks
        .values()
        .any(|t| t.owner.as_deref() == Some(id) && !t.background && !t.terminal())
}
fn abort_tree(state: &mut State, id: &str, background: bool) -> Result<(), String> {
    let children: Vec<_> = state
        .tasks
        .values()
        .filter(|t| {
            t.owner.as_deref() == Some(id) && (!t.background || background) && !t.terminal()
        })
        .map(|t| t.id.clone())
        .collect();
    for child in children {
        abort_tree(state, &child, background)?;
    }
    let children_active = active_children(state, id);
    let t = state.tasks.get_mut(id).ok_or("unknown task")?;
    if !t.terminal() && !t.abort_requested {
        t.abort_requested = true;
        t.revision += 1;
        if t.status != "running" {
            t.status = if children_active {
                "completing"
            } else {
                "aborted"
            }
            .into();
            t.outcome = Some(json!({"status":"aborted"}));
        }
    }
    Ok(())
}
fn reaches(state: &State, from: &str, target: &str, seen: &mut BTreeSet<String>) -> bool {
    if from == target {
        return true;
    }
    if !seen.insert(from.into()) {
        return false;
    }
    state
        .tasks
        .get(from)
        .is_some_and(|task| task.on.iter().any(|id| reaches(state, id, target, seen)))
}
fn apply(state: &mut State, op: &Op, sequence: u64) -> Result<(), String> {
    match op {
        Op::ConversationCreate {
            id,
            agent,
            provider_identity,
            owner,
            parent,
            fork_at,
        } => {
            if !valid(id) || state.conversations.contains_key(id) {
                return Err("invalid or duplicate conversation ID".into());
            }
            if provider_identity.len() != 32
                || !provider_identity.bytes().all(|b| b.is_ascii_hexdigit())
                || state
                    .conversations
                    .values()
                    .any(|cv| cv.provider_identity == *provider_identity)
            {
                return Err("invalid or reused provider identity".into());
            }
            if let Some(owner) = owner
                && !state.tasks.contains_key(owner) {
                    return Err("unknown owner task".into());
                }
            if let Some(parent) = parent {
                require_cv(state, parent)?;
                if fork_at.is_none_or(|at| at > state.sequence) {
                    return Err("fork requires an existing commit boundary".into());
                }
                let mut depth = 0;
                let mut ancestor = Some(parent.as_str());
                while let Some(id) = ancestor {
                    depth += 1;
                    if depth > 32 {
                        return Err("fork ancestry exceeds 32".into());
                    }
                    ancestor = state
                        .conversations
                        .get(id)
                        .and_then(|cv| cv.parent.as_deref());
                }
            }
            state.conversations.insert(
                id.clone(),
                Conversation {
                    id: id.clone(),
                    agent: agent.clone(),
                    provider_identity: provider_identity.clone(),
                    owner: owner.clone(),
                    parent: parent.clone(),
                    fork_at: *fork_at,
                    entries: 0,
                    epoch: 0,
                },
            );
        }
        Op::Configure {
            conversation,
            agent,
        } => {
            let cv = state
                .conversations
                .get_mut(conversation)
                .ok_or("unknown conversation")?;
            cv.agent = agent.clone();
        }
        Op::Entry {
            conversation,
            record,
        }
        | Op::Reset {
            conversation,
            record,
        } => {
            if !record.is_object() {
                return Err("entry must be an object".into());
            }
            let cv = state
                .conversations
                .get_mut(conversation)
                .ok_or("unknown conversation")?;
            cv.entries += 1;
            if matches!(op, Op::Reset { .. }) {
                cv.epoch = sequence;
            }
        }
        Op::Document {
            conversation,
            kind,
            version,
            expected_revision,
            value,
            history,
            fork,
            initial,
        } => {
            require_cv(state, conversation)?;
            if !valid_kind(kind)
                || *version == 0
                || !matches!(fork.as_str(), "initial" | "current" | "as_of")
            {
                return Err("invalid document definition".into());
            }
            let key = doc_key(conversation, kind);
            let old = state.documents.get(&key);
            if old.map_or(0, |d| d.revision) != *expected_revision {
                return Err("document revision conflict".into());
            }
            if old
                .is_some_and(|d| d.version != *version || d.history != *history || d.fork != *fork)
            {
                return Err("document definition mismatch; migrate explicitly".into());
            }
            state.documents.insert(
                key,
                Document {
                    conversation: conversation.clone(),
                    kind: kind.clone(),
                    version: *version,
                    revision: expected_revision + 1,
                    value: value.clone(),
                    history: *history,
                    fork: fork.clone(),
                    initial: old.map_or(initial.clone(), |d| d.initial.clone()),
                },
            );
        }
        Op::TaskCreate {
            id,
            conversation,
            kind,
            version,
            owner,
            background,
            replay,
            input,
            checkpoint,
        } => {
            require_cv(state, conversation)?;
            if !valid(id)
                || !valid_kind(kind)
                || *version == 0
                || state.tasks.contains_key(id)
                || !matches!(replay.as_str(), "safe" | "unsafe")
            {
                return Err("invalid or duplicate task definition".into());
            }
            if let Some(owner) = owner {
                let parent = state.tasks.get(owner).ok_or("unknown owner task")?;
                if parent.terminal() || parent.abort_requested {
                    return Err("owner task has finished or is aborting".into());
                }
            }
            state.tasks.insert(
                id.clone(),
                Task {
                    id: id.clone(),
                    conversation: conversation.clone(),
                    kind: kind.clone(),
                    version: *version,
                    owner: owner.clone(),
                    background: *background,
                    created_sequence: sequence,
                    replay: replay.clone(),
                    input_digest: input_digest(input),
                    input: input.clone(),
                    checkpoint: checkpoint.clone(),
                    status: "pending".into(),
                    revision: 1,
                    on: Vec::new(),
                    policy: all_settled(),
                    due_at_ms: 0,
                    outcome: None,
                    abort_requested: false,
                },
            );
        }
        Op::TaskUpdate {
            id,
            expected_revision,
            status,
            checkpoint,
            on,
            policy,
            due_at_ms,
            outcome,
        } => {
            let task = state.tasks.get(id).ok_or("unknown task")?;
            if task.revision != *expected_revision {
                return Err("task revision conflict".into());
            }
            if task.terminal() {
                return Err("terminal tasks are immutable".into());
            }
            if task.status == "completing" {
                return Err("task outcome is already decided; wait for owned work".into());
            }
            if task.abort_requested && !matches!(status.as_str(),"aborted"|"failed") {
                return Err("task abort must be acknowledged".into());
            }
            if !matches!(
                status.as_str(),
                "pending" | "running" | "waiting" | "completed" | "failed" | "aborted"
            ) || !matches!(policy.as_str(), "all_settled" | "fail_fast")
            {
                return Err("invalid task transition".into());
            }
            if status == "running"
                && (!matches!(task.status.as_str(), "pending" | "running") || task.abort_requested)
            {
                return Err("only a ready pending task may run".into());
            }
            if status == "waiting" {
                if on.is_empty() || on.len() > 256 {
                    return Err("waiting needs 1..256 task IDs".into());
                }
                for child in on {
                    let dependency = state.tasks.get(child).ok_or("unknown wait dependency")?;
                    if reaches(state, child, id, &mut BTreeSet::new()) {
                        return Err("task wait cycle".into());
                    }
                    if policy == "fail_fast" && dependency.owner.as_deref() != Some(id) {
                        return Err("fail_fast requires owned children".into());
                    }
                }
            }
            if status == "failed" || status == "aborted" {
                abort_tree(state, id, false)?;
            }
            let completing = matches!(status.as_str(), "completed" | "failed" | "aborted")
                && active_children(state, id);
            let task = state.tasks.get_mut(id).unwrap();
            task.status = if completing {
                "completing".into()
            } else {
                status.clone()
            };
            task.checkpoint = checkpoint.clone();
            task.on = on.clone();
            task.policy = policy.clone();
            task.due_at_ms = *due_at_ms;
            task.outcome = if completing {
                Some(json!({"status":status,"result":outcome}))
            } else {
                outcome.clone()
            };
            task.revision += 1;
        }
        Op::Consume { id } => {
            let task = state.tasks.get_mut(id).ok_or("unknown task")?;
            if task.kind != "gc.generation"
                || task.status != "running"
                || task.checkpoint["phase"] != "executing"
            {
                return Err("only an executing generation can consume its admitted input".into());
            }
            task.input = Value::Null;
        }
        Op::TaskMigrate {
            id,
            expected_revision,
            version,
            checkpoint,
        } => {
            let task = state.tasks.get_mut(id).ok_or("unknown task")?;
            if task.revision != *expected_revision
                || task.status != "pending"
                || *version <= task.version
            {
                return Err(
                    "migration needs a pending task, matching revision and newer version".into(),
                );
            }
            task.version = *version;
            task.checkpoint = checkpoint.clone();
            task.revision += 1;
        }
        Op::DocumentMigrate {
            conversation,
            kind,
            expected_revision,
            version,
            value,
        } => {
            let document = state
                .documents
                .get_mut(&doc_key(conversation, kind))
                .ok_or("unknown document")?;
            if document.revision != *expected_revision || *version <= document.version {
                return Err("migration needs matching revision and newer version".into());
            }
            document.version = *version;
            document.value = value.clone();
            document.revision += 1;
        }
        Op::Submit {
            conversation,
            request_id,
            task_id,
            input,
        } => {
            require_cv(state, conversation)?;
            if !valid_request(request_id) {
                return Err("invalid request ID".into());
            }
            let key = request_key(conversation, request_id);
            if state.submissions.contains_key(&key) {
                return Err("duplicate submission must be reacquired, not committed again".into());
            }
            apply(
                state,
                &Op::TaskCreate {
                    id: task_id.clone(),
                    conversation: conversation.clone(),
                    kind: "gc.generation".into(),
                    version: 1,
                    owner: state.conversations[conversation]
                        .owner
                        .clone()
                        .filter(|id| {
                            state
                                .tasks
                                .get(id)
                                .is_some_and(|t| !t.terminal() && !t.abort_requested)
                        }),
                    background: false,
                    replay: "safe".into(),
                    input: input.clone(),
                    checkpoint: json!({"phase":"admitted"}),
                },
                sequence,
            )?;
            state.submissions.insert(key, task_id.clone());
        }
        Op::Abort {
            id,
            include_background,
        } => abort_tree(state, id, *include_background)?,
    }
    if state.conversations.len()
        + state.documents.len()
        + state.tasks.len()
        + state.submissions.len()
        > OBJECTS
    {
        return Err("durable object limit reached; archive terminal work".into());
    }
    Ok(())
}
fn reconcile(state: &mut State) -> Result<(), String> {
    for _ in 0..=state.tasks.len() {
        let mut changed = false;
        let tasks: Vec<_> = state
            .tasks
            .values()
            .filter(|t| matches!(t.status.as_str(), "waiting" | "completing"))
            .cloned()
            .collect();
        for task in tasks {
            if task.status == "completing" && !active_children(state, &task.id) {
                let target = state.tasks.get_mut(&task.id).unwrap();
                target.status = task
                    .outcome
                    .as_ref()
                    .and_then(|v| v["status"].as_str())
                    .unwrap_or("completed")
                    .into();
                target.revision += 1;
                changed = true;
            } else if task.status == "waiting" {
                let failed = task.on.iter().any(|id| {
                    state
                        .tasks
                        .get(id)
                        .is_some_and(|t| matches!(t.status.as_str(), "failed" | "aborted"))
                });
                if task.policy == "fail_fast" && failed {
                    for child in &task.on {
                        abort_tree(state, child, false)?;
                    }
                }
                if task
                    .on
                    .iter()
                    .all(|id| state.tasks.get(id).is_some_and(Task::terminal))
                {
                    let target = state.tasks.get_mut(&task.id).unwrap();
                    target.status = "pending".into();
                    target.revision += 1;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(());
        }
    }
    Err("task reconciliation did not settle".into())
}
impl Store {
    pub fn open(dir: &Path, fsync: bool) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let open = |path: &Path| {
            OpenOptions::new()
                .create(true)
                .read(true)
                .append(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(path)
                .map_err(|e| e.to_string())
        };
        let lock = open(&dir.join("durable.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|_| "durable storage already has a writer")?;
        let path = dir.join("durable.jsonl");
        let file = open(&path)?;
        let meta = file.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.nlink() != 1 {
            return Err("durable journal must be a private regular file".into());
        }
        let mut store = Self {
            state: State::default(),
            path,
            file,
            _lock: lock,
            last_hash: String::new(),
            poisoned: false,
            fsync,
        };
        let mut reader = BufReader::new(File::open(&store.path).map_err(|e| e.to_string())?);
        let mut offset = 0;
        loop {
            let mut line = Vec::new();
            let n = reader
                .by_ref()
                .take((FRAME + 1) as u64)
                .read_until(b'\n', &mut line)
                .map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if n > FRAME {
                return Err("durable commit exceeds frame limit".into());
            }
            if !line.ends_with(b"\n") {
                store.file.set_len(offset).map_err(|e| e.to_string())?;
                if fsync {
                    store.file.sync_data().map_err(|e| e.to_string())?;
                }
                break;
            }
            let commit: Commit = serde_json::from_slice(&line)
                .map_err(|e| format!("corrupt durable commit at {offset}: {e}"))?;
            if commit.sequence != store.state.sequence + 1
                || commit.previous != store.last_hash
                || commit.hash != hash(commit.sequence, &commit.previous, &commit.ops)?
            {
                return Err(format!("durable commit integrity failure at {offset}"));
            }
            for op in &commit.ops {
                apply(&mut store.state, op, commit.sequence)?;
            }
            reconcile(&mut store.state)?;
            store.state.sequence = commit.sequence;
            store.last_hash = commit.hash;
            offset += n as u64;
        }
        Ok(store)
    }
    pub fn commit(&mut self, expected: u64, ops: Vec<Op>) -> Result<u64, String> {
        if self.poisoned {
            return Err("durable storage poisoned; reopen before writing".into());
        }
        if expected != self.state.sequence {
            return Err("durable sequence conflict".into());
        }
        if ops.is_empty() || ops.len() > 512 {
            return Err("commit requires 1..512 operations".into());
        }
        let sequence = expected
            .checked_add(1)
            .ok_or("durable sequence exhausted")?;
        let mut next = self.state.clone();
        for op in &ops {
            apply(&mut next, op, sequence)?;
        }
        reconcile(&mut next)?;
        next.sequence = sequence;
        if serde_json::to_vec(&next).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("durable resident state exceeds 32 MiB; archive terminal work".into());
        }
        let checksum = hash(sequence, &self.last_hash, &ops)?;
        let frame = Commit {
            sequence,
            previous: self.last_hash.clone(),
            ops,
            hash: checksum.clone(),
        };
        let mut bytes = serde_json::to_vec(&frame).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > FRAME {
            return Err("durable commit exceeds 1 MiB".into());
        }
        if let Err(error) = self
            .file
            .write_all(&bytes)
            .and_then(|_| self.file.flush())
            .and_then(|_| {
                if self.fsync {
                    self.file.sync_data()
                } else {
                    Ok(())
                }
            })
        {
            self.poisoned = true;
            return Err(format!("durable append uncertain; reopen: {error}"));
        }
        self.state = next;
        self.last_hash = checksum;
        Ok(sequence)
    }
    pub fn submission(&self, conversation: &str, request: &str) -> Option<&Task> {
        self.state
            .submissions
            .get(&request_key(conversation, request))
            .and_then(|id| self.state.tasks.get(id))
    }
    pub fn recover(&mut self) -> Result<(), String> {
        let mut running: Vec<_> = self
            .state
            .tasks
            .values()
            .filter(|t| t.status == "running")
            .collect();
        running.sort_by_key(|task| {
            let mut depth = 0;
            let mut parent = task.owner.as_ref();
            while let Some(id) = parent {
                depth += 1;
                parent = self.state.tasks.get(id).and_then(|p| p.owner.as_ref());
            }
            std::cmp::Reverse(depth)
        });
        let ops:Vec<_>=running.into_iter().map(|t|Op::TaskUpdate{id:t.id.clone(),expected_revision:t.revision,status:if t.abort_requested{"aborted"}else if t.replay=="safe"{"pending"}else{"failed"}.into(),checkpoint:t.checkpoint.clone(),on:Vec::new(),policy:all_settled(),due_at_ms:t.due_at_ms,outcome:if t.abort_requested{Some(json!({"status":"aborted"}))}else if t.replay=="unsafe"{Some(json!({"status":"failed","kind":"interrupted","message":"External effect may have completed; reconcile before retrying."}))}else{None}}).collect();
        if !ops.is_empty() {
            self.commit(self.state.sequence, ops)?;
        }
        Ok(())
    }
    pub fn changes(&self, since: u64, limit: usize) -> Result<Value, String> {
        if since > self.state.sequence {
            return Err("cursor exceeds durable sequence".into());
        }
        let mut commits = Vec::new();
        let mut cursor = since;
        let mut bytes = 0;
        for line in BufReader::new(File::open(&self.path).map_err(|e| e.to_string())?).lines() {
            let line = line.map_err(|e| e.to_string())?;
            let commit: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            if commit["sequence"].as_u64().unwrap_or(0) <= since {
                continue;
            }
            if bytes + line.len() > 768 * 1024 {
                if commits.is_empty() {
                    return Ok(
                        json!({"cursor":self.state.sequence,"commits":[],"reset":true,"reason":"large commit; reacquire paginated state"}),
                    );
                }
                break;
            }
            if commits.len() >= limit.clamp(1, 100) {
                break;
            }
            cursor = commit["sequence"]
                .as_u64()
                .ok_or("missing commit sequence")?;
            bytes += line.len();
            commits.push(commit);
        }
        Ok(json!({"cursor":cursor,"commits":commits,"hasMore":cursor<self.state.sequence}))
    }
    pub fn records(&self, conversation: &str) -> Result<Vec<crate::session::Record>, String> {
        self.records_at(conversation, self.state.sequence)
    }
    pub fn records_at(
        &self,
        conversation: &str,
        through: u64,
    ) -> Result<Vec<crate::session::Record>, String> {
        require_cv(&self.state, conversation)?;
        let cv = &self.state.conversations[conversation];
        let mut result = if let Some(parent) = &cv.parent {
            self.records_at(
                parent,
                cv.fork_at.ok_or("missing fork boundary")?.min(through),
            )?
        } else {
            Vec::new()
        };
        for line in BufReader::new(File::open(&self.path).map_err(|e| e.to_string())?).lines() {
            let commit: Commit = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if commit.sequence > through {
                break;
            }
            for op in commit.ops {
                let reset = matches!(op, Op::Reset { .. });
                if let Op::Entry {
                    conversation: cv,
                    record,
                }
                | Op::Reset {
                    conversation: cv,
                    record,
                } = op
                    && cv == conversation {
                        if reset {
                            result.clear();
                        }
                        result.push(crate::session::Record::from_json(&record).unwrap_or_else(
                            || crate::session::Record::Custom {
                                kind: "durable/entry".into(),
                                data: Some(record.to_string()),
                            },
                        ));
                    }
            }
        }
        Ok(result)
    }
    pub fn snapshot_at(&self, sequence: u64) -> Result<State, String> {
        if sequence > self.state.sequence {
            return Err("snapshot sequence is in the future".into());
        }
        let mut state = State::default();
        for line in BufReader::new(File::open(&self.path).map_err(|e| e.to_string())?).lines() {
            let commit: Commit = serde_json::from_str(&line.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            if commit.sequence > sequence {
                break;
            }
            for op in &commit.ops {
                apply(&mut state, op, commit.sequence)?;
            }
            reconcile(&mut state)?;
            state.sequence = commit.sequence;
        }
        Ok(state)
    }
    pub fn document_as_of(
        &self,
        conversation: &str,
        kind: &str,
        sequence: u64,
    ) -> Result<Option<Document>, String> {
        if sequence > self.state.sequence {
            return Err("snapshot sequence is in the future".into());
        }
        let key = doc_key(conversation, kind);
        if !self.state.documents.get(&key).is_some_and(|d| d.history) {
            return Err("document history is not enabled".into());
        }
        let mut state = self.snapshot_at(sequence)?;
        Ok(state.documents.remove(&key))
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn poison(&mut self) {
        self.poisoned = true;
    }
    pub fn task_graph(&self) -> Value {
        json!({"sequence":self.state.sequence,"tasks":self.state.tasks.values().filter(|t|!t.terminal()).collect::<Vec<_>>()})
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._lock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("gc-durable-test-{}", identity().unwrap()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn create(id: &str) -> Op {
        Op::ConversationCreate {
            id: id.into(),
            agent: json!({}),
            provider_identity: identity().unwrap(),
            owner: None,
            parent: None,
            fork_at: None,
        }
    }
    fn task(id: &str, owner: Option<&str>, safe: bool, background: bool) -> Op {
        Op::TaskCreate {
            id: id.into(),
            conversation: "root".into(),
            kind: "app.step".into(),
            version: 1,
            owner: owner.map(str::to_owned),
            background,
            replay: if safe { "safe" } else { "unsafe" }.into(),
            input: json!({}),
            checkpoint: json!({"phase":"start"}),
        }
    }
    fn transition(store: &Store, id: &str, status: &str) -> Op {
        let t = &store.state.tasks[id];
        Op::TaskUpdate {
            id: id.into(),
            expected_revision: t.revision,
            status: status.into(),
            checkpoint: json!({"phase":"next"}),
            on: Vec::new(),
            policy: "all_settled".into(),
            due_at_ms: 0,
            outcome: None,
        }
    }
    fn transact(store: &mut Store, ops: Vec<Op>) {
        store.commit(store.state.sequence, ops).unwrap();
    }
    #[test]
    fn consumed_submissions_keep_deduplication_receipts_after_reopen() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        let input = json!({"text":"hello"});
        transact(
            &mut store,
            vec![
                create("root"),
                Op::Submit {
                    conversation: "root".into(),
                    request_id: "request:1".into(),
                    task_id: "generation".into(),
                    input: input.clone(),
                },
            ],
        );
        let mut op = transition(&store, "generation", "running");
        if let Op::TaskUpdate { checkpoint, .. } = &mut op {
            *checkpoint = json!({"phase":"executing"});
        }
        transact(
            &mut store,
            vec![
                Op::Entry {
                    conversation: "root".into(),
                    record: json!({"k":"user","t":"hello"}),
                },
                op,
                Op::Consume {
                    id: "generation".into(),
                },
            ],
        );
        drop(store);
        let store = Store::open(&path, false).unwrap();
        let task = store.submission("root", "request:1").unwrap();
        assert!(task.input.is_null());
        assert_eq!(task.input_digest, input_digest(&input));
        assert_eq!(store.records("root").unwrap().len(), 1);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn aborted_running_work_stays_aborted_after_restart() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![create("root"), task("step", None, true, false)],
        );
        let op = transition(&store, "step", "running");
        transact(
            &mut store,
            vec![
                op,
                Op::Abort {
                    id: "step".into(),
                    include_background: false,
                },
            ],
        );
        drop(store);
        let mut store = Store::open(&path, false).unwrap();
        store.recover().unwrap();
        assert_eq!(store.state.tasks["step"].status, "aborted");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn explicit_migrations_and_reset_preserve_affinity() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![
                create("root"),
                task("step", None, true, false),
                Op::Entry {
                    conversation: "root".into(),
                    record: json!({"k":"user","t":"old"}),
                },
            ],
        );
        let identity = store.state.conversations["root"].provider_identity.clone();
        let before = store.state.sequence;
        transact(
            &mut store,
            vec![
                Op::TaskMigrate {
                    id: "step".into(),
                    expected_revision: 1,
                    version: 2,
                    checkpoint: json!({"phase":"v2"}),
                },
                Op::Reset {
                    conversation: "root".into(),
                    record: json!({"k":"meta","t":"reset"}),
                },
            ],
        );
        assert_eq!(store.state.tasks["step"].version, 2);
        assert_eq!(
            store.state.conversations["root"].provider_identity,
            identity
        );
        assert_eq!(store.records("root").unwrap().len(), 1);
        assert_eq!(store.records_at("root", before).unwrap().len(), 1);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn atomic_commit_reopens_with_identical_identity_and_state() {
        let path = dir();
        let mut store = Store::open(&path, true).unwrap();
        transact(&mut store, vec![create("root")]);
        let identity = store.state.conversations["root"].provider_identity.clone();
        let before = store.file.metadata().unwrap().len();
        let seq = store.state.sequence;
        assert!(
            store
                .commit(
                    seq,
                    vec![
                        Op::Entry {
                            conversation: "root".into(),
                            record: json!({"k":"user","t":"atomic"})
                        },
                        task("bad", Some("missing"), true, false)
                    ]
                )
                .is_err()
        );
        assert_eq!(store.file.metadata().unwrap().len(), before);
        assert_eq!(store.state.conversations["root"].entries, 0);
        transact(
            &mut store,
            vec![
                Op::Entry {
                    conversation: "root".into(),
                    record: json!({"k":"user","t":"atomic"}),
                },
                task("step", None, true, false),
            ],
        );
        drop(store);
        let store = Store::open(&path, true).unwrap();
        assert_eq!(
            store.state.conversations["root"].provider_identity,
            identity
        );
        assert_eq!(store.records("root").unwrap().len(), 1);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn torn_tail_is_repaired_but_committed_corruption_fails_closed() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(&mut store, vec![create("root")]);
        let before = store.file.metadata().unwrap().len();
        drop(store);
        OpenOptions::new()
            .append(true)
            .open(path.join("durable.jsonl"))
            .unwrap()
            .write_all(b"{\"torn\":")
            .unwrap();
        let store = Store::open(&path, false).unwrap();
        assert_eq!(store.file.metadata().unwrap().len(), before);
        drop(store);
        OpenOptions::new()
            .append(true)
            .open(path.join("durable.jsonl"))
            .unwrap()
            .write_all(b"{}\n")
            .unwrap();
        assert!(Store::open(&path, false).is_err());
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn one_writer_and_uncertain_append_poisoning() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        assert!(Store::open(&path, false).is_err());
        store.file = OpenOptions::new().write(true).open("/dev/full").unwrap();
        assert!(store.commit(0, vec![create("root")]).is_err());
        assert!(store.is_poisoned());
        assert_eq!(store.state.sequence, 0);
        assert!(store.commit(0, vec![create("root")]).is_err());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn safe_tasks_resume_and_unsafe_tasks_do_not_reexecute() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![
                create("root"),
                task("safe", None, true, false),
                task("unsafe", None, false, false),
            ],
        );
        let ops = vec![
            transition(&store, "safe", "running"),
            transition(&store, "unsafe", "running"),
        ];
        transact(&mut store, ops);
        drop(store);
        let mut store = Store::open(&path, false).unwrap();
        store.recover().unwrap();
        assert_eq!(store.state.tasks["safe"].status, "pending");
        assert_eq!(store.state.tasks["unsafe"].status, "failed");
        let seq = store.state.sequence;
        store.recover().unwrap();
        assert_eq!(store.state.sequence, seq);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn ownership_waits_and_background_boundaries() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![
                create("root"),
                task("parent", None, true, false),
                task("child", Some("parent"), true, false),
                task("background", Some("parent"), true, true),
            ],
        );
        let op = transition(&store, "parent", "completed");
        transact(&mut store, vec![op]);
        assert_eq!(store.state.tasks["parent"].status, "completing");
        let op = transition(&store, "child", "completed");
        transact(&mut store, vec![op]);
        assert_eq!(store.state.tasks["parent"].status, "completed");
        assert_eq!(store.state.tasks["background"].status, "pending");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn abort_requires_running_child_acknowledgement() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![
                create("root"),
                task("parent", None, true, false),
                task("child", Some("parent"), true, false),
                task("background", Some("parent"), true, true),
            ],
        );
        let op = transition(&store, "child", "running");
        transact(&mut store, vec![op]);
        transact(
            &mut store,
            vec![Op::Abort {
                id: "parent".into(),
                include_background: false,
            }],
        );
        assert_eq!(store.state.tasks["parent"].status, "completing");
        assert!(store.state.tasks["child"].abort_requested);
        assert_eq!(store.state.tasks["background"].status, "pending");
        let op = transition(&store, "child", "aborted");
        transact(&mut store, vec![op]);
        assert_eq!(store.state.tasks["parent"].status, "aborted");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn document_revisions_history_and_invalid_paths() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        assert!(store.commit(0, vec![create("../escape")]).is_err());
        transact(&mut store, vec![create("root")]);
        let put = |revision, value| Op::Document {
            conversation: "root".into(),
            kind: "app.todo".into(),
            version: 1,
            expected_revision: revision,
            value: json!({"count":value}),
            history: true,
            fork: "as_of".into(),
            initial: json!({"count":0}),
        };
        transact(&mut store, vec![put(0, 1)]);
        let old = store.state.sequence;
        transact(&mut store, vec![put(1, 2)]);
        assert_eq!(
            store
                .document_as_of("root", "app.todo", old)
                .unwrap()
                .unwrap()
                .value["count"],
            1
        );
        assert!(store.commit(store.state.sequence, vec![put(0, 3)]).is_err());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn wait_dependencies_are_checked_and_fail_fast_waits_for_ack() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![
                create("root"),
                task("parent", None, true, false),
                task("a", Some("parent"), true, false),
                task("b", Some("parent"), true, false),
            ],
        );
        let mut wait = transition(&store, "parent", "waiting");
        if let Op::TaskUpdate { on, policy, .. } = &mut wait {
            *on = vec!["a".into(), "b".into()];
            *policy = "fail_fast".into();
        }
        transact(&mut store, vec![wait]);
        let op = transition(&store, "b", "running");
        transact(&mut store, vec![op]);
        let op = transition(&store, "a", "failed");
        transact(&mut store, vec![op]);
        assert_eq!(store.state.tasks["parent"].status, "waiting");
        assert!(store.state.tasks["b"].abort_requested);
        let op = transition(&store, "b", "aborted");
        transact(&mut store, vec![op]);
        assert_eq!(store.state.tasks["parent"].status, "pending");
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn inherited_transcript_stops_at_fork_boundary() {
        let path = dir();
        let mut store = Store::open(&path, false).unwrap();
        transact(
            &mut store,
            vec![
                create("root"),
                Op::Entry {
                    conversation: "root".into(),
                    record: json!({"k":"user","t":"before"}),
                },
            ],
        );
        let at = store.state.sequence;
        let mut fork = create("fork");
        if let Op::ConversationCreate {
            parent, fork_at, ..
        } = &mut fork
        {
            *parent = Some("root".into());
            *fork_at = Some(at);
        }
        transact(
            &mut store,
            vec![
                fork,
                Op::Entry {
                    conversation: "root".into(),
                    record: json!({"k":"user","t":"after"}),
                },
            ],
        );
        assert_eq!(store.records("fork").unwrap().len(), 1);
        assert_ne!(
            store.state.conversations["root"].provider_identity,
            store.state.conversations["fork"].provider_identity
        );
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
}
