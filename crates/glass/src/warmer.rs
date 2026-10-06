//! Background prompt-cache refresh.
//!
//! Providers expire the prompt cache a few minutes after the last
//! write. On a long task the harness then goes idle — the user is
//! reading output, thinking, away from the keyboard — and the first
//! turn afterwards pays to re-read the whole prefix. A warmer
//! refreshes the cache just before it expires, so that turn is a
//! cache hit instead.
//!
//! This is a latency trade, not a saving: a cache *write* costs
//! 1.25x the prefix while a *miss* costs 1.0x, so each refresh is
//! a net spend of about 0.35x the prefix. It is worth enabling only
//! when a cold start is felt, which is why it ships disabled.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::ext::protocol::WarmerPolicy;
use crate::provider::{Provider, Request};
use glass_mod_api::Message;

/// Builds a fresh provider. The live one cannot cross a thread
/// boundary — it is not `Send` — but a provider is cheap to
/// construct from configuration, so the warmer makes its own
/// rather than sharing the turn's.
pub type ProviderFactory =
    Arc<dyn Fn() -> Result<Box<dyn Provider>, String> + Send + Sync>;

/// The cacheable prefix of one turn, owned so a background
/// thread can use it after the turn is gone.
pub struct CacheSnapshot {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<(String, String, String)>,
}

/// Schedules prompt-cache refreshes while the harness sits idle.
pub struct Warmer {
    factory: ProviderFactory,
    /// Bumped every turn. A refresh scheduled for a turn that has
    /// since been superseded does nothing, so a warm can never race
    /// a real request or refresh a prefix the user has already
    /// moved past.
    generation: Arc<AtomicU64>,
    /// How many refreshes were scheduled. Observation only: a
    /// test asserts a turn caused one without waiting for the
    /// refresh itself to fire.
    scheduled: Arc<AtomicU64>,
}

impl Warmer {
    pub fn new(factory: ProviderFactory) -> Self {
        Self {
            factory,
            generation: Arc::new(AtomicU64::new(0)),
            scheduled: Arc::new(AtomicU64::new(0)),
        }
    }

    /// How many refreshes have been scheduled so far.
    /// Observation only, so a test can confirm a turn
    /// scheduled a refresh without waiting for it to fire.
    #[cfg(test)]
    pub fn scheduled(&self) -> u64 {
        self.scheduled.load(Ordering::SeqCst)
    }

    /// Mark a new turn as started, superseding any pending refresh.
    pub fn begin_turn(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Refresh the cache `policy.refresh_after_s` from now, in the
    /// background. Returns immediately.
    pub fn schedule(&self, policy: WarmerPolicy, snapshot: CacheSnapshot) {
        self.scheduled.fetch_add(1, Ordering::SeqCst);
        let my_gen = self.generation.load(Ordering::SeqCst);
        let generation = Arc::clone(&self.generation);
        let factory = Arc::clone(&self.factory);
        let spawned = std::thread::Builder::new()
            .name("glass-cache-warmer".into())
            .spawn(move || {
                let deadline =
                    Instant::now() + Duration::from_secs(policy.refresh_after_s);
                // Sleep in slices so a new turn cancels the refresh
                // promptly instead of after the full delay.
                while Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(200));
                    if generation.load(Ordering::SeqCst) != my_gen {
                        return;
                    }
                }
                if generation.load(Ordering::SeqCst) != my_gen {
                    return;
                }
                warm(&factory, &snapshot);
            });
        // A failed spawn only costs latency on the next turn, so it
        // is deliberately not surfaced.
        let _ = spawned;
    }
}

/// Send a minimal request that re-writes the cached prefix.
///
/// The request is identical to a real turn's — same system, tools,
/// and messages, so every breakpoint lands where the provider put it —
/// but caps output at a single token. The response is discarded; the
/// point is the cache write, not the answer.
fn warm(factory: &ProviderFactory, snapshot: &CacheSnapshot) {
    let Ok(provider) = factory() else { return };
    let model = String::new();
    let req = Request {
        model: &model,
        system: &snapshot.system,
        messages: &snapshot.messages,
        tools: &snapshot.tools,
        max_tokens: 1,
        reasoning: None,
    };
    // A failure here is silent by design: a missed refresh only
    // costs latency on the next turn, never correctness.
    if let Ok(stream) = provider.stream(&req) {
        for _ in stream {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Error, Event};
    use std::sync::atomic::AtomicU64;

    /// A provider that counts how many times it was asked to
    /// stream, so a test can see whether a refresh happened.
    struct CountingProvider {
        calls: Arc<AtomicU64>,
    }

    impl Provider for CountingProvider {
        fn id(&self) -> &str {
            "counting"
        }
        fn stream(
            &self,
            _req: &Request<'_>,
        ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // A warm only needs the request to reach the provider;
            // the response is discarded.
            Ok(Box::new(vec![Event::Done].into_iter()))
        }
        fn context_window(&self) -> u32 {
            200_000
        }
    }

    fn factory(calls: Arc<AtomicU64>) -> ProviderFactory {
        std::sync::Arc::new(move || {
            Ok(Box::new(CountingProvider {
                calls: Arc::clone(&calls),
            }))
        })
    }

    fn snapshot() -> CacheSnapshot {
        CacheSnapshot {
            system: "you are glass".into(),
            messages: vec![Message::User("do a thing".into())],
            tools: vec![],
        }
    }

    fn policy(refresh_after_s: u64) -> WarmerPolicy {
        WarmerPolicy {
            refresh_after_s,
            min_prefix_bytes: 1, // never gate the test on size
        }
    }

    #[test]
    fn a_scheduled_refresh_reaches_the_provider() {
        let calls = Arc::new(AtomicU64::new(0));
        let warmer = Warmer::new(factory(Arc::clone(&calls)));
        warmer.schedule(policy(1), snapshot());

        // The refresh fires one second out, then the request
        // has to reach the provider.
        std::thread::sleep(Duration::from_millis(1600));
        assert!(
            calls.load(Ordering::SeqCst) >= 1,
            "no refresh was sent"
        );
    }

    #[test]
    fn a_new_turn_cancels_a_pending_refresh() {
        let calls = Arc::new(AtomicU64::new(0));
        let warmer = Warmer::new(factory(Arc::clone(&calls)));
        warmer.schedule(policy(1), snapshot());

        // The user comes back and starts a new turn before the
        // refresh would fire. That turn rewrites the cache
        // itself, so the scheduled refresh must not run.
        warmer.begin_turn();
        std::thread::sleep(Duration::from_millis(1600));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "a superseded refresh still reached the provider"
        );
    }

    #[test]
    fn the_refresh_sends_the_whole_prefix() {
        // The refresh must re-send system, tools and messages so
        // every breakpoint lands where a real turn put it. A
        // refresh that dropped the conversation would only warm
        // the stable prefix and leave the bulk of a long task cold.
        use std::sync::mpsc;

        struct CapturingProvider {
            tx: mpsc::Sender<(String, usize)>,
        }
        impl Provider for CapturingProvider {
            fn id(&self) -> &str {
                "capturing"
            }
            fn stream(
                &self,
                req: &Request<'_>,
            ) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
                let _ = self.tx.send((req.system.to_string(), req.messages.len()));
                Ok(Box::new(vec![Event::Done].into_iter()))
            }
            fn context_window(&self) -> u32 {
                200_000
            }
        }

        let (tx, rx) = mpsc::channel();
        let cap_factory: ProviderFactory = std::sync::Arc::new(move || {
            Ok(Box::new(CapturingProvider {
                tx: tx.clone(),
            }))
        });

        let warmer = Warmer::new(cap_factory);
        warmer.schedule(policy(1), snapshot());
        std::thread::sleep(Duration::from_millis(1600));

        let (system, messages) = rx.recv_timeout(Duration::from_millis(200)).unwrap();
        assert_eq!(system, "you are glass");
        assert_eq!(messages, 1, "the conversation must be refreshed too");
    }
}
