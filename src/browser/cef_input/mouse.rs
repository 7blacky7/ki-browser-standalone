//! CEF mouse input simulation with human-like Bezier curve movement.
//!
//! This module provides the `CefInputHandler` struct and all mouse-related
//! input methods: move, click, double-click, drag, scroll, mouse-down and
//! mouse-up. Movement paths are generated as Bezier curves with optional
//! micro-jitter to simulate realistic hand tremor.
//!
//! The `CefEventSender` trait decouples the handler from the concrete CEF
//! browser instance, enabling testing via mock implementations.

use std::collections::HashSet;
use std::time::Duration;

use crate::input::bezier::{generate_human_path, Point};
use crate::input::timing::HumanTiming;
use crate::input::{InputError, InputResult, Modifier};

use super::events::{CefKeyEvent, CefMouseButton, CefMouseEvent, EVENTFLAG_NONE};
use super::keyboard::modifier_to_event_flag;

// ============================================================================
// Event Sender Trait
// ============================================================================

/// Callback trait for delivering CEF input events to a browser instance.
///
/// Implement this trait to connect `CefInputHandler` to a live CEF browser.
/// All methods are synchronous; async delays are managed by the handler.
pub trait CefEventSender: Send + Sync {
    /// Delivers a mouse move event to the CEF browser view.
    fn send_mouse_move_event(&self, event: &CefMouseEvent, mouse_leave: bool);

    /// Delivers a mouse button down or up event to the CEF browser view.
    fn send_mouse_click_event(
        &self,
        event: &CefMouseEvent,
        button: CefMouseButton,
        mouse_up: bool,
        click_count: i32,
    );

    /// Delivers a scroll wheel event with pixel delta to the CEF browser view.
    fn send_mouse_wheel_event(&self, event: &CefMouseEvent, delta_x: i32, delta_y: i32);

    /// Delivers a keyboard event to the CEF browser view.
    fn send_key_event(&self, event: &CefKeyEvent);
}

// ============================================================================
// Input Configuration
// ============================================================================

/// Configuration controlling the behaviour of CEF input simulation.
///
/// Controls path point density for mouse movement, jitter intensity for
/// hand-tremor simulation, and optional view bounds for coordinate validation.
#[derive(Debug, Clone)]
pub struct CefInputConfig {
    /// Minimum number of intermediate points in a mouse movement path.
    pub min_path_points: usize,
    /// Maximum number of intermediate points in a mouse movement path.
    pub max_path_points: usize,
    /// Whether to add random micro-movements (jitter) along mouse paths.
    pub add_jitter: bool,
    /// Jitter intensity in pixels (0.0 = none, 1.0 = up to ±1px per point).
    pub jitter_intensity: f64,
    /// Optional view bounds (width, height) for coordinate range validation.
    pub view_bounds: Option<(i32, i32)>,
}

impl Default for CefInputConfig {
    fn default() -> Self {
        Self {
            min_path_points: 20,
            max_path_points: 50,
            add_jitter: true,
            jitter_intensity: 0.3,
            view_bounds: None,
        }
    }
}

// ============================================================================
// CEF Input Handler
// ============================================================================

/// Simulates native mouse and keyboard input for CEF offscreen rendering.
///
/// Wraps a `CefEventSender` and tracks current mouse position, pressed buttons,
/// and active modifier keys. All movement uses Bezier curve interpolation with
/// optional jitter for anti-detection human-like behaviour.
pub struct CefInputHandler<S: CefEventSender> {
    /// Underlying CEF event delivery channel.
    pub(super) sender: S,
    /// Current mouse cursor position in view coordinates.
    current_position: Point,
    /// Active input simulation configuration.
    pub(super) config: CefInputConfig,
    /// Human timing profile controlling inter-event delays.
    pub(super) timing: HumanTiming,
    /// Set of mouse buttons currently held down.
    pub(super) pressed_buttons: HashSet<CefMouseButton>,
    /// Set of modifier keys currently held down.
    pub(super) active_modifiers: HashSet<Modifier>,
}

impl<S: CefEventSender> CefInputHandler<S> {
    /// Creates a new `CefInputHandler` with default configuration.
    ///
    /// # Arguments
    ///
    /// * `sender` - Event delivery channel to the CEF browser instance.
    /// * `timing` - Human timing profile for inter-event delays.
    pub fn new(sender: S, timing: HumanTiming) -> Self {
        Self {
            sender,
            current_position: Point::new(0.0, 0.0),
            config: CefInputConfig::default(),
            timing,
            pressed_buttons: HashSet::new(),
            active_modifiers: HashSet::new(),
        }
    }

    /// Creates a new handler with a custom `CefInputConfig`.
    pub fn with_config(sender: S, timing: HumanTiming, config: CefInputConfig) -> Self {
        Self {
            sender,
            current_position: Point::new(0.0, 0.0),
            config,
            timing,
            pressed_buttons: HashSet::new(),
            active_modifiers: HashSet::new(),
        }
    }

    /// Returns the current mouse cursor position.
    pub fn position(&self) -> Point {
        self.current_position
    }

    /// Teleports the tracked mouse position without sending events.
    pub fn set_position(&mut self, x: f64, y: f64) {
        self.current_position = Point::new(x, y);
    }

    /// Returns the currently active modifier keys as a `Vec`.
    pub fn active_modifiers(&self) -> Vec<Modifier> {
        self.active_modifiers.iter().copied().collect()
    }

    /// Builds the combined EVENTFLAG bitmask for all currently active
    /// modifiers and pressed mouse buttons.
    pub(super) fn current_modifier_flags(&self) -> u32 {
        let mut flags = EVENTFLAG_NONE;
        for modifier in &self.active_modifiers {
            flags |= modifier_to_event_flag(modifier);
        }
        for button in &self.pressed_buttons {
            flags |= button.to_event_flags();
        }
        flags
    }

    /// Validates that `(x, y)` is within the configured view bounds.
    ///
    /// Returns `InputError::OutOfBounds` if coordinates are negative or
    /// exceed the configured `view_bounds`.
    pub(super) fn validate_position(&self, x: f64, y: f64) -> InputResult<()> {
        if x < 0.0 || y < 0.0 {
            return Err(InputError::OutOfBounds { x, y });
        }
        if let Some((max_x, max_y)) = self.config.view_bounds {
            if x > max_x as f64 || y > max_y as f64 {
                return Err(InputError::OutOfBounds { x, y });
            }
        }
        Ok(())
    }

    /// Creates a `CefMouseEvent` at the specified integer coordinates with
    /// the current combined modifier and button flags.
    pub(super) fn create_mouse_event(&self, x: i32, y: i32) -> CefMouseEvent {
        CefMouseEvent::with_modifiers(x, y, self.current_modifier_flags())
    }

    // ========================================================================
    // Mouse Input Methods
    // ========================================================================

    /// Moves the mouse from the current position to `(x, y)` along a
    /// human-like Bezier curve path with optional micro-jitter.
    ///
    /// # Returns
    ///
    /// The sequence of intermediate `Point` values that were visited.
    ///
    /// # Errors
    ///
    /// Returns `InputError::OutOfBounds` if the target is outside view bounds.
    pub async fn send_mouse_move(&mut self, x: f64, y: f64) -> InputResult<Vec<Point>> {
        self.validate_position(x, y)?;

        let target = Point::new(x, y);
        let distance = self.current_position.distance_to(&target);

        let num_points = calculate_path_points(
            distance,
            self.config.min_path_points,
            self.config.max_path_points,
        );

        let mut path = generate_human_path(self.current_position, target, num_points);

        if self.config.add_jitter {
            add_jitter_to_path(&mut path, self.config.jitter_intensity);
        }

        for point in &path {
            let delay = self.timing.get_move_delay();
            tokio::time::sleep(delay).await;

            self.current_position = *point;
            let event = self.create_mouse_event(point.x.round() as i32, point.y.round() as i32);
            self.sender.send_mouse_move_event(&event, false);
        }

        Ok(path)
    }

    /// Performs a complete click at `(x, y)`: move, press, hold, release.
    ///
    /// Combines `send_mouse_move` + `send_mouse_down` + `send_mouse_up` with
    /// human-like hesitation and hold delays between steps.
    ///
    /// # Errors
    ///
    /// Returns `InputError::OutOfBounds` if the position is outside view bounds.
    pub async fn send_mouse_click(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        self.send_mouse_move(x, y).await?;

        // Natural hesitation before pressing
        let pause = Duration::from_millis(rand::random::<u64>() % 50 + 20);
        tokio::time::sleep(pause).await;

        self.send_mouse_down(x, y, button).await?;

        let hold_delay = self.timing.get_click_delay();
        tokio::time::sleep(hold_delay).await;

        self.send_mouse_up(x, y, button).await?;

        Ok(())
    }

    /// Sends a mouse button down event at `(x, y)` with a brief pre-press delay.
    ///
    /// Registers the button in the internal pressed-buttons set so subsequent
    /// events carry the correct modifier flags.
    ///
    /// # Errors
    ///
    /// Returns `InputError::OutOfBounds` if the position is outside view bounds.
    pub async fn send_mouse_down(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        self.validate_position(x, y)?;

        let delay = Duration::from_millis(rand::random::<u64>() % 10 + 2);
        tokio::time::sleep(delay).await;

        self.pressed_buttons.insert(button);

        let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
        self.sender.send_mouse_click_event(&event, button, false, 1);

        Ok(())
    }

    /// Sends a mouse button up event at `(x, y)`.
    ///
    /// Removes the button from the internal pressed-buttons set.
    ///
    /// # Errors
    ///
    /// Returns `InputError::OutOfBounds` if the position is outside view bounds.
    pub async fn send_mouse_up(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        self.validate_position(x, y)?;

        self.pressed_buttons.remove(&button);

        let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
        self.sender.send_mouse_click_event(&event, button, true, 1);

        Ok(())
    }

    /// Sends a smooth scroll wheel event at `(x, y)` split into multiple steps.
    ///
    /// `delta_x` and `delta_y` are in scroll units (1 unit ≈ 40 pixels).
    /// Positive `delta_y` scrolls down, positive `delta_x` scrolls right.
    ///
    /// # Errors
    ///
    /// Returns `InputError::OutOfBounds` if the position is outside view bounds.
    pub async fn send_scroll(
        &mut self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
    ) -> InputResult<()> {
        self.validate_position(x, y)?;

        let scroll_multiplier = 40.0;
        let total_dx = (delta_x * scroll_multiplier).round() as i32;
        let total_dy = (delta_y * scroll_multiplier).round() as i32;

        let steps = ((delta_x.abs() + delta_y.abs()).ceil() as usize).max(1);
        let step_dx = total_dx / steps as i32;
        let step_dy = total_dy / steps as i32;

        for i in 0..steps {
            let delay = Duration::from_millis(rand::random::<u64>() % 30 + 10);
            tokio::time::sleep(delay).await;

            let dx = if i == steps - 1 {
                total_dx - step_dx * (steps as i32 - 1)
            } else {
                step_dx
            };
            let dy = if i == steps - 1 {
                total_dy - step_dy * (steps as i32 - 1)
            } else {
                step_dy
            };

            let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
            self.sender.send_mouse_wheel_event(&event, dx, dy);
        }

        Ok(())
    }

    /// Performs a double-click at `(x, y)` with correct click-count signalling.
    ///
    /// Executes a normal click first, waits the double-click interval, then
    /// sends explicit down/up events with `click_count = 2`.
    ///
    /// # Errors
    ///
    /// Propagates any errors from the underlying click and move operations.
    pub async fn send_double_click(
        &mut self,
        x: f64,
        y: f64,
        button: CefMouseButton,
    ) -> InputResult<()> {
        self.send_mouse_click(x, y, button).await?;

        let delay = self.timing.get_double_click_interval();
        tokio::time::sleep(delay).await;

        let event = self.create_mouse_event(x.round() as i32, y.round() as i32);
        self.sender.send_mouse_click_event(&event, button, false, 2);

        let hold = self.timing.get_click_delay();
        tokio::time::sleep(hold).await;

        self.sender.send_mouse_click_event(&event, button, true, 2);

        Ok(())
    }

    /// Performs a drag from the current position to `(target_x, target_y)`.
    ///
    /// Presses `button` at the start, moves along a Bezier path, then releases.
    ///
    /// # Returns
    ///
    /// The intermediate path points traversed during the drag.
    ///
    /// # Errors
    ///
    /// Propagates position validation or event delivery errors.
    pub async fn send_drag(
        &mut self,
        target_x: f64,
        target_y: f64,
        button: CefMouseButton,
    ) -> InputResult<Vec<Point>> {
        let start = self.current_position;

        self.send_mouse_down(start.x, start.y, button).await?;

        let delay = Duration::from_millis(rand::random::<u64>() % 50 + 30);
        tokio::time::sleep(delay).await;

        let path = self.send_mouse_move(target_x, target_y).await?;

        let delay = Duration::from_millis(rand::random::<u64>() % 50 + 30);
        tokio::time::sleep(delay).await;

        self.send_mouse_up(target_x, target_y, button).await?;

        Ok(path)
    }
}

// ============================================================================
// Private Path Helpers
// ============================================================================

/// Calculates the number of intermediate path points proportional to distance.
///
/// Clamps the result between `min` and `max` to ensure smooth but bounded paths.
fn calculate_path_points(distance: f64, min: usize, max: usize) -> usize {
    let points = (distance / 10.0).ceil() as usize;
    points.clamp(min, max)
}

/// Adds random micro-jitter to intermediate path points to simulate hand tremor.
///
/// Skips the first and last points so exact start and end positions are preserved.
fn add_jitter_to_path(path: &mut [Point], intensity: f64) {
    let len = path.len();
    if len <= 2 {
        return;
    }
    for point in path[1..len - 1].iter_mut() {
        let jitter_x = (rand::random::<f64>() - 0.5) * intensity * 2.0;
        let jitter_y = (rand::random::<f64>() - 0.5) * intensity * 2.0;
        point.x += jitter_x;
        point.y += jitter_y;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::timing::HumanTiming;

    // Minimal mock sender for mouse tests
    struct MockSender {
        moves: std::sync::Mutex<Vec<CefMouseEvent>>,
        clicks: std::sync::Mutex<Vec<(CefMouseEvent, CefMouseButton, bool, i32)>>,
        wheels: std::sync::Mutex<Vec<(CefMouseEvent, i32, i32)>>,
        keys: std::sync::Mutex<Vec<CefKeyEvent>>,
    }

    impl MockSender {
        fn new() -> Self {
            Self {
                moves: std::sync::Mutex::new(Vec::new()),
                clicks: std::sync::Mutex::new(Vec::new()),
                wheels: std::sync::Mutex::new(Vec::new()),
                keys: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl CefEventSender for MockSender {
        fn send_mouse_move_event(&self, event: &CefMouseEvent, _mouse_leave: bool) {
            self.moves.lock().unwrap().push(*event);
        }
        fn send_mouse_click_event(
            &self,
            event: &CefMouseEvent,
            button: CefMouseButton,
            mouse_up: bool,
            click_count: i32,
        ) {
            self.clicks.lock().unwrap().push((*event, button, mouse_up, click_count));
        }
        fn send_mouse_wheel_event(&self, event: &CefMouseEvent, dx: i32, dy: i32) {
            self.wheels.lock().unwrap().push((*event, dx, dy));
        }
        fn send_key_event(&self, event: &CefKeyEvent) {
            self.keys.lock().unwrap().push(event.clone());
        }
    }

    #[test]
    fn test_calculate_path_points_clamping() {
        assert_eq!(calculate_path_points(50.0, 10, 100), 10);
        assert_eq!(calculate_path_points(500.0, 10, 100), 50);
        assert_eq!(calculate_path_points(2000.0, 10, 100), 100);
    }

    #[test]
    fn test_position_validation_out_of_bounds() {
        let handler = CefInputHandler::new(MockSender::new(), HumanTiming::instant());
        let mut h = handler;
        h.config.view_bounds = Some((800, 600));

        assert!(h.validate_position(400.0, 300.0).is_ok());
        assert!(h.validate_position(-10.0, 100.0).is_err());
        assert!(h.validate_position(1000.0, 100.0).is_err());
    }

    #[tokio::test]
    async fn test_send_mouse_move_records_events() {
        let mut handler = CefInputHandler::new(MockSender::new(), HumanTiming::instant());

        handler.send_mouse_move(100.0, 100.0).await.unwrap();

        let moves = handler.sender.moves.lock().unwrap();
        assert!(!moves.is_empty());
        let last = moves.last().unwrap();
        assert_eq!(last.x, 100);
        assert_eq!(last.y, 100);
    }

    #[tokio::test]
    async fn test_send_mouse_click_generates_down_and_up() {
        let mut handler = CefInputHandler::new(MockSender::new(), HumanTiming::instant());

        handler.send_mouse_click(200.0, 150.0, CefMouseButton::Left).await.unwrap();

        let clicks = handler.sender.clicks.lock().unwrap();
        // At minimum one down (mouse_up=false) and one up (mouse_up=true)
        assert!(clicks.len() >= 2);
        let has_down = clicks.iter().any(|(_, _, up, _)| !up);
        let has_up = clicks.iter().any(|(_, _, up, _)| *up);
        assert!(has_down && has_up);
    }
}
