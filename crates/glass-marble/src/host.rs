//! Host approval boundary for runtime tool requests.

use crate::{
    MarbleError,
    conversation::{CancelToken, PUMP_INTERVAL},
    tools::{ToolOutcome, ToolRouter, ToolScope},
};
use glass_runtime_client::{RuntimeClient, protocol::ToolRequest};
use serde_json::Value;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

/// Refusal used when a stop request ends a tool call that was already running.
///
/// The executor thread cannot be preempted inside a filesystem call, so a
/// running write may still be landing. What the stop buys is that the turn
/// stops waiting: the runtime is answered at once instead of holding the pump
/// loop until a slow call reports back.
pub const STOPPED_TOOL: &str = "cancelled while this tool call was running";
/// Result of a tool body whose executor thread died without reporting back.
const LOST_TOOL: &str = "the tool ended without reporting a result";

/// Explicit host decision for one inbound tool request.
/// An explicit host decision. Deny is the default so an unanswered tool
/// request fails closed instead of falling back to an implicit allow.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Approval {
    #[default]
    Deny,
    Approve,
}

/// Selectable host posture for one durable conversation.
///
/// `decision` is the standing answer when the host does not prompt. `prompts`
/// turns each request into an explicit product-level approval so the human can
/// see the exact tool and arguments before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPolicy {
    mode: Approval,
    prompts: bool,
}

impl Default for HostPolicy {
    fn default() -> Self {
        // Fail closed while still asking: the standing answer is deny, and the
        // surface decides each request while a human is present.
        Self::asking()
    }
}

impl HostPolicy {
    /// Approve every request without prompting. Use only for headless runs the
    /// operator explicitly authorized.
    pub fn approving() -> Self {
        Self {
            mode: Approval::Approve,
            prompts: false,
        }
    }

    /// Refuse every request without prompting.
    pub fn denying() -> Self {
        Self {
            mode: Approval::Deny,
            prompts: false,
        }
    }

    /// Ask the product surface for each request; an unanswered request is denied.
    pub fn asking() -> Self {
        Self {
            mode: Approval::Deny,
            prompts: true,
        }
    }

    pub fn decision(&self) -> Approval {
        self.mode
    }

    /// Whether each tool request waits for an explicit product decision.
    pub fn prompts(&self) -> bool {
        self.prompts
    }

    /// Record an explicit Allow/Deny decision from the host surface.
    pub fn decide(&mut self, approval: Approval) {
        self.mode = approval;
    }
}

/// How one answered tool request ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAnswer {
    /// The approval decision decided what ran; this is what the runtime was
    /// told.
    Answered(ToolOutcome),
    /// A stop request ended the call while it was running, so the runtime was
    /// told the call was stopped rather than given a result nobody waited for.
    Stopped(ToolOutcome),
}

/// Answer one inbound tool request. An approved request executes through the
/// workspace router; a denied request is an error result, not permission to run.
pub fn answer_tool_request(
    client: &mut RuntimeClient,
    request_id: Value,
    call: &ToolRequest,
    approval: Approval,
    router: &ToolRouter,
) -> Result<ToolOutcome, MarbleError> {
    answer_tool_request_with_note(client, request_id, call, approval, router, None)
}

/// Answer one inbound tool request, telling Core why a denial happened so the
/// model can adapt instead of retrying blindly.
pub fn answer_tool_request_with_note(
    client: &mut RuntimeClient,
    request_id: Value,
    call: &ToolRequest,
    approval: Approval,
    router: &ToolRouter,
    note: Option<&str>,
) -> Result<ToolOutcome, MarbleError> {
    let answer = answer_tool(
        client,
        request_id,
        call,
        approval,
        router,
        &ToolScope::default(),
        note,
        None,
    )?;
    Ok(match answer {
        ToolAnswer::Answered(outcome) => outcome,
        // Without a cancel token there is nothing that can stop a call.
        ToolAnswer::Stopped(outcome) => outcome,
    })
}

/// Answer one inbound tool request, interrupting an approved call that is
/// already executing when the turn is cancelled.
#[allow(clippy::too_many_arguments)]
pub fn answer_tool(
    client: &mut RuntimeClient,
    request_id: Value,
    call: &ToolRequest,
    approval: Approval,
    router: &ToolRouter,
    scope: &ToolScope,
    note: Option<&str>,
    cancel: Option<CancelToken<'_>>,
) -> Result<ToolAnswer, MarbleError> {
    if approval != Approval::Approve {
        let outcome = refusal(call, note);
        answer(client, request_id, &outcome)?;
        return Ok(ToolAnswer::Answered(outcome));
    }
    match execute_interruptibly(router, call, cancel, scope) {
        Some(outcome) => {
            answer(client, request_id, &outcome)?;
            Ok(ToolAnswer::Answered(outcome))
        }
        None => {
            // A stop request landed while this call was running. Core still
            // needs a result for the request it made, so it learns the call was
            // stopped; `pump` then aborts the task as usual.
            let outcome =
                ToolOutcome::error(format!("marble stopped {}: {STOPPED_TOOL}", call.name));
            answer(client, request_id, &outcome)?;
            Ok(ToolAnswer::Stopped(outcome))
        }
    }
}

/// Result text for a request the decision did not approve.
fn refusal(call: &ToolRequest, note: Option<&str>) -> ToolOutcome {
    ToolOutcome::error(match note.filter(|note| !note.is_empty()) {
        Some(note) => format!("marble declined {}: {note}", call.name),
        None => format!("host declined tool {}", call.name),
    })
}

/// Send one outcome back as the result of an inbound tool request.
fn answer(
    client: &mut RuntimeClient,
    request_id: Value,
    outcome: &ToolOutcome,
) -> Result<(), MarbleError> {
    client
        .tool_result(request_id, &outcome.text, outcome.is_error)
        .map_err(MarbleError::Io)
}

/// Execute one approved call through the workspace router while a stop request
/// can still end the wait for it.
pub fn execute_interruptibly(
    router: &ToolRouter,
    call: &ToolRequest,
    cancel: Option<CancelToken<'_>>,
    scope: &ToolScope,
) -> Option<ToolOutcome> {
    let router = router.clone();
    let name = call.name.clone();
    let arguments = call.arguments.clone();
    let scope = scope.clone();
    run_interruptibly(cancel, move || {
        router.execute_scoped(&scope, &name, &arguments)
    })
}

/// Run one tool body, but stop waiting for it as soon as the turn is cancelled.
///
/// A governed call runs on its own thread so a slow filesystem walk cannot hold
/// the pump loop, which is where cancel and product events are observed. `None`
/// means a stop request ended the wait; the body may still be finishing by
/// itself, and its outcome is dropped.
pub fn run_interruptibly(
    cancel: Option<CancelToken<'_>>,
    body: impl FnOnce() -> ToolOutcome + Send + 'static,
) -> Option<ToolOutcome> {
    let Some(cancel) = cancel else {
        return Some(body());
    };
    if cancel.requested.load(Ordering::Relaxed) {
        return None;
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(body());
    });
    loop {
        if cancel.requested.load(Ordering::Relaxed) {
            return None;
        }
        match receiver.recv_timeout(PUMP_INTERVAL) {
            Ok(outcome) => return Some(outcome),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Some(ToolOutcome::error(LOST_TOOL));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    fn token(requested: &AtomicBool) -> CancelToken<'_> {
        CancelToken {
            requested,
            task_id: "task-1",
        }
    }

    #[test]
    fn a_stop_request_ends_the_wait_for_a_running_tool_promptly() {
        let requested = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let outcome = std::thread::scope(|scope| {
            let stopper = Arc::clone(&requested);
            scope.spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                stopper.store(true, Ordering::Relaxed);
            });
            run_interruptibly(Some(token(&requested)), || {
                std::thread::sleep(Duration::from_secs(30));
                ToolOutcome::ok("slow")
            })
        });
        assert_eq!(outcome, None, "a stop request ends the wait undecided");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the wait must end on the stop request, not on the tool: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_stopped_turn_does_not_start_the_tool() {
        let requested = AtomicBool::new(true);
        let ran = Arc::new(AtomicBool::new(false));
        let started = Arc::clone(&ran);
        let outcome = run_interruptibly(Some(token(&requested)), move || {
            started.store(true, Ordering::Relaxed);
            ToolOutcome::ok("slow")
        });
        assert_eq!(outcome, None);
        assert!(
            !ran.load(Ordering::Relaxed),
            "a turn stopped before the call started must not run it"
        );
    }

    #[test]
    fn a_running_tool_still_reports_its_result() {
        let requested = AtomicBool::new(false);
        assert_eq!(
            run_interruptibly(Some(token(&requested)), || ToolOutcome::ok("fast")),
            Some(ToolOutcome::ok("fast"))
        );
        assert_eq!(
            run_interruptibly(None, || ToolOutcome::error("refused")),
            Some(ToolOutcome::error("refused"))
        );
    }

    #[test]
    fn host_policy_defaults_to_fail_closed_deny_while_asking() {
        assert_eq!(HostPolicy::default().decision(), Approval::Deny);
        assert!(HostPolicy::default().prompts());
        assert_eq!(Approval::default(), Approval::Deny);
    }

    #[test]
    fn host_policy_decision_is_selectable() {
        let mut policy = HostPolicy::default();
        assert_eq!(policy.decision(), Approval::Deny);
        policy.decide(Approval::Approve);
        assert_eq!(policy.decision(), Approval::Approve);
        policy.decide(Approval::Deny);
        assert_eq!(policy.decision(), Approval::Deny);

        assert_eq!(HostPolicy::approving().decision(), Approval::Approve);
        assert!(!HostPolicy::approving().prompts());
        assert_eq!(HostPolicy::denying().decision(), Approval::Deny);
        assert!(!HostPolicy::denying().prompts());
        assert!(HostPolicy::asking().prompts());
    }
}
