//! Bézier curve implementation for realistic mouse movement
//!
//! This module provides cubic Bézier curve functionality to generate
//! human-like mouse movement paths with natural acceleration and curves.
//!
//! # Example
//!
//! ```rust
//! use ki_browser::input::bezier::{Point, BezierCurve, generate_human_path};
//!
//! // Create a simple Bézier curve
//! let start = Point::new(0.0, 0.0);
//! let end = Point::new(100.0, 100.0);
//! let control1 = Point::new(25.0, 50.0);
//! let control2 = Point::new(75.0, 50.0);
//!
//! let curve = BezierCurve::new(start, control1, control2, end);
//! let midpoint = curve.evaluate_at(0.5);
//!
//! // Generate a human-like path
//! let path = generate_human_path(start, end, 50);
//! ```

use std::f64::consts::PI;

/// A 2D point with f64 coordinates
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// X coordinate
    pub x: f64,
    /// Y coordinate
    pub y: f64,
}

impl Point {
    /// Creates a new point
    ///
    /// # Arguments
    ///
    /// * `x` - X coordinate
    /// * `y` - Y coordinate
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::bezier::Point;
    ///
    /// let p = Point::new(10.0, 20.0);
    /// assert_eq!(p.x, 10.0);
    /// assert_eq!(p.y, 20.0);
    /// ```
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Creates a point at the origin (0, 0)
    pub fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Calculates the Euclidean distance to another point
    ///
    /// # Arguments
    ///
    /// * `other` - The other point
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::bezier::Point;
    ///
    /// let a = Point::new(0.0, 0.0);
    /// let b = Point::new(3.0, 4.0);
    /// assert_eq!(a.distance_to(&b), 5.0);
    /// ```
    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Calculates the angle to another point in radians
    ///
    /// # Arguments
    ///
    /// * `other` - The other point
    pub fn angle_to(&self, other: &Point) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        dy.atan2(dx)
    }

    /// Linear interpolation between this point and another
    ///
    /// # Arguments
    ///
    /// * `other` - The other point
    /// * `t` - Interpolation factor (0.0 = this point, 1.0 = other point)
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::bezier::Point;
    ///
    /// let a = Point::new(0.0, 0.0);
    /// let b = Point::new(10.0, 10.0);
    /// let mid = a.lerp(&b, 0.5);
    /// assert_eq!(mid.x, 5.0);
    /// assert_eq!(mid.y, 5.0);
    /// ```
    pub fn lerp(&self, other: &Point, t: f64) -> Point {
        Point {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }

    /// Adds a vector offset to this point
    pub fn offset(&self, dx: f64, dy: f64) -> Point {
        Point {
            x: self.x + dx,
            y: self.y + dy,
        }
    }

    /// Returns the magnitude (length) if treated as a vector from origin
    pub fn magnitude(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns a normalized version of this point (as a unit vector)
    pub fn normalized(&self) -> Point {
        let mag = self.magnitude();
        if mag > 0.0 {
            Point {
                x: self.x / mag,
                y: self.y / mag,
            }
        } else {
            *self
        }
    }
}

impl std::ops::Add for Point {
    type Output = Point;

    fn add(self, other: Point) -> Point {
        Point {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Sub for Point {
    type Output = Point;

    fn sub(self, other: Point) -> Point {
        Point {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Mul<f64> for Point {
    type Output = Point;

    fn mul(self, scalar: f64) -> Point {
        Point {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

impl Default for Point {
    fn default() -> Self {
        Self::origin()
    }
}

/// A cubic Bézier curve defined by four control points
///
/// The curve starts at `p0`, ends at `p3`, and is shaped by the
/// intermediate control points `p1` and `p2`.
#[derive(Debug, Clone)]
pub struct BezierCurve {
    /// Start point
    pub p0: Point,
    /// First control point
    pub p1: Point,
    /// Second control point
    pub p2: Point,
    /// End point
    pub p3: Point,
}

impl BezierCurve {
    /// Creates a new cubic Bézier curve
    ///
    /// # Arguments
    ///
    /// * `p0` - Start point
    /// * `p1` - First control point
    /// * `p2` - Second control point
    /// * `p3` - End point
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::bezier::{Point, BezierCurve};
    ///
    /// let curve = BezierCurve::new(
    ///     Point::new(0.0, 0.0),
    ///     Point::new(25.0, 100.0),
    ///     Point::new(75.0, 100.0),
    ///     Point::new(100.0, 0.0),
    /// );
    /// ```
    pub fn new(p0: Point, p1: Point, p2: Point, p3: Point) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// Creates a curve from start to end with automatically generated control points
    ///
    /// The control points are generated to create a smooth, natural-looking curve.
    pub fn from_endpoints(start: Point, end: Point) -> Self {
        let distance = start.distance_to(&end);
        let angle = start.angle_to(&end);

        // Generate control points with some perpendicular offset
        let offset = distance * 0.3;
        let perp_angle = angle + PI / 2.0;

        // Add some randomness to control point positions
        let rand1 = (rand::random::<f64>() - 0.5) * 2.0;
        let rand2 = (rand::random::<f64>() - 0.5) * 2.0;

        let p1 = Point::new(
            start.x + distance * 0.3 * angle.cos() + offset * rand1 * perp_angle.cos(),
            start.y + distance * 0.3 * angle.sin() + offset * rand1 * perp_angle.sin(),
        );

        let p2 = Point::new(
            start.x + distance * 0.7 * angle.cos() + offset * rand2 * perp_angle.cos(),
            start.y + distance * 0.7 * angle.sin() + offset * rand2 * perp_angle.sin(),
        );

        Self::new(start, p1, p2, end)
    }

    /// Evaluates the curve at parameter t
    ///
    /// # Arguments
    ///
    /// * `t` - Parameter value from 0.0 (start) to 1.0 (end)
    ///
    /// # Returns
    ///
    /// The point on the curve at parameter t
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::bezier::{Point, BezierCurve};
    ///
    /// let curve = BezierCurve::new(
    ///     Point::new(0.0, 0.0),
    ///     Point::new(25.0, 50.0),
    ///     Point::new(75.0, 50.0),
    ///     Point::new(100.0, 0.0),
    /// );
    ///
    /// let start = curve.evaluate_at(0.0);
    /// assert_eq!(start.x, 0.0);
    /// assert_eq!(start.y, 0.0);
    ///
    /// let end = curve.evaluate_at(1.0);
    /// assert_eq!(end.x, 100.0);
    /// assert_eq!(end.y, 0.0);
    /// ```
    pub fn evaluate_at(&self, t: f64) -> Point {
        let t = t.clamp(0.0, 1.0);
        let t2 = t * t;
        let t3 = t2 * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;
        let mt3 = mt2 * mt;

        // Cubic Bézier formula: B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
        Point {
            x: mt3 * self.p0.x + 3.0 * mt2 * t * self.p1.x + 3.0 * mt * t2 * self.p2.x + t3 * self.p3.x,
            y: mt3 * self.p0.y + 3.0 * mt2 * t * self.p1.y + 3.0 * mt * t2 * self.p2.y + t3 * self.p3.y,
        }
    }

    /// Evaluates the derivative (tangent) of the curve at parameter t
    ///
    /// # Arguments
    ///
    /// * `t` - Parameter value from 0.0 to 1.0
    ///
    /// # Returns
    ///
    /// The tangent vector at parameter t
    pub fn derivative_at(&self, t: f64) -> Point {
        let t = t.clamp(0.0, 1.0);
        let t2 = t * t;
        let mt = 1.0 - t;
        let mt2 = mt * mt;

        // Derivative of cubic Bézier
        Point {
            x: 3.0 * mt2 * (self.p1.x - self.p0.x)
                + 6.0 * mt * t * (self.p2.x - self.p1.x)
                + 3.0 * t2 * (self.p3.x - self.p2.x),
            y: 3.0 * mt2 * (self.p1.y - self.p0.y)
                + 6.0 * mt * t * (self.p2.y - self.p1.y)
                + 3.0 * t2 * (self.p3.y - self.p2.y),
        }
    }

    /// Generates a series of points along the curve
    ///
    /// # Arguments
    ///
    /// * `num_points` - Number of points to generate
    ///
    /// # Returns
    ///
    /// A vector of points evenly distributed along the curve parameter
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::bezier::{Point, BezierCurve};
    ///
    /// let curve = BezierCurve::new(
    ///     Point::new(0.0, 0.0),
    ///     Point::new(25.0, 50.0),
    ///     Point::new(75.0, 50.0),
    ///     Point::new(100.0, 0.0),
    /// );
    ///
    /// let points = curve.generate_points(11);
    /// assert_eq!(points.len(), 11);
    /// assert_eq!(points[0], curve.evaluate_at(0.0));
    /// assert_eq!(points[10], curve.evaluate_at(1.0));
    /// ```
    pub fn generate_points(&self, num_points: usize) -> Vec<Point> {
        if num_points == 0 {
            return vec![];
        }
        if num_points == 1 {
            return vec![self.p0];
        }

        let mut points = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let t = i as f64 / (num_points - 1) as f64;
            points.push(self.evaluate_at(t));
        }
        points
    }

    /// Generates points with arc-length parameterization
    ///
    /// This creates more evenly spaced points along the actual curve length,
    /// rather than evenly spaced in parameter space.
    ///
    /// # Arguments
    ///
    /// * `num_points` - Number of points to generate
    pub fn generate_arc_length_points(&self, num_points: usize) -> Vec<Point> {
        if num_points <= 2 {
            return self.generate_points(num_points);
        }

        // First, sample many points to estimate arc length
        let samples = 100;
        let mut lengths = Vec::with_capacity(samples + 1);
        let mut total_length = 0.0;
        let mut prev_point = self.p0;
        lengths.push(0.0);

        for i in 1..=samples {
            let t = i as f64 / samples as f64;
            let point = self.evaluate_at(t);
            total_length += prev_point.distance_to(&point);
            lengths.push(total_length);
            prev_point = point;
        }

        // Now generate points at even arc-length intervals
        let mut points = Vec::with_capacity(num_points);
        points.push(self.p0);

        for i in 1..num_points - 1 {
            let target_length = total_length * i as f64 / (num_points - 1) as f64;

            // Binary search for the t value
            let mut low = 0;
            let mut high = samples;
            while high - low > 1 {
                let mid = (low + high) / 2;
                if lengths[mid] < target_length {
                    low = mid;
                } else {
                    high = mid;
                }
            }

            // Interpolate t value
            let t_low = low as f64 / samples as f64;
            let t_high = high as f64 / samples as f64;
            let length_low = lengths[low];
            let length_high = lengths[high];

            let t = if (length_high - length_low).abs() > 1e-10 {
                t_low + (t_high - t_low) * (target_length - length_low) / (length_high - length_low)
            } else {
                t_low
            };

            points.push(self.evaluate_at(t));
        }

        points.push(self.p3);
        points
    }

    /// Calculates the approximate arc length of the curve
    pub fn arc_length(&self) -> f64 {
        let samples = 50;
        let mut length = 0.0;
        let mut prev = self.p0;

        for i in 1..=samples {
            let t = i as f64 / samples as f64;
            let point = self.evaluate_at(t);
            length += prev.distance_to(&point);
            prev = point;
        }

        length
    }
}

/// Easing function type for curve animation
pub type EasingFn = fn(f64) -> f64;

/// Collection of common easing functions
pub mod easing {
    use std::f64::consts::PI;

    /// Linear easing (no easing)
    pub fn linear(t: f64) -> f64 {
        t
    }

    /// Quadratic ease-in
    pub fn ease_in_quad(t: f64) -> f64 {
        t * t
    }

    /// Quadratic ease-out
    pub fn ease_out_quad(t: f64) -> f64 {
        1.0 - (1.0 - t) * (1.0 - t)
    }

    /// Quadratic ease-in-out
    pub fn ease_in_out_quad(t: f64) -> f64 {
        if t < 0.5 {
            2.0 * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
        }
    }

    /// Cubic ease-in
    pub fn ease_in_cubic(t: f64) -> f64 {
        t * t * t
    }

    /// Cubic ease-out
    pub fn ease_out_cubic(t: f64) -> f64 {
        1.0 - (1.0 - t).powi(3)
    }

    /// Cubic ease-in-out
    pub fn ease_in_out_cubic(t: f64) -> f64 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    /// Sine ease-in
    pub fn ease_in_sine(t: f64) -> f64 {
        1.0 - (t * PI / 2.0).cos()
    }

    /// Sine ease-out
    pub fn ease_out_sine(t: f64) -> f64 {
        (t * PI / 2.0).sin()
    }

    /// Sine ease-in-out
    pub fn ease_in_out_sine(t: f64) -> f64 {
        -(((PI * t).cos() - 1.0) / 2.0)
    }

    /// Exponential ease-in
    pub fn ease_in_expo(t: f64) -> f64 {
        if t == 0.0 {
            0.0
        } else {
            (2.0_f64).powf(10.0 * t - 10.0)
        }
    }

    /// Exponential ease-out
    pub fn ease_out_expo(t: f64) -> f64 {
        if t == 1.0 {
            1.0
        } else {
            1.0 - (2.0_f64).powf(-10.0 * t)
        }
    }

    /// Back ease-in (slight overshoot at start)
    pub fn ease_in_back(t: f64) -> f64 {
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        c3 * t * t * t - c1 * t * t
    }

    /// Back ease-out (slight overshoot at end)
    pub fn ease_out_back(t: f64) -> f64 {
        let c1 = 1.70158;
        let c3 = c1 + 1.0;
        1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
    }
}

/// Generates a human-like path between two points using Bézier curves
///
/// This function creates a path that mimics natural human hand movement,
/// including slight curves, acceleration/deceleration, and optional micro-movements.
///
/// # Arguments
///
/// * `start` - Starting point
/// * `end` - Ending point
/// * `num_points` - Number of points in the path
///
/// # Returns
///
/// A vector of points representing the path
///
/// # Example
///
/// ```rust
/// use ki_browser::input::bezier::{Point, generate_human_path};
///
/// let start = Point::new(100.0, 100.0);
/// let end = Point::new(500.0, 300.0);
/// let path = generate_human_path(start, end, 50);
///
/// assert_eq!(path.len(), 50);
/// assert_eq!(path[0].x, start.x);
/// assert_eq!(path[0].y, start.y);
/// ```
pub fn generate_human_path(start: Point, end: Point, num_points: usize) -> Vec<Point> {
    if num_points == 0 {
        return vec![];
    }
    if num_points == 1 {
        return vec![start];
    }
    if num_points == 2 {
        return vec![start, end];
    }

    let distance = start.distance_to(&end);

    // For very short distances, use simple linear interpolation with slight randomness
    if distance < 10.0 {
        return generate_short_path(start, end, num_points);
    }

    // Generate control points that simulate human hand movement
    let (control1, control2) = generate_human_control_points(start, end, distance);

    // Create the main Bézier curve
    let curve = BezierCurve::new(start, control1, control2, end);

    // Generate points with arc-length parameterization for more natural spacing
    let mut points = curve.generate_arc_length_points(num_points);

    // Apply easing to simulate acceleration/deceleration
    apply_human_easing(&mut points, start, end);

    points
}

/// Generates control points that create human-like curves
fn generate_human_control_points(start: Point, end: Point, distance: f64) -> (Point, Point) {
    let angle = start.angle_to(&end);

    // Human movements often have a slight arc, not perfectly straight
    // The arc direction and magnitude vary based on distance and random factors
    let arc_factor = distance * (0.1 + rand::random::<f64>() * 0.2);

    // Randomly choose whether to arc above or below the direct line
    let arc_direction = if rand::random::<bool>() { 1.0 } else { -1.0 };

    // Perpendicular angle for the arc
    let perp_angle = angle + PI / 2.0 * arc_direction;

    // Control point positions along the path (with some randomness)
    let cp1_dist_factor = 0.2 + rand::random::<f64>() * 0.15;
    let cp2_dist_factor = 0.65 + rand::random::<f64>() * 0.15;

    // Arc offset varies - stronger in the middle of the path
    let cp1_arc = arc_factor * (0.5 + rand::random::<f64>() * 0.5);
    let cp2_arc = arc_factor * (0.3 + rand::random::<f64>() * 0.4);

    let control1 = Point::new(
        start.x + distance * cp1_dist_factor * angle.cos() + cp1_arc * perp_angle.cos(),
        start.y + distance * cp1_dist_factor * angle.sin() + cp1_arc * perp_angle.sin(),
    );

    let control2 = Point::new(
        start.x + distance * cp2_dist_factor * angle.cos() + cp2_arc * perp_angle.cos(),
        start.y + distance * cp2_dist_factor * angle.sin() + cp2_arc * perp_angle.sin(),
    );

    (control1, control2)
}

/// Generates a path for very short distances
fn generate_short_path(start: Point, end: Point, num_points: usize) -> Vec<Point> {
    let mut points = Vec::with_capacity(num_points);

    for i in 0..num_points {
        let t = i as f64 / (num_points - 1) as f64;

        // Add tiny random wobble for realism
        let wobble_x = (rand::random::<f64>() - 0.5) * 0.5;
        let wobble_y = (rand::random::<f64>() - 0.5) * 0.5;

        let point = Point::new(
            start.x + (end.x - start.x) * t + wobble_x,
            start.y + (end.y - start.y) * t + wobble_y,
        );
        points.push(point);
    }

    // Ensure exact start and end points
    points[0] = start;
    points[num_points - 1] = end;

    points
}

/// Applies human-like easing to a path
///
/// Human movements typically start slow, accelerate in the middle,
/// and decelerate near the target (Fitts's Law)
fn apply_human_easing(points: &mut [Point], _start: Point, end: Point) {
    if points.len() < 3 {
        return;
    }

    let n = points.len();

    // Apply subtle timing adjustments near the end (deceleration)
    // This simulates the "homing in" behavior as the cursor approaches the target
    let decel_start = (n as f64 * 0.7) as usize;

    for i in decel_start..n {
        let progress = (i - decel_start) as f64 / (n - decel_start) as f64;
        let correction_factor = 1.0 - progress * 0.1; // Subtle correction

        // Slightly adjust points toward the end target
        let current = points[i];
        points[i] = Point::new(
            current.x + (end.x - current.x) * (1.0 - correction_factor) * 0.1,
            current.y + (end.y - current.y) * (1.0 - correction_factor) * 0.1,
        );
    }

    // Ensure the last point is exactly the target
    points[n - 1] = end;
}

/// Generates multiple path variations for A/B testing or random selection
pub fn generate_path_variations(
    start: Point,
    end: Point,
    num_points: usize,
    num_variations: usize,
) -> Vec<Vec<Point>> {
    (0..num_variations)
        .map(|_| generate_human_path(start, end, num_points))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_new() {
        let p = Point::new(10.0, 20.0);
        assert_eq!(p.x, 10.0);
        assert_eq!(p.y, 20.0);
    }

    #[test]
    fn test_point_distance() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert!((a.distance_to(&b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_point_lerp() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 10.0);

        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid.x, 5.0);
        assert_eq!(mid.y, 5.0);

        let quarter = a.lerp(&b, 0.25);
        assert_eq!(quarter.x, 2.5);
        assert_eq!(quarter.y, 2.5);
    }

    #[test]
    fn test_point_operations() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(3.0, 4.0);

        let sum = a + b;
        assert_eq!(sum.x, 4.0);
        assert_eq!(sum.y, 6.0);

        let diff = b - a;
        assert_eq!(diff.x, 2.0);
        assert_eq!(diff.y, 2.0);

        let scaled = a * 2.0;
        assert_eq!(scaled.x, 2.0);
        assert_eq!(scaled.y, 4.0);
    }

    #[test]
    fn test_bezier_endpoints() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(25.0, 50.0);
        let p2 = Point::new(75.0, 50.0);
        let p3 = Point::new(100.0, 0.0);

        let curve = BezierCurve::new(p0, p1, p2, p3);

        let start = curve.evaluate_at(0.0);
        assert!((start.x - p0.x).abs() < 1e-10);
        assert!((start.y - p0.y).abs() < 1e-10);

        let end = curve.evaluate_at(1.0);
        assert!((end.x - p3.x).abs() < 1e-10);
        assert!((end.y - p3.y).abs() < 1e-10);
    }

    #[test]
    fn test_bezier_generate_points() {
        let curve = BezierCurve::new(
            Point::new(0.0, 0.0),
            Point::new(25.0, 50.0),
            Point::new(75.0, 50.0),
            Point::new(100.0, 0.0),
        );

        let points = curve.generate_points(11);
        assert_eq!(points.len(), 11);

        // First and last should match endpoints
        assert!((points[0].x - 0.0).abs() < 1e-10);
        assert!((points[10].x - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_generate_human_path() {
        let start = Point::new(0.0, 0.0);
        let end = Point::new(100.0, 100.0);

        let path = generate_human_path(start, end, 20);

        assert_eq!(path.len(), 20);
        assert_eq!(path[0].x, start.x);
        assert_eq!(path[0].y, start.y);
        assert_eq!(path[19].x, end.x);
        assert_eq!(path[19].y, end.y);
    }

    #[test]
    fn test_easing_functions() {
        // Test that easing functions return correct boundary values
        assert_eq!(easing::linear(0.0), 0.0);
        assert_eq!(easing::linear(1.0), 1.0);

        assert_eq!(easing::ease_in_quad(0.0), 0.0);
        assert_eq!(easing::ease_in_quad(1.0), 1.0);

        assert_eq!(easing::ease_out_quad(0.0), 0.0);
        assert!((easing::ease_out_quad(1.0) - 1.0).abs() < 1e-10);

        // Test that ease_in is slower at start (lower value at t=0.5)
        assert!(easing::ease_in_quad(0.5) < 0.5);

        // Test that ease_out is faster at start (higher value at t=0.5)
        assert!(easing::ease_out_quad(0.5) > 0.5);
    }

    #[test]
    fn test_bezier_arc_length() {
        // A straight line should have arc length equal to distance
        let start = Point::new(0.0, 0.0);
        let end = Point::new(100.0, 0.0);
        let curve = BezierCurve::new(start, start.lerp(&end, 0.33), start.lerp(&end, 0.66), end);

        let arc_length = curve.arc_length();
        assert!((arc_length - 100.0).abs() < 1.0); // Allow small error due to sampling
    }
}
