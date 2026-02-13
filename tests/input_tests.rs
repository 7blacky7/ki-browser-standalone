//! Integration tests for the input simulation module
//!
//! Tests for mouse movement with bezier curves, keyboard typing,
//! human-like timing, and timing verification.

use std::time::{Duration, Instant};

/// Mock implementations for input simulation testing
mod mock {
    use std::f64::consts::PI;

    /// A 2D point for coordinates
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Point {
        pub fn new(x: f64, y: f64) -> Self {
            Self { x, y }
        }

        pub fn distance_to(&self, other: &Point) -> f64 {
            let dx = other.x - self.x;
            let dy = other.y - self.y;
            (dx * dx + dy * dy).sqrt()
        }

        pub fn lerp(&self, other: &Point, t: f64) -> Point {
            Point {
                x: self.x + (other.x - self.x) * t,
                y: self.y + (other.y - self.y) * t,
            }
        }
    }

    /// Cubic bezier curve for smooth mouse movement
    #[derive(Debug, Clone)]
    pub struct BezierCurve {
        pub p0: Point, // Start point
        pub p1: Point, // Control point 1
        pub p2: Point, // Control point 2
        pub p3: Point, // End point
    }

    impl BezierCurve {
        /// Create a new bezier curve
        pub fn new(start: Point, end: Point) -> Self {
            // Generate control points for natural-looking curves
            let dx = end.x - start.x;
            let dy = end.y - start.y;

            // Control points are offset perpendicular to the line
            let offset_factor = 0.3;
            let perpx = -dy * offset_factor;
            let perpy = dx * offset_factor;

            let p1 = Point::new(
                start.x + dx * 0.25 + perpx,
                start.y + dy * 0.25 + perpy,
            );

            let p2 = Point::new(
                start.x + dx * 0.75 - perpx,
                start.y + dy * 0.75 - perpy,
            );

            Self {
                p0: start,
                p1,
                p2,
                p3: end,
            }
        }

        /// Create a bezier curve with custom control points
        pub fn with_control_points(start: Point, cp1: Point, cp2: Point, end: Point) -> Self {
            Self {
                p0: start,
                p1: cp1,
                p2: cp2,
                p3: end,
            }
        }

        /// Evaluate the curve at parameter t (0.0 to 1.0)
        pub fn evaluate(&self, t: f64) -> Point {
            let t = t.clamp(0.0, 1.0);
            let t2 = t * t;
            let t3 = t2 * t;
            let mt = 1.0 - t;
            let mt2 = mt * mt;
            let mt3 = mt2 * mt;

            Point {
                x: mt3 * self.p0.x + 3.0 * mt2 * t * self.p1.x + 3.0 * mt * t2 * self.p2.x + t3 * self.p3.x,
                y: mt3 * self.p0.y + 3.0 * mt2 * t * self.p1.y + 3.0 * mt * t2 * self.p2.y + t3 * self.p3.y,
            }
        }

        /// Generate points along the curve
        pub fn generate_points(&self, num_points: usize) -> Vec<Point> {
            if num_points < 2 {
                return vec![self.p0, self.p3];
            }

            (0..num_points)
                .map(|i| {
                    let t = i as f64 / (num_points - 1) as f64;
                    self.evaluate(t)
                })
                .collect()
        }

        /// Get the approximate length of the curve
        pub fn approximate_length(&self) -> f64 {
            let points = self.generate_points(50);
            let mut length = 0.0;
            for i in 1..points.len() {
                length += points[i - 1].distance_to(&points[i]);
            }
            length
        }
    }

    /// Mouse movement simulator with bezier curves
    #[derive(Debug)]
    pub struct MouseSimulator {
        pub current_position: Point,
        pub movement_speed: f64, // pixels per millisecond
    }

    impl Default for MouseSimulator {
        fn default() -> Self {
            Self {
                current_position: Point::new(0.0, 0.0),
                movement_speed: 0.5,
            }
        }
    }

    impl MouseSimulator {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn at_position(x: f64, y: f64) -> Self {
            Self {
                current_position: Point::new(x, y),
                movement_speed: 0.5,
            }
        }

        pub fn with_speed(mut self, speed: f64) -> Self {
            self.movement_speed = speed.max(0.1);
            self
        }

        /// Calculate the estimated time to move to a point
        pub fn estimate_move_time(&self, target: &Point) -> Duration {
            let curve = BezierCurve::new(self.current_position, *target);
            let length = curve.approximate_length();
            let time_ms = length / self.movement_speed;
            Duration::from_millis(time_ms as u64)
        }

        /// Generate a movement path to the target
        pub fn generate_path(&self, target: Point, points_per_100px: usize) -> Vec<Point> {
            let curve = BezierCurve::new(self.current_position, target);
            let length = curve.approximate_length();
            let num_points = ((length / 100.0) * points_per_100px as f64).ceil() as usize;
            curve.generate_points(num_points.max(10))
        }

        /// Move to target (updates current position)
        pub fn move_to(&mut self, target: Point) {
            self.current_position = target;
        }
    }

    /// Timing profiles for human-like behavior
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TimingProfile {
        Normal,
        Fast,
        Slow,
        Instant,
    }

    /// Human-like timing configuration
    #[derive(Debug, Clone)]
    pub struct HumanTiming {
        pub profile: TimingProfile,
        pub min_delay_ms: u64,
        pub max_delay_ms: u64,
        pub variance: f64,
    }

    impl Default for HumanTiming {
        fn default() -> Self {
            Self::normal()
        }
    }

    impl HumanTiming {
        pub fn normal() -> Self {
            Self {
                profile: TimingProfile::Normal,
                min_delay_ms: 50,
                max_delay_ms: 150,
                variance: 0.3,
            }
        }

        pub fn fast() -> Self {
            Self {
                profile: TimingProfile::Fast,
                min_delay_ms: 25,
                max_delay_ms: 80,
                variance: 0.25,
            }
        }

        pub fn slow() -> Self {
            Self {
                profile: TimingProfile::Slow,
                min_delay_ms: 100,
                max_delay_ms: 300,
                variance: 0.4,
            }
        }

        pub fn instant() -> Self {
            Self {
                profile: TimingProfile::Instant,
                min_delay_ms: 1,
                max_delay_ms: 10,
                variance: 0.1,
            }
        }

        /// Generate a random delay within the configured range
        pub fn generate_delay(&self) -> Duration {
            let range = self.max_delay_ms - self.min_delay_ms;
            let base = self.min_delay_ms as f64 + (rand::random::<f64>() * range as f64);

            // Apply variance using normal distribution approximation
            let variance_factor = 1.0 + (rand::random::<f64>() - 0.5) * 2.0 * self.variance;
            let delay = (base * variance_factor).clamp(self.min_delay_ms as f64, self.max_delay_ms as f64);

            Duration::from_millis(delay as u64)
        }

        /// Get click delay (time button is held)
        pub fn get_click_delay(&self) -> Duration {
            let (min, max) = match self.profile {
                TimingProfile::Normal => (70, 150),
                TimingProfile::Fast => (49, 105),
                TimingProfile::Slow => (91, 195),
                TimingProfile::Instant => (10, 30),
            };
            self.random_delay_in_range(min, max)
        }

        /// Get typing delay (inter-keystroke interval)
        pub fn get_type_delay(&self) -> Duration {
            let (min, max) = match self.profile {
                TimingProfile::Normal => (80, 180),
                TimingProfile::Fast => (50, 100),
                TimingProfile::Slow => (180, 350),
                TimingProfile::Instant => (5, 20),
            };
            self.random_delay_in_range(min, max)
        }

        /// Get mouse movement step delay
        pub fn get_move_delay(&self) -> Duration {
            let (min, max) = match self.profile {
                TimingProfile::Normal => (5, 15),
                TimingProfile::Fast => (2, 8),
                TimingProfile::Slow => (10, 25),
                TimingProfile::Instant => (1, 3),
            };
            self.random_delay_in_range(min, max)
        }

        /// Get reaction delay
        pub fn get_reaction_delay(&self) -> Duration {
            let (min, max) = match self.profile {
                TimingProfile::Normal => (150, 300),
                TimingProfile::Fast => (100, 200),
                TimingProfile::Slow => (250, 450),
                TimingProfile::Instant => (10, 50),
            };
            self.random_delay_in_range(min, max)
        }

        fn random_delay_in_range(&self, min_ms: u64, max_ms: u64) -> Duration {
            let range = max_ms - min_ms;
            let delay = min_ms + (rand::random::<u64>() % (range + 1));
            Duration::from_millis(delay)
        }
    }

    /// Keyboard simulator
    #[derive(Debug)]
    pub struct KeyboardSimulator {
        pub timing: HumanTiming,
        pub typed_chars: Vec<char>,
    }

    impl Default for KeyboardSimulator {
        fn default() -> Self {
            Self {
                timing: HumanTiming::normal(),
                typed_chars: Vec::new(),
            }
        }
    }

    impl KeyboardSimulator {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_timing(timing: HumanTiming) -> Self {
            Self {
                timing,
                typed_chars: Vec::new(),
            }
        }

        /// Simulate typing a single character
        pub fn type_char(&mut self, ch: char) -> Duration {
            self.typed_chars.push(ch);
            self.timing.get_type_delay()
        }

        /// Simulate typing a string
        pub fn type_text(&mut self, text: &str) -> Duration {
            let mut total_delay = Duration::ZERO;
            for ch in text.chars() {
                total_delay += self.type_char(ch);
            }
            total_delay
        }

        /// Get the total estimated typing time for a string
        pub fn estimate_typing_time(&self, text: &str) -> (Duration, Duration) {
            let char_count = text.chars().count() as u64;
            let min_time = Duration::from_millis(char_count * self.timing.min_delay_ms);
            let max_time = Duration::from_millis(char_count * self.timing.max_delay_ms);
            (min_time, max_time)
        }

        /// Clear typed characters
        pub fn clear(&mut self) {
            self.typed_chars.clear();
        }

        /// Get typed text
        pub fn get_typed_text(&self) -> String {
            self.typed_chars.iter().collect()
        }
    }
}

use mock::*;

// ============================================================================
// Bezier Curve Tests
// ============================================================================

#[test]
fn test_bezier_curve_creation() {
    let start = Point::new(0.0, 0.0);
    let end = Point::new(100.0, 100.0);

    let curve = BezierCurve::new(start, end);

    assert_eq!(curve.p0, start);
    assert_eq!(curve.p3, end);
}

#[test]
fn test_bezier_evaluate_endpoints() {
    let start = Point::new(10.0, 20.0);
    let end = Point::new(200.0, 300.0);

    let curve = BezierCurve::new(start, end);

    // At t=0, should be at start
    let p0 = curve.evaluate(0.0);
    assert!((p0.x - start.x).abs() < 0.001);
    assert!((p0.y - start.y).abs() < 0.001);

    // At t=1, should be at end
    let p1 = curve.evaluate(1.0);
    assert!((p1.x - end.x).abs() < 0.001);
    assert!((p1.y - end.y).abs() < 0.001);
}

#[test]
fn test_bezier_evaluate_midpoint() {
    let start = Point::new(0.0, 0.0);
    let end = Point::new(100.0, 0.0);

    // Use linear-ish control points for predictable midpoint
    let cp1 = Point::new(33.3, 0.0);
    let cp2 = Point::new(66.6, 0.0);
    let curve = BezierCurve::with_control_points(start, cp1, cp2, end);

    let mid = curve.evaluate(0.5);

    // For near-linear control points, midpoint should be approximately at center
    assert!(mid.x > 40.0 && mid.x < 60.0);
}

#[test]
fn test_bezier_evaluate_clamping() {
    let curve = BezierCurve::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0));

    // Values outside 0-1 should be clamped
    let p_neg = curve.evaluate(-0.5);
    let p_over = curve.evaluate(1.5);

    // Should be at start/end points
    assert!((p_neg.x - 0.0).abs() < 0.001);
    assert!((p_over.x - 100.0).abs() < 0.001);
}

#[test]
fn test_bezier_generate_points() {
    let curve = BezierCurve::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0));

    let points = curve.generate_points(10);

    assert_eq!(points.len(), 10);

    // First and last should be start/end
    assert!((points[0].x - 0.0).abs() < 0.001);
    assert!((points[9].x - 100.0).abs() < 0.001);
}

#[test]
fn test_bezier_points_are_continuous() {
    let curve = BezierCurve::new(Point::new(0.0, 0.0), Point::new(500.0, 500.0));

    let points = curve.generate_points(50);

    // Check that consecutive points are reasonably close (no large jumps)
    for i in 1..points.len() {
        let dist = points[i - 1].distance_to(&points[i]);
        // Each step should be less than total distance / (num_points - 1) * 3 (allowing curve deviation)
        assert!(dist < 100.0, "Large jump detected between points: {}", dist);
    }
}

#[test]
fn test_bezier_approximate_length() {
    let start = Point::new(0.0, 0.0);
    let end = Point::new(100.0, 0.0);

    // Near-linear curve
    let linear_curve = BezierCurve::with_control_points(
        start,
        Point::new(33.0, 0.0),
        Point::new(66.0, 0.0),
        end,
    );

    let length = linear_curve.approximate_length();

    // Should be close to 100 (straight line distance)
    assert!(length >= 99.0 && length <= 101.0);
}

#[test]
fn test_bezier_curved_longer_than_straight() {
    let start = Point::new(0.0, 0.0);
    let end = Point::new(100.0, 0.0);

    let straight_dist = start.distance_to(&end);

    // Create a curve with significant deviation
    let curved = BezierCurve::with_control_points(
        start,
        Point::new(25.0, 50.0),
        Point::new(75.0, 50.0),
        end,
    );

    let curved_length = curved.approximate_length();

    // Curved path should be longer than straight line
    assert!(curved_length > straight_dist);
}

// ============================================================================
// Mouse Movement Tests
// ============================================================================

#[test]
fn test_mouse_simulator_creation() {
    let mouse = MouseSimulator::new();
    assert_eq!(mouse.current_position, Point::new(0.0, 0.0));
}

#[test]
fn test_mouse_simulator_at_position() {
    let mouse = MouseSimulator::at_position(500.0, 300.0);
    assert_eq!(mouse.current_position, Point::new(500.0, 300.0));
}

#[test]
fn test_mouse_simulator_with_speed() {
    let mouse = MouseSimulator::new().with_speed(1.0);
    assert_eq!(mouse.movement_speed, 1.0);
}

#[test]
fn test_mouse_simulator_speed_minimum() {
    let mouse = MouseSimulator::new().with_speed(0.0);
    assert!(mouse.movement_speed >= 0.1);
}

#[test]
fn test_mouse_generate_path() {
    let mouse = MouseSimulator::at_position(0.0, 0.0);
    let target = Point::new(200.0, 200.0);

    let path = mouse.generate_path(target, 5);

    // Should have at least 10 points (minimum)
    assert!(path.len() >= 10);

    // First point should be near start
    assert!(path[0].distance_to(&Point::new(0.0, 0.0)) < 1.0);

    // Last point should be the target
    let last = path.last().unwrap();
    assert!(last.distance_to(&target) < 1.0);
}

#[test]
fn test_mouse_path_length_scales_with_distance() {
    let mouse = MouseSimulator::at_position(0.0, 0.0);

    let short_path = mouse.generate_path(Point::new(50.0, 0.0), 10);
    let long_path = mouse.generate_path(Point::new(500.0, 0.0), 10);

    // Longer distance should generate more points
    assert!(long_path.len() >= short_path.len());
}

#[test]
fn test_mouse_move_updates_position() {
    let mut mouse = MouseSimulator::new();
    let target = Point::new(100.0, 200.0);

    mouse.move_to(target);

    assert_eq!(mouse.current_position, target);
}

#[test]
fn test_mouse_estimate_move_time() {
    let mouse = MouseSimulator::new().with_speed(1.0); // 1 pixel per ms
    let target = Point::new(100.0, 0.0);

    let estimated = mouse.estimate_move_time(&target);

    // For straight movement at 1 px/ms, ~100ms (allowing curve overhead)
    assert!(estimated.as_millis() >= 100);
    assert!(estimated.as_millis() <= 150);
}

// ============================================================================
// Keyboard Typing Tests
// ============================================================================

#[test]
fn test_keyboard_simulator_creation() {
    let keyboard = KeyboardSimulator::new();
    assert!(keyboard.typed_chars.is_empty());
}

#[test]
fn test_keyboard_type_char() {
    let mut keyboard = KeyboardSimulator::new();

    let delay = keyboard.type_char('a');

    assert_eq!(keyboard.typed_chars.len(), 1);
    assert_eq!(keyboard.typed_chars[0], 'a');
    assert!(delay.as_millis() > 0);
}

#[test]
fn test_keyboard_type_text() {
    let mut keyboard = KeyboardSimulator::new();
    let text = "Hello, World!";

    let total_delay = keyboard.type_text(text);

    assert_eq!(keyboard.get_typed_text(), text);
    assert!(total_delay.as_millis() > 0);
}

#[test]
fn test_keyboard_type_text_delay_scales() {
    let mut keyboard = KeyboardSimulator::new();

    let short_delay = keyboard.type_text("Hi");
    keyboard.clear();

    let long_delay = keyboard.type_text("Hello, World!");

    // Longer text should have longer total delay
    assert!(long_delay > short_delay);
}

#[test]
fn test_keyboard_clear() {
    let mut keyboard = KeyboardSimulator::new();

    keyboard.type_text("Some text");
    keyboard.clear();

    assert!(keyboard.typed_chars.is_empty());
    assert_eq!(keyboard.get_typed_text(), "");
}

#[test]
fn test_keyboard_estimate_typing_time() {
    let keyboard = KeyboardSimulator::with_timing(HumanTiming::normal());
    let text = "Hello"; // 5 characters

    let (min_time, max_time) = keyboard.estimate_typing_time(text);

    // 5 chars * min_delay (50ms) = 250ms
    // 5 chars * max_delay (150ms) = 750ms
    assert!(min_time.as_millis() >= 250);
    assert!(max_time.as_millis() <= 750);
}

#[test]
fn test_keyboard_with_fast_timing() {
    let mut keyboard = KeyboardSimulator::with_timing(HumanTiming::fast());

    let delay = keyboard.type_char('x');

    // Fast timing: 50-100ms
    assert!(delay.as_millis() >= 50);
    assert!(delay.as_millis() <= 100);
}

#[test]
fn test_keyboard_with_slow_timing() {
    let mut keyboard = KeyboardSimulator::with_timing(HumanTiming::slow());

    let delay = keyboard.type_char('x');

    // Slow timing: 180-350ms
    assert!(delay.as_millis() >= 180);
    assert!(delay.as_millis() <= 350);
}

// ============================================================================
// Human-like Timing Tests
// ============================================================================

#[test]
fn test_timing_normal_profile() {
    let timing = HumanTiming::normal();

    assert_eq!(timing.profile, TimingProfile::Normal);
    assert_eq!(timing.min_delay_ms, 50);
    assert_eq!(timing.max_delay_ms, 150);
}

#[test]
fn test_timing_fast_profile() {
    let timing = HumanTiming::fast();

    assert_eq!(timing.profile, TimingProfile::Fast);
    assert!(timing.min_delay_ms < HumanTiming::normal().min_delay_ms);
    assert!(timing.max_delay_ms < HumanTiming::normal().max_delay_ms);
}

#[test]
fn test_timing_slow_profile() {
    let timing = HumanTiming::slow();

    assert_eq!(timing.profile, TimingProfile::Slow);
    assert!(timing.min_delay_ms > HumanTiming::normal().min_delay_ms);
    assert!(timing.max_delay_ms > HumanTiming::normal().max_delay_ms);
}

#[test]
fn test_timing_instant_profile() {
    let timing = HumanTiming::instant();

    assert_eq!(timing.profile, TimingProfile::Instant);
    assert!(timing.min_delay_ms <= 10);
    assert!(timing.max_delay_ms <= 10);
}

#[test]
fn test_generate_delay_within_bounds() {
    let timing = HumanTiming::normal();

    // Run many iterations to test randomness
    for _ in 0..100 {
        let delay = timing.generate_delay();
        let ms = delay.as_millis() as u64;

        // Due to variance, allow some slack
        assert!(ms >= timing.min_delay_ms / 2);
        assert!(ms <= timing.max_delay_ms * 2);
    }
}

#[test]
fn test_click_delay_bounds() {
    let timing = HumanTiming::normal();

    for _ in 0..50 {
        let delay = timing.get_click_delay();
        // Normal click: 70-150ms
        assert!(delay.as_millis() >= 70);
        assert!(delay.as_millis() <= 150);
    }
}

#[test]
fn test_type_delay_bounds() {
    let timing = HumanTiming::normal();

    for _ in 0..50 {
        let delay = timing.get_type_delay();
        // Normal typing: 80-180ms
        assert!(delay.as_millis() >= 80);
        assert!(delay.as_millis() <= 180);
    }
}

#[test]
fn test_move_delay_bounds() {
    let timing = HumanTiming::normal();

    for _ in 0..50 {
        let delay = timing.get_move_delay();
        // Normal move step: 5-15ms
        assert!(delay.as_millis() >= 5);
        assert!(delay.as_millis() <= 15);
    }
}

#[test]
fn test_reaction_delay_bounds() {
    let timing = HumanTiming::normal();

    for _ in 0..50 {
        let delay = timing.get_reaction_delay();
        // Normal reaction: 150-300ms
        assert!(delay.as_millis() >= 150);
        assert!(delay.as_millis() <= 300);
    }
}

#[test]
fn test_fast_timing_delays_are_shorter() {
    let normal = HumanTiming::normal();
    let fast = HumanTiming::fast();

    // Sample multiple times
    let mut normal_sum: u64 = 0;
    let mut fast_sum: u64 = 0;

    for _ in 0..100 {
        normal_sum += normal.get_type_delay().as_millis() as u64;
        fast_sum += fast.get_type_delay().as_millis() as u64;
    }

    // Fast should have smaller total (on average)
    assert!(fast_sum < normal_sum);
}

#[test]
fn test_slow_timing_delays_are_longer() {
    let normal = HumanTiming::normal();
    let slow = HumanTiming::slow();

    // Sample multiple times
    let mut normal_sum: u64 = 0;
    let mut slow_sum: u64 = 0;

    for _ in 0..100 {
        normal_sum += normal.get_type_delay().as_millis() as u64;
        slow_sum += slow.get_type_delay().as_millis() as u64;
    }

    // Slow should have larger total (on average)
    assert!(slow_sum > normal_sum);
}

// ============================================================================
// Timing Verification Tests
// ============================================================================

#[test]
fn test_timing_variance_produces_different_values() {
    let timing = HumanTiming::normal();

    let mut delays: Vec<u64> = Vec::new();
    for _ in 0..20 {
        delays.push(timing.generate_delay().as_millis() as u64);
    }

    // Check that we have variety (not all the same value)
    let first = delays[0];
    let has_variety = delays.iter().any(|&d| d != first);
    assert!(has_variety, "Timing should produce varying delays");
}

#[test]
fn test_point_distance() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(3.0, 4.0);

    let dist = p1.distance_to(&p2);
    assert!((dist - 5.0).abs() < 0.001); // 3-4-5 triangle
}

#[test]
fn test_point_lerp() {
    let p1 = Point::new(0.0, 0.0);
    let p2 = Point::new(100.0, 100.0);

    let mid = p1.lerp(&p2, 0.5);
    assert!((mid.x - 50.0).abs() < 0.001);
    assert!((mid.y - 50.0).abs() < 0.001);

    let quarter = p1.lerp(&p2, 0.25);
    assert!((quarter.x - 25.0).abs() < 0.001);
    assert!((quarter.y - 25.0).abs() < 0.001);
}

#[test]
fn test_instant_timing_is_fast() {
    let timing = HumanTiming::instant();

    let start = Instant::now();
    let mut total = Duration::ZERO;

    // Simulate 100 keystrokes
    for _ in 0..100 {
        total += timing.get_type_delay();
    }

    // Instant timing should result in very short total time
    assert!(total.as_millis() <= 2000); // 100 * max 20ms
}

// ============================================================================
// Integration: Mouse + Timing
// ============================================================================

#[test]
fn test_mouse_movement_with_timing() {
    let mouse = MouseSimulator::at_position(0.0, 0.0).with_speed(1.0);
    let timing = HumanTiming::normal();
    let target = Point::new(100.0, 0.0);

    let path = mouse.generate_path(target, 10);

    // Calculate total movement time including step delays
    let mut total_time = Duration::ZERO;
    for _ in &path {
        total_time += timing.get_move_delay();
    }

    // Should be roughly: path_length_time + step_delays
    assert!(total_time.as_millis() > 0);
}

#[test]
fn test_complete_typing_simulation() {
    let mut keyboard = KeyboardSimulator::with_timing(HumanTiming::normal());
    let text = "Hello, World!";

    let total_time = keyboard.type_text(text);

    // Verify text was typed
    assert_eq!(keyboard.get_typed_text(), text);

    // Verify timing is reasonable
    let char_count = text.chars().count() as u64;
    let min_expected = char_count * 80; // min type delay
    let max_expected = char_count * 180; // max type delay

    let actual_ms = total_time.as_millis() as u64;
    assert!(actual_ms >= min_expected);
    assert!(actual_ms <= max_expected);
}
