//! Account-scoped burst control. Local tools never pass through this gate.
use super::Error;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct State {
    pub(super) last: Option<Instant>,
    pub(super) previous: Option<Instant>,
    pub(super) cooldown: Option<Instant>,
    pub(super) server_cooldown: Option<Instant>,
    pub(super) gap: Duration,
    pub(super) healthy: u8,
}
static STATES: OnceLock<Mutex<HashMap<[u8; 32], State>>> = OnceLock::new();
fn states() -> &'static Mutex<HashMap<[u8; 32], State>> {
    STATES.get_or_init(Mutex::default)
}
fn key(endpoint: &str, credential: &str) -> [u8; 32] {
    let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
    digest.update(endpoint.as_bytes());
    digest.update(&[0]);
    digest.update(credential.as_bytes());
    digest.finish().as_ref().try_into().expect("SHA256 size")
}
pub(super) fn acquire(endpoint: &str, credential: &str, adaptive: bool) -> Result<[u8; 32], Error> {
    let key = key(endpoint, credential);
    let mut notified = false;
    loop {
        if super::cancellable::is_cancelled() {
            return Err(Error::Transport(
                "request cancelled while waiting for provider cooldown".into(),
            ));
        }
        let wait = {
            let mut shared = super::pacing_shared::Shared::open(key)?;
            let mut all = states().lock().unwrap_or_else(|e| e.into_inner());
            // Expire idle accounts; never retain credentials or unbounded account history.
            if all.len() >= 128 {
                all.retain(|_, s| {
                    s.last
                        .is_some_and(|t| t.elapsed() < Duration::from_secs(600))
                        || s.cooldown.is_some_and(|t| t > Instant::now())
                        || s.server_cooldown.is_some_and(|t| t > Instant::now())
                });
            }
            if all.len() >= 128 && !all.contains_key(&key) {
                return Err(Error::Config(
                    "too many active provider accounts for request pacing".into(),
                ));
            }
            let state = all.entry(key).or_default();
            if let Some(shared) = &shared { shared.load(state); }
            let now = Instant::now();
            // Old pressure must not make a fresh session slow indefinitely.
            if state.last.is_some_and(|last| now.duration_since(last) >= Duration::from_secs(300))
                && state.cooldown.is_none_or(|deadline| deadline <= now) {
                state.gap = Duration::ZERO;
                state.healthy = 0;
                state.previous = None;
            }
            let burst = state
                .previous
                .is_some_and(|t| now.duration_since(t) < Duration::from_secs(1));
            let gap = if adaptive { state.gap.max(if burst {
                Duration::from_millis(200)
            } else {
                Duration::ZERO
            }) } else { Duration::ZERO };
            let ready = state
                .last
                .map(|t| t + gap)
                .into_iter()
                .chain(if adaptive { state.cooldown } else { None })
                .chain(state.server_cooldown)
                .max()
                .unwrap_or(now);
            if ready <= now {
                state.previous = state.last;
                state.last = Some(now);
                if let Some(shared) = &mut shared { shared.save(state)?; }
                Duration::ZERO
            } else {
                ready.duration_since(now)
            }
        };
        if wait.is_zero() {
            return Ok(key);
        }
        if !notified && wait >= Duration::from_millis(500) {
            NOTICE.with(|slot| { if let Some(notice) = slot.borrow().as_ref() { notice(format!("Provider cooldown · {:.1}s · waiting before next request", wait.as_secs_f64())); } });
            notified = true;
        }
        // Poll cancellation cheaply; reopen shared state at most once per
        // five seconds during long cooldowns, or at the precise ready time.
        let deadline = Instant::now() + wait.min(Duration::from_secs(5));
        while Instant::now() < deadline {
            if super::cancellable::is_cancelled() {
                return Err(Error::Transport("provider pacing cancelled".into()));
            }
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()).min(Duration::from_millis(25)));
        }
    }
}
fn duration(value: &str) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<f64>() {
        return (seconds.is_finite() && (0.0..=86400.0).contains(&seconds))
            .then(|| Duration::from_secs_f64(seconds));
    }
    let mut total = 0.0;
    let mut rest = value;
    while !rest.is_empty() {
        let end = rest.find(|c: char| !c.is_ascii_digit() && c != '.')?;
        let number: f64 = rest[..end].parse().ok()?;
        rest = &rest[end..];
        let (unit, factor) = if rest.starts_with("ms") {
            (2, 0.001)
        } else if rest.starts_with('s') {
            (1, 1.0)
        } else if rest.starts_with('m') {
            (1, 60.0)
        } else if rest.starts_with('h') {
            (1, 3600.0)
        } else {
            return None;
        };
        total += number * factor;
        rest = &rest[unit..];
    }
    (total.is_finite() && (0.0..=86400.0).contains(&total)).then(|| Duration::from_secs_f64(total))
}
pub(super) fn observe(key: [u8; 32], status: u16, header: impl Fn(&str) -> Option<String>) {
    let retry = header("retry-after").and_then(|v| {
        duration(&v).or_else(|| {
            httpdate::parse_http_date(&v).ok().map(|t| {
                t.duration_since(std::time::SystemTime::now())
                    .unwrap_or_default().min(Duration::from_secs(86400))
            })
        })
    });
    let exhausted = header("x-ratelimit-remaining-requests").is_some_and(|v| v.trim() == "0");
    let reset = exhausted
        .then(|| header("x-ratelimit-reset-requests").and_then(|v| duration(&v)))
        .flatten();
    let mut shared = super::pacing_shared::Shared::open(key).ok().flatten();
    let mut all = states().lock().unwrap_or_else(|e| e.into_inner());
    let state = all.entry(key).or_default();
    if let Some(shared) = &shared { shared.load(state); }
    if status == 429 || status >= 500 {
        state.gap = (state.gap.max(Duration::from_secs(1)) * 2).min(Duration::from_secs(60));
        state.healthy = 0;
        let delay = retry.unwrap_or(state.gap).max(state.gap);
        state.cooldown = Some(
            state
                .cooldown
                .unwrap_or_else(Instant::now)
                .max(Instant::now() + delay),
        );
    } else if (200..300).contains(&status) {
        state.healthy = state.healthy.saturating_add(1);
        if state.healthy >= 8 {
            state.gap /= 2;
            state.healthy = 0;
            if state.gap < Duration::from_millis(100) {
                state.gap = Duration::ZERO;
            }
        }
    }
    if let Some(delay) = retry.into_iter().chain(reset).max() {
        state.server_cooldown = Some(
            state.server_cooldown.unwrap_or_else(Instant::now).max(Instant::now() + delay),
        );
    }
    if let Some(shared) = &mut shared { let _ = shared.save(state); }
}
/// Status tags originate only from a structured provider error frame.
pub(crate) fn stream_status(message: &str) -> Option<u16> {
    let status = message.strip_prefix("provider rejected stream (HTTP ")?.split_once("):")?.0.parse::<u16>().ok()?;
    (400..600).contains(&status).then_some(status)
}

/// Only explicit throttling is safe to retry; quota and billing failures are not.
pub(crate) fn rate_limited(message: &str) -> bool {
    let text = message.to_ascii_lowercase();
    if ["insufficient_quota", "quota exceeded", "credits", "billing", "usage limit"].iter().any(|s| text.contains(s)) { return false; }
    ["rate limit", "rate_limit", "too many requests", "too_many_requests"].iter().any(|s| text.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_pressure_is_recoverable() {
        assert!(rate_limited("rate_limit_error"));
        assert!(rate_limited("Too many requests"));
        assert!(!rate_limited("rate limit: insufficient_quota"));
        assert!(!rate_limited("socket closed"));
    }
    #[test]
    fn reset_durations() {
        assert_eq!(duration("1m2.5s"), Some(Duration::from_millis(62500)));
        assert_eq!(duration("125ms"), Some(Duration::from_millis(125)));
        assert!(duration("NaN").is_none());
        assert!(duration("-1").is_none());
    }
    #[test]
    fn idle_accounts_recover_the_fast_path() {
        let k=key("idle-pacing-test", "");
        states().lock().unwrap().insert(k, State { last: Some(Instant::now()-Duration::from_secs(600)), gap: Duration::from_secs(60), ..State::default() });
        acquire("idle-pacing-test", "", true).unwrap();
        assert_eq!(states().lock().unwrap()[&k].gap, Duration::ZERO);
    }
    #[test]
    fn accounts_are_isolated() {
        assert_ne!(key("url", "a"), key("url", "b"));
    }
    #[test]
    fn pressure_learns_and_recovers() {
        let k = key("test-pressure", "test");
        observe(k, 429, |_| None);
        assert_eq!(states().lock().unwrap()[&k].gap, Duration::from_secs(2));
        for _ in 0..8 {
            observe(k, 200, |_| None);
        }
        assert_eq!(states().lock().unwrap()[&k].gap, Duration::from_secs(1));
    }
}

type Notice = Option<Box<dyn Fn(String)>>;
thread_local! { static NOTICE: std::cell::RefCell<Notice> = const { std::cell::RefCell::new(None) }; }
pub(crate) fn with_notice<T>(notice: impl Fn(String) + 'static, f: impl FnOnce() -> T) -> T {
    struct Restore(Notice);
    impl Drop for Restore { fn drop(&mut self) { NOTICE.with(|slot| *slot.borrow_mut() = self.0.take()); } }
    let _restore = Restore(NOTICE.with(|slot| slot.replace(Some(Box::new(notice)))));
    f()
}

#[cfg(test)] mod gate_tests {
    use super::*;
    #[test] fn retry_after_is_preserved() {
        let k = key("test-server-delay", "test");
        observe(k, 429, |name| (name == "retry-after").then(|| "2".into()));
        assert!(states().lock().unwrap()[&k].cooldown.unwrap().duration_since(Instant::now()) > Duration::from_millis(1900));
    }
    #[test] fn disabling_adaptation_skips_learned_pressure() {
        let k = key("off-pressure", "");
        observe(k, 429, |_| None);
        let start = Instant::now();
        acquire("off-pressure", "", false).unwrap();
        assert!(start.elapsed() < Duration::from_millis(100));
        assert!(states().lock().unwrap()[&k].cooldown.is_some());
    }
    #[test] fn disabling_adaptation_preserves_server_deadline() {
        let k = key("off-server", "");
        observe(k, 429, |name| (name == "retry-after").then(|| "0.12".into()));
        let start = Instant::now();
        acquire("off-server", "", false).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test] fn cancelled_gate_does_not_send() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        super::super::cancellable::scoped(flag, || assert!(acquire("cancel-test", "", true).is_err()));
    }
}
