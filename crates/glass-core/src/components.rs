//! Per-turn client render capabilities and the reply-component catalog.
//!
//! Each prompt may carry `"client":{"name","schema","components"}`. Core
//! decides per turn: when the sending client can draw components, the model
//! gets the catalog and usage rules for that turn only; terminal turns get
//! none. Every stored component keeps its Markdown fallback (see
//! `glass_tui::components`), so model context and other clients stay readable.
use serde_json::{Value, json};

pub use glass_tui::components::{SCHEMA_VERSION, TYPES, normalize};

pub const CONTEXT_MARKER: &str = "\n\n[Glass client]";

/// What the client that sent this turn can render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientRender {
    pub name: String,
    pub schema: u64,
    pub components: Vec<String>,
}

impl ClientRender {
    /// Reads `message["client"]`. Unknown component types are dropped, and a
    /// client on a different schema version is treated as rendering none.
    pub fn from_message(message: &Value) -> Option<Self> {
        let client = message.get("client")?.as_object()?;
        let name = client.get("name")?.as_str()?.trim().to_ascii_lowercase();
        if name.is_empty()
            || name.len() > 32
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return None;
        }
        let schema = client.get("schema").and_then(Value::as_u64).unwrap_or(0);
        let mut components: Vec<String> = if schema == SCHEMA_VERSION {
            client
                .get("components")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(128)
                .filter_map(Value::as_str)
                .filter(|t| TYPES.contains(t))
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        components.sort_by_key(|t| TYPES.iter().position(|k| k == t));
        components.dedup();
        Some(Self {
            name,
            schema,
            components,
        })
    }

    /// The terminal client: no components, by design.
    pub fn tui() -> Value {
        json!({"name":"tui","schema":SCHEMA_VERSION,"components":[]})
    }

    fn label(&self) -> &str {
        match self.name.as_str() {
            "ios" => "the Glass iOS app",
            "web" => "the Glass web app",
            "tui" => "the Glass terminal app",
            _ => "a Glass client",
        }
    }
}

/// The per-turn prompt suffix. Empty when no client said who it is.
pub fn prompt_context(render: Option<&ClientRender>) -> String {
    let Some(render) = render else {
        return String::new();
    };
    let mut p = format!(
        "{CONTEXT_MARKER}\nFor this turn the user is on {}.",
        render.label()
    );
    if render.components.is_empty() {
        p.push_str(" Reply in plain Markdown.\n");
        return p;
    }
    p.push_str(&format!(
        " It can draw reply components (schema v{SCHEMA_VERSION}) in addition to Markdown, math and Mermaid.\n\
Use a component only when it makes the answer clearer than prose: structured comparisons, live data views, choices the user acts on. Most replies need none.\n\
Write one component as a fenced block with the info string glass-ui holding a single JSON object on one line:\n\
```glass-ui\n{{\"type\":\"table\",\"v\":1,\"fallback\":\"| Name | Status |\\n| --- | --- |\\n| api | up |\",\"data\":{{\"columns\":[{{\"key\":\"name\",\"label\":\"Name\"}},{{\"key\":\"status\",\"label\":\"Status\"}}],\"rows\":[{{\"name\":\"api\",\"status\":\"up\"}}],\"filter\":\"status\"}}}}\n```\n\
Rules:\n\
- fallback is required: Markdown that stands alone for clients that can't draw the type (table as a Markdown table, chart as a short data summary, cards as bullet links, form as a numbered question list). Don't repeat the component in prose.\n\
- Never invent live data (prices, weather, scores, flights, hotels, jobs, news, availability, rates). Use tools to fetch it. If you can't, either skip the component or fill it with clearly made-up values, set \"example\":true and say so.\n\
- writing blocks are drafts; the user sends them with their own apps. Buttons with a send action only post text back into this chat.\n\
- Links must be real URLs you have seen. Images and video must be https URLs.\n\
- This applies to this turn only. Earlier replies may be plain or contain glass-ui blocks from other clients; read their fallback and data either way.\n\
Types and their data fields (? optional):\n"
    ));
    for kind in &render.components {
        if let Some((_, fields)) = CATALOG.iter().find(|(k, _)| k == kind) {
            p.push_str(&format!("- {kind}: {fields}\n"));
        }
    }
    p
}

/// Data shape of each component type, shown to the model.
pub const CATALOG: &[(&str, &str)] = &[
    (
        "badges",
        "items[{text, tone?: neutral|info|success|warning|danger, style?: filled|outlined}]",
    ),
    (
        "icon_list",
        "items[{icon (SF Symbol name), title, caption?}]",
    ),
    (
        "entities",
        "items[{kind: web|place|book, title, url?, subtitle?}] — inline entity links",
    ),
    (
        "kpi",
        "title, icon?, value, delta?, trend?: up|down|flat, subtitle?, ranges?{\"7d\"|\"30d\"|\"90d\": {value, delta?, trend?}}",
    ),
    (
        "buttons",
        "items[{label, style?: primary|secondary|plain, action: {send: text} | {url} | {copy: text}}]",
    ),
    ("counter", "label, value, step?, min?, max?"),
    ("copy", "label?, text"),
    (
        "choices",
        "id, title?, multiple?, options[{id, title, subtitle?, image?}], submit? — selectable cards; submitting posts the selection",
    ),
    (
        "form",
        "id, title?, fields[{id, label, kind: text|email|select|radio|checkbox|slider|date|textarea, options?[], min?, max?, step?, required?, placeholder?}], submit? — submitting posts the answers",
    ),
    (
        "table",
        "title?, columns[{key, label}], rows[{key: value}], filter? (column key users filter by)",
    ),
    (
        "chart",
        "kind: line|bar|scatter|area|pie, title?, x_label?, y_label?, series[{name, points[{x, y}]}] (pie: one series, x is the slice label)",
    ),
    ("image", "url, alt, caption?"),
    ("gallery", "images[{url, alt, caption?}]"),
    ("carousel", "cards[{title, subtitle?, image?, url?, body?}]"),
    (
        "map",
        "title?, pins[{lat, lon, label, icon? (SF Symbol)}], zoom? (1-20)",
    ),
    (
        "disclosure",
        "style?: disclosure|popover|tooltip, title, body (Markdown)",
    ),
    ("checklist", "title?, items[{text, done}]"),
    (
        "progress",
        "label, value (0-1), style?: bar|ring|gradient, caption?",
    ),
    (
        "timeline",
        "steps[{title, detail?, time?, status: done|current|pending}]",
    ),
    (
        "html",
        "title, html (self-contained HTML+SVG+inline JS; no network, no external files), height? (pixels)",
    ),
    (
        "product",
        "title, url?, image?, price?, rating? (0-5), reviews?, merchant?, badges?[]",
    ),
    (
        "business",
        "name, category?, rating?, reviews?, price_level?, address?, hours?, phone?, url?, lat?, lon?, image?",
    ),
    (
        "reservation",
        "venue, date?, party_size?, slots[{time, url?}], url?",
    ),
    ("video", "url, title, poster?, duration?"),
    (
        "sources",
        "items[{title, url, site?, snippet?}] — cite them inline as [1], [2]",
    ),
    ("file", "name, path?, kind?, size?, excerpt?, url?"),
    (
        "writing",
        "kind: email|chat|social|document|text, to?, subject?, platform?, body",
    ),
    (
        "weather",
        "location, unit: C|F, current{temp, condition, icon?, high?, low?, humidity?, wind?}, forecast?[{day, high, low, condition, icon?}], as_of?",
    ),
    (
        "sports",
        "league?, status, time?, venue?, home{name, score?, logo?}, away{name, score?, logo?}",
    ),
    ("currency", "amount, from, to, rate, converted, as_of?"),
    (
        "flights",
        "from, to, date?, results[{airline, depart, arrive, duration?, stops?, price?, url?}]",
    ),
    (
        "hotels",
        "location, results[{name, area?, rating?, price?, image?, url?}]",
    ),
    (
        "jobs",
        "results[{title, company, location?, salary?, posted?, url?}]",
    ),
    (
        "news",
        "items[{title, source, published?, image?, url, summary?}]",
    ),
    (
        "flow",
        "title?, steps[{title, detail?, icon? (SF Symbol)}] — a \"how it works\" diagram",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn render(client: Value) -> Option<ClientRender> {
        ClientRender::from_message(&json!({"t":"prompt","text":"hi","client":client}))
    }

    #[test]
    fn catalog_covers_every_component_type() {
        let names: Vec<_> = CATALOG.iter().map(|(k, _)| *k).collect();
        assert_eq!(names, TYPES);
    }

    #[test]
    fn capabilities_drop_unknown_types_and_other_schema_versions() {
        let ios = render(
            json!({"name":"iOS","schema":1,"components":["table","hologram","chart","table"]}),
        )
        .unwrap();
        assert_eq!(ios.name, "ios");
        assert_eq!(ios.components, ["table", "chart"]);
        let future = render(json!({"name":"web","schema":2,"components":["table"]})).unwrap();
        assert!(
            future.components.is_empty(),
            "an unknown schema renders nothing"
        );
        assert_eq!(
            render(json!({"name":"tui","schema":1})).unwrap().components,
            Vec::<String>::new()
        );
        assert!(render(json!({"name":"bad name!"})).is_none());
        assert!(ClientRender::from_message(&json!({"t":"prompt","text":"hi"})).is_none());
    }

    #[test]
    fn ios_turn_gets_the_catalog_and_names_the_client() {
        let ios = render(json!({"name":"ios","schema":1,"components":TYPES})).unwrap();
        let p = prompt_context(Some(&ios));
        assert!(p.starts_with(CONTEXT_MARKER));
        assert!(p.contains("the user is on the Glass iOS app"));
        assert!(p.contains("```glass-ui") && p.contains("fallback is required"));
        assert!(p.contains("Never invent live data"));
        for kind in TYPES {
            assert!(p.contains(&format!("- {kind}: ")), "{kind}");
        }
    }

    #[test]
    fn partial_capabilities_list_only_supported_types() {
        let web = render(json!({"name":"web","schema":1,"components":["table"]})).unwrap();
        let p = prompt_context(Some(&web));
        assert!(
            p.contains("the Glass web app") && p.contains("- table: ") && !p.contains("- chart: ")
        );
    }

    #[test]
    fn tui_turns_get_no_component_instructions() {
        let tui = ClientRender::from_message(&json!({"client":ClientRender::tui()})).unwrap();
        let p = prompt_context(Some(&tui));
        assert!(p.contains("the Glass terminal app"));
        assert!(!p.contains("glass-ui") && !p.contains("component") && !p.contains("fallback"));
        assert_eq!(prompt_context(None), "");
    }

    #[test]
    fn capability_is_per_turn_and_stored_components_survive_client_switches() {
        let ios = render(json!({"name":"ios","schema":1,"components":TYPES})).unwrap();
        let tui = ClientRender::from_message(&json!({"client":ClientRender::tui()})).unwrap();
        let base = "You are Glass.";

        let reply = "Here:\n```glass-ui\n{\"type\":\"table\",\"data\":{\"columns\":[\"name\"],\"rows\":[{\"name\":\"api\"}]}}\n```\n";
        let stored = normalize(reply).into_owned();
        assert!(stored.contains("\"fallback\"") && stored.contains("\"rows\""));
        assert!(stored.contains("\"v\":1"));

        let ios_system = format!("{base}{}", prompt_context(Some(&ios)));
        let stripped = ios_system.split(CONTEXT_MARKER).next().unwrap();
        assert_eq!(stripped, base);
        let tui_system = format!("{stripped}{}", prompt_context(Some(&tui)));
        assert!(!tui_system.contains("glass-ui") && !tui_system.contains("- table: "));

        let terminal = glass_tui::components::plain(&stored);
        assert!(terminal.contains("| name |") && terminal.contains("| api |"));
        assert!(!terminal.contains("glass-ui"));
        assert_eq!(
            normalize(&stored),
            stored,
            "stored blocks are stable on re-save"
        );

        let back = format!("{stripped}{}", prompt_context(Some(&ios)));
        assert!(back.contains("- table: ") && back.contains("Earlier replies may be plain"));
    }
}
