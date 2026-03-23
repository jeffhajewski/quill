//! Configuration types for Playground mode.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for Playground Mode
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaygroundConfig {
    /// Whether playground mode is enabled
    pub enabled: bool,
    /// Unique identifier for this node
    pub node_id: Option<String>,
    /// Rules for injecting latency
    pub latency_rules: Vec<LatencyRule>,
    /// Rules for simulating network partitions
    pub partition_rules: Vec<PartitionRule>,
    /// Configuration for clock drift simulation
    pub clock_drift: Option<ClockDriftConfig>,
    /// Configuration for telemetry collection
    pub telemetry: Option<TelemetryConfig>,
}

impl PlaygroundConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder for playground configuration
    pub fn builder() -> PlaygroundConfigBuilder {
        PlaygroundConfigBuilder::default()
    }

    /// Check if playground mode is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

/// Builder for PlaygroundConfig
#[derive(Debug, Default)]
pub struct PlaygroundConfigBuilder {
    config: PlaygroundConfig,
}

impl PlaygroundConfigBuilder {
    /// Enable or disable playground mode
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.config.enabled = enabled;
        self
    }

    /// Set the node identifier
    pub fn node_id(mut self, id: impl Into<String>) -> Self {
        self.config.node_id = Some(id.into());
        self
    }

    /// Add a latency injection rule
    pub fn add_latency_rule(mut self, rule: LatencyRule) -> Self {
        self.config.latency_rules.push(rule);
        self
    }

    /// Add a partition simulation rule
    pub fn add_partition_rule(mut self, rule: PartitionRule) -> Self {
        self.config.partition_rules.push(rule);
        self
    }

    /// Set clock drift configuration
    pub fn clock_drift(mut self, config: ClockDriftConfig) -> Self {
        self.config.clock_drift = Some(config);
        self
    }

    /// Set telemetry configuration
    pub fn telemetry(mut self, config: TelemetryConfig) -> Self {
        self.config.telemetry = Some(config);
        self
    }

    /// Build the configuration
    pub fn build(self) -> PlaygroundConfig {
        self.config
    }
}

/// Rule for injecting latency into RPC calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyRule {
    /// Pattern to match service names (supports "*" wildcard)
    pub service_pattern: String,
    /// Pattern to match method names (supports "*" wildcard)
    pub method_pattern: Option<String>,
    /// Base latency to inject
    #[serde(with = "duration_millis")]
    pub base_latency: Duration,
    /// Random jitter to add/subtract from base latency
    #[serde(with = "option_duration_millis", default)]
    pub jitter: Option<Duration>,
    /// Probability of applying this rule (0.0 to 1.0)
    #[serde(default = "default_probability")]
    pub probability: f64,
    /// Schedule for when this rule is active
    pub schedule: Option<RuleSchedule>,
}

fn default_probability() -> f64 {
    1.0
}

impl LatencyRule {
    /// Create a new latency rule
    pub fn new(service_pattern: impl Into<String>, latency: Duration) -> Self {
        Self {
            service_pattern: service_pattern.into(),
            method_pattern: None,
            base_latency: latency,
            jitter: None,
            probability: 1.0,
            schedule: None,
        }
    }

    /// Set the method pattern
    pub fn with_method(mut self, pattern: impl Into<String>) -> Self {
        self.method_pattern = Some(pattern.into());
        self
    }

    /// Set jitter
    pub fn with_jitter(mut self, jitter: Duration) -> Self {
        self.jitter = Some(jitter);
        self
    }

    /// Set probability
    pub fn with_probability(mut self, probability: f64) -> Self {
        self.probability = probability.clamp(0.0, 1.0);
        self
    }
}

/// Rule for simulating network partitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionRule {
    /// Pattern for source node/service
    pub from_pattern: String,
    /// Pattern for destination node/service
    pub to_pattern: String,
    /// Behavior when partition is triggered
    pub behavior: PartitionBehavior,
    /// Duration of the partition (None = permanent until removed)
    #[serde(with = "option_duration_millis", default)]
    pub duration: Option<Duration>,
    /// Schedule for when this rule is active
    pub schedule: Option<RuleSchedule>,
}

impl PartitionRule {
    /// Create a new partition rule that drops all traffic
    pub fn drop_all(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from_pattern: from.into(),
            to_pattern: to.into(),
            behavior: PartitionBehavior::DropAll,
            duration: None,
            schedule: None,
        }
    }

    /// Create a partition rule that drops a percentage of traffic
    pub fn drop_percent(from: impl Into<String>, to: impl Into<String>, percent: f64) -> Self {
        Self {
            from_pattern: from.into(),
            to_pattern: to.into(),
            behavior: PartitionBehavior::DropPercent(percent.clamp(0.0, 1.0)),
            duration: None,
            schedule: None,
        }
    }

    /// Create a partition rule that causes timeouts
    pub fn timeout(from: impl Into<String>, to: impl Into<String>, timeout: Duration) -> Self {
        Self {
            from_pattern: from.into(),
            to_pattern: to.into(),
            behavior: PartitionBehavior::Timeout(timeout),
            duration: None,
            schedule: None,
        }
    }

    /// Set the duration of this partition
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
}

/// Behavior when a partition rule matches
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PartitionBehavior {
    /// Drop all traffic
    DropAll,
    /// Drop a percentage of traffic (0.0 to 1.0)
    DropPercent(f64),
    /// Cause requests to timeout after the specified duration
    Timeout(#[serde(with = "duration_millis")] Duration),
    /// Return a specific error
    Error(PartitionError),
}

/// Error to return when a partition rule triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionError {
    /// HTTP status code
    pub status: u16,
    /// Error message
    pub message: String,
}

impl PartitionError {
    /// Create a new partition error
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

/// Clock drift configuration for time simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockDriftConfig {
    /// Initial offset from real time
    #[serde(with = "duration_millis")]
    pub offset: Duration,
    /// Direction of the offset
    pub direction: ClockDirection,
    /// Drift rate (seconds per second, e.g., 1.01 = 1% faster)
    #[serde(default = "default_drift_rate")]
    pub drift_rate: f64,
}

fn default_drift_rate() -> f64 {
    1.0
}

impl ClockDriftConfig {
    /// Create a clock that runs ahead by the specified offset
    pub fn ahead(offset: Duration) -> Self {
        Self {
            offset,
            direction: ClockDirection::Ahead,
            drift_rate: 1.0,
        }
    }

    /// Create a clock that runs behind by the specified offset
    pub fn behind(offset: Duration) -> Self {
        Self {
            offset,
            direction: ClockDirection::Behind,
            drift_rate: 1.0,
        }
    }

    /// Set the drift rate
    pub fn with_drift_rate(mut self, rate: f64) -> Self {
        self.drift_rate = rate;
        self
    }
}

/// Direction of clock drift
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClockDirection {
    /// Clock runs ahead of real time
    Ahead,
    /// Clock runs behind real time
    Behind,
}

/// Schedule for when rules are active
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSchedule {
    /// Start time (Unix timestamp in milliseconds)
    pub start_ms: Option<u64>,
    /// End time (Unix timestamp in milliseconds)
    pub end_ms: Option<u64>,
    /// Active periods as (offset_ms, duration_ms) pairs
    pub active_periods: Vec<(u64, u64)>,
}

impl RuleSchedule {
    /// Create a schedule with no time constraints
    pub fn always() -> Self {
        Self {
            start_ms: None,
            end_ms: None,
            active_periods: Vec::new(),
        }
    }

    /// Create a schedule with a start time
    pub fn starting_at(start_ms: u64) -> Self {
        Self {
            start_ms: Some(start_ms),
            end_ms: None,
            active_periods: Vec::new(),
        }
    }

    /// Set the end time
    pub fn ending_at(mut self, end_ms: u64) -> Self {
        self.end_ms = Some(end_ms);
        self
    }
}

/// Configuration for telemetry collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Size of the event buffer
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// Dashboard WebSocket URL
    pub dashboard_url: Option<String>,
    /// Sampling rate for events (0.0 to 1.0)
    #[serde(default = "default_sampling_rate")]
    pub sampling_rate: f64,
    /// Whether to capture request/response bodies
    #[serde(default)]
    pub capture_bodies: bool,
    /// Maximum body size to capture (bytes)
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,
}

fn default_buffer_size() -> usize {
    10000
}

fn default_sampling_rate() -> f64 {
    1.0
}

fn default_max_body_size() -> usize {
    4096
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            buffer_size: default_buffer_size(),
            dashboard_url: None,
            sampling_rate: default_sampling_rate(),
            capture_bodies: false,
            max_body_size: default_max_body_size(),
        }
    }
}

impl TelemetryConfig {
    /// Create telemetry config with a dashboard URL
    pub fn with_dashboard(url: impl Into<String>) -> Self {
        Self {
            dashboard_url: Some(url.into()),
            ..Default::default()
        }
    }

    /// Enable body capture
    pub fn capture_bodies(mut self, capture: bool) -> Self {
        self.capture_bodies = capture;
        self
    }

    /// Set sampling rate
    pub fn with_sampling_rate(mut self, rate: f64) -> Self {
        self.sampling_rate = rate.clamp(0.0, 1.0);
        self
    }
}

// Serde helpers for Duration
mod duration_millis {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

mod option_duration_millis {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match duration {
            Some(d) => d.as_millis().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<u64> = Option::deserialize(deserializer)?;
        Ok(opt.map(Duration::from_millis))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playground_config_builder() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("test-node")
            .add_latency_rule(LatencyRule::new("*", Duration::from_millis(100)))
            .build();

        assert!(config.enabled);
        assert_eq!(config.node_id, Some("test-node".to_string()));
        assert_eq!(config.latency_rules.len(), 1);
    }

    #[test]
    fn test_latency_rule_builder() {
        let rule = LatencyRule::new("my.service", Duration::from_millis(50))
            .with_method("MyMethod")
            .with_jitter(Duration::from_millis(10))
            .with_probability(0.5);

        assert_eq!(rule.service_pattern, "my.service");
        assert_eq!(rule.method_pattern, Some("MyMethod".to_string()));
        assert_eq!(rule.base_latency, Duration::from_millis(50));
        assert_eq!(rule.jitter, Some(Duration::from_millis(10)));
        assert_eq!(rule.probability, 0.5);
    }

    #[test]
    fn test_partition_rule_builders() {
        let drop_all = PartitionRule::drop_all("node-1", "node-2");
        assert!(matches!(drop_all.behavior, PartitionBehavior::DropAll));

        let drop_50 = PartitionRule::drop_percent("*", "*", 0.5);
        assert!(matches!(drop_50.behavior, PartitionBehavior::DropPercent(p) if (p - 0.5).abs() < 0.001));

        let timeout = PartitionRule::timeout("a", "b", Duration::from_secs(5));
        assert!(matches!(timeout.behavior, PartitionBehavior::Timeout(d) if d == Duration::from_secs(5)));
    }

    #[test]
    fn test_clock_drift_config() {
        let ahead = ClockDriftConfig::ahead(Duration::from_secs(1)).with_drift_rate(1.01);
        assert_eq!(ahead.direction, ClockDirection::Ahead);
        assert_eq!(ahead.offset, Duration::from_secs(1));
        assert!((ahead.drift_rate - 1.01).abs() < 0.001);

        let behind = ClockDriftConfig::behind(Duration::from_millis(500));
        assert_eq!(behind.direction, ClockDirection::Behind);
    }

    #[test]
    fn test_config_serialization() {
        let config = PlaygroundConfig::builder()
            .enabled(true)
            .node_id("test")
            .add_latency_rule(LatencyRule::new("*", Duration::from_millis(100)))
            .build();

        let json = serde_json::to_string(&config).unwrap();
        let parsed: PlaygroundConfig = serde_json::from_str(&json).unwrap();

        assert!(parsed.enabled);
        assert_eq!(parsed.node_id, Some("test".to_string()));
    }
}
