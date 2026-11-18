//! Routines: the work the assistant does without being asked.
//!
//! A routine is a saved prompt plus a small schedule. Marble's scheduler wakes
//! up, finds the routines that are due, and admits them through the same queue
//! and control layer as a typed message, so an unattended run is never a
//! different permission path.

use crate::notify::QuietHours;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Routines one instance keeps.
pub const ROUTINE_LIMIT: usize = 32;
/// Fastest interval Marble accepts, so a routine cannot become a busy loop.
pub const MIN_INTERVAL_SECS: u64 = 60;

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn next_id(prefix: &str) -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{:x}-{sequence:x}", now_millis())
}

/// How often a routine runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    /// Every `secs` seconds, measured from the previous run.
    Every(u64),
    /// Once a day at `second_of_day` local time (seconds since midnight).
    Daily(u64),
}

/// Parse `every:15m`, `every:2h`, `every:1d`, or `daily:09:30`.
pub fn parse_schedule(spec: &str) -> Option<Schedule> {
    let spec = spec.trim().to_ascii_lowercase();
    if let Some(rest) = spec.strip_prefix("every:") {
        let rest = rest.trim();
        let (digits, unit) = rest.split_at(rest.len().saturating_sub(1));
        let count: u64 = digits.trim().parse().ok()?;
        if count == 0 {
            return None;
        }
        let seconds = match unit {
            "s" => count,
            "m" => count.checked_mul(60)?,
            "h" => count.checked_mul(3600)?,
            "d" => count.checked_mul(86_400)?,
            _ => return None,
        };
        return Some(Schedule::Every(seconds.max(MIN_INTERVAL_SECS)));
    }
    if let Some(rest) = spec.strip_prefix("daily:") {
        let (hours, minutes) = rest.trim().split_once(':')?;
        let hours: u64 = hours.trim().parse().ok()?;
        let minutes: u64 = minutes.trim().parse().ok()?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        return Some(Schedule::Daily(hours * 3600 + minutes * 60));
    }
    None
}

impl Schedule {
    /// Next run after `now` in UTC, strictly in the future.
    pub fn next_after(self, now: u64) -> u64 {
        self.next_after_offset(now, 0)
    }

    /// Next run after `now`, with a daily time interpreted in the host's local
    /// time. `offset_minutes` is minutes east of UTC, the same convention the
    /// quiet-hours setting uses; `every:` schedules are unaffected.
    pub fn next_after_offset(self, now: u64, offset_minutes: i32) -> u64 {
        let now_secs = now / 1000;
        match self {
            Self::Every(seconds) => (now_secs + seconds) * 1000,
            Self::Daily(second_of_day) => {
                let offset = i64::from(offset_minutes.clamp(-840, 840)) * 60;
                let local = now_secs as i64 + offset;
                let today = local.div_euclid(86_400) * 86_400 + second_of_day as i64;
                let next = if today > local { today } else { today + 86_400 };
                ((next - offset) * 1000) as u64
            }
        }
    }

    /// Human label used by product surfaces.
    pub fn label(self) -> String {
        match self {
            Self::Every(seconds) if seconds % 86_400 == 0 => {
                format!("every {}d", seconds / 86_400)
            }
            Self::Every(seconds) if seconds % 3600 == 0 => {
                format!("every {}h", seconds / 3600)
            }
            Self::Every(seconds) if seconds % 60 == 0 => {
                format!("every {}m", seconds / 60)
            }
            Self::Every(seconds) => format!("every {seconds}s"),
            Self::Daily(second_of_day) => {
                format!(
                    "daily at {:02}:{:02}",
                    second_of_day / 3600,
                    second_of_day % 3600 / 60
                )
            }
        }
    }
}

/// One saved prompt on a schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub prompt: String,
    /// The schedule spec as typed, so a surface can show it back.
    pub schedule: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub last_run_at: u64,
    #[serde(default)]
    pub next_run_at: u64,
    #[serde(default)]
    pub runs: u64,
    #[serde(default)]
    pub last_task_id: String,
    #[serde(default)]
    pub last_status: String,
    /// Occurrences that passed while Marble was not running, cumulatively.
    #[serde(default)]
    pub missed_runs: u64,
    /// Occurrences missed in the most recent gap.
    #[serde(default)]
    pub missed_last_gap: u64,
    #[serde(default)]
    pub last_missed_at: u64,
    /// Whether the first tick after a gap runs the routine once. With this off,
    /// a missed occurrence is skipped and the schedule simply resumes.
    #[serde(default = "enabled_by_default")]
    pub catch_up: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl Routine {
    /// Parsed schedule, or `None` when the stored spec no longer parses.
    pub fn parsed_schedule(&self) -> Option<Schedule> {
        parse_schedule(&self.schedule)
    }

    /// Human label for the schedule.
    pub fn schedule_label(&self) -> String {
        self.parsed_schedule()
            .map(Schedule::label)
            .unwrap_or_else(|| self.schedule.clone())
    }

    /// One line describing what happened while Marble was away.
    pub fn missed_label(&self) -> String {
        match self.missed_last_gap {
            0 => "on schedule".to_string(),
            1 => "1 run missed, caught up".to_string(),
            count => format!("{count} runs missed, caught up"),
        }
    }
}

/// How many occurrences of `schedule` passed between `scheduled` and `now`.
/// The occurrence that is due right now is not counted as missed.
fn missed_occurrences(schedule: Schedule, scheduled: u64, now: u64) -> u64 {
    let elapsed = now.saturating_sub(scheduled);
    match schedule {
        Schedule::Every(seconds) => elapsed / (seconds.max(1) * 1000),
        Schedule::Daily(_) => elapsed / 86_400_000,
    }
}

/// Durable routine document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineState {
    #[serde(default)]
    pub routines: Vec<Routine>,
    /// Minutes east of UTC used to interpret `daily:HH:MM`, supplied by the
    /// surface that saved the routine.
    #[serde(default)]
    pub offset_minutes: i32,
    /// Window in which scheduled work waits for the person to be awake.
    #[serde(default)]
    pub quiet_hours: QuietHours,
}

impl RoutineState {
    /// Save one routine. The schedule is validated here so a broken spec never
    /// reaches the scheduler.
    pub fn add(
        &mut self,
        title: &str,
        prompt: &str,
        schedule: &str,
        conversation_id: Option<String>,
    ) -> Result<Routine, String> {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return Err("a routine needs a prompt".into());
        }
        if prompt.chars().count() > 4000 {
            return Err("a routine prompt is at most 4000 characters".into());
        }
        let parsed = parse_schedule(schedule)
            .ok_or_else(|| "use every:<n>m, every:<n>h, every:<n>d, or daily:HH:MM".to_string())?;
        if self.routines.len() >= ROUTINE_LIMIT {
            return Err(format!(
                "marble keeps at most {ROUTINE_LIMIT} routines; remove one first"
            ));
        }
        let now = now_millis();
        let title = title.trim();
        let routine = Routine {
            id: next_id("routine"),
            title: if title.is_empty() {
                prompt.chars().take(64).collect()
            } else {
                title.chars().take(80).collect()
            },
            prompt: prompt.to_string(),
            schedule: schedule.trim().to_string(),
            conversation_id,
            enabled: true,
            created_at: now,
            last_run_at: 0,
            next_run_at: parsed.next_after_offset(now, self.offset_minutes),
            runs: 0,
            last_task_id: String::new(),
            last_status: "scheduled".into(),
            missed_runs: 0,
            missed_last_gap: 0,
            last_missed_at: 0,
            catch_up: true,
        };
        self.routines.push(routine.clone());
        Ok(routine)
    }

    /// Remove one routine; returns whether it existed.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.routines.len();
        self.routines.retain(|routine| routine.id != id);
        self.routines.len() != before
    }

    /// Choose whether the first tick after a gap runs the missed occurrence.
    pub fn set_catch_up(&mut self, id: &str, catch_up: bool) -> bool {
        let Some(routine) = self.routines.iter_mut().find(|routine| routine.id == id) else {
            return false;
        };
        routine.catch_up = catch_up;
        true
    }

    /// Enable or disable one routine, recomputing its next run when enabled.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        let now = now_millis();
        let Some(routine) = self.routines.iter_mut().find(|routine| routine.id == id) else {
            return false;
        };
        routine.enabled = enabled;
        routine.last_status = if enabled {
            "scheduled".to_string()
        } else {
            "paused".to_string()
        };
        if enabled && let Some(schedule) = routine.parsed_schedule() {
            routine.next_run_at = schedule.next_after_offset(now, self.offset_minutes);
        }
        true
    }

    /// Set the offset used for `daily:` schedules and recompute every daily
    /// routine's next run, so changing timezones takes effect immediately.
    pub fn set_offset(&mut self, offset_minutes: i32) -> bool {
        let offset = offset_minutes.clamp(-840, 840);
        if offset == self.offset_minutes {
            return false;
        }
        self.offset_minutes = offset;
        let now = now_millis();
        for routine in &mut self.routines {
            if let Some(schedule @ Schedule::Daily(_)) = routine.parsed_schedule() {
                routine.next_run_at = schedule.next_after_offset(now, offset);
            }
        }
        true
    }

    /// Replace the quiet window; returns whether anything changed.
    pub fn set_quiet(&mut self, quiet: QuietHours) -> bool {
        if self.quiet_hours == quiet {
            return false;
        }
        self.quiet_hours = quiet;
        true
    }

    /// Whether a schedule firing right now would interrupt the quiet window.
    ///
    /// Nothing is advanced while this is true, so a routine keeps its place and
    /// runs when the window ends; the gap is counted, so catch-up still runs it
    /// once rather than replaying every occurrence.
    pub fn quiet_now(&self, now: u64) -> bool {
        self.quiet_hours.enabled
            && !self
                .quiet_hours
                .allows_minute(self.quiet_hours.local_minute(now))
    }

    /// Record that one routine was admitted for a task.
    pub fn note_run(&mut self, id: &str, task_id: &str, now: u64) {
        let Some(routine) = self.routines.iter_mut().find(|routine| routine.id == id) else {
            return;
        };
        routine.last_run_at = now;
        routine.runs = routine.runs.saturating_add(1);
        routine.last_task_id = task_id.to_string();
        routine.last_status = "running".to_string();
        if let Some(schedule) = routine.parsed_schedule() {
            routine.next_run_at = schedule.next_after_offset(now, self.offset_minutes);
        }
    }

    /// Record the settlement of one routine's task.
    pub fn note_settled(&mut self, task_id: &str, status: &str) {
        if let Some(routine) = self
            .routines
            .iter_mut()
            .find(|routine| !task_id.is_empty() && routine.last_task_id == task_id)
        {
            routine.last_status = status.to_string();
        }
    }

    /// Set the visible status without touching counters or the schedule.
    pub fn note_status(&mut self, id: &str, status: &str) {
        if let Some(routine) = self.routines.iter_mut().find(|routine| routine.id == id) {
            routine.last_status = status.to_string();
        }
    }

    /// Routines whose next run has arrived. Every returned routine has already
    /// been advanced, so a second call in the same tick returns nothing.
    ///
    /// A laptop sleeps overnight and a host reboots; either way Marble was not
    /// awake for its schedule. Rather than silently skipping those runs, the gap
    /// is counted on the routine and, when catch-up is on, the routine runs once
    /// on the first tick. A routine with catch-up off records the missed runs
    /// and resumes its schedule without a burst of work.
    pub fn due(&mut self, now: u64) -> Vec<Routine> {
        let mut due = Vec::new();
        for routine in self.routines.iter_mut() {
            if !routine.enabled || routine.next_run_at == 0 {
                continue;
            }
            if routine.next_run_at > now {
                continue;
            }
            let Some(schedule) = routine.parsed_schedule() else {
                routine.enabled = false;
                routine.last_status = "invalid schedule".into();
                continue;
            };
            let missed = missed_occurrences(schedule, routine.next_run_at, now);
            routine.missed_last_gap = missed;
            if missed > 0 {
                routine.missed_runs = routine.missed_runs.saturating_add(missed);
                routine.last_missed_at = now;
                if !routine.catch_up {
                    routine.next_run_at = schedule.next_after_offset(now, self.offset_minutes);
                    routine.last_status = format!("missed {missed}");
                    continue;
                }
            }
            routine.next_run_at = schedule.next_after_offset(now, self.offset_minutes);
            due.push(routine.clone());
        }
        due
    }

    /// Find one routine by id.
    pub fn get(&self, id: &str) -> Option<&Routine> {
        self.routines.iter().find(|routine| routine.id == id)
    }

    /// Product document with derived counts.
    pub fn document(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Value::Object(ref mut map) = value {
            map.insert("count".into(), json!(self.routines.len()));
            map.insert("limit".into(), json!(ROUTINE_LIMIT));
            map.insert("offsetMinutes".into(), json!(self.offset_minutes));
            map.insert(
                "enabled".into(),
                json!(
                    self.routines
                        .iter()
                        .filter(|routine| routine.enabled)
                        .count()
                ),
            );
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_parse_and_label_themselves() {
        assert_eq!(parse_schedule("every:15m"), Some(Schedule::Every(900)));
        assert_eq!(parse_schedule("every:2h"), Some(Schedule::Every(7200)));
        assert_eq!(parse_schedule("every:1d"), Some(Schedule::Every(86_400)));
        assert_eq!(parse_schedule("daily:09:30"), Some(Schedule::Daily(34_200)));
        assert_eq!(parse_schedule("every:5s"), Some(Schedule::Every(60)));
        assert_eq!(parse_schedule("daily:24:00"), None);
        assert_eq!(parse_schedule("cron:0 9 * * *"), None);
        assert_eq!(parse_schedule("every:0m"), None);
        assert_eq!(Schedule::Every(900).label(), "every 15m");
        assert_eq!(Schedule::Daily(34_200).label(), "daily at 09:30");
    }

    #[test]
    fn daily_schedules_follow_the_supplied_offset() {
        // 2026-01-01T00:00:00Z
        let now = 1_767_225_600_000u64;
        assert_eq!(
            Schedule::Daily(9 * 3600).next_after_offset(now, 330),
            now + 3 * 3_600_000 + 30 * 60_000,
            "09:00 IST is 03:30 UTC"
        );
        assert_eq!(
            Schedule::Daily(9 * 3600).next_after(now),
            now + 9 * 3_600_000
        );
        // A negative offset crosses the previous UTC day instead.
        assert_eq!(
            Schedule::Daily(0).next_after_offset(now, -480),
            now + 8 * 3_600_000,
            "midnight PST is 08:00 UTC"
        );
    }

    #[test]
    fn changing_the_offset_moves_daily_routines_immediately() {
        let mut routines = RoutineState::default();
        let routine = routines
            .add("Digest", "summarize", "daily:09:00", None)
            .unwrap();
        let before = routine.next_run_at;
        assert!(routines.set_offset(330));
        assert!(!routines.set_offset(330));
        let after = routines.get(&routine.id).unwrap().next_run_at;
        assert_ne!(before, after);
        assert_eq!(routines.document()["offsetMinutes"], json!(330));
        // `every:` schedules are unaffected by a timezone.
        let hourly = routines.add("Hourly", "tidy", "every:1h", None).unwrap();
        let hourly_next = hourly.next_run_at;
        routines.set_offset(-480);
        assert_eq!(routines.get(&hourly.id).unwrap().next_run_at, hourly_next);
    }

    #[test]
    fn next_run_is_strictly_in_the_future() {
        let now = 1_700_000_000_000; // a fixed instant
        let every = Schedule::Every(900).next_after(now);
        assert_eq!(every, now + 900_000);
        let daily = Schedule::Daily(34_200).next_after(now);
        assert!(daily > now);
        assert!(daily <= now + 86_400_000);
    }

    #[test]
    fn adding_validates_and_starts_scheduled() {
        let mut routines = RoutineState::default();
        assert!(routines.add("", "  ", "every:1h", None).is_err());
        assert!(
            routines
                .add("tidy", "tidy the notes", "sometimes", None)
                .is_err()
        );
        let routine = routines
            .add(
                "Tidy notes",
                "tidy the notes",
                "daily:09:00",
                Some("c1".into()),
            )
            .unwrap();
        assert!(routine.enabled);
        assert_eq!(routine.title, "Tidy notes");
        assert!(routine.next_run_at > 0);
        assert_eq!(routines.document()["enabled"], json!(1));
    }

    #[test]
    fn due_routines_fire_once_and_advance() {
        let mut routines = RoutineState::default();
        let routine = routines
            .add("Tidy", "tidy the notes", "every:10m", None)
            .unwrap();
        let now = routine.next_run_at + 1000;
        let fired = routines.due(now);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].id, routine.id);
        assert!(routines.due(now).is_empty());
        routines.note_run(&routine.id, "task-1", now);
        assert_eq!(routines.routines[0].runs, 1);
        assert_eq!(routines.routines[0].last_task_id, "task-1");
        routines.note_settled("task-1", "completed");
        assert_eq!(routines.routines[0].last_status, "completed");
    }

    #[test]
    fn a_gap_is_counted_and_caught_up_exactly_once() {
        let mut routines = RoutineState::default();
        let routine = routines
            .add("Daily digest", "summarize today", "every:1h", None)
            .unwrap();
        // Three hours later: three occurrences passed at +0h, +1h, and +2h;
        // the one at +3h is due now and runs as the catch-up.
        let now = routine.next_run_at + 3 * 3_600_000;
        let fired = routines.due(now);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].missed_last_gap, 3);
        assert_eq!(routines.routines[0].missed_runs, 3);
        assert_eq!(routines.routines[0].last_missed_at, now);
        assert_eq!(
            routines.routines[0].missed_label(),
            "3 runs missed, caught up"
        );
        assert!(routines.due(now + 1000).is_empty());
    }

    #[test]
    fn catch_up_can_be_turned_off_without_losing_the_count() {
        let mut routines = RoutineState::default();
        let routine = routines
            .add("Daily digest", "summarize today", "every:1h", None)
            .unwrap();
        assert!(routines.set_catch_up(&routine.id, false));
        let now = routine.next_run_at + 2 * 3_600_000;
        assert!(routines.due(now).is_empty());
        let stored = routines.get(&routine.id).unwrap();
        assert_eq!(stored.missed_runs, 2);
        assert_eq!(stored.last_status, "missed 2");
        assert!(stored.next_run_at > now);
        assert!(!routines.set_catch_up("missing", true));
    }

    #[test]
    fn disabled_routines_never_fire() {
        let mut routines = RoutineState::default();
        let routine = routines.add("Tidy", "tidy", "every:10m", None).unwrap();
        assert!(routines.set_enabled(&routine.id, false));
        let now = routine.next_run_at + 10_000;
        assert!(routines.due(now).is_empty());
        assert!(routines.set_enabled(&routine.id, true));
        assert_eq!(routines.due(now).len(), 1);
        assert!(routines.remove(&routine.id));
        assert!(routines.routines.is_empty());
    }

    #[test]
    fn a_quiet_window_holds_a_routine_until_morning() {
        let mut routines = RoutineState::default();
        let routine = routines
            .add("Digest", "summarize", "every:1h", None)
            .unwrap();
        assert!(!routines.quiet_now(routine.next_run_at));
        assert!(routines.set_quiet(QuietHours {
            enabled: true,
            start_minute: 22 * 60,
            end_minute: 7 * 60,
            offset_minutes: 0,
        }));
        assert!(!routines.set_quiet(routines.quiet_hours.clone()));
        // 23:30 UTC is inside the window; 08:00 is not.
        let start_of_day = (routine.next_run_at / 86_400_000) * 86_400_000;
        assert!(routines.quiet_now(start_of_day + 23 * 3_600_000 + 30 * 60_000));
        assert!(!routines.quiet_now(start_of_day + 12 * 3_600_000));
        assert_eq!(
            routines.document()["quietHours"]["enabled"],
            json!(true),
            "the window is part of the routine document"
        );
    }

    #[test]
    fn legacy_documents_decode_with_no_routines() {
        let routines: RoutineState = serde_json::from_str("{}").unwrap();
        assert!(routines.routines.is_empty());
        assert_eq!(routines.document()["limit"], json!(ROUTINE_LIMIT));
    }
}
