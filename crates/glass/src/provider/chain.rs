//! Automatic model fallback.
//!
//! A chain exists for the failures a retry cannot fix quickly: the primary
//! model is hard rate-limited, overloaded, or gone from the account. The
//! chain tries the primary, then each configured fallback in order, and the
//! first provider whose *setup* succeeds serves the request.
//!
//! The safety rule is the same one `stream_with_retry` relies on: fall
//! through only when the failed attempt produced no events, which is exactly
//! what `Provider::stream` returning `Err` means. An HTTP 401 or 403 is
//! never routed around — that is a credential problem, and the next model
//! would fail the same way.
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
}

impl Chain {
    pub fn new(primary: String, levels: Vec<Box<dyn Provider>>) -> Self {
        Self { levels, primary }
    }

    /// Whether a failed attempt may be routed around.
    fn worth_falling_back(e: &Error) -> bool {
        !matches!(e, Error::Status(401 | 403, _) | Error::Config(_))
    }
}

impl Provider for Chain {
    fn id(&self) -> &str {
        &self.primary
    }

    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        let mut first_error: Option<Error> = None;
        for (index, provider) in self.levels.iter().enumerate() {
            let model = provider.id();
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
                    if index == 0 && !Self::worth_falling_back(&e) {
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

    fn context_window(&self) -> u32 {
        self.levels.first().map(|p| p.context_window()).unwrap_or(0)
    }

    fn estimate_tokens(&self, messages: &[Message]) -> u32 {
        match self.levels.first() {
            Some(p) => p.estimate_tokens(messages),
            None => super::default_estimate(messages),
        }
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
                Box::new(Stub::failing("primary", || Error::Transport("t".into()))),
                Box::new(Stub::failing("backup", || Error::Status(403, "nope".into()))),
            ],
        );
        let err = events(&chain).unwrap_err().to_string();
        assert!(err.contains("transport"), "{err}");
    }
}
