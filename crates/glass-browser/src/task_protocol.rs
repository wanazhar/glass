//! Strict, bounded authored tasks for deterministic Web IR compilation.
//!
//! This module defines the input boundary only. Validation is side-effect free;
//! compilation and browser execution remain separate follow-on layers.

use crate::web_ir::WebIrEntityKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Version of the authored Task Protocol contract.
pub const TASK_PROTOCOL_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_INPUTS: usize = 64;
pub(crate) const MAX_INPUT_NAME_BYTES: usize = 64;
const MAX_INPUT_VALUE_BYTES: usize = 4_096;
pub(crate) const MAX_POSTCONDITIONS: usize = 32;
pub(crate) const MAX_EXPECTATION_BYTES: usize = 256;
const MAX_ACTIONS: u32 = 256;
const MAX_TIMEOUT_MS: u64 = 120_000;
const MAX_ITEMS: u32 = 4_096;

/// Supported deterministic task families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskKind {
    #[serde(rename = "form.inspect")]
    FormInspect,
    #[serde(rename = "form.fill")]
    FormFill,
    #[serde(rename = "form.validate")]
    FormValidate,
    #[serde(rename = "form.submit")]
    FormSubmit,
    #[serde(rename = "navigation.follow")]
    NavigationFollow,
    #[serde(rename = "navigation.selectTab")]
    NavigationSelectTab,
    #[serde(rename = "navigation.openMenu")]
    NavigationOpenMenu,
    #[serde(rename = "table.extract")]
    TableExtract,
    #[serde(rename = "collection.extract")]
    CollectionExtract,
    #[serde(rename = "region.extract")]
    RegionExtract,
    #[serde(rename = "field.read")]
    FieldRead,
    #[serde(rename = "dialog.inspect")]
    DialogInspect,
    #[serde(rename = "dialog.confirm")]
    DialogConfirm,
    #[serde(rename = "dialog.cancel")]
    DialogCancel,
    #[serde(rename = "pagination.next")]
    PaginationNext,
    #[serde(rename = "pagination.collect")]
    PaginationCollect,
}

/// Effect class declared by the task author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskRiskClass {
    ReadOnly,
    LocalMutation,
    RemoteReversible,
    RemoteIrreversible,
    Authentication,
    DataDisclosure,
    UnknownRisk,
}

/// Behavior when semantic resolution returns more than one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TaskAmbiguityPolicy {
    #[default]
    Fail,
    RequireConfirmation,
}

/// Revision policy requested by the task author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TaskRevisionPolicy {
    #[default]
    Exact,
    Compatible,
    Reextract,
}

/// Typed postcondition families reserved for the compiler and verifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskPostconditionKind {
    PageKind,
    RegionPresent,
    EntityState,
    NavigationOccurred,
    DialogClosed,
    ValidationClear,
    RecordsExtracted,
}

/// Semantic task scope; it contains no selectors or browser handles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_kind: Option<WebIrEntityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_name: Option<String>,
}

/// Bounded execution limits declared before compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskLimits {
    pub max_actions: u32,
    pub timeout_ms: u64,
    pub max_items: u32,
}

impl Default for TaskLimits {
    fn default() -> Self {
        Self {
            max_actions: 16,
            timeout_ms: 15_000,
            max_items: 128,
        }
    }
}

/// One typed success condition for a compiled task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskPostcondition {
    pub kind: TaskPostconditionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

impl TaskPostcondition {
    pub(crate) fn validate_at(&self, index: usize) -> Result<(), TaskProtocolError> {
        if let Some(expected) = &self.expected
            && (expected.len() > MAX_EXPECTATION_BYTES || expected.chars().any(char::is_control))
        {
            return Err(TaskProtocolError::new(
                format!("postconditions[{index}].expected"),
                "expected value exceeds its bound or contains a control character",
            ));
        }
        match self.kind {
            TaskPostconditionKind::RegionPresent => {
                let Some(expected) = self.expected.as_deref() else {
                    return Err(TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "regionPresent requires the expected semantic region name",
                    ));
                };
                validate_text(
                    &format!("postconditions[{index}].expected"),
                    expected,
                    MAX_EXPECTATION_BYTES,
                )?;
            }
            TaskPostconditionKind::PageKind => {
                if let Some(expected) = self.expected.as_deref() {
                    validate_text(
                        &format!("postconditions[{index}].expected"),
                        expected,
                        MAX_EXPECTATION_BYTES,
                    )?;
                }
            }
            TaskPostconditionKind::EntityState => {
                let Some(expected) = self.expected.as_deref() else {
                    return Err(TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "entityState requires '<entity-name>.<state>=<true|false>'",
                    ));
                };
                let Some((selector, expected_value)) = expected.rsplit_once('=') else {
                    return Err(TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "entityState requires '<entity-name>.<state>=<true|false>'",
                    ));
                };
                let Some((entity_name, state)) = selector.rsplit_once('.') else {
                    return Err(TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "entityState requires '<entity-name>.<state>=<true|false>'",
                    ));
                };
                if entity_name.trim().is_empty()
                    || !matches!(
                        state,
                        "disabled" | "readOnly" | "required" | "checked" | "empty"
                    )
                    || !matches!(expected_value, "true" | "false")
                {
                    return Err(TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "entityState selector, state, or boolean value is invalid",
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }
}

/// Versioned declarative task input accepted by Glass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlassTask {
    pub schema_version: u32,
    pub task: TaskKind,
    pub scope: TaskScope,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    pub limits: TaskLimits,
    pub risk: TaskRiskClass,
    #[serde(default)]
    pub ambiguity: TaskAmbiguityPolicy,
    #[serde(default)]
    pub revision: TaskRevisionPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postconditions: Vec<TaskPostcondition>,
}

impl GlassTask {
    /// Parse strict authored JSON into a task.
    pub fn from_json(input: &str) -> Result<Self, TaskProtocolError> {
        let task: Self = serde_json::from_str(input)
            .map_err(|error| TaskProtocolError::new("$", error.to_string()))?;
        task.validate()?;
        Ok(task)
    }

    /// Validate the authored task without browser access or mutation.
    pub fn validate(&self) -> Result<(), TaskProtocolError> {
        if self.risk == TaskRiskClass::UnknownRisk {
            return Err(TaskProtocolError::new(
                "risk",
                "unknown risk cannot be compiled; declare a bounded risk class",
            ));
        }

        if self.schema_version != TASK_PROTOCOL_SCHEMA_VERSION {
            return Err(TaskProtocolError::new(
                "schemaVersion",
                "unsupported Task Protocol schema version",
            ));
        }
        self.scope.validate()?;
        self.scope.validate_for_task(self.task)?;
        self.limits.validate()?;
        if self.inputs.len() > MAX_INPUTS {
            return Err(TaskProtocolError::new(
                "inputs",
                "input count exceeds the Task Protocol bound",
            ));
        }
        for (name, value) in &self.inputs {
            validate_text("inputs.name", name, MAX_INPUT_NAME_BYTES)?;
            if value.len() > MAX_INPUT_VALUE_BYTES || value.chars().any(char::is_control) {
                return Err(TaskProtocolError::new(
                    format!("inputs.{name}"),
                    "input value exceeds its bound or contains a control character",
                ));
            }
        }
        validate_input_contract(self.task, &self.inputs)?;

        if self.postconditions.len() > MAX_POSTCONDITIONS {
            return Err(TaskProtocolError::new(
                "postconditions",
                "postcondition count exceeds the Task Protocol bound",
            ));
        }
        for (index, postcondition) in self.postconditions.iter().enumerate() {
            if !postcondition_allowed_for(self.task, postcondition.kind) {
                return Err(TaskProtocolError::new(
                    format!("postconditions[{index}].kind"),
                    "postcondition kind is incompatible with the task family",
                ));
            }
            postcondition.validate_at(index)?;
            if postcondition.kind == TaskPostconditionKind::RecordsExtracted
                && let Some(expected) = postcondition.expected.as_deref()
            {
                let minimum = expected.parse::<u32>().map_err(|_| {
                    TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "recordsExtracted expected must be a non-negative integer",
                    )
                })?;
                if minimum > self.limits.max_items {
                    return Err(TaskProtocolError::new(
                        format!("postconditions[{index}].expected"),
                        "recordsExtracted expected exceeds task maxItems",
                    ));
                }
            }
        }
        if matches!(self.task, TaskKind::FormSubmit) && self.postconditions.is_empty() {
            return Err(TaskProtocolError::new(
                "postconditions",
                "form.submit requires at least one bounded postcondition",
            ));
        }
        if matches!(self.task, TaskKind::FormFill) && self.inputs.is_empty() {
            return Err(TaskProtocolError::new(
                "inputs",
                "form.fill requires at least one bounded input",
            ));
        }
        if matches!(
            self.task,
            TaskKind::FormInspect
                | TaskKind::FormFill
                | TaskKind::FormValidate
                | TaskKind::FormSubmit
                | TaskKind::FieldRead
                | TaskKind::NavigationSelectTab
                | TaskKind::PaginationNext
                | TaskKind::NavigationOpenMenu
                | TaskKind::PaginationCollect
                | TaskKind::TableExtract
                | TaskKind::CollectionExtract
                | TaskKind::RegionExtract
        ) && self.scope.region_name.is_none()
        {
            return Err(TaskProtocolError::new(
                "scope.regionName",
                "browser-backed task requires a semantic region scope",
            ));
        }

        match self.task {
            TaskKind::FormSubmit if !self.inputs.contains_key("submit") => {
                return Err(TaskProtocolError::new(
                    "inputs.submit",
                    "form.submit requires the semantic submit target in inputs.submit",
                ));
            }
            TaskKind::FormSubmit if self.inputs.keys().any(|name| name != "submit") => {
                return Err(TaskProtocolError::new(
                    "inputs",
                    "form.submit accepts only the submit target input",
                ));
            }
            TaskKind::NavigationOpenMenu if !self.inputs.contains_key("menu") => {
                return Err(TaskProtocolError::new(
                    "inputs.menu",
                    "navigation.openMenu requires a bounded menu input",
                ));
            }
            TaskKind::NavigationSelectTab if !self.inputs.contains_key("tab") => {
                return Err(TaskProtocolError::new(
                    "inputs.tab",
                    "navigation.selectTab requires a bounded tab input",
                ));
            }
            TaskKind::PaginationCollect if !self.inputs.contains_key("next") => {
                return Err(TaskProtocolError::new(
                    "inputs.next",
                    "pagination.collect requires a bounded next control input",
                ));
            }
            TaskKind::PaginationNext if !self.inputs.contains_key("next") => {
                return Err(TaskProtocolError::new(
                    "inputs.next",
                    "pagination.next requires a bounded next control input",
                ));
            }
            TaskKind::NavigationFollow if !self.inputs.contains_key("url") => {
                return Err(TaskProtocolError::new(
                    "inputs.url",
                    "navigation.follow requires a bounded url input",
                ));
            }
            TaskKind::FieldRead if !self.inputs.contains_key("field") => {
                return Err(TaskProtocolError::new(
                    "inputs.field",
                    "field.read requires the semantic field name in inputs.field",
                ));
            }
            _ => {}
        }
        Ok(())
    }

    /// Serialize a validated task deterministically.
    pub fn to_canonical_json(&self) -> Result<String, TaskProtocolError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| TaskProtocolError::new("$", error.to_string()))
    }
}

impl TaskLimits {
    pub(crate) fn validate(&self) -> Result<(), TaskProtocolError> {
        if !(1..=MAX_ACTIONS).contains(&self.max_actions) {
            return Err(TaskProtocolError::new(
                "limits.maxActions",
                "maxActions must be between 1 and 256",
            ));
        }
        if !(1..=MAX_TIMEOUT_MS).contains(&self.timeout_ms) {
            return Err(TaskProtocolError::new(
                "limits.timeoutMs",
                "timeoutMs must be between 1 and 120000",
            ));
        }
        if !(1..=MAX_ITEMS).contains(&self.max_items) {
            return Err(TaskProtocolError::new(
                "limits.maxItems",
                "maxItems must be between 1 and 4096",
            ));
        }
        Ok(())
    }
}

impl TaskScope {
    pub(crate) fn validate(&self) -> Result<(), TaskProtocolError> {
        if self.region_name.is_none() && self.entity_kind.is_none() && self.entity_name.is_none() {
            return Err(TaskProtocolError::new(
                "scope",
                "scope requires a semantic region or entity constraint",
            ));
        }
        if let Some(region_name) = &self.region_name {
            validate_text("scope.regionName", region_name, 128)?;
        }
        if let Some(entity_name) = &self.entity_name {
            validate_text("scope.entityName", entity_name, 128)?;
        }
        if self.entity_name.is_some() && self.entity_kind.is_none() {
            return Err(TaskProtocolError::new(
                "scope.entityKind",
                "entityName requires entityKind",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_for_task(&self, task: TaskKind) -> Result<(), TaskProtocolError> {
        if task == TaskKind::RegionExtract {
            if let Some(actual) = self.entity_kind
                && !matches!(
                    actual,
                    WebIrEntityKind::Region
                        | WebIrEntityKind::Form
                        | WebIrEntityKind::Table
                        | WebIrEntityKind::Collection
                        | WebIrEntityKind::Dialog
                )
            {
                return Err(TaskProtocolError::new(
                    "scope.entityKind",
                    "entity kind is incompatible with region extraction",
                ));
            }
            return Ok(());
        }
        let expected_kind = match task {
            TaskKind::FormInspect
            | TaskKind::FormFill
            | TaskKind::FormValidate
            | TaskKind::FormSubmit => WebIrEntityKind::Form,
            TaskKind::FieldRead => WebIrEntityKind::Field,
            TaskKind::NavigationSelectTab => WebIrEntityKind::Tab,
            TaskKind::NavigationOpenMenu => WebIrEntityKind::NavigationItem,
            TaskKind::TableExtract => WebIrEntityKind::Table,
            TaskKind::CollectionExtract => WebIrEntityKind::Collection,
            TaskKind::RegionExtract => WebIrEntityKind::Region,
            TaskKind::DialogInspect | TaskKind::DialogConfirm | TaskKind::DialogCancel => {
                WebIrEntityKind::Dialog
            }
            TaskKind::PaginationNext | TaskKind::PaginationCollect => {
                WebIrEntityKind::PaginationControl
            }
            TaskKind::NavigationFollow => WebIrEntityKind::Page,
        };
        if let Some(actual) = self.entity_kind
            && actual != expected_kind
        {
            return Err(TaskProtocolError::new(
                "scope.entityKind",
                "entity kind is incompatible with the task family",
            ));
        }
        Ok(())
    }
}

pub(crate) fn postcondition_allowed_for(task: TaskKind, kind: TaskPostconditionKind) -> bool {
    use TaskPostconditionKind::*;
    match task {
        TaskKind::FormInspect | TaskKind::FormFill | TaskKind::FormValidate => {
            matches!(
                kind,
                ValidationClear | RegionPresent | PageKind | EntityState
            )
        }
        TaskKind::FormSubmit => matches!(
            kind,
            NavigationOccurred
                | ValidationClear
                | RegionPresent
                | PageKind
                | DialogClosed
                | EntityState
        ),
        TaskKind::NavigationFollow => matches!(kind, NavigationOccurred | RegionPresent | PageKind),
        TaskKind::NavigationSelectTab | TaskKind::NavigationOpenMenu => {
            matches!(kind, RegionPresent | PageKind | EntityState)
        }

        TaskKind::TableExtract | TaskKind::CollectionExtract | TaskKind::RegionExtract => {
            matches!(kind, RecordsExtracted | RegionPresent | PageKind)
        }
        TaskKind::FieldRead => matches!(kind, RegionPresent | PageKind | EntityState),
        TaskKind::DialogInspect => matches!(kind, DialogClosed | RegionPresent | PageKind),
        TaskKind::DialogConfirm | TaskKind::DialogCancel => {
            matches!(kind, DialogClosed | PageKind)
        }
        TaskKind::PaginationNext => matches!(
            kind,
            NavigationOccurred | RecordsExtracted | RegionPresent | PageKind
        ),
        TaskKind::PaginationCollect => {
            matches!(kind, RecordsExtracted | RegionPresent | PageKind)
        }
    }
}
fn validate_input_contract(
    task: TaskKind,
    inputs: &BTreeMap<String, String>,
) -> Result<(), TaskProtocolError> {
    let expected = match task {
        TaskKind::FormFill => return Ok(()),
        TaskKind::FormSubmit => &["submit"][..],
        TaskKind::NavigationFollow => &["url"][..],
        TaskKind::NavigationSelectTab => &["tab"][..],
        TaskKind::NavigationOpenMenu => &["menu"][..],
        TaskKind::FieldRead => &["field"][..],
        TaskKind::PaginationNext | TaskKind::PaginationCollect => &["next"][..],
        TaskKind::FormInspect
        | TaskKind::FormValidate
        | TaskKind::TableExtract
        | TaskKind::CollectionExtract
        | TaskKind::RegionExtract
        | TaskKind::DialogInspect
        | TaskKind::DialogConfirm
        | TaskKind::DialogCancel => &[][..],
    };
    if expected.len() == 1 && !inputs.contains_key(expected[0]) {
        return Ok(());
    }

    let names = inputs.keys().map(String::as_str).collect::<Vec<_>>();
    if names.as_slice() != expected {
        return Err(TaskProtocolError::new(
            "inputs",
            "input names are incompatible with the task family",
        ));
    }
    Ok(())
}

fn validate_text(path: &str, value: &str, max_bytes: usize) -> Result<(), TaskProtocolError> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(TaskProtocolError::new(
            path,
            "value must be non-empty, bounded, and free of control characters",
        ));
    }
    Ok(())
}

/// Path-aware Task Protocol validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskProtocolError {
    pub path: String,
    pub reason: String,
}

impl TaskProtocolError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl Display for TaskProtocolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl Error for TaskProtocolError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task() -> GlassTask {
        GlassTask {
            schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
            task: TaskKind::FormFill,
            scope: TaskScope {
                region_name: Some("Shipping address".into()),
                entity_kind: Some(WebIrEntityKind::Form),
                entity_name: None,
            },
            inputs: BTreeMap::from([(String::from("city"), String::from("Kuching"))]),
            limits: TaskLimits::default(),
            risk: TaskRiskClass::LocalMutation,
            ambiguity: TaskAmbiguityPolicy::Fail,
            revision: TaskRevisionPolicy::Exact,
            postconditions: vec![TaskPostcondition {
                kind: TaskPostconditionKind::ValidationClear,
                expected: None,
            }],
        }
    }

    #[test]
    fn valid_task_round_trips_canonically() {
        let task = task();
        let first = task.to_canonical_json().unwrap();
        let second = GlassTask::from_json(&first)
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(first, second);
        assert!(first.contains("form.fill"));
        assert!(first.contains("Shipping"));
    }

    #[test]
    fn authored_json_rejects_unknown_fields() {
        let mut value = serde_json::to_value(task()).unwrap();
        value["futureField"] = json!(true);
        let error = GlassTask::from_json(&value.to_string()).unwrap_err();
        assert_eq!(error.path, "$");
    }

    #[test]
    fn validation_rejects_empty_scope_and_unbounded_limits() {
        let mut invalid = task();
        invalid.scope = TaskScope::default();
        assert_eq!(invalid.validate().unwrap_err().path, "scope");
        invalid = task();
        invalid.limits.max_actions = 0;
        assert_eq!(invalid.validate().unwrap_err().path, "limits.maxActions");
        invalid = task();
        invalid.limits.timeout_ms = MAX_TIMEOUT_MS + 1;
        assert_eq!(invalid.validate().unwrap_err().path, "limits.timeoutMs");
    }

    #[test]
    fn form_fill_requires_inputs_and_entity_names_require_kinds() {
        let mut invalid = task();
        invalid.inputs.clear();
        assert_eq!(invalid.validate().unwrap_err().path, "inputs");
        invalid = task();
        invalid.scope.entity_name = Some("Email".into());
        invalid.scope.entity_kind = None;
        assert_eq!(invalid.validate().unwrap_err().path, "scope.entityKind");
    }

    #[test]
    fn field_read_requires_semantic_field_input() {
        let mut invalid = task();
        invalid.postconditions.clear();
        invalid.task = TaskKind::FieldRead;
        invalid.scope.entity_kind = Some(WebIrEntityKind::Field);
        assert_eq!(invalid.validate().unwrap_err().path, "inputs.field");
    }

    #[test]
    fn entity_state_postconditions_require_a_bounded_boolean_predicate() {
        let mut authored = task();
        authored.postconditions = vec![TaskPostcondition {
            kind: TaskPostconditionKind::EntityState,
            expected: Some("Email.disabled=false".into()),
        }];
        authored.validate().unwrap();

        for invalid in [
            "Email.disabled",
            "Email.value=private",
            "Email.unknown=true",
            ".disabled=true",
        ] {
            authored.postconditions[0].expected = Some(invalid.into());
            assert_eq!(
                authored.validate().unwrap_err().path,
                "postconditions[0].expected"
            );
        }
    }

    #[test]
    fn browser_backed_tasks_require_region_scope() {
        let mut invalid = task();
        invalid.postconditions.clear();
        invalid.task = TaskKind::FieldRead;
        invalid.scope.entity_kind = Some(WebIrEntityKind::Field);
        invalid.scope.region_name = None;
        assert_eq!(invalid.validate().unwrap_err().path, "scope.regionName");
    }

    #[test]
    fn navigation_follow_requires_bounded_url_input() {
        let mut task = task();
        task.postconditions.clear();
        task.task = TaskKind::NavigationFollow;
        task.scope.entity_kind = Some(WebIrEntityKind::Page);
        task.inputs.clear();
        assert_eq!(task.validate().unwrap_err().path, "inputs.url");
        task.inputs
            .insert("url".into(), "https://example.test/next".into());
        task.validate().unwrap();
        task.task = TaskKind::NavigationSelectTab;
        task.scope.entity_kind = Some(WebIrEntityKind::Tab);
        task.inputs.clear();
        assert_eq!(task.validate().unwrap_err().path, "inputs.tab");
        task.inputs.insert("tab".into(), "Payment".into());
        task.validate().unwrap();
        task.task = TaskKind::NavigationOpenMenu;
        task.scope.entity_kind = Some(WebIrEntityKind::NavigationItem);
        task.inputs.clear();
        assert_eq!(task.validate().unwrap_err().path, "inputs.menu");
        task.inputs.insert("menu".into(), "Products".into());
        task.validate().unwrap();
        task.task = TaskKind::PaginationNext;
        task.scope.entity_kind = Some(WebIrEntityKind::PaginationControl);
        task.inputs.clear();
        assert_eq!(task.validate().unwrap_err().path, "inputs.next");
        task.inputs.insert("next".into(), "Next page".into());
        task.validate().unwrap();
    }
    #[test]
    fn pagination_collect_requires_bounded_next_input() {
        let mut task = task();
        task.postconditions.clear();
        task.task = TaskKind::PaginationCollect;
        task.scope.entity_kind = Some(WebIrEntityKind::PaginationControl);
        task.inputs.clear();
        assert_eq!(task.validate().unwrap_err().path, "inputs.next");
        task.inputs.insert("next".into(), "Next page".into());
        task.validate().unwrap();
    }

    #[test]
    fn region_extract_accepts_structural_region_kinds() {
        for entity_kind in [
            WebIrEntityKind::Region,
            WebIrEntityKind::Form,
            WebIrEntityKind::Table,
            WebIrEntityKind::Collection,
            WebIrEntityKind::Dialog,
        ] {
            let mut authored = task();
            authored.task = TaskKind::RegionExtract;
            authored.scope.entity_kind = Some(entity_kind);
            authored.inputs.clear();
            authored.postconditions = vec![TaskPostcondition {
                kind: TaskPostconditionKind::RecordsExtracted,
                expected: Some("0".into()),
            }];
            authored.validate().unwrap();
        }
    }

    #[test]
    fn form_submit_requires_a_postcondition() {
        let mut task = task();
        task.task = TaskKind::FormSubmit;
        task.inputs = BTreeMap::from([(String::from("submit"), String::from("Submit"))]);
        task.postconditions.clear();
        assert_eq!(task.validate().unwrap_err().path, "postconditions");
        task.postconditions.push(TaskPostcondition {
            kind: TaskPostconditionKind::NavigationOccurred,
            expected: None,
        });
        task.validate().unwrap();
    }

    #[test]
    fn unknown_risk_is_rejected_fail_closed() {
        let mut invalid = task();
        invalid.risk = TaskRiskClass::UnknownRisk;
        assert_eq!(invalid.validate().unwrap_err().path, "risk");
    }

    #[test]
    fn task_families_reject_ignored_input_names() {
        let mut invalid = task();
        invalid.task = TaskKind::FormInspect;
        invalid.inputs = BTreeMap::from([(String::from("ignored"), String::from("value"))]);
        assert_eq!(invalid.validate().unwrap_err().path, "inputs");
    }

    #[test]
    fn region_postconditions_require_a_verifiable_name() {
        let mut invalid = task();
        invalid.postconditions = vec![TaskPostcondition {
            kind: TaskPostconditionKind::RegionPresent,
            expected: None,
        }];
        assert_eq!(
            invalid.validate().unwrap_err().path,
            "postconditions[0].expected"
        );
    }

    #[test]
    fn navigation_and_dialog_tasks_allow_non_region_semantic_scopes() {
        let mut navigation = task();
        navigation.task = TaskKind::NavigationFollow;
        navigation.scope.region_name = None;
        navigation.scope.entity_kind = Some(WebIrEntityKind::Page);
        navigation.inputs = BTreeMap::from([(
            String::from("url"),
            String::from("https://example.test/next"),
        )]);
        navigation.postconditions = vec![TaskPostcondition {
            kind: TaskPostconditionKind::NavigationOccurred,
            expected: None,
        }];
        navigation.validate().unwrap();

        for kind in [
            TaskKind::DialogInspect,
            TaskKind::DialogConfirm,
            TaskKind::DialogCancel,
        ] {
            let mut dialog = task();
            dialog.task = kind;
            dialog.scope.region_name = None;
            dialog.scope.entity_kind = Some(WebIrEntityKind::Dialog);
            dialog.inputs.clear();
            dialog.postconditions = vec![TaskPostcondition {
                kind: TaskPostconditionKind::DialogClosed,
                expected: None,
            }];
            dialog.validate().unwrap();
        }
    }
}
