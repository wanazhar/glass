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

use super::cdp::{CdpClient, CdpError};
use super::chrome::{
    ChromeProcess, check_chrome_health, detect_chrome, get_ws_url, launch_chrome_with_options,
};
use super::dom::{
    AxNode, DomNode, find_interactive_elements, format_tree, parse_accessibility_tree,
    parse_dom_tree,
};
use super::mouse::{MouseEngine, Point};
use super::profile::ProfileManager;

pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone)]
pub struct SessionOptions {
    pub port: u16,
    pub chrome_path: Option<PathBuf>,
    pub profile: String,
    pub incognito: bool,
    pub headed: bool,
    pub interaction_mode: InteractionMode,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
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

/// The default agent observation: structured page state without a screenshot.
#[derive(Debug, Clone, Serialize)]
pub struct PageContext {
    pub page: PageInfo,
    pub text: String,
    pub dom: Option<DomNode>,
    pub accessibility: AccessibilitySnapshot,
    /// Base64 PNG data is populated only when visual context is explicitly requested.
    pub screenshot: Option<String>,
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
    context: PageContext,
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

        let setup = async {
            cdp.enable_page().await?;
            cdp.enable_runtime().await?;
            cdp.enable_network().await?;
            cdp.enable_dom().await?;
            cdp.enable_accessibility().await?;
            Ok::<(), CdpError>(())
        }
        .await;
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
        self.page_info().await
    }

    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        let raw = self.cdp.evaluate(expression).await?;
        runtime_value(&raw)
    }

    pub async fn text(&self) -> BrowserResult<String> {
        let value = self
            .evaluate("document.body ? document.body.innerText : ''")
            .await?;
        Ok(value.as_str().unwrap_or_default().to_string())
    }

    /// Collect DOM, accessibility, and visible text context. Screenshots are opt-in.
    pub async fn observe(&self, include_screenshot: bool) -> BrowserResult<PageContext> {
        self.observe_internal(include_screenshot, true).await
    }

    /// Collect a fresh context, bypassing the event-driven cache.
    pub async fn observe_fresh(&self, include_screenshot: bool) -> BrowserResult<PageContext> {
        self.observe_internal(include_screenshot, false).await
    }

    async fn observe_internal(
        &self,
        include_screenshot: bool,
        use_cache: bool,
    ) -> BrowserResult<PageContext> {
        let revision = self.page_revision.load(Ordering::Relaxed);
        if use_cache {
            let cached_context = {
                let cache = self.observation_cache.lock().await;
                cache
                    .as_ref()
                    .filter(|cached| cached.revision == revision)
                    .map(|cached| cached.context.clone())
            };
            if let Some(mut context) = cached_context {
                if include_screenshot {
                    context.screenshot = Some(STANDARD.encode(self.screenshot_png().await?));
                }
                return Ok(context);
            }
        }

        let (page, text, accessibility, dom) = tokio::join!(
            self.page_info(),
            self.text(),
            self.cdp.get_accessibility_tree(),
            self.cdp.get_document(),
        );
        let page = page?;
        let text = text?;
        let accessibility_raw = accessibility?;
        let dom_raw = dom?;
        let roots = parse_accessibility_tree(&accessibility_raw);
        let accessibility = AccessibilitySnapshot {
            page: page.clone(),
            interactive: interactive_elements(&roots),
            roots,
        };
        let context = PageContext {
            page,
            text,
            dom: parse_dom_tree(&dom_raw),
            accessibility,
            screenshot: None,
        };
        *self.observation_cache.lock().await = Some(CachedObservation {
            revision,
            context: context.clone(),
        });
        if include_screenshot {
            let mut visual_context = context;
            visual_context.screenshot = Some(STANDARD.encode(self.screenshot_png().await?));
            Ok(visual_context)
        } else {
            Ok(context)
        }
    }

    pub async fn screenshot_png(&self) -> BrowserResult<Vec<u8>> {
        let data = self.cdp.screenshot("png").await?;
        Ok(STANDARD.decode(data.as_bytes())?)
    }

    pub async fn scroll(&self, dx: f64, dy: f64) -> BrowserResult<()> {
        self.cdp.scroll_by(dx, dy).await?;
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
        match self.interaction_mode {
            InteractionMode::Human => {
                let start = pointer.unwrap_or(Point { x: 640.0, y: 360.0 });
                let path = self.mouse.generate_path(start, point);
                for window in path.windows(2) {
                    let next = window[1];
                    self.cdp
                        .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                        .await?;
                    tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
                }
            }
            InteractionMode::Fast => {
                self.cdp
                    .dispatch_mouse_event("mouseMoved", point.x, point.y, None, None)
                    .await?;
            }
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
        }
        *pointer = Some(point);
        Ok(element.label)
    }

    pub async fn type_text(&self, text: &str, target: Option<&str>) -> BrowserResult<()> {
        if let Some(target) = target {
            self.click(target).await?;
        }
        self.cdp.insert_text(text).await?;
        Ok(())
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
    fn invalidates_context_only_for_page_or_dom_mutations() {
        assert!(context_event_invalidates_observation(
            "DOM.childNodeInserted"
        ));
        assert!(context_event_invalidates_observation("Page.frameNavigated"));
        assert!(!context_event_invalidates_observation(
            "Network.loadingFinished"
        ));
    }
}
