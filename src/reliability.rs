//! Browser-free reliability scenario contracts.
//!
//! A scenario is an input to the reliability laboratory, not a browser
//! command. Validation is deliberately independent from fixture execution so
//! malformed expectations fail before any browser is started.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const RELIABILITY_SCENARIO_SCHEMA_VERSION: u32 = 1;
pub const RELIABILITY_FIXTURE_SCHEMA_VERSION: u32 = 1;
pub const RELIABILITY_REPLAY_SCHEMA_VERSION: u32 = 1;
const MAX_SCENARIO_ID_BYTES: usize = 128;
const MAX_CATEGORY_BYTES: usize = 128;
const MAX_FIXTURE_BYTES: usize = 256;
const MAX_CAPABILITIES: usize = 32;
const MAX_STEPS: usize = 64;
const MAX_FORBIDDEN_OUTCOMES: usize = 32;
const MAX_SIDE_EFFECT_COUNTERS: usize = 32;
const MAX_DURATION_MS: u64 = 15 * 60 * 1_000;
const MAX_BROWSER_ACTIONS: u32 = 1_024;
const MAX_REPLAY_EVENTS: usize = 1_024;

/// Supported platform labels for deterministic reliability evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReliabilityPlatform {
    LinuxX86_64,
    MacosX86_64,
    MacosArm64,
}

/// Release-blocking outcomes recognized by the reliability laboratory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityForbiddenOutcome {
    WrongTargetExecuted,
    StaleRevisionExecuted,
    AmbiguousTargetSilentlyExecuted,
    NonIdempotentMutationDuplicated,
    FalseWorkflowCompletion,
    UnsafeResumeReplay,
    SecretLeaked,
    CrossProfileKnowledgeLeak,
    UnboundedLoopEscapedBudget,
    PolicyBypassed,
    CheckpointAcceptedAfterIncompatibleDefinitionChange,
    SemanticCacheHidCriticalUnexpectedState,
}

/// Faults that the reliability lab can inject without relying on timing races.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityFaultKind {
    LoseResponse,
    RendererDisconnect,
    BrowserDisconnect,
    DelayedEffect,
    DropEvent,
}

/// Deterministic controls exposed by the checked-in adversarial fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityFixtureControl {
    Reset,
    ReplaceTarget,
    RenameTarget,
    DuplicateTarget,
    ReorderTargets,
    MoveTargetToOtherRegion,
    ShowOverlay,
    MoveTarget,
    DetachFrame,
    ScheduleEffectMarker,
    CommitSubmit,
}

/// Independent oracle exposed by a reliability fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReliabilityFixtureOracle {
    Snapshot,
    SubmitSideEffectCount,
}

/// Versioned manifest for a deterministic browser fixture.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityFixtureManifest {
    pub schema_version: u32,
    pub id: String,
    pub entrypoint: String,
    pub controls: Vec<ReliabilityFixtureControl>,
    pub faults: Vec<ReliabilityFaultKind>,
    pub oracles: Vec<ReliabilityFixtureOracle>,
}

impl ReliabilityFixtureManifest {
    /// Parse and validate one fixture manifest.
    pub fn from_json(input: &str) -> Result<Self, ReliabilityScenarioError> {
        let manifest: Self = serde_json::from_str(input).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("invalid fixture JSON: {error}"))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the fixture identity, controls, faults, and independent oracles.
    pub fn validate(&self) -> Result<(), ReliabilityScenarioError> {
        if self.schema_version != RELIABILITY_FIXTURE_SCHEMA_VERSION {
            return Err(ReliabilityScenarioError::new(
                "schemaVersion",
                format!(
                    "unsupported fixture schema {}; expected {}",
                    self.schema_version, RELIABILITY_FIXTURE_SCHEMA_VERSION
                ),
            ));
        }
        validate_text("id", &self.id, MAX_SCENARIO_ID_BYTES)?;
        validate_text("entrypoint", &self.entrypoint, MAX_FIXTURE_BYTES)?;
        validate_unique("controls", &self.controls)?;
        validate_unique("faults", &self.faults)?;
        validate_unique("oracles", &self.oracles)?;
        if self.controls.is_empty() {
            return Err(ReliabilityScenarioError::new(
                "controls",
                "must expose at least one deterministic control",
            ));
        }
        if self.faults.is_empty() {
            return Err(ReliabilityScenarioError::new(
                "faults",
                "must expose at least one deterministic fault",
            ));
        }
        if self.oracles.is_empty() {
            return Err(ReliabilityScenarioError::new(
                "oracles",
                "must expose at least one independent oracle",
            ));
        }
        Ok(())
    }

    /// Return the stable content hash used to bind fixture evidence.
    pub fn content_hash(&self) -> Result<String, ReliabilityScenarioError> {
        self.validate()?;
        let canonical = serde_json::to_string(self).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("cannot serialize fixture: {error}"))
        })?;
        let digest = Sha256::digest(canonical.as_bytes());
        Ok(format!("sha256:{digest:x}"))
    }

    /// Check that a scenario only uses controls and faults exposed by this fixture.
    pub fn validate_scenario(
        &self,
        scenario: &ReliabilityScenario,
    ) -> Result<(), ReliabilityScenarioError> {
        self.validate()?;
        scenario.validate()?;
        if scenario.fixture != self.id {
            return Err(ReliabilityScenarioError::new(
                "fixture",
                "scenario references a different fixture manifest",
            ));
        }
        for (index, step) in scenario.steps.iter().enumerate() {
            if let Some(control) = step.apply_control
                && !self.controls.contains(&control)
            {
                return Err(ReliabilityScenarioError::new(
                    format!("steps[{index}].applyControl"),
                    "fixture does not expose this control",
                ));
            }
            if let Some(injection) = &step.inject
                && !self.faults.contains(&injection.fault)
            {
                return Err(ReliabilityScenarioError::new(
                    format!("steps[{index}].inject.fault"),
                    "fixture does not expose this fault",
                ));
            }
        }
        Ok(())
    }
}

/// Host and browser metadata attached to a replay bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityRunMetadata {
    pub platform: ReliabilityPlatform,
    pub browser: String,
    pub browser_version: String,
    pub duration_ms: u64,
    pub browser_actions: u32,
}

impl ReliabilityRunMetadata {
    fn validate(
        &self,
        path: &str,
        budgets: &ReliabilityScenarioBudgets,
    ) -> Result<(), ReliabilityScenarioError> {
        validate_text(
            &format!("{path}.browser"),
            &self.browser,
            MAX_CATEGORY_BYTES,
        )?;
        validate_text(
            &format!("{path}.browserVersion"),
            &self.browser_version,
            MAX_CATEGORY_BYTES,
        )?;
        if self.duration_ms == 0 || self.duration_ms > budgets.max_duration_ms {
            return Err(ReliabilityScenarioError::new(
                format!("{path}.durationMs"),
                "must be positive and within the scenario duration budget",
            ));
        }
        if self.browser_actions == 0 || self.browser_actions > budgets.max_browser_actions {
            return Err(ReliabilityScenarioError::new(
                format!("{path}.browserActions"),
                "must be positive and within the scenario action budget",
            ));
        }
        Ok(())
    }
}

/// Redacted replay event. Values and page content are intentionally absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityReplayEvent {
    pub sequence: u32,
    pub operation: String,
    pub result: String,
}

/// Stable comparison result for two replay runs of the same scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityReplayComparison {
    pub scenario_id: String,
    pub equivalent: bool,
    pub changed_fields: Vec<String>,
}

/// Versioned, redacted evidence bundle for replay and regression comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityReplayBundle {
    pub schema_version: u32,
    pub scenario_id: String,
    pub scenario_hash: String,
    pub fixture_id: String,
    pub fixture_hash: String,
    pub events: Vec<ReliabilityReplayEvent>,
    pub observation: ReliabilityScenarioObservation,
}

impl ReliabilityReplayBundle {
    /// Parse and validate one redacted replay bundle against its scenario.
    pub fn from_json(
        input: &str,
        scenario: &ReliabilityScenario,
    ) -> Result<Self, ReliabilityScenarioError> {
        let value = serde_json::from_str(input).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("invalid replay JSON: {error}"))
        })?;
        Self::from_value(value, scenario)
    }

    /// Parse and validate one replay JSON value against its scenario.
    pub fn from_value(
        value: Value,
        scenario: &ReliabilityScenario,
    ) -> Result<Self, ReliabilityScenarioError> {
        let bundle: Self = serde_json::from_value(value).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("invalid replay shape: {error}"))
        })?;
        bundle.validate(scenario)?;
        Ok(bundle)
    }

    /// Validate binding, budgets, event ordering, and redacted evidence shape.
    pub fn validate(&self, scenario: &ReliabilityScenario) -> Result<(), ReliabilityScenarioError> {
        if self.schema_version != RELIABILITY_REPLAY_SCHEMA_VERSION {
            return Err(ReliabilityScenarioError::new(
                "schemaVersion",
                format!(
                    "unsupported replay schema {}; expected {}",
                    self.schema_version, RELIABILITY_REPLAY_SCHEMA_VERSION
                ),
            ));
        }
        validate_text("scenarioId", &self.scenario_id, MAX_SCENARIO_ID_BYTES)?;
        validate_text("fixtureId", &self.fixture_id, MAX_SCENARIO_ID_BYTES)?;
        validate_digest("scenarioHash", &self.scenario_hash)?;
        validate_digest("fixtureHash", &self.fixture_hash)?;
        if self.scenario_id != scenario.id {
            return Err(ReliabilityScenarioError::new(
                "scenarioId",
                "replay bundle is bound to a different scenario",
            ));
        }
        if self.scenario_hash != scenario.content_hash()? {
            return Err(ReliabilityScenarioError::new(
                "scenarioHash",
                "replay bundle is not bound to the exact scenario content",
            ));
        }
        self.observation
            .metadata
            .validate("observation.metadata", &scenario.budgets)?;
        if self.events.is_empty() || self.events.len() > MAX_REPLAY_EVENTS {
            return Err(ReliabilityScenarioError::new(
                "events",
                format!("must contain 1..={MAX_REPLAY_EVENTS} entries"),
            ));
        }
        for (index, event) in self.events.iter().enumerate() {
            if event.sequence != index as u32 {
                return Err(ReliabilityScenarioError::new(
                    format!("events[{index}].sequence"),
                    "must be contiguous and start at zero",
                ));
            }
            validate_text(
                &format!("events[{index}].operation"),
                &event.operation,
                MAX_CATEGORY_BYTES,
            )?;
            validate_redacted_text(&format!("events[{index}].operation"), &event.operation)?;
            validate_text(
                &format!("events[{index}].result"),
                &event.result,
                MAX_CATEGORY_BYTES,
            )?;
            validate_redacted_text(&format!("events[{index}].result"), &event.result)?;
        }
        if self.observation.scenario_id != self.scenario_id
            || self.observation.scenario_hash != self.scenario_hash
        {
            return Err(ReliabilityScenarioError::new(
                "observation",
                "observation identity does not match the replay bundle",
            ));
        }
        Ok(())
    }

    /// Return stable JSON suitable for replay storage and comparison.
    pub fn to_canonical_json(
        &self,
        scenario: &ReliabilityScenario,
    ) -> Result<String, ReliabilityScenarioError> {
        self.validate(scenario)?;
        serde_json::to_string(self).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("cannot serialize replay: {error}"))
        })
    }

    /// Return a stable hash for this validated replay bundle.
    pub fn content_hash(
        &self,
        scenario: &ReliabilityScenario,
    ) -> Result<String, ReliabilityScenarioError> {
        let canonical = self.to_canonical_json(scenario)?;
        let digest = Sha256::digest(canonical.as_bytes());
        Ok(format!("sha256:{digest:x}"))
    }

    /// Compare stable replay fields after validating both bundles.
    pub fn compare(
        &self,
        other: &Self,
        scenario: &ReliabilityScenario,
    ) -> Result<ReliabilityReplayComparison, ReliabilityScenarioError> {
        self.validate(scenario)?;
        other.validate(scenario)?;
        let left = serde_json::to_value(self).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("cannot serialize replay: {error}"))
        })?;
        let right = serde_json::to_value(other).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("cannot serialize replay: {error}"))
        })?;
        let mut changed_fields = Vec::new();
        for field in ["fixtureId", "fixtureHash", "events", "observation"] {
            if left[field] != right[field] {
                changed_fields.push(field.to_string());
            }
        }
        Ok(ReliabilityReplayComparison {
            scenario_id: self.scenario_id.clone(),
            equivalent: changed_fields.is_empty(),
            changed_fields,
        })
    }
}

/// Browser and policy setup for a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScenarioSetup {
    pub browser: String,
    pub policy: String,
}

/// A controlled fault injected during a scenario step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityFaultInjection {
    pub after_dispatch: String,
    pub fault: ReliabilityFaultKind,
}

/// One declarative scenario operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScenarioStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_workflow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply_control: Option<ReliabilityFixtureControl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inject: Option<ReliabilityFaultInjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_from_checkpoint: Option<String>,
}

/// Expected typed outcome and independent side-effect oracle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScenarioExpectation {
    pub terminal_state: String,
    #[serde(default)]
    pub side_effect_count: BTreeMap<String, u64>,
}

/// Resource limits for one scenario execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScenarioBudgets {
    pub max_duration_ms: u64,
    pub max_browser_actions: u32,
}

/// Classification reported by one executed scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityRunClassification {
    Passed,
    Failed,
    SafeRefusal,
    Indeterminate,
    Unsupported,
}

/// Bounded evidence submitted to the forbidden-outcome evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScenarioObservation {
    pub scenario_id: String,
    pub scenario_hash: String,
    pub metadata: ReliabilityRunMetadata,
    pub classification: ReliabilityRunClassification,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_state: Option<String>,
    #[serde(default)]
    pub side_effect_count: BTreeMap<String, u64>,
    #[serde(default)]
    pub forbidden_outcomes: Vec<ReliabilityForbiddenOutcome>,
    pub oracle_evidence: bool,
    pub artifacts_complete: bool,
}

/// One reason certification cannot be granted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityGateFailure {
    pub scenario_id: String,
    pub code: String,
    pub detail: String,
}

/// Release-blocking result over a complete scenario set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityGateReport {
    pub schema_version: u32,
    pub certified: bool,
    pub scenario_count: usize,
    pub passed: usize,
    pub safe_refusals: usize,
    pub indeterminate: usize,
    pub unsupported: usize,
    pub forbidden_outcomes: BTreeMap<ReliabilityForbiddenOutcome, u64>,
    pub failures: Vec<ReliabilityGateFailure>,
}

/// Category-level release summary derived from the exact gate report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScorecardCategory {
    pub category: String,
    pub scenario_count: usize,
    pub certified_count: usize,
    pub blocked_count: usize,
}

/// Inspectable release scorecard. It never replaces the detailed gate report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScorecard {
    pub schema_version: u32,
    pub certified: bool,
    pub categories: Vec<ReliabilityScorecardCategory>,
    pub gate: ReliabilityGateReport,
}

/// Versioned, browser-free reliability scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReliabilityScenario {
    pub schema_version: u32,
    pub id: String,
    pub category: String,
    pub fixture: String,
    #[serde(default = "default_platforms")]
    pub platforms: Vec<ReliabilityPlatform>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub setup: ReliabilityScenarioSetup,
    pub steps: Vec<ReliabilityScenarioStep>,
    pub expect: ReliabilityScenarioExpectation,
    #[serde(default)]
    pub forbid: Vec<ReliabilityForbiddenOutcome>,
    pub budgets: ReliabilityScenarioBudgets,
}

impl ReliabilityScenario {
    /// Parse and validate one JSON scenario.
    pub fn from_json(input: &str) -> Result<Self, ReliabilityScenarioError> {
        let scenario: Self = serde_json::from_str(input).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("invalid scenario JSON: {error}"))
        })?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// Parse and validate one JSON value.
    pub fn from_value(value: Value) -> Result<Self, ReliabilityScenarioError> {
        let scenario: Self = serde_json::from_value(value).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("invalid scenario shape: {error}"))
        })?;
        scenario.validate()?;
        Ok(scenario)
    }

    /// Validate bounds, supported platforms, operations, and outcome oracles.
    pub fn validate(&self) -> Result<(), ReliabilityScenarioError> {
        if self.schema_version != RELIABILITY_SCENARIO_SCHEMA_VERSION {
            return Err(ReliabilityScenarioError::new(
                "schemaVersion",
                format!(
                    "unsupported scenario schema {}; expected {}",
                    self.schema_version, RELIABILITY_SCENARIO_SCHEMA_VERSION
                ),
            ));
        }
        validate_text("id", &self.id, MAX_SCENARIO_ID_BYTES)?;
        validate_text("category", &self.category, MAX_CATEGORY_BYTES)?;
        validate_text("fixture", &self.fixture, MAX_FIXTURE_BYTES)?;
        if self.platforms.is_empty() {
            return Err(ReliabilityScenarioError::new(
                "platforms",
                "must list at least one supported platform",
            ));
        }
        validate_unique("platforms", &self.platforms)?;
        if self.capabilities.len() > MAX_CAPABILITIES {
            return Err(ReliabilityScenarioError::new(
                "capabilities",
                format!("must contain at most {MAX_CAPABILITIES} entries"),
            ));
        }
        for (index, capability) in self.capabilities.iter().enumerate() {
            validate_text(
                &format!("capabilities[{index}]"),
                capability,
                MAX_CATEGORY_BYTES,
            )?;
        }
        validate_text("setup.browser", &self.setup.browser, MAX_CATEGORY_BYTES)?;
        validate_text("setup.policy", &self.setup.policy, MAX_CATEGORY_BYTES)?;
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(ReliabilityScenarioError::new(
                "steps",
                format!("must contain 1..={MAX_STEPS} entries"),
            ));
        }
        for (index, step) in self.steps.iter().enumerate() {
            let path = format!("steps[{index}]");
            let operations = step.run_workflow.is_some() as u8
                + step.apply_control.is_some() as u8
                + step.inject.is_some() as u8
                + step.resume_from_checkpoint.is_some() as u8;
            if operations != 1 {
                return Err(ReliabilityScenarioError::new(
                    path,
                    "provide exactly one of runWorkflow, applyControl, inject, or resumeFromCheckpoint",
                ));
            }
            if let Some(workflow) = &step.run_workflow {
                validate_text(&format!("{path}.runWorkflow"), workflow, MAX_FIXTURE_BYTES)?;
            }
            if let Some(checkpoint) = &step.resume_from_checkpoint {
                validate_text(
                    &format!("{path}.resumeFromCheckpoint"),
                    checkpoint,
                    MAX_FIXTURE_BYTES,
                )?;
            }
            if let Some(injection) = &step.inject {
                validate_text(
                    &format!("{path}.inject.afterDispatch"),
                    &injection.after_dispatch,
                    MAX_SCENARIO_ID_BYTES,
                )?;
            }
        }
        validate_text(
            "expect.terminalState",
            &self.expect.terminal_state,
            MAX_CATEGORY_BYTES,
        )?;
        if self.expect.side_effect_count.len() > MAX_SIDE_EFFECT_COUNTERS {
            return Err(ReliabilityScenarioError::new(
                "expect.sideEffectCount",
                format!("must contain at most {MAX_SIDE_EFFECT_COUNTERS} counters"),
            ));
        }
        for (name, count) in &self.expect.side_effect_count {
            validate_text("expect.sideEffectCount", name, MAX_SCENARIO_ID_BYTES)?;
            if *count > MAX_BROWSER_ACTIONS as u64 {
                return Err(ReliabilityScenarioError::new(
                    "expect.sideEffectCount",
                    format!("counter {name:?} exceeds the action bound"),
                ));
            }
        }
        if self.forbid.len() > MAX_FORBIDDEN_OUTCOMES {
            return Err(ReliabilityScenarioError::new(
                "forbid",
                format!("must contain at most {MAX_FORBIDDEN_OUTCOMES} entries"),
            ));
        }
        if self.forbid.is_empty() {
            return Err(ReliabilityScenarioError::new(
                "forbid",
                "must declare at least one release-blocking outcome",
            ));
        }
        let mut forbidden = BTreeSet::new();
        for outcome in &self.forbid {
            if !forbidden.insert(*outcome) {
                return Err(ReliabilityScenarioError::new(
                    "forbid",
                    "duplicate forbidden outcome",
                ));
            }
        }
        if self.budgets.max_duration_ms == 0 || self.budgets.max_duration_ms > MAX_DURATION_MS {
            return Err(ReliabilityScenarioError::new(
                "budgets.maxDurationMs",
                format!("must be 1..={MAX_DURATION_MS}"),
            ));
        }
        if self.budgets.max_browser_actions == 0
            || self.budgets.max_browser_actions > MAX_BROWSER_ACTIONS
        {
            return Err(ReliabilityScenarioError::new(
                "budgets.maxBrowserActions",
                format!("must be 1..={MAX_BROWSER_ACTIONS}"),
            ));
        }
        Ok(())
    }

    /// Return stable JSON suitable for fixture hashes and report metadata.
    pub fn to_canonical_json(&self) -> Result<String, ReliabilityScenarioError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            ReliabilityScenarioError::new("$", format!("cannot serialize scenario: {error}"))
        })
    }

    /// Return the content hash used to bind observations to this scenario.
    pub fn content_hash(&self) -> Result<String, ReliabilityScenarioError> {
        let canonical = self.to_canonical_json()?;
        let digest = Sha256::digest(canonical.as_bytes());
        Ok(format!("sha256:{digest:x}"))
    }
}

/// Evaluate one complete scenario corpus using independent oracle evidence.
pub fn evaluate_reliability_gate(
    scenarios: &[ReliabilityScenario],
    observations: &[ReliabilityScenarioObservation],
) -> Result<ReliabilityGateReport, ReliabilityScenarioError> {
    let mut scenario_map = BTreeMap::new();
    for scenario in scenarios {
        scenario.validate()?;
        if scenario_map
            .insert(scenario.id.as_str(), scenario)
            .is_some()
        {
            return Err(ReliabilityScenarioError::new(
                "scenarios",
                format!("duplicate scenario ID {:?}", scenario.id),
            ));
        }
    }
    let mut observation_map = BTreeMap::new();
    for observation in observations {
        validate_text(
            "observations.scenarioId",
            &observation.scenario_id,
            MAX_SCENARIO_ID_BYTES,
        )?;
        validate_unique(
            "observations.forbiddenOutcomes",
            &observation.forbidden_outcomes,
        )?;
        if observation_map
            .insert(observation.scenario_id.as_str(), observation)
            .is_some()
        {
            return Err(ReliabilityScenarioError::new(
                "observations",
                format!(
                    "duplicate observation for scenario {:?}",
                    observation.scenario_id
                ),
            ));
        }
    }
    let mut forbidden_outcomes = BTreeMap::new();
    let mut failures = Vec::new();
    let mut passed = 0;
    let mut safe_refusals = 0;
    let mut indeterminate = 0;
    let mut unsupported = 0;

    for scenario in scenarios {
        let Some(observation) = observation_map.get(scenario.id.as_str()) else {
            failures.push(gate_failure(
                &scenario.id,
                "missing_evidence",
                "no observation was provided for the required scenario",
            ));
            continue;
        };
        let expected_hash = scenario.content_hash()?;
        if observation.scenario_hash != expected_hash {
            failures.push(gate_failure(
                &scenario.id,
                "scenario_hash_mismatch",
                "observation is not bound to the exact scenario content",
            ));
        }
        if !observation.oracle_evidence {
            failures.push(gate_failure(
                &scenario.id,
                "missing_oracle_evidence",
                "required independent oracle evidence was not collected",
            ));
        }
        if !observation.artifacts_complete {
            failures.push(gate_failure(
                &scenario.id,
                "incomplete_artifacts",
                "required release evidence artifacts are incomplete",
            ));
        }
        if !scenario.platforms.contains(&observation.metadata.platform) {
            failures.push(gate_failure(
                &scenario.id,
                "unsupported_platform",
                "observation platform is not listed by the scenario",
            ));
        }
        if let Err(error) = observation.metadata.validate("metadata", &scenario.budgets) {
            failures.push(gate_failure(
                &scenario.id,
                "invalid_run_metadata",
                &error.to_string(),
            ));
        }
        match observation.classification {
            ReliabilityRunClassification::Passed => passed += 1,
            ReliabilityRunClassification::SafeRefusal => safe_refusals += 1,
            ReliabilityRunClassification::Indeterminate => indeterminate += 1,
            ReliabilityRunClassification::Unsupported => unsupported += 1,
            ReliabilityRunClassification::Failed => failures.push(gate_failure(
                &scenario.id,
                "scenario_failed",
                "scenario classification is failed",
            )),
        }
        if matches!(
            observation.classification,
            ReliabilityRunClassification::Indeterminate | ReliabilityRunClassification::Unsupported
        ) {
            failures.push(gate_failure(
                &scenario.id,
                "non_certifying_classification",
                "indeterminate and unsupported runs cannot certify a release",
            ));
        }
        if observation.terminal_state.as_deref() != Some(scenario.expect.terminal_state.as_str()) {
            failures.push(gate_failure(
                &scenario.id,
                "terminal_state_mismatch",
                "observed terminal state does not match the declared expectation",
            ));
        }
        if observation.side_effect_count != scenario.expect.side_effect_count {
            failures.push(gate_failure(
                &scenario.id,
                "side_effect_oracle_mismatch",
                "observed side-effect counters do not match the declared expectation",
            ));
        }
        for outcome in &observation.forbidden_outcomes {
            *forbidden_outcomes.entry(*outcome).or_default() += 1;
        }
    }
    for observation in observations {
        if !scenario_map.contains_key(observation.scenario_id.as_str()) {
            failures.push(gate_failure(
                &observation.scenario_id,
                "unexpected_observation",
                "observation does not identify a required scenario",
            ));
        }
    }
    if !forbidden_outcomes.is_empty() {
        failures.push(gate_failure(
            "*",
            "forbidden_outcome",
            "one or more release-blocking forbidden outcomes were observed",
        ));
    }
    let report = ReliabilityGateReport {
        schema_version: RELIABILITY_SCENARIO_SCHEMA_VERSION,
        certified: failures.is_empty(),
        scenario_count: scenarios.len(),
        passed,
        safe_refusals,
        indeterminate,
        unsupported,
        forbidden_outcomes,
        failures,
    };
    Ok(report)
}

/// Build a category summary from the same evidence used by certification.
pub fn build_reliability_scorecard(
    scenarios: &[ReliabilityScenario],
    observations: &[ReliabilityScenarioObservation],
) -> Result<ReliabilityScorecard, ReliabilityScenarioError> {
    let gate = evaluate_reliability_gate(scenarios, observations)?;
    let mut categories: BTreeMap<&str, ReliabilityScorecardCategory> = BTreeMap::new();
    for scenario in scenarios {
        let category = categories
            .entry(scenario.category.as_str())
            .or_insert_with(|| ReliabilityScorecardCategory {
                category: scenario.category.clone(),
                scenario_count: 0,
                certified_count: 0,
                blocked_count: 0,
            });
        category.scenario_count += 1;
        let blocked = gate
            .failures
            .iter()
            .any(|failure| failure.scenario_id == scenario.id || failure.scenario_id == "*");
        if blocked {
            category.blocked_count += 1;
        } else {
            category.certified_count += 1;
        }
    }
    Ok(ReliabilityScorecard {
        schema_version: RELIABILITY_SCENARIO_SCHEMA_VERSION,
        certified: gate.certified,
        categories: categories.into_values().collect(),
        gate,
    })
}

fn gate_failure(scenario_id: &str, code: &str, detail: &str) -> ReliabilityGateFailure {
    ReliabilityGateFailure {
        scenario_id: scenario_id.into(),
        code: code.into(),
        detail: detail.into(),
    }
}

fn default_platforms() -> Vec<ReliabilityPlatform> {
    vec![
        ReliabilityPlatform::LinuxX86_64,
        ReliabilityPlatform::MacosX86_64,
        ReliabilityPlatform::MacosArm64,
    ]
}

fn validate_unique<T: Ord>(path: &str, values: &[T]) -> Result<(), ReliabilityScenarioError> {
    let mut unique = BTreeSet::new();
    for value in values {
        if !unique.insert(value) {
            return Err(ReliabilityScenarioError::new(
                path,
                "must not contain duplicates",
            ));
        }
    }
    Ok(())
}

fn validate_text(path: &str, value: &str, maximum: usize) -> Result<(), ReliabilityScenarioError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ReliabilityScenarioError::new(
            path,
            format!("must contain 1..={maximum} non-control bytes"),
        ));
    }
    Ok(())
}

fn validate_digest(path: &str, value: &str) -> Result<(), ReliabilityScenarioError> {
    if !value.starts_with("sha256:") || value.len() != "sha256:".len() + 64 {
        return Err(ReliabilityScenarioError::new(
            path,
            "must be a sha256 digest",
        ));
    }
    if value["sha256:".len()..]
        .chars()
        .any(|character| !character.is_ascii_hexdigit())
    {
        return Err(ReliabilityScenarioError::new(
            path,
            "must contain only hexadecimal digest characters",
        ));
    }
    Ok(())
}

fn validate_redacted_text(path: &str, value: &str) -> Result<(), ReliabilityScenarioError> {
    let normalized = value.to_ascii_lowercase();
    for marker in [
        "password",
        "secret",
        "cookie",
        "authorization",
        "bearer ",
        "token=",
    ] {
        if normalized.contains(marker) {
            return Err(ReliabilityScenarioError::new(
                path,
                "replay text contains a sensitive-value marker",
            ));
        }
    }
    Ok(())
}

/// A path-aware scenario contract failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReliabilityScenarioError {
    pub path: String,
    pub reason: String,
}

impl ReliabilityScenarioError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for ReliabilityScenarioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for ReliabilityScenarioError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scenario() -> ReliabilityScenario {
        ReliabilityScenario {
            schema_version: RELIABILITY_SCENARIO_SCHEMA_VERSION,
            id: "duplicate-submit".into(),
            category: "transactional-workflow".into(),
            fixture: "checkout-submit".into(),
            platforms: vec![ReliabilityPlatform::LinuxX86_64],
            capabilities: vec!["workflow".into(), "idempotency".into()],
            setup: ReliabilityScenarioSetup {
                browser: "chromium".into(),
                policy: "hardened".into(),
            },
            steps: vec![
                ReliabilityScenarioStep {
                    run_workflow: Some("submit-request.json".into()),
                    apply_control: None,
                    inject: None,
                    resume_from_checkpoint: None,
                },
                ReliabilityScenarioStep {
                    run_workflow: None,
                    apply_control: None,
                    inject: Some(ReliabilityFaultInjection {
                        after_dispatch: "submit".into(),
                        fault: ReliabilityFaultKind::LoseResponse,
                    }),
                    resume_from_checkpoint: None,
                },
                ReliabilityScenarioStep {
                    run_workflow: None,
                    apply_control: None,
                    inject: None,
                    resume_from_checkpoint: Some("latest".into()),
                },
            ],
            expect: ReliabilityScenarioExpectation {
                terminal_state: "completed".into(),
                side_effect_count: BTreeMap::from([(String::from("submit"), 1)]),
            },
            forbid: vec![
                ReliabilityForbiddenOutcome::NonIdempotentMutationDuplicated,
                ReliabilityForbiddenOutcome::FalseWorkflowCompletion,
            ],
            budgets: ReliabilityScenarioBudgets {
                max_duration_ms: 30_000,
                max_browser_actions: 20,
            },
        }
    }

    #[test]
    fn scenario_round_trip_is_canonical_and_bounded() {
        let scenario = scenario();
        let canonical = scenario.to_canonical_json().unwrap();
        let parsed = ReliabilityScenario::from_json(&canonical).unwrap();
        assert_eq!(parsed.id, "duplicate-submit");
        assert!(canonical.contains("nonIdempotentMutationDuplicated"));
    }

    #[test]
    fn replay_bundle_is_redacted_bound_and_ordered() {
        let scenario = scenario();
        let observation = observation(&scenario, ReliabilityRunClassification::Passed, Vec::new());
        let bundle = ReliabilityReplayBundle {
            schema_version: RELIABILITY_REPLAY_SCHEMA_VERSION,
            scenario_id: scenario.id.clone(),
            scenario_hash: scenario.content_hash().unwrap(),
            fixture_id: scenario.fixture.clone(),
            fixture_hash: format!("sha256:{}", "a".repeat(64)),
            events: vec![ReliabilityReplayEvent {
                sequence: 0,
                operation: "runWorkflow".into(),
                result: "committed".into(),
            }],
            observation,
        };
        let canonical = bundle.to_canonical_json(&scenario).unwrap();
        let parsed = ReliabilityReplayBundle::from_json(&canonical, &scenario).unwrap();
        assert_eq!(parsed.events[0].sequence, 0);
        assert!(!canonical.contains("secret"));

        let mut changed = parsed.clone();
        changed.events[0].result = "refused".into();
        let comparison = parsed.compare(&changed, &scenario).unwrap();
        assert!(!comparison.equivalent);
        assert_eq!(comparison.changed_fields, vec!["events"]);
    }

    #[test]
    fn replay_bundle_rejects_non_contiguous_events() {
        let scenario = scenario();
        let mut observation =
            observation(&scenario, ReliabilityRunClassification::Passed, Vec::new());
        observation.scenario_hash = scenario.content_hash().unwrap();
        let bundle = ReliabilityReplayBundle {
            schema_version: RELIABILITY_REPLAY_SCHEMA_VERSION,
            scenario_id: scenario.id.clone(),
            scenario_hash: scenario.content_hash().unwrap(),
            fixture_id: scenario.fixture.clone(),
            fixture_hash: format!("sha256:{}", "b".repeat(64)),
            events: vec![ReliabilityReplayEvent {
                sequence: 1,
                operation: "runWorkflow".into(),
                result: "committed".into(),
            }],
            observation,
        };
        let error = bundle.validate(&scenario).unwrap_err();
        assert_eq!(error.path, "events[0].sequence");
    }

    #[test]
    fn replay_bundle_rejects_sensitive_event_markers() {
        let scenario = scenario();
        let observation = observation(&scenario, ReliabilityRunClassification::Passed, Vec::new());
        let bundle = ReliabilityReplayBundle {
            schema_version: RELIABILITY_REPLAY_SCHEMA_VERSION,
            scenario_id: scenario.id.clone(),
            scenario_hash: scenario.content_hash().unwrap(),
            fixture_id: scenario.fixture.clone(),
            fixture_hash: format!("sha256:{}", "c".repeat(64)),
            events: vec![ReliabilityReplayEvent {
                sequence: 0,
                operation: "observe".into(),
                result: "secret=redacted".into(),
            }],
            observation,
        };
        let error = bundle.validate(&scenario).unwrap_err();
        assert_eq!(error.path, "events[0].result");
    }

    #[test]
    fn scenario_rejects_multiple_operations_in_one_step() {
        let mut value = serde_json::to_value(scenario()).unwrap();
        value["steps"][0]["inject"] = json!({
            "afterDispatch": "submit",
            "fault": "loseResponse"
        });
        let error = ReliabilityScenario::from_value(value).unwrap_err();
        assert_eq!(error.path, "steps[0]");
    }

    #[test]
    fn scenario_rejects_windows_and_duplicate_forbidden_outcomes() {
        let mut value = serde_json::to_value(scenario()).unwrap();
        value["platforms"] = json!(["windows-x86-64"]);
        assert!(ReliabilityScenario::from_value(value).is_err());

        let mut value = serde_json::to_value(scenario()).unwrap();
        value["forbid"] = json!(["secretLeaked", "secretLeaked"]);
        let error = ReliabilityScenario::from_value(value).unwrap_err();
        assert_eq!(error.path, "forbid");

        let mut value = serde_json::to_value(scenario()).unwrap();
        value["forbid"] = json!([]);
        let error = ReliabilityScenario::from_value(value).unwrap_err();
        assert_eq!(error.path, "forbid");
    }

    fn observation(
        scenario: &ReliabilityScenario,
        classification: ReliabilityRunClassification,
        forbidden_outcomes: Vec<ReliabilityForbiddenOutcome>,
    ) -> ReliabilityScenarioObservation {
        ReliabilityScenarioObservation {
            scenario_id: scenario.id.clone(),
            scenario_hash: scenario.content_hash().unwrap(),
            metadata: ReliabilityRunMetadata {
                platform: ReliabilityPlatform::LinuxX86_64,
                browser: "chromium".into(),
                browser_version: "stable".into(),
                duration_ms: 100,
                browser_actions: 3,
            },
            classification,
            terminal_state: Some(scenario.expect.terminal_state.clone()),
            side_effect_count: scenario.expect.side_effect_count.clone(),
            forbidden_outcomes,
            oracle_evidence: true,
            artifacts_complete: true,
        }
    }

    #[test]
    fn reliability_gate_certifies_complete_clean_evidence() {
        let scenario = scenario();
        let report = evaluate_reliability_gate(
            std::slice::from_ref(&scenario),
            &[observation(
                &scenario,
                ReliabilityRunClassification::Passed,
                Vec::new(),
            )],
        )
        .unwrap();
        assert!(report.certified);
        assert_eq!(report.passed, 1);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn reliability_scorecard_preserves_gate_and_groups_categories() {
        let scenario = scenario();
        let scorecard = build_reliability_scorecard(
            std::slice::from_ref(&scenario),
            &[observation(
                &scenario,
                ReliabilityRunClassification::Passed,
                Vec::new(),
            )],
        )
        .unwrap();
        assert!(scorecard.certified);
        assert_eq!(scorecard.gate.passed, 1);
        assert_eq!(scorecard.categories[0].category, "transactional-workflow");
        assert_eq!(scorecard.categories[0].certified_count, 1);
        assert_eq!(scorecard.categories[0].blocked_count, 0);
    }

    #[test]
    fn reliability_gate_blocks_forbidden_outcomes_and_missing_evidence() {
        let scenario = scenario();
        let mut failed = observation(
            &scenario,
            ReliabilityRunClassification::SafeRefusal,
            vec![ReliabilityForbiddenOutcome::SecretLeaked],
        );
        failed.artifacts_complete = false;
        let report = evaluate_reliability_gate(std::slice::from_ref(&scenario), &[failed]).unwrap();
        assert!(!report.certified);
        assert_eq!(
            report.forbidden_outcomes[&ReliabilityForbiddenOutcome::SecretLeaked],
            1
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == "incomplete_artifacts")
        );
    }

    #[test]
    fn reliability_gate_blocks_non_certifying_platform_and_classification() {
        let scenario = scenario();
        let mut unsupported = observation(
            &scenario,
            ReliabilityRunClassification::Indeterminate,
            Vec::new(),
        );
        unsupported.metadata.platform = ReliabilityPlatform::MacosArm64;
        unsupported.metadata.duration_ms = scenario.budgets.max_duration_ms + 1;
        let report =
            evaluate_reliability_gate(std::slice::from_ref(&scenario), &[unsupported]).unwrap();
        assert!(!report.certified);
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == "unsupported_platform")
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == "non_certifying_classification")
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.code == "invalid_run_metadata")
        );
    }

    #[test]
    fn reliability_gate_rejects_duplicate_observed_forbidden_outcomes() {
        let scenario = scenario();
        let mut observation = observation(
            &scenario,
            ReliabilityRunClassification::Passed,
            vec![ReliabilityForbiddenOutcome::SecretLeaked],
        );
        observation
            .forbidden_outcomes
            .push(ReliabilityForbiddenOutcome::SecretLeaked);
        let error = evaluate_reliability_gate(&[scenario], &[observation]).unwrap_err();
        assert_eq!(error.path, "observations.forbiddenOutcomes");
    }
}
