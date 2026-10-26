use super::{DevelopmentError, DevelopmentResult, read_bounded_utf8};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

const MAX_LSP_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const RENAME_INDEX_WAIT: Duration = Duration::from_secs(8);
const RENAME_CONTENT_MODIFIED_RETRIES: usize = 3;
// Some rust-analyzer versions publish an empty snapshot before their first
// real check completes. Keep waiting within the existing bounded request
// deadline so that the empty snapshot cannot mask a later diagnostic.
const DIAGNOSTICS_EMPTY_GRACE: Duration = Duration::from_secs(30);

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
pub(crate) struct LspClient {
    root: PathBuf,
    server_settings: Value,
    child: Child,
    input: ChildStdin,
    output: Receiver<Value>,
    next_id: u64,
    documents: BTreeMap<String, OpenDocument>,
    shutdown: bool,
}

#[derive(Debug, Clone)]
struct OpenDocument {
    uri: String,
    version: i64,
    text: String,
}

/// Bounded, method-labelled response for interactive language operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageResponse {
    pub method: String,
    pub result: Value,
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
    pub(crate) fn spawn(
        root: &Path,
        server: &str,
        arguments: &[String],
    ) -> DevelopmentResult<Self> {
        Self::spawn_with_initialization_options(root, server, arguments, None)
    }

    fn spawn_with_initialization_options(
        root: &Path,
        server: &str,
        arguments: &[String],
        initialization_options: Option<Value>,
    ) -> DevelopmentResult<Self> {
        let root = fs::canonicalize(root)?;
        let server_settings = initialization_options
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
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
            server_settings,
            child,
            input,
            output: receiver,
            next_id: 1,
            documents: BTreeMap::new(),
            shutdown: false,
        };
        client.initialize(initialization_options)?;
        Ok(client)
    }

    pub(crate) fn rust_analyzer(root: &Path) -> DevelopmentResult<Self> {
        Self::spawn_with_initialization_options(
            root,
            "rust-analyzer",
            &[],
            Some(rust_analyzer_initialization_options()),
        )
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
        let uri = self.sync_document(path)?.uri.clone();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut empty_published_at = None;
        while Instant::now() < deadline {
            if empty_published_at
                .is_some_and(|published: Instant| published.elapsed() >= DIAGNOSTICS_EMPTY_GRACE)
            {
                return Ok(Vec::new());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let wait = empty_published_at
                .map(|published: Instant| {
                    remaining.min(
                        (published + DIAGNOSTICS_EMPTY_GRACE)
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
                note_empty_diagnostics(&mut empty_published_at, Instant::now());
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

    /// Open a document once, then publish monotonic full-text changes.
    pub fn sync_document(&mut self, path: &str) -> DevelopmentResult<LanguageDocument> {
        let (absolute, _) = self.resolve_document(path)?;
        let text = read_bounded_utf8(&absolute, super::MAX_BUFFER_BYTES, "language document")?;
        self.sync_document_with_content(path, &text)
    }

    /// Sync the exact in-memory editor contents instead of rereading the file.
    pub fn sync_document_with_content(
        &mut self,
        path: &str,
        text: &str,
    ) -> DevelopmentResult<LanguageDocument> {
        if text.len() > super::MAX_BUFFER_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "language document exceeds the {} byte limit",
                super::MAX_BUFFER_BYTES
            )));
        }
        let (absolute, normalized) = self.resolve_document(path)?;
        let text = text.to_string();
        if let Some(document) = self.documents.get_mut(&normalized) {
            if document.text != text {
                document.version = document.version.saturating_add(1);
                document.text.clone_from(&text);
                let notification = serde_json::json!({
                    "jsonrpc": "2.0", "method": "textDocument/didChange",
                    "params": {"textDocument": {"uri": document.uri, "version": document.version},
                    "contentChanges": [{"text": document.text}]}
                });
                self.notify(notification)?;
            }
        } else {
            let uri = file_uri(&absolute)?;
            self.notify(serde_json::json!({
                "jsonrpc": "2.0", "method": "textDocument/didOpen",
                "params": {"textDocument": {"uri": uri, "languageId": language_id(&absolute),
                    "version": 1, "text": text}}
            }))?;
            self.documents.insert(
                normalized.clone(),
                OpenDocument {
                    uri,
                    version: 1,
                    text,
                },
            );
        }
        let document = self.documents.get(&normalized).expect("inserted above");
        Ok(LanguageDocument {
            uri: document.uri.clone(),
            version: document.version,
        })
    }

    #[cfg(test)]
    fn save_document(&mut self, path: &str) -> DevelopmentResult<()> {
        let document = self.sync_document(path)?;
        let uri = document.uri.clone();
        self.notify(serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{"textDocument":{"uri":uri}}}))
    }

    #[cfg(test)]
    fn close_document(&mut self, path: &str) -> DevelopmentResult<()> {
        let (_, normalized) = self.resolve_document(path)?;
        let document = self
            .documents
            .remove(&normalized)
            .ok_or_else(|| DevelopmentError::NotFound(format!("open language document {path}")))?;
        self.notify(serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":document.uri}}}))
    }

    pub fn hover(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request("textDocument/hover", path, line, character, Value::Null)
    }

    pub fn definition(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(
            "textDocument/definition",
            path,
            line,
            character,
            Value::Null,
        )
    }

    pub fn declaration(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(
            "textDocument/declaration",
            path,
            line,
            character,
            Value::Null,
        )
    }

    pub fn implementation(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(
            "textDocument/implementation",
            path,
            line,
            character,
            Value::Null,
        )
    }

    pub fn references(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(
            "textDocument/references",
            path,
            line,
            character,
            serde_json::json!({"includeDeclaration": true}),
        )
    }

    pub fn document_symbols(&mut self, path: &str) -> DevelopmentResult<LanguageResponse> {
        let uri = self.sync_document(path)?.uri.clone();
        self.request_result(
            "textDocument/documentSymbol",
            serde_json::json!({"textDocument":{"uri":uri}}),
        )
    }

    pub fn workspace_symbols(&mut self, query: &str) -> DevelopmentResult<LanguageResponse> {
        validate_lsp_text("workspace symbol query", query, 1024, true)?;
        self.request_result("workspace/symbol", serde_json::json!({"query":query}))
    }

    pub fn completion(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(
            "textDocument/completion",
            path,
            line,
            character,
            serde_json::json!({"context":{"triggerKind":1}}),
        )
    }

    pub fn inlay_hints(&mut self, path: &str) -> DevelopmentResult<LanguageResponse> {
        let document = self.sync_document(path)?;
        let uri = document.uri.clone();
        let last_line = self
            .documents
            .values()
            .find(|item| item.uri == uri)
            .map(|item| item.text.split('\n').count().saturating_sub(1))
            .unwrap_or(0);
        self.request_result(
            "textDocument/inlayHint",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "range":{
                    "start":{"line":0,"character":0},
                    "end":{"line":last_line,"character":0}
                }
            }),
        )
    }

    pub fn signature_help(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(
            "textDocument/signatureHelp",
            path,
            line,
            character,
            serde_json::json!({"context":{"triggerKind":1,"isRetrigger":false}}),
        )
    }

    pub fn code_actions(
        &mut self,
        path: &str,
        start: DiagnosticPosition,
        end: DiagnosticPosition,
        diagnostics: &[Value],
    ) -> DevelopmentResult<LanguageResponse> {
        if diagnostics.len() > 512 {
            return Err(DevelopmentError::InvalidInput(
                "code action diagnostics exceed the 512 item limit".into(),
            ));
        }
        let uri = self.sync_document(path)?.uri.clone();
        self.request_result(
            "textDocument/codeAction",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "range":{"start":protocol_position(&start),"end":protocol_position(&end)},
                "context":{"diagnostics":diagnostics}
            }),
        )
    }

    pub fn formatting_with_content(
        &mut self,
        path: &str,
        content: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        let uri = self.sync_document_with_content(path, content)?.uri;
        self.request_result("textDocument/formatting", serde_json::json!({"textDocument":{"uri":uri},"options":{"tabSize":4,"insertSpaces":true}}))
    }

    pub fn range_formatting_with_content(
        &mut self,
        path: &str,
        start: DiagnosticPosition,
        end: DiagnosticPosition,
        content: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        let uri = self.sync_document_with_content(path, content)?.uri;
        self.request_result(
            "textDocument/rangeFormatting",
            serde_json::json!({
                "textDocument":{"uri":uri},
                "range":{"start":protocol_position(&start),"end":protocol_position(&end)},
                "options":{"tabSize":4,"insertSpaces":true}
            }),
        )
    }

    pub fn semantic_tokens(&mut self, path: &str) -> DevelopmentResult<LanguageResponse> {
        let uri = self.sync_document(path)?.uri.clone();
        self.request_result(
            "textDocument/semanticTokens/full",
            serde_json::json!({"textDocument":{"uri":uri}}),
        )
    }

    /// Bounded advanced request escape hatch. Lifecycle and protocol methods
    /// stay owned by Glass and cannot be overridden by callers.
    pub fn raw_request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> DevelopmentResult<LanguageResponse> {
        validate_lsp_text("LSP method", method, 256, false)?;
        if !method.contains('/')
            || matches!(
                method,
                "initialize" | "initialized" | "shutdown" | "exit" | "$/cancelRequest"
            )
            || method.starts_with("window/")
        {
            return Err(DevelopmentError::InvalidInput(
                "raw LSP requests cannot control lifecycle or server UI".into(),
            ));
        }
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(DevelopmentError::InvalidInput(
                "raw LSP timeout must be between 1 ms and 60 seconds".into(),
            ));
        }
        if serde_json::to_vec(&params)?.len() > MAX_LSP_MESSAGE_BYTES {
            return Err(DevelopmentError::InvalidInput(
                "raw LSP parameters exceed the size limit".into(),
            ));
        }
        let response = self.call(method, params, timeout)?;
        if let Some(error) = response.get("error") {
            return Err(DevelopmentError::Process(format!(
                "{method} failed: {error}"
            )));
        }
        Ok(LanguageResponse {
            method: method.into(),
            result: response.get("result").cloned().unwrap_or(Value::Null),
        })
    }

    pub fn rename_with_content(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
        new_name: &str,
        content: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        if new_name.is_empty() || new_name.len() > 256 {
            return Err(DevelopmentError::InvalidInput(
                "rename target must be 1 to 256 bytes".into(),
            ));
        }
        let uri = self.sync_document_with_content(path, content)?.uri;
        let params = serde_json::json!({
            "textDocument":{"uri":uri},
            "position":{"line":line.saturating_sub(1),"character":character.saturating_sub(1)},
            "newName":new_name
        });
        let initial = self.rename_request(params.clone());
        let needs_index_check = match &initial {
            Err(error) => error
                .to_string()
                .contains("No references found at position"),
            Ok(response) => response.result.is_null() || workspace_edit_is_empty(&response.result),
        };
        if !needs_index_check {
            return initial;
        }
        let position = DiagnosticPosition { line, character };
        if !self.wait_for_rename_references(&uri, &position) {
            return initial;
        }
        let response = self.rename_request(params)?;
        if response.result.is_null() {
            return Err(DevelopmentError::Process(
                "textDocument/rename returned no WorkspaceEdit after the selected symbol was indexed".into(),
            ));
        }
        Ok(response)
    }

    /// LSP error -32801 means the server invalidated a request because its
    /// document view changed while the request was running. Retrying this
    /// exact read-only rename request is safe, but keep the retry count and
    /// delays bounded so other errors remain visible to the caller.
    fn rename_request(&mut self, params: Value) -> DevelopmentResult<LanguageResponse> {
        for attempt in 0..=RENAME_CONTENT_MODIFIED_RETRIES {
            let response = self.call(
                "textDocument/rename",
                params.clone(),
                Duration::from_secs(15),
            )?;
            if let Some(error) = response.get("error") {
                if error.get("code").and_then(Value::as_i64) == Some(-32801)
                    && attempt < RENAME_CONTENT_MODIFIED_RETRIES
                {
                    thread::sleep(Duration::from_millis(50 * (1 << attempt)));
                    continue;
                }
                return Err(DevelopmentError::Process(format!(
                    "textDocument/rename failed: {error}"
                )));
            }
            let result = response.get("result").cloned().unwrap_or(Value::Null);
            if serde_json::to_vec(&result)?.len() > MAX_LSP_MESSAGE_BYTES {
                return Err(DevelopmentError::Serialization(
                    "language response exceeds the size limit".into(),
                ));
            }
            return Ok(LanguageResponse {
                method: "textDocument/rename".into(),
                result,
            });
        }
        unreachable!("bounded rename retry loop returns or errors")
    }

    /// rust-analyzer can answer the first rename request before its initial
    /// Cargo workspace index is ready. Retry only the known empty/no-reference
    /// outcomes, and only after a bounded read-only references probe confirms
    /// the selected symbol is indexed. Other errors remain visible.
    fn wait_for_rename_references(&mut self, uri: &str, position: &DiagnosticPosition) -> bool {
        let deadline = Instant::now() + RENAME_INDEX_WAIT;
        let params = serde_json::json!({
            "textDocument":{"uri":uri},
            "position":protocol_position(position),
            "context":{"includeDeclaration":true}
        });
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            // Keep only one references request in flight. Short timeouts leave
            // stale responses queued in the language server and can make a
            // busy server spend the whole readiness window answering probes
            // Glass has already abandoned.
            match self.request_result_with_timeout(
                "textDocument/references",
                params.clone(),
                remaining,
            ) {
                Ok(response)
                    if response
                        .result
                        .as_array()
                        .is_some_and(|references| !references.is_empty()) =>
                {
                    return true;
                }
                Err(error) if error.to_string().contains("timed out") => {}
                Err(_) => return false,
                _ => {}
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            thread::sleep(remaining.min(Duration::from_millis(250)));
        }
    }

    fn position_request(
        &mut self,
        method: &str,
        path: &str,
        line: u32,
        character: u32,
        extra: Value,
    ) -> DevelopmentResult<LanguageResponse> {
        let uri = self.sync_document(path)?.uri.clone();
        let mut params = serde_json::json!({"textDocument":{"uri":uri},"position":{"line":line.saturating_sub(1),"character":character.saturating_sub(1)}});
        if let (Some(target), Some(source)) = (params.as_object_mut(), extra.as_object()) {
            target.extend(source.clone());
        }
        self.request_result(method, params)
    }

    fn request_result(
        &mut self,
        method: &str,
        params: Value,
    ) -> DevelopmentResult<LanguageResponse> {
        self.request_result_with_timeout(method, params, Duration::from_secs(15))
    }

    fn request_result_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> DevelopmentResult<LanguageResponse> {
        let response = self.call(method, params, timeout)?;
        if let Some(error) = response.get("error") {
            return Err(DevelopmentError::Process(format!(
                "{method} failed: {error}"
            )));
        }
        let result = response.get("result").cloned().unwrap_or(Value::Null);
        if serde_json::to_vec(&result)?.len() > MAX_LSP_MESSAGE_BYTES {
            return Err(DevelopmentError::Serialization(
                "language response exceeds the size limit".into(),
            ));
        }
        Ok(LanguageResponse {
            method: method.into(),
            result,
        })
    }

    fn resolve_document(&self, path: &str) -> DevelopmentResult<(PathBuf, String)> {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(DevelopmentError::PathOutsideWorkspace(relative.into()));
        }
        let absolute = fs::canonicalize(self.root.join(relative))?;
        if !absolute.starts_with(&self.root) {
            return Err(DevelopmentError::PathOutsideWorkspace(absolute));
        }
        Ok((absolute, relative.to_string_lossy().into_owned()))
    }

    fn initialize(&mut self, initialization_options: Option<Value>) -> DevelopmentResult<()> {
        let root_uri = file_uri(&self.root)?;
        let mut initialize_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "workspace": {
                    "configuration": true,
                    "workspaceFolders": true,
                    "workspaceEdit": {"documentChanges": true}
                },
                "textDocument": {
                    "publishDiagnostics": {"relatedInformation": true},
                    "completion": {"completionItem": {"snippetSupport": false}},
                    "signatureHelp": {},
                    "codeAction": {"dataSupport": true},
                    "rename": {"prepareSupport": true},
                    "semanticTokens": {"requests": {"full": true}, "tokenTypes": [], "tokenModifiers": [], "formats": ["relative"]}
                }
            },
            "workspaceFolders": [{"uri": root_uri, "name": self.root.file_name().and_then(|name| name.to_str()).unwrap_or("project")}]
        });
        if let Some(options) = initialization_options {
            initialize_params["initializationOptions"] = options;
        }
        let response = self.call("initialize", initialize_params, Duration::from_secs(30))?;
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
            if let (Some(request_id), Some(server_method)) =
                (value.get("id"), value.get("method").and_then(Value::as_str))
            {
                let response = server_request_response(
                    server_method,
                    request_id.clone(),
                    value.get("params").unwrap_or(&Value::Null),
                    &self.server_settings,
                    &self.root,
                )?;
                self.notify(response)?;
                continue;
            }
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

fn workspace_edit_is_empty(result: &Value) -> bool {
    if result.is_null() {
        return true;
    }
    let document_changes_empty = result
        .get("documentChanges")
        .is_none_or(|changes| changes.as_array().is_some_and(Vec::is_empty));
    let changes_empty = result
        .get("changes")
        .is_none_or(|changes| changes.as_object().is_some_and(serde_json::Map::is_empty));
    document_changes_empty && changes_empty
}

fn note_empty_diagnostics(first_empty: &mut Option<Instant>, now: Instant) {
    first_empty.get_or_insert(now);
}

fn server_request_response(
    method: &str,
    id: Value,
    params: &Value,
    settings: &Value,
    root: &Path,
) -> DevelopmentResult<Value> {
    let result = match method {
        "workspace/configuration" => {
            let items = params
                .get("items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let values = items
                .iter()
                .map(|item| {
                    configuration_value(settings, item.get("section").and_then(Value::as_str))
                })
                .collect::<Vec<_>>();
            serde_json::json!(values)
        }
        "workspace/workspaceFolders" => {
            serde_json::json!([{
                "uri": file_uri(root)?,
                "name": root.file_name().and_then(|name| name.to_str()).unwrap_or("project")
            }])
        }
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create"
        | "window/showMessageRequest"
        | "workspace/diagnostic/refresh" => Value::Null,
        "workspace/applyEdit" => serde_json::json!({
            "applied": false,
            "failureReason": "Glass requires workspace edits to pass through its review proposals"
        }),
        "window/showDocument" => serde_json::json!({"success": false}),
        _ => {
            return Ok(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("unsupported language-server request: {method}")
                }
            }));
        }
    };
    Ok(serde_json::json!({"jsonrpc":"2.0","id":id,"result":result}))
}

fn configuration_value(settings: &Value, section: Option<&str>) -> Value {
    let Some(section) = section else {
        return settings.clone();
    };
    let mut value = settings.get("rust-analyzer").unwrap_or(settings);
    let section = section
        .strip_prefix("rust-analyzer")
        .map(|section| section.strip_prefix('.').unwrap_or(section))
        .unwrap_or(section);
    if section.is_empty() {
        return value.clone();
    }
    for part in section.split('.') {
        let Some(next) = value.get(part) else {
            return Value::Null;
        };
        value = next;
    }
    value.clone()
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if !self.shutdown {
            let _ = self.call("shutdown", Value::Null, Duration::from_secs(2));
            self.shutdown = true;
        }
        let _ = self.notify(serde_json::json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": null
        }));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Stable read-only view of an open document's protocol identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageDocument {
    pub uri: String,
    pub version: i64,
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

fn protocol_position(position: &DiagnosticPosition) -> Value {
    serde_json::json!({
        "line": position.line.saturating_sub(1),
        "character": position.character.saturating_sub(1)
    })
}

fn validate_lsp_text(
    description: &str,
    value: &str,
    limit: usize,
    allow_empty: bool,
) -> DevelopmentResult<()> {
    if (!allow_empty && value.is_empty())
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} must contain {}..={limit} non-control bytes",
            usize::from(!allow_empty)
        )));
    }
    Ok(())
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

fn rust_analyzer_initialization_options() -> Value {
    serde_json::json!({
        "rust-analyzer": {
            "cargo": {"allTargets": false},
            "check": {"allTargets": false, "workspace": false}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_empty_diagnostics_do_not_extend_the_grace_deadline() {
        let first = Instant::now();
        let mut published_at = None;
        note_empty_diagnostics(&mut published_at, first);
        note_empty_diagnostics(
            &mut published_at,
            first + DIAGNOSTICS_EMPTY_GRACE - Duration::from_secs(1),
        );
        assert_eq!(published_at, Some(first));
        assert_eq!(
            published_at.unwrap() + DIAGNOSTICS_EMPTY_GRACE,
            first + DIAGNOSTICS_EMPTY_GRACE
        );
    }

    #[test]
    fn raw_language_requests_reject_lifecycle_and_unbounded_inputs() {
        assert!(validate_lsp_text("method", "textDocument/hover", 256, false).is_ok());
        assert!(validate_lsp_text("method", "", 256, false).is_err());
        assert!(validate_lsp_text("method", "textDocument\nhover", 256, false).is_err());
        assert_eq!(
            protocol_position(&DiagnosticPosition {
                line: 4,
                character: 2
            }),
            serde_json::json!({"line":3,"character":1})
        );
    }

    #[test]
    fn rust_analyzer_uses_package_scoped_non_all_target_checks() {
        assert_eq!(
            rust_analyzer_initialization_options(),
            serde_json::json!({
                "rust-analyzer": {
                    "cargo": {"allTargets": false},
                    "check": {"allTargets": false, "workspace": false}
                }
            })
        );
    }

    #[test]
    fn language_server_configuration_requests_receive_the_selected_settings() {
        let settings = rust_analyzer_initialization_options();
        let response = server_request_response(
            "workspace/configuration",
            serde_json::json!(41),
            &serde_json::json!({"items":[
                {"section":"rust-analyzer"},
                {"section":"rust-analyzer.cargo"},
                {"section":"rust-analyzer.check.workspace"},
                {"section":"rust-analyzer.unknown"}
            ]}),
            &settings,
            Path::new("/tmp/project"),
        )
        .unwrap();
        assert_eq!(response["id"], 41);
        assert_eq!(
            response["result"],
            serde_json::json!([
                {"cargo":{"allTargets":false},"check":{"allTargets":false,"workspace":false}},
                {"allTargets":false},
                false,
                null
            ])
        );
    }

    #[test]
    fn server_initiated_workspace_edits_are_returned_to_review_in_glass() {
        let response = server_request_response(
            "workspace/applyEdit",
            serde_json::json!(9),
            &serde_json::json!({"edit":{"changes":{}}}),
            &Value::Null,
            Path::new("/tmp/project"),
        )
        .unwrap();
        assert_eq!(response["id"], 9);
        assert_eq!(response["result"]["applied"], false);
        assert!(
            response["result"]["failureReason"]
                .as_str()
                .unwrap()
                .contains("review proposals")
        );
    }

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
        #[cfg(unix)]
        let server_pid = client.child.id();
        assert_eq!(client.sync_document("src/lib.rs").unwrap().version, 1);
        fs::write(root.join("src/lib.rs"), "pub fn broken( -> u8 {\n").unwrap();
        assert_eq!(client.sync_document("src/lib.rs").unwrap().version, 2);
        client.save_document("src/lib.rs").unwrap();
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
        client.close_document("src/lib.rs").unwrap();
        drop(client);
        #[cfg(unix)]
        {
            // SAFETY: signal zero performs a read-only existence probe.
            let result = unsafe { libc::kill(server_pid as i32, 0) };
            assert_ne!(result, 0, "language server survived client shutdown");
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rust_analyzer_waits_for_cross_file_rename_index_when_available() {
        if !Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return;
        }
        let root = std::env::temp_dir().join(format!("glass-lsp-rename-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='glass_lsp_rename_fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        let source = "pub mod call;\npub fn legacy_name() {}\n";
        let call_site = "pub fn invoke() { crate::legacy_name(); }\n";
        fs::write(root.join("src/lib.rs"), source).unwrap();
        fs::write(root.join("src/call.rs"), call_site).unwrap();
        let check = Command::new("cargo")
            .args(["check", "--offline", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .status()
            .expect("check local rename fixture");
        assert!(check.success());

        let mut client = LspClient::spawn(&root, "rust-analyzer", &[]).unwrap();
        client
            .sync_document_with_content("src/call.rs", call_site)
            .unwrap();
        let response = client
            .rename_with_content("src/lib.rs", 2, 8, "modern_name", source)
            .unwrap();
        assert_eq!(response.method, "textDocument/rename");
        let mut changed_paths = std::collections::BTreeSet::new();
        if let Some(documents) = response
            .result
            .get("documentChanges")
            .and_then(Value::as_array)
        {
            for document in documents {
                let uri = document
                    .pointer("/textDocument/uri")
                    .and_then(Value::as_str);
                let has_rename =
                    document
                        .get("edits")
                        .and_then(Value::as_array)
                        .is_some_and(|edits| {
                            edits.iter().any(|edit| {
                                edit.get("newText")
                                    .and_then(Value::as_str)
                                    .is_some_and(|text| text == "modern_name")
                            })
                        });
                if has_rename && let Some(uri) = uri {
                    changed_paths.insert(uri.to_string());
                }
            }
        }
        assert!(
            changed_paths
                .iter()
                .any(|path| path.ends_with("/src/lib.rs")),
            "rust-analyzer rename omitted the declaration: {:#}",
            response.result
        );
        assert!(
            changed_paths
                .iter()
                .any(|path| path.ends_with("/src/call.rs")),
            "rust-analyzer rename omitted the call site: {:#}",
            response.result
        );
        drop(client);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rename_retries_only_the_lsp_content_modified_response() {
        let Some(python) = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("python3"))
            .find(|candidate| candidate.is_file())
        else {
            return;
        };
        let root = std::env::temp_dir().join(format!("glass-lsp-modified-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        let source = "// unsaved first\nfn old_name() {}\n";
        let call_site = "// unsaved second\nold_name();\n";
        fs::write(root.join("src/first.rs"), source).unwrap();
        fs::write(root.join("src/second.rs"), call_site).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_lsp_workspace_edit.py");
        let mut client = LspClient::spawn(
            &root,
            &python.to_string_lossy(),
            &[fixture.to_string_lossy().into_owned()],
        )
        .unwrap();
        client
            .sync_document_with_content("src/second.rs", call_site)
            .unwrap();
        let response = client
            .rename_with_content("src/first.rs", 2, 4, "retry_once", source)
            .expect("ContentModified is retried and the later WorkspaceEdit is returned");
        assert_eq!(
            response.result["documentChanges"].as_array().unwrap().len(),
            2
        );
        drop(client);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rename_waits_for_slow_references_without_overlapping_requests() {
        let Some(python) = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("python3"))
            .find(|candidate| candidate.is_file())
        else {
            return;
        };
        let root = std::env::temp_dir().join(format!("glass-lsp-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        let source = "// unsaved first\nfn old_name() {}\n";
        let call_site = "// unsaved second\nold_name();\n";
        fs::write(root.join("src/first.rs"), source).unwrap();
        fs::write(root.join("src/second.rs"), call_site).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_lsp_workspace_edit.py");
        let mut client = LspClient::spawn(
            &root,
            &python.to_string_lossy(),
            &[fixture.to_string_lossy().into_owned()],
        )
        .unwrap();
        client
            .sync_document_with_content("src/second.rs", call_site)
            .unwrap();
        let response = client
            .rename_with_content("src/first.rs", 2, 4, "retry_slow_references", source)
            .expect("a slow references response confirms the empty WorkspaceEdit was transient");
        assert_eq!(
            response.result["documentChanges"].as_array().unwrap().len(),
            2
        );
        drop(client);
        fs::remove_dir_all(root).unwrap();
    }
}
