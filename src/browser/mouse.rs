use std::time::Duration;
use tracing::debug;

/// A 2D point for mouse path calculations.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Bezier mouse engine for realistic cursor movement.
pub struct MouseEngine {
    /// Human-like speed variation (pixels per second range)
    pub min_speed: f64,
    pub max_speed: f64,
    /// Steps per second for the movement curve
    pub steps_per_second: u32,
}

impl Default for MouseEngine {
    fn default() -> Self {
        Self {
            min_speed: 400.0,
            max_speed: 800.0,
            steps_per_second: 60,
        }
    }
}

impl MouseEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate a cubic Bezier curve between two points with control points.
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

    /// Generate control points for a natural-looking curve between two points.
    fn generate_control_points(&self, start: Point, end: Point) -> (Point, Point) {
        let distance = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();

        // Control point offset proportional to distance, with some randomness
        let offset = distance * 0.3;

        // Add slight curvature (humans rarely move in perfectly straight lines)
        let curvature = ((end.x - start.x) * 0.1 + (end.y - start.y) * 0.05).abs();

        let cp1 = Point {
            x: start.x + (end.x - start.x) * 0.25 + curvature,
            y: start.y + (end.y - start.y) * 0.25 + offset * 0.5,
        };

        let cp2 = Point {
            x: start.x + (end.x - start.x) * 0.75 - curvature * 0.5,
            y: start.y + (end.y - start.y) * 0.75 - offset * 0.3,
        };

        (cp1, cp2)
    }

    /// Generate a list of points along a Bezier curve from start to end.
    pub fn generate_path(&self, start: Point, end: Point) -> Vec<Point> {
        let distance = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();

        // Number of steps proportional to distance
        let steps = ((distance / self.steps_per_second as f64) * 60.0).max(10.0) as usize;

        let (cp1, cp2) = self.generate_control_points(start, end);

        let mut points = Vec::with_capacity(steps + 1);

        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            // Ease-in-out timing
            let t_eased = if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            };
            points.push(self.bezier_curve(start, end, cp1, cp2, t_eased));
        }

        points
    }

    /// Calculate the delay between moves based on speed and distance.
    fn move_delay(&self, start: Point, end: Point) -> Duration {
        let distance = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
        let speed = self.min_speed + (self.max_speed - self.min_speed) * 0.5; // Mid-range speed
        let seconds = (distance / speed).max(0.005); // Minimum 5ms between moves
        Duration::from_secs_f64(seconds)
    }

    /// Move the mouse along a Bezier curve from start to end, calling a callback for each step.
    pub async fn move_to<F>(
        &self,
        start: Point,
        end: Point,
        mut callback: F,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnMut(Point) -> futures_util::future::BoxFuture<'static, ()>,
    {
        let path = self.generate_path(start, end);
        debug!(
            "Mouse path: {} steps, {}px distance",
            path.len(),
            ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt() as u32
        );

        for point in &path {
            callback(*point).await;
            tokio::time::sleep(self.move_delay(*point, *point)).await;
        }

        Ok(())
    }

    /// Generate the mouse events for a click at a given point.
    pub fn generate_click_events(&self, point: Point) -> Vec<MouseEvent> {
        vec![
            MouseEvent {
                event_type: "mouseMoved".to_string(),
                x: point.x,
                y: point.y,
                button: "none".to_string(),
                click_count: 0,
            },
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

    /// Generate the mouse events for a double-click at a given point.
    pub fn generate_double_click_events(&self, point: Point) -> Vec<MouseEvent> {
        vec![
            MouseEvent {
                event_type: "mouseMoved".to_string(),
                x: point.x,
                y: point.y,
                button: "none".to_string(),
                click_count: 0,
            },
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

    /// Generate mouse events for a drag from start to end.
    pub fn generate_drag_events(&self, start: Point, end: Point) -> Vec<MouseEvent> {
        let path = self.generate_path(start, end);
        let mut events = Vec::new();

        // Mouse down at start
        events.push(MouseEvent {
            event_type: "mousePressed".to_string(),
            x: start.x,
            y: start.y,
            button: "left".to_string(),
            click_count: 1,
        });

        // Mouse moves along path
        for point in &path[1..path.len() - 1] {
            events.push(MouseEvent {
                event_type: "mouseMoved".to_string(),
                x: point.x,
                y: point.y,
                button: "left".to_string(),
                click_count: 1,
            });
        }

        // Mouse up at end
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

/// A mouse event to be dispatched via CDP.
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
    fn test_bezier_curve_endpoint() {
        let engine = MouseEngine::new();
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 100.0, y: 100.0 };
        let (cp1, cp2) = engine.generate_control_points(start, end);

        let at_zero = engine.bezier_curve(start, end, cp1, cp2, 0.0);
        let at_one = engine.bezier_curve(start, end, cp1, cp2, 1.0);

        assert!((at_zero.x - start.x).abs() < 0.001);
        assert!((at_zero.y - start.y).abs() < 0.001);
        assert!((at_one.x - end.x).abs() < 0.001);
        assert!((at_one.y - end.y).abs() < 0.001);
    }

    #[test]
    fn test_path_generation() {
        let engine = MouseEngine::new();
        let start = Point { x: 0.0, y: 0.0 };
        let end = Point { x: 200.0, y: 100.0 };
        let path = engine.generate_path(start, end);

        assert!(path.len() >= 10);
        // First point should be at start
        assert!((path[0].x - start.x).abs() < 1.0);
        assert!((path[0].y - start.y).abs() < 1.0);
        // Last point should be at end
        assert!((path.last().unwrap().x - end.x).abs() < 1.0);
        assert!((path.last().unwrap().y - end.y).abs() < 1.0);
    }
}
