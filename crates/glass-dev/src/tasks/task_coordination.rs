//! Durable task bindings, child-event inbox, and fenced wake reservations.
//!
//! This is an extension of `TaskScheduler`, not a second scheduler. All state
//! below is encoded in the scheduler checkpoint so an accepted event or wake
//! reservation is durable before its caller can acknowledge it.

use super::*;
use std::collections::{BTreeMap, VecDeque};

const COORDINATION_VERSION: u32 = 1;
const MAX_BINDINGS: usize = MAX_TASKS * 4;
const MAX_HISTORICAL_BINDINGS: usize = MAX_BINDINGS;
const MAX_EVENT_STREAMS: usize = MAX_BINDINGS;
const MAX_INBOX_EVENTS: usize = 256;
const MAX_INBOX_EVENT_BYTES: usize = 8 * 1024;
const MAX_INBOX_BYTES: usize = 512 * 1024;
const MAX_REORDERED_EVENTS_PER_STREAM: usize = 32;
const MAX_REORDER_DISTANCE: u64 = 32;
const MAX_REQUIRED_TASKS: usize = MAX_TASKS;
const MAX_WAKE_LEASE_MS: u64 = 60_000;
const MAX_COORDINATION_TEXT_BYTES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskBindingIntent {
    pub(crate) intent_id: String,
    pub(crate) workspace_id: String,
    pub(crate) core_session_id: String,
    pub(crate) todo_id: String,
    pub(crate) todo_revision: u64,
    pub(crate) plan_revision: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum BindingStatus {
    Pending,
    Resolved,
    ReconciliationRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskExecutionBinding {
    pub(crate) version: u32,
    pub(crate) intent: TaskBindingIntent,
    pub(crate) root_task_id: Option<TaskId>,
    pub(crate) task_id: Option<TaskId>,
    pub(crate) attempt: Option<u32>,
    pub(crate) child_session_id: Option<String>,
    pub(crate) parent_call_id: Option<String>,
    pub(crate) status: BindingStatus,
    pub(crate) stop_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TaskInboxEventKind {
    Progress,
    Question,
    Blocked,
    VerifiedCompletion,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskInboxEvent {
    pub(crate) event_id: String,
    pub(crate) workspace_id: String,
    pub(crate) root_task_id: TaskId,
    pub(crate) task_id: TaskId,
    pub(crate) attempt: u32,
    pub(crate) parent_session_id: String,
    pub(crate) recipient_session_id: String,
    pub(crate) child_session_id: String,
    pub(crate) parent_call_id: String,
    pub(crate) sequence: u64,
    pub(crate) kind: TaskInboxEventKind,
    /// The parent Core adapter must apply its SecretVault before constructing
    /// this event. Glass bounds the text but cannot independently verify the
    /// assertion recorded by `parent_redaction_applied`.
    pub(crate) redacted_summary: String,
    pub(crate) parent_redaction_applied: bool,
    pub(crate) observed_at_ms: u128,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum InboxAppendResult {
    Accepted,
    BufferedOutOfOrder,
    Duplicate,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskEventStreamId {
    pub(crate) root_task_id: TaskId,
    pub(crate) task_id: TaskId,
    pub(crate) attempt: u32,
    pub(crate) parent_session_id: String,
    pub(crate) parent_call_id: String,
    pub(crate) child_session_id: String,
}

impl From<TaskEventStreamId> for String {
    fn from(stream: TaskEventStreamId) -> Self {
        serde_json::to_string(&(
            stream.root_task_id,
            stream.task_id,
            stream.attempt,
            stream.parent_session_id,
            stream.parent_call_id,
            stream.child_session_id,
        ))
        .expect("task event stream identity serializes")
    }
}

impl TryFrom<String> for TaskEventStreamId {
    type Error = serde_json::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let (root_task_id, task_id, attempt, parent_session_id, parent_call_id, child_session_id) =
            serde_json::from_str(&value)?;
        Ok(Self {
            root_task_id,
            task_id,
            attempt,
            parent_session_id,
            parent_call_id,
            child_session_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskStreamCursor {
    pub(crate) next_sequence: u64,
    pub(crate) gap_detected_at_ms: Option<u128>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskCoordinationCounters {
    pub(crate) accepted_events: u64,
    pub(crate) accepted_event_bytes: u64,
    pub(crate) duplicate_events: u64,
    pub(crate) stale_events: u64,
    pub(crate) wake_reservations: u32,
    pub(crate) wake_dispatches: u32,
    pub(crate) unknown_wake_outcomes: u32,
    pub(crate) stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RootWakePolicy {
    pub(crate) version: u32,
    pub(crate) root_task_id: TaskId,
    pub(crate) owner_session_id: String,
    pub(crate) owner_turn_id: String,
    pub(crate) required_task_ids: Vec<TaskId>,
    pub(crate) max_events: u64,
    pub(crate) max_event_bytes: u64,
    pub(crate) max_owner_wakes: u32,
    pub(crate) reserved_wakes: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WakeLeaseState {
    Reserved,
    Dispatching,
    Delivered,
    Settled,
    Unknown,
    FailedBeforeDispatch,
}

impl WakeLeaseState {
    fn terminal(self) -> bool {
        matches!(self, Self::Settled | Self::FailedBeforeDispatch)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OwnerWakeLease {
    pub(crate) lease_id: String,
    pub(crate) root_task_id: TaskId,
    pub(crate) owner_session_id: String,
    pub(crate) owner_turn_id: String,
    pub(crate) event_ids: Vec<String>,
    pub(crate) inbox_watermark: u64,
    pub(crate) reserved_at_ms: u128,
    pub(crate) expires_at_ms: u128,
    pub(crate) state: WakeLeaseState,
    pub(crate) stop_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Outcomes are consumed by the later Core wake dispatcher adapter.
pub(crate) enum WakeDeliveryOutcome {
    Acknowledged,
    DefinitelyNotSent,
    Unknown,
}

/// Offline-testable boundary for the later owner-turn dispatcher. This task
/// provides no production adapter and never launches a provider.
pub(crate) trait OwnerWakeDispatchAdapter {
    fn dispatch_owner_wake(&mut self, lease: &OwnerWakeLease) -> WakeDeliveryOutcome;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CoordinationState {
    format_version: u32,
    bindings: BTreeMap<String, TaskExecutionBinding>,
    #[serde(default)]
    historical_bindings: Vec<TaskExecutionBinding>,
    inbox: VecDeque<TaskInboxEvent>,
    out_of_order: BTreeMap<TaskEventStreamId, BTreeMap<u64, TaskInboxEvent>>,
    stream_cursors: BTreeMap<TaskEventStreamId, TaskStreamCursor>,
    completion_receipts: BTreeMap<TaskId, TaskInboxEvent>,
    wake_policies: BTreeMap<TaskId, RootWakePolicy>,
    wake_leases: BTreeMap<String, OwnerWakeLease>,
    counters: BTreeMap<TaskId, TaskCoordinationCounters>,
}

impl Default for CoordinationState {
    fn default() -> Self {
        Self {
            format_version: COORDINATION_VERSION,
            bindings: BTreeMap::new(),
            historical_bindings: Vec::new(),
            inbox: VecDeque::new(),
            out_of_order: BTreeMap::new(),
            stream_cursors: BTreeMap::new(),
            completion_receipts: BTreeMap::new(),
            wake_policies: BTreeMap::new(),
            wake_leases: BTreeMap::new(),
            counters: BTreeMap::new(),
        }
    }
}

impl CoordinationState {
    pub(super) fn validate(
        &self,
        workspace: &Path,
        tasks: &BTreeMap<TaskId, TaskRecord>,
    ) -> DevelopmentResult<()> {
        if self.format_version != COORDINATION_VERSION
            || self.bindings.len() > MAX_BINDINGS
            || self.historical_bindings.len() > MAX_HISTORICAL_BINDINGS
            || self.stream_cursors.len() > MAX_EVENT_STREAMS
            || self.inbox.len() > MAX_INBOX_EVENTS
            || self.wake_policies.len() > MAX_TASKS
            || self.wake_leases.len() > MAX_TASKS
            || self.counters.len() > MAX_TASKS
        {
            return Err(DevelopmentError::InvalidInput(
                "task coordination checkpoint exceeds supported bounds".into(),
            ));
        }
        let mut inbox_bytes = 0usize;
        let mut event_ids = BTreeSet::new();
        for event in &self.inbox {
            self.validate_stored_event(event, workspace, tasks)?;
            if !event_ids.insert(event.event_id.as_str()) {
                return Err(DevelopmentError::InvalidInput(
                    "task event inbox repeats an event ID".into(),
                ));
            }
            inbox_bytes = inbox_bytes.saturating_add(encoded_event_len(event)?);
        }
        let mut pending_count = 0usize;
        for (stream, pending) in &self.out_of_order {
            validate_stream_id(stream)?;
            if pending.len() > MAX_REORDERED_EVENTS_PER_STREAM {
                return Err(DevelopmentError::InvalidInput(
                    "task event reorder buffer exceeds supported bounds".into(),
                ));
            }
            pending_count = pending_count.saturating_add(pending.len());
            let cursor = self.stream_cursors.get(stream).ok_or_else(|| {
                DevelopmentError::InvalidInput(
                    "task event reorder buffer has no stream cursor".into(),
                )
            })?;
            for (sequence, event) in pending {
                self.validate_stored_event(event, workspace, tasks)?;
                if !event_ids.insert(event.event_id.as_str())
                    || event.sequence != *sequence
                    || *sequence <= cursor.next_sequence
                    || (*sequence).saturating_sub(cursor.next_sequence) > MAX_REORDER_DISTANCE
                    || event_stream_id(event) != *stream
                {
                    return Err(DevelopmentError::InvalidInput(
                        "task event reorder entry has a mismatched identity".into(),
                    ));
                }
                inbox_bytes = inbox_bytes.saturating_add(encoded_event_len(event)?);
            }
        }
        for (stream, cursor) in &self.stream_cursors {
            validate_stream_id(stream)?;
            if cursor.next_sequence == 0
                || !tasks.contains_key(&stream.root_task_id)
                || tasks
                    .get(&stream.task_id)
                    .is_none_or(|task| stream.attempt > task.snapshot.attempt)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task event stream cursor is invalid".into(),
                ));
            }
        }
        if self.inbox.len().saturating_add(pending_count) > MAX_INBOX_EVENTS
            || inbox_bytes > MAX_INBOX_BYTES
        {
            return Err(DevelopmentError::InvalidInput(
                "task event inbox exceeds supported bounds".into(),
            ));
        }
        for (intent_id, binding) in &self.bindings {
            validate_binding(&binding.intent, workspace)?;
            if binding.version != 1 || intent_id != &binding.intent.intent_id {
                return Err(DevelopmentError::InvalidInput(
                    "task binding key does not match its intent".into(),
                ));
            }
            if let Some(task_id) = &binding.task_id
                && !tasks.contains_key(task_id)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task binding references a missing task".into(),
                ));
            }
            if let Some(root_task_id) = &binding.root_task_id
                && !tasks.contains_key(root_task_id)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task binding references a missing root task".into(),
                ));
            }
            if binding.status == BindingStatus::Resolved
                && (binding.root_task_id.is_none()
                    || binding.task_id.is_none()
                    || binding.attempt.is_none()
                    || binding.child_session_id.is_none()
                    || binding.parent_call_id.is_none())
            {
                return Err(DevelopmentError::InvalidInput(
                    "resolved task binding is incomplete".into(),
                ));
            }
        }
        for binding in &self.historical_bindings {
            validate_binding(&binding.intent, workspace)?;
            if binding.version != 1
                || binding.status != BindingStatus::Resolved
                || binding.root_task_id.is_none()
                || binding.task_id.is_none()
                || binding.attempt.is_none()
                || binding.child_session_id.is_none()
                || binding.parent_call_id.is_none()
                || binding
                    .task_id
                    .as_ref()
                    .is_none_or(|task_id| !tasks.contains_key(task_id))
                || binding
                    .root_task_id
                    .as_ref()
                    .is_none_or(|root_task_id| !tasks.contains_key(root_task_id))
            {
                return Err(DevelopmentError::InvalidInput(
                    "historical task binding is invalid or incomplete".into(),
                ));
            }
        }
        let mut resolved_child_sessions = BTreeSet::new();
        let mut resolved_task_attempts = BTreeSet::new();
        for binding in self
            .bindings
            .values()
            .chain(self.historical_bindings.iter())
        {
            if binding.status != BindingStatus::Resolved {
                continue;
            }
            let task_id = binding.task_id.as_ref().expect("resolved binding checked");
            let root_task_id = binding
                .root_task_id
                .as_ref()
                .expect("resolved binding checked");
            let attempt = binding.attempt.expect("resolved binding checked");
            let child_session = binding
                .child_session_id
                .as_ref()
                .expect("resolved binding checked");
            if tasks[task_id].snapshot.attempt < attempt
                || !task_is_descendant(root_task_id, task_id, tasks)
                || !resolved_child_sessions.insert(child_session)
                || !resolved_task_attempts.insert((task_id, attempt))
            {
                return Err(DevelopmentError::InvalidInput(
                    "resolved task binding identities overlap or exceed the scheduler attempt"
                        .into(),
                ));
            }
        }
        for (root_task_id, policy) in &self.wake_policies {
            validate_wake_policy(policy, tasks)?;
            if root_task_id != &policy.root_task_id {
                return Err(DevelopmentError::InvalidInput(
                    "task wake policy key does not match its root".into(),
                ));
            }
            if self
                .bindings
                .values()
                .chain(self.historical_bindings.iter())
                .any(|binding| {
                    binding.status == BindingStatus::Resolved
                        && binding.root_task_id.as_ref() == Some(root_task_id)
                        && binding
                            .task_id
                            .as_ref()
                            .is_some_and(|task_id| policy.required_task_ids.contains(task_id))
                        && binding.intent.core_session_id != policy.owner_session_id
                })
            {
                return Err(DevelopmentError::InvalidInput(
                    "resolved required-task binding does not match its root wake owner".into(),
                ));
            }
            let lease_count = self
                .wake_leases
                .values()
                .filter(|lease| &lease.root_task_id == root_task_id)
                .count();
            if lease_count != policy.reserved_wakes as usize {
                return Err(DevelopmentError::InvalidInput(
                    "task wake reservation count does not match persisted leases".into(),
                ));
            }
            if policy.reserved_wakes > 0
                && self
                    .counters
                    .get(root_task_id)
                    .is_none_or(|counters| counters.wake_reservations != policy.reserved_wakes)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task wake counters do not match the persisted policy".into(),
                ));
            }
        }
        for (lease_id, lease) in &self.wake_leases {
            validate_wake_lease(lease)?;
            if lease_id != &lease.lease_id || !tasks.contains_key(&lease.root_task_id) {
                return Err(DevelopmentError::InvalidInput(
                    "task wake lease identity is invalid".into(),
                ));
            }
            let policy = self.wake_policies.get(&lease.root_task_id).ok_or_else(|| {
                DevelopmentError::InvalidInput("owner wake lease has no root policy".into())
            })?;
            if lease.owner_session_id != policy.owner_session_id
                || lease.owner_turn_id != policy.owner_turn_id
            {
                return Err(DevelopmentError::InvalidInput(
                    "owner wake lease does not match its root policy".into(),
                ));
            }
            if !lease.state.terminal()
                && (lease.event_ids.len() != policy.required_task_ids.len()
                    || policy.required_task_ids.iter().any(|task_id| {
                        self.completion_receipts.get(task_id).is_none_or(|receipt| {
                            receipt.root_task_id != lease.root_task_id
                                || !lease.event_ids.contains(&receipt.event_id)
                                || tasks[task_id].snapshot.attempt != receipt.attempt
                        })
                    }))
            {
                return Err(DevelopmentError::InvalidInput(
                    "active owner wake lease does not match required completion receipts".into(),
                ));
            }
        }
        for (root_task_id, counters) in &self.counters {
            if !tasks.contains_key(root_task_id)
                || counters
                    .stop_reason
                    .as_deref()
                    .is_some_and(|reason| reason.len() > MAX_COORDINATION_TEXT_BYTES)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task coordination counter identity is invalid".into(),
                ));
            }
            if let Some(policy) = self.wake_policies.get(root_task_id)
                && (counters.accepted_events > policy.max_events
                    || counters.accepted_event_bytes > policy.max_event_bytes
                    || counters.wake_reservations != policy.reserved_wakes
                    || counters.wake_dispatches > counters.wake_reservations
                    || counters.unknown_wake_outcomes > counters.wake_reservations)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task coordination counters exceed the root wake policy".into(),
                ));
            }
            if !self.wake_policies.contains_key(root_task_id) {
                return Err(DevelopmentError::InvalidInput(
                    "task coordination counters have no root wake policy".into(),
                ));
            }
        }
        for (task_id, event) in &self.completion_receipts {
            self.validate_stored_event(event, workspace, tasks)?;
            if event.kind != TaskInboxEventKind::VerifiedCompletion
                || event.task_id != *task_id
                || !tasks.contains_key(task_id)
            {
                return Err(DevelopmentError::InvalidInput(
                    "task completion receipt identity is invalid".into(),
                ));
            }
        }
        Ok(())
    }

    fn validate_stored_event(
        &self,
        event: &TaskInboxEvent,
        workspace: &Path,
        tasks: &BTreeMap<TaskId, TaskRecord>,
    ) -> DevelopmentResult<()> {
        validate_event(event, workspace)?;
        let task = tasks.get(&event.task_id).ok_or_else(|| {
            DevelopmentError::InvalidInput("task event references a missing task".into())
        })?;
        if !tasks.contains_key(&event.root_task_id) || event.attempt > task.snapshot.attempt {
            return Err(DevelopmentError::InvalidInput(
                "task event references an invalid root or future attempt".into(),
            ));
        }
        let policy = self.wake_policies.get(&event.root_task_id).ok_or_else(|| {
            DevelopmentError::InvalidInput("task event has no persisted root policy".into())
        })?;
        if !policy.required_task_ids.contains(&event.task_id) {
            return Err(DevelopmentError::InvalidInput(
                "task event is not in the persisted root requirement set".into(),
            ));
        }
        if policy.owner_session_id != event.parent_session_id {
            return Err(DevelopmentError::InvalidInput(
                "task event parent session does not match the persisted root owner".into(),
            ));
        }
        if !self
            .bindings
            .values()
            .chain(self.historical_bindings.iter())
            .any(|binding| {
                binding.status == BindingStatus::Resolved
                    && binding.root_task_id.as_ref() == Some(&event.root_task_id)
                    && binding.task_id.as_ref() == Some(&event.task_id)
                    && binding.attempt == Some(event.attempt)
                    && binding.child_session_id.as_deref() == Some(&event.child_session_id)
                    && binding.parent_call_id.as_deref() == Some(&event.parent_call_id)
                    && binding.intent.core_session_id == event.parent_session_id
                    && binding.intent.core_session_id == event.recipient_session_id
            })
        {
            return Err(DevelopmentError::InvalidInput(
                "task event has no persisted current or historical execution binding".into(),
            ));
        }
        if event.kind == TaskInboxEventKind::VerifiedCompletion
            && event.attempt == task.snapshot.attempt
            && (task.snapshot.state != TaskState::Succeeded
                || !has_required_attempt_evidence(&task.snapshot))
        {
            return Err(DevelopmentError::InvalidInput(
                "current completion receipt has no current-attempt verifier evidence".into(),
            ));
        }
        Ok(())
    }

    fn inbox_bytes(&self) -> usize {
        self.inbox
            .iter()
            .chain(self.out_of_order.values().flat_map(BTreeMap::values))
            .filter_map(|event| serde_json::to_vec(event).ok())
            .map(|event| event.len())
            .fold(0usize, usize::saturating_add)
    }

    pub(super) fn mark_interrupted_wake_dispatches_unknown(&mut self) -> bool {
        let mut changed = false;
        for lease in self.wake_leases.values_mut() {
            if lease.state == WakeLeaseState::Dispatching {
                lease.state = WakeLeaseState::Unknown;
                lease.stop_reason =
                    Some("recovery required after interrupted wake dispatch".into());
                let counters = self.counters.entry(lease.root_task_id.clone()).or_default();
                counters.unknown_wake_outcomes = counters.unknown_wake_outcomes.saturating_add(1);
                counters.stop_reason = lease.stop_reason.clone();
                changed = true;
            }
        }
        changed
    }
}

impl TaskScheduler {
    /// Persist a cross-store intent before writing a Core checklist link.
    /// Retrying the same intent is idempotent; a conflicting reuse is rejected.
    pub(crate) fn begin_task_binding(
        &mut self,
        intent: TaskBindingIntent,
    ) -> DevelopmentResult<TaskExecutionBinding> {
        validate_binding(&intent, &self.root)?;
        if let Some(existing) = self.coordination.bindings.get(&intent.intent_id) {
            if existing.intent == intent {
                return Ok(existing.clone());
            }
            return Err(DevelopmentError::Conflict(
                "task binding intent ID was reused with different source revisions".into(),
            ));
        }
        if self.coordination.bindings.len() >= MAX_BINDINGS {
            return Err(DevelopmentError::Conflict(
                "task binding table reached its supported limit".into(),
            ));
        }
        let binding = TaskExecutionBinding {
            version: 1,
            intent: intent.clone(),
            root_task_id: None,
            task_id: None,
            attempt: None,
            child_session_id: None,
            parent_call_id: None,
            status: BindingStatus::Pending,
            stop_reason: None,
        };
        self.coordination
            .bindings
            .insert(intent.intent_id.clone(), binding.clone());
        self.persist_checkpoint()?;
        Ok(binding)
    }

    /// Resolve an intent only after the scheduler task and child session have
    /// been found or created. The caller must reconcile both stores after a
    /// crash; this method does not claim a cross-store transaction.
    pub(crate) fn resolve_task_binding(
        &mut self,
        intent_id: &str,
        root_task_id: TaskId,
        task_id: TaskId,
        attempt: u32,
        child_session_id: String,
        parent_call_id: String,
    ) -> DevelopmentResult<TaskExecutionBinding> {
        validate_identifier("child session ID", &child_session_id, 256)?;
        validate_identifier("parent call ID", &parent_call_id, 256)?;
        if attempt == 0 {
            return Err(DevelopmentError::InvalidInput(
                "task binding attempt must be nonzero".into(),
            ));
        }
        let task = self.record(&task_id)?.snapshot.clone();
        self.record(&root_task_id)?;
        if root_task_id == task_id || !task_is_descendant(&root_task_id, &task_id, &self.tasks) {
            return Err(DevelopmentError::Conflict(
                "bound child task must be a dependency of its root task".into(),
            ));
        }
        if task.attempt != attempt {
            return Err(DevelopmentError::Conflict(
                "task binding attempt does not match the scheduler".into(),
            ));
        }
        if self
            .coordination
            .bindings
            .iter()
            .filter(|(id, _)| id.as_str() != intent_id)
            .map(|(_, binding)| binding)
            .chain(self.coordination.historical_bindings.iter())
            .any(|other| {
                other.status == BindingStatus::Resolved
                    && (other.child_session_id.as_deref() == Some(child_session_id.as_str())
                        || (other.task_id.as_ref() == Some(&task_id)
                            && other.attempt == Some(attempt)))
            })
        {
            return Err(DevelopmentError::Conflict(
                "task execution identity is already bound".into(),
            ));
        }
        let binding = self
            .coordination
            .bindings
            .get(intent_id)
            .ok_or_else(|| DevelopmentError::NotFound("task binding intent".into()))?;
        if let Some(policy) = self.coordination.wake_policies.get(&root_task_id)
            && policy.required_task_ids.contains(&task_id)
            && policy.owner_session_id != binding.intent.core_session_id
        {
            return Err(DevelopmentError::Conflict(
                "required child binding must use the root owner's Core session".into(),
            ));
        }
        if let Some(existing_task) = &binding.task_id
            && (existing_task != &task_id || binding.root_task_id.as_ref() != Some(&root_task_id))
        {
            return Err(DevelopmentError::Conflict(
                "task binding intent already resolves to another task".into(),
            ));
        }
        let prior_attempt = binding.attempt;
        if let Some(existing_attempt) = prior_attempt {
            if attempt < existing_attempt {
                return Err(DevelopmentError::Conflict(
                    "task binding cannot move back to an earlier attempt".into(),
                ));
            }
            if attempt == existing_attempt
                && (binding.child_session_id.as_deref() != Some(child_session_id.as_str())
                    || binding.parent_call_id.as_deref() != Some(parent_call_id.as_str()))
            {
                return Err(DevelopmentError::Conflict(
                    "task execution identity cannot change within an attempt".into(),
                ));
            }
        }
        if prior_attempt.is_some_and(|existing| attempt > existing) {
            if self.coordination.historical_bindings.len() >= MAX_HISTORICAL_BINDINGS {
                return Err(DevelopmentError::Conflict(
                    "historical task binding capacity is full; reconcile before retrying".into(),
                ));
            }
            self.coordination.historical_bindings.push(binding.clone());
            self.coordination
                .stream_cursors
                .retain(|stream, _| stream.task_id != task_id || stream.attempt >= attempt);
            self.coordination
                .out_of_order
                .retain(|stream, _| stream.task_id != task_id || stream.attempt >= attempt);
        }
        let binding = self
            .coordination
            .bindings
            .get_mut(intent_id)
            .expect("binding validated above");
        binding.root_task_id = Some(root_task_id);
        binding.task_id = Some(task_id);
        binding.attempt = Some(attempt);
        binding.child_session_id = Some(child_session_id);
        binding.parent_call_id = Some(parent_call_id);
        binding.status = BindingStatus::Resolved;
        binding.stop_reason = None;
        let resolved = binding.clone();
        self.persist_checkpoint()?;
        Ok(resolved)
    }

    #[allow(dead_code)] // The cross-store reconciliation adapter is intentionally out of scope.
    pub(crate) fn mark_binding_reconciliation_required(
        &mut self,
        intent_id: &str,
        reason: &str,
    ) -> DevelopmentResult<()> {
        validate_identifier("binding stop reason", reason, MAX_COORDINATION_TEXT_BYTES)?;
        let binding = self
            .coordination
            .bindings
            .get_mut(intent_id)
            .ok_or_else(|| DevelopmentError::NotFound("task binding intent".into()))?;
        binding.status = BindingStatus::ReconciliationRequired;
        binding.stop_reason = Some(reason.to_owned());
        self.persist_checkpoint()
    }

    pub(crate) fn task_bindings(&self) -> Vec<TaskExecutionBinding> {
        self.coordination.bindings.values().cloned().collect()
    }

    pub(crate) fn register_root_wake_policy(
        &mut self,
        policy: RootWakePolicy,
    ) -> DevelopmentResult<()> {
        if policy.reserved_wakes != 0 {
            return Err(DevelopmentError::InvalidInput(
                "owner wake reservations are scheduler-managed".into(),
            ));
        }
        validate_wake_policy(&policy, &self.tasks)?;
        for task_id in &policy.required_task_ids {
            let current_attempt = self.tasks[task_id].snapshot.attempt;
            if self
                .coordination
                .bindings
                .values()
                .chain(self.coordination.historical_bindings.iter())
                .any(|binding| {
                    binding.status == BindingStatus::Resolved
                        && binding.root_task_id.as_ref() == Some(&policy.root_task_id)
                        && binding.task_id.as_ref() == Some(task_id)
                        && binding
                            .attempt
                            .is_some_and(|attempt| attempt <= current_attempt)
                        && binding.intent.core_session_id != policy.owner_session_id
                })
            {
                return Err(DevelopmentError::Conflict(
                    "root wake owner must match every resolved required-task binding".into(),
                ));
            }
        }
        if self
            .coordination
            .wake_policies
            .contains_key(&policy.root_task_id)
        {
            let prior = &self.coordination.wake_policies[&policy.root_task_id];
            if prior.version == policy.version
                && prior.root_task_id == policy.root_task_id
                && prior.owner_session_id == policy.owner_session_id
                && prior.owner_turn_id == policy.owner_turn_id
                && prior.required_task_ids == policy.required_task_ids
                && prior.max_events == policy.max_events
                && prior.max_event_bytes == policy.max_event_bytes
                && prior.max_owner_wakes == policy.max_owner_wakes
            {
                return Ok(());
            }
            return Err(DevelopmentError::Conflict(
                "root task already has a different wake policy".into(),
            ));
        }
        self.coordination
            .wake_policies
            .insert(policy.root_task_id.clone(), policy);
        self.persist_checkpoint()
    }

    /// Append an already parent-redacted event. Persistence completes before
    /// this method returns an acknowledgement disposition.
    pub(crate) fn append_task_inbox_event(
        &mut self,
        event: TaskInboxEvent,
    ) -> DevelopmentResult<InboxAppendResult> {
        validate_event(&event, &self.root)?;
        if event.sequence == u64::MAX {
            return Err(DevelopmentError::InvalidInput(
                "task event sequence is exhausted".into(),
            ));
        }
        let stream = event_stream_id(&event);
        if let Some(receipt) = self
            .coordination
            .completion_receipts
            .values()
            .find(|receipt| receipt.event_id == event.event_id)
        {
            if receipt != &event {
                return Err(DevelopmentError::Conflict(
                    "task event ID was reused with different completion content".into(),
                ));
            }
            let counters = self
                .coordination
                .counters
                .entry(event.root_task_id.clone())
                .or_default();
            counters.duplicate_events = counters.duplicate_events.saturating_add(1);
            self.persist_checkpoint()?;
            return Ok(InboxAppendResult::Duplicate);
        }
        if self
            .coordination
            .inbox
            .iter()
            .chain(
                self.coordination
                    .out_of_order
                    .values()
                    .flat_map(BTreeMap::values),
            )
            .find(|existing| existing.event_id == event.event_id)
            .is_some_and(|existing| existing == &event)
        {
            let counters = self
                .coordination
                .counters
                .entry(event.root_task_id.clone())
                .or_default();
            counters.duplicate_events = counters.duplicate_events.saturating_add(1);
            self.persist_checkpoint()?;
            return Ok(InboxAppendResult::Duplicate);
        }
        if self
            .coordination
            .inbox
            .iter()
            .chain(
                self.coordination
                    .out_of_order
                    .values()
                    .flat_map(BTreeMap::values),
            )
            .any(|existing| existing.event_id == event.event_id)
        {
            return Err(DevelopmentError::Conflict(
                "task event ID was reused with different content".into(),
            ));
        }
        let binding = self
            .coordination
            .bindings
            .values()
            .chain(self.coordination.historical_bindings.iter())
            .find(|binding| {
                binding.status == BindingStatus::Resolved
                    && binding.root_task_id.as_ref() == Some(&event.root_task_id)
                    && binding.task_id.as_ref() == Some(&event.task_id)
                    && binding.child_session_id.as_deref() == Some(&event.child_session_id)
                    && binding.parent_call_id.as_deref() == Some(&event.parent_call_id)
                    && binding.intent.core_session_id == event.parent_session_id
                    && binding.intent.core_session_id == event.recipient_session_id
            })
            .ok_or_else(|| {
                DevelopmentError::Conflict("task event has no matching binding".into())
            })?;
        if binding.attempt != Some(event.attempt) {
            self.note_stale_event(&event.root_task_id)?;
            return Ok(InboxAppendResult::Stale);
        }
        let policy = self
            .coordination
            .wake_policies
            .get(&event.root_task_id)
            .cloned()
            .ok_or_else(|| DevelopmentError::Conflict("task event has no root policy".into()))?;
        if policy.owner_session_id != event.parent_session_id {
            return Err(DevelopmentError::Conflict(
                "task event parent session does not match the root wake owner".into(),
            ));
        }
        if !policy.required_task_ids.contains(&event.task_id) {
            return Err(DevelopmentError::Conflict(
                "task event is not from a required root task".into(),
            ));
        }
        let task = self.record(&event.task_id)?.snapshot.clone();
        if task.attempt != event.attempt {
            self.note_stale_event(&event.root_task_id)?;
            return Ok(InboxAppendResult::Stale);
        }
        if task.state.terminal()
            && event.kind != TaskInboxEventKind::VerifiedCompletion
            && !(event.kind == TaskInboxEventKind::Cancelled && task.state == TaskState::Cancelled)
        {
            self.note_stale_event(&event.root_task_id)?;
            return Ok(InboxAppendResult::Stale);
        }
        if event.kind == TaskInboxEventKind::VerifiedCompletion {
            if let Some(receipt) = self.coordination.completion_receipts.get(&event.task_id)
                && receipt.attempt == event.attempt
            {
                if receipt.event_id == event.event_id {
                    if receipt == &event {
                        let counters = self
                            .coordination
                            .counters
                            .entry(event.root_task_id.clone())
                            .or_default();
                        counters.duplicate_events = counters.duplicate_events.saturating_add(1);
                        self.persist_checkpoint()?;
                        return Ok(InboxAppendResult::Duplicate);
                    }
                    return Err(DevelopmentError::Conflict(
                        "task event ID was reused with different completion content".into(),
                    ));
                }
                self.note_stale_event(&event.root_task_id)?;
                return Ok(InboxAppendResult::Stale);
            }
            if self
                .coordination
                .inbox
                .iter()
                .chain(
                    self.coordination
                        .out_of_order
                        .values()
                        .flat_map(BTreeMap::values),
                )
                .any(|existing| {
                    existing.task_id == event.task_id
                        && existing.attempt == event.attempt
                        && existing.kind == TaskInboxEventKind::VerifiedCompletion
                })
            {
                self.note_stale_event(&event.root_task_id)?;
                return Ok(InboxAppendResult::Stale);
            }
            self.validate_verified_completion(&event)?;
        }
        if !self.coordination.stream_cursors.contains_key(&stream)
            && self.coordination.stream_cursors.len() >= MAX_EVENT_STREAMS
        {
            self.coordination
                .counters
                .entry(event.root_task_id.clone())
                .or_default()
                .stop_reason = Some("task event stream table requires reconciliation".into());
            self.persist_checkpoint()?;
            return Err(DevelopmentError::Conflict(
                "task event stream table is full; reconciliation is required".into(),
            ));
        }
        let next_sequence = self
            .coordination
            .stream_cursors
            .get(&stream)
            .map_or(1, |cursor| cursor.next_sequence);
        if event.sequence < next_sequence {
            self.note_stale_event(&event.root_task_id)?;
            return Ok(InboxAppendResult::Stale);
        }
        if event.sequence == 0 {
            return Err(DevelopmentError::InvalidInput(
                "task event sequence must start at one".into(),
            ));
        }
        if event.sequence > next_sequence {
            if event.sequence.saturating_sub(next_sequence) > MAX_REORDER_DISTANCE {
                self.coordination
                    .stream_cursors
                    .entry(stream.clone())
                    .or_insert(TaskStreamCursor {
                        next_sequence,
                        gap_detected_at_ms: None,
                    })
                    .gap_detected_at_ms = Some(event.observed_at_ms);
                self.coordination
                    .counters
                    .entry(event.root_task_id.clone())
                    .or_default()
                    .stop_reason = Some("task event sequence gap requires reconciliation".into());
                self.persist_checkpoint()?;
                return Err(DevelopmentError::Conflict(
                    "task event sequence gap requires reconciliation".into(),
                ));
            }
            if let Some(existing) = self
                .coordination
                .out_of_order
                .get(&stream)
                .and_then(|pending| pending.get(&event.sequence))
            {
                if existing == &event {
                    return Ok(InboxAppendResult::Duplicate);
                }
                return Err(DevelopmentError::Conflict(
                    "task stream sequence was reused with a different event ID".into(),
                ));
            }
            let pending_len = self
                .coordination
                .out_of_order
                .get(&stream)
                .map_or(0, BTreeMap::len);
            if pending_len >= MAX_REORDERED_EVENTS_PER_STREAM
                || self.coordination.inbox.len()
                    + self
                        .coordination
                        .out_of_order
                        .values()
                        .map(BTreeMap::len)
                        .sum::<usize>()
                    >= MAX_INBOX_EVENTS
                || self
                    .coordination
                    .inbox_bytes()
                    .saturating_add(encoded_event_len(&event)?)
                    > MAX_INBOX_BYTES
            {
                self.coordination
                    .counters
                    .entry(event.root_task_id.clone())
                    .or_default()
                    .stop_reason = Some("task event inbox requires reconciliation".into());
                self.persist_checkpoint()?;
                return Err(DevelopmentError::Conflict(
                    "task event inbox is full; reconcile or acknowledge queued events".into(),
                ));
            }
            self.ensure_event_budget(&policy, &event)?;
            self.coordination
                .out_of_order
                .entry(stream.clone())
                .or_default()
                .insert(event.sequence, event.clone());
            self.count_accepted_event(&event)?;
            self.coordination
                .stream_cursors
                .entry(stream.clone())
                .or_insert(TaskStreamCursor {
                    next_sequence,
                    gap_detected_at_ms: None,
                })
                .gap_detected_at_ms = Some(event.observed_at_ms);
            self.coordination
                .counters
                .entry(event.root_task_id.clone())
                .or_default()
                .stop_reason = Some("task event sequence gap is awaiting earlier events".into());
            self.persist_checkpoint()?;
            return Ok(InboxAppendResult::BufferedOutOfOrder);
        }
        self.ensure_event_budget(&policy, &event)?;
        if self.coordination.inbox.len()
            + self
                .coordination
                .out_of_order
                .values()
                .map(BTreeMap::len)
                .sum::<usize>()
            >= MAX_INBOX_EVENTS
            || self
                .coordination
                .inbox_bytes()
                .saturating_add(encoded_event_len(&event)?)
                > MAX_INBOX_BYTES
        {
            self.coordination
                .counters
                .entry(event.root_task_id.clone())
                .or_default()
                .stop_reason = Some("task event inbox requires reconciliation".into());
            self.persist_checkpoint()?;
            return Err(DevelopmentError::Conflict(
                "task event inbox is full; reconcile or acknowledge queued events".into(),
            ));
        }
        self.enqueue_in_order(event)?;
        self.promote_contiguous_pending(&stream)?;
        self.ensure_inbox_capacity()?;
        self.persist_checkpoint()?;
        Ok(InboxAppendResult::Accepted)
    }

    pub(crate) fn task_inbox(&self, root_task_id: &TaskId) -> Vec<TaskInboxEvent> {
        self.coordination
            .inbox
            .iter()
            .filter(|event| &event.root_task_id == root_task_id)
            .cloned()
            .collect()
    }

    pub(crate) fn acknowledge_task_inbox_event(
        &mut self,
        event_id: &str,
    ) -> DevelopmentResult<bool> {
        let Some(index) = self
            .coordination
            .inbox
            .iter()
            .position(|event| event.event_id == event_id)
        else {
            return Ok(false);
        };
        if self.coordination.inbox[index].kind == TaskInboxEventKind::VerifiedCompletion {
            return Err(DevelopmentError::Conflict(
                "verified completion is retained until the owner wake is reconciled".into(),
            ));
        }
        self.coordination.inbox.remove(index);
        self.persist_checkpoint()?;
        Ok(true)
    }

    pub(crate) fn task_coordination_counters(
        &self,
        root_task_id: &TaskId,
    ) -> TaskCoordinationCounters {
        self.coordination
            .counters
            .get(root_task_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn reserve_owner_wake(
        &mut self,
        root_task_id: &TaskId,
        lease_id: String,
        now_ms: u128,
        lease_duration_ms: u64,
    ) -> DevelopmentResult<Option<OwnerWakeLease>> {
        validate_identifier("owner wake lease ID", &lease_id, 128)?;
        if lease_duration_ms == 0 || lease_duration_ms > MAX_WAKE_LEASE_MS {
            return Err(DevelopmentError::InvalidInput(
                "owner wake lease duration is outside the supported bound".into(),
            ));
        }
        if let Some(existing) = self.coordination.wake_leases.get(&lease_id) {
            if existing.root_task_id != *root_task_id {
                return Err(DevelopmentError::Conflict(
                    "owner wake lease ID was reused for another root".into(),
                ));
            }
            return Ok(Some(existing.clone()));
        }
        let policy = self
            .coordination
            .wake_policies
            .get(root_task_id)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound("root wake policy".into()))?;
        if let Some(existing) = self
            .coordination
            .wake_leases
            .values()
            .find(|lease| lease.root_task_id == *root_task_id && !lease.state.terminal())
        {
            return Ok(Some(existing.clone()));
        }
        if policy.reserved_wakes >= policy.max_owner_wakes {
            return Ok(None);
        }
        if self.coordination.wake_leases.values().any(|lease| {
            lease.owner_session_id == policy.owner_session_id && !lease.state.terminal()
        }) {
            return Err(DevelopmentError::Conflict(
                "owner session already has a fenced wake lease".into(),
            ));
        }
        let mut event_ids = Vec::with_capacity(policy.required_task_ids.len());
        let mut watermark = 0_u64;
        for task_id in &policy.required_task_ids {
            let snapshot = self.record(task_id)?.snapshot.clone();
            if snapshot.state != TaskState::Succeeded || !has_required_attempt_evidence(&snapshot) {
                return Ok(None);
            }
            if !self.coordination.bindings.values().any(|binding| {
                binding.status == BindingStatus::Resolved
                    && binding.root_task_id.as_ref() == Some(root_task_id)
                    && binding.task_id.as_ref() == Some(task_id)
                    && binding.attempt == Some(snapshot.attempt)
                    && binding.intent.core_session_id == policy.owner_session_id
            }) {
                return Ok(None);
            }
            let Some(receipt) = self.coordination.completion_receipts.get(task_id) else {
                return Ok(None);
            };
            if receipt.attempt != snapshot.attempt
                || receipt.kind != TaskInboxEventKind::VerifiedCompletion
            {
                return Ok(None);
            }
            event_ids.push(receipt.event_id.clone());
            watermark = watermark.max(receipt.sequence);
        }
        event_ids.sort();
        event_ids.dedup();
        if event_ids.is_empty() {
            return Ok(None);
        }
        let lease = OwnerWakeLease {
            lease_id: lease_id.clone(),
            root_task_id: root_task_id.clone(),
            owner_session_id: policy.owner_session_id.clone(),
            owner_turn_id: policy.owner_turn_id.clone(),
            event_ids,
            inbox_watermark: watermark,
            reserved_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(u128::from(lease_duration_ms)),
            state: WakeLeaseState::Reserved,
            stop_reason: None,
        };
        self.coordination
            .wake_leases
            .insert(lease_id, lease.clone());
        let policy = self
            .coordination
            .wake_policies
            .get_mut(root_task_id)
            .expect("policy loaded above");
        policy.reserved_wakes = policy.reserved_wakes.saturating_add(1);
        let counters = self
            .coordination
            .counters
            .entry(root_task_id.clone())
            .or_default();
        counters.wake_reservations = counters.wake_reservations.saturating_add(1);
        self.persist_checkpoint()?;
        Ok(Some(lease))
    }

    pub(crate) fn mark_owner_wake_dispatching(
        &mut self,
        lease_id: &str,
        now_ms: u128,
    ) -> DevelopmentResult<OwnerWakeLease> {
        let lease = self
            .coordination
            .wake_leases
            .get_mut(lease_id)
            .ok_or_else(|| DevelopmentError::NotFound("owner wake lease".into()))?;
        if lease.state == WakeLeaseState::Dispatching {
            return Err(DevelopmentError::Conflict(
                "owner wake dispatch is already in progress and must be reconciled".into(),
            ));
        }
        if lease.state != WakeLeaseState::Reserved {
            return Err(DevelopmentError::Conflict(
                "owner wake lease is no longer available for dispatch".into(),
            ));
        }
        if now_ms >= lease.expires_at_ms {
            lease.state = WakeLeaseState::Unknown;
            lease.stop_reason = Some("wake reservation expired before reconciliation".into());
            let root_task_id = lease.root_task_id.clone();
            let stop_reason = lease.stop_reason.clone();
            let counters = self.coordination.counters.entry(root_task_id).or_default();
            counters.unknown_wake_outcomes = counters.unknown_wake_outcomes.saturating_add(1);
            counters.stop_reason = stop_reason;
            self.persist_checkpoint()?;
            return Err(DevelopmentError::Conflict(
                "owner wake reservation requires reconciliation".into(),
            ));
        }
        lease.state = WakeLeaseState::Dispatching;
        let lease = lease.clone();
        let counters = self
            .coordination
            .counters
            .entry(lease.root_task_id.clone())
            .or_default();
        counters.wake_dispatches = counters.wake_dispatches.saturating_add(1);
        self.persist_checkpoint()?;
        Ok(lease)
    }

    pub(crate) fn finish_owner_wake_dispatch(
        &mut self,
        lease_id: &str,
        outcome: WakeDeliveryOutcome,
        stop_reason: Option<String>,
    ) -> DevelopmentResult<OwnerWakeLease> {
        if let Some(reason) = stop_reason.as_deref() {
            validate_identifier("wake stop reason", reason, MAX_COORDINATION_TEXT_BYTES)?;
        }
        let lease = self
            .coordination
            .wake_leases
            .get_mut(lease_id)
            .ok_or_else(|| DevelopmentError::NotFound("owner wake lease".into()))?;
        if lease.state != WakeLeaseState::Dispatching {
            return Err(DevelopmentError::Conflict(
                "owner wake dispatch was not started".into(),
            ));
        }
        match outcome {
            WakeDeliveryOutcome::Acknowledged => {
                lease.state = WakeLeaseState::Delivered;
                lease.stop_reason = None;
            }
            WakeDeliveryOutcome::DefinitelyNotSent => {
                lease.state = WakeLeaseState::FailedBeforeDispatch;
                lease.stop_reason =
                    Some(stop_reason.unwrap_or_else(|| "wake was confirmed not sent".into()));
            }
            WakeDeliveryOutcome::Unknown => {
                lease.state = WakeLeaseState::Unknown;
                lease.stop_reason =
                    Some(stop_reason.unwrap_or_else(|| "wake delivery outcome is unknown".into()));
            }
        }
        let result = lease.clone();
        if result.state == WakeLeaseState::Unknown {
            let counters = self
                .coordination
                .counters
                .entry(result.root_task_id.clone())
                .or_default();
            counters.unknown_wake_outcomes = counters.unknown_wake_outcomes.saturating_add(1);
            counters.stop_reason = result.stop_reason.clone();
        }
        self.persist_checkpoint()?;
        Ok(result)
    }

    pub(crate) fn reconcile_owner_wake(
        &mut self,
        lease_id: &str,
        delivered: bool,
    ) -> DevelopmentResult<OwnerWakeLease> {
        let lease = self
            .coordination
            .wake_leases
            .get_mut(lease_id)
            .ok_or_else(|| DevelopmentError::NotFound("owner wake lease".into()))?;
        if lease.state != WakeLeaseState::Unknown {
            return Err(DevelopmentError::Conflict(
                "only an unknown owner wake can be reconciled".into(),
            ));
        }
        lease.state = if delivered {
            WakeLeaseState::Delivered
        } else {
            WakeLeaseState::FailedBeforeDispatch
        };
        lease.stop_reason = None;
        let lease = lease.clone();
        if let Some(counters) = self.coordination.counters.get_mut(&lease.root_task_id) {
            counters.stop_reason = None;
        }
        self.persist_checkpoint()?;
        Ok(lease)
    }

    pub(crate) fn settle_owner_wake(
        &mut self,
        lease_id: &str,
        owner_session_id: &str,
        owner_turn_id: &str,
    ) -> DevelopmentResult<OwnerWakeLease> {
        let lease = self
            .coordination
            .wake_leases
            .get_mut(lease_id)
            .ok_or_else(|| DevelopmentError::NotFound("owner wake lease".into()))?;
        if lease.state != WakeLeaseState::Delivered
            || lease.owner_session_id != owner_session_id
            || lease.owner_turn_id != owner_turn_id
        {
            return Err(DevelopmentError::Conflict(
                "owner wake settlement does not match its delivered session turn".into(),
            ));
        }
        lease.state = WakeLeaseState::Settled;
        let lease = lease.clone();
        self.coordination
            .inbox
            .retain(|event| !lease.event_ids.iter().any(|id| id == &event.event_id));
        self.persist_checkpoint()?;
        Ok(lease)
    }

    pub(crate) fn owner_wake_lease(&self, lease_id: &str) -> Option<OwnerWakeLease> {
        self.coordination.wake_leases.get(lease_id).cloned()
    }

    pub(crate) fn dispatch_reserved_owner_wake<A: OwnerWakeDispatchAdapter>(
        &mut self,
        lease_id: &str,
        now_ms: u128,
        adapter: &mut A,
    ) -> DevelopmentResult<OwnerWakeLease> {
        let dispatching = self.mark_owner_wake_dispatching(lease_id, now_ms)?;
        let outcome = adapter.dispatch_owner_wake(&dispatching);
        self.finish_owner_wake_dispatch(lease_id, outcome, None)
    }

    fn validate_verified_completion(&self, event: &TaskInboxEvent) -> DevelopmentResult<()> {
        let snapshot = &self.record(&event.task_id)?.snapshot;
        if snapshot.state != TaskState::Succeeded
            || snapshot.attempt != event.attempt
            || !has_required_attempt_evidence(snapshot)
        {
            return Err(DevelopmentError::Conflict(
                "verified completion requires current-attempt evidence".into(),
            ));
        }
        Ok(())
    }

    fn count_accepted_event(&mut self, event: &TaskInboxEvent) -> DevelopmentResult<()> {
        let bytes = encoded_event_len(event)?;
        let counters = self
            .coordination
            .counters
            .entry(event.root_task_id.clone())
            .or_default();
        counters.accepted_events = counters.accepted_events.saturating_add(1);
        counters.accepted_event_bytes = counters
            .accepted_event_bytes
            .saturating_add(bytes.try_into().unwrap_or(u64::MAX));
        Ok(())
    }

    fn ensure_event_budget(
        &mut self,
        policy: &RootWakePolicy,
        event: &TaskInboxEvent,
    ) -> DevelopmentResult<()> {
        let event_bytes = encoded_event_len(event)? as u64;
        let counters = self
            .coordination
            .counters
            .get(&event.root_task_id)
            .cloned()
            .unwrap_or_default();
        if counters.accepted_events >= policy.max_events
            || counters.accepted_event_bytes.saturating_add(event_bytes) > policy.max_event_bytes
        {
            self.coordination
                .counters
                .entry(event.root_task_id.clone())
                .or_default()
                .stop_reason = Some("root task event budget exhausted".into());
            self.persist_checkpoint()?;
            return Err(DevelopmentError::Conflict(
                "root task event budget exhausted; human reconciliation is required".into(),
            ));
        }
        Ok(())
    }

    fn note_stale_event(&mut self, root_task_id: &TaskId) -> DevelopmentResult<()> {
        let counters = self
            .coordination
            .counters
            .entry(root_task_id.clone())
            .or_default();
        counters.stale_events = counters.stale_events.saturating_add(1);
        self.persist_checkpoint()
    }

    fn enqueue_in_order(&mut self, event: TaskInboxEvent) -> DevelopmentResult<()> {
        self.count_accepted_event(&event)?;
        let cursor = self
            .coordination
            .stream_cursors
            .entry(event_stream_id(&event))
            .or_insert(TaskStreamCursor {
                next_sequence: 1,
                gap_detected_at_ms: None,
            });
        if event.sequence != cursor.next_sequence {
            return Err(DevelopmentError::Conflict(
                "task inbox event is not next in its stream".into(),
            ));
        }
        cursor.next_sequence = cursor.next_sequence.saturating_add(1);
        cursor.gap_detected_at_ms = None;
        if event.kind == TaskInboxEventKind::VerifiedCompletion {
            self.coordination
                .completion_receipts
                .insert(event.task_id.clone(), event.clone());
        }
        self.project_event_stop_reason(&event);
        self.coordination.inbox.push_back(event);
        Ok(())
    }

    fn promote_contiguous_pending(&mut self, stream: &TaskEventStreamId) -> DevelopmentResult<()> {
        loop {
            let next_sequence = self
                .coordination
                .stream_cursors
                .get(stream)
                .map_or(1, |cursor| cursor.next_sequence);
            let Some(event) = self
                .coordination
                .out_of_order
                .get_mut(stream)
                .and_then(|pending| pending.remove(&next_sequence))
            else {
                if let Some(pending) = self.coordination.out_of_order.get(stream)
                    && !pending.is_empty()
                    && let Some(cursor) = self.coordination.stream_cursors.get_mut(stream)
                {
                    cursor.gap_detected_at_ms =
                        pending.values().next().map(|event| event.observed_at_ms);
                }
                break;
            };
            let cursor = self
                .coordination
                .stream_cursors
                .get_mut(stream)
                .expect("in-order cursor was initialized");
            cursor.next_sequence = cursor.next_sequence.saturating_add(1);
            cursor.gap_detected_at_ms = None;
            if event.kind == TaskInboxEventKind::VerifiedCompletion {
                self.coordination
                    .completion_receipts
                    .insert(event.task_id.clone(), event.clone());
            }
            self.project_event_stop_reason(&event);
            self.coordination.inbox.push_back(event);
        }
        if self
            .coordination
            .out_of_order
            .get(stream)
            .is_some_and(BTreeMap::is_empty)
        {
            self.coordination.out_of_order.remove(stream);
        }
        if self
            .coordination
            .stream_cursors
            .values()
            .all(|cursor| cursor.gap_detected_at_ms.is_none())
        {
            for counters in self.coordination.counters.values_mut() {
                if counters.stop_reason.as_deref()
                    == Some("task event sequence gap is awaiting earlier events")
                    || counters.stop_reason.as_deref()
                        == Some("task event sequence gap requires reconciliation")
                {
                    counters.stop_reason = None;
                }
            }
        }
        Ok(())
    }

    fn project_event_stop_reason(&mut self, event: &TaskInboxEvent) {
        let counters = self
            .coordination
            .counters
            .entry(event.root_task_id.clone())
            .or_default();
        match event.kind {
            TaskInboxEventKind::Question => {
                counters.stop_reason = Some("child is waiting for a human answer".into());
            }
            TaskInboxEventKind::Blocked | TaskInboxEventKind::Cancelled => {
                counters.stop_reason = Some(event.redacted_summary.clone());
            }
            TaskInboxEventKind::Progress | TaskInboxEventKind::VerifiedCompletion => {}
        }
    }

    fn ensure_inbox_capacity(&self) -> DevelopmentResult<()> {
        let event_count = self.coordination.inbox.len().saturating_add(
            self.coordination
                .out_of_order
                .values()
                .map(BTreeMap::len)
                .sum::<usize>(),
        );
        if event_count > MAX_INBOX_EVENTS || self.coordination.inbox_bytes() > MAX_INBOX_BYTES {
            Err(DevelopmentError::Conflict(
                "task event inbox is full; reconcile or acknowledge queued events".into(),
            ))
        } else {
            Ok(())
        }
    }
}

fn has_required_attempt_evidence(snapshot: &TaskSnapshot) -> bool {
    fn requirement_has_evidence(
        requirement: &VerificationRequirement,
        snapshot: &TaskSnapshot,
    ) -> bool {
        match requirement {
            VerificationRequirement::Settled | VerificationRequirement::Inferred => false,
            VerificationRequirement::All { requirements } => {
                requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        VerificationRequirement::Settled | VerificationRequirement::Inferred
                    ) || requirement_has_evidence(requirement, snapshot)
                })
            }
            requirement => {
                let kind = if matches!(requirement, VerificationRequirement::Command { .. }) {
                    "command"
                } else {
                    super::verification_kind(requirement)
                };
                snapshot.evidence.iter().any(|evidence| {
                    evidence.kind == kind
                        && evidence.attempt == snapshot.attempt
                        && super::evidence_satisfies(requirement, evidence)
                })
            }
        }
    }

    verification_has_evidence_requirement(&snapshot.verification)
        && requirement_has_evidence(&snapshot.verification, snapshot)
}

fn validate_binding(intent: &TaskBindingIntent, workspace: &Path) -> DevelopmentResult<()> {
    validate_identifier("binding intent ID", &intent.intent_id, 128)?;
    validate_identifier("workspace ID", &intent.workspace_id, 1024)?;
    validate_identifier("Core session ID", &intent.core_session_id, 256)?;
    validate_identifier("Core todo ID", &intent.todo_id, 64)?;
    if intent.workspace_id != workspace.to_string_lossy().as_ref()
        || intent.todo_revision == 0
        || intent.plan_revision == 0
        || !intent.todo_id.starts_with('T')
        || !intent.todo_id[1..]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err(DevelopmentError::InvalidInput(
            "task binding workspace or todo identity is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_event(event: &TaskInboxEvent, workspace: &Path) -> DevelopmentResult<()> {
    validate_identifier("task event ID", &event.event_id, 128)?;
    validate_identifier("task event workspace ID", &event.workspace_id, 1024)?;
    validate_identifier("child session ID", &event.child_session_id, 256)?;
    validate_identifier("parent session ID", &event.parent_session_id, 256)?;
    validate_identifier("recipient session ID", &event.recipient_session_id, 256)?;
    validate_identifier("parent call ID", &event.parent_call_id, 256)?;
    validate_identifier(
        "redacted task event summary",
        &event.redacted_summary,
        MAX_INBOX_EVENT_BYTES,
    )?;
    if event.workspace_id != workspace.to_string_lossy().as_ref()
        || event.sequence == 0
        || event.attempt == 0
        || !event.parent_redaction_applied
        || event.redacted_summary.len() > MAX_COORDINATION_TEXT_BYTES
        || encoded_event_len(event)? > MAX_INBOX_EVENT_BYTES
    {
        return Err(DevelopmentError::InvalidInput(
            "task event identity or payload exceeds supported bounds".into(),
        ));
    }
    Ok(())
}

fn validate_stream_id(stream: &TaskEventStreamId) -> DevelopmentResult<()> {
    validate_identifier("child session ID", &stream.child_session_id, 256)?;
    validate_identifier("parent session ID", &stream.parent_session_id, 256)?;
    validate_identifier("parent call ID", &stream.parent_call_id, 256)?;
    if stream.attempt == 0 {
        return Err(DevelopmentError::InvalidInput(
            "task event stream attempt must be nonzero".into(),
        ));
    }
    Ok(())
}

fn validate_wake_policy(
    policy: &RootWakePolicy,
    tasks: &BTreeMap<TaskId, TaskRecord>,
) -> DevelopmentResult<()> {
    validate_identifier("owner session ID", &policy.owner_session_id, 256)?;
    validate_identifier("owner turn ID", &policy.owner_turn_id, 256)?;
    if policy.version != 1
        || policy.max_owner_wakes != 1
        || policy.reserved_wakes > policy.max_owner_wakes
        || policy.max_events == 0
        || policy.max_events > 10_000_000
        || policy.max_event_bytes == 0
        || policy.max_event_bytes > MAX_INBOX_BYTES as u64
        || policy.required_task_ids.is_empty()
        || policy.required_task_ids.len() > MAX_REQUIRED_TASKS
        || !tasks.contains_key(&policy.root_task_id)
    {
        return Err(DevelopmentError::InvalidInput(
            "root wake policy exceeds supported bounds".into(),
        ));
    }
    let mut unique = BTreeSet::new();
    let mut inherited_event_limit = 0_u64;
    for task_id in &policy.required_task_ids {
        let task = tasks.get(task_id).ok_or_else(|| {
            DevelopmentError::InvalidInput("wake policy references a missing task".into())
        })?;
        inherited_event_limit =
            inherited_event_limit.saturating_add(task.snapshot.budget.max_events);
        if !verification_has_evidence_requirement(&task.snapshot.verification)
            || !unique.insert(task_id)
            || task_id == &policy.root_task_id
            || !task_is_descendant(&policy.root_task_id, task_id, tasks)
        {
            return Err(DevelopmentError::InvalidInput(
                "required wake tasks must be unique and use evidence verification".into(),
            ));
        }
    }
    if policy.max_events > inherited_event_limit {
        return Err(DevelopmentError::InvalidInput(
            "root wake event limit exceeds the required tasks' inherited event budgets".into(),
        ));
    }
    if !verification_has_evidence_requirement(&tasks[&policy.root_task_id].snapshot.verification) {
        return Err(DevelopmentError::InvalidInput(
            "wake root must use evidence-producing verification".into(),
        ));
    }
    Ok(())
}

fn verification_has_evidence_requirement(requirement: &VerificationRequirement) -> bool {
    match requirement {
        VerificationRequirement::Settled | VerificationRequirement::Inferred => false,
        VerificationRequirement::All { requirements } => requirements
            .iter()
            .any(verification_has_evidence_requirement),
        _ => true,
    }
}

fn task_is_descendant(
    root_task_id: &TaskId,
    required_task_id: &TaskId,
    tasks: &BTreeMap<TaskId, TaskRecord>,
) -> bool {
    let mut pending = tasks
        .get(root_task_id)
        .map(|record| record.snapshot.dependencies.clone())
        .unwrap_or_default();
    let mut visited = BTreeSet::new();
    while let Some(task_id) = pending.pop() {
        if task_id == *required_task_id {
            return true;
        }
        if visited.insert(task_id.clone())
            && let Some(record) = tasks.get(&task_id)
        {
            pending.extend(record.snapshot.dependencies.iter().cloned());
        }
    }
    false
}

fn validate_wake_lease(lease: &OwnerWakeLease) -> DevelopmentResult<()> {
    validate_identifier("owner wake lease ID", &lease.lease_id, 128)?;
    validate_identifier("owner session ID", &lease.owner_session_id, 256)?;
    validate_identifier("owner turn ID", &lease.owner_turn_id, 256)?;
    if lease.event_ids.is_empty()
        || lease.event_ids.len() > MAX_REQUIRED_TASKS
        || lease.expires_at_ms < lease.reserved_at_ms
        || lease.expires_at_ms.saturating_sub(lease.reserved_at_ms) > u128::from(MAX_WAKE_LEASE_MS)
    {
        return Err(DevelopmentError::InvalidInput(
            "owner wake lease event set is outside its bound".into(),
        ));
    }
    let mut unique_events = BTreeSet::new();
    for event_id in &lease.event_ids {
        validate_identifier("owner wake event ID", event_id, 128)?;
        if !unique_events.insert(event_id) {
            return Err(DevelopmentError::InvalidInput(
                "owner wake lease repeats an event ID".into(),
            ));
        }
    }
    if let Some(reason) = lease.stop_reason.as_deref() {
        validate_identifier(
            "owner wake stop reason",
            reason,
            MAX_COORDINATION_TEXT_BYTES,
        )?;
    }
    Ok(())
}

fn event_stream_id(event: &TaskInboxEvent) -> TaskEventStreamId {
    TaskEventStreamId {
        root_task_id: event.root_task_id.clone(),
        task_id: event.task_id.clone(),
        attempt: event.attempt,
        parent_session_id: event.parent_session_id.clone(),
        parent_call_id: event.parent_call_id.clone(),
        child_session_id: event.child_session_id.clone(),
    }
}

fn encoded_event_len(event: &TaskInboxEvent) -> DevelopmentResult<usize> {
    serde_json::to_vec(event)
        .map(|encoded| encoded.len())
        .map_err(Into::into)
}

fn validate_identifier(label: &str, value: &str, maximum: usize) -> DevelopmentResult<()> {
    if value.is_empty() || value.len() > maximum || value.contains('\0') {
        return Err(DevelopmentError::InvalidInput(format!(
            "{label} must contain 1..={maximum} bytes without NUL"
        )));
    }
    Ok(())
}
