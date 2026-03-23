//! PlaygroundController - Central coordinator for playground mode.
//!
//! The controller manages all playground rules and emits telemetry events.
//! It is designed to be cheaply cloneable (using `Arc`) and thread-safe.

use crate::error::PlaygroundError;
use quill_core::playground::{
    InterceptContext, LatencyRule, PartitionBehavior, PartitionRule, PlaygroundConfig,
    PlaygroundEvent,
};
use rand::Rng;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;

/// Central controller for playground mode.
///
/// This is the main coordination point for all playground functionality.
/// It manages rules, evaluates them for each request, and emits telemetry.
///
/// The controller is designed to be:
/// - **Cheap to clone**: Uses `Arc<Inner>` for shared state
/// - **Thread-safe**: Uses atomic operations for fast checks
/// - **Non-blocking**: Event emission uses channels, never blocks callers
///
/// # Example
///
/// ```
/// use quill_playground::{PlaygroundController, PlaygroundConfig, LatencyRule};
/// use std::time::Duration;
///
/// let controller = PlaygroundController::new(
///     PlaygroundConfig::builder()
///         .enabled(true)
///         .node_id("node-1")
///         .add_latency_rule(LatencyRule::new("*", Duration::from_millis(50)))
///         .build()
/// );
///
/// // Fast check if enabled (uses atomic bool)
/// if controller.is_enabled() {
///     // Evaluate latency for a request
///     if let Some(delay) = controller.evaluate_latency("my.Service", "MyMethod") {
///         println!("Will inject {:?} latency", delay);
///     }
/// }
/// ```
#[derive(Clone)]
pub struct PlaygroundController {
    inner: Arc<ControllerInner>,
}

struct ControllerInner {
    /// Whether playground mode is enabled (fast atomic check)
    enabled: AtomicBool,
    /// Node identifier for this instance
    node_id: String,
    /// Latency injection rules
    latency_rules: RwLock<Vec<LatencyRule>>,
    /// Partition simulation rules
    partition_rules: RwLock<Vec<PartitionRule>>,
    /// Original configuration
    config: PlaygroundConfig,
    /// Event sender for telemetry
    event_tx: Option<mpsc::UnboundedSender<PlaygroundEvent>>,
    /// Counter for events emitted
    events_emitted: AtomicU64,
    /// Counter for events dropped
    events_dropped: AtomicU64,
}

impl PlaygroundController {
    /// Create a new playground controller with the given configuration.
    pub fn new(config: PlaygroundConfig) -> Self {
        let node_id = config
            .node_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let inner = ControllerInner {
            enabled: AtomicBool::new(config.enabled),
            node_id,
            latency_rules: RwLock::new(config.latency_rules.clone()),
            partition_rules: RwLock::new(config.partition_rules.clone()),
            config,
            event_tx: None,
            events_emitted: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    /// Create a controller with a telemetry event channel.
    pub fn with_event_channel(
        config: PlaygroundConfig,
    ) -> (Self, mpsc::UnboundedReceiver<PlaygroundEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let node_id = config
            .node_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let inner = ControllerInner {
            enabled: AtomicBool::new(config.enabled),
            node_id,
            latency_rules: RwLock::new(config.latency_rules.clone()),
            partition_rules: RwLock::new(config.partition_rules.clone()),
            config,
            event_tx: Some(tx),
            events_emitted: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
        };

        (
            Self {
                inner: Arc::new(inner),
            },
            rx,
        )
    }

    /// Check if playground mode is enabled.
    ///
    /// This is a fast atomic check that should be used before any other operations.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    /// Enable or disable playground mode at runtime.
    pub fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Get the node ID for this controller.
    pub fn node_id(&self) -> &str {
        &self.inner.node_id
    }

    /// Get the configuration.
    pub fn config(&self) -> &PlaygroundConfig {
        &self.inner.config
    }

    /// Evaluate latency rules and return the total delay to inject.
    ///
    /// Returns `None` if no rules match or playground is disabled.
    ///
    /// # Example
    ///
    /// ```
    /// use quill_playground::{PlaygroundController, PlaygroundConfig, LatencyRule};
    /// use std::time::Duration;
    ///
    /// let controller = PlaygroundController::new(
    ///     PlaygroundConfig::builder()
    ///         .enabled(true)
    ///         .add_latency_rule(LatencyRule::new("my.*", Duration::from_millis(100)))
    ///         .build()
    /// );
    ///
    /// let delay = controller.evaluate_latency("my.Service", "Method");
    /// assert!(delay.is_some());
    /// ```
    pub fn evaluate_latency(&self, service: &str, method: &str) -> Option<Duration> {
        if !self.is_enabled() {
            return None;
        }

        let rules = self.inner.latency_rules.read().ok()?;
        let mut total_delay = Duration::ZERO;
        let mut rng = rand::thread_rng();

        for rule in rules.iter() {
            if !matches_pattern(&rule.service_pattern, service) {
                continue;
            }

            if let Some(ref method_pattern) = rule.method_pattern {
                if !matches_pattern(method_pattern, method) {
                    continue;
                }
            }

            // Check probability
            if rule.probability < 1.0 && rng.gen::<f64>() > rule.probability {
                continue;
            }

            // Calculate delay with jitter
            let mut delay = rule.base_latency;
            if let Some(jitter) = rule.jitter {
                let jitter_ms = jitter.as_millis() as i64;
                let jitter_offset = rng.gen_range(-jitter_ms..=jitter_ms);
                if jitter_offset >= 0 {
                    delay += Duration::from_millis(jitter_offset as u64);
                } else {
                    delay = delay.saturating_sub(Duration::from_millis((-jitter_offset) as u64));
                }
            }

            total_delay += delay;
        }

        if total_delay > Duration::ZERO {
            Some(total_delay)
        } else {
            None
        }
    }

    /// Evaluate partition rules for a request.
    ///
    /// Returns `Ok(())` if the request should proceed, or an error if
    /// the request should be blocked.
    pub fn evaluate_partition(
        &self,
        ctx: &InterceptContext,
    ) -> Result<(), PlaygroundError> {
        if !self.is_enabled() {
            return Ok(());
        }

        let rules = self
            .inner
            .partition_rules
            .read()
            .map_err(|_| PlaygroundError::InvalidRule("Lock poisoned".to_string()))?;

        let source = ctx
            .source_node
            .as_deref()
            .unwrap_or(&self.inner.node_id);
        let dest = ctx
            .destination_node
            .as_deref()
            .unwrap_or(&ctx.service_name);

        let mut rng = rand::thread_rng();

        for rule in rules.iter() {
            if !matches_pattern(&rule.from_pattern, source) {
                continue;
            }
            if !matches_pattern(&rule.to_pattern, dest) {
                continue;
            }

            match &rule.behavior {
                PartitionBehavior::DropAll => {
                    return Err(PlaygroundError::partition_drop(source, dest));
                }
                PartitionBehavior::DropPercent(pct) => {
                    if rng.gen::<f64>() < *pct {
                        return Err(PlaygroundError::partition_drop(source, dest));
                    }
                }
                PartitionBehavior::Timeout(duration) => {
                    return Err(PlaygroundError::partition_timeout(source, dest, *duration));
                }
                PartitionBehavior::Error(err) => {
                    return Err(PlaygroundError::partition_error(err.status, &err.message));
                }
            }
        }

        Ok(())
    }

    /// Add a latency rule at runtime.
    pub fn add_latency_rule(&self, rule: LatencyRule) {
        if let Ok(mut rules) = self.inner.latency_rules.write() {
            rules.push(rule);
        }
    }

    /// Add a partition rule at runtime.
    pub fn add_partition_rule(&self, rule: PartitionRule) {
        if let Ok(mut rules) = self.inner.partition_rules.write() {
            rules.push(rule);
        }
    }

    /// Clear all latency rules.
    pub fn clear_latency_rules(&self) {
        if let Ok(mut rules) = self.inner.latency_rules.write() {
            rules.clear();
        }
    }

    /// Clear all partition rules.
    pub fn clear_partition_rules(&self) {
        if let Ok(mut rules) = self.inner.partition_rules.write() {
            rules.clear();
        }
    }

    /// Emit a telemetry event.
    ///
    /// This method never blocks. If the channel is full or not connected,
    /// the event is dropped and a counter is incremented.
    pub fn emit_event(&self, event: PlaygroundEvent) {
        if let Some(ref tx) = self.inner.event_tx {
            if tx.send(event).is_ok() {
                self.inner.events_emitted.fetch_add(1, Ordering::Relaxed);
            } else {
                self.inner.events_dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Get the number of events emitted.
    pub fn events_emitted(&self) -> u64 {
        self.inner.events_emitted.load(Ordering::Relaxed)
    }

    /// Get the number of events dropped.
    pub fn events_dropped(&self) -> u64 {
        self.inner.events_dropped.load(Ordering::Relaxed)
    }
}

/// Simple wildcard pattern matching.
///
/// Supports:
/// - `*` matches everything
/// - `prefix*` matches strings starting with prefix
/// - `*suffix` matches strings ending with suffix
/// - exact match otherwise
fn matches_pattern(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with('*') && pattern.ends_with('*') && pattern.len() > 2 {
        let middle = &pattern[1..pattern.len() - 1];
        return value.contains(middle);
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }

    pattern == value
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill_core::playground::PartitionError;

    #[test]
    fn test_controller_new() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("test-node")
            .build();

        let controller = PlaygroundController::new(config);
        assert!(controller.is_enabled());
        assert_eq!(controller.node_id(), "test-node");
    }

    #[test]
    fn test_controller_disabled() {
        let config = PlaygroundConfig::builder()
            .enabled(false)
            .node_id("test")
            .build();

        let controller = PlaygroundController::new(config);
        assert!(!controller.is_enabled());

        // Latency should return None when disabled
        let delay = controller.evaluate_latency("any.Service", "AnyMethod");
        assert!(delay.is_none());
    }

    #[test]
    fn test_set_enabled() {
        let controller = PlaygroundController::new(PlaygroundConfig::default());
        assert!(!controller.is_enabled());

        controller.set_enabled(true);
        assert!(controller.is_enabled());

        controller.set_enabled(false);
        assert!(!controller.is_enabled());
    }

    #[test]
    fn test_evaluate_latency() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .add_latency_rule(LatencyRule::new("my.*", Duration::from_millis(100)))
            .build();

        let controller = PlaygroundController::new(config);

        // Should match
        let delay = controller.evaluate_latency("my.Service", "Method");
        assert!(delay.is_some());
        assert!(delay.unwrap() >= Duration::from_millis(90)); // Allow for jitter

        // Should not match
        let delay = controller.evaluate_latency("other.Service", "Method");
        assert!(delay.is_none());
    }

    #[test]
    fn test_evaluate_latency_with_method_pattern() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .add_latency_rule(
                LatencyRule::new("*", Duration::from_millis(50)).with_method("Get*"),
            )
            .build();

        let controller = PlaygroundController::new(config);

        // Should match
        let delay = controller.evaluate_latency("any.Service", "GetUser");
        assert!(delay.is_some());

        // Should not match
        let delay = controller.evaluate_latency("any.Service", "CreateUser");
        assert!(delay.is_none());
    }

    #[test]
    fn test_evaluate_partition_drop() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("node-a")
            .add_partition_rule(PartitionRule::drop_all("node-a", "node-b"))
            .build();

        let controller = PlaygroundController::new(config);

        let ctx = InterceptContext::new("test.Service", "Method")
            .with_destination_node("node-b");

        let result = controller.evaluate_partition(&ctx);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlaygroundError::PartitionDrop { .. }
        ));
    }

    #[test]
    fn test_evaluate_partition_timeout() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("node-a")
            .add_partition_rule(PartitionRule::timeout(
                "node-a",
                "*",
                Duration::from_secs(30),
            ))
            .build();

        let controller = PlaygroundController::new(config);

        let ctx = InterceptContext::new("test.Service", "Method");

        let result = controller.evaluate_partition(&ctx);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlaygroundError::PartitionTimeout { timeout, .. } if timeout == Duration::from_secs(30)
        ));
    }

    #[test]
    fn test_evaluate_partition_error() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("node-a")
            .add_partition_rule(PartitionRule {
                from_pattern: "*".to_string(),
                to_pattern: "*.database".to_string(),
                behavior: PartitionBehavior::Error(PartitionError::new(503, "Database unavailable")),
                duration: None,
                schedule: None,
            })
            .build();

        let controller = PlaygroundController::new(config);

        let ctx = InterceptContext::new("test.Service", "Method")
            .with_destination_node("mysql.database");

        let result = controller.evaluate_partition(&ctx);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PlaygroundError::PartitionError { status: 503, .. }
        ));
    }

    #[test]
    fn test_add_rules_at_runtime() {
        let config = PlaygroundConfig::builder().enabled(true).build();
        let controller = PlaygroundController::new(config);

        // No rules initially
        let delay = controller.evaluate_latency("any.Service", "Method");
        assert!(delay.is_none());

        // Add rule at runtime
        controller.add_latency_rule(LatencyRule::new("*", Duration::from_millis(10)));

        let delay = controller.evaluate_latency("any.Service", "Method");
        assert!(delay.is_some());

        // Clear rules
        controller.clear_latency_rules();
        let delay = controller.evaluate_latency("any.Service", "Method");
        assert!(delay.is_none());
    }

    #[test]
    fn test_event_channel() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("test")
            .build();

        let (controller, mut rx) = PlaygroundController::with_event_channel(config);

        // Emit event
        let event = PlaygroundEvent::heartbeat("test", quill_core::playground::event::NodeStatus::Healthy);
        controller.emit_event(event);

        // Receive event
        let received = rx.try_recv();
        assert!(received.is_ok());
        assert_eq!(controller.events_emitted(), 1);
        assert_eq!(controller.events_dropped(), 0);
    }

    #[test]
    fn test_matches_pattern() {
        // Wildcard
        assert!(matches_pattern("*", "anything"));

        // Prefix
        assert!(matches_pattern("my.*", "my.Service"));
        assert!(!matches_pattern("my.*", "other.Service"));

        // Suffix
        assert!(matches_pattern("*.Service", "my.Service"));
        assert!(!matches_pattern("*.Service", "my.Handler"));

        // Contains
        assert!(matches_pattern("*Service*", "MyServiceHandler"));
        assert!(!matches_pattern("*Service*", "MyHandler"));

        // Exact
        assert!(matches_pattern("exact", "exact"));
        assert!(!matches_pattern("exact", "notexact"));
    }

    #[test]
    fn test_controller_is_clone() {
        let config = PlaygroundConfig::builder().enabled(true).build();
        let controller = PlaygroundController::new(config);

        let controller2 = controller.clone();
        controller.set_enabled(false);

        // Both should see the change (shared state)
        assert!(!controller.is_enabled());
        assert!(!controller2.is_enabled());
    }
}
