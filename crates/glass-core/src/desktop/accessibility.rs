use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use atspi::{
    Interface, ObjectRefOwned, RelationType, State, StateSet,
    proxy::accessible::AccessibleProxy,
    proxy::{accessible::ObjectRefExt, proxy_ext::ProxyExt},
    zbus::{Proxy, connection::Builder},
};
use serde_json::{Value, json};

const MAX_NODES: usize = 160;
const MAX_DEPTH: usize = 12;
const MAX_NAME_CHARS: usize = 120;
const MAX_VALUE_CHARS: usize = 160;
const MAX_ACTIONS: usize = 8;
const DBUS_CALL_TIMEOUT: Duration = Duration::from_millis(500);
const OBSERVATION_BUDGET: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
pub(super) struct Target {
    pub object: ObjectRefOwned,
    pub role: String,
    pub supports_click: bool,
    pub supports_focus: bool,
    pub supports_text: bool,
    pub is_password: bool,
}

pub(super) struct Snapshot {
    pub value: Value,
    pub targets: HashMap<String, Target>,
}

pub(super) fn observe(
    session_bus_address: &str,
    session_id: &str,
    revision: u64,
    query: Option<&str>,
) -> Result<Snapshot, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| format!("Could not start accessibility runtime: {error}"))?;
    runtime.block_on(async {
        tokio::time::timeout(
            OBSERVATION_BUDGET,
            observe_async(session_bus_address, session_id, revision, query),
        )
        .await
        .map_err(|_| "Accessibility observation exceeded its 3 second limit".to_owned())?
    })
}

async fn observe_async(
    session_bus_address: &str,
    session_id: &str,
    revision: u64,
    query: Option<&str>,
) -> Result<Snapshot, String> {
    let query_terms = query.map(validate_query).transpose()?;
    validate_private_bus_address(session_bus_address)?;
    let session_bus = Builder::address(session_bus_address)
        .map_err(accessibility_error)?
        .method_timeout(DBUS_CALL_TIMEOUT)
        .build()
        .await
        .map_err(accessibility_error)?;
    let bus_proxy = Proxy::new(
        &session_bus,
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.a11y.Bus",
    )
    .await
    .map_err(accessibility_error)?;
    let address: String = bus_proxy
        .call("GetAddress", &())
        .await
        .map_err(accessibility_error)?;
    validate_private_bus_address(&address)?;
    let a11y_bus_address =
        atspi::zbus::address::Address::try_from(address.as_str()).map_err(accessibility_error)?;
    let accessibility = Builder::address(a11y_bus_address)
        .map_err(accessibility_error)?
        .method_timeout(DBUS_CALL_TIMEOUT)
        .build()
        .await
        .map_err(accessibility_error)?;
    let root = AccessibleProxy::builder(&accessibility)
        .destination("org.a11y.atspi.Registry")
        .map_err(accessibility_error)?
        .build()
        .await
        .map_err(accessibility_error)?;
    let application_roots = root.get_children().await.map_err(accessibility_error)?;
    let mut pending = VecDeque::new();
    for object in application_roots {
        pending.push_back((object, 0usize, None::<String>, None::<Value>));
    }

    let mut nodes = Vec::new();
    let mut applications = Vec::new();
    let mut focused_window = None;
    let mut targets = HashMap::new();
    let mut visited = 0usize;
    let mut truncated = false;
    let started = Instant::now();
    while let Some((object, depth, parent_application, parent_window)) = pending.pop_front() {
        if visited >= MAX_NODES || started.elapsed() >= OBSERVATION_BUDGET {
            truncated = true;
            break;
        }
        visited += 1;
        let accessible = match object.as_accessible_proxy(&accessibility).await {
            Ok(accessible) => accessible,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        // These properties are independent D-Bus calls on one object. Run
        // them together so tree latency is bounded by the slowest response,
        // rather than paying three round trips for every node.
        let (role_result, interfaces_result, states_result) = tokio::join!(
            accessible.get_role_name(),
            accessible.get_interfaces(),
            accessible.get_state(),
        );
        let role = role_result.unwrap_or_else(|_| "unknown".to_owned());
        let interfaces = match interfaces_result {
            Ok(interfaces) => interfaces,
            Err(_) => {
                truncated = true;
                continue;
            }
        };
        let states = states_result.unwrap_or_else(|_| StateSet::empty());
        let role_lower = role.to_ascii_lowercase();
        let is_password = role_lower.contains("password");
        let is_window = matches!(
            role_lower.as_str(),
            "frame" | "window" | "dialog" | "alert" | "file chooser"
        );
        let supports_click = interfaces.contains(Interface::Action);
        let supports_focus = interfaces.contains(Interface::Component);
        let supports_text = interfaces.contains(Interface::EditableText);
        let visible = states.contains(State::Visible) || states.contains(State::Showing);
        let role_can_activate = matches!(
            role_lower.as_str(),
            "button"
                | "push button"
                | "toggle button"
                | "check box"
                | "radio button"
                | "link"
                | "menu item"
                | "menu button"
                | "list item"
                | "tab"
                | "combo box"
                | "tree item"
        );
        let actions = if supports_click && role_can_activate {
            match accessible.proxies().await {
                Ok(proxies) => match proxies.action().await {
                    Ok(proxy) => proxy.get_actions().await.unwrap_or_default(),
                    Err(_) => Vec::new(),
                },
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let action_names = actions
            .iter()
            .take(MAX_ACTIONS)
            .map(|action| action.name.chars().take(MAX_NAME_CHARS).collect::<String>())
            .collect::<Vec<_>>();
        let is_actionable = supports_click || supports_focus || supports_text;
        let name = if is_password {
            "Protected field".to_owned()
        } else {
            let name = accessible
                .name()
                .await
                .unwrap_or_default()
                .chars()
                .take(MAX_NAME_CHARS)
                .collect::<String>();
            if name.trim().is_empty() && (supports_text || role_can_activate || is_window) {
                accessible_context_name(&accessible, &accessibility).await
            } else {
                name
            }
        };
        let node_is_relevant = depth == 0 || is_window || (visible && is_actionable);
        let application = if depth == 0 {
            let application_name = (!name.trim().is_empty()).then(|| name.clone());
            if let Some(name) = application_name.as_ref()
                && applications.len() < 12
                && !applications.contains(name)
            {
                applications.push(name.clone());
            }
            application_name
        } else {
            parent_application
        };
        let window = if is_window {
            Some(json!({
                "role": role.clone(),
                "name": name.clone(),
                "application": application.clone(),
            }))
        } else {
            parent_window
        };
        // Window-manager helpers can themselves expose a window role and a
        // focused state. Prefer a focused control's actual owning app window.
        if states.contains(State::Focused) && !is_window && focused_window.is_none() {
            focused_window = window.clone();
        }
        if node_is_relevant {
            let target_id = is_actionable.then(|| {
                format!(
                    "ax-{}-{revision}-{}",
                    session_id.chars().take(12).collect::<String>(),
                    visited
                )
            });
            if let Some(target_id) = target_id.as_ref() {
                targets.insert(
                    target_id.clone(),
                    Target {
                        object: object.clone(),
                        role: role.clone(),
                        supports_click,
                        supports_focus,
                        supports_text,
                        is_password,
                    },
                );
            }
            let value = if supports_text && !is_password && query.is_none() {
                read_text_value(&accessible, MAX_VALUE_CHARS).await
            } else {
                None
            };
            nodes.push(json!({
                "targetId": target_id,
                "depth": depth,
                "role": role,
                "name": name,
                "application": application.clone(),
                "states": state_names(states),
                "actions": action_names,
                "value": value,
                "protected": is_password,
            }));
        }
        if depth < MAX_DEPTH {
            match accessible.get_children().await {
                Ok(children) => {
                    let children = children
                        .into_iter()
                        .filter(|child| !child.is_null())
                        .map(|child| (child, depth + 1, application.clone(), window.clone()))
                        .collect::<Vec<_>>();
                    // Walk breadth-first so every application root gets a
                    // chance to contribute its foreground window before a
                    // large background tree consumes the observation budget.
                    pending.extend(children);
                }
                Err(_) => truncated = true,
            }
        } else if accessible.child_count().await.unwrap_or_default() > 0 {
            truncated = true;
        }
    }

    let matched_targets = if let Some(terms) = query_terms.as_ref() {
        nodes.retain(|node| query_matches(node, terms));
        nodes
            .iter()
            .filter_map(|node| node["targetId"].as_str().map(ToOwned::to_owned))
            .collect::<std::collections::HashSet<_>>()
    } else {
        targets.keys().cloned().collect()
    };
    targets.retain(|target_id, _| matched_targets.contains(target_id));

    Ok(Snapshot {
        value: json!({
            "sessionId": session_id,
            "revision": revision,
            "query": query,
            "applications": applications,
            "focusedWindow": focused_window,
            "truncated": truncated || !pending.is_empty(),
            "nodesVisited": visited,
            "nodes": nodes,
        }),
        targets,
    })
}

async fn accessible_context_name(
    accessible: &AccessibleProxy<'_>,
    accessibility: &atspi::zbus::Connection,
) -> String {
    if let Ok(relations) = accessible.get_relation_set().await {
        for (relation, labels) in relations {
            if relation != RelationType::LabelledBy {
                continue;
            }
            for label in labels {
                let Ok(label) = label.as_accessible_proxy(accessibility).await else {
                    continue;
                };
                let name = label.name().await.unwrap_or_default();
                if !name.trim().is_empty() {
                    return name.chars().take(MAX_NAME_CHARS).collect();
                }
            }
        }
    }
    let description = accessible.description().await.unwrap_or_default();
    if !description.trim().is_empty() {
        return description.chars().take(MAX_NAME_CHARS).collect();
    }
    for (key, value) in accessible.get_attributes().await.unwrap_or_default() {
        if matches!(
            key.to_ascii_lowercase().as_str(),
            "placeholder" | "placeholder-text" | "label" | "accessible-name"
        ) && !value.trim().is_empty()
        {
            return value.chars().take(MAX_NAME_CHARS).collect();
        }
    }
    String::new()
}

fn validate_query(query: &str) -> Result<Vec<String>, String> {
    let query = query.trim();
    if query.is_empty()
        || query.len() > 120
        || query.chars().any(char::is_control)
        || query.split_whitespace().count() > 8
    {
        return Err("query must contain 1–8 words and be at most 120 characters".into());
    }
    Ok(query.split_whitespace().map(str::to_lowercase).collect())
}

fn query_matches(node: &Value, terms: &[String]) -> bool {
    let mut fields = vec![
        node["role"].as_str().unwrap_or_default(),
        node["name"].as_str().unwrap_or_default(),
        node["application"].as_str().unwrap_or_default(),
    ];
    if let Some(actions) = node["actions"].as_array() {
        fields.extend(actions.iter().filter_map(Value::as_str));
    }
    let haystack = fields.join(" ").to_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

async fn read_text_value(
    accessible: &atspi::proxy::accessible::AccessibleProxy<'_>,
    limit: usize,
) -> Option<String> {
    let proxies = accessible.proxies().await.ok()?;
    let text = proxies.text().await.ok()?;
    let char_count = text.character_count().await.ok()?.max(0) as usize;
    if char_count == 0 {
        return Some(String::new());
    }
    let raw = text.get_text(0, char_count.min(limit) as i32).await.ok()?;
    Some(raw.chars().take(limit).collect())
}

pub(super) fn act(
    session_bus_address: &str,
    target: &Target,
    action: &str,
    text: Option<&str>,
) -> Result<Value, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| format!("Could not start accessibility runtime: {error}"))?;
    runtime.block_on(async {
        tokio::time::timeout(
            DBUS_CALL_TIMEOUT * 4,
            act_async(session_bus_address, target, action, text),
        )
        .await
        .map_err(|_| "Accessibility action exceeded its 2 second limit".to_owned())?
    })
}

async fn act_async(
    session_bus_address: &str,
    target: &Target,
    action: &str,
    text: Option<&str>,
) -> Result<Value, String> {
    validate_private_bus_address(session_bus_address)?;
    let session_bus = Builder::address(session_bus_address)
        .map_err(accessibility_error)?
        .method_timeout(DBUS_CALL_TIMEOUT)
        .build()
        .await
        .map_err(accessibility_error)?;
    let bus_proxy = Proxy::new(
        &session_bus,
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.a11y.Bus",
    )
    .await
    .map_err(accessibility_error)?;
    let address: String = bus_proxy
        .call("GetAddress", &())
        .await
        .map_err(accessibility_error)?;
    validate_private_bus_address(&address)?;
    let a11y_bus_address =
        atspi::zbus::address::Address::try_from(address.as_str()).map_err(accessibility_error)?;
    let accessibility = Builder::address(a11y_bus_address)
        .map_err(accessibility_error)?
        .method_timeout(DBUS_CALL_TIMEOUT)
        .build()
        .await
        .map_err(accessibility_error)?;
    let accessible = target
        .object
        .as_accessible_proxy(&accessibility)
        .await
        .map_err(accessibility_error)?;
    let current_role = accessible
        .get_role_name()
        .await
        .map_err(|_| "Accessibility target is stale; call desktop_observe again".to_owned())?;
    let states = accessible
        .get_state()
        .await
        .map_err(|_| "Accessibility target is stale; call desktop_observe again".to_owned())?;
    if current_role != target.role || states.contains(State::Defunct) {
        return Err("Accessibility target is stale; call desktop_observe again".into());
    }
    if !states.contains(State::Sensitive)
        || (!states.contains(State::Visible) && !states.contains(State::Showing))
    {
        return Err("Accessibility target is not currently visible and sensitive".into());
    }

    match action {
        "click" => {
            if !target.supports_click {
                return Err("Target has no accessibility action; use desktop_screenshot then explicit desktop_input".to_owned());
            }
            let proxies = accessible.proxies().await.map_err(accessibility_error)?;
            let action_proxy = proxies.action().await.map_err(accessibility_error)?;
            let actions = action_proxy
                .get_actions()
                .await
                .map_err(accessibility_error)?;
            let index = actions
                .iter()
                .position(|item| {
                    matches!(
                        item.name.to_ascii_lowercase().as_str(),
                        "click" | "press" | "activate" | "open"
                    )
                })
                .ok_or_else(|| {
                    "Target reports no recognized accessibility click action; use desktop_screenshot then explicit desktop_input".to_owned()
                })?;
            let performed = action_proxy
                .do_action(index as i32)
                .await
                .map_err(accessibility_error)?;
            if !performed {
                return Err("The target refused its accessibility action".to_owned());
            }
            Ok(
                json!({"effect":"performed","verified":false,"action":actions[index].name,"followUp":"Check the fresh observation for the expected UI state before continuing."}),
            )
        }
        "focus" => {
            if !target.supports_focus {
                return Err("Target does not support accessibility focus; use desktop_screenshot then explicit desktop_input".to_owned());
            }
            let proxies = accessible.proxies().await.map_err(accessibility_error)?;
            let component = proxies.component().await.map_err(accessibility_error)?;
            if !component.grab_focus().await.map_err(accessibility_error)? {
                return Err("The target refused accessibility focus".to_owned());
            }
            let states = accessible.get_state().await.map_err(accessibility_error)?;
            let verified = states.contains(State::Focused);
            Ok(
                json!({"effect":"performed","verified":verified,"action":"focus","followUp":"Check the fresh observation for the focused state before continuing."}),
            )
        }
        "set_text" => {
            if !target.supports_text {
                return Err("Target does not support accessible text editing; use desktop_screenshot then explicit desktop_input".to_owned());
            }
            let text_value = text.ok_or("set_text requires text")?;
            if text_value.len() > 512
                || text_value.contains('\0')
                || text_value
                    .chars()
                    .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
            {
                return Err(
                    "text must be at most 512 UTF-8 bytes without control characters".to_owned(),
                );
            }
            let proxies = accessible.proxies().await.map_err(accessibility_error)?;
            let editable = proxies.editable_text().await.map_err(accessibility_error)?;
            if !editable
                .set_text_contents(text_value)
                .await
                .map_err(accessibility_error)?
            {
                return Err("The target refused accessible text input".to_owned());
            }
            let read_back = if target.is_password {
                None
            } else {
                read_text_value(&accessible, 512).await
            };
            let verified = read_back.as_deref() == Some(text_value);
            Ok(json!({
                "effect":"performed",
                "verified":verified,
                "value":read_back,
                "protected":target.is_password,
                "followUp":"Check the fresh observation; target IDs have been refreshed."
            }))
        }
        _ => Err("action must be click, focus, or set_text".to_owned()),
    }
}

fn state_names(states: StateSet) -> Vec<&'static str> {
    states.iter().map(|state| state.to_static_str()).collect()
}

fn validate_private_bus_address(address: &str) -> Result<(), String> {
    if address.starts_with("unix:path=") || address.starts_with("unix:abstract=") {
        Ok(())
    } else {
        Err("The accessibility registry did not return a private Unix D-Bus address".into())
    }
}

fn accessibility_error(error: impl std::fmt::Display) -> String {
    format!("Accessibility interface unavailable: {error}")
}

#[cfg(test)]
mod tests {
    use super::{query_matches, validate_query};
    use serde_json::json;

    #[test]
    fn accessibility_query_is_bounded_and_searches_semantics_not_values() {
        assert_eq!(
            validate_query("  Save button ").unwrap(),
            ["save", "button"]
        );
        assert!(validate_query("").is_err());
        assert!(validate_query("one two three four five six seven eight nine").is_err());
        assert!(validate_query(&"x".repeat(121)).is_err());

        let button = json!({
            "role":"push button",
            "name":"Save",
            "application":"Zenity",
            "actions":["click"],
            "value":"secret text must not be searchable"
        });
        assert!(query_matches(&button, &["save".into(), "click".into()]));
        assert!(!query_matches(&button, &["secret".into()]));
    }
}
