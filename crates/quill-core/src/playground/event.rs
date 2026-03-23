//! Telemetry events for playground dashboard visualization.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Events emitted by the playground system for visualization.
///
/// These events are sent to the dashboard via WebSocket for real-time
/// distributed systems visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaygroundEvent {
    /// RPC request sent from client
    RpcSend {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Request body as JSON (if ToDebugJson is implemented)
        request_body: Option<serde_json::Value>,
        /// Request size in bytes
        request_size: usize,
    },

    /// RPC request received by server
    RpcRecv {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Request body as JSON (if ToDebugJson is implemented)
        request_body: Option<serde_json::Value>,
        /// Request size in bytes
        request_size: usize,
    },

    /// RPC response sent from server
    RpcResponseSend {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Response body as JSON (if ToDebugJson is implemented)
        response_body: Option<serde_json::Value>,
        /// Response size in bytes
        response_size: usize,
        /// Whether the response was an error
        is_error: bool,
        /// HTTP status code
        status_code: u16,
    },

    /// RPC response received by client
    RpcResponseRecv {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Response body as JSON (if ToDebugJson is implemented)
        response_body: Option<serde_json::Value>,
        /// Response size in bytes
        response_size: usize,
        /// Whether the response was an error
        is_error: bool,
        /// HTTP status code
        status_code: u16,
        /// Round-trip latency
        #[serde(with = "duration_millis")]
        latency: Duration,
    },

    /// Stream started
    StreamStart {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Stream direction
        direction: String,
    },

    /// Stream message sent
    StreamMsgSend {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Message sequence number within the stream
        sequence: u64,
        /// Message body as JSON (if ToDebugJson is implemented)
        message_body: Option<serde_json::Value>,
        /// Message size in bytes
        message_size: usize,
    },

    /// Stream message received
    StreamMsgRecv {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Message sequence number within the stream
        sequence: u64,
        /// Message body as JSON (if ToDebugJson is implemented)
        message_body: Option<serde_json::Value>,
        /// Message size in bytes
        message_size: usize,
    },

    /// Stream ended normally
    StreamEnd {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Total messages sent
        messages_sent: u64,
        /// Total messages received
        messages_received: u64,
        /// Total stream duration
        #[serde(with = "duration_millis")]
        duration: Duration,
    },

    /// Stream cancelled
    StreamCancel {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Reason for cancellation
        reason: Option<String>,
        /// Who initiated the cancellation
        initiated_by: CancelInitiator,
    },

    /// Stream error occurred
    StreamError {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Error message
        error: String,
        /// Error code if available
        error_code: Option<String>,
    },

    /// Latency was injected
    LatencyInjected {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Base latency that was injected
        #[serde(with = "duration_millis")]
        base_latency: Duration,
        /// Jitter that was added
        #[serde(with = "option_duration_millis", default)]
        jitter: Option<Duration>,
        /// Total latency (base + jitter)
        #[serde(with = "duration_millis")]
        total_latency: Duration,
        /// Rule that triggered this injection
        rule_id: Option<String>,
    },

    /// Request was dropped due to partition
    PartitionDrop {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Rule that triggered the drop
        rule_id: Option<String>,
    },

    /// Request timed out due to partition
    PartitionTimeout {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Timeout duration
        #[serde(with = "duration_millis")]
        timeout: Duration,
        /// Rule that triggered the timeout
        rule_id: Option<String>,
    },

    /// Credit flow control event
    CreditGranted {
        /// Common event metadata
        #[serde(flatten)]
        metadata: EventMetadata,
        /// Credits granted
        credits: u64,
        /// Current credit balance
        balance: u64,
    },

    /// Node heartbeat for liveness tracking
    Heartbeat {
        /// Node identifier
        node_id: String,
        /// Timestamp
        timestamp_ms: u64,
        /// Node status
        status: NodeStatus,
    },
}

/// Common metadata included in all events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Timestamp when the event occurred (Unix millis)
    pub timestamp_ms: u64,
    /// Node that generated the event
    pub node_id: String,
    /// Trace ID for correlation
    pub trace_id: Option<String>,
    /// Span ID
    pub span_id: Option<String>,
    /// Service name
    pub service: String,
    /// Method name
    pub method: String,
}

impl EventMetadata {
    /// Create new event metadata.
    pub fn new(
        node_id: impl Into<String>,
        service: impl Into<String>,
        method: impl Into<String>,
    ) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            timestamp_ms,
            node_id: node_id.into(),
            trace_id: None,
            span_id: None,
            service: service.into(),
            method: method.into(),
        }
    }

    /// Set trace context.
    pub fn with_trace_context(mut self, trace_id: Option<String>, span_id: Option<String>) -> Self {
        self.trace_id = trace_id;
        self.span_id = span_id;
        self
    }
}

/// Who initiated a stream cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CancelInitiator {
    /// Client initiated the cancellation
    Client,
    /// Server initiated the cancellation
    Server,
    /// Cancellation due to timeout
    Timeout,
    /// Unknown initiator
    Unknown,
}

/// Node status for heartbeat events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    /// Node is healthy
    Healthy,
    /// Node is degraded but operational
    Degraded,
    /// Node is unhealthy
    Unhealthy,
}

impl PlaygroundEvent {
    /// Create an RPC send event.
    pub fn rpc_send(
        metadata: EventMetadata,
        request_body: Option<serde_json::Value>,
        request_size: usize,
    ) -> Self {
        Self::RpcSend {
            metadata,
            request_body,
            request_size,
        }
    }

    /// Create an RPC receive event.
    pub fn rpc_recv(
        metadata: EventMetadata,
        request_body: Option<serde_json::Value>,
        request_size: usize,
    ) -> Self {
        Self::RpcRecv {
            metadata,
            request_body,
            request_size,
        }
    }

    /// Create a heartbeat event.
    pub fn heartbeat(node_id: impl Into<String>, status: NodeStatus) -> Self {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self::Heartbeat {
            node_id: node_id.into(),
            timestamp_ms,
            status,
        }
    }

    /// Get the event type as a string.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::RpcSend { .. } => "RPC_SEND",
            Self::RpcRecv { .. } => "RPC_RECV",
            Self::RpcResponseSend { .. } => "RPC_RESPONSE_SEND",
            Self::RpcResponseRecv { .. } => "RPC_RESPONSE_RECV",
            Self::StreamStart { .. } => "STREAM_START",
            Self::StreamMsgSend { .. } => "STREAM_MSG_SEND",
            Self::StreamMsgRecv { .. } => "STREAM_MSG_RECV",
            Self::StreamEnd { .. } => "STREAM_END",
            Self::StreamCancel { .. } => "STREAM_CANCEL",
            Self::StreamError { .. } => "STREAM_ERROR",
            Self::LatencyInjected { .. } => "LATENCY_INJECTED",
            Self::PartitionDrop { .. } => "PARTITION_DROP",
            Self::PartitionTimeout { .. } => "PARTITION_TIMEOUT",
            Self::CreditGranted { .. } => "CREDIT_GRANTED",
            Self::Heartbeat { .. } => "HEARTBEAT",
        }
    }

    /// Get the node ID from the event.
    pub fn node_id(&self) -> &str {
        match self {
            Self::RpcSend { metadata, .. }
            | Self::RpcRecv { metadata, .. }
            | Self::RpcResponseSend { metadata, .. }
            | Self::RpcResponseRecv { metadata, .. }
            | Self::StreamStart { metadata, .. }
            | Self::StreamMsgSend { metadata, .. }
            | Self::StreamMsgRecv { metadata, .. }
            | Self::StreamEnd { metadata, .. }
            | Self::StreamCancel { metadata, .. }
            | Self::StreamError { metadata, .. }
            | Self::LatencyInjected { metadata, .. }
            | Self::PartitionDrop { metadata, .. }
            | Self::PartitionTimeout { metadata, .. }
            | Self::CreditGranted { metadata, .. } => &metadata.node_id,
            Self::Heartbeat { node_id, .. } => node_id,
        }
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
    fn test_event_metadata_new() {
        let meta = EventMetadata::new("node-1", "my.Service", "MyMethod");
        assert_eq!(meta.node_id, "node-1");
        assert_eq!(meta.service, "my.Service");
        assert_eq!(meta.method, "MyMethod");
        assert!(meta.timestamp_ms > 0);
    }

    #[test]
    fn test_event_metadata_with_trace() {
        let meta = EventMetadata::new("node-1", "svc", "method")
            .with_trace_context(Some("trace-123".into()), Some("span-456".into()));
        assert_eq!(meta.trace_id, Some("trace-123".to_string()));
        assert_eq!(meta.span_id, Some("span-456".to_string()));
    }

    #[test]
    fn test_rpc_send_event() {
        let meta = EventMetadata::new("node-1", "svc", "method");
        let event = PlaygroundEvent::rpc_send(meta, None, 1024);

        assert_eq!(event.event_type(), "RPC_SEND");
        assert_eq!(event.node_id(), "node-1");
    }

    #[test]
    fn test_heartbeat_event() {
        let event = PlaygroundEvent::heartbeat("node-1", NodeStatus::Healthy);

        assert_eq!(event.event_type(), "HEARTBEAT");
        assert_eq!(event.node_id(), "node-1");
    }

    #[test]
    fn test_event_serialization() {
        let meta = EventMetadata::new("node-1", "test.Service", "TestMethod");
        let event = PlaygroundEvent::rpc_send(meta, Some(serde_json::json!({"key": "value"})), 256);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"RPC_SEND\""));
        assert!(json.contains("\"node_id\":\"node-1\""));
        assert!(json.contains("\"request_size\":256"));

        // Verify deserialization
        let parsed: PlaygroundEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type(), "RPC_SEND");
    }

    #[test]
    fn test_latency_event_serialization() {
        let meta = EventMetadata::new("node-1", "svc", "method");
        let event = PlaygroundEvent::LatencyInjected {
            metadata: meta,
            base_latency: Duration::from_millis(100),
            jitter: Some(Duration::from_millis(10)),
            total_latency: Duration::from_millis(105),
            rule_id: Some("rule-1".into()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"base_latency\":100"));
        assert!(json.contains("\"jitter\":10"));
        assert!(json.contains("\"total_latency\":105"));
    }

    #[test]
    fn test_stream_events() {
        let meta = EventMetadata::new("node-1", "svc", "method");

        let start = PlaygroundEvent::StreamStart {
            metadata: meta.clone(),
            direction: "bidirectional".into(),
        };
        assert_eq!(start.event_type(), "STREAM_START");

        let msg = PlaygroundEvent::StreamMsgSend {
            metadata: meta.clone(),
            sequence: 1,
            message_body: None,
            message_size: 64,
        };
        assert_eq!(msg.event_type(), "STREAM_MSG_SEND");

        let end = PlaygroundEvent::StreamEnd {
            metadata: meta,
            messages_sent: 10,
            messages_received: 5,
            duration: Duration::from_secs(5),
        };
        assert_eq!(end.event_type(), "STREAM_END");
    }

    #[test]
    fn test_cancel_initiator_serialization() {
        assert_eq!(
            serde_json::to_string(&CancelInitiator::Client).unwrap(),
            "\"client\""
        );
        assert_eq!(
            serde_json::to_string(&CancelInitiator::Server).unwrap(),
            "\"server\""
        );
    }

    #[test]
    fn test_node_status_serialization() {
        assert_eq!(
            serde_json::to_string(&NodeStatus::Healthy).unwrap(),
            "\"healthy\""
        );
        assert_eq!(
            serde_json::to_string(&NodeStatus::Degraded).unwrap(),
            "\"degraded\""
        );
    }
}
