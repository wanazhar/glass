//! Bounded native projections for JSON, diagnostics and test output.
use serde_json::Value;
fn label(s: &str) -> String {
    glass_tui::markdown::sanitize(s)
        .replace('|', "\\|")
        .replace(['\n', '\r'], " ")
}
pub fn render(value: &Value) -> String {
    fn walk(v: &Value, depth: usize, nodes: &mut usize, out: &mut String) {
        if *nodes >= 500 || out.len() > 32 * 1024 {
            return;
        }
        *nodes += 1;
        if depth >= 8 {
            out.push_str("… nested value; inspect raw output\n");
            return;
        }
        match v {
            Value::Object(map) => {
                if map.is_empty() {
                    out.push_str("{} · no fields\n");
                }
                for (key, value) in map {
                    if *nodes >= 500 || out.len() > 32 * 1024 {
                        break;
                    }
                    out.push_str(&format!("{} **{}**", "  ".repeat(depth) + "-", label(key)));
                    if value.is_object() || value.is_array() {
                        out.push('\n');
                        walk(value, depth + 1, nodes, out);
                    } else {
                        out.push_str(": ");
                        walk(value, depth + 1, nodes, out);
                    }
                }
            }
            Value::Array(rows) => {
                if rows.is_empty() {
                    out.push_str("[] · no items\n");
                }
                if !rows.is_empty()
                    && rows[0].as_object().is_some_and(|m| !m.is_empty())
                    && rows.iter().all(|row| row.is_object())
                {
                    let columns = rows[0]
                        .as_object()
                        .unwrap()
                        .keys()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>();
                    out.push_str(&format!(
                        "\n| {} |\n| {} |\n",
                        columns
                            .iter()
                            .map(|s| label(s))
                            .collect::<Vec<_>>()
                            .join(" | "),
                        vec!["---"; columns.len()].join(" | ")
                    ));
                    for row in rows.iter().take(100) {
                        if out.len() > 32 * 1024 {
                            break;
                        }
                        out.push_str("| ");
                        out.push_str(
                            &columns
                                .iter()
                                .map(|k| {
                                    let cell = if let Some(s) = row[k].as_str() {
                                        s.to_string()
                                    } else {
                                        row[k].to_string()
                                    };
                                    label(&glass_tui::text::clip(&cell, 200))
                                })
                                .collect::<Vec<_>>()
                                .join(" | "),
                        );
                        out.push_str(" |\n");
                    }
                    if rows.len() > 100 {
                        out.push_str("\n… table capped at 100 rows\n");
                    }
                } else {
                    for (row_index, row) in rows.iter().take(100).enumerate() {
                        out.push_str(&format!("\n**Item {}**\n", row_index + 1));
                        walk(row, depth + 1, nodes, out);
                    }
                }
            }
            Value::String(s) => {
                out.push_str(&label(&glass_tui::text::clip(s, 2000)));
                out.push('\n');
            }
            _ => {
                out.push_str(&v.to_string());
                out.push('\n');
            }
        }
    }
    let mut out = String::new();
    walk(value, 0, &mut 0, &mut out);
    if out.len() > 32 * 1024 {
        let mut end = 32 * 1024;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out.push_str("\n… preview capped; raw output remains available\n");
    }
    out
}
pub fn view(keys: &crate::input::KeyReader, title: &str, text: &str) {
    if text.len() > 64 * 1024 {
        crate::modal::rich_info(
            keys,
            title,
            "Preview exceeds 64 KiB. Use the artifact shelf or full tool output to narrow the data.",
        );
        return;
    }
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        loop {
            let options = ["Formatted overview".into(), "Raw JSON".into()];
            match crate::modal::choose(keys, title, "Native structured view · Esc back", &options)
            {
                Some(0) => crate::modal::rich_info(keys, title, &render(&value)),
                Some(1) => crate::modal::diff(
                    keys,
                    title,
                    &serde_json::to_string_pretty(&value).unwrap_or_default(),
                ),
                _ => return,
            }
        }
    } else {
        crate::modal::rich_info(keys, title, text);
    }
}
pub fn outputs(keys: &crate::input::KeyReader, session: &crate::session::Session) {
    crate::output_view::view(keys, session, crate::output_view::Filter::All);
}

pub fn command(args: &[String]) -> Result<(), String> {
    let path = args.first().ok_or("Usage: gc inspect FILE [--json]")?;
    let bytes = crate::requests::bounded(std::path::Path::new(path), 64 * 1024)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Choose UTF-8 text")?;
    if args.iter().any(|a| a == "--json") {
        let value: Value = serde_json::from_str(text).map_err(|_| "Input is not valid JSON")?;
        crate::emit_json(value);
    } else if let Ok(value) = serde_json::from_str::<Value>(text) {
        crate::cli::markdown("Structured output", &render(&value));
    } else {
        crate::cli::markdown("Output", text);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendering_is_bounded_and_safe() {
        let v = serde_json::json!({"diagnostics":[{"severity":"error","path":"a.rs","line":5,"message":"bad\u{1b}]52;c;SECRET\u{7}"}]});
        let out = render(&v);
        assert!(out.contains("|"));
        assert!(!out.contains('\x1b'));
        let out = render(&serde_json::json!(["x".repeat(100000)]));
        assert!(out.len() < 33 * 1024);
    }
}
