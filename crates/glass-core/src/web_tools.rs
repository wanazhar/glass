//! Core-owned web search and readable HTTP fetch tools.
//!
//! Search uses Exa's hosted, keyless Streamable HTTP MCP endpoint by default.
//! Fetch is a direct GET through the Core HTTP client; neither path launches a
//! browser or asks a second permission authority.

use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use url::Url;

const SEARCH_ENDPOINT: &str = "https://mcp.exa.ai/mcp";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_FETCH_BYTES: u64 = 512 * 1024;
const DEFAULT_FETCH_CHARS: usize = 20_000;
const MAX_FETCH_CHARS: usize = 60_000;
const MAX_SEARCH_RESULTS: u64 = 25;

pub fn is_tool(name: &str) -> bool {
    matches!(name, "web_search" | "web_fetch")
}

pub fn execute(call: &glass_mod_api::ToolCall) -> crate::tools::Outcome {
    let args = match serde_json::from_str::<Value>(&call.arguments) {
        Ok(value) if value.is_object() => value,
        Ok(_) => return crate::tools::Outcome::err("arguments must be a JSON object"),
        Err(error) => {
            return crate::tools::Outcome::err(format!("invalid JSON arguments: {error}"));
        }
    };
    let result = match call.name.as_str() {
        "web_search" => search(&args),
        "web_fetch" => fetch(&args),
        _ => return crate::tools::Outcome::err("unknown web tool"),
    };
    match result {
        Ok(text) => crate::tools::Outcome::ok(text),
        Err(error) => crate::tools::Outcome::err(error),
    }
}

pub fn fetch(arguments: &Value) -> Result<String, String> {
    let raw_url = required_string(arguments, "url")?;
    let url = validate_http_url(raw_url)?;
    let max_chars = arguments
        .get("max_chars")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_FETCH_CHARS as u64)
        .clamp(1, MAX_FETCH_CHARS as u64)
        .try_into()
        .unwrap_or(MAX_FETCH_CHARS);

    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let mut final_url = url;
    let mut response = None;
    for _ in 0..=5 {
        let current = agent_with_timeout(remaining(deadline)?)
            .get(final_url.as_str())
            .header("User-Agent", "glass-core/1.0 (web fetch)")
            .header(
                "Accept",
                "text/html,application/xhtml+xml,text/plain,application/json,application/xml,text/xml;q=0.9,*/*;q=0.1",
            )
            .call()
            .map_err(|error| format!("web_fetch request failed: {error}"))?;
        let status = current.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = current
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| format!("web_fetch received HTTP {status} without a Location"))?;
            final_url = final_url
                .join(location)
                .map_err(|_| "web_fetch received an invalid redirect URL".to_string())?;
            final_url = validate_http_url(final_url.as_str())?;
            continue;
        }
        response = Some(current);
        break;
    }
    let mut response = response.ok_or_else(|| "web_fetch exceeded five redirects".to_string())?;

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_owned();
    if !(200..300).contains(&status) {
        return Err(format!("web_fetch returned HTTP {status} from {final_url}"));
    }
    let media_type = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if !(media_type.starts_with("text/")
        || matches!(
            media_type.as_str(),
            "application/json" | "application/xml" | "application/xhtml+xml"
        ))
    {
        return Err(format!(
            "web_fetch does not extract {content_type}; use browser tools for visual or interactive pages"
        ));
    }

    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_FETCH_BYTES)
        .read_to_vec()
        .map_err(|error| format!("web_fetch could not read response body: {error}"))?;
    let (source, decode_warning) = decode_response(&bytes, &content_type);
    let text = if media_type == "text/html" || media_type == "application/xhtml+xml" {
        html_to_text(&source)
    } else {
        source
    };
    let char_count = text.chars().count();
    let excerpt: String = text.chars().take(max_chars).collect();
    let truncated = char_count > max_chars;
    Ok(format!(
        "URL: {final_url}\nHTTP: {status}\nContent-Type: {content_type}\n{}{}{}",
        excerpt.trim(),
        decode_warning
            .as_deref()
            .map(|warning| format!("\n[{warning}]"))
            .unwrap_or_default(),
        if truncated {
            "\n[content truncated; use web_fetch with a larger max_chars, up to 60000]"
        } else {
            ""
        }
    ))
}

pub fn search(arguments: &Value) -> Result<String, String> {
    let query = required_string(arguments, "query")?.trim();
    if query.is_empty() || query.len() > 2048 {
        return Err("web_search query must be 1..=2048 bytes".into());
    }
    let count = arguments
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(1, MAX_SEARCH_RESULTS);
    exa_search(query, count)
}

fn exa_search(query: &str, count: u64) -> Result<String, String> {
    let (endpoint, key) = search_config()?;
    exa_search_at(&endpoint, key.as_deref(), query, count)
}

fn exa_search_at(
    endpoint: &Url,
    key: Option<&str>,
    query: &str,
    count: u64,
) -> Result<String, String> {
    let deadline = Instant::now() + REQUEST_TIMEOUT;
    let initialize = json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2025-03-26","capabilities":{},
            "clientInfo":{"name":"glass-core","version":env!("CARGO_PKG_VERSION")}}
    });
    let (initialized, session) = mcp_post(endpoint, &initialize, key, None, deadline)?;
    if initialized.get("error").is_some() || initialized.get("result").is_none() {
        return Err(mcp_error("initialize", &initialized));
    }
    let notification = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
    let _ = mcp_post(endpoint, &notification, key, session.as_deref(), deadline)?;

    let call = json!({
        "jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"web_search_exa","arguments":{"query":query,"numResults":count}}
    });
    let (result, _) = mcp_post(endpoint, &call, key, session.as_deref(), deadline)?;
    if result.get("error").is_some() {
        return Err(mcp_error("web_search_exa", &result));
    }
    let content = result
        .pointer("/result/content")
        .and_then(Value::as_array)
        .ok_or_else(|| "Exa MCP returned no tool content".to_string())?;
    if result.pointer("/result/isError").and_then(Value::as_bool) == Some(true) {
        return Err(content
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"));
    }
    let text = content
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return Err("Exa MCP returned empty search content".into());
    }
    let text = format_exa_search_results(&text, usize::try_from(count).unwrap_or(usize::MAX));
    let character_count = text.chars().count();
    let mut output: String = text.chars().take(32_000).collect();
    if character_count > 32_000 {
        output.push_str("\n[search output truncated at 32000 characters]");
    }
    Ok(output)
}

fn format_exa_search_results(text: &str, count: usize) -> String {
    let mut blocks = Vec::new();
    let mut block = String::new();
    for line in text.lines() {
        if line.trim() == "---" {
            blocks.push(std::mem::take(&mut block));
        } else {
            if !block.is_empty() {
                block.push('\n');
            }
            block.push_str(line);
        }
    }
    if !block.is_empty() {
        blocks.push(block);
    }
    let has_source_urls = blocks.iter().any(|block| source_url(block).is_some());
    if !has_source_urls {
        return format!("Search provider: Exa MCP\n\n{text}");
    }

    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let mut source_count = 0usize;
    for block in &blocks {
        if let Some(raw_url) = source_url(block) {
            if let Some(key) = canonical_search_url(raw_url)
                && !seen.insert(key)
            {
                continue;
            }
            if source_count >= count {
                break;
            }
            source_count += 1;
            selected.push(block.as_str());
        } else if source_count == 0 && selected.is_empty() {
            // Preserve an introductory note before the first result, if any.
            selected.push(block.as_str());
        }
    }
    format!(
        "Search provider: Exa MCP · {source_count} unique sources\n\n{}",
        selected.join("\n\n---\n\n")
    )
}

fn source_url(block: &str) -> Option<&str> {
    block.lines().find_map(|line| {
        line.strip_prefix("URL:")
            .map(str::trim)
            .filter(|url| !url.is_empty())
    })
}

fn canonical_search_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.set_fragment(None);
    let query = url
        .query_pairs()
        .filter(|(name, _)| {
            let name = name.to_ascii_lowercase();
            !(name.starts_with("utm_")
                || matches!(
                    name.as_str(),
                    "fbclid" | "gclid" | "dclid" | "msclkid" | "mc_cid" | "mc_eid"
                ))
        })
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Some(url.to_string())
}

fn decode_response(bytes: &[u8], content_type: &str) -> (String, Option<String>) {
    let declared = content_type.split(';').skip(1).find_map(|parameter| {
        let (name, value) = parameter.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"').trim_matches('\'').to_owned())
    });
    if let Some(label) = declared {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            let (decoded, _, had_errors) = encoding.decode(bytes);
            let warning = had_errors
                .then(|| format!("invalid bytes for declared {label} charset were replaced"));
            return (decoded.into_owned(), warning);
        }
        let (decoded, _, _) = encoding_rs::UTF_8.decode(bytes);
        return (
            decoded.into_owned(),
            Some(format!(
                "unknown declared charset {label}; decoded as UTF-8"
            )),
        );
    }
    let (decoded, _, had_errors) = encoding_rs::UTF_8.decode(bytes);
    let warning = had_errors.then(|| "invalid UTF-8 bytes were replaced".to_owned());
    (decoded.into_owned(), warning)
}

fn mcp_post(
    endpoint: &Url,
    payload: &Value,
    api_key: Option<&str>,
    session: Option<&str>,
    deadline: Instant,
) -> Result<(Value, Option<String>), String> {
    let body = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
    let agent = agent_with_timeout(remaining(deadline)?);
    let mut request = agent
        .post(endpoint.as_str())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", "2025-03-26")
        .header(
            "User-Agent",
            concat!(
                "glass-core/",
                env!("CARGO_PKG_VERSION"),
                " (Core web search)"
            ),
        )
        .header("x-exa-source", "glass-core");
    if let Some(key) = api_key {
        request = request.header("x-api-key", key);
    }
    if let Some(session) = session {
        request = request.header("Mcp-Session-Id", session);
    }
    let mut response = request
        .send(body.as_slice())
        .map_err(|error| format!("Exa MCP request failed: {error}"))?;
    let status = response.status().as_u16();
    let session = response
        .headers()
        .get("Mcp-Session-Id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .or_else(|| session.map(str::to_owned));
    if !(200..300).contains(&status) {
        let detail = response
            .body_mut()
            .with_config()
            .limit(16 * 1024)
            .read_to_string()
            .unwrap_or_default();
        return Err(format!(
            "Exa MCP returned HTTP {status}: {}",
            detail.chars().take(400).collect::<String>()
        ));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(128 * 1024)
        .read_to_string()
        .map_err(|error| format!("could not read Exa MCP response: {error}"))?;
    let result: Value = if body.trim_start().starts_with("data:") || body.contains("\ndata:") {
        let data = body
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .rfind(|line| !line.is_empty() && *line != "[DONE]")
            .ok_or_else(|| "Exa MCP event stream contained no result".to_string())?;
        serde_json::from_str(data).map_err(|error| format!("invalid Exa MCP event: {error}"))?
    } else if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&body)
            .map_err(|error| format!("invalid Exa MCP JSON response: {error}"))?
    };
    Ok((result, session))
}

fn remaining(deadline: Instant) -> Result<Duration, String> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(|| "web request exceeded its 20-second total time limit".into())
}

fn agent_with_timeout(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .build()
        .into()
}

fn search_config() -> Result<(Url, Option<String>), String> {
    let configured = std::env::var_os("GC_WEB_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::config::config_dir().join("web.json"));
    let value = match std::fs::read(&configured) {
        Ok(bytes) if bytes.len() <= 64 * 1024 => serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| format!("{} is not valid JSON", configured.display()))?,
        Ok(_) => return Err("GC web config exceeds 64 KiB".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
        Err(error) => return Err(format!("could not read GC web config: {error}")),
    };
    let endpoint = std::env::var("GC_WEB_SEARCH_ENDPOINT")
        .ok()
        .or_else(|| {
            value
                .get("exa_mcp_url")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| SEARCH_ENDPOINT.to_owned());
    let endpoint = validate_http_url(&endpoint)?;
    let key = std::env::var("EXA_API_KEY").ok().or_else(|| {
        value
            .get("exa_api_key")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    Ok((endpoint, key.filter(|key| !key.trim().is_empty())))
}

fn required_string<'a>(value: &'a Value, name: &str) -> Result<&'a str, String> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("`{name}` must be a string"))
}

fn validate_http_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "URL must be absolute HTTP(S)".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("URL must be absolute HTTP(S) without embedded credentials".into());
    }
    Ok(url)
}

fn mcp_error(tool: &str, value: &Value) -> String {
    let message = value
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("unknown protocol error");
    format!("Exa MCP {tool} failed: {message}")
}

fn html_to_text(source: &str) -> String {
    let max_capacity = usize::try_from(MAX_FETCH_BYTES).unwrap_or(usize::MAX);
    let mut output = String::with_capacity(source.len().min(max_capacity));
    let mut chars = source.chars().peekable();
    let mut in_tag = false;
    let mut tag = String::new();
    let mut skip_depth = 0u8;
    let mut skipped_tag = String::new();
    while let Some(character) = chars.next() {
        if in_tag {
            if character == '>' {
                in_tag = false;
                let text = tag.trim();
                let closing = text.starts_with('/');
                let name = text
                    .trim_start_matches('/')
                    .split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "script" | "style" | "noscript" | "svg" | "head"
                ) {
                    if closing && name == skipped_tag {
                        skip_depth = skip_depth.saturating_sub(1);
                        if skip_depth == 0 {
                            skipped_tag.clear();
                        }
                    } else if !closing && skip_depth == 0 {
                        skipped_tag = name;
                        skip_depth = 1;
                    }
                } else if skip_depth == 0
                    && matches!(
                        name.as_str(),
                        "p" | "div"
                            | "br"
                            | "li"
                            | "tr"
                            | "table"
                            | "section"
                            | "article"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "pre"
                            | "blockquote"
                            | "main"
                            | "hr"
                    )
                {
                    output.push('\n');
                }
                tag.clear();
            } else if tag.len() < 128 {
                tag.push(character);
            }
            continue;
        }
        if character == '<' {
            in_tag = true;
            continue;
        }
        if skip_depth > 0 {
            continue;
        }
        if character == '&' {
            let mut entity = String::new();
            for next in chars.by_ref().take(16) {
                if next == ';' {
                    break;
                }
                if !next.is_ascii_alphanumeric() && next != '#' {
                    break;
                }
                entity.push(next);
            }
            output.push(match entity.as_str() {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" | "#39" => '\'',
                "nbsp" => ' ',
                _ if entity.starts_with("#x") => u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or(' '),
                _ if entity.starts_with('#') => entity[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or(' '),
                _ => ' ',
            });
        } else {
            output.push(character);
        }
    }
    let mut normalized = String::with_capacity(output.len());
    let mut line_space = false;
    for character in output.chars() {
        if character.is_whitespace() {
            if character == '\n' {
                while normalized.ends_with(' ') {
                    normalized.pop();
                }
                if !normalized.ends_with('\n') {
                    normalized.push('\n');
                }
                line_space = false;
            } else if !line_space && !normalized.ends_with('\n') {
                normalized.push(' ');
                line_space = true;
            }
        } else {
            normalized.push(character);
            line_space = false;
        }
    }
    normalized.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn read_request(stream: &mut TcpStream) -> (String, String) {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let content_length = String::from_utf8_lossy(&bytes[..header_end])
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    let headers = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
                    let body = String::from_utf8_lossy(
                        &bytes[header_end + 4..header_end + 4 + content_length],
                    )
                    .into_owned();
                    return (headers, body);
                }
            }
        }
        (String::from_utf8_lossy(&bytes).into_owned(), String::new())
    }

    fn reply(stream: &mut TcpStream, status: &str, headers: &str, body: &str) {
        reply_bytes(stream, status, headers, body.as_bytes());
    }

    fn reply_bytes(stream: &mut TcpStream, status: &str, headers: &str, body: &[u8]) {
        // The header is written separately so arbitrary encoded response bytes
        // are never formatted as UTF-8 text.
        write!(
            stream,
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn fetch_follows_bounded_redirect_and_returns_readable_source_metadata() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let (request, _) = read_request(&mut first);
            assert!(request.starts_with("GET /start "));
            reply(&mut first, "302 Found", "Location: /article\r\n", "");
            let (mut second, _) = listener.accept().unwrap();
            let (request, _) = read_request(&mut second);
            assert!(request.starts_with("GET /article "));
            let html = "<html><head><title>ignored</title><style>hidden style</style></head><body><h1>Useful page</h1><p>A &amp; B.</p><script>secret()</script><p>Next paragraph</p></body></html>";
            reply(
                &mut second,
                "200 OK",
                "Content-Type: text/html; charset=utf-8\r\n",
                html,
            );
        });
        let output = fetch(&json!({"url":format!("http://{address}/start")})).unwrap();
        server.join().unwrap();
        assert!(output.contains(&format!("URL: http://{address}/article")));
        assert!(output.contains("HTTP: 200"));
        assert!(output.contains("Content-Type: text/html"));
        assert!(output.contains("Useful page"));
        assert!(output.contains("A & B."));
        assert!(output.contains("Next paragraph"));
        assert!(!output.contains("hidden style"));
        assert!(!output.contains("secret()"));
        assert!(!output.contains("ignored"));
    }

    #[test]
    fn fetch_reports_non_success_and_rejects_non_text_without_claiming_success() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut missing, _) = listener.accept().unwrap();
            let _ = read_request(&mut missing);
            reply(
                &mut missing,
                "404 Not Found",
                "Content-Type: text/plain\r\n",
                "missing",
            );
            let (mut binary, _) = listener.accept().unwrap();
            let _ = read_request(&mut binary);
            reply(
                &mut binary,
                "200 OK",
                "Content-Type: application/pdf\r\n",
                "%PDF",
            );
        });
        let missing = fetch(&json!({"url":format!("http://{address}/missing")})).unwrap_err();
        assert!(missing.contains("HTTP 404"));
        let binary = fetch(&json!({"url":format!("http://{address}/report")})).unwrap_err();
        assert!(binary.contains("does not extract application/pdf"));
        server.join().unwrap();
    }

    #[test]
    fn fetch_decodes_declared_legacy_charset_and_reports_unknown_charset() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut legacy, _) = listener.accept().unwrap();
            let _ = read_request(&mut legacy);
            reply_bytes(
                &mut legacy,
                "200 OK",
                "Content-Type: text/plain; charset=windows-1252\r\n",
                b"caf\xe9",
            );
            let (mut unknown, _) = listener.accept().unwrap();
            let _ = read_request(&mut unknown);
            reply_bytes(
                &mut unknown,
                "200 OK",
                "Content-Type: text/plain; charset=x-no-such-charset\r\n",
                b"plain utf-8 text",
            );
        });
        let legacy = fetch(&json!({"url":format!("http://{address}/legacy")})).unwrap();
        assert!(legacy.contains("café"), "{legacy}");
        let unknown = fetch(&json!({"url":format!("http://{address}/unknown")})).unwrap();
        assert!(unknown.contains("plain utf-8 text"), "{unknown}");
        assert!(unknown.contains("unknown declared charset x-no-such-charset"));
        server.join().unwrap();
    }

    #[test]
    fn fetch_fails_explicitly_when_body_exceeds_the_byte_limit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            reply_bytes(
                &mut stream,
                "200 OK",
                "Content-Type: text/plain; charset=utf-8\r\n",
                &vec![b'x'; MAX_FETCH_BYTES as usize + 1],
            );
        });
        let error = fetch(&json!({"url":format!("http://{address}/large")})).unwrap_err();
        server.join().unwrap();
        assert!(
            error.contains("web_fetch could not read response body"),
            "{error}"
        );
    }

    #[test]
    fn fetch_rejects_credentialed_redirect_before_following_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_request(&mut stream);
            reply(
                &mut stream,
                "302 Found",
                "Location: http://user:secret@example.test/private\r\n",
                "",
            );
        });
        let error = fetch(&json!({"url":format!("http://{address}/start")})).unwrap_err();
        server.join().unwrap();
        assert!(error.contains("without embedded credentials"));
    }

    #[test]
    fn exa_mcp_search_uses_initialize_session_and_bounded_tool_call() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let (headers, body) = read_request(&mut stream);
                assert!(headers.to_ascii_lowercase().contains("x-api-key: test-key"));
                if index > 0 {
                    assert!(
                        headers
                            .to_ascii_lowercase()
                            .contains("mcp-session-id: fixture-session")
                    );
                }
                let request: Value = serde_json::from_str(&body).unwrap();
                requests.push(request.clone());
                match request.get("method").and_then(Value::as_str).unwrap() {
                    "initialize" => reply(
                        &mut stream,
                        "200 OK",
                        "Content-Type: application/json\r\nMcp-Session-Id: fixture-session\r\n",
                        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}"#,
                    ),
                    "notifications/initialized" => reply(
                        &mut stream,
                        "202 Accepted",
                        "Mcp-Session-Id: fixture-session\r\n",
                        "",
                    ),
                    "tools/call" => {
                        let mut result = "Title: First\nURL: https://docs.example.test/?utm_source=one#section\nPublished: 2026-10-23\nHighlights:\nPrimary excerpt.\n\n---\n\nTitle: Duplicate\nURL: https://docs.example.test/\nPublished: 2026-10-22\nHighlights:\nDuplicate excerpt.\n\n---\n\nTitle: Second\nURL: https://other.example.test/\nPublished: 2026-10-21\nHighlights:\nAnother source.".to_string();
                        result.push_str(&"x".repeat(32_010));
                        let response = json!({"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":result}]}}).to_string();
                        reply(
                            &mut stream,
                            "200 OK",
                            "Content-Type: application/json\r\nMcp-Session-Id: fixture-session\r\n",
                            &response,
                        );
                    }
                    method => panic!("unexpected MCP method: {method}"),
                }
            }
            requests
        });
        let endpoint = Url::parse(&format!("http://{address}/mcp")).unwrap();
        let result = exa_search_at(&endpoint, Some("test-key"), "fixture query", 4).unwrap();
        let requests = server.join().unwrap();
        assert!(result.starts_with("Search provider: Exa MCP · 2 unique sources"));
        assert!(result.contains("https://docs.example.test/"));
        assert!(result.contains("Primary excerpt."));
        assert!(!result.contains("Duplicate excerpt."));
        assert!(result.contains("Title: Second"));
        assert!(result.contains("[search output truncated at 32000 characters]"));
        assert_eq!(requests[2]["params"]["name"], "web_search_exa");
        assert_eq!(requests[2]["params"]["arguments"]["query"], "fixture query");
        assert_eq!(requests[2]["params"]["arguments"]["numResults"], 4);
    }

    #[test]
    fn search_deduplicates_tracking_and_fragment_variants_before_counting_sources() {
        let response = "Title: First ranked\nURL: https://EXAMPLE.test/article?utm_source=one#intro\nPublished: 2026-10-23\nHighlights:\nFirst excerpt.\n\n---\n\nTitle: Duplicate\nURL: https://example.test/article?utm_campaign=two\nPublished: 2026-10-23\nHighlights:\nDuplicate excerpt.\n\n---\n\nTitle: Distinct query\nURL: https://example.test/article?id=2\nPublished: 2026-10-22\nHighlights:\nDistinct excerpt.";
        let output = format_exa_search_results(response, 2);
        assert!(output.starts_with("Search provider: Exa MCP · 2 unique sources"));
        assert!(output.contains("Title: First ranked"));
        assert!(output.contains("URL: https://EXAMPLE.test/article?utm_source=one#intro"));
        assert!(output.contains("Published: 2026-10-23"));
        assert!(output.contains("First excerpt."));
        assert!(!output.contains("Title: Duplicate"));
        assert!(!output.contains("Duplicate excerpt."));
        assert!(output.contains("Title: Distinct query"));
        assert!(output.contains("Distinct excerpt."));
        assert_ne!(
            canonical_search_url("https://example.test/article?id=1"),
            canonical_search_url("https://example.test/article?id=2")
        );
    }

    #[test]
    fn search_validation_and_html_normalization_are_bounded() {
        assert!(
            search(&json!({"query":"  "}))
                .unwrap_err()
                .contains("query")
        );
        let text = html_to_text("<p>one &amp; two</p><script>x</script><p>three</p>");
        assert_eq!(text, "one & two\nthree");
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("https://user:secret@example.test/").is_err());
    }

    #[test]
    fn exa_search_metadata_preserves_source_provenance() {
        let formatted = format_exa_search_results(
            "Title: Official guide\nURL: https://docs.example.test/guide\nPublished: 2026-10-22\nAuthor: Core team\nHighlights:\nPrimary source excerpt.",
            5,
        );
        assert!(formatted.starts_with("Search provider: Exa MCP · 1 unique sources"));
        assert!(formatted.contains("URL: https://docs.example.test/guide"));
        assert!(formatted.contains("Published: 2026-10-22"));
        assert!(formatted.contains("Author: Core team"));
        assert!(formatted.contains("Primary source excerpt."));
    }
}
