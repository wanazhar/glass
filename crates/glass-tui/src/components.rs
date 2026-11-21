//! Glass reply components: a fenced `glass-ui` block holding one JSON object
//! `{"type", "v", "fallback", "example"?, "data"}`. Clients that render the
//! type draw it natively; everything else, including this terminal, shows the
//! required Markdown `fallback`, so shared history stays readable everywhere.
use serde_json::{Map, Value};
use std::borrow::Cow;

/// Version of the component envelope and data shapes.
pub const SCHEMA_VERSION: u64 = 1;
/// Info string of the fenced block that carries a component.
pub const FENCE: &str = "glass-ui";
/// Largest component block accepted, in bytes.
pub const MAX_BYTES: usize = 128 * 1024;

/// Every component type in schema v1.
pub const TYPES: &[&str] = &[
    "badges",
    "icon_list",
    "entities",
    "kpi",
    "buttons",
    "counter",
    "copy",
    "choices",
    "form",
    "table",
    "chart",
    "image",
    "gallery",
    "carousel",
    "map",
    "disclosure",
    "checklist",
    "progress",
    "timeline",
    "html",
    "product",
    "business",
    "reservation",
    "video",
    "sources",
    "file",
    "writing",
    "weather",
    "sports",
    "currency",
    "flights",
    "hotels",
    "jobs",
    "news",
    "flow",
];

pub fn is_known(kind: &str) -> bool {
    TYPES.contains(&kind)
}

/// Whether a fence info string opens a component block.
pub fn is_fence(info: &str) -> bool {
    info.trim().eq_ignore_ascii_case(FENCE)
}

/// Parses a block body. Streaming can split long lines, and JSON never needs
/// raw newlines, so they are dropped before parsing.
pub fn parse(body: &str) -> Option<Map<String, Value>> {
    if body.len() > MAX_BYTES {
        return None;
    }
    let joined: Cow<str> = if body.contains('\n') {
        Cow::Owned(body.replace(['\n', '\r'], ""))
    } else {
        Cow::Borrowed(body)
    };
    match serde_json::from_str::<Value>(joined.trim()).ok()? {
        Value::Object(map) if map.get("type").and_then(Value::as_str).is_some() => Some(map),
        _ => None,
    }
}

/// The Markdown shown by clients that can't draw the component: the stored
/// fallback, or one derived from the data when a block arrived without it.
pub fn fallback(component: &Map<String, Value>) -> String {
    let stored = component
        .get("fallback")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let mut text = match stored {
        Some(s) => strip_nested(s),
        None => derived(component),
    };
    if component.get("example") == Some(&Value::Bool(true)) && !text.starts_with("_Example") {
        text = format!("_Example data, not live._\n\n{text}");
    }
    text
}

/// Adds `v` and a derived `fallback` to a block that lacks them. Returns
/// `None` when the block is not a component or needs no change.
pub fn normalize_block(body: &str) -> Option<String> {
    let mut map = parse(body)?;
    let mut changed = false;
    if !map.contains_key("v") {
        map.insert("v".into(), Value::from(SCHEMA_VERSION));
        changed = true;
    }
    let missing = map
        .get("fallback")
        .and_then(Value::as_str)
        .is_none_or(|s| s.trim().is_empty());
    if missing {
        let text = derived(&map);
        map.insert("fallback".into(), Value::String(text));
        changed = true;
    }
    changed.then(|| Value::Object(map).to_string())
}

/// Ensures every component block in `text` carries its fallback.
pub fn normalize(text: &str) -> Cow<'_, str> {
    rewrite(text, |body| normalize_block(body).map(|json| block(&json)))
}

/// Replaces every component block with its fallback Markdown.
pub fn plain(text: &str) -> Cow<'_, str> {
    rewrite(text, |body| parse(body).map(|map| fallback(&map)))
}

fn block(json: &str) -> String {
    format!("```{FENCE}\n{json}\n```")
}

/// Calls `replace` with the body of each complete `glass-ui` fence; a
/// `Some` result replaces the whole fence.
fn rewrite(text: &str, mut replace: impl FnMut(&str) -> Option<String>) -> Cow<'_, str> {
    if !text.contains(FENCE) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    let mut lines = text.split_inclusive('\n').peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'));
        let run = marker
            .map(|m| trimmed.chars().take_while(|c| *c == m).count())
            .unwrap_or(0);
        if run < 3 || !is_fence(&trimmed[run..]) {
            out.push_str(line);
            continue;
        }
        let mut body = String::new();
        let mut raw = String::from(line);
        let mut closed = false;
        for next in lines.by_ref() {
            raw.push_str(next);
            let t = next.trim();
            if t.len() >= run && t.chars().all(|c| Some(c) == marker) {
                closed = true;
                break;
            }
            body.push_str(next);
        }
        match closed.then(|| replace(&body)).flatten() {
            Some(replacement) => {
                out.push_str(&replacement);
                if raw.ends_with('\n') {
                    out.push('\n');
                }
                changed = true;
            }
            None => out.push_str(&raw),
        }
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// A fallback must not itself contain a component.
fn strip_nested(text: &str) -> String {
    plain(text).replace("```glass-ui", "```text")
}

fn s(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.trim().is_empty() => Some(clean(s)),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn clean(s: &str) -> String {
    s.replace(['\n', '\r'], " ").trim().to_string()
}

fn first(item: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| s(item.get(*k)))
}

const TITLE_KEYS: &[&str] = &[
    "title", "name", "label", "text", "headline", "airline", "venue", "question",
];
const DETAIL_KEYS: &[&str] = &[
    "subtitle",
    "summary",
    "detail",
    "description",
    "caption",
    "company",
    "source",
    "site",
    "price",
    "value",
    "status",
    "location",
    "address",
];
const LIST_KEYS: &[&str] = &[
    "items", "results", "cards", "sources", "images", "options", "steps", "pins", "forecast",
    "fields", "slots",
];

/// A readable Markdown summary of a component's data.
pub fn derived(component: &Map<String, Value>) -> String {
    let kind = component.get("type").and_then(Value::as_str).unwrap_or("");
    let empty = Map::new();
    let data = component
        .get("data")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let mut out = String::new();
    if let Some(title) = first(data, &["title", "name", "location", "label"]) {
        out.push_str(&format!("**{title}**\n\n"));
    }
    match kind {
        "table" => out.push_str(&table(data)),
        "chart" => out.push_str(&chart(data)),
        "form" => out.push_str(&numbered(data.get("fields"), true)),
        "choices" => out.push_str(&numbered(data.get("options"), false)),
        "kpi" => {
            let line = [
                first(data, &["value"]),
                first(data, &["delta"]),
                first(data, &["subtitle"]),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            out.push_str(&line);
            out.push('\n');
        }
        "progress" => {
            let value = data.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            out.push_str(&format!(
                "{:.0}% complete\n",
                (value.clamp(0.0, 1.0) * 100.0)
            ));
        }
        "currency" => {
            let line = [
                first(data, &["amount"]),
                first(data, &["from"]),
                Some("=".into()),
                first(data, &["converted"]),
                first(data, &["to"]),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            out.push_str(&line);
            out.push('\n');
            if let Some(rate) = first(data, &["rate"]) {
                out.push_str(&format!("Rate {rate}\n"));
            }
        }
        "sports" => {
            let side = |key: &str| {
                data.get(key)
                    .and_then(Value::as_object)
                    .map(|t| {
                        format!(
                            "{} {}",
                            first(t, &["name"]).unwrap_or_default(),
                            first(t, &["score"]).unwrap_or_default()
                        )
                    })
                    .unwrap_or_default()
            };
            out.push_str(&format!("{} – {}\n", side("home"), side("away")));
            if let Some(status) = first(data, &["status"]) {
                out.push_str(&format!("{status}\n"));
            }
        }
        "copy" | "writing" | "html" => {
            if let Some(body) = first(data, &["subject"]) {
                out.push_str(&format!("Subject: {body}\n\n"));
            }
            let body = data
                .get("text")
                .or_else(|| data.get("body"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if kind == "html" {
                out.push_str("Interactive view; open in the Glass app or web UI.\n");
            } else {
                out.push_str(&format!("> {}\n", body.trim().replace('\n', "\n> ")));
            }
        }
        _ => {}
    }
    if !matches!(kind, "table" | "chart" | "form" | "choices") {
        if let Some((_, list)) = LIST_KEYS
            .iter()
            .find_map(|k| data.get(*k).and_then(Value::as_array).map(|v| (k, v)))
        {
            out.push_str(&bullets(list));
        } else if out.trim().is_empty()
            || matches!(
                kind,
                "weather"
                    | "business"
                    | "product"
                    | "reservation"
                    | "file"
                    | "video"
                    | "image"
                    | "map"
            )
        {
            out.push_str(&fields(data));
        }
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        format!("({kind} component)")
    } else {
        out
    }
}

fn bullets(list: &[Value]) -> String {
    let mut out = String::new();
    for item in list.iter().take(50) {
        match item {
            Value::Object(item) => {
                let title = first(item, TITLE_KEYS).unwrap_or_else(|| "Item".into());
                let title = match first(item, &["url", "link"]) {
                    Some(url) => format!("[{title}]({url})"),
                    None => title,
                };
                let done = match item.get("done").or_else(|| item.get("status")) {
                    Some(Value::Bool(true)) => "[x] ",
                    Some(Value::Bool(false)) => "[ ] ",
                    _ => "",
                };
                let detail = first(item, DETAIL_KEYS)
                    .filter(|d| !title.contains(d.as_str()))
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default();
                out.push_str(&format!("- {done}{title}{detail}\n"));
            }
            other => {
                if let Some(text) = s(Some(other)) {
                    out.push_str(&format!("- {text}\n"));
                }
            }
        }
    }
    out
}

fn numbered(list: Option<&Value>, form: bool) -> String {
    let mut out = String::new();
    for (index, item) in list
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(50)
        .enumerate()
    {
        let Some(item) = item.as_object() else {
            continue;
        };
        let label = first(item, TITLE_KEYS).unwrap_or_else(|| format!("Question {}", index + 1));
        let options = item
            .get("options")
            .and_then(Value::as_array)
            .map(|o| {
                o.iter()
                    .filter_map(|v| {
                        s(Some(v)).or_else(|| v.as_object().and_then(|m| first(m, TITLE_KEYS)))
                    })
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .filter(|o| !o.is_empty());
        let detail = if form {
            options.map(|o| format!(" ({o})")).unwrap_or_default()
        } else {
            first(item, DETAIL_KEYS)
                .map(|d| format!(" — {d}"))
                .unwrap_or_default()
        };
        out.push_str(&format!("{}. {label}{detail}\n", index + 1));
    }
    if form && !out.is_empty() {
        out.push_str("\nReply with your answers.\n");
    }
    out
}

fn cell(v: Option<&Value>) -> String {
    s(v).unwrap_or_default().replace('|', "\\|")
}

fn table(data: &Map<String, Value>) -> String {
    let columns: Vec<(String, String)> = data
        .get("columns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| match c {
            Value::String(name) => Some((name.clone(), name.clone())),
            Value::Object(m) => {
                let key = first(m, &["key", "label"])?;
                Some((key.clone(), first(m, &["label"]).unwrap_or(key)))
            }
            _ => None,
        })
        .collect();
    let rows = data
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if columns.is_empty() {
        return bullets(&rows);
    }
    let mut out = format!(
        "| {} |\n|{}|\n",
        columns
            .iter()
            .map(|c| c.1.replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(" | "),
        columns
            .iter()
            .map(|_| " --- ")
            .collect::<Vec<_>>()
            .join("|")
    );
    for row in rows.iter().take(200) {
        let cells: Vec<String> = match row {
            Value::Array(values) => (0..columns.len()).map(|i| cell(values.get(i))).collect(),
            Value::Object(m) => columns.iter().map(|c| cell(m.get(&c.0))).collect(),
            _ => continue,
        };
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    if rows.len() > 200 {
        out.push_str(&format!("\n{} more rows\n", rows.len() - 200));
    }
    out
}

fn chart(data: &Map<String, Value>) -> String {
    let kind = first(data, &["kind"]).unwrap_or_else(|| "line".into());
    let mut out = format!("{} chart", capitalize(&kind));
    if let (Some(x), Some(y)) = (first(data, &["x_label"]), first(data, &["y_label"])) {
        out.push_str(&format!(" of {y} by {x}"));
    }
    out.push_str(":\n");
    for series in data
        .get("series")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(12)
    {
        let Some(series) = series.as_object() else {
            continue;
        };
        let name = first(series, &["name"]).unwrap_or_else(|| "Series".into());
        let points: Vec<String> = series
            .get("points")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .map(|p| {
                format!(
                    "{} {}",
                    cell(p.get("x").or_else(|| p.get("label"))),
                    cell(p.get("y").or_else(|| p.get("value")))
                )
            })
            .collect();
        let shown = points
            .iter()
            .take(24)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if points.len() > 24 {
            format!(", … {} more", points.len() - 24)
        } else {
            String::new()
        };
        out.push_str(&format!("- {name}: {shown}{more}\n"));
    }
    out
}

fn fields(data: &Map<String, Value>) -> String {
    let mut out = String::new();
    for (key, value) in data.iter().take(24) {
        if matches!(
            key.as_str(),
            "title" | "name" | "image" | "poster" | "icon" | "logo"
        ) {
            continue;
        }
        let text = match value {
            Value::Object(m) => m
                .iter()
                .filter_map(|(k, v)| s(Some(v)).map(|v| format!("{} {v}", label(k))))
                .collect::<Vec<_>>()
                .join(", "),
            Value::Array(list) => format!("{} items", list.len()),
            other => s(Some(other)).unwrap_or_default(),
        };
        if !text.is_empty() {
            out.push_str(&format!("- {}: {text}\n", capitalize(&label(key))));
        }
    }
    out
}

fn label(key: &str) -> String {
    key.replace('_', " ")
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_fallback_wins_and_example_data_is_marked() {
        let map = parse(r#"{"type":"weather","example":true,"fallback":"Sunny, 21°C","data":{}}"#)
            .unwrap();
        assert_eq!(fallback(&map), "_Example data, not live._\n\nSunny, 21°C");
    }

    #[test]
    fn table_without_fallback_becomes_a_markdown_table() {
        let map = parse(r#"{"type":"table","data":{"columns":[{"key":"n","label":"Name"},"status"],"rows":[{"n":"api","status":"up"},["web","down"]]}}"#).unwrap();
        assert_eq!(
            fallback(&map),
            "| Name | status |\n| --- | --- |\n| api | up |\n| web | down |"
        );
    }

    #[test]
    fn chart_summarises_its_data_and_form_lists_questions() {
        let chart = parse(r#"{"type":"chart","data":{"kind":"bar","series":[{"name":"Visits","points":[{"x":"Mon","y":3},{"x":"Tue","y":5}]}]}}"#).unwrap();
        assert!(fallback(&chart).contains("Bar chart:\n- Visits: Mon 3, Tue 5"));
        let form = parse(r#"{"type":"form","data":{"title":"Trip","fields":[{"label":"Name"},{"label":"Seat","options":["Window","Aisle"]}]}}"#).unwrap();
        assert_eq!(
            fallback(&form),
            "**Trip**\n\n1. Name\n2. Seat (Window / Aisle)\n\nReply with your answers."
        );
    }

    #[test]
    fn cards_become_bullet_links() {
        let map = parse(r#"{"type":"news","data":{"items":[{"title":"Launch","url":"https://e.x/a","source":"Wire"}]}}"#).unwrap();
        assert_eq!(fallback(&map), "- [Launch](https://e.x/a) — Wire");
    }

    #[test]
    fn normalize_adds_missing_fallback_and_version_only() {
        let text = "Intro\n```glass-ui\n{\"type\":\"copy\",\"data\":{\"text\":\"abc\"}}\n```\nDone";
        let out = normalize(text);
        let body = out.lines().nth(2).unwrap();
        let map = parse(body).unwrap();
        assert_eq!(map["v"], 1);
        assert_eq!(map["fallback"], "> abc");
        assert!(out.starts_with("Intro\n```glass-ui\n") && out.ends_with("\n```\nDone"));
        let complete =
            "```glass-ui\n{\"type\":\"copy\",\"v\":1,\"fallback\":\"x\",\"data\":{}}\n```";
        assert!(matches!(normalize(complete), Cow::Borrowed(_)));
    }

    #[test]
    fn invalid_or_unclosed_blocks_are_left_alone() {
        for text in [
            "```glass-ui\nnot json\n```",
            "```glass-ui\n{\"type\":\"copy\"",
            "```json\n{\"type\":\"copy\"}\n```",
        ] {
            assert!(matches!(plain(text), Cow::Borrowed(_)), "{text}");
        }
    }

    #[test]
    fn plain_replaces_blocks_and_tolerates_split_lines() {
        let text =
            "A\n```glass-ui\n{\"type\":\"badges\",\"fallback\":\"**Up**\",\n\"data\":{}}\n```\nB\n";
        assert_eq!(plain(text), "A\n**Up**\nB\n");
    }
}
