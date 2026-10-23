//! Validation and bounded feedback for model-issued calls. No execution or coercion.
use glass_mod_api::ToolCall;
use serde_json::Value;
pub const MAX_REPAIRS: usize = 2;

/// Protocol adapters must send object-shaped input even for rejected calls.
/// Original arguments remain untouched in the durable record.
pub fn wire_arguments(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(value @ Value::Object(_)) => value.to_string(),
        _ => "{}".into(),
    }
}

pub fn validate(call: &ToolCall, tools: &[(String, Value)]) -> Result<(), String> {
    let schema = tools
        .iter()
        .find(|(name, _)| name == &call.name)
        .ok_or_else(|| unknown_tool_feedback(&call.name, tools))?;
    schema_shape(&schema.1, 0)?;
    let args: Value = serde_json::from_str(&call.arguments).map_err(|e| {
        format!(
            "invalid JSON at line {}, column {}; send one complete JSON object",
            e.line(),
            e.column()
        )
    })?;
    if !args.is_object() {
        return Err("arguments must be a JSON object".into());
    }
    check(&schema.1, &args, "$", 0)
}

fn unknown_tool_feedback(name: &str, tools: &[(String, Value)]) -> String {
    let Some((namespace, requested)) = name.rsplit_once('.') else {
        return "unknown tool; use the exact names in the current tool catalog".into();
    };
    let prefix = format!("{namespace}.");
    let scoped = tools
        .iter()
        .filter_map(|(candidate, _)| candidate.strip_prefix(&prefix))
        .collect::<Vec<_>>();
    if scoped.is_empty() {
        return "unknown tool; use the exact names in the current tool catalog".into();
    }

    // Catch common near-misses like `glass.browser.action` without creating
    // an alias or accepting an unadvertised name. The suggestion is still
    // validated against the exact current catalog on the next request.
    let close_matches = scoped
        .iter()
        .filter(|candidate| candidate.starts_with(requested) || requested.starts_with(**candidate))
        .take(3)
        .map(|candidate| format!("{prefix}{candidate}"))
        .collect::<Vec<_>>();
    if !close_matches.is_empty() {
        return format!(
            "unknown tool name; use the exact advertised name {}",
            close_matches.join(" or ")
        );
    }

    let preferred = if namespace == "glass.browser" {
        ["act", "navigate", "observe", "state", "start", "stop"]
            .into_iter()
            .filter(|candidate| scoped.contains(candidate))
            .take(4)
            .map(|candidate| format!("{prefix}{candidate}"))
            .collect::<Vec<_>>()
    } else {
        scoped
            .into_iter()
            .take(4)
            .map(|candidate| format!("{prefix}{candidate}"))
            .collect::<Vec<_>>()
    };
    format!(
        "unknown tool name; use an exact advertised name in this namespace, such as {}",
        preferred.join(", ")
    )
}

/// Bounded, value-free diagnostic for the visible transcript when a batch is
/// rejected before execution. This identifies the invalid call and schema
/// path without displaying model-supplied argument values.
pub fn rejection_summary(calls: &[ToolCall], errors: &[Option<String>]) -> String {
    calls
        .iter()
        .zip(errors)
        .filter_map(|(call, error)| {
            error.as_deref().map(|error| {
                let name = call
                    .name
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(80)
                    .collect::<String>();
                let reason = error
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(240)
                    .collect::<String>();
                format!("{name}: {reason}")
            })
        })
        .take(3)
        .collect::<Vec<_>>()
        .join("; ")
}

// Validate the shape of supported schema keywords; unsupported extension
// constraints remain explicitly outside this lightweight validator.
pub(crate) fn schema_shape(schema: &Value, depth: usize) -> Result<(), String> {
    let invalid = || "advertised tool schema is invalid; fix the tool definition".to_string();
    if depth > 32 {
        return Err(invalid());
    }
    if schema.is_boolean() {
        return Ok(());
    }
    if !schema.is_object() {
        return Err(invalid());
    }
    if let Some(kind) = schema.get("type") {
        let known = |v: &Value| {
            v.as_str().is_some_and(|s| {
                matches!(
                    s,
                    "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
                )
            })
        };
        if !(known(kind)
            || kind
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(known)))
        {
            return Err(invalid());
        }
    }
    if let Some(required) = schema.get("required")
        && !required
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(invalid());
    }
    for keyword in ["anyOf", "allOf", "oneOf"] {
        if let Some(value) = schema.get(keyword) {
            let branches = value
                .as_array()
                .filter(|a| !a.is_empty())
                .ok_or_else(invalid)?;
            for child in branches {
                schema_shape(child, depth + 1)?;
            }
        }
    }
    if let Some(properties) = schema.get("properties") {
        for child in properties.as_object().ok_or_else(invalid)?.values() {
            schema_shape(child, depth + 1)?;
        }
    }
    for keyword in ["items", "additionalProperties", "not"] {
        if let Some(child) = schema.get(keyword) {
            schema_shape(child, depth + 1)?;
        }
    }
    if let Some(options) = schema.get("enum")
        && !options.as_array().is_some_and(|a| !a.is_empty())
    {
        return Err(invalid());
    }
    for keyword in ["minimum", "maximum"] {
        if schema.get(keyword).is_some_and(|v| !v.is_number()) {
            return Err(invalid());
        }
    }
    for keyword in [
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "minProperties",
        "maxProperties",
    ] {
        if schema.get(keyword).is_some_and(|v| !v.is_u64()) {
            return Err(invalid());
        }
    }
    Ok(())
}

// Deliberately a structural validator, not a claim of full JSON Schema support.
fn check(schema: &Value, value: &Value, path: &str, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("tool input nesting exceeds validation limit".into());
    }
    if schema == &Value::Bool(false) {
        return Err(format!("{path}: value forbidden by schema"));
    }
    if let Some(types) = schema.get("type") {
        let matches = |kind: &str| match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => true,
        };
        let valid = types.as_str().map(matches).unwrap_or_else(|| {
            types
                .as_array()
                .is_some_and(|ts| ts.iter().filter_map(Value::as_str).any(matches))
        });
        if !valid {
            return Err(format!("{path}: expected {types}"));
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        return Err(format!("{path}: value must match the advertised enum"));
    }
    if let Some(expected) = schema.get("const")
        && expected != value
    {
        return Err(format!("{path}: value must match the advertised constant"));
    }
    for (keyword, all) in [("anyOf", false), ("allOf", true)] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            let valid = if all {
                branches
                    .iter()
                    .all(|s| check(s, value, path, depth + 1).is_ok())
            } else {
                branches
                    .iter()
                    .any(|s| check(s, value, path, depth + 1).is_ok())
            };
            if !valid {
                return Err(format!(
                    "{path}: does not satisfy {keyword}; check required fields in the advertised schema"
                ));
            }
        }
    }
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array)
        && branches
            .iter()
            .filter(|s| check(s, value, path, depth + 1).is_ok())
            .count()
            != 1
    {
        return Err(format!("{path}: must match exactly one schema alternative"));
    }
    if let Some(excluded) = schema.get("not")
        && check(excluded, value, path, depth + 1).is_ok()
    {
        return Err(format!("{path}: matches a forbidden schema"));
    }
    for (length, minimum, maximum) in [
        (
            value.as_str().map(|s| s.chars().count()),
            "minLength",
            "maxLength",
        ),
        (value.as_array().map(Vec::len), "minItems", "maxItems"),
        (
            value.as_object().map(serde_json::Map::len),
            "minProperties",
            "maxProperties",
        ),
    ] {
        if let Some(length) = length
            && (schema
                .get(minimum)
                .and_then(Value::as_u64)
                .is_some_and(|min| (length as u64) < min)
                || schema
                    .get(maximum)
                    .and_then(Value::as_u64)
                    .is_some_and(|max| (length as u64) > max))
        {
            return Err(format!("{path}: outside advertised length bounds"));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}.{key}: required field missing"));
                }
            }
        }
        for (key, child) in object {
            let child_path = format!("{path}.{}", key.chars().take(80).collect::<String>());
            if let Some(property) = schema.get("properties").and_then(|p| p.get(key)) {
                check(property, child, &child_path, depth + 1)?;
            } else if let Some(additional) = schema.get("additionalProperties") {
                if additional == &Value::Bool(false) {
                    return Err(format!("{child_path}: unknown field"));
                }
                check(additional, child, &child_path, depth + 1)?;
            }
        }
    }
    if let Some(array) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for (i, child) in array.iter().enumerate() {
            check(items, child, &format!("{path}[{i}]"), depth + 1)?;
        }
    }
    if let Some(number) = value.as_f64()
        && (schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number > max))
    {
        return Err(format!("{path}: outside advertised numeric bounds"));
    }
    Ok(())
}

pub fn feedback(error: &str, attempt: usize) -> String {
    let error: String = error.chars().take(800).collect();
    format!(
        "error: tool call validation failed: {error}. No calls in this batch executed. Malformed arguments may be represented as {{}} in history; that is a rejected placeholder. Repair {attempt}/{MAX_REPAIRS}: issue corrected calls using the advertised schemas and original task context. Do not repeat previously completed actions; follow GC's permission decisions."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "c".into(),
            name: name.into(),
            arguments: args.into(),
        }
    }
    #[test]
    fn malformed_advertised_schemas_fail_closed() {
        for raw in [
            "null",
            r#"{"type":"bogus"}"#,
            r#"{"required":7}"#,
            r#"{"oneOf":"wrong"}"#,
            r#"{"properties":{"x":null}}"#,
        ] {
            let tools = vec![("custom".into(), serde_json::from_str(raw).unwrap())];
            assert!(validate(&call("custom", "{}"), &tools).is_err(), "{raw}");
        }
    }

    #[test]
    fn rejected_batch_summary_is_actionable_without_echoing_argument_values() {
        let calls = vec![call("glass.browser.navigate", r#"{"url":"private-value"}"#)];
        let summary = rejection_summary(
            &calls,
            &[Some("$.browserRevision: expected integer".into())],
        );
        assert_eq!(
            summary,
            "glass.browser.navigate: $.browserRevision: expected integer"
        );
        assert!(!summary.contains("private-value"));
    }

    #[test]
    fn unknown_browser_tool_suggests_the_exact_action_tool_without_generic_catalog_dump() {
        let tools = vec![
            ("bash".into(), serde_json::json!({"type":"object"})),
            (
                "glass.file.read".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.navigate".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.act".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.observe".into(),
                serde_json::json!({"type":"object"}),
            ),
        ];
        let bad = call(
            "glass.browser.action",
            r#"{"action":"click","target":"private"}"#,
        );
        let error = validate(&bad, &tools).unwrap_err();
        assert_eq!(
            error,
            "unknown tool name; use the exact advertised name glass.browser.act"
        );
        let summary = rejection_summary(&[bad], &[Some(error)]);
        assert!(summary.contains("glass.browser.act"));
        assert!(!summary.contains("bash"));
        assert!(!summary.contains("glass.file.read"));
        assert!(!summary.contains("private"));
        assert!(summary.len() < 240);
    }

    #[test]
    fn unknown_tool_feedback_is_bounded_to_its_namespace() {
        let tools = vec![
            ("bash".into(), serde_json::json!({"type":"object"})),
            (
                "glass.browser.act".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.navigate".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.observe".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.state".into(),
                serde_json::json!({"type":"object"}),
            ),
            (
                "glass.browser.start".into(),
                serde_json::json!({"type":"object"}),
            ),
        ];
        let error = validate(&call("glass.browser.unknown", "{}"), &tools).unwrap_err();
        assert!(error.contains("glass.browser.act"));
        assert!(error.contains("glass.browser.navigate"));
        assert!(!error.contains("bash"));
        assert!(error.len() < 240);
    }

    #[test]
    fn checks_schema_alternatives_and_lengths() {
        let duplicated = serde_json::json!({"oneOf":[{"const":"safe"},{"const":"safe"}]});
        assert!(check(&duplicated, &serde_json::json!("safe"), "$", 0).is_err());
        assert!(
            check(
                &serde_json::json!({"not":{"const":"bad"}}),
                &serde_json::json!("bad"),
                "$",
                0
            )
            .is_err()
        );
        assert!(
            check(
                &serde_json::json!({"minLength":2}),
                &serde_json::json!("x"),
                "$",
                0
            )
            .is_err()
        );
        assert!(
            check(
                &serde_json::json!({"maxItems":1}),
                &serde_json::json!([1, 2]),
                "$",
                0
            )
            .is_err()
        );
    }

    #[test]
    fn validates_builtin_structure_without_coercing() {
        let tools: Vec<_> = crate::provider::builtin_tools()
            .into_iter()
            .map(|(n, _, s)| (n, serde_json::from_str(&s).unwrap()))
            .collect();
        for raw in [
            "{",
            "[]",
            r#"{"command":9}"#,
            r#"{"job":{"action":"unknown"}}"#,
            r#"{"job":{"action":"poll","limit":-1}}"#,
            "{}",
        ] {
            assert!(validate(&call("bash", raw), &tools).is_err(), "{raw}");
        }
        assert!(validate(&call("absent", "{}"), &tools).is_err());
        assert!(validate(&call("bash", r#"{"command":"echo ok"}"#), &tools).is_ok());
        assert!(validate(&call("state", "{}"), &tools).is_ok());
        assert_eq!(wire_arguments("{"), "{}");
        assert_eq!(wire_arguments("[]"), "{}");
        assert_eq!(wire_arguments(r#"{"command":"ok"}"#), r#"{"command":"ok"}"#);
    }
}
