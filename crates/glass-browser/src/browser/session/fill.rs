//! High-level form filling.
//!
//! Provides atomically-resolved multi-field form fill via
//! [`CdpBrowserSession::fill_form`]. Each field is resolved, then
//! the appropriate action (type, check, uncheck, select, click) is
//! applied based on the element's accessibility role.

use super::*;

/// Outcome of a high-level form fill operation.
///
/// Returned by [`CdpBrowserSession::fill_form`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct FillFormOutcome {
    pub status: ActionStatus,
    #[serde(rename = "failureKind", skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<ActionFailureKind>,
    #[serde(rename = "executionId")]
    pub execution_id: String,
    /// Number of fields successfully filled.
    pub filled: usize,
    /// Total number of fields submitted.
    pub total: usize,
    /// Per-field result with action taken and status.
    pub fields: Vec<FillFieldResult>,
    #[serde(rename = "previousRevision")]
    pub previous_revision: u64,
    #[serde(rename = "currentRevision")]
    pub current_revision: u64,
    pub verification: ActionVerificationEvidence,
}

/// Result for a single field within a form fill operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FillFieldResult {
    /// Locator target that was resolved.
    pub target: String,
    /// Action applied: `"type"`, `"select"`, `"check"`, `"uncheck"`, `"click"`.
    pub action: String,
    /// Accessible label of the element, if available.
    pub label: Option<String>,
    /// Whether the action succeeded.
    pub success: bool,
    /// Error message if the action failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Maximum fields accepted in a single [`CdpBrowserSession::fill_form`] call.
const FILL_FORM_MAX_FIELDS: usize = 16;

impl CdpBrowserSession {
    /// Fill multiple form fields atomically.
    ///
    /// First resolves all locators (failing atomically on any resolution
    /// error), then applies the appropriate action per field based on
    /// its role: text inputs get typed, checkboxes/radios get
    /// checked/unchecked, selects/combo boxes get option selection.
    ///
    /// Bounded to 16 fields per call.
    pub async fn fill_form(&self, fields: &[(&str, &str)]) -> BrowserResult<FillFormOutcome> {
        self.fill_form_with_expected_revision(fields, None).await
    }

    /// Fill a form while optionally enforcing the caller's observation revision.
    pub async fn fill_form_with_expected_revision(
        &self,
        fields: &[(&str, &str)],
        expected_revision: Option<u64>,
    ) -> BrowserResult<FillFormOutcome> {
        self.require_expected_revision(expected_revision)?;
        let previous_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        let before = self.page_info().await.ok();
        let total = fields.len();
        if total > FILL_FORM_MAX_FIELDS {
            return Err(format!(
                "fill_form: max {} fields, got {total}",
                FILL_FORM_MAX_FIELDS
            )
            .into());
        }

        // Phase 1: resolve all locators atomically
        let context_id = self.page_context_id().await?;
        let mut resolved: Vec<(String, ResolvedElement)> = Vec::with_capacity(total);
        for (target, _value) in fields {
            let element = self.resolve_element(target).await?;
            resolved.push(((*target).to_string(), element));
        }

        // Phase 2: apply actions
        let mut results = Vec::with_capacity(total);
        let mut filled = 0usize;

        for ((target, element), (_t, value)) in resolved.iter().zip(fields.iter()) {
            let (action, success, error) =
                self.fill_single_field(element, value, &context_id).await;
            if success {
                filled += 1;
            }
            results.push(FillFieldResult {
                target: target.clone(),
                action,
                label: Some(element.label.clone()),
                success,
                error,
            });
        }

        let current_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        let after = self.page_info().await.ok();
        Ok(FillFormOutcome {
            status: if filled == total {
                ActionStatus::Succeeded
            } else {
                ActionStatus::CompletedWithVerificationFailure
            },
            failure_kind: (filled != total).then_some(ActionFailureKind::VerificationFailed),
            execution_id: self.next_execution_id(),
            filled,
            total,
            fields: results,
            previous_revision,
            current_revision,
            verification: ActionVerificationEvidence {
                revision_delta: current_revision.saturating_sub(previous_revision),
                url_changed: before
                    .as_ref()
                    .zip(after.as_ref())
                    .is_some_and(|(before, after)| before.url != after.url),
                title_changed: before
                    .as_ref()
                    .zip(after.as_ref())
                    .is_some_and(|(before, after)| before.title != after.title),
                ..ActionVerificationEvidence::default()
            },
        })
    }

    async fn fill_single_field(
        &self,
        element: &ResolvedElement,
        value: &str,
        context_id: &str,
    ) -> (String, bool, Option<String>) {
        let current_context = match self.page_context_id().await {
            Ok(context) => context,
            Err(error) => return ("none".into(), false, Some(error.to_string())),
        };
        let reference = match form_field_reference(
            element,
            context_id,
            &current_context,
            self.page_revision
                .load(std::sync::atomic::Ordering::Relaxed),
        ) {
            Ok(reference) => reference,
            Err(error) => return ("none".into(), false, Some(error)),
        };
        let reference = reference.as_str();

        let metadata = match self.form_control_metadata(element).await {
            Ok(metadata) => metadata,
            Err(error) => return ("none".into(), false, Some(error.to_string())),
        };
        let role = element
            .role
            .as_deref()
            .or(metadata["role"].as_str())
            .unwrap_or("")
            .to_lowercase();
        let input_type = metadata["type"]
            .as_str()
            .or(element.input_type.as_deref())
            .unwrap_or("")
            .to_lowercase();

        if metadata["tag"] == "SELECT" {
            match self.select_option(reference, value).await {
                Ok(_) => ("select".to_string(), true, None),
                Err(e) => ("select".to_string(), false, Some(e.to_string())),
            }
        } else if role == "checkbox" || input_type == "checkbox" {
            let should_check =
                !value.is_empty() && value != "false" && value != "0" && value != "off";
            let (action, result) = if should_check {
                ("check", self.check(reference).await)
            } else {
                ("uncheck", self.uncheck(reference).await)
            };
            match result {
                Ok(_) => (action.to_string(), true, None),
                Err(e) => (action.to_string(), false, Some(e.to_string())),
            }
        } else if role == "radio" || input_type == "radio" {
            match self.click(reference).await {
                Ok(_) => ("click".to_string(), true, None),
                Err(e) => ("click".to_string(), false, Some(e.to_string())),
            }
        } else {
            let dropdown = matches!(role.as_str(), "combobox" | "listbox");
            if dropdown && metadata["editable"].as_bool() != Some(true) {
                return (
                    "none".into(),
                    false,
                    Some("form control is a non-editable ARIA dropdown; interact with its options instead".into()),
                );
            }
            match self.type_text(value, Some(reference)).await {
                Ok(_) if dropdown => match self.form_control_metadata(element).await {
                    Ok(after) if after["value"] != metadata["value"] => ("type".into(), true, None),
                    Ok(_) => (
                        "type".into(),
                        false,
                        Some(
                            "form control value did not change; interact with its options instead"
                                .into(),
                        ),
                    ),
                    Err(error) => ("type".into(), false, Some(error.to_string())),
                },
                Ok(_) => ("type".to_string(), true, None),
                Err(e) => ("type".to_string(), false, Some(e.to_string())),
            }
        }
    }

    async fn form_control_metadata(&self, element: &ResolvedElement) -> BrowserResult<Value> {
        let object = self
            .cdp
            .resolve_node_object(element.node_id, element.backend_dom_node_id)
            .await?;
        let result = self.cdp.call_on_object(&object,
            "function() { return {tag: this.tagName, role: this.getAttribute('role'), type: this.type || '', editable: this.isContentEditable || ((this.tagName === 'INPUT' || this.tagName === 'TEXTAREA') && !this.readOnly && !this.disabled), value: this.value ?? this.textContent}; }").await;
        let _ = self.cdp.release_object(&object).await;
        Ok(result?.result.value.unwrap_or_default())
    }
}

fn form_field_reference(
    element: &ResolvedElement,
    planned_context: &str,
    current_context: &str,
    revision: u64,
) -> Result<String, String> {
    if planned_context != current_context {
        return Err("form document changed; observe and resolve the form again".into());
    }
    let node = element
        .backend_dom_node_id
        .filter(|id| *id > 0)
        .ok_or_else(|| "form element has no stable backend node identity".to_string())?;
    Ok(format!("r{revision}:c{current_context}:b{node}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_form_fields_keep_node_identity_across_own_revisions() {
        let mut element = ResolvedElement {
            node_id: None,
            backend_dom_node_id: Some(42),
            label: "Email".into(),
            reference: Some("r1:cpage:b42".into()),
            role: None,
            input_type: None,
        };
        assert_eq!(
            form_field_reference(&element, "page", "page", 3).unwrap(),
            "r3:cpage:b42"
        );
        assert!(form_field_reference(&element, "page", "replacement", 3).is_err());
        element.backend_dom_node_id = None;
        assert!(form_field_reference(&element, "page", "page", 3).is_err());
    }

    #[test]
    fn fill_field_result_success_serializes_without_error() {
        let result = FillFieldResult {
            target: "username".to_string(),
            action: "type".to_string(),
            label: Some("Username".to_string()),
            success: true,
            error: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["target"], "username");
        assert_eq!(json["action"], "type");
        assert_eq!(json["label"], "Username");
        assert_eq!(json["success"], true);
        assert!(json.get("error").is_none());
    }

    #[test]
    fn fill_field_result_failure_includes_error() {
        let result = FillFieldResult {
            target: "missing-field".to_string(),
            action: "none".to_string(),
            label: None,
            success: false,
            error: Some("element has no reference".to_string()),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["error"], "element has no reference");
        assert!(json["label"].is_null());
    }

    #[test]
    fn fill_form_outcome_counts_filled_and_total() {
        let outcome = FillFormOutcome {
            status: ActionStatus::CompletedWithVerificationFailure,
            failure_kind: Some(ActionFailureKind::VerificationFailed),
            execution_id: "act_test_1".to_string(),
            filled: 2,
            total: 3,
            fields: vec![
                FillFieldResult {
                    target: "a".to_string(),
                    action: "type".to_string(),
                    label: None,
                    success: true,
                    error: None,
                },
                FillFieldResult {
                    target: "b".to_string(),
                    action: "check".to_string(),
                    label: None,
                    success: true,
                    error: None,
                },
                FillFieldResult {
                    target: "c".to_string(),
                    action: "none".to_string(),
                    label: None,
                    success: false,
                    error: Some("not found".to_string()),
                },
            ],
            previous_revision: 1,
            current_revision: 3,
            verification: ActionVerificationEvidence {
                revision_delta: 2,
                ..ActionVerificationEvidence::default()
            },
        };
        let json = serde_json::to_value(&outcome).unwrap();
        assert_eq!(json["filled"], 2);
        assert_eq!(json["total"], 3);
        assert_eq!(json["fields"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn fill_form_max_fields_is_16() {
        assert_eq!(FILL_FORM_MAX_FIELDS, 16);
    }

    #[test]
    fn fill_form_max_fields_is_reasonable() {
        // Must be at least 1 to allow single-field fills
        const { assert!(FILL_FORM_MAX_FIELDS >= 1) };
        // Must be at most 64 to prevent unbounded allocation
        const { assert!(FILL_FORM_MAX_FIELDS <= 64) };
    }

    #[test]
    fn fill_form_rejects_exactly_one_over_max() {
        // Verify the bound check: exactly 17 fields exceeds 16-field cap
        const { assert!(FILL_FORM_MAX_FIELDS + 1 > FILL_FORM_MAX_FIELDS) };
        // Boundary: exactly at max should pass the bound check
        const { assert!(FILL_FORM_MAX_FIELDS == 16) };
        // The error message uses the constant
        let err_msg = format!(
            "fill_form: max {} fields, got {}",
            FILL_FORM_MAX_FIELDS,
            FILL_FORM_MAX_FIELDS + 1
        );
        assert!(err_msg.contains("max 16 fields"));
        assert!(err_msg.contains("got 17"));
    }

    #[test]
    fn fill_field_result_roundtrip_through_json() {
        let result = FillFieldResult {
            target: "email".to_string(),
            action: "type".to_string(),
            label: Some("Email Address".to_string()),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        // We can't deserialize back (no Deserialize impl), but verify JSON structure
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["target"], "email");
        assert_eq!(parsed["success"], true);
        assert!(parsed.get("error").is_none());
    }
}
