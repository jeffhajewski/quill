//! Flow control for streaming RPCs.
//!
//! Credit-based flow control prevents buffer overflow by limiting the number
//! of messages a sender can transmit before receiving more credits from the receiver.
//!
//! Two types of credit tracking are supported:
//! - `CreditTracker`: Message-based credits for standard RPC streaming
//! - `TensorCreditTracker`: Byte-based credits for tensor/ML workloads

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Default initial credits granted to senders
pub const DEFAULT_INITIAL_CREDITS: u32 = 16;

/// Default credits to grant when buffer space becomes available
pub const DEFAULT_CREDIT_REFILL: u32 = 8;

/// Credit tracker for flow control
///
/// The sender tracks available credits and decrements them when sending messages.
/// The receiver tracks consumed credits and grants more when buffer space is available.
#[derive(Debug, Clone)]
pub struct CreditTracker {
    /// Number of available credits (for senders) or consumed credits (for receivers)
    credits: Arc<AtomicU32>,
}

impl CreditTracker {
    /// Create a new credit tracker with the specified initial credits
    pub fn new(initial_credits: u32) -> Self {
        Self {
            credits: Arc::new(AtomicU32::new(initial_credits)),
        }
    }

    /// Create a credit tracker with default initial credits
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_INITIAL_CREDITS)
    }

    /// Try to consume one credit for sending a message
    ///
    /// Returns true if a credit was available and consumed, false otherwise
    pub fn try_consume(&self) -> bool {
        let mut current = self.credits.load(Ordering::Acquire);
        loop {
            if current == 0 {
                return false;
            }
            match self.credits.compare_exchange_weak(
                current,
                current - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Grant additional credits
    pub fn grant(&self, amount: u32) {
        self.credits.fetch_add(amount, Ordering::AcqRel);
    }

    /// Get the current number of available credits
    pub fn available(&self) -> u32 {
        self.credits.load(Ordering::Acquire)
    }

    /// Set credits to a specific value
    pub fn set(&self, value: u32) {
        self.credits.store(value, Ordering::Release);
    }
}

impl Default for CreditTracker {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// Tensor Flow Control
// ============================================================================

/// Default initial byte budget for tensor streaming (256 KB)
pub const DEFAULT_TENSOR_INITIAL_BYTES: u64 = 256 * 1024;

/// Default high water mark - pause sending above this (512 KB)
pub const DEFAULT_TENSOR_HIGH_WATER: u64 = 512 * 1024;

/// Default low water mark - resume sending below this (128 KB)
pub const DEFAULT_TENSOR_LOW_WATER: u64 = 128 * 1024;

/// Byte-based credit tracker for tensor workloads.
///
/// Unlike message-based flow control, tensor flow control uses byte budgets
/// to handle variable-sized tensor data efficiently. This prevents memory
/// exhaustion when streaming large tensors while maintaining throughput.
///
/// # High/Low Water Marks
///
/// The tracker uses hysteresis to prevent oscillation:
/// - When `bytes_in_flight` exceeds `high_water`, sending should pause
/// - Sending can resume when `bytes_in_flight` drops below `low_water`
///
/// # Example
///
/// ```rust
/// use quill_core::flow_control::TensorCreditTracker;
///
/// let tracker = TensorCreditTracker::new();
///
/// // Check if we can send 64KB
/// if tracker.try_consume(65536) {
///     // Send data...
/// }
///
/// // Receiver acknowledges data
/// tracker.grant(65536);
/// ```
#[derive(Debug)]
pub struct TensorCreditTracker {
    /// Available byte budget
    bytes_budget: Arc<AtomicU64>,
    /// Maximum bytes in flight before pausing
    high_water: u64,
    /// Resume sending when bytes in flight drops below this
    low_water: u64,
    /// Current pause state (for hysteresis)
    paused: Arc<std::sync::atomic::AtomicBool>,
}

impl TensorCreditTracker {
    /// Creates a new tracker with default settings.
    pub fn new() -> Self {
        Self {
            bytes_budget: Arc::new(AtomicU64::new(DEFAULT_TENSOR_INITIAL_BYTES)),
            high_water: DEFAULT_TENSOR_HIGH_WATER,
            low_water: DEFAULT_TENSOR_LOW_WATER,
            paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Creates a tracker with custom settings.
    pub fn with_settings(initial_bytes: u64, high_water: u64, low_water: u64) -> Self {
        assert!(
            low_water < high_water,
            "low_water must be less than high_water"
        );
        Self {
            bytes_budget: Arc::new(AtomicU64::new(initial_bytes)),
            high_water,
            low_water,
            paused: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Creates a tracker optimized for small tensors (embeddings, activations).
    pub fn for_small_tensors() -> Self {
        Self::with_settings(
            128 * 1024,  // 128 KB initial
            256 * 1024,  // 256 KB high water
            64 * 1024,   // 64 KB low water
        )
    }

    /// Creates a tracker optimized for large tensors (model weights, features).
    pub fn for_large_tensors() -> Self {
        Self::with_settings(
            1024 * 1024,      // 1 MB initial
            2 * 1024 * 1024,  // 2 MB high water
            512 * 1024,       // 512 KB low water
        )
    }

    /// Tries to consume bytes from the budget.
    ///
    /// Returns `true` if the bytes were consumed, `false` if insufficient budget.
    pub fn try_consume(&self, bytes: u64) -> bool {
        let mut current = self.bytes_budget.load(Ordering::Acquire);
        loop {
            if current < bytes {
                return false;
            }
            match self.bytes_budget.compare_exchange_weak(
                current,
                current - bytes,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    // Check if we should pause
                    if current - bytes < self.low_water {
                        self.paused.store(true, Ordering::Release);
                    }
                    return true;
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Grants additional bytes to the budget.
    pub fn grant(&self, bytes: u64) {
        let new_budget = self.bytes_budget.fetch_add(bytes, Ordering::AcqRel) + bytes;

        // Only unpause when we exceed high water mark (hysteresis behavior)
        // This prevents oscillation between paused/unpaused states
        if new_budget > self.high_water {
            self.paused.store(false, Ordering::Release);
        }
    }

    /// Returns the current available byte budget.
    pub fn available(&self) -> u64 {
        self.bytes_budget.load(Ordering::Acquire)
    }

    /// Returns whether sending should be paused.
    ///
    /// Uses hysteresis: pauses at high water mark, resumes at low water mark.
    pub fn should_pause(&self) -> bool {
        let available = self.bytes_budget.load(Ordering::Acquire);

        // Update pause state based on water marks
        if available > self.high_water {
            self.paused.store(false, Ordering::Release);
            return false;
        }

        if available < self.low_water {
            self.paused.store(true, Ordering::Release);
            return true;
        }

        // In between water marks - use cached state (hysteresis)
        self.paused.load(Ordering::Acquire)
    }

    /// Returns whether the tracker is currently in paused state.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Sets the byte budget to a specific value.
    pub fn set_budget(&self, bytes: u64) {
        self.bytes_budget.store(bytes, Ordering::Release);
        self.paused.store(bytes < self.low_water, Ordering::Release);
    }

    /// Returns the high water mark.
    pub fn high_water(&self) -> u64 {
        self.high_water
    }

    /// Returns the low water mark.
    pub fn low_water(&self) -> u64 {
        self.low_water
    }

    /// Calculates suggested grant size based on current state.
    ///
    /// Returns a suggested number of bytes to grant to maintain throughput
    /// while respecting the high water mark.
    pub fn suggested_grant(&self) -> u64 {
        let current = self.bytes_budget.load(Ordering::Acquire);
        if current >= self.high_water {
            0
        } else {
            // Grant enough to reach between low and high water
            let target = (self.low_water + self.high_water) / 2;
            target.saturating_sub(current)
        }
    }
}

impl Default for TensorCreditTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for TensorCreditTracker {
    fn clone(&self) -> Self {
        Self {
            bytes_budget: Arc::clone(&self.bytes_budget),
            high_water: self.high_water,
            low_water: self.low_water,
            paused: Arc::clone(&self.paused),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_consumption() {
        let tracker = CreditTracker::new(5);
        assert_eq!(tracker.available(), 5);

        assert!(tracker.try_consume());
        assert_eq!(tracker.available(), 4);

        assert!(tracker.try_consume());
        assert_eq!(tracker.available(), 3);
    }

    #[test]
    fn test_credit_exhaustion() {
        let tracker = CreditTracker::new(2);

        assert!(tracker.try_consume());
        assert!(tracker.try_consume());
        assert!(!tracker.try_consume()); // Should fail
    }

    #[test]
    fn test_credit_grant() {
        let tracker = CreditTracker::new(1);

        assert!(tracker.try_consume());
        assert!(!tracker.try_consume());

        tracker.grant(5);
        assert_eq!(tracker.available(), 5);
        assert!(tracker.try_consume());
    }

    // TensorCreditTracker tests

    #[test]
    fn test_tensor_credit_consumption() {
        let tracker = TensorCreditTracker::with_settings(
            100 * 1024,  // 100 KB initial
            200 * 1024,  // 200 KB high water
            50 * 1024,   // 50 KB low water
        );

        assert_eq!(tracker.available(), 100 * 1024);

        // Consume 30 KB
        assert!(tracker.try_consume(30 * 1024));
        assert_eq!(tracker.available(), 70 * 1024);

        // Consume another 30 KB
        assert!(tracker.try_consume(30 * 1024));
        assert_eq!(tracker.available(), 40 * 1024);

        // Should be paused now (below low water of 50 KB)
        assert!(tracker.is_paused());
    }

    #[test]
    fn test_tensor_credit_exhaustion() {
        let tracker = TensorCreditTracker::with_settings(
            64 * 1024,   // 64 KB
            128 * 1024,
            32 * 1024,
        );

        // Try to consume more than available
        assert!(!tracker.try_consume(100 * 1024));

        // Consume all
        assert!(tracker.try_consume(64 * 1024));
        assert!(!tracker.try_consume(1));
        assert_eq!(tracker.available(), 0);
    }

    #[test]
    fn test_tensor_credit_grant() {
        let tracker = TensorCreditTracker::with_settings(
            10 * 1024,   // Start with 10 KB
            100 * 1024,  // High water
            50 * 1024,   // Low water
        );

        // Should be paused initially (below low water)
        assert!(tracker.should_pause());

        // Grant 60 KB - now at 70 KB (above low water, below high water)
        tracker.grant(60 * 1024);
        assert_eq!(tracker.available(), 70 * 1024);

        // Still paused due to hysteresis (haven't exceeded high water yet)
        assert!(tracker.should_pause());

        // Grant more to exceed high water
        tracker.grant(40 * 1024);  // Now at 110 KB (above high water)
        assert_eq!(tracker.available(), 110 * 1024);

        // Now should be unpaused
        assert!(!tracker.should_pause());
    }

    #[test]
    fn test_tensor_hysteresis() {
        let tracker = TensorCreditTracker::with_settings(
            75 * 1024,   // 75 KB - between water marks
            100 * 1024,  // High water
            50 * 1024,   // Low water
        );

        // Initially not paused
        assert!(!tracker.should_pause());

        // Consume to drop below low water
        assert!(tracker.try_consume(30 * 1024)); // Now at 45 KB
        assert!(tracker.should_pause());

        // Grant some, still between water marks
        tracker.grant(10 * 1024); // Now at 55 KB
        // Should still be paused due to hysteresis
        assert!(tracker.should_pause());

        // Grant more to get above low water
        tracker.grant(50 * 1024); // Now at 105 KB
        // Now above high water, should unpause
        assert!(!tracker.should_pause());
    }

    #[test]
    fn test_suggested_grant() {
        let tracker = TensorCreditTracker::with_settings(
            50 * 1024,   // 50 KB initial
            200 * 1024,  // High water
            100 * 1024,  // Low water
        );

        // Suggested grant should bring us to ~150 KB (midpoint)
        let suggested = tracker.suggested_grant();
        assert!(suggested > 0);
        assert_eq!(suggested, 150 * 1024 - 50 * 1024); // 100 KB

        // If we're above high water, no grant needed
        tracker.set_budget(250 * 1024);
        assert_eq!(tracker.suggested_grant(), 0);
    }

    #[test]
    fn test_tensor_preset_configs() {
        let small = TensorCreditTracker::for_small_tensors();
        assert_eq!(small.available(), 128 * 1024);
        assert_eq!(small.high_water(), 256 * 1024);
        assert_eq!(small.low_water(), 64 * 1024);

        let large = TensorCreditTracker::for_large_tensors();
        assert_eq!(large.available(), 1024 * 1024);
        assert_eq!(large.high_water(), 2 * 1024 * 1024);
        assert_eq!(large.low_water(), 512 * 1024);
    }
}
