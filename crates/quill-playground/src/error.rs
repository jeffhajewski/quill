//! Error types for playground mode.

use std::time::Duration;
use thiserror::Error;

/// Errors that can occur in playground mode.
#[derive(Debug, Error)]
pub enum PlaygroundError {
    /// Playground mode is not enabled
    #[error("Playground mode is not enabled")]
    NotEnabled,

    /// Request was dropped due to a partition rule
    #[error("Request dropped: partition between {from} and {to}")]
    PartitionDrop {
        /// Source node/service
        from: String,
        /// Destination node/service
        to: String,
        /// Rule ID that triggered the drop
        rule_id: Option<String>,
    },

    /// Request timed out due to a partition rule
    #[error("Request timed out after {timeout:?}: partition between {from} and {to}")]
    PartitionTimeout {
        /// Source node/service
        from: String,
        /// Destination node/service
        to: String,
        /// Timeout duration
        timeout: Duration,
        /// Rule ID that triggered the timeout
        rule_id: Option<String>,
    },

    /// Partition rule returned an error
    #[error("Partition error ({status}): {message}")]
    PartitionError {
        /// HTTP status code
        status: u16,
        /// Error message
        message: String,
        /// Rule ID that triggered the error
        rule_id: Option<String>,
    },

    /// Failed to connect to dashboard
    #[error("Failed to connect to dashboard at {url}: {reason}")]
    DashboardConnectionFailed {
        /// Dashboard URL
        url: String,
        /// Failure reason
        reason: String,
    },

    /// Telemetry channel is full (events being dropped)
    #[error("Telemetry buffer full, events being dropped")]
    TelemetryBufferFull,

    /// Invalid rule configuration
    #[error("Invalid rule configuration: {0}")]
    InvalidRule(String),

    /// Pattern matching error
    #[error("Pattern matching error: {0}")]
    PatternError(String),
}

impl PlaygroundError {
    /// Create a partition drop error.
    pub fn partition_drop(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::PartitionDrop {
            from: from.into(),
            to: to.into(),
            rule_id: None,
        }
    }

    /// Create a partition timeout error.
    pub fn partition_timeout(
        from: impl Into<String>,
        to: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self::PartitionTimeout {
            from: from.into(),
            to: to.into(),
            timeout,
            rule_id: None,
        }
    }

    /// Create a partition error response.
    pub fn partition_error(status: u16, message: impl Into<String>) -> Self {
        Self::PartitionError {
            status,
            message: message.into(),
            rule_id: None,
        }
    }

    /// Set the rule ID that caused this error.
    pub fn with_rule_id(mut self, rule_id: impl Into<String>) -> Self {
        match &mut self {
            Self::PartitionDrop { rule_id: r, .. } => *r = Some(rule_id.into()),
            Self::PartitionTimeout { rule_id: r, .. } => *r = Some(rule_id.into()),
            Self::PartitionError { rule_id: r, .. } => *r = Some(rule_id.into()),
            _ => {}
        }
        self
    }

    /// Check if this error should abort the request.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::PartitionDrop { .. }
                | Self::PartitionTimeout { .. }
                | Self::PartitionError { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_drop_error() {
        let err = PlaygroundError::partition_drop("node-a", "node-b");
        assert!(err.is_fatal());
        assert!(err.to_string().contains("partition"));
    }

    #[test]
    fn test_partition_timeout_error() {
        let err = PlaygroundError::partition_timeout("node-a", "node-b", Duration::from_secs(5));
        assert!(err.is_fatal());
        assert!(err.to_string().contains("5s"));
    }

    #[test]
    fn test_with_rule_id() {
        let err =
            PlaygroundError::partition_drop("a", "b").with_rule_id("rule-123");

        if let PlaygroundError::PartitionDrop { rule_id, .. } = err {
            assert_eq!(rule_id, Some("rule-123".to_string()));
        } else {
            panic!("Expected PartitionDrop");
        }
    }

    #[test]
    fn test_non_fatal_errors() {
        let err = PlaygroundError::NotEnabled;
        assert!(!err.is_fatal());

        let err = PlaygroundError::TelemetryBufferFull;
        assert!(!err.is_fatal());
    }
}
