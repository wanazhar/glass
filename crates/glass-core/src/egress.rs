//! Egress policy proxy: one loopback HTTP CONNECT proxy that enforces an
//! allow/deny host list for sandboxed shell children.
//!
//! Ported from codex's `network-proxy`, scoped to what a blocking-I/O,
//! no-tokio build can do honestly: an HTTP CONNECT proxy on 127.0.0.1 that
//! blindly relays the tunnel bytes after the CONNECT is granted (TLS
//! termination and SOCKS5 are *not* in this slice, and the docs must not
//! claim them). Sandboxed children get `http_proxy`/`https_proxy`/`ALL_PROXY`
//! pointing here; a CONNECT to a non-allowlisted host is refused with the
//! host named so the model can reshape the request rather than guess.
//!
//! Bounds: request line ≤ 8 KiB, per-connection worker threads capped, each
//! relay wakes at most one reader loop, and a malformed CONNECT can never
//! panic the listener — the session's shell commands run through it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};

use crate::sandbox::HostPolicy;

/// Request-line and header cap for the CONNECT preamble.
const HEADER_CAP: usize = 8 * 1024;
/// Cap on concurrent relays; over the cap the CONNECT is refused, so a
/// burst of shells cannot exhaust the session.
const MAX_RELAYS: usize = 32;

/// A running proxy handle. Dropping it stops the listener; existing relays
/// wind down as their sockets close.
pub struct Proxy {
    port: u16,
    listener: Option<std::thread::JoinHandle<()>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    policy: std::sync::Arc<HostPolicy>,
}

impl Proxy {
    /// Spawn the listener on a free loopback port. The caller passes the
    /// compiled host policy; the proxy itself is transport-only.
    pub fn spawn(policy: HostPolicy) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .map_err(|e| format!("egress proxy bind failed: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("egress proxy local addr: {e}"))?
            .port();
        let policy = std::sync::Arc::new(policy);
        let thread_policy = std::sync::Arc::clone(&policy);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = stop.clone();
        let handle = std::thread::Builder::new()
            .name("glass-egress-proxy".into())
            .spawn(move || serve(listener, thread_policy, worker_stop))
            .map_err(|e| format!("egress proxy spawn failed: {e}"))?;
        Ok(Self {
            port,
            listener: Some(handle),
            stop,
            policy,
        })
    }

    /// Loopback port the proxy accepted. Caller injects this into
    /// `http_proxy`-style environment variables for sandboxed children.
    #[allow(dead_code)] // used by tests that probe the port directly
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The proxy URL children receive, e.g. `http://127.0.0.1:43210`.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// How many hosts the policy allows and denies; for diagnostics.
    pub fn policy_shape(&self) -> (usize, usize) {
        (self.policy.allow_len(), self.policy.deny_len())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(listener) = self.listener.take() { let _ = listener.join(); }
    }
}

fn serve(listener: TcpListener, policy: std::sync::Arc<HostPolicy>, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let relays = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                if stop.load(std::sync::atomic::Ordering::Acquire) { break; }
                if relays.load(std::sync::atomic::Ordering::SeqCst) >= MAX_RELAYS {
                    let mut s = stream;
                    let _ = s.write_all(b"HTTP/1.1 503 Service Unavailable\r\n\r\n");
                    let _ = s.flush();
                    continue;
                }
                let policy = std::sync::Arc::clone(&policy);
                relays.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let worker_relays = std::sync::Arc::clone(&relays);
                let spawned = std::thread::Builder::new()
                    .name("glass-egress-relay".into())
                    .spawn(move || {
                        handle(stream, policy);
                        worker_relays.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    });
                if spawned.is_err() {
                    relays.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                    continue;
                }
            }
            Err(e) => {
                // Listener closed (process exiting): leave.
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted
                ) {
                    continue;
                }
                return;
            }
        }
    }
}

fn handle(mut client: TcpStream, policy: std::sync::Arc<HostPolicy>) {
    // Read the CONNECT preamble up to the blank line, capped so a long
    // header cannot allocate unbounded memory.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let _ = client.set_write_timeout(Some(std::time::Duration::from_secs(5)));
    let mut preamble = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() || client.set_read_timeout(Some(remaining)).is_err() { return; }
        match client.read(&mut byte) {
            Ok(1) => {
                preamble.push(byte[0]);
                if preamble.ends_with(b"\r\n\r\n") || preamble.ends_with(b"\n\n") {
                    break;
                }
                if preamble.len() > HEADER_CAP {
                    let _ = client.write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n");
                    return;
                }
            }
            _ => return, // closed or malformed preamble
        }
    }
    let text = String::from_utf8_lossy(&preamble);
    let Some(target_line) = text.lines().next() else {
        let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        return;
    };
    let Some(rest) = target_line.strip_prefix("CONNECT ") else {
        // Only CONNECT is intercepted; a plain GET to the proxy port is
        // refused so no one mistakes this for an HTTP forward proxy.
        let _ = client.write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n");
        return;
    };
    let Some(host) = parse_host(rest) else {
        let _ = client.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
        return;
    };
    if let Err(reason) = policy.check(&host) {
        // Naming the host is the whole point of a policy proxy: the model
        // sees which host was refused and can ask for permission properly.
        let body = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}\n",
            reason.len() + 1,
            reason
        );
        let _ = client.write_all(body.as_bytes());
        let _ = client.flush();
        return;
    }
    // Tunnel open: connect upstream, reply 200, then blind relay both ways.
    let upstream = match parse_host_port(rest)
        .ok_or_else(|| std::io::Error::other("invalid target port"))
        .and_then(|(host, port)| (host.as_str(), port).to_socket_addrs())
        .and_then(|addresses| {
            let mut last = std::io::Error::other("no target addresses");
            for address in addresses.take(8) {
                match TcpStream::connect_timeout(&address, std::time::Duration::from_secs(3)) {
                    Ok(stream) => return Ok(stream),
                    Err(error) => last = error,
                }
            }
            Err(last)
        }) {
        Ok(s) => s,
        Err(e) => {
            let body = format!("HTTP/1.1 502 Bad Gateway\r\nContent-Length: {}\r\n\r\nupstream: {e}\n", (e.to_string().len())+10usize);
            let _ = client.write_all(body.as_bytes());
            return;
        }
    };
    upstream
        .set_read_timeout(Some(std::time::Duration::from_secs(300)))
        .ok();
    let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");
    let _ = client.flush();
    let _ = client.set_read_timeout(Some(std::time::Duration::from_secs(300)));
    relay(client, upstream);
}

/// `host:port` → host. The port must look numeric; anything else refused.
fn parse_host(rest: &str) -> Option<String> {
    let (h, _p) = split_host_port(rest)?;
    Some(h)
}

fn parse_host_port(rest: &str) -> Option<(String, u16)> {
    split_host_port(rest)
}

fn split_host_port(rest: &str) -> Option<(String, u16)> {
    let target = rest.split_whitespace().next()?;
    let target = target.trim_end_matches(['\r', '\n']);
    if let Some(stripped) = target.strip_prefix('[') {
        // IPv6 literal [::1]:443
        let close = stripped.find(']')?;
        let host = &stripped[..close];
        let port = stripped[close + 1..].strip_prefix(':')?;
        return Some((host.to_string(), port.parse().ok()?));
    }
    let (h, p) = target.rsplit_once(':')?;
    let port: u16 = p.parse().ok()?;
    // Reject control characters in the host.
    if h.chars().any(|c| c.is_control()) {
        return None;
    }
    Some((h.to_string(), port))
}

/// Copy bytes both ways until either side closes. Two threads, one per
/// direction; a failed spawn degrades to one-directional relay rather than
/// dropping the tunnel entirely.
fn relay(a: TcpStream, b: TcpStream) {
    // Two independent directions; either end closing terminates its own
    // pump, whose shutdown halves the other side. The join is advisory:
    // process exit reaps leftovers.
    let directed = move |src: TcpStream, dst: TcpStream, name: &'static str| {
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let mut src = src;
                pump(&mut src, dst);
            })
    };
    if let (Ok(a2), Ok(b2)) = (a.try_clone(), b.try_clone()) {
        let _ = directed(b2, a2, "glass-egress-up");
    }
    let mut a = a;
    pump(&mut a, b);
}

fn pump(r: &mut std::net::TcpStream, mut w: std::net::TcpStream) {
    let mut buf = [0u8; 8192];
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if w.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    let _ = w.shutdown(std::net::Shutdown::Both);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::HostPolicy;

    #[test]
    fn host_parsing_rejects_non_numerical_ports_and_control_chars() {
        assert_eq!(
            split_host_port("example.com:443 HTTP/1.1"),
            Some(("example.com".into(), 443))
        );
        assert_eq!(split_host_port("example.com:https "), None);
        assert_eq!(split_host_port("bad\t\x03.com:443"), None);
        assert_eq!(
            split_host_port("[::1]:8443 HTTP/1.1"),
            Some(("::1".into(), 8443))
        );
    }

    #[test]
    fn policy_refusal_names_the_host() {
        let policy = HostPolicy::new(vec!["api.github.com".into()], vec![]);
        let err = policy.check("evil.example.net").unwrap_err();
        assert!(err.contains("evil.example.net"), "{err}");
        let policy = HostPolicy::new(vec!["*.github.com".into()], vec![]);
        policy.check("api.github.com").expect("wildcard ok");
    }

    #[test]
    fn proxy_url_is_loopback_only() {
        let proxy = Proxy::spawn(HostPolicy::new(vec!["127.0.0.1".into()], vec![])).unwrap();
        assert!(proxy.url().starts_with("http://127.0.0.1:"), "{}", proxy.url());
    }
    #[test]
    fn connect_uses_requested_port_and_drop_releases_listener() {
        let upstream = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        upstream.set_nonblocking(true).unwrap();
        let target_port = upstream.local_addr().unwrap().port();
        let proxy = Proxy::spawn(HostPolicy::new(vec!["127.0.0.1".into()], vec![])).unwrap();
        let proxy_port = proxy.port();
        let mut client = TcpStream::connect(("127.0.0.1", proxy_port)).unwrap();
        client.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
        write!(client, "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\n\r\n").unwrap();
        let mut response = [0; 128];
        let count = client.read(&mut response).unwrap();
        assert!(String::from_utf8_lossy(&response[..count]).contains("200 Connection Established"));
        assert!(upstream.accept().is_ok(), "requested endpoint accepted the tunnel");
        drop(client);
        drop(proxy);
        assert!(TcpStream::connect(("127.0.0.1", proxy_port)).is_err());
    }

}
