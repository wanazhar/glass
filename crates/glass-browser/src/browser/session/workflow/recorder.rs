use super::*;
/// Semantic target captured by the workflow recorder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecordedTarget {
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_kind: Option<super::super::SemanticRegionKind>,
}

/// Bounded route evidence captured by a semantic recorder.
///
/// Browser target and frame handles are hashed because they are useful for
/// comparing a recording with later evidence but are not valid replay
/// selectors. Query strings and fragments are removed from the retained URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecordedRoute {
    pub target_digest: String,
    pub frame_digest: String,
    pub url: String,
}

/// Resolution evidence retained with one semantic draft step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecordedSemantic {
    pub intent: String,
    pub normalized_intent: String,
    pub action: SemanticIntentAction,
    pub resolution: SemanticResolution,
    pub policy_decision: IntentPolicyDecision,
    pub candidate_count: usize,
    pub excluded_count: usize,
    pub ambiguous: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<WorkflowRecordedRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_fingerprint: Option<String>,
}

/// Confidence attached to a recorded draft, never to a runtime guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRecordingConfidence {
    High,
    Medium,
    Low,
}

/// One reviewable semantic recorder draft step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDraftStep {
    pub id: String,
    pub action: BatchStep,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<WorkflowIntentStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<WorkflowRecordedTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<WorkflowRecordedSemantic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect: Option<VerificationPredicate>,
    pub transaction: WorkflowTransactionClass,
    pub confidence: WorkflowRecordingConfidence,
    pub review_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_name: Option<String>,
    #[serde(default)]
    pub sensitive_input: bool,
}

/// A bounded recorder output that remains a draft until explicitly reviewed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDraft {
    pub schema_version: u32,
    pub name: String,
    pub workflow_version: String,
    pub steps: Vec<WorkflowDraftStep>,
}

/// In-memory recorder for semantic workflow drafts.
#[derive(Debug, Clone)]
pub struct WorkflowRecorder {
    draft: WorkflowDraft,
}

impl WorkflowRecorder {
    /// Start a bounded recorder draft. Recording is local and does not attach
    /// to Chrome or intercept browser traffic.
    pub fn new(name: impl Into<String>, workflow_version: impl Into<String>) -> Self {
        Self {
            draft: WorkflowDraft {
                schema_version: WORKFLOW_SCHEMA_VERSION,
                name: name.into(),
                workflow_version: workflow_version.into(),
                steps: Vec::new(),
            },
        }
    }

    /// Record a semantic click draft using an explicit role and accessible name.
    pub fn record_click(
        &mut self,
        id: impl Into<String>,
        role: impl Into<String>,
        name: impl Into<String>,
        expect: Option<VerificationPredicate>,
    ) -> Result<(), WorkflowValidationError> {
        let target = recorded_target(role.into(), name.into(), None, None)?;
        let locator = format!("role={};name={}", target.role, target.name);
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Click { target: locator },
            intent: None,
            target: Some(target),
            semantic: None,
            expect,
            transaction: WorkflowTransactionClass::Unknown,
            confidence: WorkflowRecordingConfidence::High,
            review_required: true,
            input_name: None,
            sensitive_input: false,
        })
    }

    /// Record text as a typed input placeholder, never as a literal value.
    pub fn record_type_input(
        &mut self,
        id: impl Into<String>,
        role: impl Into<String>,
        name: impl Into<String>,
        input_name: impl Into<String>,
    ) -> Result<(), WorkflowValidationError> {
        let target = recorded_target(role.into(), name.into(), None, None)?;
        let input_name = input_name.into();
        validate_name("inputName", &input_name)?;
        let sensitive_input = looks_sensitive_input_name(&input_name);
        let locator = format!("role={};name={}", target.role, target.name);
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Type {
                text: format!("${{inputs.{input_name}}}"),
                target: Some(locator),
            },
            intent: None,
            target: Some(target),
            semantic: None,
            expect: None,
            transaction: WorkflowTransactionClass::Unknown,
            confidence: WorkflowRecordingConfidence::High,
            review_required: true,
            input_name: Some(input_name),
            sensitive_input,
        })
    }

    /// Record a read-only observation draft.
    pub fn record_observe(&mut self, id: impl Into<String>) -> Result<(), WorkflowValidationError> {
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Observe {
                include_dom: false,
                include_screenshot: false,
                include_form_values: false,
            },
            intent: None,
            target: None,
            semantic: None,
            expect: None,
            transaction: WorkflowTransactionClass::ReadOnly,
            confidence: WorkflowRecordingConfidence::High,
            review_required: true,
            input_name: None,
            sensitive_input: false,
        })
    }

    /// Record a semantic resolution as a reviewable workflow intent step.
    ///
    /// The result may be ambiguous, rejected, or lack a selected candidate;
    /// those states are retained as evidence and never turned into a replay
    /// target. Value-bearing actions receive an input placeholder only.
    pub fn record_semantic_intent(
        &mut self,
        id: impl Into<String>,
        request: &SemanticIntentRequest,
        result: &SemanticIntentResult,
        input_name: Option<impl Into<String>>,
        transaction: WorkflowTransactionClass,
        expect: Option<VerificationPredicate>,
    ) -> Result<(), WorkflowValidationError> {
        request
            .validate()
            .map_err(|error| WorkflowValidationError::new("semantic.request", error.to_string()))?;
        result
            .validate()
            .map_err(|error| WorkflowValidationError::new("semantic.result", error.to_string()))?;
        if request.action != result.action || request.intent != result.intent {
            return Err(WorkflowValidationError::new(
                "semantic",
                "request and result action/intent do not match",
            ));
        }

        let input_name = input_name.map(Into::into);
        let value = match request.action {
            SemanticIntentAction::Type | SemanticIntentAction::Select => {
                let input_name = input_name.as_deref().ok_or_else(|| {
                    WorkflowValidationError::new(
                        "inputName",
                        "type and select recordings require an input name",
                    )
                })?;
                validate_name("inputName", input_name)?;
                Some(format!("${{inputs.{input_name}}}"))
            }
            _ if input_name.is_some() => {
                return Err(WorkflowValidationError::new(
                    "inputName",
                    "only type and select recordings accept an input name",
                ));
            }
            _ => None,
        };

        let selected = result.selected_candidate.as_deref().and_then(|id| {
            result
                .candidates
                .iter()
                .find(|candidate| candidate.id == id)
        });
        let target = selected
            .map(|candidate| {
                recorded_target(
                    candidate.role.clone(),
                    candidate.name.clone(),
                    candidate.region_kind.map(|kind| format!("{kind:?}")),
                    candidate.region_kind,
                )
            })
            .transpose()?;
        let target_fingerprint = selected.and_then(|candidate| {
            candidate.fingerprint.as_ref().map(|fingerprint| {
                target_fingerprint_digest(
                    &candidate.role,
                    &candidate.name,
                    candidate.input_type.as_deref(),
                    candidate.region_kind,
                    fingerprint.purpose,
                )
            })
        });
        let confidence = selected
            .map(|candidate| recording_confidence(candidate.confidence))
            .unwrap_or(WorkflowRecordingConfidence::Low);
        let semantic = WorkflowRecordedSemantic {
            intent: result.intent.clone(),
            normalized_intent: result.normalized_intent.clone(),
            action: result.action,
            resolution: result.resolution,
            policy_decision: result.policy_decision,
            candidate_count: result.candidates.len(),
            excluded_count: result.excluded_count,
            ambiguous: matches!(result.resolution, SemanticResolution::Ambiguous),
            revision: result.revision,
            route: result.route.as_ref().map(recorded_route),
            target_fingerprint,
        };
        let intent = WorkflowIntentStep {
            action: request.action,
            purpose: None,
            intent: Some(request.intent.clone()),
            scope: request.scope.clone(),
            constraints: request.constraints.clone(),
            resolution_policy: request.resolution_policy,
            value,
        };
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Observe {
                include_dom: false,
                include_screenshot: false,
                include_form_values: false,
            },
            intent: Some(intent),
            target,
            semantic: Some(semantic),
            expect,
            transaction,
            confidence,
            review_required: true,
            sensitive_input: input_name
                .as_deref()
                .is_some_and(looks_sensitive_input_name),
            input_name,
        })
    }

    pub fn draft(&self) -> &WorkflowDraft {
        &self.draft
    }

    /// Attach a postcondition to the most recently recorded step.
    pub fn attach_expect_to_last(
        &mut self,
        expect: VerificationPredicate,
    ) -> Result<(), WorkflowValidationError> {
        let last = self.draft.steps.last_mut().ok_or_else(|| {
            WorkflowValidationError::new("expect", "record a step before attaching a postcondition")
        })?;
        last.expect = Some(expect);
        Ok(())
    }

    /// Infer declarations for the placeholders observed in the draft.
    ///
    /// No value is retained. Names that look sensitive are marked sensitive;
    /// callers still decide whether the resulting declaration is appropriate
    /// before compiling the final workflow.
    pub fn inferred_inputs(&self) -> BTreeMap<String, WorkflowInput> {
        let mut inputs = BTreeMap::new();
        for step in &self.draft.steps {
            let Some(name) = &step.input_name else {
                continue;
            };
            let sensitive = step.sensitive_input;
            inputs
                .entry(name.clone())
                .and_modify(|input: &mut WorkflowInput| {
                    if sensitive {
                        input.sensitive = Some(true);
                    }
                })
                .or_insert_with(|| WorkflowInput {
                    value_type: WorkflowValueType::String,
                    required: true,
                    max_length: None,
                    sensitive: sensitive.then_some(true),
                });
        }
        inputs
    }

    /// Convert a reviewed draft into the normal validated workflow contract.
    pub fn into_definition(
        self,
        inputs: BTreeMap<String, WorkflowInput>,
        budgets: WorkflowBudgets,
        terminal_condition: VerificationPredicate,
        outputs: BTreeMap<String, WorkflowOutputDeclaration>,
    ) -> Result<WorkflowDefinition, WorkflowValidationError> {
        let definition = WorkflowDefinition {
            schema_version: self.draft.schema_version,
            name: self.draft.name,
            workflow_version: self.draft.workflow_version,
            description: Some("Recorded draft; review before execution.".into()),
            inputs,
            budgets,
            preconditions: Vec::new(),
            steps: self
                .draft
                .steps
                .into_iter()
                .map(|step| WorkflowStep {
                    id: step.id,
                    action: step.action,
                    intent: step.intent,
                    when: None,
                    expect: step.expect,
                    before_retry: None,
                    transaction: step.transaction,
                    idempotency_key: None,
                    max_retries: 0,
                    repeat: 1,
                })
                .collect(),
            terminal_condition,
            outputs,
        };
        definition.validate()?;
        Ok(definition)
    }

    fn push(&mut self, step: WorkflowDraftStep) -> Result<(), WorkflowValidationError> {
        if self.draft.steps.len() >= MAX_STEPS {
            return Err(WorkflowValidationError::new(
                "steps",
                format!("must contain at most {MAX_STEPS} entries"),
            ));
        }
        validate_name("steps.id", &step.id)?;
        if self.draft.steps.iter().any(|item| item.id == step.id) {
            return Err(WorkflowValidationError::new(
                "steps.id",
                format!("duplicate step ID {:?}", step.id),
            ));
        }
        self.draft.steps.push(step);
        Ok(())
    }
}

fn recorded_target(
    role: String,
    name: String,
    context: Option<String>,
    region_kind: Option<super::super::SemanticRegionKind>,
) -> Result<WorkflowRecordedTarget, WorkflowValidationError> {
    validate_bytes("target.role", &role, 1, 128)?;
    validate_bytes("target.name", &name, 1, MAX_TARGET_BYTES)?;
    if role.contains([';', '\n', '\r']) || name.contains([';', '\n', '\r']) {
        return Err(WorkflowValidationError::new(
            "target",
            "semantic target fields cannot contain locator separators or newlines",
        ));
    }
    if let Some(context) = &context {
        validate_bytes("target.context", context, 1, 256)?;
    }
    Ok(WorkflowRecordedTarget {
        role,
        name,
        context,
        region_kind,
    })
}

fn recording_confidence(confidence: IntentConfidence) -> WorkflowRecordingConfidence {
    match confidence {
        IntentConfidence::Exact | IntentConfidence::High => WorkflowRecordingConfidence::High,
        IntentConfidence::Medium => WorkflowRecordingConfidence::Medium,
        IntentConfidence::Low | IntentConfidence::Insufficient => WorkflowRecordingConfidence::Low,
    }
}

fn looks_sensitive_input_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "password", "passwd", "secret", "token", "api_key", "apikey", "cookie",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn recorded_route(route: &SemanticRouteIdentity) -> WorkflowRecordedRoute {
    let url = Url::parse(&route.url)
        .map(|mut parsed| {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.to_string()
        })
        .unwrap_or_else(|_| bound_workflow_text(&route.url, 2_048));
    WorkflowRecordedRoute {
        target_digest: hash_recorded_identifier(&route.target_id),
        frame_digest: hash_recorded_identifier(&route.frame_id),
        url,
    }
}

fn hash_recorded_identifier(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("sha256:{digest:x}")
}
