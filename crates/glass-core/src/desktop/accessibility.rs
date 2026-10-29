use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

use atspi::{
    CoordType, Interface, InterfaceSet, ObjectRefOwned, RelationType, State, StateSet,
    proxy::accessible::AccessibleProxy,
    proxy::{accessible::ObjectRefExt, proxy_ext::ProxyExt},
    zbus::{Proxy, connection::Builder},
};
use serde_json::{Value, json};

const MAX_NODES: usize = 160;
const MAX_DEPTH: usize = 12;
const MAX_SEARCH_NODES: usize = 768;
const MAX_SEARCH_DEPTH: usize = 16;
const MAX_WINDOW_ROOTS: usize = 64;
const MAX_NAME_CHARS: usize = 120;
const MAX_VALUE_CHARS: usize = 160;
const MAX_ACTIONS: usize = 8;
const DBUS_CALL_TIMEOUT: Duration = Duration::from_millis(500);
const OBSERVATION_BUDGET: Duration = Duration::from_secs(3);
const FOCUSED_SEARCH_BUDGET: Duration = Duration::from_secs(12);

#[derive(Clone, Debug)]
pub(super) struct Target {
    pub object: ObjectRefOwned,
    pub role: String,
    pub observed_name: Option<String>,
    pub owning_window: Option<(String, String)>,
    pub owning_window_object: Option<ObjectRefOwned>,
    pub supports_click: bool,
    pub supports_focus: bool,
    pub supports_text: bool,
    pub is_password: bool,
    pub screen_bound: bool,
}

pub(super) struct Snapshot {
    pub value: Value,
    pub targets: HashMap<String, Target>,
    pub windows: HashMap<String, WindowRoot>,
    #[cfg(test)]
    pub application_root_names: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct WindowRoot {
    pub object: ObjectRefOwned,
    pub application: String,
    pub role: String,
    pub identity: String,
}

pub(super) fn observe(
    session_bus_address: &str,
    session_id: &str,
    revision: u64,
    query: Option<&str>,
) -> Result<Snapshot, String> {
    observe_scoped(session_bus_address, session_id, revision, query, None)
}

pub(super) fn observe_scoped(
    session_bus_address: &str,
    session_id: &str,
    revision: u64,
    query: Option<&str>,
    window_root: Option<WindowRoot>,
) -> Result<Snapshot, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| format!("Could not start accessibility runtime: {error}"))?;
    runtime.block_on(async {
        let budget = if query.is_some_and(|value| value.split_whitespace().count() > 1)
            || (query.is_some() && window_root.is_some())
        {
            FOCUSED_SEARCH_BUDGET
        } else {
            OBSERVATION_BUDGET
        };
        tokio::time::timeout(
            budget,
            observe_async(
                session_bus_address,
                session_id,
                revision,
                query,
                None,
                window_root,
            ),
        )
        .await
        .map_err(|_| format!("Accessibility observation exceeded its {budget:?} limit"))?
    })
}

async fn observe_async(
    session_bus_address: &str,
    session_id: &str,
    revision: u64,
    query: Option<&str>,
    window_root: Option<WindowRoot>,
    application_window_scope: Option<WindowRoot>,
) -> Result<Snapshot, String> {
    let query_terms = query.map(validate_query).transpose()?;
    let window_scoped = window_root.is_some() || application_window_scope.is_some();
    let scoped_window_id = application_window_scope.as_ref().map(|window| {
        accessibility_window_id(session_id, &window.object, &window.role, &window.identity)
    });
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
    if let Some(window_root) = window_root.as_ref().or(application_window_scope.as_ref()) {
        let window = window_root
            .object
            .as_accessible_proxy(&accessibility)
            .await
            .map_err(|_| "windowId is stale; observe visible windows again")?;
        let (role, states, raw_name) =
            tokio::join!(window.get_role_name(), window.get_state(), window.name(),);
        let role = role.unwrap_or_default();
        let states = states.unwrap_or_else(|_| StateSet::empty());
        let raw_name = raw_name.unwrap_or_default();
        let fallback = if raw_name.trim().is_empty() {
            Some(accessible_context_name(&window, &accessibility).await)
        } else {
            None
        };
        let identity_source = fallback.as_deref().unwrap_or(&raw_name);
        let identity = window_name_identity(identity_source);
        if !window_identity_matches(&window_root.role, &window_root.identity, &role, &identity)
            || !matches!(
                role.to_ascii_lowercase().as_str(),
                "frame" | "window" | "dialog"
            )
            || !states.contains(State::Visible)
            || !states.contains(State::Showing)
        {
            return Err(
                "windowId is stale or no longer visible; observe visible windows again".into(),
            );
        }
    }
    let root = AccessibleProxy::builder(&accessibility)
        .destination("org.a11y.atspi.Registry")
        .map_err(accessibility_error)?
        .build()
        .await
        .map_err(accessibility_error)?;
    let mut application_roots = if window_root.is_some() && application_window_scope.is_none() {
        Vec::new()
    } else {
        root.get_children().await.map_err(accessibility_error)?
    };
    let roots_truncated = application_roots.len() > MAX_NODES;
    application_roots.truncate(MAX_NODES);
    let mut app_scoped_query = window_scoped;
    let mut scoped_window_roots = Vec::<(ObjectRefOwned, String)>::new();
    if let Some(window_root) = window_root {
        scoped_window_roots.push((window_root.object, window_root.application));
    }
    if let Some(window_scope) = application_window_scope.as_ref() {
        let mut matching_applications = Vec::new();
        for object in &application_roots {
            let Ok(app) = object.as_accessible_proxy(&accessibility).await else {
                continue;
            };
            let application_name = app.name().await.unwrap_or_default();
            if application_name.eq_ignore_ascii_case(&window_scope.application) {
                matching_applications.push(object.clone());
            }
        }
        if matching_applications.is_empty() {
            return Err(
                "windowId application is no longer registered; observe visible windows again"
                    .into(),
            );
        }
        application_roots = matching_applications;
    }
    #[cfg(test)]
    let mut application_root_names = Vec::new();
    if application_window_scope.is_none()
        && let Some(terms) = query_terms.as_deref()
    {
        let mut matching_roots = Vec::new();
        for object in &application_roots {
            let Ok(app) = object.as_accessible_proxy(&accessibility).await else {
                continue;
            };
            let application_name = app.name().await.unwrap_or_default();
            let root_name = application_name.to_ascii_lowercase();
            if terms.iter().any(|term| root_name.contains(term)) {
                #[cfg(test)]
                application_root_names.push(if root_name.trim().is_empty() {
                    "<empty>".to_owned()
                } else {
                    root_name.clone()
                });
                matching_roots.push(object.clone());
                let owner_terms = window_title_terms(terms, &root_name);
                if owner_terms.len() >= 2 {
                    let children = app.get_children().await.unwrap_or_default();
                    for child in children.into_iter().take(32) {
                        let Ok(window) = child.as_accessible_proxy(&accessibility).await else {
                            continue;
                        };
                        let (role, name, states) = tokio::join!(
                            window.get_role_name(),
                            window.name(),
                            window.get_state(),
                        );
                        let role = role.unwrap_or_default().to_ascii_lowercase();
                        let name = name.unwrap_or_default().to_ascii_lowercase();
                        let states = states.unwrap_or_else(|_| StateSet::empty());
                        if matches!(role.as_str(), "frame" | "window" | "dialog")
                            && states.contains(State::Visible)
                            && states.contains(State::Showing)
                            && owner_terms
                                .iter()
                                .filter(|term| name.contains(term.as_str()))
                                .count()
                                >= 2
                        {
                            scoped_window_roots.push((child, application_name.clone()));
                        }
                    }
                }
            }
        }
        if !matching_roots.is_empty() {
            application_roots = matching_roots;
            app_scoped_query = true;
        }
    }
    let mut pending = VecDeque::new();
    let mut discovered = HashSet::new();
    let mut scoped_application_names = Vec::new();
    if !scoped_window_roots.is_empty() {
        for (object, application_name) in scoped_window_roots {
            if discovered.insert(object.clone()) {
                if !scoped_application_names.contains(&application_name) {
                    scoped_application_names.push(application_name.clone());
                }
                pending.push_back((
                    object,
                    1usize,
                    Some(application_name),
                    None::<Value>,
                    None::<ObjectRefOwned>,
                ));
            }
        }
    } else {
        for object in application_roots {
            if discovered.insert(object.clone()) {
                pending.push_back((
                    object,
                    0usize,
                    None::<String>,
                    None::<Value>,
                    None::<ObjectRefOwned>,
                ));
            }
        }
    }
    #[cfg(test)]
    let owner_window_scoped = application_window_scope.is_some()
        || (!discovered.is_empty()
            && pending
                .iter()
                .all(|(_, depth, application, _, _)| *depth > 0 && application.is_some()));

    let mut nodes = Vec::new();
    let mut applications = scoped_application_names;
    let mut focused_window = None;
    let mut active_window = None;
    let mut targets = HashMap::new();
    let mut windows = HashMap::new();
    let mut visible_windows = Vec::new();
    let focused_search = query_is_focused(query_terms.as_deref(), window_scoped);
    let document_content_query = focused_search
        && query_terms
            .as_deref()
            .is_some_and(query_prefers_document_content);
    let node_budget = if focused_search {
        MAX_SEARCH_NODES
    } else {
        MAX_NODES
    };
    let traversal_budget = if focused_search {
        FOCUSED_SEARCH_BUDGET
    } else {
        OBSERVATION_BUDGET
    };
    let depth_budget = if focused_search {
        MAX_SEARCH_DEPTH
    } else {
        MAX_DEPTH
    };
    let mut visited = 0usize;
    let mut truncated = roots_truncated;
    let mut query_ambiguous = false;
    let mut matched_actionable_count = 0usize;
    let mut scoped_window_root_seen = false;
    #[cfg(test)]
    let mut test_role_counts = HashMap::<String, usize>::new();
    #[cfg(test)]
    let mut test_visited_samples = Vec::<Value>::new();
    #[cfg(test)]
    let mut test_pruned_branches = Vec::<Value>::new();
    let started = Instant::now();
    while let Some((object, depth, parent_application, parent_window, parent_window_object)) =
        pending.pop_front()
    {
        if visited >= node_budget || started.elapsed() >= traversal_budget {
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
        // Focused searches read interfaces only after role/name/window fields
        // match the query. A long page otherwise pays one extra D-Bus call
        // for every irrelevant accessible node.
        let children_future = async {
            if focused_search && !document_content_query && depth < depth_budget {
                Some(accessible.get_children().await)
            } else {
                None
            }
        };
        let (role_result, states_result, name_result, prefetched_children) = tokio::join!(
            accessible.get_role_name(),
            accessible.get_state(),
            accessible.name(),
            children_future,
        );
        let role = role_result.unwrap_or_else(|_| "unknown".to_owned());
        #[cfg(test)]
        {
            *test_role_counts
                .entry(role.to_ascii_lowercase())
                .or_default() += 1;
        }
        let states = states_result.unwrap_or_else(|_| StateSet::empty());
        let role_lower = role.to_ascii_lowercase();
        let is_password = is_password_role(&role_lower);
        let is_window = matches!(
            role_lower.as_str(),
            "frame" | "window" | "dialog" | "alert" | "file chooser"
        );
        let visible = states.contains(State::Visible) && states.contains(State::Showing);
        let tree_visible = states.contains(State::Visible);
        let can_activate_role = role_can_activate(&role_lower);
        let full_raw_name = if is_password {
            "Protected field".to_owned()
        } else {
            name_result.unwrap_or_default()
        };
        let raw_name = bounded_accessible_name(full_raw_name.clone());
        let preliminary_application = if depth == 0 {
            (!raw_name.trim().is_empty()).then(|| raw_name.clone())
        } else {
            parent_application.clone()
        };
        let preliminary_window = if is_window {
            Some(json!({
                "role": role.clone(),
                "name": raw_name.clone(),
                "application": preliminary_application.clone(),
            }))
        } else {
            parent_window.clone()
        };
        let preliminary_node = json!({
            "role": role.clone(),
            "name": raw_name.clone(),
            "application": preliminary_application,
            "owningWindow": preliminary_window,
            "actions": [],
        });
        let preliminary_match = query_terms
            .as_ref()
            .is_none_or(|terms| query_matches(&preliminary_node, terms));
        let query_needs_action_names = query_terms.as_ref().is_none_or(|terms| {
            terms
                .iter()
                .any(|term| matches!(term.as_str(), "click" | "press" | "activate" | "open"))
        });
        let needs_interfaces = !focused_search
            || preliminary_match
            || is_window
            || (can_activate_role && query_needs_action_names);
        let interfaces = if needs_interfaces {
            match accessible.get_interfaces().await {
                Ok(interfaces) => interfaces,
                Err(_) => {
                    truncated = true;
                    continue;
                }
            }
        } else {
            InterfaceSet::empty()
        };
        let supports_click =
            interfaces.contains(Interface::Action) && role_can_activate(&role_lower);
        let supports_focus =
            interfaces.contains(Interface::Component) && states.contains(State::Focusable);
        let supports_text =
            interfaces.contains(Interface::EditableText) && !states.contains(State::ReadOnly);
        let actions = if supports_click && can_activate_role && query_needs_action_names {
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
        let observed_name = (!is_password).then(|| raw_name.clone());
        let name = if is_password {
            "Protected field".to_owned()
        } else {
            let raw_name = observed_name.clone().unwrap_or_default();
            let fallback = if raw_name.trim().is_empty()
                && (supports_text || can_activate_role || is_window)
            {
                Some(accessible_context_name(&accessible, &accessibility).await)
            } else {
                None
            };
            display_accessible_name(raw_name, fallback)
        };
        let identity_source = if full_raw_name.trim().is_empty() {
            name.as_str()
        } else {
            full_raw_name.as_str()
        };
        let identity = if is_window {
            Some(window_name_identity(identity_source))
        } else {
            None
        };
        let application = if depth == 0 {
            #[cfg(test)]
            application_root_names.push(if name.trim().is_empty() {
                "<empty>".to_owned()
            } else {
                name.clone()
            });
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
        let window_object = if is_window {
            Some(object.clone())
        } else {
            parent_window_object
        };
        let window_id = if is_window && visible && depth <= 1 {
            let identity = identity.as_deref().unwrap_or_default();
            let window_id = accessibility_window_id(session_id, &object, &role, identity);
            if scoped_window_id.as_deref() == Some(window_id.as_str()) {
                scoped_window_root_seen = true;
            }
            if !window_id_is_in_scope(Some(&window_id), scoped_window_id.as_deref()) {
                None
            } else if let Some(application) = application.as_ref()
                && (windows.len() < MAX_WINDOW_ROOTS || windows.contains_key(&window_id))
            {
                windows.insert(
                    window_id.clone(),
                    WindowRoot {
                        object: object.clone(),
                        application: application.clone(),
                        role: role.clone(),
                        identity: identity.to_owned(),
                    },
                );
                Some(window_id)
            } else {
                None
            }
        } else {
            parent_window
                .as_ref()
                .and_then(|window| window["windowId"].as_str())
                .map(ToOwned::to_owned)
        };
        let window = if is_window {
            Some(json!({
                "role": role.clone(),
                "name": name.clone(),
                "application": application.clone(),
                "windowId": window_id,
            }))
        } else {
            parent_window
        };
        let window_is_in_scope = window_id_is_in_scope(
            window
                .as_ref()
                .and_then(|window| window["windowId"].as_str()),
            scoped_window_id.as_deref(),
        );
        #[cfg(test)]
        if test_visited_samples.len() < 40 {
            test_visited_samples.push(json!({
                "depth":depth,
                "role":role_lower,
                "name":name,
                "application":application,
                "windowName":window.as_ref().and_then(|window|window["name"].as_str()),
                "windowId":window.as_ref().and_then(|window|window["windowId"].as_str()),
                "inScope":window_is_in_scope,
            }));
        }
        if is_window
            && visible
            && window_is_in_scope
            && depth <= 1
            && visible_windows.len() < MAX_WINDOW_ROOTS
            && let Some(window) = window.as_ref()
        {
            visible_windows.push(window.clone());
        }
        if is_window && visible && window_is_in_scope && states.contains(State::Active) {
            let is_window_manager = application
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("xfwm4"));
            if active_window.is_none() || !is_window_manager {
                active_window = window.clone();
            }
        }
        // Window-manager helpers can themselves expose a window role and a
        // focused state. Prefer a focused control's actual owning app window.
        if visible
            && window_is_in_scope
            && states.contains(State::Focused)
            && !is_window
            && focused_window.is_none()
        {
            focused_window = window.clone();
        }
        let mut skip_matched_target_subtree = false;
        let pointer_fallback_candidate = is_pointer_fallback_candidate(
            &role_lower,
            &name,
            interfaces.contains(Interface::Component),
            visible,
        );
        let focused_search_candidate = focused_search
            && window_is_in_scope
            && window.is_some()
            && tree_visible
            && (is_actionable || pointer_fallback_candidate || is_search_context_role(&role_lower));
        if focused_search
            && focused_query_role_is_leaf(&role_lower)
            && (role_lower == "static" || !name.trim().is_empty())
        {
            // Semantic leaves and static text cannot contain another
            // actionable desktop target. Long browser documents expose
            // hundreds of these nodes; fetching their children adds a D-Bus
            // round trip without improving focused control search.
            skip_matched_target_subtree = true;
        }
        if window_is_in_scope
            && (node_is_relevant(depth, is_window, visible, is_actionable)
                || focused_search_candidate)
        {
            // An offscreen match can guide scrolling, but it must not authorize
            // an action until a fresh observation confirms it is showing.
            let mut target_id = None;
            let exposes_text = interfaces.contains(Interface::Text)
                && matches!(
                    role_lower.as_str(),
                    "text"
                        | "text box"
                        | "text entry"
                        | "text area"
                        | "entry"
                        | "combo box"
                        | "paragraph"
                );
            let mut node = json!({
                "targetId": null,
                "depth": depth,
                "role": role,
                "name": name,
                "application": application.clone(),
                "owningWindow": if query.is_some() { window.clone() } else { None },
                "states": state_names(states),
                "actions": action_names,
                "pointerFallback": pointer_fallback_candidate,
                "pointerTarget": null,
                "editable": supports_text && !is_password,
                "value": null,
                "protected": is_password,
            });
            let matches_query = query_terms
                .as_ref()
                .is_none_or(|terms| query_matches(&node, terms));
            let screen_target_value = if matches_query
                && query.is_some()
                && (is_actionable || pointer_fallback_candidate)
            {
                query_screen_target(&accessible).await
            } else {
                None
            };
            if query.is_some() && (is_actionable || pointer_fallback_candidate) {
                // For query results this reports measured screen-rectangle
                // intersection, not merely AT-SPI's Visible state.
                node["onscreen"] = json!(screen_target_value.is_some());
            }
            let target_is_visible = query_can_issue_action_tokens(
                query.is_some(),
                is_actionable,
                visible,
                screen_target_value.is_some(),
            );
            if matches_query && target_is_visible {
                target_id = Some(accessibility_target_id(session_id, revision, visited));
                node["targetId"] = json!(target_id);
            }
            if matches_query
                && query.is_some()
                && visible
                && screen_target_value.is_some()
                && (target_id.is_some() || pointer_fallback_candidate)
            {
                node["pointerTarget"] = screen_target_value.clone().unwrap_or(Value::Null);
            }
            skip_matched_target_subtree |= query.is_some()
                && matches_query
                && target_id.is_some()
                && (supports_text || can_activate_role);
            if matches_query {
                if visible
                    && (supports_text || exposes_text)
                    && (query.is_none() || app_scoped_query)
                {
                    let value = read_text_value(&accessible, MAX_VALUE_CHARS).await;
                    node["value"] = protect_text_value(value, is_password)
                        .map(Value::String)
                        .unwrap_or(Value::Null);
                }
                if let Some(target_id) = target_id.as_ref() {
                    targets.insert(
                        target_id.clone(),
                        Target {
                            object: object.clone(),
                            role: role.clone(),
                            observed_name: observed_name.clone(),
                            owning_window: window.as_ref().and_then(|window| {
                                Some((
                                    window["role"].as_str()?.to_owned(),
                                    window["name"].as_str()?.to_owned(),
                                ))
                            }),
                            owning_window_object: window_object.clone(),
                            supports_click,
                            supports_focus,
                            supports_text,
                            is_password,
                            screen_bound: query.is_some(),
                        },
                    );
                }
                nodes.push(node);
            }
            if matches_query && (is_actionable || pointer_fallback_candidate) {
                matched_actionable_count += 1;
            }
            if query.is_some() && matched_actionable_count >= 2 {
                query_ambiguous = true;
                truncated = !pending.is_empty();
                break;
            }
        }
        // Pop-up menus are often transient top-level accessibles rather than
        // children of the panel. The reciprocal PopupFor relation is exposed
        // as ParentWindowOf on the visible owner frame, not on the menu
        // toggle itself, so follow it from visible app-scoped windows too.
        let checked_menu_control = matches!(role_lower.as_str(), "toggle button" | "menu button")
            && visible
            && states.contains(State::Checked);
        let mut embedded_document_found = false;
        if app_scoped_query && window_is_in_scope && visible && (checked_menu_control || is_window)
        {
            if depth >= depth_budget {
                truncated = true;
                continue;
            }
            for (relation, related_objects) in
                accessible.get_relation_set().await.unwrap_or_default()
            {
                if !should_follow_related_object(is_window, checked_menu_control, relation) {
                    continue;
                }
                for related in related_objects
                    .into_iter()
                    .filter(|related| !related.is_null())
                {
                    if relation == RelationType::Embeds {
                        embedded_document_found = true;
                    }
                    if visited + pending.len() >= node_budget {
                        truncated = true;
                        break;
                    }
                    if discovered.insert(related.clone()) {
                        let entry = (
                            related,
                            depth + 1,
                            application.clone(),
                            window.clone(),
                            window_object.clone(),
                        );
                        // Browser content queries need the document before the
                        // browser chrome; otherwise a long page can consume
                        // the focused-search budget on toolbar and tab nodes.
                        // Address-bar/toolbar queries keep the original order.
                        if relation == RelationType::Embeds {
                            if document_content_query {
                                pending.push_front(entry);
                            } else {
                                pending.push_back(entry);
                            }
                        } else {
                            pending.push_front(entry);
                        }
                    }
                }
            }
        }
        // Keep whole-desktop observations compact, but allow a focused
        // application query to inspect menus in that application's bounded
        // subtree. This is needed for visible application/category menus,
        // whose entries otherwise never become addressable to the agent.
        let skip_menu_subtree = (!app_scoped_query || query_skips_app_menus(&query_terms))
            && matches!(role_lower.as_str(), "menu bar" | "menu" | "menu item");
        #[cfg(test)]
        let child_prune_reason = if depth >= depth_budget {
            Some("depth_budget")
        } else if skip_menu_subtree {
            Some("menu_subtree")
        } else if skip_matched_target_subtree {
            Some("matched_target_subtree")
        } else if document_content_query && is_window && embedded_document_found {
            Some("embedded_document_window_children")
        } else if !(depth == 0
            || visible
            || (focused_search && window.is_some() && !is_window && tree_visible))
        {
            Some("invisible_ancestor")
        } else {
            None
        };
        #[cfg(test)]
        if test_pruned_branches.len() < 32
            && let Some(reason) = child_prune_reason
        {
            test_pruned_branches.push(json!({
                "depth":depth,
                "role":role_lower,
                "name":name,
                "reason":reason,
                "windowName":window.as_ref().and_then(|window|window["name"].as_str()),
                "windowId":window.as_ref().and_then(|window|window["windowId"].as_str()),
            }));
        }
        // Invisible background windows and their descendants cannot produce
        // actionable targets. Avoid walking their often large document trees
        // when a focused application observation is already budgeted.
        if depth < depth_budget
            && (depth == 0 || window_is_in_scope)
            && !skip_menu_subtree
            && !skip_matched_target_subtree
            && !(document_content_query && is_window && embedded_document_found)
            && (depth == 0
                || visible
                || (focused_search && window.is_some() && !is_window && tree_visible))
        {
            let children = match prefetched_children {
                Some(children) => children,
                None => accessible.get_children().await,
            };
            match children {
                Ok(children) => {
                    // Walk breadth-first so every application root gets a
                    // chance to contribute its foreground window before a
                    // large background tree consumes the observation budget.
                    // Bound discovered nodes as well as visited nodes so one
                    // high-fanout application cannot inflate the queue.
                    for child in children.into_iter().filter(|child| !child.is_null()) {
                        if visited + pending.len() >= node_budget {
                            truncated = true;
                            break;
                        }
                        if discovered.insert(child.clone()) {
                            pending.push_back((
                                child,
                                depth + 1,
                                application.clone(),
                                window.clone(),
                                window_object.clone(),
                            ));
                        }
                    }
                }
                Err(_) => {
                    truncated = true;
                }
            }
        } else if depth >= depth_budget
            && (visible || (focused_search && window.is_some() && !is_window && tree_visible))
            && !skip_menu_subtree
            && !skip_matched_target_subtree
            && !(document_content_query && is_window && embedded_document_found)
            && accessible.child_count().await.unwrap_or_default() > 0
        {
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
    withhold_unverified_query_tokens(
        &mut nodes,
        &mut targets,
        query.is_some(),
        query_ambiguous,
        truncated,
    );

    if scoped_window_id.is_some() && !scoped_window_root_seen {
        return Err(
            "windowId is no longer a visible child of its application; observe visible windows again"
                .into(),
        );
    }

    let value = json!({
        "sessionId": session_id,
        "revision": revision,
        "query": query,
        "applications": applications,
        "focusedWindow": focused_window.or(active_window),
        "windows": visible_windows,
        "truncated": truncated || !pending.is_empty(),
        "nodesVisited": visited,
        "queryAmbiguous": query_ambiguous,
        "nodes": nodes,
    });
    #[cfg(test)]
    let value = {
        let mut value = value;
        value["_testOwnerWindowScoped"] = json!(owner_window_scoped);
        value["_testRoleCounts"] = json!(test_role_counts);
        value["_testVisitedSamples"] = json!(test_visited_samples);
        value["_testPrunedBranches"] = json!(test_pruned_branches);
        value
    };
    Ok(Snapshot {
        value,
        targets,
        windows,
        #[cfg(test)]
        application_root_names,
    })
}

fn focused_search_result_is_actionable(
    search_query: bool,
    query_ambiguous: bool,
    truncated: bool,
) -> bool {
    !search_query || (!query_ambiguous && !truncated)
}

fn query_is_focused(terms: Option<&[String]>, window_scoped: bool) -> bool {
    window_scoped || terms.is_some_and(|terms| terms.len() > 1)
}

fn withhold_unverified_query_tokens(
    nodes: &mut [Value],
    targets: &mut HashMap<String, Target>,
    has_query: bool,
    query_ambiguous: bool,
    truncated: bool,
) {
    if focused_search_result_is_actionable(has_query, query_ambiguous, truncated) {
        return;
    }
    // A partial tree cannot prove uniqueness, and multiple matches cannot
    // safely identify a control. Keep descriptive nodes for refinement, but
    // withhold semantic and coordinate tokens until the result is complete.
    for node in nodes {
        if node["targetId"].is_string() {
            node["targetId"] = Value::Null;
        }
        node["pointerTarget"] = Value::Null;
    }
    targets.clear();
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
    let role = node["role"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let role_hints = terms
        .iter()
        .filter(|term| is_role_hint(term))
        .collect::<Vec<_>>();
    if !role_hints.is_empty() && !role_hints.iter().all(|hint| role_matches_hint(&role, hint)) {
        return false;
    }
    let mut fields = vec![
        node["role"].as_str().unwrap_or_default(),
        node["name"].as_str().unwrap_or_default(),
        node["application"].as_str().unwrap_or_default(),
    ];
    if let Some(actions) = node["actions"].as_array() {
        fields.extend(actions.iter().filter_map(Value::as_str));
    }
    if let Some(window) = node["owningWindow"].as_object() {
        fields.extend(
            ["role", "name", "application"]
                .into_iter()
                .filter_map(|key| window.get(key).and_then(Value::as_str)),
        );
    }
    let haystack = fields.join(" ").to_lowercase();
    terms
        .iter()
        .filter(|term| !is_role_hint(term))
        .all(|term| haystack.contains(term))
}

fn window_title_terms(terms: &[String], application_name: &str) -> Vec<String> {
    terms
        .iter()
        .filter(|term| {
            term.len() >= 4 && !is_role_hint(term) && !application_name.contains(term.as_str())
        })
        .cloned()
        .collect()
}

fn query_prefers_document_content(terms: &[String]) -> bool {
    let has_document_role = terms
        .iter()
        .any(|term| matches!(term.as_str(), "link" | "heading" | "paragraph" | "document"));
    let is_table_of_contents_toggle = ["label", "toggle", "table", "contents"]
        .into_iter()
        .all(|needle| terms.iter().any(|term| term == needle));
    has_document_role || is_table_of_contents_toggle
}

fn query_skips_app_menus(terms: &Option<Vec<String>>) -> bool {
    terms.as_deref().is_some_and(|terms| {
        terms
            .iter()
            .any(|term| matches!(term.as_str(), "link" | "heading" | "paragraph" | "document"))
    })
}

fn focused_query_role_is_leaf(role: &str) -> bool {
    matches!(
        role,
        "link"
            | "push button"
            | "toggle button"
            | "check box"
            | "radio button"
            | "tab"
            | "text"
            | "static"
            | "image"
    )
}

fn is_role_hint(term: &str) -> bool {
    matches!(
        term,
        "link"
            | "button"
            | "tab"
            | "checkbox"
            | "radio"
            | "field"
            | "textbox"
            | "text"
            | "window"
            | "dialog"
            | "heading"
            | "paragraph"
    )
}

fn role_matches_hint(role: &str, hint: &str) -> bool {
    match hint {
        "link" => role == "link",
        "button" => role.contains("button"),
        "tab" => role == "tab" || role == "page tab",
        "checkbox" => role == "check box",
        "radio" => role == "radio button",
        "field" | "textbox" | "text" => {
            matches!(
                role,
                "text" | "text box" | "text entry" | "text area" | "entry" | "combo box"
            )
        }
        "window" => matches!(role, "frame" | "window"),
        "dialog" => matches!(role, "dialog" | "alert" | "file chooser"),
        "heading" => role == "heading",
        "paragraph" => role == "paragraph",
        _ => true,
    }
}

fn is_search_context_role(role: &str) -> bool {
    matches!(
        role,
        "heading" | "paragraph" | "static" | "text" | "label" | "document web"
    )
}

fn is_pointer_fallback_candidate(
    role: &str,
    name: &str,
    has_component: bool,
    visible: bool,
) -> bool {
    let label = name.to_ascii_lowercase();
    role == "label"
        && has_component
        && visible
        && ["toggle", "menu"].iter().any(|term| label.contains(term))
}

fn role_can_activate(role: &str) -> bool {
    matches!(
        role,
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
    )
}

fn node_is_relevant(depth: usize, is_window: bool, visible: bool, actionable: bool) -> bool {
    depth == 0 || (visible && (is_window || actionable))
}

fn target_can_be_exposed(
    actionable: bool,
    visible: bool,
    geometry_required: bool,
    intersects_screen: bool,
) -> bool {
    actionable && visible && (!geometry_required || intersects_screen)
}

fn query_can_issue_action_tokens(
    query_present: bool,
    actionable: bool,
    visible: bool,
    intersects_screen: bool,
) -> bool {
    query_present && target_can_be_exposed(actionable, visible, true, intersects_screen)
}

fn window_id_is_in_scope(candidate: Option<&str>, selected: Option<&str>) -> bool {
    selected.is_none_or(|selected| candidate == Some(selected))
}

fn window_identity_matches(
    observed_role: &str,
    observed_identity: &str,
    current_role: &str,
    current_identity: &str,
) -> bool {
    observed_role.eq_ignore_ascii_case(current_role) && observed_identity == current_identity
}

fn should_follow_related_object(
    is_window: bool,
    checked_menu_control: bool,
    relation: RelationType,
) -> bool {
    if is_window {
        matches!(
            relation,
            RelationType::ParentWindowOf | RelationType::Embeds
        )
    } else {
        checked_menu_control
            && matches!(
                relation,
                RelationType::ControllerFor | RelationType::ParentWindowOf
            )
    }
}

fn accessibility_target_id(session_id: &str, revision: u64, node_index: usize) -> String {
    format!(
        "ax-{}-{revision}-{node_index}",
        session_id.chars().take(12).collect::<String>()
    )
}

fn accessibility_window_id(
    session_id: &str,
    object: &ObjectRefOwned,
    role: &str,
    name: &str,
) -> String {
    accessibility_window_id_from_parts(
        session_id,
        object.name_as_str().unwrap_or_default(),
        object.path_as_str(),
        role,
        name,
    )
}

fn accessibility_window_id_from_parts(
    session_id: &str,
    service: &str,
    path: &str,
    role: &str,
    identity: &str,
) -> String {
    let mut material = Vec::with_capacity(
        session_id.len() + service.len() + path.len() + role.len() + identity.len() + 40,
    );
    for component in [session_id, service, path, role, identity] {
        material.extend_from_slice(&(component.len() as u64).to_be_bytes());
        material.extend_from_slice(component.as_bytes());
    }
    let hash = crate::hashing::sha256(&material);
    format!(
        "axw-{}-{hash}",
        session_id.chars().take(12).collect::<String>()
    )
}

fn window_name_identity(name: &str) -> String {
    crate::hashing::sha256(name.as_bytes())
}

fn bounded_accessible_name(name: String) -> String {
    name.chars().take(MAX_NAME_CHARS).collect()
}

fn pointer_target_from_extents(extents: (i32, i32, i32, i32)) -> Option<Value> {
    let (x, y, width, height) = extents;
    if width <= 0 || height <= 0 {
        return None;
    }
    let right = x.checked_add(width)?;
    let bottom = y.checked_add(height)?;
    let left = x.max(0);
    let top = y.max(0);
    let right = right.min(super::WIDTH as i32);
    let bottom = bottom.min(super::HEIGHT as i32);
    if right <= left || bottom <= top {
        return None;
    }
    let center_x = left + (right - left) / 2;
    let center_y = top + (bottom - top) / 2;
    Some(json!({
        "x": center_x as f64 / (super::WIDTH - 1) as f64,
        "y": center_y as f64 / (super::HEIGHT - 1) as f64,
        "basis": "screen"
    }))
}

async fn query_screen_target(accessible: &AccessibleProxy<'_>) -> Option<Value> {
    let proxies = accessible.proxies().await.ok()?;
    let component = proxies.component().await.ok()?;
    let extents = component.get_extents(CoordType::Screen).await.ok()?;
    pointer_target_from_extents(extents)
}

fn unverified_click_result(action_name: &str) -> Value {
    json!({
        "effect": "performed",
        "verified": false,
        "action": action_name,
        "followUp": "Check the returned observation for the expected UI state, then run a fresh focused desktop_observe before the next action."
    })
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
    if let Some(expected_name) = target.observed_name.as_deref()
        && accessible.name().await.unwrap_or_default() != expected_name
    {
        return Err("Accessibility target identity changed; call desktop_observe again".into());
    }
    if let (Some((expected_role, expected_name)), Some(window_object)) = (
        target.owning_window.as_ref(),
        target.owning_window_object.as_ref(),
    ) {
        if &target.object != window_object
            && !target_is_descendant_of_window(&accessible, &accessibility, window_object).await?
        {
            return Err(
                "Accessibility target moved to another window; call desktop_observe again".into(),
            );
        }
        let window = window_object
            .as_accessible_proxy(&accessibility)
            .await
            .map_err(|_| {
                "Accessibility target window is stale; call desktop_observe again".to_owned()
            })?;
        let (role, name) = tokio::join!(window.get_role_name(), window.name());
        let role_matches = role.as_ref().is_ok_and(|role| role == expected_role);
        let current_raw_name = name.unwrap_or_default();
        let fallback = if current_raw_name.trim().is_empty() {
            Some(accessible_context_name(&window, &accessibility).await)
        } else {
            None
        };
        let current_name = display_accessible_name(current_raw_name, fallback);
        let name_matches = current_name == *expected_name;
        if !role_matches || !name_matches {
            return Err("Accessibility target window changed; call desktop_observe again".into());
        }
    }
    if !states.contains(State::Sensitive)
        || !states.contains(State::Visible)
        || !states.contains(State::Showing)
    {
        return Err("Accessibility target is not currently visible and sensitive".into());
    }
    if target.screen_bound
        && (target.supports_click || target.supports_focus || target.supports_text)
        && query_screen_target(&accessible).await.is_none()
    {
        return Err(
            "Accessibility target is outside the current screen; observe and scroll again".into(),
        );
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
            Ok(unverified_click_result(&actions[index].name))
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
            let (current_interfaces, current_states) =
                tokio::join!(accessible.get_interfaces(), accessible.get_state());
            let current_interfaces = current_interfaces.map_err(accessibility_error)?;
            let current_states = current_states.map_err(accessibility_error)?;
            if !text_edit_is_supported(
                target.supports_text,
                current_interfaces.contains(Interface::EditableText),
                current_states.contains(State::ReadOnly),
            ) {
                return Err(
                    "Target is read-only or no longer supports accessible text editing; call desktop_observe again".to_owned(),
                );
            }
            if !current_states.contains(State::Sensitive)
                || !current_states.contains(State::Visible)
                || !current_states.contains(State::Showing)
            {
                return Err("Accessibility text target is no longer visible and sensitive".into());
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
            let mut read_back =
                protect_text_value(read_text_value(&accessible, 512).await, target.is_password);
            if !target.is_password && read_back.as_deref() != Some(text_value) {
                tokio::time::sleep(Duration::from_millis(40)).await;
                read_back =
                    protect_text_value(read_text_value(&accessible, 512).await, target.is_password);
            }
            let verified = read_back.as_deref() == Some(text_value);
            Ok(json!({
                "effect":"performed",
                "verified":verified,
                "value":read_back,
                "protected":target.is_password,
                "followUp":"Check the returned observation for the expected UI state, then run a fresh focused desktop_observe before the next action."
            }))
        }
        _ => Err("action must be click, focus, or set_text".to_owned()),
    }
}

async fn target_is_descendant_of_window(
    accessible: &AccessibleProxy<'_>,
    accessibility: &atspi::zbus::Connection,
    expected_window: &ObjectRefOwned,
) -> Result<bool, String> {
    let mut parent = accessible
        .parent()
        .await
        .map_err(|_| "Accessibility target is stale; call desktop_observe again".to_owned())?;
    let mut owner_matches = Vec::with_capacity(MAX_SEARCH_DEPTH);
    for _ in 0..MAX_SEARCH_DEPTH {
        if parent.is_null() {
            return Ok(owner_is_within_ancestor_limit(
                &owner_matches,
                MAX_SEARCH_DEPTH,
            ));
        }
        let matches_owner = &parent == expected_window;
        owner_matches.push(matches_owner);
        if matches_owner {
            break;
        }
        let ancestor = parent
            .as_accessible_proxy(accessibility)
            .await
            .map_err(|_| "Accessibility target is stale; call desktop_observe again".to_owned())?;
        parent = ancestor
            .parent()
            .await
            .map_err(|_| "Accessibility target is stale; call desktop_observe again".to_owned())?;
    }
    Ok(owner_is_within_ancestor_limit(
        &owner_matches,
        MAX_SEARCH_DEPTH,
    ))
}

fn owner_is_within_ancestor_limit(matches: &[bool], limit: usize) -> bool {
    matches
        .iter()
        .take(limit)
        .any(|matches_owner| *matches_owner)
}

fn state_names(states: StateSet) -> Vec<&'static str> {
    states.iter().map(|state| state.to_static_str()).collect()
}

fn protect_text_value(value: Option<String>, is_password: bool) -> Option<String> {
    if is_password { None } else { value }
}

fn text_edit_is_supported(
    observed_support: bool,
    currently_editable: bool,
    currently_read_only: bool,
) -> bool {
    observed_support && currently_editable && !currently_read_only
}

fn display_accessible_name(raw_name: String, fallback: Option<String>) -> String {
    if raw_name.trim().is_empty() {
        fallback.unwrap_or(raw_name)
    } else {
        raw_name
    }
}

fn is_password_role(role: &str) -> bool {
    role.to_ascii_lowercase().contains("password")
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
    use super::{
        MAX_DEPTH, MAX_NAME_CHARS, MAX_SEARCH_DEPTH, RelationType, accessibility_target_id,
        accessibility_window_id_from_parts, bounded_accessible_name, display_accessible_name,
        focused_query_role_is_leaf, focused_search_result_is_actionable, is_password_role,
        is_pointer_fallback_candidate, node_is_relevant, owner_is_within_ancestor_limit,
        pointer_target_from_extents, protect_text_value, query_can_issue_action_tokens,
        query_is_focused, query_matches, query_prefers_document_content, query_skips_app_menus,
        role_matches_hint, should_follow_related_object, target_can_be_exposed,
        text_edit_is_supported, unverified_click_result, validate_query, window_id_is_in_scope,
        window_identity_matches, window_name_identity, window_title_terms,
        withhold_unverified_query_tokens,
    };
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
        let text_copy = json!({"role":"static","name":"Observed target link"});
        let link = json!({"role":"link","name":"Observed target link"});
        let link_query = ["observed".into(), "target".into(), "link".into()];
        assert!(!query_matches(&text_copy, &link_query));
        assert!(query_matches(&link, &link_query));
        assert!(role_matches_hint("push button", "button"));
        assert!(role_matches_hint("entry", "textbox"));
        assert!(role_matches_hint("combo box", "field"));
        assert!(query_matches(
            &json!({"role":"entry","name":"Address and search bar"}),
            &["textbox".into(), "address".into()]
        ));
        let window_scoped_link = json!({
            "role":"link",
            "name":"Observed chapter link",
            "application":"Google Chrome for Testing",
            "owningWindow":{"role":"frame","name":"The Rust Programming Language","application":"Google Chrome for Testing"}
        });
        assert!(query_matches(
            &window_scoped_link,
            &[
                "the rust programming language".into(),
                "chapter".into(),
                "link".into()
            ]
        ));
        let other_window_link = json!({
            "role":"link",
            "name":"Observed chapter link",
            "application":"Google Chrome for Testing",
            "owningWindow":{"role":"frame","name":"Other browser window","application":"Google Chrome for Testing"}
        });
        assert!(!query_matches(
            &other_window_link,
            &[
                "the rust programming language".into(),
                "chapter".into(),
                "link".into()
            ]
        ));
    }

    #[test]
    fn incomplete_or_ambiguous_focused_search_cannot_authorize_actions() {
        assert!(focused_search_result_is_actionable(false, true, true));
        assert!(focused_search_result_is_actionable(true, false, false));
        assert!(!focused_search_result_is_actionable(true, true, false));
        assert!(!focused_search_result_is_actionable(true, false, true));

        let mut nodes = vec![json!({
            "targetId":"stale",
            "pointerTarget":{"x":0.5,"y":0.5}
        })];
        let mut targets = std::collections::HashMap::new();
        withhold_unverified_query_tokens(&mut nodes, &mut targets, true, false, true);
        assert!(nodes[0]["targetId"].is_null());
        assert!(nodes[0]["pointerTarget"].is_null());
        assert!(targets.is_empty());

        for (ambiguous, truncated) in [(true, false), (false, true)] {
            let mut pointer_fallback = vec![json!({
                "targetId":null,
                "pointerFallback":true,
                "pointerTarget":{"x":0.25,"y":0.75}
            })];
            let mut targets = std::collections::HashMap::new();
            withhold_unverified_query_tokens(
                &mut pointer_fallback,
                &mut targets,
                true,
                ambiguous,
                truncated,
            );
            assert!(pointer_fallback[0]["targetId"].is_null());
            assert!(pointer_fallback[0]["pointerTarget"].is_null());
            assert!(targets.is_empty());
        }
    }

    #[test]
    fn hidden_windows_and_controls_are_not_observed_or_targetable() {
        assert!(node_is_relevant(0, true, false, false));
        assert!(!node_is_relevant(1, true, false, true));
        assert!(!node_is_relevant(2, false, false, true));
        assert!(node_is_relevant(2, false, true, true));
    }

    #[test]
    fn frame_embeds_relation_reaches_remote_web_content_tree() {
        assert!(should_follow_related_object(
            true,
            false,
            RelationType::Embeds
        ));
        assert!(should_follow_related_object(
            true,
            false,
            RelationType::ParentWindowOf
        ));
        assert!(!should_follow_related_object(
            false,
            false,
            RelationType::Embeds
        ));
        assert!(should_follow_related_object(
            false,
            true,
            RelationType::ControllerFor
        ));
    }

    #[test]
    fn password_text_is_never_exposed_by_observations_or_action_readback() {
        let sentinel = "never-expose-password-canary".to_owned();
        let password = is_password_role("Password Text");
        assert!(password);
        let observed = protect_text_value(Some(sentinel.clone()), password);
        let read_back = protect_text_value(Some(sentinel.clone()), password);
        assert_eq!(observed, None);
        assert_eq!(read_back, None);
        assert!(!is_password_role("text entry"));
        assert_eq!(
            protect_text_value(Some("ordinary text".into()), false).as_deref(),
            Some("ordinary text")
        );
    }

    #[test]
    fn text_edit_rechecks_current_read_only_state_and_interface() {
        assert!(text_edit_is_supported(true, true, false));
        assert!(!text_edit_is_supported(true, true, true));
        assert!(!text_edit_is_supported(true, false, false));
        assert!(!text_edit_is_supported(false, true, false));
    }

    #[test]
    fn synthesized_accessible_name_is_revalidated_consistently() {
        assert_eq!(
            display_accessible_name(String::new(), Some("Labeled dialog".into())),
            "Labeled dialog"
        );
        assert_eq!(
            display_accessible_name("Raw title".into(), Some("Child label".into())),
            "Raw title"
        );
    }

    #[test]
    fn focused_search_owner_check_covers_the_full_search_depth() {
        let mut ancestors = vec![false; MAX_DEPTH + 1];
        ancestors.push(true);
        assert!(!owner_is_within_ancestor_limit(&ancestors, MAX_DEPTH));
        assert!(owner_is_within_ancestor_limit(&ancestors, MAX_SEARCH_DEPTH));
    }

    #[test]
    fn pointer_fallback_target_uses_visible_screen_center() {
        assert_eq!(
            pointer_target_from_extents((100, 200, 40, 20)),
            Some(json!({
                "x": 120.0 / 1279.0,
                "y": 210.0 / 799.0,
                "basis": "screen"
            }))
        );
        assert_eq!(
            pointer_target_from_extents((-10, 10, 20, 20)),
            Some(json!({
                "x": 5.0 / 1279.0,
                "y": 20.0 / 799.0,
                "basis": "screen"
            }))
        );
        assert!(pointer_target_from_extents((-20, 10, 10, 20)).is_none());
        assert!(pointer_target_from_extents((10, 10, 0, 20)).is_none());
        assert_eq!(
            pointer_target_from_extents((1270, 10, 20, 20)),
            Some(json!({
                "x": 1275.0 / 1279.0,
                "y": 20.0 / 799.0,
                "basis": "screen"
            }))
        );
    }

    #[test]
    fn pointer_fallback_is_limited_to_visible_component_toggle_or_menu_labels() {
        assert!(is_pointer_fallback_candidate(
            "label",
            "Toggle Table of Contents",
            true,
            true
        ));
        assert!(is_pointer_fallback_candidate(
            "label",
            "Main menu",
            true,
            true
        ));
        assert!(!is_pointer_fallback_candidate(
            "label",
            "Chapter title",
            true,
            true
        ));
        assert!(!is_pointer_fallback_candidate(
            "label",
            "Toggle Table of Contents",
            false,
            true
        ));
        assert!(!is_pointer_fallback_candidate(
            "label",
            "Toggle Table of Contents",
            true,
            false
        ));
        assert!(!is_pointer_fallback_candidate(
            "text",
            "Toggle Table of Contents",
            true,
            true
        ));
    }

    #[test]
    fn owner_title_scope_ignores_application_and_role_terms() {
        let terms =
            validate_query("Chrome The Rust Programming Language Understanding Ownership link")
                .unwrap();
        let owner_terms = window_title_terms(&terms, "google chrome for testing");
        assert_eq!(
            owner_terms,
            [
                "rust",
                "programming",
                "language",
                "understanding",
                "ownership"
            ]
        );
        assert!(window_title_terms(&["chrome".into(), "link".into()], "google chrome").is_empty());
    }

    #[test]
    fn focused_document_queries_prioritize_embedded_web_content() {
        assert!(query_prefers_document_content(&[
            "chrome".into(),
            "slice".into(),
            "link".into()
        ]));
        assert!(query_prefers_document_content(&[
            "chrome".into(),
            "toggle".into(),
            "label".into(),
            "table".into(),
            "contents".into()
        ]));
        assert!(!query_prefers_document_content(&[
            "chrome".into(),
            "address".into(),
            "textbox".into()
        ]));
        assert!(!query_skips_app_menus(&Some(vec![
            "app".into(),
            "menu".into(),
            "label".into()
        ])));
        assert!(query_skips_app_menus(&Some(vec![
            "chrome".into(),
            "link".into()
        ])));
        assert!(!query_prefers_document_content(&[
            "chrome".into(),
            "main".into(),
            "menu".into(),
            "label".into()
        ]));
    }

    #[test]
    fn focused_query_prunes_named_semantic_leaves_but_keeps_containers() {
        for role in ["link", "push button", "tab", "text", "image"] {
            assert!(focused_query_role_is_leaf(role), "{role}");
        }
        assert!(focused_query_role_is_leaf("static"));
        for role in ["frame", "panel", "list", "menu", "table"] {
            assert!(!focused_query_role_is_leaf(role), "{role}");
        }
    }

    #[test]
    fn one_word_window_scoped_queries_get_focused_search_budget() {
        assert!(query_is_focused(Some(&["link".into()]), true));
        assert!(!query_is_focused(Some(&["link".into()]), false));
        assert!(query_is_focused(
            Some(&["slice".into(), "link".into()]),
            false
        ));
    }

    #[test]
    fn offscreen_and_unmeasured_controls_never_receive_action_tokens() {
        assert!(target_can_be_exposed(true, true, false, false));
        assert!(target_can_be_exposed(true, true, true, true));
        assert!(!target_can_be_exposed(true, false, true, true));
        assert!(!target_can_be_exposed(true, true, true, false));
        assert!(!target_can_be_exposed(false, true, true, true));
    }

    #[test]
    fn broad_snapshots_cannot_issue_unmeasured_action_tokens() {
        assert!(!query_can_issue_action_tokens(false, true, true, true));
        assert!(!query_can_issue_action_tokens(true, true, true, false));
        assert!(!query_can_issue_action_tokens(true, true, false, true));
        assert!(query_can_issue_action_tokens(true, true, true, true));
    }

    #[test]
    fn semantic_click_result_never_claims_verified_ui_change() {
        let result = unverified_click_result("Click");
        assert_eq!(result["effect"], "performed");
        assert_eq!(result["action"], "Click");
        assert_eq!(result["verified"], false);
        assert!(result["followUp"].as_str().is_some());
    }

    #[test]
    fn accessibility_target_ids_are_observation_revision_bound() {
        let first = accessibility_target_id("0123456789abcdef", 17, 3);
        let next = accessibility_target_id("0123456789abcdef", 18, 3);
        assert_eq!(first, "ax-0123456789ab-17-3");
        assert_ne!(first, next);
    }

    #[test]
    fn window_ids_are_session_scoped_object_selectors() {
        let first = accessibility_window_id_from_parts(
            "session-a",
            ":1.8",
            "/window/3",
            "frame",
            "References and Borrowing",
        );
        assert_eq!(
            first,
            accessibility_window_id_from_parts(
                "session-a",
                ":1.8",
                "/window/3",
                "frame",
                "References and Borrowing",
            )
        );
        assert_ne!(
            first,
            accessibility_window_id_from_parts(
                "session-a",
                ":1.8",
                "/window/4",
                "frame",
                "References and Borrowing",
            )
        );
        assert_ne!(
            first,
            accessibility_window_id_from_parts(
                "session-b",
                ":1.8",
                "/window/3",
                "frame",
                "References and Borrowing",
            )
        );
        assert_ne!(
            first,
            accessibility_window_id_from_parts(
                "session-a",
                ":1.8",
                "/window/3",
                "frame",
                "Downloads",
            )
        );
    }

    #[test]
    fn window_selector_rejects_reused_role_with_changed_identity() {
        assert!(window_identity_matches(
            "frame",
            "References and Borrowing",
            "FRAME",
            "References and Borrowing",
        ));
        assert!(!window_identity_matches(
            "frame",
            "References and Borrowing",
            "frame",
            "Downloads",
        ));
    }

    #[test]
    fn long_window_titles_use_bounded_display_and_full_identity() {
        let mut first = "t".repeat(MAX_NAME_CHARS);
        let mut replacement = first.clone();
        first.push('a');
        replacement.push('b');
        assert_eq!(
            bounded_accessible_name(first.clone()).chars().count(),
            MAX_NAME_CHARS
        );
        assert_eq!(
            bounded_accessible_name(first.clone()),
            bounded_accessible_name(replacement.clone()),
        );
        let first_identity = window_name_identity(&first);
        let replacement_identity = window_name_identity(&replacement);
        assert_ne!(first_identity, replacement_identity);
        assert!(!window_identity_matches(
            "frame",
            &first_identity,
            "frame",
            &replacement_identity,
        ));
    }

    #[test]
    fn scoped_observation_withholds_sibling_window_candidates() {
        let selected = "window-selected";
        assert!(window_id_is_in_scope(Some(selected), Some(selected)));
        assert!(!window_id_is_in_scope(
            Some("window-sibling"),
            Some(selected)
        ));
        assert!(!window_id_is_in_scope(None, Some(selected)));
        assert!(window_id_is_in_scope(None, None));
    }
}
