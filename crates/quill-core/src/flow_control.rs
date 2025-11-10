//! Flow control for streaming RPCs.
//!
//! Credit-based flow control prevents buffer overflow by limiting the number
//! of messages a sender can transmit before receiving more credits from the receiver.

use std::sync::atomic::{AtomicU32, Ordering};
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
}
