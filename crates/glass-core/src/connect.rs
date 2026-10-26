//! Opt-in browser gateway. Normal GC startup never starts listeners or workers.
//! The Unix session owner executes tools; this process only projects and routes.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    os::unix::{fs::OpenOptionsExt, net::UnixStream, process::CommandExt},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
const MAX_BODY: usize = 65536;
const MAX_EVENTS: usize = 1024;
const MAX_EVENT_BYTES: usize = 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    port: u16,
    https: u16,
    #[serde(default)]
    http: u16,
    #[serde(default)]
    http_host: String,
    host: String,
    login: String,
    token: String,
    pid: u32,
    stamp: String,
    local: bool,
    workspace: PathBuf,
    assets: PathBuf,
    #[serde(default)]
    security: Value,
}
/// Carry explicit launch restrictions into detached workers; administrator rules
/// are applied afterward by main. This contains policy names, never credentials.
pub fn inherit_security(cfg: &mut crate::config::Config, args: &[String]) -> Result<(), String> {
    if !(args.first().is_some_and(|s| s == "serve")
        || (args.first().is_some_and(|s| s == "connect")
            && args.get(1).is_some_and(|s| s == "_serve")))
    {
        return Ok(());
    }
    let Ok(raw) = std::env::var("GC_CONNECT_SECURITY") else {
        return Ok(());
    };
    if raw.len() > 16384 {
        return Err("Inherited connect policy exceeds bounds".into());
    }
    let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if value.is_null() {
        return Ok(());
    }
    cfg.permission_profile = crate::sandbox::PermissionProfile::parse(
        value["profile"]
            .as_str()
            .ok_or("Missing inherited profile")?,
    )
    .ok_or("Invalid inherited profile")?;
    cfg.sandbox = crate::sandbox::Policy::parse(
        value["sandbox"]
            .as_str()
            .ok_or("Missing inherited sandbox")?,
    )
    .ok_or("Invalid inherited sandbox")?;
    cfg.sandbox_ports =
        serde_json::from_value(value["ports"].clone()).map_err(|e| e.to_string())?;
    cfg.sandbox_env = value["env"]
        .as_str()
        .ok_or("Missing inherited environment policy")?
        .into();
    cfg.extension_isolation = value["extensionIsolation"]
        .as_str()
        .ok_or("Missing inherited extension policy")?
        .into();
    cfg.sandbox_egress = value["egress"]
        .as_str()
        .ok_or("Missing inherited egress policy")?
        .into();
    cfg.egress_allow = serde_json::from_value(value["allow"].clone()).map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn security(cfg: &crate::config::Config) -> Value {
    json!({"profile":match cfg.permission_profile{crate::sandbox::PermissionProfile::Unrestricted=>"unrestricted",crate::sandbox::PermissionProfile::WorkspaceEdit=>"workspace-edit",crate::sandbox::PermissionProfile::ReadOnly=>"read-only"},"sandbox":match cfg.sandbox{crate::sandbox::Policy::Off=>"off",crate::sandbox::Policy::Workspace=>"workspace",crate::sandbox::Policy::Strict=>"strict"},"ports":cfg.sandbox_ports,"env":cfg.sandbox_env,"extensionIsolation":cfg.extension_isolation,"egress":cfg.sandbox_egress,"allow":cfg.egress_allow})
}

fn state_path() -> PathBuf {
    crate::config::data_dir().join("connect.json")
}
fn load() -> Result<Settings, String> {
    serde_json::from_slice(&crate::requests::bounded(&state_path(), 16384)?)
        .map_err(|e| e.to_string())
}
fn load_status() -> Result<Option<Settings>, String> {
    parse_status_settings(crate::requests::bounded_optional(&state_path(), 16384)?)
}
fn parse_status_settings(bytes: Option<Vec<u8>>) -> Result<Option<Settings>, String> {
    bytes
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
        .transpose()
}
fn status_output(settings: Option<&Settings>, product: &str) -> String {
    match settings {
        Some(settings) => format!(
            "{product} · {}\n{}",
            if alive(settings) {
                "running"
            } else {
                "stopped"
            },
            web_url(settings).split('#').next().unwrap()
        ),
        None => format!("{product} · stopped"),
    }
}
fn save(settings: &Settings) -> Result<(), String> {
    crate::auth::secure_write(
        &state_path(),
        &serde_json::to_vec(settings).map_err(|e| e.to_string())?,
    )
}
fn alive(settings: &Settings) -> bool {
    crate::navigation::process_stamp(settings.pid).as_deref() == Some(settings.stamp.as_str())
        && !settings.stamp.is_empty()
}
fn cli(args: &[&str]) -> Result<Value, String> {
    let out = Command::new("tailscale")
        .args(args)
        .output()
        .map_err(|e| format!("Tailscale is required: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr)
            .chars()
            .take(1000)
            .collect());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("Invalid Tailscale response: {e}"))
}
// Tailscale's HTTP router appends the tailnet DNS suffix even to IP Hosts.
// Add just that alias through its local API, with CAS to protect other routes.
fn tailscale_api(method: &str, body: &[u8], etag: &str) -> Result<(u16, String, Vec<u8>), String> {
    let path = std::env::var_os("GC_TAILSCALE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/run/tailscale/tailscaled.sock"));
    let mut stream = UnixStream::connect(path).map_err(|e| format!("Tailscale local API: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    if !etag
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'"' || b == b'-')
    {
        return Err("Invalid Tailscale configuration version".into());
    }
    write!(stream, "{method} /localapi/v0/serve-config HTTP/1.0\r\nHost: local-tailscaled.sock\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nIf-Match: {etag}\r\n\r\n", body.len()).map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    stream
        .take(1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 1024 * 1024 {
        return Err("Tailscale configuration exceeds bounds".into());
    }
    let split = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("Invalid Tailscale API response")?;
    let headers = std::str::from_utf8(&data[..split]).map_err(|e| e.to_string())?;
    let status = headers
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or("Invalid Tailscale API status")?;
    let etag = headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(k, _)| k.eq_ignore_ascii_case("etag"))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();
    Ok((status, etag, data[split + 4..].to_vec()))
}
fn http_alias(settings: &Settings, add: bool) -> Result<(), String> {
    if settings.http == 0 {
        return Ok(());
    }
    let suffix = settings
        .host
        .split_once('.')
        .ok_or("Invalid Tailscale hostname")?
        .1;
    let key = format!("{}.{}:{}", settings.http_host, suffix, settings.http);
    for _ in 0..3 {
        let (status, etag, body) = tailscale_api("GET", b"", "")?;
        if status != 200 || etag.is_empty() {
            return Err(format!(
                "Cannot read Tailscale route version: HTTP {status}"
            ));
        }
        let mut config: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let web = config["Web"]
            .as_object_mut()
            .ok_or("Tailscale web routes unavailable")?;
        if add {
            let source = format!("{}:{}", settings.host, settings.http);
            let handler = web
                .get(&source)
                .filter(|h| h["Handlers"]["/"]["Proxy"] == route(settings))
                .cloned()
                .ok_or("GC HTTP route changed during setup")?;
            if web.get(&key).is_some_and(|h| h != &handler) {
                return Err("HTTP IP alias belongs to another service".into());
            }
            web.insert(key.clone(), handler);
        } else if web
            .get(&key)
            .is_some_and(|h| h["Handlers"]["/"]["Proxy"] == route(settings))
        {
            web.remove(&key);
        } else {
            return Ok(());
        }
        let encoded = serde_json::to_vec(&config).map_err(|e| e.to_string())?;
        let (status, _, _) = tailscale_api("POST", &encoded, &etag)?;
        if status == 200 {
            return Ok(());
        }
        if status != 412 {
            return Err(format!("Cannot update GC HTTP IP route: HTTP {status}"));
        }
    }
    Err(format!(
        "Tailscale routes changed concurrently; retry {}",
        crate::cli::connect_command()
    ))
}
fn port_available(port: u16) -> bool {
    match TcpListener::bind(("0.0.0.0", port)) {
        Ok(listener) => drop(listener),
        Err(_) => return false,
    }
    match TcpListener::bind(("::", port)) {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(error) => matches!(
            error.kind(),
            std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::Unsupported
        ),
    }
}
fn route(settings: &Settings) -> String {
    format!("http://127.0.0.1:{}", settings.port)
}
fn web_url(settings: &Settings) -> String {
    if settings.http > 0 {
        return format!("http://{}:{}", settings.http_host, settings.http);
    }
    https_url(settings)
}
fn https_url(settings: &Settings) -> String {
    if settings.local {
        format!("http://127.0.0.1:{}/#{}", settings.port, settings.token)
    } else {
        format!(
            "https://{}{}",
            settings.host,
            if settings.https == 443 {
                String::new()
            } else {
                format!(":{}", settings.https)
            }
        )
    }
}
fn assets() -> Result<PathBuf, String> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("GC_WEB_ASSETS") {
        paths.push(PathBuf::from(path));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        paths.push(parent.join("../share/glass-core/web"));
    }
    paths
        .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../products/glass-core/web/dist"));
    paths.into_iter().find(|p|p.join("app.js").is_file()&&p.join("index.html").is_file()).and_then(|p|p.canonicalize().ok()).ok_or("Web assets are missing. Install the GC web asset bundle, or build it with npm run build:web. Set GC_WEB_ASSETS to its directory.".into())
}
pub fn command(cfg: crate::config::Config, args: &[String]) -> Result<(), String> {
    let lock_path = crate::config::data_dir().join("connect.lock");
    if !args.first().is_some_and(|s| s == "_serve") {
        std::fs::create_dir_all(crate::config::data_dir()).map_err(|e| e.to_string())?;
        if std::fs::symlink_metadata(&lock_path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Connect lock must not be a symlink".into());
        }
    }
    let _lock = if args.first().is_some_and(|s| s == "_serve") {
        None
    } else {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&file)
            .map_err(|_| "Another gc connect command is running")?;
        Some(file)
    };
    match args.first().map(String::as_str) {
        Some("_serve") => return gateway(cfg, load()?),
        Some("status") => {
            crate::cli_println!(
                "{}",
                status_output(load_status()?.as_ref(), crate::cli::connect_product_name())
            );
            return Ok(());
        }
        Some("stop") => {
            let s = load()?;
            if !s.local {
                http_alias(&s, false)?;
                let status = cli(&["serve", "status", "--json"])?;
                for (protocol, port) in [("https", s.https), ("http", s.http)] {
                    if port == 0 {
                        continue;
                    }
                    let key = format!("{}:{}", s.host, port);
                    if status["Web"][&key]["Handlers"]["/"]["Proxy"] == route(&s) {
                        let out = Command::new("tailscale")
                            .args(["serve", &format!("--{protocol}={port}"), "off"])
                            .output()
                            .map_err(|e| e.to_string())?;
                        if !out.status.success() {
                            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
                        }
                    }
                }
            }
            if alive(&s) {
                unsafe {
                    libc::kill(s.pid as i32, libc::SIGTERM);
                }
            }
            let _ = std::fs::remove_file(state_path());
            crate::cli_println!(
                "{} · stopped\nSession workers and running tasks are preserved.",
                crate::cli::connect_product_name()
            );
            return Ok(());
        }
        None | Some("--local" | "--http") => {}
        _ => {
            return Err(format!(
                "usage: {} [--local | --http] | {} status | {} stop",
                crate::cli::connect_command(),
                crate::cli::connect_command(),
                crate::cli::connect_command()
            ));
        }
    }
    if args.len() > 1 {
        return Err("Unexpected connect arguments".into());
    }
    if let Ok(s) = load()
        && alive(&s)
    {
        if args.first().is_some_and(|a| a == "--http") && s.http == 0 {
            return Err(format!(
                "Remote access is already running. Run {} stop, then {} --http. Served tasks continue.",
                crate::cli::connect_command(),
                crate::cli::connect_command()
            ));
        }
        crate::cli_println!(
            "{} · ready\n{}",
            crate::cli::connect_product_name(),
            web_url(&s)
        );
        return Ok(());
    }
    let local = args.first().is_some_and(|s| s == "--local");
    let (host, login, https) = if local {
        ("127.0.0.1".into(), String::new(), 0)
    } else {
        let status = cli(&["status", "--json"])?;
        if status["BackendState"] != "Running" {
            return Err("Tailscale is not connected. Run tailscale up first.".into());
        }
        let host = status["Self"]["DNSName"]
            .as_str()
            .ok_or("Tailscale DNS name is unavailable")?
            .trim_end_matches('.')
            .to_string();
        let user = status["Self"]["UserID"]
            .as_u64()
            .ok_or("Tailscale user identity is unavailable on this node; use a user-owned node")?
            .to_string();
        let login = status["User"][&user]["LoginName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Cannot resolve this node's Tailscale user")?
            .to_string();
        let routes = cli(&["serve", "status", "--json"])?;
        let previous = load().ok().filter(|s| {
            !s.local
                && s.host == host
                && routes["Web"][format!("{}:{}", s.host, s.https)]["Handlers"]["/"]["Proxy"]
                    == route(s)
        });
        let https = previous
            .map(|s| s.https)
            .or_else(|| {
                [443, 8443, 8444, 8445]
                    .into_iter()
                    .find(|port| routes["TCP"][port.to_string()].is_null() && port_available(*port))
            })
            .or_else(|| args.first().is_some_and(|a| a == "--http").then_some(0))
            .ok_or("No free GC HTTPS port; existing Tailscale routes were preserved")?;
        (host, login, https)
    };
    let (http, http_host) = if args.first().is_some_and(|a| a == "--http") {
        let status = cli(&["status", "--json"])?;
        let ip = status["TailscaleIPs"]
            .as_array()
            .and_then(|ips| {
                ips.iter()
                    .filter_map(Value::as_str)
                    .find(|ip| ip.parse::<std::net::Ipv4Addr>().is_ok())
            })
            .ok_or("Tailscale IPv4 address is unavailable")?
            .to_string();
        let routes = cli(&["serve", "status", "--json"])?;
        let http = [8080, 8081, 8082, 8083]
            .into_iter()
            .find(|port| routes["TCP"][port.to_string()].is_null() && port_available(*port))
            .ok_or("No free GC HTTP port; existing routes were preserved")?;
        (http, ip)
    } else {
        (0, String::new())
    };
    let port = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| e.to_string())?
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    let mut random = [0u8; 32];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random)
        .map_err(|_| "Secure random unavailable")?;
    let mut settings = Settings {
        port,
        https,
        http,
        http_host,
        host,
        login,
        token: random.iter().map(|b| format!("{b:02x}")).collect(),
        pid: 0,
        stamp: String::new(),
        local,
        workspace: cfg.workspace.clone(),
        assets: assets()?,
        security: security(&cfg),
    };
    save(&settings)?;
    let log_path = crate::config::data_dir().join("connect.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(crate::executable::path()?);
    command
        .args(["connect", "_serve"])
        .env("GC_CONNECT_SECURITY", settings.security.to_string())
        .current_dir(&cfg.workspace)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    settings.pid = child.id();
    settings.stamp =
        crate::navigation::process_stamp(settings.pid).ok_or("Gateway process did not start")?;
    save(&settings)?;
    let start = Instant::now();
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("Gateway startup failed; see gc data directory/connect.log".into());
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        if start.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            return Err("Gateway did not become ready".into());
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    if !local {
        for (protocol, port) in [("https", https), ("http", http)] {
            if port == 0 {
                continue;
            }
            let out = Command::new("tailscale")
                .args([
                    "serve",
                    "--bg",
                    &format!("--{protocol}={port}"),
                    &route(&settings),
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                if protocol == "https" && http > 0 {
                    crate::cli_println!("HTTPS setup unavailable; continuing with tailnet HTTP.");
                    continue;
                }
                // Keep the verified HTTPS gateway available if optional HTTP setup fails.
                return Err(format!(
                    "Tailscale {protocol} Serve could not start: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
    }
    http_alias(&settings, true)?;
    crate::cli_println!(
        "{} · ready\n{}\n\nOpen in Safari → Share → Add to Home Screen.\nSame threads · tasks continue when you disconnect.\n{} status · {} stop",
        crate::cli::connect_product_name(),
        web_url(&settings),
        crate::cli::connect_command(),
        crate::cli::connect_command()
    );
    Ok(())
}
struct Feed {
    seq: u64,
    bytes: usize,
    events: VecDeque<(u64, Value, usize)>,
    snapshot: Value,
    active: bool,
    interaction: Option<Value>,
    connected: bool,
    live: VecDeque<Value>,
    live_bytes: usize,
    live_clipped: bool,
}
struct Bridge {
    epoch: u64,
    writer: Mutex<UnixStream>,
    feed: Mutex<Feed>,
    changed: Condvar,
}
impl Bridge {
    fn send(&self, value: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BODY {
            return Err("Input exceeds 64 KiB".into());
        }
        let mut stream = self.writer.lock().unwrap();
        stream
            .write_all(&bytes)
            .and_then(|_| stream.write_all(b"\n"))
            .map_err(|e| e.to_string())
    }
    fn push(&self, value: Value) {
        let size = value.to_string().len();
        let mut f = self.feed.lock().unwrap();
        f.seq += 1;
        let seq = f.seq;
        let kind = value["t"].as_str().unwrap_or("");
        if matches!(
            kind,
            "turn_start" | "waiting" | "tool" | "tool_end" | "done"
        ) {
            f.live.clear();
            f.live_bytes = 0;
            f.live_clipped = false;
        }
        if matches!(kind, "text" | "reasoning" | "tool") {
            let delta = value["delta"].as_str().unwrap_or("");
            let merged = if matches!(kind, "text" | "reasoning") {
                if let Some(last) = f.live.back_mut() {
                    if last["t"] == kind {
                        if let Value::String(text) = &mut last["delta"] {
                            text.push_str(delta);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };
            if merged {
                f.live_bytes += delta.len();
            } else {
                f.live.push_back(value.clone());
                f.live_bytes += size;
            }
            while f.live_bytes > 512 * 1024 || f.live.len() > 256 {
                if let Some(old) = f.live.pop_front() {
                    f.live_bytes = f.live_bytes.saturating_sub(old.to_string().len());
                    f.live_clipped = true;
                } else {
                    break;
                }
            }
        }
        match value["t"].as_str() {
            Some("replay") => f.snapshot = value.clone(),
            Some("command_catalogue") => {
                f.snapshot["commands"] = value["commands"].clone();
            }
            Some("control") => {
                f.snapshot["can_write"] = value["can_write"].clone();
            }
            Some("command_start" | "turn_start" | "waiting" | "text" | "tool" | "reasoning") => {
                f.active = true
            }
            Some("error") if value["settled"] == true => {
                f.active = false;
                f.interaction = None;
            }
            Some("command_done" | "done") => {
                f.active = false;
                f.interaction = None;
            }
            Some("interaction") => f.interaction = Some(value.clone()),
            Some("interaction_done") => f.interaction = None,
            _ => {}
        }
        while !f.events.is_empty()
            && (f.events.len() >= MAX_EVENTS || f.bytes + size > MAX_EVENT_BYTES)
        {
            let (_, _, old) = f.events.pop_front().unwrap();
            f.bytes -= old;
        }
        f.bytes += size;
        f.events.push_back((seq, value, size));
        self.changed.notify_all();
    }
}
struct App {
    settings: Settings,
    cfg: crate::config::Config,
    bridges: Mutex<HashMap<String, Arc<Bridge>>>,
    clients: AtomicUsize,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() < 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn thread_path(id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Invalid thread ID".into());
    }
    let root = crate::config::data_dir()
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = root
        .join(format!("{id}.jsonl"))
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if path.parent() != Some(root.as_path()) {
        return Err("Thread path escapes storage".into());
    }
    Ok(path)
}
fn thread_workspace(id: &str) -> Result<PathBuf, String> {
    thread_path(id)?;
    let meta = crate::requests::bounded(
        &crate::config::data_dir().join(format!("{id}.meta.json")),
        4 * 1024 * 1024,
    )?;
    let meta: crate::polish::Metadata = serde_json::from_slice(&meta).map_err(|e| e.to_string())?;
    meta.workspace.canonicalize().map_err(|e| e.to_string())
}
fn bridge(app: &App, id: &str) -> Result<Arc<Bridge>, String> {
    let mut map = app.bridges.lock().unwrap();
    if let Some(b) = map.get(id) {
        if b.feed.lock().unwrap().connected {
            return Ok(b.clone());
        }
        map.remove(id);
    }
    if map.len() >= 16
        && let Some(key) = map.iter().find_map(|(key, b)| {
            let f = b.feed.lock().unwrap();
            (!f.active && f.interaction.is_none() && Arc::strong_count(b) <= 2).then(|| key.clone())
        })
        && let Some(b) = map.remove(&key)
    {
        let _ = b.writer.lock().unwrap().shutdown(std::net::Shutdown::Both);
    }
    if map.len() >= 16 {
        return Err(
            "16 active threads are connected; restart the gateway to release idle connections"
                .into(),
        );
    }
    let workspace = thread_workspace(id)?;
    let socket = crate::config::data_dir().join(format!("serve-{id}.sock"));
    let stream = match UnixStream::connect(&socket) {
        Ok(s) => s,
        Err(_) => {
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(crate::config::data_dir().join(format!("serve-{id}.log")))
                .map_err(|e| e.to_string())?;
            let mut command = Command::new(crate::executable::path()?);
            command
                .args(["serve", id])
                .env("GC_CONNECT_SECURITY", app.settings.security.to_string())
                .current_dir(workspace)
                .stdin(Stdio::null())
                .stdout(log.try_clone().map_err(|e| e.to_string())?)
                .stderr(log);
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            let start = Instant::now();
            loop {
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    return Err("Thread is open in another GC process, or its worker failed. Close that TUI first, then retry; GC never steals a writer lock.".into());
                }
                if let Ok(stream) = UnixStream::connect(&socket) {
                    break stream;
                }
                if start.elapsed() > Duration::from_secs(5) {
                    return Err("Session worker startup timed out; inspect gc logs".into());
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        }
    };
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let reader = stream.try_clone().map_err(|e| e.to_string())?;
    static EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let b = Arc::new(Bridge {
        epoch: (u64::from(std::process::id()) << 32) | EPOCH.fetch_add(1, Ordering::Relaxed),
        writer: Mutex::new(stream),
        feed: Mutex::new(Feed {
            seq: 0,
            bytes: 0,
            events: VecDeque::new(),
            snapshot: Value::Null,
            active: false,
            interaction: None,
            connected: true,
            live: VecDeque::new(),
            live_bytes: 0,
            live_clipped: false,
        }),
        changed: Condvar::new(),
    });
    b.send(&json!({"t":"attach","protocol_version":1,"human":true,"observe":true}))?;
    let worker = b.clone();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        loop {
            let mut line = Vec::new();
            match reader
                .by_ref()
                .take(1024 * 1024 + 1)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if line.len() > 1024 * 1024 || !line.ends_with(b"\n") {
                        break;
                    }
                    if let Ok(value) = serde_json::from_slice(&line) {
                        worker.push(value);
                    }
                }
            }
        }
        worker.push(json!({"t":"disconnected","message":"Session connection closed; reconnect to inspect retained history"}));
        worker.feed.lock().unwrap().connected = false;
        worker.changed.notify_all();
    });
    let mut feed = b.feed.lock().unwrap();
    let started = Instant::now();
    while feed.snapshot.is_null() && feed.connected && started.elapsed() < Duration::from_secs(5) {
        feed = b
            .changed
            .wait_timeout(feed, Duration::from_millis(100))
            .unwrap()
            .0;
    }
    if feed.snapshot.is_null() || feed.snapshot["security"] != app.settings.security {
        drop(feed);
        let _ = b.writer.lock().unwrap().shutdown(std::net::Shutdown::Both);
        return Err("The served thread uses a different execution policy or an older protocol. Stop that idle session worker and reopen it with GC Connect; its policy was not changed.".into());
    }
    drop(feed);
    b.send(&json!({"t":"control","action":"claim"}))?;
    map.insert(id.into(), b.clone());
    Ok(b)
}
struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}
fn request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 16384 {
            return Err("Headers exceed 16 KiB".into());
        }
        let mut b = [0];
        stream.read_exact(&mut b).map_err(|e| e.to_string())?;
        header.push(b[0]);
    }
    let text = std::str::from_utf8(&header).map_err(|_| "Invalid headers")?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next().ok_or("Missing request")?.split_whitespace();
    let method = first.next().ok_or("Missing method")?.to_string();
    let path = first.next().ok_or("Missing path")?.to_string();
    let mut headers: HashMap<String, String> = HashMap::new();
    for line in lines.filter(|s| !s.is_empty()) {
        let (k, v) = line.split_once(':').ok_or("Invalid header")?;
        if headers
            .insert(k.to_ascii_lowercase(), v.trim().into())
            .is_some()
        {
            return Err("Duplicate header".into());
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err("Chunked requests are not accepted".into());
    }
    let len = headers
        .get("content-length")
        .map(|s| s.parse::<usize>())
        .transpose()
        .map_err(|_| "Invalid body length")?
        .unwrap_or(0);
    let limit = match (method.as_str(), path.split('?').next()) {
        ("POST", Some("/api/upload")) => 8 * 1024 * 1024,
        ("POST", Some("/api/artifact")) => 512 * 1024,
        _ => MAX_BODY,
    };
    if len > limit {
        return Err(format!("Input exceeds {} bytes", limit));
    }
    let mut body = vec![0; len];
    stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}
fn response(
    stream: &mut TcpStream,
    status: u16,
    kind: &str,
    body: &[u8],
    extra: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self'; frame-src 'self' blob:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n{extra}\r\n",
        if status < 400 { "OK" } else { "Error" },
        body.len()
    )?;
    stream.write_all(body)
}
fn json_response(stream: &mut TcpStream, status: u16, value: Value) -> std::io::Result<()> {
    response(
        stream,
        status,
        "application/json",
        value.to_string().as_bytes(),
        "",
    )
}
fn origin(settings: &Settings) -> String {
    if settings.local {
        format!("http://127.0.0.1:{}", settings.port)
    } else {
        web_url(settings)
    }
}
fn origin_authority(value: &str) -> Option<&str> {
    let (scheme, authority) = value.split_once("://")?;
    (!scheme.is_empty() && !authority.is_empty()).then_some(authority)
}
fn authorized(settings: &Settings, r: &Request) -> bool {
    let candidates = [origin(settings), https_url(settings)];
    let Some(expected) = candidates.iter().find(|candidate| {
        let Some(authority) = origin_authority(candidate) else {
            return false;
        };
        r.headers.get("host").is_some_and(|host| host == authority)
    }) else {
        return false;
    };
    if r.headers.get("origin").is_some_and(|o| o != expected) {
        return false;
    }
    if r.headers
        .get("sec-fetch-site")
        .is_some_and(|s| !matches!(s.as_str(), "same-origin" | "none"))
    {
        return false;
    }
    if settings.local {
        r.headers.get("authorization") == Some(&format!("Bearer {}", settings.token))
            || r.headers.get("cookie").is_some_and(|s| {
                s.split(';')
                    .any(|part| part.trim() == format!("gc_local={}", settings.token))
            })
    } else {
        r.headers.get("tailscale-user-login") == Some(&settings.login)
    }
}
fn history(id: &str, before: usize) -> Result<Value, String> {
    history_path(&thread_path(id)?, before)
}
fn history_path(path: &std::path::Path, before: usize) -> Result<Value, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let length = file
        .metadata()
        .map_err(|e| e.to_string())?
        .len()
        .try_into()
        .unwrap_or(usize::MAX);
    let end = before.min(length);
    let offset = end.saturating_sub(512 * 1024);
    let starts_mid_record = if offset > 0 {
        file.seek(SeekFrom::Start(offset as u64 - 1))
            .map_err(|e| e.to_string())?;
        let mut previous = [0];
        file.read_exact(&mut previous).map_err(|e| e.to_string())?;
        previous[0] != b'\n'
    } else {
        false
    };
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((end - offset) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let mut pos = offset;
    let mut records = VecDeque::new();
    let mut first = true;
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        let index = pos;
        pos += line.len();
        if first && starts_mid_record {
            first = false;
            continue;
        }
        first = false;
        if !line.ends_with(b"\n") || line.len() == 1 {
            continue;
        }
        let record: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        if crate::reasoning::private_value(&record) {
            continue;
        }
        if records.len() >= 60 {
            records.pop_front();
        }
        records.push_back(json!({"index":index,"record":record}));
    }
    let mut omitted = Vec::new();
    let mut private_before = None;
    if starts_mid_record {
        let mut prefix_file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let floor = offset.saturating_sub(8 * 1024 * 1024);
        let mut cursor = offset;
        let mut record_start = None;
        // Usually one 32 KiB read locates the preceding boundary. Never read
        // an 8 MiB prefix eagerly for an ordinary history page.
        while cursor > floor {
            let start = cursor.saturating_sub(32768).max(floor);
            prefix_file
                .seek(SeekFrom::Start(start as u64))
                .map_err(|e| e.to_string())?;
            let mut chunk = vec![0; cursor - start];
            prefix_file
                .read_exact(&mut chunk)
                .map_err(|e| e.to_string())?;
            if let Some(n) = chunk.iter().rposition(|b| *b == b'\n') {
                record_start = Some(start + n + 1);
                break;
            }
            cursor = start;
        }
        if record_start.is_none() && cursor == 0 {
            record_start = Some(0);
        }
        let record_end = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| offset + n + 1)
            .or_else(|| (end < length).then_some(end));
        if let (Some(start), Some(finish)) = (record_start, record_end) {
            if crate::reasoning::private_at(path, start as u64)? {
                private_before = Some(start as u64);
            } else if finish - start > 512 * 1024 {
                omitted.push(json!({"recordOffset":start,"bytes":finish-start,"readable":finish-start<=8*1024*1024}));
            }
        }
    }
    let start = records
        .front()
        .and_then(|v| v["index"].as_u64())
        .unwrap_or_else(|| {
            omitted
                .first()
                .and_then(|v| v["recordOffset"].as_u64())
                .unwrap_or(private_before.unwrap_or(offset as u64))
        });
    Ok(
        json!({"omitted":omitted,"records":records,"before":start,"totalBytes":length,"windowBytes":512*1024,"omittedLargeRecord":records.is_empty()&&end>0}),
    )
}
// Deliberate message actions resolve the byte cursor once, never on every
// history page. Known native records define ordinal space; blank/unknown JSON
// lines must not shift quote/rewrite indices.
fn activity_location(id: &str, requested: usize) -> Result<Value, String> {
    use std::io::BufRead;
    if requested > 64 * 1024 * 1024 {
        return Err("Activity mapping exceeds 64 MiB; use native history controls".into());
    }
    let file = std::fs::File::open(thread_path(id)?).map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(file.take((requested + 8 * 1024 * 1024 + 1) as u64));
    let mut offset = 0usize;
    let mut ordinal = 0usize;
    let mut turn = 0usize;
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader
            .by_ref()
            .take(8 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if n > 8 * 1024 * 1024 {
            return Err("Activity record exceeds 8 MiB".into());
        }
        if let Ok(value) = serde_json::from_slice::<Value>(&line)
            && let Some(record) = crate::session::Record::from_json(&value)
        {
            if matches!(record, crate::session::Record::User(_)) {
                turn += 1;
            }
            if offset == requested {
                return Ok(
                    json!({"recordOffset":offset,"record":ordinal,"turn":turn,"revision":match &record {
                crate::session::Record::User(t)|crate::session::Record::Assistant(t)=>Some(crate::workspace::hash(t.as_bytes())),
                crate::session::Record::ToolResult{body:t,..}|crate::session::Record::Bash{output:t,..}=>Some(crate::workspace::hash(t.as_bytes())),
                _=>None}}),
                );
            }
            ordinal += 1;
        }
        offset += n;
        if offset > requested {
            break;
        }
    }
    Err("Activity no longer exists at this byte cursor; reload history".into())
}
struct ActivityText {
    created: Instant,
    text: String,
    kind: String,
    revision: String,
}
fn activity_texts() -> &'static Mutex<HashMap<String, ActivityText>> {
    static STORE: std::sync::OnceLock<Mutex<HashMap<String, ActivityText>>> =
        std::sync::OnceLock::new();
    STORE.get_or_init(Default::default)
}
fn activity_text(
    id: &str,
    record_offset: usize,
    offset: usize,
    expected: &str,
) -> Result<Value, String> {
    use std::io::BufRead;
    let path = thread_path(id)?;
    if crate::reasoning::private_at(&path, record_offset as u64)? {
        return Err("Provider replay metadata is private; use the reasoning view".into());
    }
    if offset > 0 && expected.is_empty() {
        return Err("Text continuation requires its previous revision".into());
    }
    let key = format!("{id}:{record_offset}:{expected}");
    let page = |entry: &ActivityText| -> Result<Value, String> {
        if offset > entry.text.len() || !entry.text.is_char_boundary(offset) {
            return Err("Invalid UTF-8 text cursor".into());
        }
        let mut end = (offset + 128 * 1024).min(entry.text.len());
        while !entry.text.is_char_boundary(end) {
            end -= 1;
        }
        Ok(
            json!({"recordOffset":record_offset,"offset":offset,"text":&entry.text[offset..end],"nextOffset":end,"eof":end==entry.text.len(),"totalBytes":entry.text.len(),"kind":entry.kind,"revision":entry.revision}),
        )
    };
    {
        let mut store = activity_texts()
            .lock()
            .map_err(|_| "Activity reader unavailable")?;
        store.retain(|_, entry| entry.created.elapsed() < Duration::from_secs(120));
        if offset > 0
            && let Some(entry) = store.get(&key)
        {
            return page(entry);
        }
    }
    let mut file = std::fs::File::open(thread_path(id)?).map_err(|e| e.to_string())?;
    if record_offset > 0 {
        file.seek(SeekFrom::Start(record_offset as u64 - 1))
            .map_err(|e| e.to_string())?;
        let mut previous = [0];
        file.read_exact(&mut previous).map_err(|e| e.to_string())?;
        if previous[0] != b'\n' {
            return Err("recordOffset must identify a record boundary".into());
        }
    }
    file.seek(SeekFrom::Start(record_offset as u64))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::BufReader::new(file.take(8 * 1024 * 1024 + 1))
        .read_until(b'\n', &mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Activity exceeds 8 MiB; export the transcript".into());
    }
    if !bytes.ends_with(b"\n") {
        return Err("Activity is incomplete; retry after it settles".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let kind = value["k"]
        .as_str()
        .ok_or("Activity kind unavailable")?
        .to_string();
    let text = value["t"]
        .as_str()
        .or_else(|| value["out"].as_str())
        .or_else(|| value["summary"].as_str())
        .ok_or("This activity has no text body; use its normal viewer or transcript export")?
        .to_string();
    let revision = crate::workspace::hash(text.as_bytes());
    if !expected.is_empty() && revision != expected {
        return Err("Activity changed; reopen the reader".into());
    }
    let entry = ActivityText {
        created: Instant::now(),
        text,
        kind,
        revision,
    };
    let result = page(&entry)?;
    let key = format!("{id}:{record_offset}:{}", entry.revision);
    let mut store = activity_texts()
        .lock()
        .map_err(|_| "Activity reader unavailable")?;
    store.retain(|_, entry| entry.created.elapsed() < Duration::from_secs(120));
    store.remove(&key);
    while store.len() >= 16
        || store.values().map(|v| v.text.len()).sum::<usize>() + entry.text.len() > 16 * 1024 * 1024
    {
        let oldest = store
            .iter()
            .min_by_key(|(_, v)| v.created)
            .map(|(k, _)| k.clone());
        if let Some(key) = oldest {
            store.remove(&key);
        } else {
            break;
        }
    }
    store.insert(key, entry);
    Ok(result)
}
fn history_record(id: &str, record_offset: usize, offset: usize) -> Result<Value, String> {
    use base64::Engine;
    let record_path = thread_path(id)?;
    if crate::reasoning::private_at(&record_path, record_offset as u64)? {
        return Err("Provider replay metadata is private; use the reasoning view".into());
    }
    if offset >= 8 * 1024 * 1024 {
        return Err("Large activity exceeds the 8 MiB reader limit; export the transcript".into());
    }
    let mut file = std::fs::File::open(thread_path(id)?).map_err(|e| e.to_string())?;
    if record_offset > 0 {
        file.seek(SeekFrom::Start(record_offset as u64 - 1))
            .map_err(|e| e.to_string())?;
        let mut previous = [0];
        file.read_exact(&mut previous).map_err(|e| e.to_string())?;
        if previous[0] != b'\n' {
            return Err("recordOffset must identify a record boundary".into());
        }
    }
    file.seek(SeekFrom::Start(record_offset as u64))
        .map_err(|e| e.to_string())?;
    let mut first = [0];
    file.read_exact(&mut first).map_err(|e| e.to_string())?;
    if first[0] != b'{' {
        return Err("recordOffset must identify JSON activity".into());
    }
    file.seek(SeekFrom::Start(
        record_offset
            .checked_add(offset)
            .ok_or("Invalid activity offset")? as u64,
    ))
    .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((32768usize.min(8 * 1024 * 1024 - offset)) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let eof = bytes
        .iter()
        .position(|b| *b == b'\n')
        .map(|n| {
            bytes.truncate(n + 1);
            true
        })
        .unwrap_or(false);
    if bytes.is_empty() {
        return Err("Activity is incomplete or no longer available".into());
    }
    Ok(
        json!({"recordOffset":record_offset,"offset":offset,"nextOffset":offset+bytes.len(),"eof":eof,"data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
    )
}
fn browse_path(settings: &Settings, path: &str) -> Result<PathBuf, String> {
    let root = settings
        .workspace
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let requested = if path.is_empty() {
        root.clone()
    } else {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    };
    let resolved = requested.canonicalize().map_err(|e| e.to_string())?;
    let unrestricted =
        settings.security["profile"] == "unrestricted" && settings.security["sandbox"] == "off";
    if !unrestricted && !resolved.starts_with(&root) {
        return Err("Browsing is limited to the permitted workspace".into());
    }
    Ok(resolved)
}
struct Directory(*mut libc::DIR);
impl Drop for Directory {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}
impl Directory {
    fn open(path: &std::path::Path) -> Result<Self, String> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let mut file = std::fs::File::open("/").map_err(|e| e.to_string())?;
        for component in path.components() {
            if let std::path::Component::Normal(name) = component {
                use std::os::unix::ffi::OsStrExt;
                let name = std::ffi::CString::new(name.as_bytes()).map_err(|e| e.to_string())?;
                let fd = unsafe {
                    libc::openat(
                        file.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                file = unsafe { std::fs::File::from_raw_fd(fd) };
            }
        }
        use std::os::fd::IntoRawFd;
        let fd = file.into_raw_fd();
        let dir = unsafe { libc::fdopendir(fd) };
        if dir.is_null() {
            unsafe {
                libc::close(fd);
            }
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Self(dir))
    }
    fn next(&self) -> Option<(String, libc::stat)> {
        for _ in 0..512 {
            let entry = unsafe { libc::readdir(self.0) };
            if entry.is_null() {
                return None;
            }
            let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) };
            let Ok(text) = name.to_str() else {
                continue;
            };
            if matches!(text, "." | "..") {
                continue;
            }
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            if unsafe {
                libc::fstatat(
                    libc::dirfd(self.0),
                    name.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } != 0
            {
                continue;
            }
            return Some((text.to_string(), unsafe { stat.assume_init() }));
        }
        None
    }
}
fn browse(settings: &Settings, query: &HashMap<String, String>) -> Result<Value, String> {
    let path = browse_path(
        settings,
        query.get("path").map(String::as_str).unwrap_or(""),
    )?;
    let dir = Directory::open(&path)?;
    let offset = query
        .get("offset")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0)
        .min(8192);
    let start = Instant::now();
    for _ in 0..offset {
        if dir.next().is_none() {
            break;
        }
    }
    let mut entries = Vec::new();
    let mut more = false;
    for _ in 0..257 {
        let Some((name, stat)) = dir.next() else {
            break;
        };
        if entries.len() == 256 {
            more = true;
            break;
        }
        let kind = match stat.st_mode & libc::S_IFMT {
            libc::S_IFDIR => "directory",
            libc::S_IFREG => "file",
            libc::S_IFLNK => "link",
            _ => "special",
        };
        entries.push(json!({"name":name,"kind":kind,"size":stat.st_size.max(0),"bytes":allocation_bytes(stat.st_blocks)}));
        if start.elapsed() > Duration::from_millis(150) {
            more = true;
            break;
        }
    }
    let scanned = entries.len();
    entries.sort_by(|a, b| {
        (a["kind"] != "directory", a["name"].as_str().unwrap_or(""))
            .cmp(&(b["kind"] != "directory", b["name"].as_str().unwrap_or("")))
    });
    let parent = path
        .parent()
        .filter(|p| browse_path(settings, &p.to_string_lossy()).is_ok())
        .map(|p| p.to_string_lossy().into_owned());
    Ok(
        json!({"path":path,"parent":parent,"entries":entries,"next":if more && offset+scanned<8192{Some(offset+scanned)}else{None},"limited":more,"limit":256}),
    )
}
fn disk_usage(settings: &Settings, path: &str) -> Result<Value, String> {
    let root = browse_path(settings, path)?;
    let start = Instant::now();
    let mut pending = VecDeque::from([(root.clone(), 0usize)]);
    let mut bytes = 0u64;
    let mut count = 0usize;
    let mut partial = false;
    use std::os::unix::fs::MetadataExt;
    let device = root.metadata().map_err(|e| e.to_string())?.dev();
    let mut visited = std::collections::HashSet::new();
    while let Some((path, depth)) = pending.pop_front() {
        let Ok(dir) = Directory::open(&path) else {
            partial = true;
            continue;
        };
        while let Some((name, stat)) = dir.next() {
            count += 1;
            if i128::from(stat.st_dev) != i128::from(device)
                || !visited.insert((stat.st_dev, stat.st_ino))
            {
                partial |= i128::from(stat.st_dev) != i128::from(device);
                if count >= 4096 || start.elapsed() >= Duration::from_millis(100) {
                    return Ok(json!({"path":root,"bytes":bytes,"entries":count,"partial":true}));
                }
                continue;
            }
            bytes = bytes.saturating_add(allocation_bytes(stat.st_blocks));
            if stat.st_mode & libc::S_IFMT == libc::S_IFDIR {
                if depth < 16 && pending.len() < 256 {
                    pending.push_back((path.join(name), depth + 1));
                } else {
                    partial = true;
                }
            }
            if count >= 4096 || start.elapsed() >= Duration::from_millis(100) {
                return Ok(json!({"path":root,"bytes":bytes,"entries":count,"partial":true}));
            }
        }
    }
    Ok(json!({"path":root,"bytes":bytes,"entries":count,"partial":partial}))
}

fn allocation_bytes(blocks: libc::blkcnt_t) -> u64 {
    u64::try_from(blocks.max(0))
        .unwrap_or(u64::MAX)
        .saturating_mul(512)
}
struct Artifact {
    created: Instant,
    html: Vec<u8>,
}
fn artifacts() -> &'static Mutex<HashMap<String, Artifact>> {
    static STORE: std::sync::OnceLock<Mutex<HashMap<String, Artifact>>> =
        std::sync::OnceLock::new();
    STORE.get_or_init(Default::default)
}
fn create_artifact(id: &str, html: &[u8]) -> Result<Value, String> {
    use ring::rand::SecureRandom;
    thread_path(id)?;
    if html.is_empty() || html.len() > 512 * 1024 {
        return Err("Interactive HTML must be between 1 byte and 512 KiB".into());
    }
    std::str::from_utf8(html).map_err(|_| "Interactive HTML must be UTF-8")?;
    let mut random = [0u8; 32];
    ring::rand::SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| "Artifact token unavailable")?;
    let token = crate::workspace::hash(&random);
    let mut store = artifacts()
        .lock()
        .map_err(|_| "Artifact storage unavailable")?;
    store.retain(|_, v| v.created.elapsed() < Duration::from_secs(600));
    while store.len() >= 16
        || store.values().map(|v| v.html.len()).sum::<usize>() + html.len() > 4 * 1024 * 1024
    {
        let oldest = store
            .iter()
            .min_by_key(|(_, v)| v.created)
            .map(|(k, _)| k.clone());
        if let Some(key) = oldest {
            store.remove(&key);
        } else {
            break;
        }
    }
    store.insert(
        token.clone(),
        Artifact {
            created: Instant::now(),
            html: html.to_vec(),
        },
    );
    Ok(json!({"token":token,"url":format!("/api/artifact?token={token}"),"expiresIn":600}))
}
fn artifact_document(stream: &mut TcpStream, token: &str) -> Result<(), String> {
    let html = {
        let mut store = artifacts()
            .lock()
            .map_err(|_| "Artifact storage unavailable")?;
        store.retain(|_, v| v.created.elapsed() < Duration::from_secs(600));
        store
            .get(token)
            .ok_or("Artifact expired; choose Run scripts again")?
            .html
            .clone()
    };
    // This response intentionally has its own policy, rather than trying to
    // relax the app's CSP from srcdoc. Both header and iframe sandbox omit
    // allow-same-origin: scripts receive an opaque origin, never host APIs.
    // Navigation by the frame itself is not an absolute no-network guarantee.
    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nPermissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=(), clipboard-read=(), clipboard-write=()\r\nContent-Security-Policy: sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'\r\n\r\n",html.len()).map_err(|e|e.to_string())?;
    stream.write_all(&html).map_err(|e| e.to_string())
}
fn context_attachments(id: &str) -> Result<Value, String> {
    thread_path(id)?;
    // Context chips need metadata only, not a parse of the entire transcript.
    let bytes = crate::requests::bounded(
        &crate::config::data_dir().join(format!("{id}.meta.json")),
        4 * 1024 * 1024,
    )?;
    let meta: crate::polish::Metadata =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let attachments=meta.attachments.iter().take(128).map(|a|json!({"path":a.path,"source":a.source,"bytes":a.text.len()+a.data.as_ref().map(|d|d.len()*3/4).unwrap_or(0),"mediaType":a.media_type})).collect::<Vec<_>>();
    Ok(json!({"attachments":attachments,"usage":null}))
}
// Upload storage is private, bounded per thread, and outside the project. An
// upload alone never enters model context; the actor's attach command does that.
fn upload(app: &App, query: &HashMap<String, String>, body: &[u8]) -> Result<Value, String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    static UPLOAD_LOCK: Mutex<()> = Mutex::new(());
    let _guard = UPLOAD_LOCK
        .lock()
        .map_err(|_| "Upload storage unavailable")?;
    let id = query.get("id").ok_or("Missing thread")?;
    let b = bridge(app, id)?;
    if b.feed
        .lock()
        .map_err(|_| "Thread state unavailable")?
        .snapshot["can_write"]
        != true
    {
        return Err("Read-only observer: take control before uploading".into());
    }
    if body.is_empty() {
        return Err("Attachment is empty".into());
    }
    let image = crate::polish::image_media_type(body).is_some();
    if !image && (body.len() > 32768 || std::str::from_utf8(body).is_err() || body.contains(&0)) {
        return Err("Choose an image up to 8 MiB or UTF-8 text up to 32 KiB".into());
    }
    let name = query
        .get("name")
        .map(String::as_str)
        .unwrap_or("attachment");
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '/' && *c != '\\')
        .take(120)
        .collect();
    let token = crate::workspace::hash(&[name.as_bytes(), b"\0", body].concat());
    let dir = crate::config::data_dir().join("web-uploads").join(id);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| e.to_string())?;
    Directory::open(&dir)?;
    let path = dir.join(&token);
    if !path.exists() {
        let mut count = 0;
        let mut bytes = 0u64;
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())?.take(65) {
            let entry = entry.map_err(|e| e.to_string())?;
            count += 1;
            bytes += entry.metadata().map_err(|e| e.to_string())?.len();
        }
        if count >= 64 || bytes + body.len() as u64 > 32 * 1024 * 1024 {
            return Err("Thread upload budget reached (64 files / 32 MiB)".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|e| e.to_string())?;
        if let Err(error) = file.write_all(body).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(&path);
            return Err(error.to_string());
        }
    }
    Ok(
        json!({"token":token,"name":name,"bytes":body.len(),"kind":if image {"image"} else {"text"}}),
    )
}
fn edit_file(id: &str, path: &str) -> Result<Value, String> {
    let root = thread_workspace(id)?;
    let file = PathBuf::from(path);
    let file = if file.is_absolute() {
        file
    } else {
        root.join(file)
    };
    let file = file.canonicalize().map_err(|e| e.to_string())?;
    let relative = file
        .strip_prefix(&root)
        .map_err(|_| "Editing is limited to the thread workspace")?;
    let relative = relative.to_str().ok_or("File path must be UTF-8")?;
    let bytes = crate::files::read_workspace_bytes(&root, relative)?;
    if bytes.len() > 32768 {
        return Err("Browser editing is limited to 32 KiB text files".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Only UTF-8 text files can be edited")?;
    if text.contains('\0') {
        return Err("Binary files cannot be edited as text".into());
    }
    Ok(
        json!({"path":relative,"text":text,"revision":crate::files::revision(&bytes),"maxBytes":32768}),
    )
}
fn preview_file(id: &str, path: &str) -> Result<(String, Vec<u8>), String> {
    let root = thread_workspace(id)?;
    preview_path(&root, path)
}
fn preview_path(root: &std::path::Path, path: &str) -> Result<(String, Vec<u8>), String> {
    let requested = PathBuf::from(path);
    let file = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    }
    .canonicalize()
    .map_err(|e| e.to_string())?;
    if !file.starts_with(root) {
        return Err("File previews are limited to the thread workspace".into());
    }
    let relative = file.strip_prefix(root).map_err(|_| "File path rejected")?;
    let bytes = crate::files::read_workspace_bytes(
        root,
        relative.to_str().ok_or("File path must be UTF-8")?,
    )?;
    if bytes.starts_with(b"%PDF-") {
        return Ok(("application/pdf".into(), bytes));
    }
    if let Some(mime) = crate::polish::image_media_type(&bytes) {
        let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
        if u64::from(width) * u64::from(height) > 16_000_000 {
            return Err("Image preview exceeds 16 million pixels".into());
        }
        return Ok((mime.into(), bytes));
    }
    if bytes.len() > 512 * 1024 {
        return Err("Text preview exceeds 512 KiB; inspect it with the read tool".into());
    }
    std::str::from_utf8(&bytes).map_err(|_| "Binary file cannot be previewed as text")?;
    Ok(("text/plain; charset=utf-8".into(), bytes))
}
fn event_stream(mut stream: TcpStream, b: Arc<Bridge>, since: u64) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nX-Accel-Buffering: no\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;
    let mut cursor = since;
    loop {
        let mut f = b.feed.lock().unwrap();
        if f.seq <= cursor && f.connected {
            f = b
                .changed
                .wait_timeout(f, Duration::from_secs(15))
                .unwrap()
                .0;
        }
        let reset = cursor == 0
            || cursor > f.seq
            || f.events
                .front()
                .is_some_and(|(seq, _, _)| cursor + 1 < *seq);
        let data = if reset {
            json!({"reset":true,"cursor":f.seq,"active":f.active,"control":f.snapshot,"interaction":f.interaction,"live":if f.active{f.live.iter().cloned().collect::<Vec<_>>()}else{Vec::new()},"liveClipped":f.live_clipped})
        } else {
            json!({"cursor":f.seq,"events":f.events.iter().filter(|(seq,_,_)|*seq>cursor).map(|(_,v,_)|v).collect::<Vec<_>>()})
        };
        cursor = f.seq;
        let connected = f.connected;
        drop(f);
        write!(stream, "id: {}:{cursor}\ndata: {data}\n\n", b.epoch)?;
        stream.flush()?;
        if !connected {
            return Ok(());
        }
    }
}
fn handle(mut stream: TcpStream, app: Arc<App>) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let r = match request(&mut stream) {
        Ok(r) => r,
        Err(e) => {
            let _ = json_response(&mut stream, 400, json!({"error":e}));
            return Ok(());
        }
    };
    let url = url::Url::parse(&format!("http://gateway{}", r.path)).map_err(|e| e.to_string())?;
    let query: HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let path = url.path();
    if path == "/api/local" && app.settings.local && r.method == "POST" {
        let expected = origin(&app.settings);
        let Some(expected_authority) = origin_authority(&expected) else {
            return json_response(&mut stream, 403, json!({"error":"Origin rejected"}))
                .map_err(|e| e.to_string());
        };
        if r.headers.get("host").map(String::as_str) != Some(expected_authority)
            || r.headers.get("origin") != Some(&expected)
        {
            return json_response(&mut stream, 403, json!({"error":"Origin rejected"}))
                .map_err(|e| e.to_string());
        }
        let v: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
        if v["token"] != app.settings.token {
            return json_response(&mut stream, 403, json!({"error":"Access rejected"}))
                .map_err(|e| e.to_string());
        }
        return response(
            &mut stream,
            200,
            "application/json",
            b"{}",
            &format!(
                "Set-Cookie: gc_local={}; HttpOnly; SameSite=Strict; Path=/\r\n",
                app.settings.token
            ),
        )
        .map_err(|e| e.to_string());
    }
    let is_asset = !path.starts_with("/api/");
    if (!is_asset || !app.settings.local) && !authorized(&app.settings, &r) {
        return json_response(
            &mut stream,
            403,
            json!({"error":"Access is limited to this host's Tailscale owner"}),
        )
        .map_err(|e| e.to_string());
    }
    if is_asset {
        if r.method != "GET" {
            return Err("Unsupported method".into());
        }
        let file = if path == "/" {
            app.settings.assets.join("index.html")
        } else {
            app.settings.assets.join(path.trim_start_matches('/'))
        };
        let file = file.canonicalize().map_err(|_| "Asset not found")?;
        if !file.starts_with(&app.settings.assets) {
            return Err("Asset path rejected".into());
        }
        let kind = match file.extension().and_then(|s| s.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8",
            "js" | "mjs" => "text/javascript",
            "css" => "text/css",
            "json" | "webmanifest" => "application/manifest+json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "woff2" => "font/woff2",
            "woff" => "font/woff",
            "ttf" => "font/ttf",
            _ => "application/octet-stream",
        };
        let gzip = file.with_file_name(format!(
            "{}.gz",
            file.file_name().unwrap().to_string_lossy()
        ));
        let zipped = r
            .headers
            .get("accept-encoding")
            .is_some_and(|s| s.split(',').any(|s| s.trim() == "gzip"))
            && gzip.is_file();
        let data = crate::requests::bounded(if zipped { &gzip } else { &file }, 16 * 1024 * 1024)?;
        return response(
            &mut stream,
            200,
            kind,
            &data,
            if zipped {
                "Content-Encoding: gzip\r\nVary: Accept-Encoding\r\n"
            } else {
                ""
            },
        )
        .map_err(|e| e.to_string());
    }
    if path == "/api/artifact" && r.method == "GET" {
        let result = artifact_document(
            &mut stream,
            query.get("token").map(String::as_str).unwrap_or(""),
        );
        return match result {
            Ok(()) => Ok(()),
            Err(error) => {
                json_response(&mut stream, 410, json!({"error":error})).map_err(|e| e.to_string())
            }
        };
    }
    if path == "/api/export" && r.method == "GET" {
        let result = (|| -> Result<Vec<u8>, String> {
            let path = thread_path(query.get("id").ok_or("Missing thread")?)?;
            crate::requests::bounded(&path, 64 * 1024 * 1024)?;
            let session = crate::session::Session::open_readonly(
                &crate::config::data_dir(),
                path.to_str().ok_or("Invalid session path")?,
            )
            .map_err(|e| e.to_string())?;
            Ok(
                crate::export::markdown(&session, false, &crate::export::raw_records(&session))
                    .into_bytes(),
            )
        })();
        return match result {
            Ok(bytes) => response(
                &mut stream,
                200,
                "text/markdown; charset=utf-8",
                &bytes,
                "Content-Disposition: attachment; filename=glass-transcript.md\r\n",
            )
            .map_err(|e| e.to_string()),
            Err(error) => {
                json_response(&mut stream, 400, json!({"error":error})).map_err(|e| e.to_string())
            }
        };
    }
    if path == "/api/file" && r.method == "GET" {
        let result = (|| {
            if let Some(workspace) = query.get("workspace") {
                let root = browse_path(&app.settings, workspace)?;
                preview_path(&root, query.get("path").ok_or("Missing file path")?)
            } else {
                preview_file(
                    query.get("id").ok_or("Missing thread")?,
                    query.get("path").ok_or("Missing file path")?,
                )
            }
        })();
        match result {
            Ok((kind, body)) => {
                return response(&mut stream, 200, &kind, &body, "").map_err(|e| e.to_string());
            }
            Err(e) => {
                return json_response(&mut stream, 400, json!({"error":e}))
                    .map_err(|e| e.to_string());
            }
        }
    }
    let result: Result<Value, String> = (|| match (r.method.as_str(), path) {
        ("GET", "/api/status") => Ok(
            json!({"workspace":app.settings.workspace,"remote":!app.settings.local,"name":"Glass","web_workflows":1,"interactive_artifacts":1,"profile":crate::sandbox::permission_profile_label(),"security":app.settings.security}),
        ),
        ("GET", "/api/commands") => Ok(json!({"commands":crate::web_commands::catalogue(None)})),
        ("POST", "/api/upgrade") => {
            let value: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            let id = value["id"].as_str().ok_or("Missing thread")?;
            if value["confirmed"] != true {
                return Err(
                    "Confirm restarting this thread runtime and its owned background work".into(),
                );
            }
            let current = bridge(&app, id)?;
            {
                let feed = current
                    .feed
                    .lock()
                    .map_err(|_| "Thread state unavailable")?;
                if feed.active || feed.interaction.is_some() {
                    return Err(
                        "Finish or cancel the active work before restarting its runtime".into(),
                    );
                }
                if feed.snapshot["can_write"] != true {
                    return Err("Take control before restarting the thread runtime".into());
                }
            }
            current.send(&json!({"t":"stop"}))?;
            let socket = crate::config::data_dir().join(format!("serve-{id}.sock"));
            let lock = crate::config::data_dir().join(format!("{id}.writer.lock"));
            let started = Instant::now();
            loop {
                if !socket.exists() {
                    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
                    if let Ok(file) = std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&lock)
                        && unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) }
                            == 0
                    {
                        break;
                    }
                }
                if started.elapsed() > Duration::from_secs(5) {
                    return Err("The runtime is still finishing. No process was killed; reconnect after it settles.".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            app.bridges
                .lock()
                .map_err(|_| "Thread connections unavailable")?
                .remove(id);
            let next = bridge(&app, id)?;
            next.send(&json!({"t":"control","action":"claim"}))?;
            Ok(json!({"upgraded":true,"thread":id,"history_preserved":true}))
        }
        ("POST", "/api/command") => {
            let value: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            let id = value["id"].as_str().ok_or("Missing thread")?;
            let name = crate::actions::canonical(value["name"].as_str().ok_or("Missing command")?);
            let path = thread_path(id)?;
            if !crate::web_commands::readonly(name) {
                return Err("This command requires the session control channel".into());
            }
            let session = crate::session::Session::open_readonly(
                &crate::config::data_dir(),
                path.to_str().ok_or("Invalid session path")?,
            )
            .map_err(|e| e.to_string())?;
            let mut cfg = app.cfg.clone();
            cfg.workspace = crate::polish::metadata(&session).workspace;
            crate::web_commands::inspect_request(name, &value["values"], &session, &cfg)
                .map(crate::web_commands::bounded_result)
        }
        ("POST", "/api/artifact") => {
            create_artifact(query.get("id").ok_or("Missing thread")?, &r.body)
        }
        ("GET", "/api/context") => context_attachments(query.get("id").ok_or("Missing thread")?),
        ("POST", "/api/upload") => upload(&app, &query, &r.body),
        ("GET", "/api/edit") => edit_file(
            query.get("id").ok_or("Missing thread")?,
            query.get("path").ok_or("Missing file path")?,
        ),
        ("GET", "/api/changes") => {
            let id = query.get("id").ok_or("Missing thread")?;
            let mut cfg = app.cfg.clone();
            cfg.workspace = thread_workspace(id)?;
            crate::web_commands::changes(&cfg)
        }
        ("GET", "/api/browse") => browse(&app.settings, &query),
        ("GET", "/api/usage") => {
            disk_usage(&app.settings, query.get("path").ok_or("Missing directory")?)
        }
        ("GET", "/api/threads") => Ok(crate::navigation::inspect_web(
            &app.settings.workspace,
            query
                .get("offset")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0),
            query.get("q").map(String::as_str).unwrap_or(""),
            query.get("project").map(String::as_str).unwrap_or(""),
            query.get("id").map(String::as_str).unwrap_or(""),
        )),
        ("GET", "/api/agents") => {
            let id = query.get("id").ok_or("Missing thread")?;
            thread_path(id)?;
            crate::team::client(id, &json!({"action":"list"}))
        }
        ("GET", "/api/agent") => {
            let id = query.get("id").ok_or("Missing thread")?;
            thread_path(id)?;
            crate::team::client(
                id,
                &json!({"action":"transcript","id":query.get("agent").ok_or("Missing agent")?,"before":query.get("before").and_then(|v|v.parse::<u64>().ok())}),
            )
        }
        ("GET", "/api/activity-location") => activity_location(
            query.get("id").ok_or("Missing thread")?,
            query
                .get("recordOffset")
                .and_then(|v| v.parse().ok())
                .ok_or("Missing recordOffset")?,
        ),
        ("GET", "/api/activity-text") => activity_text(
            query.get("id").ok_or("Missing thread")?,
            query
                .get("recordOffset")
                .and_then(|v| v.parse().ok())
                .ok_or("Missing recordOffset")?,
            query
                .get("offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            query.get("expected").map(String::as_str).unwrap_or(""),
        ),
        ("GET", "/api/history-record") => history_record(
            query.get("id").ok_or("Missing thread")?,
            query
                .get("recordOffset")
                .and_then(|v| v.parse().ok())
                .ok_or("Missing recordOffset")?,
            query
                .get("offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        ),
        ("GET", "/api/history") => history(
            query.get("id").ok_or("Missing thread")?,
            query
                .get("before")
                .and_then(|s| s.parse().ok())
                .unwrap_or(usize::MAX),
        ),
        ("GET", "/api/events") => {
            let b = bridge(&app, query.get("id").ok_or("Missing thread")?)?;
            let since = r
                .headers
                .get("last-event-id")
                .and_then(|id| id.split_once(':'))
                .and_then(|(epoch, seq)| {
                    (epoch.parse::<u64>().ok() == Some(b.epoch))
                        .then(|| seq.parse::<u64>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let cloned = stream.try_clone().map_err(|e| e.to_string())?;
            event_stream(cloned, b, since).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        ("POST", "/api/open") => {
            let v: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            let id = v["id"].as_str().ok_or("Missing thread")?;
            bridge(&app, id)?;
            Ok(json!({"connected":true}))
        }
        ("POST", "/api/new") => {
            let request: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            let workspace =
                browse_path(&app.settings, request["workspace"].as_str().unwrap_or(""))?;
            Directory::open(&workspace)?;
            let session = crate::session::Session::create(&crate::config::data_dir())
                .map_err(|e| e.to_string())?;
            crate::polish::bind(&session, &workspace)?;
            Ok(json!({"id":session.id}))
        }
        ("POST", "/api/send") => {
            let value: Value = serde_json::from_slice(&r.body).map_err(|e| e.to_string())?;
            let id = value["id"].as_str().ok_or("Missing thread")?;
            let message = &value["message"];
            if !matches!(
                message["t"].as_str(),
                Some(
                    "prompt"
                        | "steer"
                        | "cancel"
                        | "cancel_command"
                        | "control"
                        | "answer"
                        | "command"
                )
            ) {
                return Err("Unsupported command".into());
            }
            bridge(&app, id)?.send(message)?;
            Ok(json!({"sent":true,"note":"Session acknowledgment determines durable acceptance"}))
        }
        _ => Err("Not found".into()),
    })();
    if path == "/api/events" && result.is_ok() {
        return Ok(());
    }
    match result {
        Ok(v) => json_response(&mut stream, 200, v),
        Err(e) => json_response(&mut stream, 400, json!({"error":e})),
    }
    .map_err(|e| e.to_string())
}
fn gateway(cfg: crate::config::Config, settings: Settings) -> Result<(), String> {
    let listener = TcpListener::bind(("127.0.0.1", settings.port)).map_err(|e| e.to_string())?;
    let app = Arc::new(App {
        settings,
        cfg,
        bridges: Mutex::new(HashMap::new()),
        clients: AtomicUsize::new(0),
    });
    for stream in listener.incoming() {
        let stream = stream.map_err(|e| e.to_string())?;
        if app.clients.fetch_add(1, Ordering::SeqCst) >= 32 {
            app.clients.fetch_sub(1, Ordering::SeqCst);
            drop(stream);
            continue;
        }
        let app = app.clone();
        std::thread::spawn(move || {
            let _ = handle(stream, app.clone());
            app.clients.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn allocation_block_bytes_are_nonnegative_and_saturating() {
        assert_eq!(super::allocation_bytes(-1), 0);
        assert_eq!(super::allocation_bytes(2), 1024);
        assert_eq!(super::allocation_bytes(libc::blkcnt_t::MAX), u64::MAX);
    }

    use super::*;
    fn settings() -> Settings {
        Settings {
            port: 8765,
            https: 8443,
            http: 0,
            http_host: String::new(),
            host: "host.tail.ts.net".into(),
            login: "owner@example.test".into(),
            token: "secret".into(),
            pid: 0,
            stamp: String::new(),
            local: false,
            workspace: PathBuf::from("/tmp"),
            assets: PathBuf::from("/tmp"),
            security: Value::Null,
        }
    }
    #[test]
    fn missing_connect_state_is_a_stopped_status_with_invocation_identity() {
        assert!(parse_status_settings(None).unwrap().is_none());
        assert_eq!(
            status_output(None, "Glass Connect"),
            "Glass Connect · stopped"
        );
        assert_eq!(
            status_output(None, "Glass Core Connect"),
            "Glass Core Connect · stopped"
        );
        assert_eq!(status_output(None, "GC Connect"), "GC Connect · stopped");
    }
    #[test]
    fn malformed_connect_status_state_remains_an_error() {
        assert!(parse_status_settings(Some(b"{".to_vec())).is_err());
    }
    #[test]
    fn identity_and_origin_are_both_required() {
        let s = settings();
        let mut r = Request {
            method: "POST".into(),
            path: "/api/send".into(),
            headers: HashMap::from([
                ("host".into(), "host.tail.ts.net:8443".into()),
                ("tailscale-user-login".into(), s.login.clone()),
            ]),
            body: Vec::new(),
        };
        assert!(authorized(&s, &r));
        r.headers
            .insert("origin".into(), "https://evil.test".into());
        assert!(!authorized(&s, &r));
        r.headers.remove("origin");
        r.headers
            .insert("tailscale-user-login".into(), "other".into());
        assert!(!authorized(&s, &r));
    }
    #[test]
    fn origin_authority_rejects_malformed_values_without_panicking() {
        assert_eq!(
            origin_authority("https://example.test:8443"),
            Some("example.test:8443")
        );
        assert_eq!(origin_authority("https://"), None);
        assert_eq!(origin_authority("missing-scheme"), None);
    }
    #[test]
    fn directory_pages_policy_and_usage_are_bounded() {
        let root = std::env::temp_dir().join(format!(
            "gc-browser-{}-{}",
            std::process::id(),
            crate::loops::now()
        ));
        std::fs::create_dir_all(root.join("workspace/sub")).unwrap();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        std::fs::write(root.join("outside/secret"), b"not part of the scan").unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("workspace/link")).unwrap();
        for i in 0..300 {
            std::fs::write(root.join(format!("workspace/file-{i}")), b"data").unwrap();
        }
        std::fs::hard_link(
            root.join("workspace/file-0"),
            root.join("workspace/sub/hardlink"),
        )
        .unwrap();
        let mut s = settings();
        s.workspace = root.join("workspace");
        s.security = json!({"profile":"workspace-edit","sandbox":"workspace"});
        assert!(browse_path(&s, &root.join("outside").to_string_lossy()).is_err());
        assert!(browse_path(&s, "link").is_err());
        assert!(Directory::open(&s.workspace.join("link")).is_err());
        let first = browse(&s, &HashMap::new()).unwrap();
        assert_eq!(first["entries"].as_array().unwrap().len(), 256);
        assert_eq!(first["next"], 256);
        let second = browse(&s, &HashMap::from([("offset".into(), "256".into())])).unwrap();
        assert_eq!(second["entries"].as_array().unwrap().len(), 46);
        let usage = disk_usage(&s, "").unwrap();
        assert!(!usage["partial"].as_bool().unwrap());
        assert_eq!(usage["entries"], 303);
        assert!(preview_path(&s.workspace, "link/secret").is_err());
        s.security = json!({"profile":"unrestricted","sandbox":"off"});
        let canonical_outside = root.join("outside").canonicalize().unwrap();
        assert_eq!(
            browse_path(&s, &root.join("outside").to_string_lossy()).unwrap(),
            canonical_outside
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ids_never_form_paths() {
        for id in ["../x", "/tmp/file", "", "a.jsonl", "a/b"] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("thread_123-456"));
    }
}
#[cfg(test)]
mod regression {
    use super::*;
    #[test]
    fn tail_history_pages_without_loading_the_whole_log() {
        let path = std::env::temp_dir().join(format!(
            "gc-connect-history-{}-{}.jsonl",
            std::process::id(),
            crate::loops::now()
        ));
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..1000 {
            writeln!(file, "{}", json!({"k":"user","t":format!("message-{i}")})).unwrap();
        }
        drop(file);
        let mut before = usize::MAX;
        let mut seen = std::collections::HashSet::new();
        loop {
            let page = history_path(&path, before).unwrap();
            let next = page["before"].as_u64().unwrap() as usize;
            for r in page["records"].as_array().unwrap() {
                assert!(seen.insert(r["index"].as_u64().unwrap()));
            }
            if next == 0 {
                break;
            }
            assert!(next < before);
            before = next;
        }
        assert_eq!(seen.len(), 1000);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn live_feed_is_bounded_and_keeps_decision_until_answered() {
        let (writer, _reader) = UnixStream::pair().unwrap();
        let b = Bridge {
            epoch: 0,
            writer: Mutex::new(writer),
            feed: Mutex::new(Feed {
                seq: 0,
                bytes: 0,
                events: VecDeque::new(),
                snapshot: Value::Null,
                active: false,
                interaction: None,
                connected: true,
                live: VecDeque::new(),
                live_bytes: 0,
                live_clipped: false,
            }),
            changed: Condvar::new(),
        };
        b.push(json!({"t":"interaction","id":"one","request":{"kind":"question"}}));
        for _ in 0..2000 {
            b.push(json!({"t":"text","delta":"x".repeat(1000)}));
        }
        {
            let f = b.feed.lock().unwrap();
            assert!(f.events.len() <= MAX_EVENTS);
            assert!(f.bytes <= MAX_EVENT_BYTES);
            assert_eq!(f.interaction.as_ref().unwrap()["id"], "one");
        }
        b.push(json!({"t":"interaction_done"}));
        assert!(b.feed.lock().unwrap().interaction.is_none());
    }
    #[test]
    fn parser_rejects_duplicate_lengths_and_chunked_frames() {
        for wire in [
            "POST / HTTP/1.1\r\nContent-Length: 0\r\nContent-Length: 1\r\n\r\n",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 65537\r\n\r\n",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let wire = wire.to_string();
            let writer = std::thread::spawn(move || {
                let mut s = TcpStream::connect(address).unwrap();
                s.write_all(wire.as_bytes()).unwrap();
            });
            let (mut stream, _) = listener.accept().unwrap();
            assert!(request(&mut stream).is_err());
            writer.join().unwrap();
        }
    }
}
