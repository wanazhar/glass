use base64::{Engine, engine::general_purpose::STANDARD};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::Mutex;

use super::cdp::CdpClient;
use super::chrome::{
    ChromeProcess, check_chrome_health, detect_chrome, get_ws_url, launch_chrome_with_options,
};
use super::dom::{
    AxNode, CompactAxNode, CompactInteractiveElement, DomNode, find_interactive_elements,
    format_tree, parse_accessibility_tree, parse_dom_tree, project_compact_accessibility,
};
use super::mouse::{MouseEngine, Point};
use super::profile::ProfileManager;

pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

/// Maximum UTF-8 byte length of visible text returned by a compact observation.
pub const COMPACT_TEXT_MAX_BYTES: usize = 16 * 1024;
const TEXT_TRUNCATION_MARKER: &str = "\n[truncated]";
const COMPACT_PAGE_STATE_EXPRESSION: &str = "JSON.stringify({url: location.href, title: document.title, ready_state: document.readyState, text: document.body ? document.body.innerText : ''})";

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub port: u16,
    pub chrome_path: Option<PathBuf>,
    pub profile: String,
    pub incognito: bool,
    pub headed: bool,
    pub interaction_mode: InteractionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InteractionMode {
    Human,
    Fast,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            port: 9222,
            chrome_path: None,
            profile: "default".to_string(),
            incognito: false,
            headed: false,
            interaction_mode: InteractionMode::Human,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub ready_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveElement {
    pub reference: String,
    pub role: String,
    pub name: String,
    pub description: String,
    pub backend_dom_node_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessibilitySnapshot {
    pub page: PageInfo,
    pub roots: Vec<AxNode>,
    pub interactive: Vec<InteractiveElement>,
}

/// Bounded accessibility state included with compact page observations.
#[derive(Debug, Clone, Serialize)]
pub struct CompactAccessibilitySnapshot {
    pub page: PageInfo,
    pub roots: Vec<CompactAxNode>,
    pub interactive: Vec<CompactInteractiveElement>,
    #[serde(skip_serializing_if = "is_false")]
    pub truncated: bool,
}

/// Structured page state. Default observations omit optional deep data.
#[derive(Debug, Clone, Serialize)]
pub struct PageContext {
    pub page: PageInfo,
    pub text: String,
    /// Full DOM data is included only by an explicit deep-DOM observation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomNode>,
    pub accessibility: CompactAccessibilitySnapshot,
    /// Base64 PNG data is populated only when visual context is explicitly requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
}

#[derive(Debug, Clone)]
struct CompactPageContext {
    page: PageInfo,
    text: String,
    accessibility: CompactAccessibilitySnapshot,
}

impl CompactPageContext {
    fn into_page_context(self) -> PageContext {
        PageContext {
            page: self.page,
            text: self.text,
            dom: None,
            accessibility: self.accessibility,
            screenshot: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EvaluatedPageState {
    url: String,
    title: String,
    #[serde(default)]
    ready_state: String,
    text: String,
}

impl AccessibilitySnapshot {
    pub fn format(&self) -> String {
        let mut output = format!(
            "url: {}\ntitle: {}\nreadyState: {}\n\n{}",
            self.page.url,
            self.page.title,
            self.page.ready_state,
            format_tree(&self.roots, 0)
        );
        if !self.interactive.is_empty() {
            output.push_str("\nInteractive elements:\n");
            for element in &self.interactive {
                output.push_str(&format!(
                    "{} [{}] {}\n",
                    element.reference, element.role, element.name
                ));
            }
        }
        output
    }
}

/// A browser process, one CDP page connection, and its profile state.
pub struct BrowserSession {
    cdp: CdpClient,
    chrome: Option<ChromeProcess>,
    profile_manager: ProfileManager,
    profile: String,
    interaction_mode: InteractionMode,
    mouse: MouseEngine,
    pointer: Mutex<Option<Point>>,
    page_revision: Arc<AtomicU64>,
    observation_cache: Mutex<Option<CachedObservation>>,
}

struct CachedObservation {
    revision: u64,
    context: CompactPageContext,
}

impl BrowserSession {
    pub async fn start(options: &SessionOptions) -> BrowserResult<Self> {
        ProfileManager::validate_name(&options.profile)?;
        let profile_manager = ProfileManager::new();
        let profile_dir = if options.incognito {
            None
        } else {
            Some(profile_manager.chrome_data_dir(&options.profile))
        };

        let mut chrome = None;
        if !check_chrome_health(options.port).await {
            let chrome_path = options
                .chrome_path
                .clone()
                .or_else(detect_chrome)
                .ok_or("Chrome/Chromium not found; install it or pass --chrome-path")?;
            chrome = Some(
                launch_chrome_with_options(
                    &chrome_path,
                    options.port,
                    profile_dir.as_deref(),
                    options.headed,
                )
                .await?,
            );
        }

        let ws_url = match wait_for_ws_url(options.port).await {
            Ok(url) => url,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };
        let cdp = match CdpClient::connect(&ws_url).await {
            Ok(cdp) => cdp,
            Err(error) => {
                if let Some(process) = chrome.as_mut() {
                    let _ = process.shutdown().await;
                }
                return Err(error);
            }
        };

        let setup = cdp.enable_observation_events().await;
        if let Err(error) = setup {
            cdp.close().await;
            if let Some(process) = chrome.as_mut() {
                let _ = process.shutdown().await;
            }
            return Err(Box::new(error));
        }

        let page_revision = Arc::new(AtomicU64::new(1));
        let mut events = cdp.subscribe_events();
        let revision_for_events = Arc::clone(&page_revision);
        tokio::spawn(async move {
            while let Ok(event) = events.recv().await {
                if context_event_invalidates_observation(&event.method) {
                    revision_for_events.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        Ok(Self {
            cdp,
            chrome,
            profile_manager,
            profile: options.profile.clone(),
            interaction_mode: options.interaction_mode,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision,
            observation_cache: Mutex::new(None),
        })
    }

    pub fn cdp(&self) -> &CdpClient {
        &self.cdp
    }

    pub fn profile_manager(&self) -> &ProfileManager {
        &self.profile_manager
    }

    pub fn profile_name(&self) -> &str {
        &self.profile
    }

    pub async fn close(mut self) -> BrowserResult<()> {
        self.cdp.close().await;
        if let Some(process) = self.chrome.as_mut() {
            process.shutdown().await?;
        }
        self.chrome = None;
        Ok(())
    }

    pub async fn page_info(&self) -> BrowserResult<PageInfo> {
        let raw = self
            .cdp
            .evaluate(
                "JSON.stringify({url: location.href, title: document.title, ready_state: document.readyState})",
            )
            .await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("document state evaluation returned a non-string value")?;
        Ok(serde_json::from_str(json)?)
    }

    pub async fn navigate(&self, url: &str) -> BrowserResult<PageInfo> {
        let url = normalize_url(url);
        let mut events = self.cdp.subscribe_events();
        self.cdp.navigate(&url).await?;

        let wait = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                match events.recv().await {
                    Ok(event) if event.method == "Page.loadEventFired" => break Ok(()),
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(error) => break Err(format!("CDP event stream closed: {error}")),
                }
            }
        })
        .await;
        match wait {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => return Err("navigation timed out waiting for Page.loadEventFired".into()),
        }
        let page = self.page_info().await?;
        self.invalidate_observation();
        Ok(page)
    }

    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        let result = self.evaluate_value(expression).await;
        // Arbitrary JavaScript may mutate DOM, styles, form state, or history.
        // Invalidate synchronously so the next cached observation cannot race
        // the asynchronous CDP mutation event stream.
        self.invalidate_observation();
        result
    }

    pub async fn text(&self) -> BrowserResult<String> {
        let value = self
            .evaluate_value("document.body ? document.body.innerText : ''")
            .await?;
        Ok(value.as_str().unwrap_or_default().to_string())
    }

    /// Collect compact page context without a deep DOM or screenshot.
    pub async fn observe(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, true).await
    }

    /// Collect compact context and explicitly include the full DOM tree.
    pub async fn observe_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, true).await
    }

    /// Collect structured context and explicitly include a current screenshot.
    pub async fn observe_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, true).await
    }

    /// Collect context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, true).await
    }

    /// Collect fresh compact context, bypassing the compact-context cache.
    pub async fn observe_fresh(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, false, false).await
    }

    /// Collect fresh context and explicitly include the full DOM tree.
    pub async fn observe_fresh_with_dom(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, false, false).await
    }

    /// Collect fresh structured context and explicitly include a screenshot.
    pub async fn observe_fresh_with_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(false, true, false).await
    }

    /// Collect fresh context with both explicitly requested deep DOM and screenshot data.
    pub async fn observe_fresh_with_dom_and_screenshot(&self) -> BrowserResult<PageContext> {
        self.observe_internal(true, true, false).await
    }

    async fn observe_internal(
        &self,
        include_dom: bool,
        include_screenshot: bool,
        use_cache: bool,
    ) -> BrowserResult<PageContext> {
        let mut context = self
            .compact_observation(use_cache)
            .await?
            .into_page_context();
        if include_dom {
            let raw = self.cdp.get_deep_document().await?;
            context.dom = Some(
                parse_dom_tree(&raw)
                    .ok_or("CDP deep DOM response contained no parseable root node")?,
            );
        }
        if include_screenshot {
            context.screenshot = Some(self.screenshot_base64().await?);
        }
        Ok(context)
    }

    async fn compact_observation(&self, use_cache: bool) -> BrowserResult<CompactPageContext> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        if use_cache {
            let cached_context = {
                let cache = self.observation_cache.lock().await;
                cache
                    .as_ref()
                    .filter(|cached| cached.revision == revision)
                    .map(|cached| cached.context.clone())
            };
            if let Some(context) = cached_context {
                return Ok(context);
            }
        }

        let (page_state, accessibility) =
            tokio::join!(self.compact_page_state(), self.cdp.get_accessibility_tree(),);
        let page_state = page_state?;
        let accessibility_raw = accessibility?;
        let page = PageInfo {
            url: page_state.url,
            title: page_state.title,
            ready_state: page_state.ready_state,
        };
        let full_roots = parse_accessibility_tree(&accessibility_raw);
        let compact_accessibility = project_compact_accessibility(&full_roots);
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            roots: compact_accessibility.roots,
            interactive: compact_accessibility.interactive,
            truncated: compact_accessibility.truncated,
        };
        let context = CompactPageContext {
            page,
            text: truncate_visible_text(&page_state.text, COMPACT_TEXT_MAX_BYTES),
            accessibility,
        };
        *self.observation_cache.lock().await = Some(CachedObservation {
            revision,
            context: context.clone(),
        });
        Ok(context)
    }

    async fn compact_page_state(&self) -> BrowserResult<EvaluatedPageState> {
        let raw = self.cdp.evaluate(COMPACT_PAGE_STATE_EXPRESSION).await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("compact page-state evaluation returned a non-string value")?;
        Ok(serde_json::from_str(json)?)
    }

    pub async fn screenshot_png(&self) -> BrowserResult<Vec<u8>> {
        let data = self.screenshot_base64().await?;
        Ok(STANDARD.decode(data.as_bytes())?)
    }

    /// Capture a PNG while preserving CDP's base64 payload for image APIs.
    pub async fn screenshot_base64(&self) -> BrowserResult<String> {
        Ok(self.cdp.screenshot("png").await?)
    }

    pub async fn scroll(&self, dx: f64, dy: f64) -> BrowserResult<()> {
        self.cdp.scroll_by(dx, dy).await?;
        self.invalidate_observation();
        Ok(())
    }

    pub async fn snapshot(&self) -> BrowserResult<AccessibilitySnapshot> {
        let raw = self.cdp.get_accessibility_tree().await?;
        let roots = parse_accessibility_tree(&raw);
        let interactive = interactive_elements(&roots);
        Ok(AccessibilitySnapshot {
            page: self.page_info().await?,
            roots,
            interactive,
        })
    }

    pub async fn click(&self, target: &str) -> BrowserResult<String> {
        let element = self.resolve_element(target).await?;
        let model = match (element.node_id, element.backend_dom_node_id) {
            (Some(node_id), _) => self.cdp.get_box_model(node_id).await?,
            (_, Some(backend_node_id)) => {
                self.cdp.get_box_model_for_backend(backend_node_id).await?
            }
            _ => return Err(format!("element has no DOM reference: {target}").into()),
        };
        let (x, y) = center_of_box_model(&model)?;
        let point = Point { x, y };
        let mut pointer = self.pointer.lock().await;
        let start = match (self.interaction_mode, *pointer) {
            (_, Some(point)) => point,
            (InteractionMode::Human, None) => self
                .viewport_center()
                .await
                .unwrap_or(Point { x: 640.0, y: 360.0 }),
            (InteractionMode::Fast, None) => point,
        };
        let path = interaction_path(self.interaction_mode, &self.mouse, start, point);
        if self.interaction_mode == InteractionMode::Human && pointer.is_none() {
            self.cdp
                .dispatch_mouse_event("mouseMoved", start.x, start.y, None, None)
                .await?;
        }
        for window in path.windows(2) {
            let next = window[1];
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                .await?;
        }
        for event in self.mouse.generate_click_events(point) {
            self.cdp
                .dispatch_mouse_event(
                    &event.event_type,
                    event.x,
                    event.y,
                    Some(&event.button),
                    Some(event.click_count),
                )
                .await?;
            if self.interaction_mode == InteractionMode::Human && event.event_type == "mousePressed"
            {
                tokio::time::sleep(self.mouse.click_delay()).await;
            }
        }
        *pointer = Some(point);
        self.invalidate_observation();
        Ok(element.label)
    }

    pub async fn type_text(&self, text: &str, target: Option<&str>) -> BrowserResult<()> {
        if let Some(target) = target {
            self.click(target).await?;
        }
        self.cdp.insert_text(text).await?;
        self.invalidate_observation();
        Ok(())
    }

    async fn viewport_center(&self) -> BrowserResult<Point> {
        let value = self
            .evaluate_value("[window.innerWidth / 2, window.innerHeight / 2]")
            .await?;
        let coordinates = value
            .as_array()
            .filter(|coordinates| coordinates.len() == 2)
            .ok_or("viewport evaluation returned invalid coordinates")?;
        let x = coordinates[0]
            .as_f64()
            .ok_or("viewport width was not numeric")?;
        let y = coordinates[1]
            .as_f64()
            .ok_or("viewport height was not numeric")?;
        Ok(Point { x, y })
    }

    async fn evaluate_value(&self, expression: &str) -> BrowserResult<Value> {
        let raw = self.cdp.evaluate(expression).await?;
        runtime_value(&raw)
    }

    fn invalidate_observation(&self) {
        self.page_revision.fetch_add(1, Ordering::Relaxed);
    }

    async fn resolve_element(&self, target: &str) -> BrowserResult<ResolvedElement> {
        let target = target.trim().trim_matches('"');
        if target.is_empty() {
            return Err("element target cannot be empty".into());
        }

        let snapshot = self.snapshot().await?;
        let lower = target.to_lowercase();
        let by_reference = snapshot
            .interactive
            .iter()
            .find(|element| element.reference.eq_ignore_ascii_case(target));
        let by_number = target
            .parse::<usize>()
            .ok()
            .and_then(|index| snapshot.interactive.get(index.saturating_sub(1)));
        let by_name = snapshot.interactive.iter().find(|element| {
            element.name.to_lowercase() == lower
                || element.name.to_lowercase().contains(&lower)
                || element.role.to_lowercase() == lower
        });
        if let Some(element) = by_reference.or(by_number).or(by_name) {
            return Ok(ResolvedElement {
                node_id: None,
                backend_dom_node_id: element.backend_dom_node_id,
                label: format!("{} {}", element.role, element.name),
            });
        }

        let node = self.cdp.query_selector(target).await?;
        let node_id = node["nodeId"].as_i64().filter(|id| *id != 0);
        if let Some(node_id) = node_id {
            return Ok(ResolvedElement {
                node_id: Some(node_id),
                backend_dom_node_id: None,
                label: target.to_string(),
            });
        }
        Err(format!("element not found: {target}").into())
    }
}

fn interactive_elements(roots: &[AxNode]) -> Vec<InteractiveElement> {
    find_interactive_elements(roots)
        .into_iter()
        .enumerate()
        .map(|(index, node)| InteractiveElement {
            reference: format!("e{}", index + 1),
            role: node.role.clone(),
            name: node.name.clone(),
            description: node.description.clone(),
            backend_dom_node_id: node.backend_dom_node_id,
        })
        .collect()
}

fn truncate_visible_text(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let content_limit = max_bytes.saturating_sub(TEXT_TRUNCATION_MARKER.len());
    let mut end = content_limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }

    let mut truncated = text[..end].to_string();
    if max_bytes >= TEXT_TRUNCATION_MARKER.len() {
        truncated.push_str(TEXT_TRUNCATION_MARKER);
    }
    truncated
}

fn interaction_path(
    mode: InteractionMode,
    mouse: &MouseEngine,
    start: Point,
    end: Point,
) -> Vec<Point> {
    match mode {
        InteractionMode::Human => mouse.generate_path(start, end),
        InteractionMode::Fast => vec![start, end],
    }
}

fn context_event_invalidates_observation(method: &str) -> bool {
    matches!(
        method,
        "Page.frameNavigated"
            | "Page.loadEventFired"
            | "Page.frameStartedLoading"
            | "Page.frameStoppedLoading"
            | "DOM.documentUpdated"
            | "DOM.childNodeInserted"
            | "DOM.childNodeRemoved"
            | "DOM.attributeModified"
            | "DOM.attributeRemoved"
            | "DOM.characterDataModified"
            | "DOM.setChildNodes"
    )
}

struct ResolvedElement {
    node_id: Option<i64>,
    backend_dom_node_id: Option<i64>,
    label: String,
}

fn runtime_value(raw: &Value) -> BrowserResult<Value> {
    if let Some(exception) = raw.get("exceptionDetails") {
        return Err(format!("JavaScript evaluation failed: {exception}").into());
    }
    Ok(raw["result"]["value"].clone())
}

fn center_of_box_model(raw: &Value) -> BrowserResult<(f64, f64)> {
    let content = raw["model"]["content"]
        .as_array()
        .ok_or("CDP box model response contained no content points")?;
    if content.len() < 8 {
        return Err("CDP box model content did not contain four points".into());
    }
    let coordinates: Vec<f64> = content.iter().filter_map(Value::as_f64).collect();
    if coordinates.len() < 8 {
        return Err("CDP box model content contained non-numeric coordinates".into());
    }
    let xs = coordinates.iter().step_by(2);
    let ys = coordinates.iter().skip(1).step_by(2);
    let min_x = xs.clone().copied().fold(f64::INFINITY, f64::min);
    let max_x = xs.copied().fold(f64::NEG_INFINITY, f64::max);
    let min_y = ys.clone().copied().fold(f64::INFINITY, f64::min);
    let max_y = ys.copied().fold(f64::NEG_INFINITY, f64::max);
    Ok(((min_x + max_x) / 2.0, (min_y + max_y) / 2.0))
}

async fn wait_for_ws_url(port: u16) -> BrowserResult<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        match get_ws_url(port).await {
            Ok(url) => return Ok(url),
            Err(error) if tokio::time::Instant::now() < deadline => {
                tracing::debug!(%error, "waiting for Chrome page target");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("about:")
        || url.starts_with("file:")
        || url.starts_with("data:")
    {
        url.to_string()
    } else {
        format!("https://{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};

    fn test_session(cdp: CdpClient) -> BrowserSession {
        BrowserSession {
            cdp,
            chrome: None,
            profile_manager: ProfileManager::new(),
            profile: "test".to_string(),
            interaction_mode: InteractionMode::Fast,
            mouse: MouseEngine::new(),
            pointer: Mutex::new(None),
            page_revision: Arc::new(AtomicU64::new(1)),
            observation_cache: Mutex::new(None),
        }
    }

    async fn observation_server(include_dom: bool) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut saw_runtime = false;
            let mut saw_accessibility = false;
            let mut saw_deep_dom = false;

            for _ in 0..if include_dom { 3 } else { 2 } {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Runtime.evaluate") => {
                        saw_runtime = true;
                        let expression = request["params"]["expression"].as_str().unwrap();
                        assert!(expression.contains("document.body.innerText"));
                        assert!(!expression.contains(".slice(0,"));
                        let text = format!(
                            "{}😀{}",
                            "a".repeat(4_095),
                            "b".repeat(COMPACT_TEXT_MAX_BYTES)
                        );
                        let page_state = serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text,
                        })
                        .to_string();
                        serde_json::json!({
                            "result": {"value": page_state}
                        })
                    }
                    Some("Accessibility.getFullAXTree") => {
                        saw_accessibility = true;
                        serde_json::json!({"nodes": []})
                    }
                    Some("DOM.getDocument") => {
                        saw_deep_dom = true;
                        assert_eq!(request["params"], serde_json::json!({"depth": -1}));
                        serde_json::json!({
                            "root": {
                                "nodeId": 1,
                                "nodeName": "#document",
                                "nodeValue": "",
                                "children": []
                            }
                        })
                    }
                    method => panic!("unexpected compact-observation command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }

            assert!(saw_runtime);
            assert!(saw_accessibility);
            assert_eq!(saw_deep_dom, include_dom);
        });
        (format!("ws://{address}"), server)
    }

    async fn large_accessibility_server() -> (String, tokio::task::JoinHandle<()>, String) {
        let huge_text = "x".repeat(33 * 1024);
        let tree = serde_json::json!({
            "nodes": [
                {
                    "nodeId": "root",
                    "role": {"value": "RootWebArea"},
                    "name": {"value": huge_text.clone()},
                    "description": {"value": huge_text.clone()},
                    "value": {"value": huge_text.clone()},
                    "childIds": ["save"]
                },
                {
                    "nodeId": "save",
                    "parentId": "root",
                    "backendDOMNodeId": 42,
                    "role": {"value": "button"},
                    "name": {"value": "Save"},
                    "description": {"value": huge_text.clone()},
                    "value": {"value": huge_text.clone()},
                    "childIds": []
                }
            ]
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let text_for_server = huge_text.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            for _ in 0..4 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text CDP request"),
                };
                let result = match request["method"].as_str() {
                    Some("Runtime.evaluate") => serde_json::json!({
                        "result": {"value": serde_json::json!({
                            "url": "https://example.test",
                            "title": "Example",
                            "ready_state": "complete",
                            "text": text_for_server.clone(),
                        }).to_string()}
                    }),
                    Some("Accessibility.getFullAXTree") => tree.clone(),
                    method => panic!("unexpected compact-observation command: {method:?}"),
                };
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": result})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        (format!("ws://{address}"), server, huge_text)
    }

    #[test]
    fn normalizes_urls_without_touching_supported_schemes() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(normalize_url(" about:blank "), "about:blank");
        assert_eq!(
            normalize_url("file:///tmp/page.html"),
            "file:///tmp/page.html"
        );
    }

    #[test]
    fn computes_the_center_of_a_quad() {
        let raw = serde_json::json!({
            "model": {"content": [10.0, 20.0, 30.0, 20.0, 30.0, 40.0, 10.0, 40.0]}
        });
        assert_eq!(center_of_box_model(&raw).unwrap(), (20.0, 30.0));
    }

    #[test]
    fn interaction_modes_plan_smooth_or_direct_motion() {
        let mouse = MouseEngine::new();
        let start = Point { x: 10.0, y: 20.0 };
        let end = Point { x: 410.0, y: 220.0 };

        let human = interaction_path(InteractionMode::Human, &mouse, start, end);
        let fast = interaction_path(InteractionMode::Fast, &mouse, start, end);

        assert!(human.len() > 2);
        assert_eq!(human.first(), Some(&start));
        assert_eq!(human.last(), Some(&end));
        assert_eq!(fast, vec![start, end]);
    }

    #[test]
    fn invalidates_context_only_for_page_or_dom_mutations() {
        assert!(context_event_invalidates_observation(
            "DOM.childNodeInserted"
        ));
        assert!(context_event_invalidates_observation("Page.frameNavigated"));
        assert!(!context_event_invalidates_observation(
            "Network.loadingFinished"
        ));
    }

    #[test]
    fn structured_context_omits_screenshot_until_explicitly_populated() {
        let page = PageInfo {
            url: "https://example.test".to_string(),
            title: "Example".to_string(),
            ready_state: "complete".to_string(),
        };
        let mut context = PageContext {
            page: page.clone(),
            text: "Example".to_string(),
            dom: None,
            accessibility: CompactAccessibilitySnapshot {
                page,
                roots: Vec::new(),
                interactive: Vec::new(),
                truncated: false,
            },
            screenshot: None,
        };

        let structured = serde_json::to_value(&context).unwrap();
        assert!(structured.get("dom").is_none());
        assert!(structured.get("screenshot").is_none());

        context.screenshot = Some("png-data".to_string());
        let visual = serde_json::to_value(&context).unwrap();
        assert_eq!(visual["screenshot"], "png-data");
    }

    #[test]
    fn compact_text_cap_is_utf8_safe_and_marks_truncation() {
        let text = "🙂".repeat(COMPACT_TEXT_MAX_BYTES);
        let compact = truncate_visible_text(&text, COMPACT_TEXT_MAX_BYTES);

        assert!(compact.len() <= COMPACT_TEXT_MAX_BYTES);
        assert!(compact.ends_with(TEXT_TRUNCATION_MARKER));
        assert!(compact.is_char_boundary(compact.len()));
    }

    #[tokio::test]
    async fn default_observation_is_compact_and_never_requests_deep_dom() {
        let (url, server) = observation_server(false).await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        assert!(context.dom.is_none());
        assert!(context.screenshot.is_none());
        assert!(context.text.contains('😀'));
        assert!(context.text.ends_with(TEXT_TRUNCATION_MARKER));
        assert!(context.text.len() <= COMPACT_TEXT_MAX_BYTES);
        assert!(std::str::from_utf8(context.text.as_bytes()).is_ok());
        let serialized = serde_json::to_value(&context).unwrap();
        assert!(serialized.get("dom").is_none());
        assert!(serialized.get("screenshot").is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn deep_dom_observation_is_explicit_and_not_cached() {
        let (url, server) = observation_server(true).await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let deep = session.observe_with_dom().await.unwrap();
        assert_eq!(deep.dom.as_ref().unwrap().node_name, "#document");
        assert!(serde_json::to_value(&deep).unwrap().get("dom").is_some());

        let compact = session.observe().await.unwrap();
        assert!(compact.dom.is_none());

        session.close().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn compact_observation_bounds_accessibility_while_snapshot_remains_full() {
        let (url, server, huge_text) = large_accessibility_server().await;
        let session = test_session(CdpClient::connect(&url).await.unwrap());

        let context = session.observe().await.unwrap();
        let serialized = serde_json::to_string(&context).unwrap();
        assert!(context.accessibility.truncated);
        assert_eq!(context.accessibility.roots[0].role, "RootWebArea");
        assert_eq!(context.accessibility.interactive[0].reference, "e1");
        assert_eq!(context.accessibility.interactive[0].role, "button");
        assert_eq!(context.accessibility.interactive[0].name, "Save");
        assert!(!serialized.contains(&huge_text));
        assert!(
            serialized.len()
                <= COMPACT_TEXT_MAX_BYTES + crate::browser::dom::COMPACT_AX_TEXT_MAX_BYTES + 2_048
        );

        let cached = {
            let cache = session.observation_cache.lock().await;
            cache.as_ref().unwrap().context.clone()
        };
        let cached_json = serde_json::to_string(&cached.into_page_context()).unwrap();
        assert!(!cached_json.contains(&huge_text));

        let snapshot = session.snapshot().await.unwrap();
        assert_eq!(snapshot.roots[0].name, huge_text);
        assert_eq!(snapshot.interactive[0].reference, "e1");
        assert_eq!(snapshot.interactive[0].description.len(), 33 * 1024);

        session.close().await.unwrap();
        server.await.unwrap();
    }
}
