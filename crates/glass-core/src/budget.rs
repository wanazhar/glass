//! Shared accounting for parent, delegated, side, and warmer requests.
use crate::{
    pricing::RateCard,
    provider::{Error, Event, Provider, Request},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard},
};

const MAX_LEDGER_BYTES: u64 = 1024 * 1024;
const MAX_OUTSTANDING_REQUESTS: usize = 4096;
/// The first coordinated Core runtime has no recursive child execution. Keep
/// a hard lifetime ceiling for child sessions when a cost limit opts into the
/// shared root budget; legacy sessions without a configured limit are unchanged.
pub const MAX_ROOT_CHILDREN: usize = 128;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Ledger {
    pub spent: f64,
    pub limit: Option<f64>,
    pub unknown: bool,
    /// Outstanding request reservations survive restart. They are never
    /// silently released because the process that owned them disappeared.
    reservations: BTreeMap<u64, f64>,
    next_reservation_id: u64,
    /// Child IDs are opaque Core-generated identifiers, not prompt content.
    /// Keeping them makes child admission idempotent across retries/restarts.
    child_tasks: BTreeSet<String>,
}
pub type Shared = Arc<Mutex<Ledger>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reservation {
    id: u64,
}

pub struct Metered {
    pub inner: Box<dyn Provider>,
    pub ledger: Shared,
    pub card: Option<RateCard>,
    pub path: Arc<Mutex<std::path::PathBuf>>,
}

/// Canonical in-memory and on-disk budget authority for one Core session.
/// Clone this value when constructing parent, child, completion, or compaction
/// providers; do not call `load` again for the same live session.
#[derive(Clone)]
pub struct Context {
    ledger: Shared,
    path: Arc<Mutex<std::path::PathBuf>>,
}

impl Context {
    pub fn load(path: &std::path::Path, limit: Option<f64>) -> Self {
        Self {
            ledger: load(path, limit),
            path: Arc::new(Mutex::new(path.to_path_buf())),
        }
    }

    pub fn from_parts(ledger: Shared, path: Arc<Mutex<std::path::PathBuf>>) -> Self {
        Self { ledger, path }
    }

    pub fn ledger(&self) -> Shared {
        Arc::clone(&self.ledger)
    }

    pub fn path(&self) -> Arc<Mutex<std::path::PathBuf>> {
        Arc::clone(&self.path)
    }

    pub fn set_limit(&self, limit: Option<f64>) {
        if let Some(limit) = limit {
            lock(&self.ledger).limit = Some(limit);
        }
    }

    /// Shared metering constructor used by Core's provider factory and by
    /// tests that exercise parent/child providers from the same scope.
    pub fn metered(&self, inner: Box<dyn Provider>, card: Option<RateCard>) -> Metered {
        Metered {
            inner,
            ledger: self.ledger(),
            card,
            path: self.path(),
        }
    }

    pub fn provider(
        &self,
        cfg: &crate::config::Config,
        model: Option<&str>,
    ) -> Result<Box<dyn Provider>, String> {
        crate::make_provider(cfg, self.ledger(), self.path(), model)
    }
}

impl Ledger {
    fn reserved(&self) -> f64 {
        self.reservations.values().copied().sum()
    }

    fn remaining(&self) -> Option<f64> {
        self.limit
            .map(|limit| (limit - self.spent - self.reserved()).max(0.0))
    }
}

/// Recover a poisoned ledger without treating partially updated spending as
/// known. A configured cost limit will then refuse metering until the user
/// reloads or explicitly changes the budget state.
pub fn lock(ledger: &Shared) -> MutexGuard<'_, Ledger> {
    match ledger.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            guard.unknown = true;
            ledger.clear_poison();
            guard
        }
    }
}

pub fn spent_label(ledger: &Ledger) -> String {
    if ledger.unknown {
        "unavailable".into()
    } else {
        crate::pricing::money(ledger.spent)
    }
}

/// Reserve the defensible maximum cost for this request before dispatch.
/// Input is bounded by the provider's context window and output by the
/// request's explicit max_tokens. Cache billing uses the most expensive prompt
/// rate, so unknown cache placement cannot understate the reservation.
fn request_cost_bound(card: RateCard, context_window: u32, max_tokens: u32) -> Option<f64> {
    let rates = [card.input, card.cache_read, card.cache_write, card.output];
    if rates.iter().any(|rate| !rate.is_finite() || *rate < 0.0) {
        return None;
    }
    let prompt_rate = card.input.max(card.cache_read).max(card.cache_write);
    let amount = (f64::from(context_window) * prompt_rate + f64::from(max_tokens) * card.output)
        / 1_000_000.0;
    amount.is_finite().then_some(amount)
}

fn reserve_request(
    ledger: &Shared,
    path: &Mutex<std::path::PathBuf>,
    amount: f64,
) -> Result<Reservation, Error> {
    if !amount.is_finite() || amount < 0.0 {
        return Err(Error::Config("request has no safe cost reservation".into()));
    }
    let mut l = lock(ledger);
    if l.reservations.len() >= MAX_OUTSTANDING_REQUESTS {
        return Err(Error::Config(
            "too many outstanding budget reservations".into(),
        ));
    }
    if let Some(limit) = l.limit {
        if l.unknown {
            return Err(Error::Config("session budget usage is unknown".into()));
        }
        if amount <= 0.0 {
            return Err(Error::Config(
                "request has no positive safe cost reservation".into(),
            ));
        }
        let remaining = (limit - l.spent - l.reserved()).max(0.0);
        if remaining <= 0.0 || amount > remaining {
            return Err(Error::Config(
                "request exceeds the remaining session cost limit".into(),
            ));
        }
    }
    let id = l.next_reservation_id;
    let Some(next_id) = id.checked_add(1) else {
        l.unknown = true;
        let _ = persist(path, &mut l);
        return Err(Error::Config("budget reservation IDs are exhausted".into()));
    };
    l.next_reservation_id = next_id;
    l.reservations.insert(id, amount);
    if persist(path, &mut l).is_err() {
        l.unknown = true;
        if l.limit.is_some() {
            return Err(Error::Config(
                "could not persist the request budget reservation".into(),
            ));
        }
    }
    Ok(Reservation { id })
}

/// Settle a reservation once. A duplicate settlement is a no-op because IDs
/// are monotonic and finalized reservations are removed from the ledger.
fn settle_request(
    ledger: &Shared,
    path: &Mutex<std::path::PathBuf>,
    reservation: Reservation,
    actual: f64,
) {
    let mut l = lock(ledger);
    if !actual.is_finite() || actual < 0.0 {
        l.unknown = true;
        let _ = persist(path, &mut l);
        return;
    }
    if let Some(reserved) = l.reservations.remove(&reservation.id) {
        l.spent += actual;
        let tolerance = (reserved.abs() * 1e-9).max(1e-12);
        if actual > reserved + tolerance {
            // A provider reported cost above the conservative admission bound.
            // Keep the actual charge, but refuse further calls because the
            // assumptions that made the configured hard limit safe failed.
            l.unknown = true;
        }
        if !l.spent.is_finite() {
            l.unknown = true;
        }
        if persist(path, &mut l).is_err() {
            // The on-disk reservation is retained if this write failed, so a
            // restart remains conservative. The live process also fails closed.
            l.unknown = true;
        }
    }
}

/// Abandon only when the caller knows dispatch did not begin. Provider errors
/// after invoking `stream` do not use this path; their reservation is retained.
fn abandon_request(ledger: &Shared, path: &Mutex<std::path::PathBuf>, reservation: Reservation) {
    let mut l = lock(ledger);
    if l.reservations.remove(&reservation.id).is_some() && persist(path, &mut l).is_err() {
        l.unknown = true;
    }
}

/// Admit one child session before creating its transcript or dispatching its
/// provider. A configured cost limit opts into the root-wide child ceiling;
/// without it, existing standalone subagent behavior remains unchanged.
pub fn reserve_child(
    ledger: &Shared,
    path: &Mutex<std::path::PathBuf>,
    task_id: &str,
) -> Result<(), String> {
    if task_id.is_empty() || task_id.len() > 128 || task_id.chars().any(char::is_control) {
        return Err("invalid child task identifier".into());
    }
    let mut l = lock(ledger);
    if l.limit.is_none() {
        return Ok(());
    }
    if l.unknown {
        return Err("session budget usage is unknown".into());
    }
    if l.child_tasks.contains(task_id) {
        return Ok(());
    }
    if l.child_tasks.len() >= MAX_ROOT_CHILDREN {
        return Err("session child limit reached".into());
    }
    if l.remaining().is_some_and(|remaining| remaining <= 0.0) {
        return Err("session cost limit reached before child dispatch".into());
    }
    l.child_tasks.insert(task_id.to_owned());
    if persist(path, &mut l).is_err() {
        l.unknown = true;
        return Err("could not persist the child budget reservation".into());
    }
    Ok(())
}

fn persist(path: &Mutex<std::path::PathBuf>, ledger: &mut Ledger) -> Result<(), String> {
    let path = path
        .lock()
        .map_err(|_| "budget persistence path lock was poisoned".to_owned())?;
    save(&path, ledger)
}

pub fn path_snapshot(path: &Mutex<std::path::PathBuf>) -> Result<std::path::PathBuf, String> {
    path.lock()
        .map(|path| path.clone())
        .map_err(|_| "budget persistence path lock is poisoned".to_owned())
}

impl Provider for Metered {
    fn rate_card(&self) -> Option<RateCard> {
        self.card
    }
    fn model(&self) -> &str {
        self.inner.model()
    }
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
        let limit_enabled = lock(&self.ledger).limit.is_some();
        let mut reservation = if limit_enabled {
            match self.card {
                Some(card) => {
                    match request_cost_bound(card, self.inner.context_window(), req.max_tokens) {
                        Some(amount) => Some(reserve_request(&self.ledger, &self.path, amount)?),
                        None => {
                            return Err(Error::Config(
                                "request has no safe cost reservation".into(),
                            ));
                        }
                    }
                }
                None => {
                    return Err(Error::Config(
                        "cost limit requires complete pricing and usage; ChatGPT plan usage uses provider quotas, so use /budget off".into(),
                    ));
                }
            }
        } else {
            None
        };
        if req.max_tokens == 0 {
            if let Some(reservation) = reservation.take() {
                abandon_request(&self.ledger, &self.path, reservation);
            }
            return Err(Error::Config("request max_tokens must be positive".into()));
        }
        let iter = match self.inner.stream(req) {
            Ok(iter) => iter,
            Err(error) => {
                // The provider may have transmitted the request before the
                // transport reported failure. Retain any reservation and mark
                // the ledger unknown instead of permitting a duplicate retry.
                let mut ledger = lock(&self.ledger);
                ledger.unknown = true;
                let _ = persist(&self.path, &mut ledger);
                return Err(error);
            }
        };
        Ok(Box::new(Meter {
            iter,
            owner: self,
            usage: None,
            reservation,
        }))
    }
}
struct Meter<'a> {
    iter: Box<dyn Iterator<Item = Event> + 'a>,
    owner: &'a Metered,
    usage: Option<crate::provider::Usage>,
    reservation: Option<Reservation>,
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
        match (self.owner.card, self.usage, self.reservation) {
            (Some(card), Some(usage), Some(reservation)) if usage.priced_complete(card) => {
                settle_request(
                    &self.owner.ledger,
                    &self.owner.path,
                    reservation,
                    card.cost(&usage),
                )
            }
            (Some(card), Some(usage), None) if usage.priced_complete(card) => {
                let mut ledger = lock(&self.owner.ledger);
                ledger.spent += card.cost(&usage);
                if !ledger.spent.is_finite() || persist(&self.owner.path, &mut ledger).is_err() {
                    ledger.unknown = true;
                }
            }
            _ => {
                // Missing or partial usage means the request's true cost is
                // not known. Keep any outstanding reservation so a restart
                // cannot free it.
                let mut ledger = lock(&self.owner.ledger);
                ledger.unknown = true;
                let _ = persist(&self.owner.path, &mut ledger);
            }
        }
    }
}
pub fn load(path: &std::path::Path, limit: Option<f64>) -> Shared {
    let mut l: Ledger = match std::fs::metadata(path) {
        Ok(metadata) if metadata.len() > MAX_LEDGER_BYTES => Ledger {
            unknown: true,
            ..Default::default()
        },
        Ok(_) => match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| Ledger {
                unknown: true,
                ..Default::default()
            }),
            Err(_) => Ledger {
                unknown: true,
                ..Default::default()
            },
        },
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
    if l.reservations.len() > MAX_OUTSTANDING_REQUESTS
        || l.reservations.iter().any(|(id, amount)| {
            *id >= l.next_reservation_id || !amount.is_finite() || *amount < 0.0
        })
        || l.child_tasks.len() > MAX_ROOT_CHILDREN
    {
        l.unknown = true;
    }
    if !l.reservations.is_empty() {
        // `load` is a new budget owner. Any persisted in-flight reservation
        // belongs to an interrupted/uncertain request and cannot be safely
        // settled from a later process.
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
    if data.len() as u64 > MAX_LEDGER_BYTES {
        return Err("budget ledger exceeds its size bound".into());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    file.write_all(&data).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(temp, path)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            #[cfg(unix)]
            if let Some(parent) = path.parent() {
                std::fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Usage;
    use std::sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    };
    struct Fake;
    impl Provider for Fake {
        fn id(&self) -> &str {
            "fake"
        }
        fn context_window(&self) -> u32 {
            1_000_000
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![
                    Event::Usage(Usage {
                        input: 1_000_000,
                        input_known: true,
                        cache_read_known: true,
                        cache_write_known: true,
                        output_known: true,
                        ..Default::default()
                    }),
                    Event::Done,
                ]
                .into_iter(),
            ))
        }
    }
    struct CountingFake(Arc<AtomicUsize>);
    impl Provider for CountingFake {
        fn id(&self) -> &str {
            "counting-fake"
        }
        fn context_window(&self) -> u32 {
            1
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(
                vec![
                    Event::Usage(Usage {
                        input_known: true,
                        cache_read_known: true,
                        cache_write_known: true,
                        output_known: true,
                        ..Default::default()
                    }),
                    Event::Done,
                ]
                .into_iter(),
            ))
        }
    }

    struct NoUsageFake;
    impl Provider for NoUsageFake {
        fn id(&self) -> &str {
            "no-usage-fake"
        }
        fn context_window(&self) -> u32 {
            1
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(vec![Event::Done].into_iter()))
        }
    }

    struct PartialUsageFake;
    impl Provider for PartialUsageFake {
        fn id(&self) -> &str {
            "partial-usage-fake"
        }
        fn context_window(&self) -> u32 {
            1
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![
                    Event::Usage(Usage {
                        input: 1,
                        input_known: true,
                        ..Default::default()
                    }),
                    Event::Done,
                ]
                .into_iter(),
            ))
        }
    }

    struct CacheBreakdownMissingFake;
    impl Provider for CacheBreakdownMissingFake {
        fn id(&self) -> &str {
            "cache-breakdown-missing-fake"
        }
        fn context_window(&self) -> u32 {
            1
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![
                    Event::Usage(Usage {
                        input: 1,
                        cache_read: 0,
                        cache_write: 0,
                        output: 1,
                        input_known: true,
                        output_known: true,
                        cache_read_known: false,
                        cache_write_known: false,
                    }),
                    Event::Done,
                ]
                .into_iter(),
            ))
        }
    }

    struct OverboundUsageFake;
    impl Provider for OverboundUsageFake {
        fn id(&self) -> &str {
            "overbound-usage-fake"
        }
        fn context_window(&self) -> u32 {
            1
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            Ok(Box::new(
                vec![
                    Event::Usage(Usage {
                        input: 2,
                        input_known: true,
                        cache_read_known: true,
                        cache_write_known: true,
                        output_known: true,
                        ..Default::default()
                    }),
                    Event::Done,
                ]
                .into_iter(),
            ))
        }
    }

    struct ErrorFake(Arc<AtomicUsize>);
    impl Provider for ErrorFake {
        fn id(&self) -> &str {
            "error-fake"
        }
        fn context_window(&self) -> u32 {
            1
        }
        fn stream(&self, _: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(Error::Config("simulated transport uncertainty".into()))
        }
    }

    fn test_path(name: &str) -> std::path::PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "glass-budget-{name}-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ))
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
        let context = Context::load(&path, Some(2.0));
        let parent_context = context.clone();
        let child_context = context.clone();
        let card = Some(RateCard {
            input: 1.0,
            cache_read: 0.0,
            cache_write: 0.0,
            output: 0.0,
        });
        let a = parent_context.metered(Box::new(Fake), card);
        let b = child_context.metered(Box::new(Fake), card);
        assert!(Arc::ptr_eq(&a.ledger, &b.ledger));
        assert!(Arc::ptr_eq(&a.path, &b.path));
        for _ in a.stream(&request()).unwrap() {}
        for _ in b.stream(&request()).unwrap() {}
        assert_eq!(lock(&context.ledger).spent, 2.0);
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

    #[test]
    fn poisoned_ledger_is_recovered_as_unknown_and_fails_closed_under_limit() {
        let ledger = Arc::new(Mutex::new(Ledger {
            spent: 0.5,
            limit: Some(1.0),
            ..Default::default()
        }));
        let poisoner = Arc::clone(&ledger);
        let _ = std::thread::spawn(move || {
            let mut guard = poisoner.lock().unwrap();
            guard.spent = 0.75;
            panic!("simulate interruption while accounting is locked");
        })
        .join();

        let recovered = lock(&ledger);
        assert!(recovered.unknown);
        assert_eq!(recovered.spent, 0.75);
        drop(recovered);
        assert!(!ledger.is_poisoned());

        let metered = Metered {
            inner: Box::new(Fake),
            ledger,
            card: Some(RateCard {
                input: 1.0,
                cache_read: 0.0,
                cache_write: 0.0,
                output: 0.0,
            }),
            path: Arc::new(Mutex::new(std::env::temp_dir().join("unused-cost.json"))),
        };
        assert!(metered.stream(&request()).is_err());
    }

    #[test]
    fn poisoned_persistence_path_does_not_panic_from_meter_drop() {
        let ledger = Arc::new(Mutex::new(Ledger {
            limit: Some(1.0),
            ..Default::default()
        }));
        let path = Arc::new(Mutex::new(std::env::temp_dir().join("unused-cost.json")));
        let poisoner = Arc::clone(&path);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("simulate interruption while the cost path is locked");
        })
        .join();

        let metered = Metered {
            inner: Box::new(Fake),
            ledger: Arc::clone(&ledger),
            card: Some(RateCard {
                input: 1.0,
                cache_read: 0.0,
                cache_write: 0.0,
                output: 0.0,
            }),
            path,
        };
        let meter = Meter {
            iter: Box::new(std::iter::empty()),
            owner: &metered,
            usage: Some(Usage {
                input_known: true,
                cache_read_known: true,
                cache_write_known: true,
                output_known: true,
                ..Default::default()
            }),
            reservation: None,
        };
        drop(meter);

        assert!(lock(&ledger).unknown);
        assert!(path_snapshot(&metered.path).is_err());
        assert!(metered.stream(&request()).is_err());
    }

    #[test]
    fn unknown_spending_is_not_presented_as_a_precise_amount() {
        let known = Ledger {
            spent: 1.25,
            ..Default::default()
        };
        let unknown = Ledger {
            spent: 1.25,
            unknown: true,
            ..Default::default()
        };

        assert_eq!(spent_label(&known), crate::pricing::money(known.spent));
        assert_eq!(spent_label(&unknown), "unavailable");
    }

    #[test]
    fn parallel_fake_provider_admissions_reserve_before_invocation() {
        let path = test_path("parallel");
        let ledger = load(&path, Some(0.000003));
        let calls = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(Barrier::new(3));
        let admitted = Arc::new(Barrier::new(3));
        let card = Some(RateCard {
            input: 1.0,
            cache_read: 1.0,
            cache_write: 1.0,
            output: 1.0,
        });
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let ledger = Arc::clone(&ledger);
                let path = path.clone();
                let calls = Arc::clone(&calls);
                let start = Arc::clone(&start);
                let admitted = Arc::clone(&admitted);
                std::thread::spawn(move || {
                    let provider = Metered {
                        inner: Box::new(CountingFake(Arc::clone(&calls))),
                        ledger,
                        card,
                        path: Arc::new(Mutex::new(path)),
                    };
                    start.wait();
                    let result = provider.stream(&request());
                    admitted.wait();
                    match result {
                        Ok(events) => {
                            for _ in events {}
                            true
                        }
                        Err(_) => false,
                    }
                })
            })
            .collect();
        start.wait();
        admitted.wait();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();

        assert_eq!(results.iter().filter(|admitted| **admitted).count(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let current = lock(&ledger);
        assert_eq!(current.reserved(), 0.0);
        assert_eq!(current.spent, 0.0);
        drop(current);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_usage_keeps_reservation_and_fails_closed_after_reload() {
        let path = test_path("unknown");
        let ledger = load(&path, Some(1.0));
        let provider = Metered {
            inner: Box::new(NoUsageFake),
            ledger: Arc::clone(&ledger),
            card: Some(RateCard {
                input: 1.0,
                cache_read: 0.1,
                cache_write: 1.25,
                output: 5.0,
            }),
            path: Arc::new(Mutex::new(path.clone())),
        };
        for _ in provider.stream(&request()).unwrap() {}
        let loaded = load(&path, Some(1.0));
        let state = lock(&loaded);
        assert!(state.unknown);
        assert!(state.reserved() > 0.0);
        drop(state);
        assert!(provider.stream(&request()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn partial_usage_is_unknown_and_cannot_settle_the_reservation() {
        let path = test_path("partial-usage");
        let context = Context::load(&path, Some(1.0));
        let provider = context.metered(
            Box::new(PartialUsageFake),
            Some(RateCard {
                input: 1.0,
                cache_read: 0.1,
                cache_write: 1.25,
                output: 5.0,
            }),
        );
        for _ in provider.stream(&request()).unwrap() {}
        let state = lock(&context.ledger);
        assert!(state.unknown);
        assert!(state.reserved() > 0.0);
        drop(state);
        assert!(provider.stream(&request()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_priced_cache_breakdown_keeps_reservation_unknown() {
        let path = test_path("missing-priced-cache");
        let ledger = load(&path, Some(1.0));
        let provider = Metered {
            inner: Box::new(CacheBreakdownMissingFake),
            ledger: Arc::clone(&ledger),
            card: Some(RateCard {
                input: 1.0,
                cache_read: 0.0,
                cache_write: 1.25,
                output: 0.0,
            }),
            path: Arc::new(Mutex::new(path.clone())),
        };
        for _ in provider.stream(&request()).unwrap() {}

        let state = lock(&ledger);
        assert!(state.unknown);
        assert!(state.reserved() > 0.0);
        assert_eq!(state.spent, 0.0);
        drop(state);
        assert!(provider.stream(&request()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn actual_cost_above_reservation_marks_budget_unknown() {
        let path = test_path("actual-above-reservation");
        let context = Context::load(&path, Some(0.000001));
        let provider = context.metered(
            Box::new(OverboundUsageFake),
            Some(RateCard {
                input: 1.0,
                cache_read: 0.0,
                cache_write: 0.0,
                output: 0.0,
            }),
        );
        for _ in provider.stream(&request()).unwrap() {}

        let state = lock(&context.ledger);
        assert!(state.unknown);
        assert_eq!(state.spent, 0.000002);
        assert_eq!(state.reserved(), 0.0);
        drop(state);
        assert!(provider.stream(&request()).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn provider_error_retains_uncertain_reservation_without_retry() {
        let path = test_path("provider-error");
        let ledger = load(&path, Some(1.0));
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Metered {
            inner: Box::new(ErrorFake(Arc::clone(&calls))),
            ledger: Arc::clone(&ledger),
            card: Some(RateCard {
                input: 1.0,
                cache_read: 0.1,
                cache_write: 1.25,
                output: 5.0,
            }),
            path: Arc::new(Mutex::new(path.clone())),
        };
        assert!(provider.stream(&request()).is_err());
        assert!(provider.stream(&request()).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let persisted = load(&path, Some(1.0));
        let state = lock(&persisted);
        assert!(state.unknown);
        assert!(state.reserved() > 0.0);
        drop(state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_persisted_inflight_reservation_is_unknown_after_restart() {
        let path = test_path("crash");
        let ledger = load(&path, Some(1.0));
        let path_mutex = Mutex::new(path.clone());
        let reservation = reserve_request(&ledger, &path_mutex, 0.5).unwrap();
        let reloaded = load(&path, Some(1.0));
        let state = lock(&reloaded);
        assert!(state.unknown);
        assert_eq!(state.reservations.get(&reservation.id), Some(&0.5));
        drop(state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reservation_settle_and_abandon_are_idempotent() {
        let path = test_path("idempotent");
        let ledger = load(&path, Some(2.0));
        let path_mutex = Mutex::new(path.clone());
        let reservation = reserve_request(&ledger, &path_mutex, 0.75).unwrap();
        settle_request(&ledger, &path_mutex, reservation, 0.25);
        settle_request(&ledger, &path_mutex, reservation, 0.25);
        {
            let state = lock(&ledger);
            assert_eq!(state.spent, 0.25);
            assert_eq!(state.reserved(), 0.0);
        }

        let abandoned = reserve_request(&ledger, &path_mutex, 0.5).unwrap();
        abandon_request(&ledger, &path_mutex, abandoned);
        abandon_request(&ledger, &path_mutex, abandoned);
        let state = lock(&ledger);
        assert_eq!(state.spent, 0.25);
        assert_eq!(state.reserved(), 0.0);
        assert!(!state.unknown);
        drop(state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn locally_rejected_request_abandons_its_reservation_before_provider_call() {
        let path = test_path("abandon-before-call");
        let context = Context::load(&path, Some(1.0));
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = context.metered(
            Box::new(CountingFake(Arc::clone(&calls))),
            Some(RateCard {
                input: 1.0,
                cache_read: 0.1,
                cache_write: 1.25,
                output: 5.0,
            }),
        );
        let mut invalid = request();
        invalid.max_tokens = 0;
        assert!(provider.stream(&invalid).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let state = lock(&context.ledger);
        assert!(state.reservations.is_empty());
        assert!(!state.unknown);
        drop(state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn configured_root_child_admission_is_shared_idempotent_and_bounded() {
        let path = test_path("children");
        let ledger = load(&path, Some(1.0));
        let path_mutex = Mutex::new(path.clone());
        reserve_child(&ledger, &path_mutex, "child-000").unwrap();
        reserve_child(&ledger, &path_mutex, "child-000").unwrap();
        for index in 1..MAX_ROOT_CHILDREN {
            reserve_child(&ledger, &path_mutex, &format!("child-{index:03}")).unwrap();
        }
        assert!(reserve_child(&ledger, &path_mutex, "child-over-limit").is_err());
        let reloaded = load(&path, Some(1.0));
        assert_eq!(lock(&reloaded).child_tasks.len(), MAX_ROOT_CHILDREN);
        let _ = std::fs::remove_file(path);
    }
}
