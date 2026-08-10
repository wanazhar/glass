//! Shared resident language-service ownership for humans and agents.

use glass_browser::development::{
    DevelopmentError, DevelopmentResult, DiagnosticPosition, LanguageDiagnostic, LanguageResponse,
    LspClient,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

const EVENT_LIMIT: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageServerConfig {
    pub command: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageServiceEvent {
    pub sequence: u64,
    pub server: String,
    pub actor: String,
    pub operation: String,
    pub path: Option<String>,
    pub result_count: Option<usize>,
}

/// One shared set of persistent language servers for the workspace.
pub struct LanguageService {
    root: PathBuf,
    servers: BTreeMap<String, LspClient>,
    events: VecDeque<LanguageServiceEvent>,
    next_event: u64,
}

impl LanguageService {
    pub fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        Ok(Self {
            root: std::fs::canonicalize(root)?,
            servers: BTreeMap::new(),
            events: VecDeque::new(),
            next_event: 1,
        })
    }

    pub fn start(&mut self, name: &str, config: &LanguageServerConfig) -> DevelopmentResult<()> {
        validate_name(name)?;
        validate_command(config)?;
        if self.servers.contains_key(name) {
            return Err(DevelopmentError::Conflict(format!(
                "language server {name} is already running"
            )));
        }
        let client = LspClient::spawn(&self.root, &config.command, &config.arguments)?;
        self.servers.insert(name.into(), client);
        self.record(name, "system", "started", None, None);
        Ok(())
    }

    pub fn start_rust_analyzer(&mut self) -> DevelopmentResult<()> {
        self.start(
            "rust",
            &LanguageServerConfig {
                command: "rust-analyzer".into(),
                arguments: Vec::new(),
            },
        )
    }

    pub fn stop(&mut self, name: &str) -> DevelopmentResult<()> {
        self.servers
            .remove(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("language server {name}")))?;
        self.record(name, "system", "stopped", None, None);
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.servers.keys().map(String::as_str)
    }

    pub fn client_mut(&mut self, name: &str) -> DevelopmentResult<&mut LspClient> {
        self.servers
            .get_mut(name)
            .ok_or_else(|| DevelopmentError::NotFound(format!("language server {name}")))
    }

    pub fn diagnostics(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
    ) -> DevelopmentResult<Vec<LanguageDiagnostic>> {
        validate_actor(actor)?;
        let diagnostics = self.client_mut(name)?.diagnostics(path)?;
        self.record(
            name,
            actor,
            "diagnostics",
            Some(path),
            Some(diagnostics.len()),
        );
        Ok(diagnostics)
    }

    pub fn completion(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        validate_actor(actor)?;
        let response = self.client_mut(name)?.completion(path, line, character)?;
        self.record(
            name,
            actor,
            "completion",
            Some(path),
            value_count(&response),
        );
        Ok(response)
    }

    pub fn hover(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        validate_actor(actor)?;
        let response = self.client_mut(name)?.hover(path, line, character)?;
        self.record(name, actor, "hover", Some(path), value_count(&response));
        Ok(response)
    }

    pub fn definition(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "definition", path, |client| {
            client.definition(path, line, character)
        })
    }

    pub fn declaration(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "declaration", path, |client| {
            client.declaration(path, line, character)
        })
    }

    pub fn implementation(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "implementation", path, |client| {
            client.implementation(path, line, character)
        })
    }

    pub fn references(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "references", path, |client| {
            client.references(path, line, character)
        })
    }

    pub fn document_symbols(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "documentSymbols", path, |client| {
            client.document_symbols(path)
        })
    }

    pub fn workspace_symbols(
        &mut self,
        name: &str,
        actor: &str,
        query: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "workspaceSymbols", "", |client| {
            client.workspace_symbols(query)
        })
    }

    pub fn signature_help(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "signatureHelp", path, |client| {
            client.signature_help(path, line, character)
        })
    }

    pub fn code_actions(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        start: DiagnosticPosition,
        end: DiagnosticPosition,
        diagnostics: Vec<Value>,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "codeActions", path, |client| {
            client.code_actions(path, start, end, &diagnostics)
        })
    }

    pub fn formatting(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "formatting", path, |client| {
            client.formatting(path)
        })
    }

    pub fn range_formatting(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        start: DiagnosticPosition,
        end: DiagnosticPosition,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "rangeFormatting", path, |client| {
            client.range_formatting(path, start, end)
        })
    }

    pub fn semantic_tokens(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "semanticTokens", path, |client| {
            client.semantic_tokens(path)
        })
    }

    pub fn rename(
        &mut self,
        name: &str,
        actor: &str,
        path: &str,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> DevelopmentResult<LanguageResponse> {
        self.position_request(name, actor, "rename", path, |client| {
            client.rename(path, line, character, new_name)
        })
    }

    fn position_request(
        &mut self,
        name: &str,
        actor: &str,
        operation: &str,
        path: &str,
        request: impl FnOnce(&mut LspClient) -> DevelopmentResult<LanguageResponse>,
    ) -> DevelopmentResult<LanguageResponse> {
        validate_actor(actor)?;
        let response = request(self.client_mut(name)?)?;
        self.record(
            name,
            actor,
            operation,
            (!path.is_empty()).then_some(path),
            value_count(&response),
        );
        Ok(response)
    }

    pub fn events(&self, since: u64) -> Vec<LanguageServiceEvent> {
        self.events
            .iter()
            .filter(|event| event.sequence > since)
            .cloned()
            .collect()
    }

    fn record(
        &mut self,
        server: &str,
        actor: &str,
        operation: &str,
        path: Option<&str>,
        result_count: Option<usize>,
    ) {
        if self.events.len() == EVENT_LIMIT {
            self.events.pop_front();
        }
        self.events.push_back(LanguageServiceEvent {
            sequence: self.next_event,
            server: server.into(),
            actor: actor.into(),
            operation: operation.into(),
            path: path.map(str::to_string),
            result_count,
        });
        self.next_event = self.next_event.saturating_add(1);
    }
}

fn value_count(response: &LanguageResponse) -> Option<usize> {
    response
        .result
        .as_array()
        .map(Vec::len)
        .or_else(|| response.result.as_object().map(serde_json::Map::len))
}

fn validate_name(name: &str) -> DevelopmentResult<()> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(DevelopmentError::InvalidInput(
            "language server names must be 1..=64 ASCII letters, digits, '-' or '_'".into(),
        ));
    }
    Ok(())
}

fn validate_actor(actor: &str) -> DevelopmentResult<()> {
    if actor.is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
        return Err(DevelopmentError::InvalidInput(
            "language-service actor must contain 1..=256 non-control bytes".into(),
        ));
    }
    Ok(())
}

fn validate_command(config: &LanguageServerConfig) -> DevelopmentResult<()> {
    if config.command.is_empty()
        || config.command.len() > 4096
        || config.command.chars().any(char::is_control)
        || config.arguments.len() > 64
        || config.arguments.iter().any(|argument| {
            argument.len() > 4096 || argument.chars().any(|character| character == '\0')
        })
    {
        return Err(DevelopmentError::InvalidInput(
            "language server command or arguments are invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn language_service_configuration_fails_closed() {
        assert!(validate_name("rust").is_ok());
        assert!(validate_name("../rust").is_err());
        assert!(
            validate_command(&LanguageServerConfig {
                command: "rust-analyzer".into(),
                arguments: Vec::new()
            })
            .is_ok()
        );
        assert!(validate_actor("embedded-agent-1").is_ok());
        assert!(validate_actor("bad\nactor").is_err());
    }

    #[test]
    fn shared_rust_language_service_serves_multiple_actors() {
        if !Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return;
        }
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-shared-lsp-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='shared_lsp_fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn value() -> u32 { 42 }\npub fn use_value() { val }\n",
        )
        .unwrap();
        let mut service = LanguageService::new(&root).unwrap();
        service.start_rust_analyzer().unwrap();
        let response = service
            .completion("rust", "human", "src/lib.rs", 2, 25)
            .unwrap();
        assert_eq!(response.method, "textDocument/completion");
        let hover = service
            .hover("rust", "embedded-agent", "src/lib.rs", 1, 8)
            .unwrap();
        assert_eq!(hover.method, "textDocument/hover");
        assert_eq!(service.names().collect::<Vec<_>>(), vec!["rust"]);
        assert_eq!(service.events(0).len(), 3);
        service.stop("rust").unwrap();
        drop(service);
        std::fs::remove_dir_all(root).unwrap();
    }
}
