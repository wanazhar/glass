use super::{DevelopmentError, DevelopmentResult, read_bounded_utf8};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const MAX_LSP_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageDiagnostic {
    pub path: String,
    pub start: DiagnosticPosition,
    pub end: DiagnosticPosition,
    pub severity: Option<u8>,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

/// Minimal standards-compliant LSP client used by the Development Runtime.
/// Language intelligence stays in external language servers; Glass owns only
/// lifecycle, bounded framing, normalized diagnostics, and timeline routing.
pub struct LspClient {
    root: PathBuf,
    child: Child,
    input: ChildStdin,
    output: Receiver<Value>,
    next_id: u64,
}

impl std::fmt::Debug for LspClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LspClient")
            .field("root", &self.root)
            .field("pid", &self.child.id())
            .finish()
    }
}

impl LspClient {
    pub fn spawn(root: &Path, server: &str, arguments: &[String]) -> DevelopmentResult<Self> {
        let root = fs::canonicalize(root)?;
        let mut child = Command::new(server)
            .args(arguments)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                DevelopmentError::Process(format!(
                    "failed to start language server {server}: {error}"
                ))
            })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| DevelopmentError::Process("language server stdin unavailable".into()))?;
        let output = child.stdout.take().ok_or_else(|| {
            DevelopmentError::Process("language server stdout unavailable".into())
        })?;
        let (sender, receiver) = mpsc::sync_channel(256);
        thread::Builder::new()
            .name("glass-lsp-reader".into())
            .spawn(move || read_lsp_messages(output, sender))
            .map_err(DevelopmentError::Io)?;
        let mut client = Self {
            root,
            child,
            input,
            output: receiver,
            next_id: 1,
        };
        client.initialize()?;
        Ok(client)
    }

    pub fn rust_analyzer(root: &Path) -> DevelopmentResult<Self> {
        Self::spawn(root, "rust-analyzer", &[])
    }

    pub fn diagnostics(&mut self, path: &str) -> DevelopmentResult<Vec<LanguageDiagnostic>> {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(DevelopmentError::PathOutsideWorkspace(relative.into()));
        }
        let absolute = fs::canonicalize(self.root.join(relative))?;
        if !absolute.starts_with(&self.root) {
            return Err(DevelopmentError::PathOutsideWorkspace(absolute));
        }
        let text = read_bounded_utf8(&absolute, super::MAX_BUFFER_BYTES, "language document")?;
        let uri = file_uri(&absolute)?;
        self.notify(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id(&absolute),
                    "version": 1,
                    "text": text
                }
            }
        }))?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut empty_published_at = None;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait = empty_published_at
                .map(|published: Instant| {
                    remaining.min(
                        (published + Duration::from_secs(5))
                            .saturating_duration_since(Instant::now()),
                    )
                })
                .unwrap_or(remaining);
            let value = match self.output.recv_timeout(wait) {
                Ok(value) => value,
                Err(mpsc::RecvTimeoutError::Timeout) if empty_published_at.is_some() => {
                    return Ok(Vec::new());
                }
                Err(error) => {
                    return Err(DevelopmentError::Process(format!(
                        "language diagnostics timed out: {error}"
                    )));
                }
            };
            if value.get("method").and_then(Value::as_str)
                != Some("textDocument/publishDiagnostics")
                || value.pointer("/params/uri").and_then(Value::as_str) != Some(uri.as_str())
            {
                continue;
            }
            let diagnostics = value
                .pointer("/params/diagnostics")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DevelopmentError::Serialization(
                        "language server diagnostics must be an array".into(),
                    )
                })?;
            if diagnostics.is_empty() {
                empty_published_at.get_or_insert_with(Instant::now);
                continue;
            }
            return diagnostics
                .iter()
                .take(512)
                .map(|diagnostic| parse_diagnostic(path, diagnostic))
                .collect();
        }
        Err(DevelopmentError::Process(
            "language diagnostics timed out".into(),
        ))
    }

    fn initialize(&mut self) -> DevelopmentResult<()> {
        let root_uri = file_uri(&self.root)?;
        let response = self.call(
            "initialize",
            serde_json::json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {"textDocument": {"publishDiagnostics": {"relatedInformation": true}}},
                "workspaceFolders": [{"uri": root_uri, "name": self.root.file_name().and_then(|name| name.to_str()).unwrap_or("project")}]
            }),
            Duration::from_secs(30),
        )?;
        if response.get("error").is_some() {
            return Err(DevelopmentError::Process(format!(
                "language server initialization failed: {}",
                response["error"]
            )));
        }
        self.notify(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }))
    }

    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> DevelopmentResult<Value> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.notify(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;
        let deadline = Instant::now() + timeout;
        loop {
            let value = self
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|error| {
                    DevelopmentError::Process(format!(
                        "language server response timed out: {error}"
                    ))
                })?;
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(value);
            }
        }
    }

    fn notify(&mut self, value: Value) -> DevelopmentResult<()> {
        let body = serde_json::to_vec(&value)?;
        if body.len() > MAX_LSP_MESSAGE_BYTES {
            return Err(DevelopmentError::InvalidInput(
                "language protocol message exceeds the size limit".into(),
            ));
        }
        write!(self.input, "Content-Length: {}\r\n\r\n", body.len())?;
        self.input.write_all(&body)?;
        self.input.flush()?;
        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.notify(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_lsp_messages(output: impl Read, sender: mpsc::SyncSender<Value>) {
    let mut reader = BufReader::new(output);
    loop {
        let mut content_length = None;
        loop {
            let mut header = String::new();
            match reader.read_line(&mut header) {
                Ok(0) | Err(_) => return,
                Ok(_) if header == "\r\n" || header == "\n" => break,
                Ok(_) => {
                    if let Some(value) = header
                        .strip_prefix("Content-Length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                    {
                        content_length = Some(value);
                    }
                }
            }
        }
        let Some(length) = content_length.filter(|length| *length <= MAX_LSP_MESSAGE_BYTES) else {
            return;
        };
        let mut body = vec![0; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        if let Ok(value) = serde_json::from_slice(&body)
            && sender.send(value).is_err()
        {
            return;
        }
    }
}

fn parse_diagnostic(path: &str, value: &Value) -> DevelopmentResult<LanguageDiagnostic> {
    let position = |pointer: &str| -> DevelopmentResult<DiagnosticPosition> {
        let value = value.pointer(pointer).ok_or_else(|| {
            DevelopmentError::Serialization(format!("diagnostic is missing {pointer}"))
        })?;
        Ok(DiagnosticPosition {
            line: value.get("line").and_then(Value::as_u64).unwrap_or(0) as u32 + 1,
            character: value.get("character").and_then(Value::as_u64).unwrap_or(0) as u32 + 1,
        })
    };
    let code = value.get("code").and_then(|code| {
        code.as_str()
            .map(str::to_string)
            .or_else(|| code.as_i64().map(|number| number.to_string()))
    });
    Ok(LanguageDiagnostic {
        path: path.into(),
        start: position("/range/start")?,
        end: position("/range/end")?,
        severity: value
            .get("severity")
            .and_then(Value::as_u64)
            .map(|value| value as u8),
        code,
        source: value
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_string),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("language diagnostic")
            .chars()
            .take(4096)
            .collect(),
    })
}

fn file_uri(path: &Path) -> DevelopmentResult<String> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| {
            DevelopmentError::InvalidInput(format!(
                "cannot represent path as URI: {}",
                path.display()
            ))
        })
}

fn language_id(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => "rust",
        Some("ts" | "tsx") => "typescript",
        Some("js" | "jsx") => "javascript",
        Some("py") => "python",
        Some("go") => "go",
        _ => "plaintext",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_analyzer_publishes_real_diagnostics_when_available() {
        if !Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return;
        }
        let root = std::env::temp_dir().join(format!("glass-lsp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='glass_lsp_fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn broken( {\n").unwrap();
        let mut client = LspClient::rust_analyzer(&root).unwrap();
        let diagnostics = client.diagnostics("src/lib.rs").unwrap();
        assert!(
            !diagnostics.is_empty(),
            "rust-analyzer returned no diagnostics"
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.path == "src/lib.rs")
        );
        let _ = fs::remove_dir_all(root);
    }
}
