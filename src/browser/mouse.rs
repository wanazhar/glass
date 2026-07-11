use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A 2D point for mouse path calculations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A smooth pointer-motion generator.
///
/// The engine uses a bounded number of samples and small per-movement
/// variations. This keeps the event stream realistic without turning a click
/// into hundreds of unnecessary CDP round trips.
pub struct MouseEngine {
    pub min_speed: f64,
    pub max_speed: f64,
    pub steps_per_second: u32,
    seed: AtomicU64,
}

impl Default for MouseEngine {
    fn default() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15)
            | 1;
        Self {
            min_speed: 400.0,
            max_speed: 800.0,
            steps_per_second: 60,
            seed: AtomicU64::new(seed),
        }
    }
}

impl MouseEngine {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_seed(seed: u64) -> Self {
        Self {
            min_speed: 400.0,
            max_speed: 800.0,
            steps_per_second: 60,
            seed: AtomicU64::new(seed | 1),
        }
    }

    fn next_unit(&self) -> f64 {
        let mut current = self.seed.load(Ordering::Relaxed);
        loop {
            let next = current
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            match self.seed.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return next as f64 / u64::MAX as f64,
                Err(actual) => current = actual,
            }
        }
    }

    /// Generate a cubic Bezier curve between two points.
    fn bezier_curve(&self, start: Point, end: Point, cp1: Point, cp2: Point, t: f64) -> Point {
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        Point {
            x: mt3 * start.x + 3.0 * mt2 * t * cp1.x + 3.0 * mt * t2 * cp2.x + t3 * end.x,
            y: mt3 * start.y + 3.0 * mt2 * t * cp1.y + 3.0 * mt * t2 * cp2.y + t3 * end.y,
        }
    }

    /// Generate slightly varied control points around a direct trajectory.
    fn generate_control_points(&self, start: Point, end: Point) -> (Point, Point) {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance < f64::EPSILON {
            return (start, end);
        }

        let direction = (dx / distance, dy / distance);
        let normal = (-direction.1, direction.0);
        let bend = (self.next_unit() * 2.0 - 1.0) * distance * 0.12;
        let along = (self.next_unit() * 2.0 - 1.0) * distance * 0.04;

        let cp1 = Point {
            x: start.x + dx * 0.28 + normal.0 * bend + direction.0 * along,
            y: start.y + dy * 0.28 + normal.1 * bend + direction.1 * along,
        };
        let cp2 = Point {
            x: start.x + dx * 0.72 - normal.0 * bend * 0.7 + direction.0 * along * 0.4,
            y: start.y + dy * 0.72 - normal.1 * bend * 0.7 + direction.1 * along * 0.4,
        };
        (cp1, cp2)
    }

    /// Generate a bounded list of points along a smooth path.
    pub fn generate_path(&self, start: Point, end: Point) -> Vec<Point> {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let distance = (dx * dx + dy * dy).sqrt();
        if distance < f64::EPSILON {
            return vec![start];
        }

        let speed = self.min_speed + (self.max_speed - self.min_speed) * self.next_unit();
        let duration = distance / speed;
        let steps = (duration * self.steps_per_second as f64)
            .round()
            .clamp(8.0, 120.0) as usize;
        let (cp1, cp2) = self.generate_control_points(start, end);
        let mut points = Vec::with_capacity(steps + 1);

        for index in 0..=steps {
            let t = index as f64 / steps as f64;
            let eased = if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            };
            points.push(self.bezier_curve(start, end, cp1, cp2, eased));
        }
        points
    }

    /// Calculate the cadence between consecutive pointer samples.
    ///
    /// Samples are evenly timed so the eased spatial curve produces actual
    /// acceleration and deceleration instead of having its timing cancelled
    /// out by distance-proportional sleeps.
    pub fn move_delay(&self, _start: Point, _end: Point) -> Duration {
        let cadence = 1.0 / self.steps_per_second.max(1) as f64;
        let jitter = 0.9 + self.next_unit() * 0.2;
        Duration::from_secs_f64((cadence * jitter).max(0.001))
    }

    /// Return a short, varied mouse-down dwell before release.
    pub fn click_delay(&self) -> Duration {
        Duration::from_secs_f64(0.04 + self.next_unit() * 0.05)
    }

    /// Generate the press/release events after the pointer reaches a target.
    pub fn generate_click_events(&self, point: Point) -> Vec<MouseEvent> {
        vec![
            MouseEvent {
                event_type: "mousePressed".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 1,
            },
            MouseEvent {
                event_type: "mouseReleased".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 1,
            },
        ]
    }

    pub fn generate_double_click_events(&self, point: Point) -> Vec<MouseEvent> {
        vec![
            MouseEvent {
                event_type: "mousePressed".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 1,
            },
            MouseEvent {
                event_type: "mouseReleased".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 1,
            },
            MouseEvent {
                event_type: "mousePressed".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 2,
            },
            MouseEvent {
                event_type: "mouseReleased".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 2,
            },
        ]
    }

    pub fn generate_drag_events(&self, start: Point, end: Point) -> Vec<MouseEvent> {
        let path = self.generate_path(start, end);
        let mut events = vec![MouseEvent {
            event_type: "mousePressed".to_string(),
            x: start.x,
            y: start.y,
            button: "left".to_string(),
            click_count: 1,
        }];
        for point in path.iter().skip(1).take(path.len().saturating_sub(2)) {
            events.push(MouseEvent {
                event_type: "mouseMoved".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 1,
            });
        }
        events.push(MouseEvent {
            event_type: "mouseReleased".to_string(),
            x: end.x,
            y: end.y,
            button: "left".to_string(),
            click_count: 1,
        });
        events
    }
}

#[derive(Debug, Clone)]
pub struct MouseEvent {
    pub event_type: String,
    pub x: f64,
    pub y: f64,
    pub button: String,
    pub click_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_preserves_endpoints_and_has_bounded_samples() {
        let engine = MouseEngine::with_seed(7);
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point {
            x: 1_000.0,
            y: 600.0,
        };
        let path = engine.generate_path(start, end);
        assert_eq!(path.first(), Some(&start));
        assert_eq!(path.last(), Some(&end));
        assert!((8..=121).contains(&path.len()));
    }

    #[test]
    fn path_is_not_always_a_teleport() {
        let engine = MouseEngine::with_seed(11);
        let path = engine.generate_path(Point { x: 0.0, y: 0.0 }, Point { x: 400.0, y: 0.0 });
        assert!(path.len() > 2);
        assert!(path.iter().any(|point| point.y.abs() > f64::EPSILON));
    }

    #[test]
    fn click_events_only_press_and_release_after_motion() {
        let events = MouseEngine::with_seed(13).generate_click_events(Point { x: 2.0, y: 3.0 });
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "mousePressed");
        assert_eq!(events[1].event_type, "mouseReleased");
    }

    #[test]
    fn movement_and_click_delays_stay_in_human_ranges() {
        let engine = MouseEngine::with_seed(17);
        let move_delay = engine.move_delay(Point { x: 0.0, y: 0.0 }, Point { x: 5.0, y: 5.0 });
        assert!((Duration::from_millis(15)..=Duration::from_millis(19)).contains(&move_delay));
        assert!(
            (Duration::from_millis(40)..=Duration::from_millis(90)).contains(&engine.click_delay())
        );
    }
}
