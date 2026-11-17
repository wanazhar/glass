//! Telling the person when the assistant needs them.
//!
//! An always-on assistant is only useful if it can reach a human who is not
//! looking at it. Two channels cover that without adding a provider SDK:
//!
//! - a webhook (ntfy, Home Assistant, a Tailscale-hosted receiver), posted as
//!   plain JSON over HTTP, or through `curl` when the URL is HTTPS;
//! - Apple Push Notification service, using token-based authentication with a
//!   `.p8` key the operator already has. The JWT is signed with `openssl` and
//!   the request is sent with `curl --http2`, so no TLS stack is vendored here.
//!
//! Every delivery is best effort: a failed notification is logged and dropped,
//! never allowed to fail or delay a turn.

use crate::events::EventBus;
use crate::state::ProductState;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Device tokens one instance pushes to.
pub const DEVICE_LIMIT: usize = 16;
/// Events a person can subscribe to.
pub const NOTIFY_EVENTS: [&str; 5] = ["approval", "settled", "failed", "routine", "memory"];
/// Bound on any outbound notification request.
const DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// Apple Push Notification service credentials.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApnsSettings {
    /// Path to the `.p8` authentication key on the host.
    #[serde(default)]
    pub key_file: String,
    #[serde(default)]
    pub team_id: String,
    #[serde(default)]
    pub key_id: String,
    /// App bundle id, sent as `apns-topic`.
    #[serde(default)]
    pub topic: String,
    /// `production` (default) or `sandbox`.
    #[serde(default)]
    pub environment: String,
}

impl ApnsSettings {
    fn is_configured(&self) -> bool {
        !self.key_file.trim().is_empty()
            && !self.team_id.trim().is_empty()
            && !self.key_id.trim().is_empty()
            && !self.topic.trim().is_empty()
    }

    fn host(&self) -> &'static str {
        if self.environment.eq_ignore_ascii_case("sandbox") {
            "https://api.sandbox.push.apple.com/3/device/"
        } else {
            "https://api.push.apple.com/3/device/"
        }
    }
}

/// One registered device that can receive a push.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRegistration {
    pub token: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub last_seen_at: u64,
}

/// Where notifications go, and which events are worth waking a phone for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    #[serde(default)]
    pub webhook_url: String,
    /// Optional bearer token sent with the webhook.
    #[serde(default)]
    pub webhook_token: String,
    #[serde(default = "default_events")]
    pub events: Vec<String>,
    #[serde(default)]
    pub apns: Option<ApnsSettings>,
}

fn default_events() -> Vec<String> {
    NOTIFY_EVENTS
        .iter()
        .map(|event| (*event).to_string())
        .collect()
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            webhook_url: String::new(),
            webhook_token: String::new(),
            events: default_events(),
            apns: None,
        }
    }
}

impl NotificationSettings {
    /// Whether this event should be delivered somewhere.
    pub fn wants(&self, event: &str) -> bool {
        if !self.events.iter().any(|configured| configured == event) {
            return false;
        }
        !self.webhook_url.trim().is_empty()
            || self
                .apns
                .as_ref()
                .map(ApnsSettings::is_configured)
                .unwrap_or(false)
    }

    /// Product document: settings plus what is actually deliverable.
    pub fn document(&self) -> Value {
        json!({
            "webhookUrl": self.webhook_url,
            "webhookTokenSet": !self.webhook_token.is_empty(),
            "events": self.events,
            "availableEvents": NOTIFY_EVENTS,
            "apns": self.apns.clone(),
            "apnsConfigured": self
                .apns
                .as_ref()
                .map(ApnsSettings::is_configured)
                .unwrap_or(false),
            "webhookConfigured": !self.webhook_url.trim().is_empty(),
        })
    }

    /// Compact summary used by the presence document.
    pub fn summary(&self) -> Value {
        json!({
            "webhook": !self.webhook_url.trim().is_empty(),
            "apns": self
                .apns
                .as_ref()
                .map(ApnsSettings::is_configured)
                .unwrap_or(false),
            "events": self.events,
        })
    }
}

/// One message worth interrupting a person for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// `approval`, `settled`, `failed`, `routine`, or `memory`.
    pub event: String,
    pub title: String,
    pub body: String,
    pub conversation_id: String,
    pub task_id: String,
}

impl Notification {
    /// Build one notification with a bounded body.
    pub fn new(
        event: &str,
        title: impl Into<String>,
        body: impl Into<String>,
        conversation_id: &str,
        task_id: &str,
    ) -> Self {
        let body = body.into();
        let body = if body.chars().count() > 320 {
            let mut short: String = body.chars().take(320).collect();
            short.push('…');
            short
        } else {
            body
        };
        Self {
            event: event.to_string(),
            title: title.into(),
            body,
            conversation_id: conversation_id.to_string(),
            task_id: task_id.to_string(),
        }
    }

    /// JSON body posted to a webhook.
    pub fn webhook_payload(&self) -> Value {
        json!({
            "source": "glass-marble",
            "event": self.event,
            "title": self.title,
            "body": self.body,
            "conversationId": self.conversation_id,
            "taskId": self.task_id,
            "at": now_millis(),
        })
    }

    /// APNs payload, including a small `marble` object the app can deep-link.
    pub fn apns_payload(&self) -> Value {
        json!({
            "aps": {
                "alert": { "title": self.title, "body": self.body },
                "sound": "default",
                "thread-id": "glass-marble",
            },
            "marble": {
                "event": self.event,
                "conversationId": self.conversation_id,
                "taskId": self.task_id,
            }
        })
    }
}

/// Outcome of one delivery attempt, useful for logs and the test route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub channel: String,
    pub ok: bool,
    pub detail: String,
}

impl Delivery {
    fn ok(channel: &str, detail: impl Into<String>) -> Self {
        Self {
            channel: channel.to_string(),
            ok: true,
            detail: detail.into(),
        }
    }

    fn failed(channel: &str, detail: impl Into<String>) -> Self {
        Self {
            channel: channel.to_string(),
            ok: false,
            detail: detail.into(),
        }
    }
}

/// Deliver one notification to every configured channel.
pub fn deliver(
    settings: &NotificationSettings,
    devices: &[DeviceRegistration],
    notification: &Notification,
) -> Vec<Delivery> {
    if !settings.wants(&notification.event) {
        return Vec::new();
    }
    let mut reports = Vec::new();
    let url = settings.webhook_url.trim();
    if !url.is_empty() {
        let token = settings.webhook_token.trim();
        let body = notification.webhook_payload().to_string();
        reports.push(match post_json(url, token, &body) {
            Ok(code) if (200..300).contains(&code) => Delivery::ok("webhook", format!("{code}")),
            Ok(code) => Delivery::failed("webhook", format!("status {code}")),
            Err(error) => Delivery::failed("webhook", error),
        });
    }
    if let Some(apns) = settings.apns.as_ref().filter(|apns| apns.is_configured()) {
        for device in devices {
            if !device.token.is_empty() {
                reports.push(push_device(apns, device, notification));
            }
        }
    }
    reports
}

/// Deliver off the caller's thread so a slow receiver never delays a turn.
pub fn spawn_delivery(
    settings: NotificationSettings,
    devices: Vec<DeviceRegistration>,
    notification: Notification,
) {
    if !settings.wants(&notification.event) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("glass-marble-notify".into())
        .spawn(move || {
            for report in deliver(&settings, &devices, &notification) {
                if report.ok {
                    tracing::info!(channel = %report.channel, detail = %report.detail, "notification delivered");
                } else {
                    tracing::warn!(channel = %report.channel, detail = %report.detail, "notification failed");
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the notification worker");
    }
}

/// Watch the product event bus for the one thing a turn worker cannot see: a
/// governed tool call that is now waiting for a human. An always-on assistant
/// should reach the person instead of quietly timing out.
pub fn spawn_event_watcher(events: EventBus, state: Arc<Mutex<ProductState>>) {
    let receiver = events.subscribe();
    let spawned = std::thread::Builder::new()
        .name("glass-marble-alerts".into())
        .spawn(move || {
            while let Ok(event) = receiver.recv() {
                if event.kind != "approval_pending" {
                    continue;
                }
                let (settings, devices) = {
                    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
                    if !guard.notifications.wants("approval") {
                        continue;
                    }
                    (guard.notifications.clone(), guard.devices.clone())
                };
                let tool = event
                    .data
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("a governed tool");
                let arguments = event.data.get("arguments").cloned().unwrap_or(Value::Null);
                spawn_delivery(
                    settings,
                    devices,
                    Notification::new(
                        "approval",
                        "Marble needs your approval",
                        format!("{tool} is waiting for your decision: {arguments}"),
                        &event.conversation_id,
                        &event.task_id,
                    ),
                );
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start the notification watcher");
    }
}

/// Register or refresh one device token.
pub fn upsert_device(
    devices: &mut Vec<DeviceRegistration>,
    token: &str,
    platform: &str,
    topic: &str,
) -> Result<DeviceRegistration, String> {
    let token = token.trim().to_ascii_lowercase();
    if token.len() < 32
        || token.len() > 200
        || !token.chars().all(|digit| digit.is_ascii_hexdigit())
    {
        return Err("a device token is 32 to 200 hexadecimal characters".into());
    }
    let now = now_millis();
    if let Some(existing) = devices.iter_mut().find(|device| device.token == token) {
        existing.last_seen_at = now;
        if !platform.trim().is_empty() {
            existing.platform = platform.trim().to_string();
        }
        if !topic.trim().is_empty() {
            existing.topic = topic.trim().to_string();
        }
        return Ok(existing.clone());
    }
    if devices.len() >= DEVICE_LIMIT {
        return Err(format!(
            "marble registers at most {DEVICE_LIMIT} devices; remove one first"
        ));
    }
    let device = DeviceRegistration {
        token,
        platform: if platform.trim().is_empty() {
            "ios".to_string()
        } else {
            platform.trim().to_string()
        },
        topic: topic.trim().to_string(),
        created_at: now,
        last_seen_at: now,
    };
    devices.push(device.clone());
    Ok(device)
}

/// POST one JSON body. `http://` is spoken directly so a tailnet receiver needs
/// no TLS at all; `https://` goes through `curl`, which every host has.
fn post_json(url: &str, token: &str, body: &str) -> Result<u16, String> {
    if let Some(rest) = url.strip_prefix("http://") {
        return post_plain_http(rest, token, body);
    }
    if url.starts_with("https://") {
        return post_with_curl(url, token, body);
    }
    Err("the webhook URL must start with http:// or https://".into())
}

fn post_plain_http(rest: &str, token: &str, body: &str) -> Result<u16, String> {
    let (authority, path) = match rest.split_once('/') {
        Some((authority, tail)) => (authority, format!("/{tail}")),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() {
        return Err("the webhook URL has no host".into());
    }
    let address = authority
        .to_socket_addrs()
        .map_err(|error| format!("cannot resolve {authority}: {error}"))?
        .next()
        .ok_or_else(|| format!("cannot resolve {authority}"))?;
    let mut stream = TcpStream::connect_timeout(&address, DELIVERY_TIMEOUT)
        .map_err(|error| format!("cannot reach {authority}: {error}"))?;
    let _ = stream.set_read_timeout(Some(DELIVERY_TIMEOUT));
    let _ = stream.set_write_timeout(Some(DELIVERY_TIMEOUT));
    let mut request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\nUser-Agent: glass-marble\r\n",
        body.len()
    );
    if !token.is_empty() {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("cannot write to {authority}: {error}"))?;
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    parse_status(&response)
}

fn post_with_curl(url: &str, token: &str, body: &str) -> Result<u16, String> {
    let file = temp_payload(body)?;
    let mut command = Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--max-time",
        "10",
        "--output",
        "/dev/null",
        "--write-out",
        "%{http_code}",
        "--request",
        "POST",
        "--header",
        "content-type: application/json",
        "--data-binary",
        &format!("@{file}"),
    ]);
    if !token.is_empty() {
        command.args(["--header", &format!("authorization: Bearer {token}")]);
    }
    command.arg(url);
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("curl is required for https webhooks: {error}"));
    let _ = std::fs::remove_file(&file);
    let output = output?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let code = text.parse::<u16>().unwrap_or(0);
    if code == 0 {
        return Err(format!(
            "curl could not deliver: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(code)
}

fn push_device(
    apns: &ApnsSettings,
    device: &DeviceRegistration,
    notification: &Notification,
) -> Delivery {
    let jwt = match apns_jwt(apns, now_millis() / 1000) {
        Ok(jwt) => jwt,
        Err(error) => return Delivery::failed("apns", error),
    };
    let payload = notification.apns_payload().to_string();
    let file = match temp_payload(&payload) {
        Ok(file) => file,
        Err(error) => return Delivery::failed("apns", error),
    };
    let topic = if device.topic.trim().is_empty() {
        apns.topic.trim()
    } else {
        device.topic.trim()
    };
    let url = format!("{}{}", apns.host(), device.token);
    let output = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            "10",
            "--http2",
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code}",
            "--request",
            "POST",
            "--header",
            "content-type: application/json",
            "--header",
            &format!("authorization: bearer {jwt}"),
            "--header",
            &format!("apns-topic: {topic}"),
            "--header",
            "apns-push-type: alert",
            "--header",
            "apns-priority: 10",
            "--data-binary",
            &format!("@{file}"),
            &url,
        ])
        .stdin(Stdio::null())
        .output();
    let _ = std::fs::remove_file(&file);
    match output {
        Ok(output) => {
            let code = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u16>()
                .unwrap_or(0);
            if code == 200 {
                Delivery::ok(
                    "apns",
                    format!("{}…", &device.token[..8.min(device.token.len())]),
                )
            } else {
                Delivery::failed(
                    "apns",
                    format!(
                        "status {code}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    ),
                )
            }
        }
        Err(error) => Delivery::failed("apns", format!("curl is required for APNs: {error}")),
    }
}

fn temp_payload(body: &str) -> Result<String, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "glass-marble-notify-{}-{sequence}.json",
        std::process::id()
    ));
    std::fs::write(&path, body).map_err(|error| format!("cannot stage payload: {error}"))?;
    Ok(path.display().to_string())
}

fn parse_status(response: &str) -> Result<u16, String> {
    let line = response
        .lines()
        .next()
        .ok_or_else(|| "the receiver closed without answering".to_string())?;
    line.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| format!("unexpected response line: {line}"))
}

/// Build the short-lived ES256 token APNs expects.
fn apns_jwt(apns: &ApnsSettings, issued_at: u64) -> Result<String, String> {
    let header = json!({"alg": "ES256", "kid": apns.key_id.trim()});
    let claims = json!({"iss": apns.team_id.trim(), "iat": issued_at});
    let signing_input = format!(
        "{}.{}",
        base64url(header.to_string().as_bytes()),
        base64url(claims.to_string().as_bytes())
    );
    let der = sign_es256(apns.key_file.trim(), &signing_input)?;
    let raw = der_to_raw_signature(&der)
        .ok_or_else(|| "openssl returned a signature that is not ES256".to_string())?;
    Ok(format!("{signing_input}.{}", base64url(&raw)))
}

fn sign_es256(key_file: &str, signing_input: &str) -> Result<Vec<u8>, String> {
    let mut child = Command::new("openssl")
        .args(["dgst", "-sha256", "-sign", key_file])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("openssl is required for APNs: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(signing_input.as_bytes())
            .map_err(|error| format!("cannot sign the APNs token: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("cannot sign the APNs token: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "openssl could not sign with {}: {}",
            key_file,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

/// Convert a DER-encoded ECDSA signature into the 64-byte `r || s` JOSE form.
fn der_to_raw_signature(der: &[u8]) -> Option<Vec<u8>> {
    let (tag, rest) = der.split_first()?;
    if *tag != 0x30 {
        return None;
    }
    let (_, rest) = der_length(rest)?;
    let (tag_r, rest) = rest.split_first()?;
    if *tag_r != 0x02 {
        return None;
    }
    let (len_r, rest) = der_length(rest)?;
    let mut raw = vec![0u8; 64];
    let r = take(rest, len_r)?;
    let (tag_s, rest) = rest.get(len_r..)?.split_first()?;
    if *tag_s != 0x02 {
        return None;
    }
    let (len_s, rest_s) = der_length(rest)?;
    let s = take(rest_s, len_s)?;
    write_scalar(r, &mut raw[..32])?;
    write_scalar(s, &mut raw[32..])?;
    Some(raw)
}

fn der_length(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let (first, rest) = bytes.split_first()?;
    if *first & 0x80 == 0 {
        return Some((*first as usize, rest));
    }
    let count = (*first & 0x7f) as usize;
    if count == 0 || count > 2 {
        return None;
    }
    let mut length = 0usize;
    for byte in rest.get(..count)? {
        length = (length << 8) | *byte as usize;
    }
    Some((length, rest.get(count..)?))
}

fn take(bytes: &[u8], length: usize) -> Option<&[u8]> {
    bytes.get(..length)
}

fn write_scalar(component: &[u8], target: &mut [u8]) -> Option<()> {
    let mut trimmed = component;
    while trimmed.first() == Some(&0) {
        trimmed = &trimmed[1..];
    }
    if trimmed.len() > target.len() {
        return None;
    }
    let offset = target.len() - trimmed.len();
    target[offset..].copy_from_slice(trimmed);
    Some(())
}

/// URL-safe base64 without padding, as JOSE requires.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            encoded.push(ALPHABET[(third & 0b111111) as usize] as char);
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_the_jose_encoding() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        assert_eq!(base64url(b"{\"alg\":\"ES256\"}"), "eyJhbGciOiJFUzI1NiJ9");
    }

    #[test]
    fn der_signatures_become_the_64_byte_jose_form() {
        // A real ES256 signature shape: SEQUENCE(INTEGER r, INTEGER s).
        let mut der = vec![0x30, 0x46, 0x02, 0x21, 0x00];
        der.extend(std::iter::repeat_n(0x11u8, 32));
        der.extend([0x02, 0x21, 0x00]);
        der.extend(std::iter::repeat_n(0x22u8, 32));
        let raw = der_to_raw_signature(&der).expect("a valid signature");
        assert_eq!(raw.len(), 64);
        assert!(raw[..32].iter().all(|byte| *byte == 0x11));
        assert!(raw[32..].iter().all(|byte| *byte == 0x22));
        assert_eq!(der_to_raw_signature(b"nonsense"), None);
    }

    #[test]
    fn notifications_are_only_sent_for_subscribed_events() {
        let settings = NotificationSettings::default();
        assert!(!settings.wants("approval"), "no channel is configured yet");
        let configured = NotificationSettings {
            webhook_url: "http://127.0.0.1:9/hook".into(),
            events: vec!["approval".into()],
            ..NotificationSettings::default()
        };
        assert!(configured.wants("approval"));
        assert!(!configured.wants("settled"));
        assert_eq!(
            deliver(
                &configured,
                &[],
                &Notification::new("settled", "done", "done", "c1", "t1")
            )
            .len(),
            0
        );
    }

    #[test]
    fn payloads_carry_enough_to_deep_link() {
        let notification =
            Notification::new("approval", "Marble needs you", "read notes.md", "c1", "t1");
        let webhook = notification.webhook_payload();
        assert_eq!(webhook["event"], json!("approval"));
        assert_eq!(webhook["conversationId"], json!("c1"));
        let apns = notification.apns_payload();
        assert_eq!(apns["aps"]["alert"]["title"], json!("Marble needs you"));
        assert_eq!(apns["marble"]["taskId"], json!("t1"));
    }

    #[test]
    fn device_tokens_are_validated_and_refreshed_in_place() {
        let mut devices = Vec::new();
        let token = "a".repeat(64);
        let first = upsert_device(&mut devices, &token, "ios", "dev.glass.marble").unwrap();
        assert_eq!(first.topic, "dev.glass.marble");
        let second = upsert_device(&mut devices, &token.to_uppercase(), "ios", "").unwrap();
        assert_eq!(second.created_at, first.created_at);
        assert_eq!(devices.len(), 1);
        assert!(upsert_device(&mut devices, "not-a-token", "ios", "").is_err());
    }

    #[test]
    fn a_plain_http_webhook_receives_the_notification() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let receiver = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 2048];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let _ = stream.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
            request
        });
        let settings = NotificationSettings {
            webhook_url: format!("http://127.0.0.1:{port}/marble"),
            webhook_token: "t0ken".into(),
            ..NotificationSettings::default()
        };
        let reports = deliver(
            &settings,
            &[],
            &Notification::new(
                "settled",
                "Marble finished",
                "the notes are tidy",
                "c1",
                "t1",
            ),
        );
        assert_eq!(reports.len(), 1);
        assert!(reports[0].ok, "{:?}", reports[0]);
        let request = receiver.join().unwrap();
        assert!(request.starts_with("POST /marble HTTP/1.1"), "{request}");
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer t0ken"),
            "{request}"
        );
        assert!(request.contains("the notes are tidy"), "{request}");
    }

    #[test]
    fn legacy_settings_take_every_event_by_default() {
        let settings: NotificationSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.events, default_events());
        assert!(settings.apns.is_none());
        assert_eq!(settings.document()["webhookConfigured"], json!(false));
    }
}
