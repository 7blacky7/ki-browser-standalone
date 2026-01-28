//! Mouse simulation module for human-like mouse interactions
//!
//! This module provides realistic mouse simulation including movement with
//! Bézier curves, clicks, double-clicks, scrolling, and drag operations.
//!
//! # Example
//!
//! ```rust,no_run
//! use ki_browser::input::mouse::{MouseSimulator, MouseButton};
//!
//! async fn example() {
//!     let mut mouse = MouseSimulator::new();
//!
//!     // Move to position with human-like path
//!     mouse.move_to(500.0, 300.0).await.unwrap();
//!
//!     // Click at current position
//!     mouse.click(MouseButton::Left).await.unwrap();
//!
//!     // Drag to a new position
//!     mouse.drag_to(600.0, 400.0, MouseButton::Left).await.unwrap();
//! }
//! ```

use super::bezier::{generate_human_path, Point};
use super::timing::HumanTiming;
use super::{InputError, InputResult};
use std::time::Duration;

/// Represents the different mouse buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left mouse button (primary)
    Left,
    /// Right mouse button (secondary/context menu)
    Right,
    /// Middle mouse button (scroll wheel click)
    Middle,
}

impl MouseButton {
    /// Returns the platform-specific button code
    pub fn button_code(&self) -> u8 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Right => 2,
            MouseButton::Middle => 1,
        }
    }
}

impl std::fmt::Display for MouseButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MouseButton::Left => write!(f, "left"),
            MouseButton::Right => write!(f, "right"),
            MouseButton::Middle => write!(f, "middle"),
        }
    }
}

/// Represents different types of mouse events
#[derive(Debug, Clone, PartialEq)]
pub enum MouseEvent {
    /// Mouse moved to a position
    Move { x: f64, y: f64 },
    /// Single click at current position
    Click { button: MouseButton },
    /// Double click at current position
    DoubleClick { button: MouseButton },
    /// Mouse button pressed down
    Down { button: MouseButton },
    /// Mouse button released
    Up { button: MouseButton },
    /// Scroll wheel movement
    Scroll { delta_x: f64, delta_y: f64 },
}

/// Configuration for mouse simulation behavior
#[derive(Debug, Clone)]
pub struct MouseConfig {
    /// Minimum number of points for movement path
    pub min_path_points: usize,
    /// Maximum number of points for movement path
    pub max_path_points: usize,
    /// Whether to add random micro-movements
    pub add_jitter: bool,
    /// Jitter intensity (0.0 - 1.0)
    pub jitter_intensity: f64,
    /// Screen bounds for validation
    pub screen_bounds: Option<(f64, f64)>,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            min_path_points: 20,
            max_path_points: 50,
            add_jitter: true,
            jitter_intensity: 0.3,
            screen_bounds: None,
        }
    }
}

/// Simulates realistic human-like mouse movements and interactions
#[derive(Debug)]
pub struct MouseSimulator {
    /// Current mouse position
    current_position: Point,
    /// Configuration for mouse behavior
    config: MouseConfig,
    /// Timing utility for realistic delays
    timing: HumanTiming,
    /// History of recent mouse events (for pattern analysis)
    event_history: Vec<MouseEvent>,
    /// Maximum events to keep in history
    history_limit: usize,
}

impl Default for MouseSimulator {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseSimulator {
    /// Creates a new MouseSimulator with default settings
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::mouse::MouseSimulator;
    ///
    /// let mouse = MouseSimulator::new();
    /// ```
    pub fn new() -> Self {
        Self {
            current_position: Point::new(0.0, 0.0),
            config: MouseConfig::default(),
            timing: HumanTiming::default(),
            event_history: Vec::new(),
            history_limit: 100,
        }
    }

    /// Creates a new MouseSimulator with custom configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Custom mouse configuration
    /// * `timing` - Custom timing settings
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::mouse::{MouseSimulator, MouseConfig};
    /// use ki_browser::input::timing::HumanTiming;
    ///
    /// let config = MouseConfig {
    ///     add_jitter: false,
    ///     ..Default::default()
    /// };
    /// let mouse = MouseSimulator::with_config(config, HumanTiming::fast());
    /// ```
    pub fn with_config(config: MouseConfig, timing: HumanTiming) -> Self {
        Self {
            current_position: Point::new(0.0, 0.0),
            config,
            timing,
            event_history: Vec::new(),
            history_limit: 100,
        }
    }

    /// Returns the current mouse position
    pub fn position(&self) -> Point {
        self.current_position
    }

    /// Sets the current position without animation
    ///
    /// This is useful for initialization or teleporting the cursor.
    pub fn set_position(&mut self, x: f64, y: f64) {
        self.current_position = Point::new(x, y);
    }

    /// Validates that coordinates are within screen bounds
    fn validate_position(&self, x: f64, y: f64) -> InputResult<()> {
        if let Some((max_x, max_y)) = self.config.screen_bounds {
            if x < 0.0 || y < 0.0 || x > max_x || y > max_y {
                return Err(InputError::OutOfBounds { x, y });
            }
        }
        Ok(())
    }

    /// Records an event in the history
    fn record_event(&mut self, event: MouseEvent) {
        self.event_history.push(event);
        if self.event_history.len() > self.history_limit {
            self.event_history.remove(0);
        }
    }

    /// Moves the mouse to the specified position using a human-like path
    ///
    /// The movement follows a Bézier curve with random control points
    /// to simulate natural human hand movement.
    ///
    /// # Arguments
    ///
    /// * `x` - Target X coordinate
    /// * `y` - Target Y coordinate
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<Point>)` containing the path taken, or an error if
    /// the coordinates are invalid.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::mouse::MouseSimulator;
    ///
    /// async fn example() {
    ///     let mut mouse = MouseSimulator::new();
    ///     let path = mouse.move_to(500.0, 300.0).await.unwrap();
    ///     println!("Moved through {} points", path.len());
    /// }
    /// ```
    pub async fn move_to(&mut self, x: f64, y: f64) -> InputResult<Vec<Point>> {
        self.validate_position(x, y)?;

        let target = Point::new(x, y);
        let distance = self.current_position.distance_to(&target);

        // Calculate number of points based on distance
        let num_points = calculate_path_points(
            distance,
            self.config.min_path_points,
            self.config.max_path_points,
        );

        // Generate human-like path
        let mut path = generate_human_path(self.current_position, target, num_points);

        // Add micro-jitter if enabled
        if self.config.add_jitter {
            add_jitter_to_path(&mut path, self.config.jitter_intensity);
        }

        // Simulate movement along the path
        for point in &path {
            // Get delay for this movement step
            let delay = self.timing.get_move_delay();
            tokio::time::sleep(delay).await;

            self.current_position = *point;

            // Record move event
            self.record_event(MouseEvent::Move {
                x: point.x,
                y: point.y,
            });
        }

        Ok(path)
    }

    /// Performs a single click at the current position
    ///
    /// # Arguments
    ///
    /// * `button` - Which mouse button to click
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::mouse::{MouseSimulator, MouseButton};
    ///
    /// async fn example() {
    ///     let mut mouse = MouseSimulator::new();
    ///     mouse.click(MouseButton::Left).await.unwrap();
    /// }
    /// ```
    pub async fn click(&mut self, button: MouseButton) -> InputResult<()> {
        // Mouse down
        self.mouse_down(button).await?;

        // Realistic delay between down and up
        let hold_delay = self.timing.get_click_delay();
        tokio::time::sleep(hold_delay).await;

        // Mouse up
        self.mouse_up(button).await?;

        // Record click event
        self.record_event(MouseEvent::Click { button });

        Ok(())
    }

    /// Performs a double click at the current position
    ///
    /// The timing between clicks is calibrated to be recognized as a
    /// double-click by the operating system.
    ///
    /// # Arguments
    ///
    /// * `button` - Which mouse button to double-click
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::mouse::{MouseSimulator, MouseButton};
    ///
    /// async fn example() {
    ///     let mut mouse = MouseSimulator::new();
    ///     mouse.double_click(MouseButton::Left).await.unwrap();
    /// }
    /// ```
    pub async fn double_click(&mut self, button: MouseButton) -> InputResult<()> {
        // First click
        self.click(button).await?;

        // Inter-click delay (50-150ms is typical for double-click recognition)
        let inter_click_delay = Duration::from_millis(rand::random::<u64>() % 100 + 50);
        tokio::time::sleep(inter_click_delay).await;

        // Second click
        self.click(button).await?;

        // Record double-click event
        self.record_event(MouseEvent::DoubleClick { button });

        Ok(())
    }

    /// Presses a mouse button down without releasing
    ///
    /// # Arguments
    ///
    /// * `button` - Which mouse button to press
    pub async fn mouse_down(&mut self, button: MouseButton) -> InputResult<()> {
        // Small delay before pressing
        let pre_delay = Duration::from_millis(rand::random::<u64>() % 20 + 5);
        tokio::time::sleep(pre_delay).await;

        self.record_event(MouseEvent::Down { button });

        // Here would be the actual platform-specific mouse down implementation
        // For now, we just simulate the timing

        Ok(())
    }

    /// Releases a mouse button
    ///
    /// # Arguments
    ///
    /// * `button` - Which mouse button to release
    pub async fn mouse_up(&mut self, button: MouseButton) -> InputResult<()> {
        self.record_event(MouseEvent::Up { button });

        // Here would be the actual platform-specific mouse up implementation

        Ok(())
    }

    /// Scrolls the mouse wheel
    ///
    /// # Arguments
    ///
    /// * `delta_x` - Horizontal scroll amount (positive = right)
    /// * `delta_y` - Vertical scroll amount (positive = down)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::mouse::MouseSimulator;
    ///
    /// async fn example() {
    ///     let mut mouse = MouseSimulator::new();
    ///     // Scroll down 3 units
    ///     mouse.scroll(0.0, 3.0).await.unwrap();
    /// }
    /// ```
    pub async fn scroll(&mut self, delta_x: f64, delta_y: f64) -> InputResult<()> {
        // Calculate number of scroll steps for smooth scrolling
        let total_delta = (delta_x.abs() + delta_y.abs()).ceil() as usize;
        let steps = total_delta.max(1);

        let step_x = delta_x / steps as f64;
        let step_y = delta_y / steps as f64;

        for _ in 0..steps {
            // Small delay between scroll steps
            let delay = Duration::from_millis(rand::random::<u64>() % 30 + 10);
            tokio::time::sleep(delay).await;

            // Record scroll event
            self.record_event(MouseEvent::Scroll {
                delta_x: step_x,
                delta_y: step_y,
            });

            // Here would be the actual scroll implementation
        }

        Ok(())
    }

    /// Drags from current position to target position
    ///
    /// This simulates a click-and-drag operation with human-like movement.
    ///
    /// # Arguments
    ///
    /// * `x` - Target X coordinate
    /// * `y` - Target Y coordinate
    /// * `button` - Which mouse button to use for dragging
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use ki_browser::input::mouse::{MouseSimulator, MouseButton};
    ///
    /// async fn example() {
    ///     let mut mouse = MouseSimulator::new();
    ///     mouse.move_to(100.0, 100.0).await.unwrap();
    ///     mouse.drag_to(500.0, 300.0, MouseButton::Left).await.unwrap();
    /// }
    /// ```
    pub async fn drag_to(&mut self, x: f64, y: f64, button: MouseButton) -> InputResult<Vec<Point>> {
        self.validate_position(x, y)?;

        // Press button down
        self.mouse_down(button).await?;

        // Small delay after pressing
        let post_press_delay = Duration::from_millis(rand::random::<u64>() % 50 + 30);
        tokio::time::sleep(post_press_delay).await;

        // Move to target (this returns the path)
        let path = self.move_to(x, y).await?;

        // Small delay before releasing
        let pre_release_delay = Duration::from_millis(rand::random::<u64>() % 50 + 30);
        tokio::time::sleep(pre_release_delay).await;

        // Release button
        self.mouse_up(button).await?;

        Ok(path)
    }

    /// Moves to position and clicks in a single operation
    ///
    /// # Arguments
    ///
    /// * `x` - Target X coordinate
    /// * `y` - Target Y coordinate
    /// * `button` - Which mouse button to click
    pub async fn click_at(&mut self, x: f64, y: f64, button: MouseButton) -> InputResult<()> {
        self.move_to(x, y).await?;

        // Small pause before clicking (natural hesitation)
        let pause = Duration::from_millis(rand::random::<u64>() % 100 + 50);
        tokio::time::sleep(pause).await;

        self.click(button).await
    }

    /// Returns a copy of the event history
    pub fn event_history(&self) -> Vec<MouseEvent> {
        self.event_history.clone()
    }

    /// Clears the event history
    pub fn clear_history(&mut self) {
        self.event_history.clear();
    }
}

/// Calculates the number of path points based on distance
fn calculate_path_points(distance: f64, min: usize, max: usize) -> usize {
    // More points for longer distances
    let points = (distance / 10.0).ceil() as usize;
    points.clamp(min, max)
}

/// Adds random micro-jitter to a path to simulate hand tremor
fn add_jitter_to_path(path: &mut [Point], intensity: f64) {
    for point in path.iter_mut() {
        let jitter_x = (rand::random::<f64>() - 0.5) * intensity * 2.0;
        let jitter_y = (rand::random::<f64>() - 0.5) * intensity * 2.0;
        point.x += jitter_x;
        point.y += jitter_y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_button_code() {
        assert_eq!(MouseButton::Left.button_code(), 0);
        assert_eq!(MouseButton::Middle.button_code(), 1);
        assert_eq!(MouseButton::Right.button_code(), 2);
    }

    #[test]
    fn test_mouse_button_display() {
        assert_eq!(MouseButton::Left.to_string(), "left");
        assert_eq!(MouseButton::Right.to_string(), "right");
        assert_eq!(MouseButton::Middle.to_string(), "middle");
    }

    #[test]
    fn test_calculate_path_points() {
        assert_eq!(calculate_path_points(50.0, 10, 100), 10);
        assert_eq!(calculate_path_points(500.0, 10, 100), 50);
        assert_eq!(calculate_path_points(2000.0, 10, 100), 100);
    }

    #[test]
    fn test_mouse_simulator_position() {
        let mut mouse = MouseSimulator::new();
        assert_eq!(mouse.position(), Point::new(0.0, 0.0));

        mouse.set_position(100.0, 200.0);
        assert_eq!(mouse.position(), Point::new(100.0, 200.0));
    }

    #[test]
    fn test_validate_position() {
        let mut mouse = MouseSimulator::new();
        mouse.config.screen_bounds = Some((1920.0, 1080.0));

        assert!(mouse.validate_position(500.0, 500.0).is_ok());
        assert!(mouse.validate_position(-1.0, 500.0).is_err());
        assert!(mouse.validate_position(2000.0, 500.0).is_err());
    }

    #[test]
    fn test_jitter() {
        let mut path = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(20.0, 20.0),
        ];
        let original = path.clone();

        add_jitter_to_path(&mut path, 1.0);

        // At least one point should have changed
        let changed = path
            .iter()
            .zip(original.iter())
            .any(|(a, b)| (a.x - b.x).abs() > 0.001 || (a.y - b.y).abs() > 0.001);
        assert!(changed);
    }
}
