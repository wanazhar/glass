//! Automatic model fallback.
//!
//! A chain exists for the failures a retry cannot fix quickly: the primary
//! model is hard rate-limited, overloaded, or gone from the account. The
//! chain tries the primary, then each configured fallback in order, and the
//! first provider whose *setup* succeeds serves the request.
//!
//! Fallback is limited to explicit HTTP responses. Transport errors may occur
//! after a billable request was accepted and are never replayed automatically.
//! HTTP 401/403 and configuration failures stop the chain at any level.
//!
//! The serving fallback is visible: the stream opens with a `Warning` naming
//! it, because a silently swapped model is a debugging trap when outputs and
//! prices differ.

use glass_mod_api::Message;

use super::{Error, Event, Provider, Request};

/// One provider per model, in preference order (index 0 is the primary).
pub struct Chain {
    levels: Vec<Box<dyn Provider>>,
    /// The model id the primary was built for, reported by `id()`.
    primary: String,
    /// Sticky serving index, set when a fallback answers. Requests until the
    /// next `unstick()` reuse that model instead of probing the failing
    /// primary every round (pi's sticky virtual-model routing). A user-set
    /// selection lives above this type: the provider is rebuilt on `/model`,
    /// which constructs a fresh chain and therefore resets stickiness.
    sticky: std::cell::Cell<usize>,
}

impl Chain {
    pub fn new(primary: String, levels: Vec<Box<dyn Provider>>) -> Self {
        Self {
            levels,
            primary,
            sticky: std::cell::Cell::new(0),
        }
    }

    /// Whether a failed attempt may be routed around.
    fn worth_falling_back(e: &Error) -> bool {
        !matches!(e, Error::Status(401 | 403, _) | Error::Config(_) | Error::Transport(_))
    }

    /// Start fresh: the primary is probed again. The live harness rebuilds
    /// the chain for every user turn (`factory()` at turn dispatch), which
    /// resets stickiness by construction; this explicit reset exists for
    /// callers that reuse one chain across turn sequences, and for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn unstick(&self) {
        self.sticky.set(0);
    }

    /// The model id actually answering, the primary's when healthy.
    /// Diagnostic surface for tests and the recovery inspector's provider
    /// identity record.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn serving_model(&self) -> &str {
        self.levels
            .get(self.sticky.get())
            .map(|p| p.model())
            .unwrap_or(&self.primary)
    }
}

impl Provider for Chain {
    fn id(&self) -> &str {
        &self.primary
    }

    fn model(&self) -> &str { self.serving_model() }

    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        let mut first_error: Option<Error> = None;
        for (index, provider) in self.levels.iter().enumerate() {
            // Stickiness: once a fallback has served this turn's requests,
            // keep probing from there rather than failing the primary each
            // round. Index 0 is never sticky, so the healthy path is
            // unchanged; and skip-to-sticky only advances from a level that
            // already produced an Ok stream once.
            if index < self.sticky.get() {
                continue;
            }
            let model = provider.model();
            // The request is rebuilt per level: the model id travels in the
            // request, and sending the primary's id to a fallback adapter
            // would ask the wrong model to answer.
            let rebuilt = Request {
                model,
                system: req.system,
                messages: req.messages,
                tools: req.tools,
                max_tokens: req.max_tokens,
                reasoning: req.reasoning,
            };
            match provider.stream(&rebuilt) {
                Ok(stream) if index == 0 => return Ok(stream),
                Ok(stream) => {
                    self.sticky.set(index);
                    let why = first_error
                        .as_ref()
                        .map(|e| e.to_string())
                        .unwrap_or_default();
                    let warning = Event::Warning(format!(
                        "model `{}` unavailable ({why}); using `{model}`",
                        self.primary
                    ));
                    return Ok(Box::new(std::iter::once(warning).chain(stream)));
                }
                Err(e) => {
                    if !Self::worth_falling_back(&e) {
                        return Err(e);
                    }
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }
        Err(first_error.unwrap_or_else(|| Error::Config("fallback chain is empty".into())))
    }

    fn rate_card(&self) -> Option<crate::pricing::RateCard> {
        self.levels.get(self.sticky.get()).and_then(|p| p.rate_card())
    }

    fn context_window(&self) -> u32 {
        // A request may fall back before the parent can observe the serving model.
        self.levels.iter().skip(self.sticky.get()).map(|p| p.context_window()).min().unwrap_or(0)
    }

    fn estimate_tokens(&self, messages: &[Message]) -> u32 {
        self.levels.iter().skip(self.sticky.get()).map(|p| p.estimate_tokens(messages)).max()
            .unwrap_or_else(|| super::default_estimate(messages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    enum Reply {
        Text(&'static str),
        Fail(fn() -> Error),
    }

    /// Shared counters, cloned out before the stub moves into the chain.
    #[derive(Clone)]
    struct Handle {
        calls: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl Handle {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn seen(&self) -> Vec<String> {
            self.seen.lock().unwrap().clone()
        }
    }

    struct Stub {
        id: &'static str,
        reply: Reply,
        handle: Handle,
    }

    impl Stub {
        fn new(id: &'static str, reply: Reply) -> Self {
            Self {
                id,
                reply,
                handle: Handle {
                    calls: Arc::new(AtomicUsize::new(0)),
                    seen: Arc::new(Mutex::new(Vec::new())),
                },
            }
        }

        fn ok(id: &'static str, text: &'static str) -> Self {
            Self::new(id, Reply::Text(text))
        }

        fn failing(id: &'static str, make: fn() -> Error) -> Self {
            Self::new(id, Reply::Fail(make))
        }

        fn handle(&self) -> Handle {
            self.handle.clone()
        }
    }

    impl Provider for Stub {
        fn id(&self) -> &str {
            self.id
        }

        fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            self.handle.calls.fetch_add(1, Ordering::SeqCst);
            self.handle
                .seen
                .lock()
                .unwrap()
                .push(req.model.to_string());
            match &self.reply {
                Reply::Text(t) => Ok(Box::new(
                    [Event::Text((*t).to_string()), Event::Done].into_iter(),
                )),
                Reply::Fail(make) => Err(make()),
            }
        }

        fn context_window(&self) -> u32 {
            100_000
        }
    }

    fn req() -> Request<'static> {
        Request {
            model: "primary",
            system: "",
            messages: &[],
            tools: &[],
            max_tokens: 16,
            reasoning: None,
        }
    }

    fn events(chain: &Chain) -> Result<Vec<Event>, Error> {
        chain.stream(&req()).map(|s| s.collect())
    }

    #[test]
    fn fallback_context_and_pricing_follow_safe_serving_limits() {
        struct Sized { window: u32, estimate: u32, rate: f64, fail: bool }
        impl Provider for Sized {
            fn id(&self) -> &str { "sized" }
            fn context_window(&self) -> u32 { self.window }
            fn estimate_tokens(&self, _: &[Message]) -> u32 { self.estimate }
            fn rate_card(&self) -> Option<crate::pricing::RateCard> {
                Some(crate::pricing::RateCard { input: self.rate, output: self.rate, cache_read: self.rate, cache_write: self.rate })
            }
            fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item=Event> + '_>, Error> {
                if self.fail { Err(Error::Status(429, "busy".into())) }
                else { Ok(Box::new([Event::Done].into_iter())) }
            }
        }
        let chain = Chain::new("primary".into(), vec![
            Box::new(Sized { window:200_000, estimate:10, rate:5.0, fail:true }),
            Box::new(Sized { window:32_000, estimate:20, rate:1.0, fail:false }),
        ]);
        assert_eq!(chain.context_window(), 32_000);
        assert_eq!(chain.estimate_tokens(&[]), 20);
        assert_eq!(chain.rate_card().unwrap().input, 5.0);
        events(&chain).unwrap();
        assert_eq!(chain.rate_card().unwrap().input, 1.0);
        assert_eq!(chain.context_window(), 32_000);
    }

    #[test]
    fn a_healthy_primary_serves_without_a_warning() {
        let primary = Stub::ok("primary", "hello");
        let h = primary.handle();
        let chain = Chain::new(
            "primary".into(),
            vec![Box::new(primary), Box::new(Stub::ok("backup", "x"))],
        );
        let out = events(&chain).unwrap();
        assert!(matches!(out[0], Event::Text(ref t) if t == "hello"));
        assert!(!out.iter().any(|e| matches!(e, Event::Warning(_))));
        assert_eq!(h.calls(), 1);
    }

    #[test]
    fn a_transient_failure_falls_through_and_is_named() {
        let primary = Stub::failing("primary", || Error::Status(429, "overloaded".into()));
        let backup = Stub::ok("backup", "from backup");
        let ph = primary.handle();
        let bh = backup.handle();
        let chain = Chain::new("primary".into(), vec![Box::new(primary), Box::new(backup)]);
        let out = events(&chain).unwrap();
        match &out[0] {
            Event::Warning(w) => {
                assert!(w.contains("primary"), "{w}");
                assert!(w.contains("429"), "{w}");
                assert!(w.contains("backup"), "{w}");
            }
            other => panic!("expected a warning first, got {other:?}"),
        }
        assert!(matches!(&out[1], Event::Text(t) if t == "from backup"));
        assert_eq!(ph.calls(), 1, "the primary is attempted exactly once");
        assert_eq!(
            bh.seen(),
            vec!["backup"],
            "the fallback must be asked for its own model"
        );
    }

    #[test]
    fn credentials_are_never_routed_around() {
        let primary = Stub::failing("primary", || Error::Status(401, "bad key".into()));
        let backup = Stub::ok("backup", "x");
        let bh = backup.handle();
        let chain = Chain::new("primary".into(), vec![Box::new(primary), Box::new(backup)]);
        let err = events(&chain).unwrap_err();
        assert!(err.to_string().contains("401"), "{err}");
        assert_eq!(bh.calls(), 0, "an auth failure must not try another model");
    }

    #[test]
    fn everything_failing_reports_the_primary_error() {
        let chain = Chain::new(
            "primary".into(),
            vec![
                Box::new(Stub::failing("primary", || Error::Status(429, "busy".into()))),
                Box::new(Stub::failing("backup", || Error::Status(500, "sad".into()))),
            ],
        );
        let err = events(&chain).unwrap_err();
        assert!(
            err.to_string().contains("429"),
            "the primary's failure is the actionable one: {err}"
        );
    }

    #[test]
    fn identity_and_window_come_from_the_primary() {
        let chain = Chain::new("primary".into(), vec![Box::new(Stub::ok("primary", "x"))]);
        assert_eq!(chain.id(), "primary");
        assert_eq!(chain.context_window(), 100_000);
    }

    #[test]
    fn a_failure_on_a_later_level_keeps_the_first_error() {
        // Even when a fallback fails for its own reasons, the report should
        // describe why fallback started — the primary's failure.
        let chain = Chain::new(
            "primary".into(),
            vec![
                Box::new(Stub::failing("primary", || Error::Status(503, "overloaded".into()))),
                Box::new(Stub::failing("backup", || Error::Status(502, "upstream".into()))),
            ],
        );
        let err = events(&chain).unwrap_err().to_string();
        assert!(err.contains("503"), "{err}");
    }

    #[test]
    fn ambiguous_transport_failure_stops_primary_and_later_fallbacks() {
        for failed_level in [0, 1] {
            let primary = if failed_level == 0 { Stub::failing("primary", || Error::Transport("POST accepted".into())) }
                else { Stub::failing("primary", || Error::Status(429,"busy".into())) };
            let backup = if failed_level == 1 { Stub::failing("backup", || Error::Transport("POST accepted".into())) }
                else { Stub::ok("backup", "must not run") };
            let backup_handle = backup.handle();
            let third = Stub::ok("third", "must not run");
            let third_handle = third.handle();
            let chain = Chain::new("primary".into(), vec![Box::new(primary),Box::new(backup),Box::new(third)]);
            assert!(matches!(events(&chain), Err(Error::Transport(_))));
            assert_eq!(backup_handle.calls(), usize::from(failed_level == 1));
            assert_eq!(third_handle.calls(), 0);
        }
    }

    #[test]
    fn fallback_authentication_errors_cannot_route_to_another_model() {
        let primary = Stub::failing("primary", || Error::Status(429, "busy".into()));
        let backup = Stub::failing("backup", || Error::Status(403, "denied".into()));
        let third = Stub::ok("third", "must not run");
        let handle = third.handle();
        let chain = Chain::new("primary".into(), vec![Box::new(primary),Box::new(backup),Box::new(third)]);
        assert!(matches!(events(&chain), Err(Error::Status(403,_))));
        assert_eq!(handle.calls(), 0);
    }

    #[test]
    fn sticky_routing_skips_a_known_bad_primary_within_a_chain() {
        let primary = Stub::failing("primary", || Error::Status(503, "overloaded".into()));
        let h = primary.handle().clone();
        let backup = Stub::ok("backup", "b");
        let bh = backup.handle().clone();
        let chain = Chain::new(
            "primary".into(),
            vec![Box::new(primary), Box::new(backup)],
        );
        // First request: falls to backup (1 primary attempt).
        events(&chain).expect("backup answers");
        assert_eq!(h.calls(), 1);
        // Second request: sticky — no new primary attempt.
        events(&chain).expect("backup answers again");
        assert_eq!(h.calls(), 1, "primary must not be re-probed while sticky");
        assert_eq!(bh.calls(), 2);
        // unstick re-opens probing: the primary is attempted (and fails)
        // again, then the backup serves as before.
        chain.unstick();
        events(&chain).expect("backup answers after unstick");
        assert_eq!(h.calls(), 2, "unstick must re-probe the primary");
    }

    #[test]
    fn serving_model_reflects_the_sticky_level() {
        let chain = Chain::new(
            "primary".into(),
            vec![
                Box::new(Stub::failing("primary", || Error::Status(503, "overloaded".into()))),
                Box::new(Stub::ok("backup", "b")),
            ],
        );
        assert_eq!(chain.serving_model(), "primary");
        let _ = events(&chain).expect("backup");
        assert_eq!(chain.serving_model(), "backup");
        chain.unstick();
        assert_eq!(chain.serving_model(), "primary");
    }
}
