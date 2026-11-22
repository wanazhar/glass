//! Typed `browser_*` tools for standalone Core.
//!
//! When `glass-browser` is installed and supports the agent tool profile,
//! its curated catalog is offered to the model as first-class tools next to
//! the `desktop_*` family, instead of leaving the model to discover the CLI
//! through `bash`. One `glass-browser --mcp --tool-profile agent` process per
//! workspace keeps the page, tabs and refs alive across calls.

use base64::Engine;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_BODY_BYTES: usize = 60_000;

type Schema = (String, String, String);

static CATALOG: OnceLock<Vec<Schema>> = OnceLock::new();
static CLIENT: Mutex<Option<Client>> = Mutex::new(None);

pub fn is_tool(name: &str) -> bool {
    name.len() > "browser_".len()
        && name.starts_with("browser_")
        && schemas().iter().any(|(tool, _, _)| tool == name)
}

/// The agent catalog advertised by the installed `glass-browser`, or nothing
/// when it is missing, disabled with `GC_BROWSER_TOOLS=0`, or too old.
pub fn schemas() -> Vec<Schema> {
    CATALOG
        .get_or_init(|| {
            if std::env::var("GC_BROWSER_TOOLS").is_ok_and(|value| value == "0") {
                return Vec::new();
            }
            probe().unwrap_or_default()
        })
        .clone()
}

fn binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("GC_BROWSER_BIN") {
        return Some(PathBuf::from(path));
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join("glass-browser"))
        .find(|path| path.is_file())
}

fn command(workspace: Option<&Path>) -> Option<Command> {
    let mut command = Command::new(binary()?);
    if let Ok(runtime) = std::env::var("GC_BROWSER_RUNTIME")
        && !runtime.is_empty()
    {
        command.args(["--browser-runtime", &runtime]);
    }
    if let Some(port) = free_port() {
        command.args(["--port", &port.to_string()]);
    }
    command
        .args(["--mcp", "--tool-profile", "agent"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(workspace) = workspace {
        command.current_dir(workspace);
    }
    Some(command)
}

/// A per-process CDP port, so a Chrome the user already runs on 9222 never
/// blocks the agent's browser.
fn free_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    Some(listener.local_addr().ok()?.port())
}

fn probe() -> Option<Vec<Schema>> {
    let mut client = Client::spawn(None, PROBE_TIMEOUT)?;
    let tools = client
        .request("tools/list", json!({}), PROBE_TIMEOUT)
        .ok()?;
    let schemas = tools["tools"]
        .as_array()?
        .iter()
        .filter_map(|tool| {
            let name = tool["name"].as_str()?;
            name.starts_with("browser_").then(|| {
                (
                    name.to_owned(),
                    tool["description"].as_str().unwrap_or_default().to_owned(),
                    tool["inputSchema"].to_string(),
                )
            })
        })
        .collect::<Vec<_>>();
    (!schemas.is_empty()).then_some(schemas)
}

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
    workspace: Option<PathBuf>,
}

impl Client {
    fn spawn(workspace: Option<&Path>, timeout: Duration) -> Option<Self> {
        let mut child = command(workspace)?.spawn().ok()?;
        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin: Some(stdin),
            lines,
            next_id: 0,
            workspace: workspace.map(Path::to_path_buf),
        };
        client
            .request(
                "initialize",
                json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"glass-core","version":env!("CARGO_PKG_VERSION")}}),
                timeout,
            )
            .ok()?;
        client
            .send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .ok()?;
        Some(client)
    }

    fn send(&mut self, message: &Value) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        writeln!(stdin, "{message}")?;
        stdin.flush()
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .map_err(|error| format!("glass-browser is not accepting requests: {error}"))?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            let line = match self.lines.recv_timeout(remaining) {
                Ok(line) => line,
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "glass-browser did not answer within {} s",
                        timeout.as_secs()
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("glass-browser exited".into());
                }
            };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message["id"] != id {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(error["message"]
                    .as_str()
                    .unwrap_or("glass-browser request failed")
                    .to_owned());
            }
            return Ok(message["result"].clone());
        }
    }
}

impl Drop for Client {
    /// Closing stdin lets glass-browser shut its Chrome down; killing it
    /// outright would orphan the browser process.
    fn drop(&mut self) {
        drop(self.stdin.take());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn execute(call: &glass_mod_api::ToolCall, workspace: &Path) -> crate::tools::Outcome {
    let arguments = match serde_json::from_str::<Value>(&call.arguments) {
        Ok(value) if value.is_object() => value,
        Ok(Value::Null) => json!({}),
        Ok(_) => return crate::tools::Outcome::err("arguments must be a JSON object"),
        Err(error) => {
            return crate::tools::Outcome::err(format!("invalid JSON arguments: {error}"));
        }
    };
    let mut guard = CLIENT.lock().unwrap_or_else(|poison| poison.into_inner());
    let reusable = guard.as_mut().is_some_and(|client| {
        client.workspace.as_deref() == Some(workspace)
            && matches!(client.child.try_wait(), Ok(None))
    });
    if !reusable {
        *guard = None;
        match Client::spawn(Some(workspace), PROBE_TIMEOUT) {
            Some(client) => *guard = Some(client),
            None => {
                return crate::tools::Outcome::err(
                    "glass-browser could not start. Check `glass-browser --help` with bash; on Linux hosts without a working native sandbox set GC_BROWSER_RUNTIME=chromium.",
                );
            }
        }
    }
    let client = guard.as_mut().expect("client was just ensured");
    let result = client.request(
        "tools/call",
        json!({"name": call.name, "arguments": arguments}),
        CALL_TIMEOUT,
    );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            *guard = None;
            return crate::tools::Outcome::err(format!(
                "{error}. The browser session was reset; call browser_navigate again."
            ));
        }
    };
    drop(guard);
    let body = render(&result, workspace);
    if result["isError"] == true {
        crate::tools::Outcome::err(body)
    } else {
        crate::tools::Outcome::ok(body)
    }
}

fn render(result: &Value, workspace: &Path) -> String {
    let mut parts = Vec::new();
    for item in result["content"].as_array().into_iter().flatten() {
        match item["type"].as_str() {
            Some("text") => parts.push(item["text"].as_str().unwrap_or_default().to_owned()),
            Some("image") => parts.push(
                save_image(item, workspace)
                    .unwrap_or_else(|error| format!("Screenshot could not be saved: {error}")),
            ),
            _ => {}
        }
    }
    let mut body = parts.join("\n");
    if body.len() > MAX_BODY_BYTES {
        let mut end = MAX_BODY_BYTES;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n[truncated; use browser_snapshot with a region, or browser_get_text]");
    }
    body
}

fn save_image(item: &Value, workspace: &Path) -> Result<String, String> {
    let data = item["data"].as_str().ok_or("no image data")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())?;
    let extension = match item["mimeType"].as_str() {
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        _ => "png",
    };
    let directory = workspace.join(".gc-artifacts").join("browser");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let filename = format!("browser-{stamp}.{extension}");
    std::fs::write(directory.join(&filename), bytes).map_err(|error| error.to_string())?;
    Ok(format!(
        "Screenshot saved to `.gc-artifacts/browser/{filename}`. Use `view_image` with that path to see it; pixel positions are CSS pixels for browser_click_at."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_saves_images_and_keeps_text_order() {
        let workspace =
            std::env::temp_dir().join(format!("gc-browser-tools-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        let png = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\n");
        let body = render(
            &json!({"content":[{"type":"text","text":"{\"width\":800}"},{"type":"image","data":png,"mimeType":"image/png"}]}),
            &workspace,
        );
        let mut lines = body.lines();
        assert_eq!(lines.next(), Some("{\"width\":800}"));
        let saved = lines.next().unwrap();
        assert!(saved.contains(".gc-artifacts/browser/browser-"), "{saved}");
        assert!(saved.contains("view_image"), "{saved}");
        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn render_bounds_large_text() {
        let body = render(
            &json!({"content":[{"type":"text","text":"é".repeat(MAX_BODY_BYTES)}]}),
            Path::new("."),
        );
        assert!(body.len() < MAX_BODY_BYTES + 100);
        assert!(body.ends_with("browser_get_text]"));
    }

    /// Opt-in: `GLASS_E2E=1 GC_BROWSER_BIN=target/debug/glass-browser
    /// GC_BROWSER_RUNTIME=chromium cargo test -p glass-core browser_tools`.
    #[test]
    fn typed_browser_tools_drive_a_real_page() {
        if std::env::var("GLASS_E2E").as_deref() != Ok("1")
            || std::env::var_os("GC_BROWSER_BIN").is_none()
        {
            return;
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            const PAGE: &str = "<!doctype html><title>Core bridge</title><main><h1>Orders</h1><label>Email <input id=email></label><button onclick=\"document.querySelector('h1').textContent='Saved '+document.querySelector('#email').value\">Save</button></main>";
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buffer = [0u8; 2048];
                let _ = std::io::Read::read(&mut stream, &mut buffer);
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
                    PAGE.len()
                );
            }
        });
        let names = schemas()
            .into_iter()
            .map(|(name, _, _)| name)
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 19, "{names:?}");
        assert!(is_tool("browser_navigate") && !is_tool("browser_unknown"));
        let workspace = std::env::temp_dir().join(format!("gc-browser-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        let call = |name: &str, arguments: Value| {
            execute(
                &glass_mod_api::ToolCall {
                    id: name.into(),
                    name: name.into(),
                    arguments: arguments.to_string(),
                },
                &workspace,
            )
        };
        let outcome = call(
            "browser_navigate",
            json!({"url": format!("http://{address}/")}),
        );
        assert!(outcome.exit_code == Some(0), "{}", outcome.body);
        let snapshot = call("browser_snapshot", json!({}));
        assert!(snapshot.body.contains("Orders"), "{}", snapshot.body);
        let typed = call(
            "browser_type",
            json!({"target":"name=Email","text":"ada@example.com"}),
        );
        assert!(typed.exit_code == Some(0), "{}", typed.body);
        let clicked = call("browser_click", json!({"target":"role=button;name=Save"}));
        assert!(clicked.exit_code == Some(0), "{}", clicked.body);
        let waited = call("browser_wait_for", json!({"text":"Saved ada@example.com"}));
        assert!(waited.exit_code == Some(0), "{}", waited.body);
        let shot = call("browser_screenshot", json!({}));
        assert!(
            shot.body.contains(".gc-artifacts/browser/"),
            "{}",
            shot.body
        );
        assert!(
            std::fs::read_dir(workspace.join(".gc-artifacts/browser"))
                .unwrap()
                .count()
                >= 1
        );
        let missing = call(
            "browser_click",
            json!({"target":"role=button;name=Delete everything"}),
        );
        assert!(missing.exit_code != Some(0));
        assert!(missing.body.contains("Next:"), "{}", missing.body);
        let _ = std::fs::remove_dir_all(&workspace);
    }
}
