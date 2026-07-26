use super::*;

/// Outcome of a high-level form fill operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FillFormOutcome {
    pub filled: usize,
    pub total: usize,
    pub fields: Vec<FillFieldResult>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FillFieldResult {
    pub target: String,
    pub action: String,
    pub label: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

const FILL_FORM_MAX_FIELDS: usize = 16;

impl BrowserSession {
    /// Fill multiple form fields atomically.
    ///
    /// First resolves all locators (failing atomically on any resolution
    /// error), then applies the appropriate action per field based on
    /// its role: text inputs get typed, checkboxes/radios get
    /// checked/unchecked, selects/combo boxes get option selection.
    ///
    /// Bounded to 16 fields per call.
    pub async fn fill_form(&self, fields: &[(&str, &str)]) -> BrowserResult<FillFormOutcome> {
        let total = fields.len();
        if total > FILL_FORM_MAX_FIELDS {
            return Err(format!(
                "fill_form: max {} fields, got {total}",
                FILL_FORM_MAX_FIELDS
            )
            .into());
        }

        // Phase 1: resolve all locators atomically
        let mut resolved: Vec<(String, ResolvedElement)> = Vec::with_capacity(total);
        for (target, _value) in fields {
            let element = self
                .resolve_element(target)
                .await
                .map_err(|e| format!("fill_form: resolution failed for \"{target}\": {e}"))?;
            resolved.push(((*target).to_string(), element));
        }

        // Phase 2: apply actions
        let mut results = Vec::with_capacity(total);
        let mut filled = 0usize;

        for ((target, element), (_t, value)) in resolved.iter().zip(fields.iter()) {
            let (action, success, error) = self.fill_single_field(element, value).await;
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

        Ok(FillFormOutcome {
            filled,
            total,
            fields: results,
        })
    }

    async fn fill_single_field(
        &self,
        element: &ResolvedElement,
        value: &str,
    ) -> (String, bool, Option<String>) {
        let role = element.role.as_deref().unwrap_or("").to_lowercase();
        let input_type = element.input_type.as_deref().unwrap_or("").to_lowercase();

        let Some(ref reference) = element.reference else {
            return (
                "none".to_string(),
                false,
                Some("element has no reference".to_string()),
            );
        };

        if matches!(role.as_str(), "listbox" | "combobox") {
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
            match self.type_text(value, Some(reference)).await {
                Ok(_) => ("type".to_string(), true, None),
                Err(e) => ("type".to_string(), false, Some(e.to_string())),
            }
        }
    }
}
