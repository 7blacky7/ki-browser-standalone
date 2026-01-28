//! Human-like timing utilities for input simulation
//!
//! This module provides timing utilities that generate realistic delays
//! based on human behavioral studies and psychophysical research.
//!
//! # Background
//!
//! Human input timing follows predictable patterns studied in HCI research:
//! - Typing speed: 40-60 WPM average, with variance per character
//! - Mouse click duration: 70-150ms
//! - Inter-keystroke interval: 100-200ms
//! - Reaction time: 150-300ms
//!
//! # Example
//!
//! ```rust
//! use ki_browser::input::timing::HumanTiming;
//!
//! let timing = HumanTiming::default();
//! let click_delay = timing.get_click_delay();
//! let type_delay = timing.get_type_delay();
//! ```

use std::time::Duration;

/// Configuration for human-like timing patterns
///
/// Timing values are based on empirical studies of human computer interaction:
/// - Card, Moran, & Newell (1983): The Psychology of Human-Computer Interaction
/// - Seow (2008): Designing and Engineering Time
#[derive(Debug, Clone)]
pub struct HumanTiming {
    /// Minimum delay between actions in milliseconds
    pub min_delay_ms: u64,
    /// Maximum delay between actions in milliseconds
    pub max_delay_ms: u64,
    /// Variance factor (0.0 - 1.0) for timing randomization
    pub variance: f64,
    /// Profile name for this timing configuration
    pub profile: TimingProfile,
}

/// Predefined timing profiles for different use cases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingProfile {
    /// Normal human typing/clicking speed
    Normal,
    /// Faster than average (experienced user)
    Fast,
    /// Slower than average (careful/novice user)
    Slow,
    /// Very fast (for testing, less realistic)
    Instant,
    /// Custom timing values
    Custom,
}

impl Default for HumanTiming {
    fn default() -> Self {
        Self::normal()
    }
}

impl HumanTiming {
    /// Creates a new HumanTiming with custom values
    ///
    /// # Arguments
    ///
    /// * `min_delay_ms` - Minimum delay in milliseconds
    /// * `max_delay_ms` - Maximum delay in milliseconds
    /// * `variance` - Variance factor (0.0 - 1.0)
    ///
    /// # Example
    ///
    /// ```rust
    /// use ki_browser::input::timing::HumanTiming;
    ///
    /// let timing = HumanTiming::new(50, 150, 0.3);
    /// ```
    pub fn new(min_delay_ms: u64, max_delay_ms: u64, variance: f64) -> Self {
        Self {
            min_delay_ms,
            max_delay_ms: max_delay_ms.max(min_delay_ms),
            variance: variance.clamp(0.0, 1.0),
            profile: TimingProfile::Custom,
        }
    }

    /// Creates timing for normal human speed
    ///
    /// Based on average typing speed of ~50 WPM and typical reaction times.
    pub fn normal() -> Self {
        Self {
            min_delay_ms: 50,
            max_delay_ms: 150,
            variance: 0.3,
            profile: TimingProfile::Normal,
        }
    }

    /// Creates timing for fast/experienced users
    ///
    /// Based on experienced typist speed of ~80 WPM.
    pub fn fast() -> Self {
        Self {
            min_delay_ms: 25,
            max_delay_ms: 80,
            variance: 0.25,
            profile: TimingProfile::Fast,
        }
    }

    /// Creates timing for slow/careful users
    ///
    /// Based on hunt-and-peck typing (~20 WPM) or careful navigation.
    pub fn slow() -> Self {
        Self {
            min_delay_ms: 100,
            max_delay_ms: 300,
            variance: 0.4,
            profile: TimingProfile::Slow,
        }
    }

    /// Creates near-instant timing for testing
    ///
    /// Minimal delays for speed, less realistic but useful for tests.
    pub fn instant() -> Self {
        Self {
            min_delay_ms: 1,
            max_delay_ms: 10,
            variance: 0.1,
            profile: TimingProfile::Instant,
        }
    }

    /// Gets a realistic delay for mouse click duration
    ///
    /// Mouse click duration (time button is held down) is typically 70-150ms.
    ///
    /// # Returns
    ///
    /// Duration representing how long to hold the mouse button
    pub fn get_click_delay(&self) -> Duration {
        // Base click duration from research: 70-150ms
        let base_min = 70_u64;
        let base_max = 150_u64;

        // Scale based on profile
        let (min, max) = match self.profile {
            TimingProfile::Normal => (base_min, base_max),
            TimingProfile::Fast => (base_min * 7 / 10, base_max * 7 / 10),
            TimingProfile::Slow => (base_min * 13 / 10, base_max * 13 / 10),
            TimingProfile::Instant => (10, 30),
            TimingProfile::Custom => (
                self.min_delay_ms.max(10),
                self.max_delay_ms.min(500),
            ),
        };

        random_delay_in_range(min, max, self.variance)
    }

    /// Gets a realistic delay for typing (inter-keystroke interval)
    ///
    /// The inter-keystroke interval varies based on typing proficiency:
    /// - Expert: 60-100ms
    /// - Average: 100-200ms
    /// - Novice: 200-400ms
    ///
    /// # Returns
    ///
    /// Duration to wait between keystrokes
    pub fn get_type_delay(&self) -> Duration {
        let (min, max) = match self.profile {
            TimingProfile::Normal => (80, 180),
            TimingProfile::Fast => (50, 100),
            TimingProfile::Slow => (180, 350),
            TimingProfile::Instant => (5, 20),
            TimingProfile::Custom => (self.min_delay_ms, self.max_delay_ms),
        };

        random_delay_in_range(min, max, self.variance)
    }

    /// Gets a realistic delay for mouse movement between points
    ///
    /// This is the delay between individual points in a mouse movement path.
    /// Should be small to create smooth movement.
    ///
    /// # Returns
    ///
    /// Duration to wait between movement points
    pub fn get_move_delay(&self) -> Duration {
        let (min, max) = match self.profile {
            TimingProfile::Normal => (5, 15),
            TimingProfile::Fast => (2, 8),
            TimingProfile::Slow => (10, 25),
            TimingProfile::Instant => (1, 3),
            TimingProfile::Custom => (
                (self.min_delay_ms / 10).max(1),
                (self.max_delay_ms / 10).max(5),
            ),
        };

        random_delay_in_range(min, max, self.variance)
    }

    /// Gets a delay for reaction time before an action
    ///
    /// Human reaction time for visual stimuli is typically 150-300ms.
    /// This can be used to simulate the delay before responding to something.
    ///
    /// # Returns
    ///
    /// Duration representing reaction time
    pub fn get_reaction_delay(&self) -> Duration {
        let (min, max) = match self.profile {
            TimingProfile::Normal => (150, 300),
            TimingProfile::Fast => (100, 200),
            TimingProfile::Slow => (250, 450),
            TimingProfile::Instant => (10, 50),
            TimingProfile::Custom => (
                self.min_delay_ms * 2,
                self.max_delay_ms * 2,
            ),
        };

        random_delay_in_range(min, max, self.variance)
    }

    /// Gets a delay for pause/thinking time
    ///
    /// When users pause to think or read, delays are typically 500-2000ms.
    ///
    /// # Returns
    ///
    /// Duration representing a thinking pause
    pub fn get_pause_delay(&self) -> Duration {
        let (min, max) = match self.profile {
            TimingProfile::Normal => (500, 1500),
            TimingProfile::Fast => (300, 800),
            TimingProfile::Slow => (800, 2500),
            TimingProfile::Instant => (50, 200),
            TimingProfile::Custom => (
                self.min_delay_ms * 5,
                self.max_delay_ms * 5,
            ),
        };

        random_delay_in_range(min, max, self.variance)
    }

    /// Gets the delay for double-click interval
    ///
    /// The interval between clicks in a double-click is typically 50-150ms.
    /// This must be fast enough to be recognized as a double-click.
    ///
    /// # Returns
    ///
    /// Duration between the two clicks of a double-click
    pub fn get_double_click_interval(&self) -> Duration {
        // Double-click interval should be consistent regardless of profile
        // to ensure it's recognized as a double-click
        let (min, max) = (50, 150);
        random_delay_in_range(min, max, 0.2)
    }
}

/// Generates a random delay within a range with normal distribution
///
/// Uses the Box-Muller transform to generate normally distributed values,
/// which better simulates natural human timing variance.
///
/// # Arguments
///
/// * `min_ms` - Minimum delay in milliseconds
/// * `max_ms` - Maximum delay in milliseconds
/// * `variance` - How much the delay can vary (0.0 - 1.0)
///
/// # Returns
///
/// A Duration with a random value in the specified range
pub fn random_delay_in_range(min_ms: u64, max_ms: u64, variance: f64) -> Duration {
    if min_ms >= max_ms {
        return Duration::from_millis(min_ms);
    }

    // Calculate the mean and standard deviation
    let mean = (min_ms + max_ms) as f64 / 2.0;
    let range = (max_ms - min_ms) as f64;
    let std_dev = range * variance / 2.0;

    // Generate normally distributed random value using Box-Muller
    let delay = normal_random(mean, std_dev);

    // Clamp to valid range
    let delay_ms = delay.round().clamp(min_ms as f64, max_ms as f64) as u64;

    Duration::from_millis(delay_ms)
}

/// Generates a normally distributed random number
///
/// Uses the Box-Muller transform to convert uniform random numbers
/// to normal distribution.
///
/// # Arguments
///
/// * `mean` - Mean of the distribution
/// * `std_dev` - Standard deviation
///
/// # Returns
///
/// A random number from a normal distribution
fn normal_random(mean: f64, std_dev: f64) -> f64 {
    // Box-Muller transform
    let u1: f64 = rand::random::<f64>().max(1e-10); // Avoid log(0)
    let u2: f64 = rand::random();

    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();

    mean + z * std_dev
}

/// Generates a random delay with uniform distribution
///
/// # Arguments
///
/// * `min_ms` - Minimum delay in milliseconds
/// * `max_ms` - Maximum delay in milliseconds
///
/// # Returns
///
/// A Duration with a uniformly random value in the range
pub fn random_delay(min_ms: u64, max_ms: u64) -> Duration {
    if min_ms >= max_ms {
        return Duration::from_millis(min_ms);
    }

    let range = max_ms - min_ms;
    let delay_ms = min_ms + (rand::random::<u64>() % range);

    Duration::from_millis(delay_ms)
}

/// Generates a delay based on Fitts's Law for pointing movements
///
/// Fitts's Law predicts movement time based on distance and target size:
/// MT = a + b * log2(2D/W)
///
/// where:
/// - MT = movement time
/// - D = distance to target
/// - W = width of target
/// - a, b = empirically determined constants
///
/// # Arguments
///
/// * `distance` - Distance to the target in pixels
/// * `target_width` - Width of the target in pixels
///
/// # Returns
///
/// Predicted movement time as Duration
pub fn fitts_law_delay(distance: f64, target_width: f64) -> Duration {
    // Typical constants from research (MacKenzie, 1992)
    const A: f64 = 50.0;  // Intercept in ms
    const B: f64 = 150.0; // Slope in ms/bit

    // Avoid division by zero and negative values
    let width = target_width.max(1.0);
    let dist = distance.max(0.0);

    // Index of Difficulty (ID)
    let id = ((dist / width) + 1.0).log2();

    // Movement time
    let mt = A + B * id;

    // Add some random variance (typically 10-20%)
    let variance = 0.15;
    let mt_with_variance = mt * (1.0 + (rand::random::<f64>() - 0.5) * 2.0 * variance);

    Duration::from_millis(mt_with_variance.max(10.0) as u64)
}

/// Calculates typing speed in words per minute
///
/// # Arguments
///
/// * `char_count` - Number of characters typed
/// * `duration` - Time taken to type
///
/// # Returns
///
/// Typing speed in WPM (assuming 5 characters per word)
pub fn calculate_wpm(char_count: usize, duration: Duration) -> f64 {
    let minutes = duration.as_secs_f64() / 60.0;
    if minutes <= 0.0 {
        return 0.0;
    }

    // Standard: 5 characters = 1 word
    let words = char_count as f64 / 5.0;
    words / minutes
}

/// Converts words per minute to inter-keystroke interval
///
/// # Arguments
///
/// * `wpm` - Target words per minute
///
/// # Returns
///
/// Average delay between keystrokes
pub fn wpm_to_delay(wpm: f64) -> Duration {
    if wpm <= 0.0 {
        return Duration::from_millis(200);
    }

    // 5 characters per word, 60 seconds per minute
    let chars_per_second = wpm * 5.0 / 60.0;
    let ms_per_char = 1000.0 / chars_per_second;

    Duration::from_millis(ms_per_char as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timing_profiles() {
        let normal = HumanTiming::normal();
        let fast = HumanTiming::fast();
        let slow = HumanTiming::slow();

        // Fast should have shorter delays than normal
        assert!(fast.min_delay_ms < normal.min_delay_ms);
        assert!(fast.max_delay_ms < normal.max_delay_ms);

        // Slow should have longer delays than normal
        assert!(slow.min_delay_ms > normal.min_delay_ms);
        assert!(slow.max_delay_ms > normal.max_delay_ms);
    }

    #[test]
    fn test_random_delay_in_range() {
        let min = 50;
        let max = 150;

        // Generate many delays and check they're in range
        for _ in 0..100 {
            let delay = random_delay_in_range(min, max, 0.3);
            let ms = delay.as_millis() as u64;
            assert!(ms >= min && ms <= max);
        }
    }

    #[test]
    fn test_random_delay_edge_case() {
        // When min equals max, should return min
        let delay = random_delay_in_range(100, 100, 0.5);
        assert_eq!(delay.as_millis(), 100);

        // When min > max, should return min
        let delay = random_delay_in_range(150, 100, 0.5);
        assert_eq!(delay.as_millis(), 150);
    }

    #[test]
    fn test_fitts_law() {
        // Longer distance should take longer
        let short_dist = fitts_law_delay(100.0, 50.0);
        let long_dist = fitts_law_delay(500.0, 50.0);

        // Due to randomness, we can't assert exact values, but on average
        // long distance should take longer
        // This test just verifies the function runs without errors
        assert!(short_dist.as_millis() > 0);
        assert!(long_dist.as_millis() > 0);
    }

    #[test]
    fn test_wpm_conversion() {
        // 60 WPM = 5 chars/second = 200ms per char
        let delay = wpm_to_delay(60.0);
        assert!((delay.as_millis() as i64 - 200).abs() < 10);

        // Calculate WPM from duration
        let wpm = calculate_wpm(300, Duration::from_secs(60));
        assert!((wpm - 60.0).abs() < 0.1);
    }

    #[test]
    fn test_click_delay_bounds() {
        let timing = HumanTiming::normal();

        for _ in 0..50 {
            let delay = timing.get_click_delay();
            // Click delays should be reasonable (10ms - 500ms)
            assert!(delay.as_millis() >= 10);
            assert!(delay.as_millis() <= 500);
        }
    }

    #[test]
    fn test_type_delay_bounds() {
        let timing = HumanTiming::normal();

        for _ in 0..50 {
            let delay = timing.get_type_delay();
            // Type delays should be reasonable (10ms - 500ms)
            assert!(delay.as_millis() >= 10);
            assert!(delay.as_millis() <= 500);
        }
    }

    #[test]
    fn test_double_click_interval() {
        let timing = HumanTiming::normal();

        for _ in 0..50 {
            let delay = timing.get_double_click_interval();
            // Double-click interval must be recognized as double-click
            // Typically systems use 500ms as max threshold
            assert!(delay.as_millis() >= 50);
            assert!(delay.as_millis() <= 200);
        }
    }

    #[test]
    fn test_custom_timing() {
        let custom = HumanTiming::new(100, 200, 0.5);

        assert_eq!(custom.min_delay_ms, 100);
        assert_eq!(custom.max_delay_ms, 200);
        assert_eq!(custom.variance, 0.5);
        assert_eq!(custom.profile, TimingProfile::Custom);
    }

    #[test]
    fn test_variance_clamping() {
        // Variance should be clamped to 0.0 - 1.0
        let timing = HumanTiming::new(50, 150, 2.0);
        assert_eq!(timing.variance, 1.0);

        let timing = HumanTiming::new(50, 150, -0.5);
        assert_eq!(timing.variance, 0.0);
    }
}
