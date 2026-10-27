use serde_json::Value;

pub(crate) const MAX_STRUCTURED_JSON_NODES: usize = 32 * 1024;
pub(crate) const MAX_STRUCTURED_JSON_DEPTH: usize = 128;

/// Bound already-allocated JSON values before converting them into typed data.
pub(crate) fn check_value_budget(value: &Value, max_bytes: usize) -> Result<(), &'static str> {
    let mut stack = vec![(value, 0_usize)];
    let mut visited = 0_usize;
    let mut accounted_bytes = 0_usize;

    while let Some((value, depth)) = stack.pop() {
        visited += 1;
        if visited > MAX_STRUCTURED_JSON_NODES || depth > MAX_STRUCTURED_JSON_DEPTH {
            return Err("structured JSON exceeds the complexity limit");
        }
        accounted_bytes = accounted_bytes.saturating_add(8);
        match value {
            Value::String(text) => accounted_bytes = accounted_bytes.saturating_add(text.len()),
            Value::Array(values) => {
                if visited
                    .saturating_add(stack.len())
                    .saturating_add(values.len())
                    > MAX_STRUCTURED_JSON_NODES
                {
                    return Err("structured JSON exceeds the complexity limit");
                }
                stack.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(fields) => {
                if visited
                    .saturating_add(stack.len())
                    .saturating_add(fields.len())
                    > MAX_STRUCTURED_JSON_NODES
                {
                    return Err("structured JSON exceeds the complexity limit");
                }
                for (name, value) in fields {
                    accounted_bytes = accounted_bytes.saturating_add(name.len()).saturating_add(4);
                    stack.push((value, depth + 1));
                }
            }
            Value::Number(_) => accounted_bytes = accounted_bytes.saturating_add(16),
            Value::Null | Value::Bool(_) => {}
        }
        if accounted_bytes > max_bytes {
            return Err("structured JSON exceeds the size limit");
        }
    }
    Ok(())
}
