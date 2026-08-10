//! Bounded discovery and classification of local Chrome debugging endpoints.
//!
//! A listening TCP socket is not attach authority.  Glass only offers attach
//! after a bounded HTTP probe proves that the endpoint speaks the expected CDP
//! discovery protocol.  The caller still has to make the attach decision.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const PROBE_TIMEOUT: Duration = Duration::from_millis(750);
const MAX_DISCOVERY_BYTES: usize = 512 * 1024;
const MAX_TARGETS: usize = 64;

/// What a bounded probe established about a requested local CDP endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointClassification {
    /// Nothing is listening on the requested port.
    Free,
    /// Chrome-compatible discovery endpoints were verified.
    CompatibleBrowser,
    /// A service answered, but it did not prove Chrome/CDP compatibility.
    UnrelatedService,
    /// The probe timed out or otherwise could not establish a safe result.
    Unknown,
}

/// Privacy-preserving target metadata suitable for a recovery picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredTarget {
    pub id: String,
    pub title: String,
    pub origin: String,
}

/// Evidence returned by an endpoint probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointProbe {
    pub port: u16,
    pub classification: EndpointClassification,
    pub product: Option<String>,
    pub targets: Vec<DiscoveredTarget>,
    pub detail: String,
}

/// Probe one loopback port without attaching to or mutating the service.
pub async fn probe_local_endpoint(port: u16) -> EndpointProbe {
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    match tokio::time::timeout(PROBE_TIMEOUT, tokio::net::TcpStream::connect(address)).await {
        Ok(Ok(stream)) => drop(stream),
        Ok(Err(error))
            if error.kind() == std::io::ErrorKind::ConnectionRefused
                || (cfg!(windows) && error.raw_os_error() == Some(10061)) =>
        {
            return probe(port, EndpointClassification::Free, "port is available");
        }
        Ok(Err(error)) => {
            return probe(
                port,
                EndpointClassification::Unknown,
                format!("TCP probe failed: {error}"),
            );
        }
        Err(_) => {
            return probe(port, EndpointClassification::Unknown, "TCP probe timed out");
        }
    }

    let client = match reqwest::Client::builder().timeout(PROBE_TIMEOUT).build() {
        Ok(client) => client,
        Err(error) => {
            return probe(
                port,
                EndpointClassification::Unknown,
                format!("HTTP probe setup failed: {error}"),
            );
        }
    };
    let version = match bounded_json(&client, port, "/json/version").await {
        Ok(value) => value,
        Err(error) => {
            return probe(port, EndpointClassification::UnrelatedService, error);
        }
    };
    let Some(websocket) = version
        .get("webSocketDebuggerUrl")
        .and_then(serde_json::Value::as_str)
    else {
        return probe(
            port,
            EndpointClassification::UnrelatedService,
            "missing CDP browser WebSocket URL",
        );
    };
    if !websocket.starts_with(&format!("ws://127.0.0.1:{port}/devtools/browser/"))
        && !websocket.starts_with(&format!("ws://localhost:{port}/devtools/browser/"))
    {
        return probe(
            port,
            EndpointClassification::Unknown,
            "CDP endpoint advertised a non-loopback or mismatched WebSocket URL",
        );
    }

    let product = version
        .get("Browser")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let targets = match bounded_json(&client, port, "/json/list").await {
        Ok(serde_json::Value::Array(values)) => values
            .into_iter()
            .take(MAX_TARGETS)
            .filter(|value| value.get("type").and_then(serde_json::Value::as_str) == Some("page"))
            .filter_map(project_target)
            .collect(),
        Ok(_) => Vec::new(),
        Err(_) => Vec::new(),
    };
    EndpointProbe {
        port,
        classification: EndpointClassification::CompatibleBrowser,
        product,
        targets,
        detail: "verified Chrome DevTools discovery endpoint".to_string(),
    }
}

/// Reserve an ephemeral loopback port for a subsequent bounded launch retry.
///
/// The listener is intentionally returned to the caller: keep it alive until
/// immediately before spawning Chrome to minimise the bind race.
pub fn reserve_loopback_port() -> std::io::Result<(u16, TcpListener)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    Ok((port, listener))
}

async fn bounded_json(
    client: &reqwest::Client,
    port: u16,
    path: &str,
) -> Result<serde_json::Value, String> {
    let response = client
        .get(format!("http://127.0.0.1:{port}{path}"))
        .send()
        .await
        .map_err(|error| format!("HTTP discovery failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP discovery returned {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("HTTP discovery body failed: {error}"))?;
    if bytes.len() > MAX_DISCOVERY_BYTES {
        return Err("HTTP discovery response exceeded the bounded limit".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid discovery JSON: {error}"))
}

fn project_target(value: serde_json::Value) -> Option<DiscoveredTarget> {
    let id = value.get("id")?.as_str()?;
    if id.is_empty() || id.len() > 256 {
        return None;
    }
    let id = id.to_string();
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Untitled")
        .chars()
        .take(80)
        .collect();
    let origin = value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .and_then(url_origin)
        .unwrap_or_else(|| "opaque".to_string());
    Some(DiscoveredTarget { id, title, origin })
}

fn url_origin(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    Some(match parsed.port() {
        Some(port) => format!("{}://{host}:{port}", parsed.scheme()),
        None => format!("{}://{host}", parsed.scheme()),
    })
}

fn probe(
    port: u16,
    classification: EndpointClassification,
    detail: impl Into<String>,
) -> EndpointProbe {
    EndpointProbe {
        port,
        classification,
        product: None,
        targets: Vec::new(),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn target_projection_drops_paths_queries_and_fragments() {
        let target = project_target(serde_json::json!({
            "id": "page-1",
            "type": "page",
            "title": "Account",
            "url": "https://example.test:8443/private?token=secret#card"
        }))
        .unwrap();
        assert_eq!(target.origin, "https://example.test:8443");
        assert!(!target.origin.contains("secret"));
    }

    #[test]
    fn port_reservation_is_loopback_and_nonzero() {
        let (port, listener) = reserve_loopback_port().unwrap();
        assert_ne!(port, 0);
        assert!(listener.local_addr().unwrap().ip().is_loopback());
    }

    #[tokio::test]
    async fn unused_port_is_classified_free() {
        let (port, listener) = reserve_loopback_port().unwrap();
        drop(listener);
        let result = probe_local_endpoint(port).await;
        assert_eq!(result.classification, EndpointClassification::Free);
    }

    #[tokio::test]
    async fn verified_cdp_is_compatible_and_projects_targets() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = vec![0_u8; 2048];
                let size = stream.read(&mut request).await.unwrap();
                if size == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&request[..size]);
                let body = if request.starts_with("GET /json/version") {
                    format!(
                        r#"{{"Browser":"Chrome/Test","webSocketDebuggerUrl":"ws://127.0.0.1:{port}/devtools/browser/owned"}}"#
                    )
                } else {
                    r#"[{"id":"p1","type":"page","title":"Private","url":"https://example.test/path?token=secret"}]"#.into()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let result = probe_local_endpoint(port).await;
        assert_eq!(
            result.classification,
            EndpointClassification::CompatibleBrowser
        );
        assert_eq!(result.targets[0].origin, "https://example.test");
        assert!(!serde_json::to_string(&result).unwrap().contains("secret"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn unrelated_http_service_never_becomes_attach_authority() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 512];
                if stream.read(&mut request).await.unwrap() == 0 {
                    continue;
                }
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"app\":\"x\"}").await.unwrap();
            }
        });
        let result = probe_local_endpoint(port).await;
        assert_eq!(
            result.classification,
            EndpointClassification::UnrelatedService
        );
        server.abort();
    }
}
