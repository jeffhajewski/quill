//! Time abstraction with clock drift simulation.
//!
//! This module provides time functions that can be configured to simulate
//! clock drift for testing distributed systems scenarios.
//!
//! # Example
//!
//! ```
//! use quill_playground::time::{TimeController, now, sleep};
//! use std::time::Duration;
//!
//! // Create a time controller with 1 second ahead drift
//! let controller = TimeController::ahead(Duration::from_secs(1));
//!
//! // Get current time (will be 1 second in the future)
//! let time = controller.now();
//!
//! // Sleep with drift adjustment
//! // If drift_rate is 1.1 (10% faster), a 1s sleep will actually be ~0.9s
//! // controller.sleep(Duration::from_secs(1)).await;
//! ```

use quill_core::playground::{ClockDirection, ClockDriftConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use tokio::time::Sleep;

/// Global time controller for the process.
static GLOBAL_CONTROLLER: std::sync::OnceLock<TimeController> = std::sync::OnceLock::new();

/// Controller for simulated time.
///
/// The time controller can adjust the current time by an offset and
/// optionally simulate clock drift (time running faster or slower).
#[derive(Clone)]
pub struct TimeController {
    inner: Arc<TimeControllerInner>,
}

struct TimeControllerInner {
    /// Whether time manipulation is enabled
    enabled: AtomicBool,
    /// Current drift configuration
    config: RwLock<Option<ClockDriftConfig>>,
    /// When the controller was created (for drift calculation)
    created_at: Instant,
}

impl Default for TimeController {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeController {
    /// Create a new time controller with no drift.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TimeControllerInner {
                enabled: AtomicBool::new(false),
                config: RwLock::new(None),
                created_at: Instant::now(),
            }),
        }
    }

    /// Create a time controller with the clock running ahead.
    pub fn ahead(offset: Duration) -> Self {
        let controller = Self::new();
        controller.set_config(ClockDriftConfig::ahead(offset));
        controller
    }

    /// Create a time controller with the clock running behind.
    pub fn behind(offset: Duration) -> Self {
        let controller = Self::new();
        controller.set_config(ClockDriftConfig::behind(offset));
        controller
    }

    /// Set the drift configuration.
    pub fn set_config(&self, config: ClockDriftConfig) {
        if let Ok(mut cfg) = self.inner.config.write() {
            *cfg = Some(config);
            self.inner.enabled.store(true, Ordering::Release);
        }
    }

    /// Clear the drift configuration.
    pub fn clear_config(&self) {
        if let Ok(mut cfg) = self.inner.config.write() {
            *cfg = None;
            self.inner.enabled.store(false, Ordering::Release);
        }
    }

    /// Check if time manipulation is enabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Acquire)
    }

    /// Get the current time, adjusted for drift.
    pub fn now(&self) -> SystemTime {
        let real_now = SystemTime::now();

        if !self.is_enabled() {
            return real_now;
        }

        let Some(config) = self.inner.config.read().ok().and_then(|c| c.clone()) else {
            return real_now;
        };

        // Calculate total offset including drift
        let elapsed = self.inner.created_at.elapsed();
        let drift_adjustment = if config.drift_rate != 1.0 {
            let drift_factor = config.drift_rate - 1.0;
            Duration::from_secs_f64(elapsed.as_secs_f64() * drift_factor)
        } else {
            Duration::ZERO
        };

        // Apply direction and offset
        match config.direction {
            ClockDirection::Ahead => real_now + config.offset + drift_adjustment,
            ClockDirection::Behind => {
                let total_offset = config.offset + drift_adjustment;
                real_now.checked_sub(total_offset).unwrap_or(real_now)
            }
        }
    }

    /// Get a monotonic instant, adjusted for drift.
    ///
    /// Note: This is less precise than `now()` for absolute time,
    /// but useful for relative timing.
    pub fn instant(&self) -> Instant {
        if !self.is_enabled() {
            return Instant::now();
        }

        // For Instant, we can only adjust relative to creation time
        // This is a simplified model
        Instant::now()
    }

    /// Sleep for the given duration, adjusted for drift rate.
    ///
    /// If the clock is running faster (drift_rate > 1.0), the actual
    /// sleep will be shorter. If slower (drift_rate < 1.0), it will
    /// be longer.
    pub async fn sleep(&self, duration: Duration) {
        let adjusted = self.adjust_duration(duration);
        tokio::time::sleep(adjusted).await
    }

    /// Create a sleep future for the given duration, adjusted for drift.
    pub fn sleep_future(&self, duration: Duration) -> Sleep {
        let adjusted = self.adjust_duration(duration);
        tokio::time::sleep(adjusted)
    }

    /// Adjust a duration based on drift rate.
    ///
    /// - drift_rate = 1.0: no adjustment
    /// - drift_rate = 2.0: time runs twice as fast, so sleep half as long
    /// - drift_rate = 0.5: time runs half as fast, so sleep twice as long
    pub fn adjust_duration(&self, duration: Duration) -> Duration {
        if !self.is_enabled() {
            return duration;
        }

        let Some(config) = self.inner.config.read().ok().and_then(|c| c.clone()) else {
            return duration;
        };

        if config.drift_rate == 1.0 || config.drift_rate <= 0.0 {
            return duration;
        }

        // If time runs faster, we need to sleep less real time
        Duration::from_secs_f64(duration.as_secs_f64() / config.drift_rate)
    }
}

/// Initialize the global time controller.
///
/// This should be called once at application startup if you want to use
/// the global `now()` and `sleep()` functions.
pub fn init_global(controller: TimeController) {
    let _ = GLOBAL_CONTROLLER.set(controller);
}

/// Get the global time controller.
pub fn global() -> Option<&'static TimeController> {
    GLOBAL_CONTROLLER.get()
}

/// Get the current time using the global controller.
///
/// If no global controller is set, returns the real system time.
pub fn now() -> SystemTime {
    global().map(|c| c.now()).unwrap_or_else(SystemTime::now)
}

/// Sleep using the global controller's drift adjustment.
///
/// If no global controller is set, uses the real duration.
pub async fn sleep(duration: Duration) {
    if let Some(controller) = global() {
        controller.sleep(duration).await
    } else {
        tokio::time::sleep(duration).await
    }
}

/// Adjust a duration using the global controller.
pub fn adjust_duration(duration: Duration) -> Duration {
    global()
        .map(|c| c.adjust_duration(duration))
        .unwrap_or(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_controller_new() {
        let controller = TimeController::new();
        assert!(!controller.is_enabled());

        // With no config, now() should return approximately real time
        let real = SystemTime::now();
        let controlled = controller.now();
        let diff = controlled
            .duration_since(real)
            .or_else(|e| Ok::<_, std::time::SystemTimeError>(e.duration()))
            .unwrap();
        assert!(diff < Duration::from_millis(10));
    }

    #[test]
    fn test_time_controller_ahead() {
        let controller = TimeController::ahead(Duration::from_secs(60));
        assert!(controller.is_enabled());

        let real = SystemTime::now();
        let controlled = controller.now();

        // Controlled time should be about 60 seconds ahead
        let diff = controlled.duration_since(real).unwrap();
        assert!(diff >= Duration::from_secs(59));
        assert!(diff < Duration::from_secs(61));
    }

    #[test]
    fn test_time_controller_behind() {
        let controller = TimeController::behind(Duration::from_secs(60));
        assert!(controller.is_enabled());

        let real = SystemTime::now();
        let controlled = controller.now();

        // Controlled time should be about 60 seconds behind
        let diff = real.duration_since(controlled).unwrap();
        assert!(diff >= Duration::from_secs(59));
        assert!(diff < Duration::from_secs(61));
    }

    #[test]
    fn test_time_controller_clear() {
        let controller = TimeController::ahead(Duration::from_secs(60));
        assert!(controller.is_enabled());

        controller.clear_config();
        assert!(!controller.is_enabled());

        // Now should return approximately real time
        let real = SystemTime::now();
        let controlled = controller.now();
        let diff = controlled
            .duration_since(real)
            .or_else(|e| Ok::<_, std::time::SystemTimeError>(e.duration()))
            .unwrap();
        assert!(diff < Duration::from_millis(10));
    }

    #[test]
    fn test_adjust_duration_normal() {
        let controller = TimeController::new();
        let duration = Duration::from_secs(10);
        assert_eq!(controller.adjust_duration(duration), duration);
    }

    #[test]
    fn test_adjust_duration_faster() {
        let controller = TimeController::ahead(Duration::ZERO);
        controller.set_config(ClockDriftConfig::ahead(Duration::ZERO).with_drift_rate(2.0));

        let duration = Duration::from_secs(10);
        let adjusted = controller.adjust_duration(duration);

        // With drift_rate = 2.0, we should sleep half as long
        assert_eq!(adjusted, Duration::from_secs(5));
    }

    #[test]
    fn test_adjust_duration_slower() {
        let controller = TimeController::ahead(Duration::ZERO);
        controller.set_config(ClockDriftConfig::ahead(Duration::ZERO).with_drift_rate(0.5));

        let duration = Duration::from_secs(10);
        let adjusted = controller.adjust_duration(duration);

        // With drift_rate = 0.5, we should sleep twice as long
        assert_eq!(adjusted, Duration::from_secs(20));
    }

    #[test]
    fn test_time_controller_clone() {
        let controller1 = TimeController::ahead(Duration::from_secs(30));
        let controller2 = controller1.clone();

        // Both should see the same configuration
        assert!(controller1.is_enabled());
        assert!(controller2.is_enabled());

        controller1.clear_config();

        // Change should be visible in both
        assert!(!controller1.is_enabled());
        assert!(!controller2.is_enabled());
    }

    #[tokio::test]
    async fn test_sleep_with_drift() {
        let controller = TimeController::ahead(Duration::ZERO);
        controller.set_config(ClockDriftConfig::ahead(Duration::ZERO).with_drift_rate(2.0));

        let start = Instant::now();
        controller.sleep(Duration::from_millis(100)).await;
        let elapsed = start.elapsed();

        // Should sleep about half the time (50ms instead of 100ms)
        assert!(elapsed < Duration::from_millis(80));
    }

    #[test]
    fn test_global_functions_no_controller() {
        // Without a global controller, now() should return real time
        let real = SystemTime::now();
        let result = now();
        let diff = result
            .duration_since(real)
            .or_else(|e| Ok::<_, std::time::SystemTimeError>(e.duration()))
            .unwrap();
        assert!(diff < Duration::from_millis(10));

        // adjust_duration should return unchanged duration
        let duration = Duration::from_secs(10);
        assert_eq!(adjust_duration(duration), duration);
    }
}
