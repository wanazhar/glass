//! Shared accounting for parent, delegated, side, and warmer requests.
use crate::{
    pricing::RateCard,
    provider::{Error, Event, Provider, Request},
};
use std::sync::{Arc, Mutex};

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Ledger {
    pub spent: f64,
    pub limit: Option<f64>,
    pub unknown: bool,
}
pub type Shared = Arc<Mutex<Ledger>>;
pub struct Metered {
    pub inner: Box<dyn Provider>,
    pub ledger: Shared,
    pub card: Option<RateCard>,
    pub path: Arc<Mutex<std::path::PathBuf>>,
}
impl Provider for Metered {
    fn rate_card(&self) -> Option<RateCard> { self.card }
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn context_window(&self) -> u32 {
        self.inner.context_window()
    }
    fn estimate_tokens(&self, m: &[glass_mod_api::Message]) -> u32 {
        self.inner.estimate_tokens(m)
    }
    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        {
            let l = self.ledger.lock().unwrap();
            if let Some(limit) = l.limit {
                if self.card.is_none() || l.unknown {
                    return Err(Error::Config(
                        "cost limit requires complete pricing and usage; ChatGPT plan usage uses provider quotas, so use /budget off".into(),
                    ));
                }
                if l.spent >= limit {
                    return Err(Error::Config(
                        "session cost limit reached; use /budget to raise it".into(),
                    ));
                }
            }
        }
        let iter = self.inner.stream(req)?;
        Ok(Box::new(Meter {
            iter,
            owner: self,
            usage: None,
        }))
    }
}
struct Meter<'a> {
    iter: Box<dyn Iterator<Item = Event> + 'a>,
    owner: &'a Metered,
    usage: Option<crate::provider::Usage>,
}
impl Iterator for Meter<'_> {
    type Item = Event;
    fn next(&mut self) -> Option<Event> {
        let e = self.iter.next();
        if let Some(Event::Usage(u)) = &e {
            self.usage = Some(*u);
        }
        e
    }
}
impl Drop for Meter<'_> {
    fn drop(&mut self) {
        let mut l = self.owner.ledger.lock().unwrap();
        match (self.owner.card, self.usage) {
            (Some(c), Some(u)) => l.spent += c.cost(&u),
            _ => l.unknown = true,
        }
        if save(&self.owner.path.lock().unwrap(), &l).is_err() {
            l.unknown = true;
        }
    }
}
pub fn load(path: &std::path::Path, limit: Option<f64>) -> Shared {
    let mut l: Ledger = match std::fs::read(path) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|_| Ledger {
            unknown: true,
            ..Default::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ledger::default(),
        Err(_) => Ledger {
            unknown: true,
            ..Default::default()
        },
    };
    if !l.spent.is_finite() || l.spent < 0.0 || l.limit.is_some_and(|n| !n.is_finite() || n <= 0.0)
    {
        l.unknown = true;
    }
    if limit.is_some() {
        l.limit = limit;
    }
    Arc::new(Mutex::new(l))
}

pub fn save(path: &std::path::Path, ledger: &Ledger) -> Result<(), String> {
    let temp = path.with_extension("cost.tmp");
    let data = serde_json::to_vec(ledger).map_err(|e| e.to_string())?;
    std::fs::write(&temp, data).map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Usage;
    struct Fake;
    impl Provider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn context_window(&self) -> u32 {
            100_000
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![
                    Event::Usage(Usage {
                        input: 1_000_000,
                        ..Default::default()
                    }),
                    Event::Done,
                ]
                .into_iter(),
            ))
        }
    }
    fn request<'a>() -> Request<'a> {
        Request {
            model: "fake",
            system: "",
            messages: &[],
            tools: &[],
            max_tokens: 1,
            reasoning: None,
        }
    }
    #[test]
    fn shared_requests_charge_once_and_stop_at_limit_and_survive_reload() {
        let path = std::env::temp_dir().join(format!("glass-cost-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let ledger = load(&path, Some(2.0));
        let card = Some(RateCard {
            input: 1.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.0,
        });
        let a = Metered {
            inner: Box::new(Fake),
            ledger: Arc::clone(&ledger),
            card,
            path: Arc::new(Mutex::new(path.clone())),
        };
        let b = Metered {
            inner: Box::new(Fake),
            ledger: Arc::clone(&ledger),
            card,
            path: Arc::new(Mutex::new(path.clone())),
        };
        for _ in a.stream(&request()).unwrap() {}
        for _ in b.stream(&request()).unwrap() {}
        assert_eq!(ledger.lock().unwrap().spent, 2.0);
        assert!(a.stream(&request()).is_err());
        assert_eq!(load(&path, None).lock().unwrap().spent, 2.0);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn unreadable_ledger_does_not_reset_known_spending_to_zero_silently() {
        let path =
            std::env::temp_dir().join(format!("glass-corrupt-cost-{}.json", std::process::id()));
        std::fs::write(&path, b"{truncated").unwrap();
        assert!(load(&path, Some(1.0)).lock().unwrap().unknown);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unpriced_model_is_refused_under_a_limit() {
        let a = Metered {
            inner: Box::new(Fake),
            ledger: Arc::new(Mutex::new(Ledger {
                limit: Some(1.0),
                ..Default::default()
            })),
            card: None,
            path: Arc::new(Mutex::new(std::env::temp_dir().join("unused-cost.json"))),
        };
        assert!(a.stream(&request()).is_err());
    }
}
