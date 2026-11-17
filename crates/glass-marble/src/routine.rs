//! Routines: the work the assistant does without being asked.
//!
//! A routine is a saved prompt plus a small schedule. Marble's scheduler wakes
//! up, finds the routines that are due, and admits them through the same queue
//! and control layer as a typed message, so an unattended run is never a
//! different permission path.

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
    /// Next run after `now` (unix milliseconds), strictly in the future.
    pub fn next_after(self, now: u64) -> u64 {
        let now_secs = now / 1000;
        match self {
            Self::Every(seconds) => (now_secs + seconds) * 1000,
            Self::Daily(second_of_day) => {
                let today = now_secs / 86_400 * 86_400 + second_of_day;
                let next = if today > now_secs {
                    today
                } else {
                    today + 86_400
                };
                next * 1000
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
}

/// Durable routine document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineState {
    #[serde(default)]
    pub routines: Vec<Routine>,
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
            next_run_at: parsed.next_after(now),
            runs: 0,
            last_task_id: String::new(),
            last_status: "scheduled".into(),
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
            routine.next_run_at = schedule.next_after(now);
        }
        true
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
            routine.next_run_at = schedule.next_after(now);
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
            routine.next_run_at = schedule.next_after(now);
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
    fn legacy_documents_decode_with_no_routines() {
        let routines: RoutineState = serde_json::from_str("{}").unwrap();
        assert!(routines.routines.is_empty());
        assert_eq!(routines.document()["limit"], json!(ROUTINE_LIMIT));
    }
}
