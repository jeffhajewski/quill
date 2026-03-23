//! Intercept context for request metadata.

use std::collections::HashMap;
use std::time::Instant;

/// Context passed through the interceptor chain for each RPC call.
///
/// Contains metadata about the request that interceptors can use
/// to make decisions (e.g., latency injection, partition simulation).
#[derive(Debug, Clone)]
pub struct InterceptContext {
    /// Unique trace ID for distributed tracing
    pub trace_id: Option<String>,
    /// Span ID for this specific call
    pub span_id: Option<String>,
    /// Parent span ID if this is a child span
    pub parent_span_id: Option<String>,
    /// Service name being called (e.g., "greeter.v1.GreeterService")
    pub service_name: String,
    /// Method name being called (e.g., "SayHello")
    pub method_name: String,
    /// Source node identifier (for partition rules)
    pub source_node: Option<String>,
    /// Destination node identifier (for partition rules)
    pub destination_node: Option<String>,
    /// Whether this is a streaming call
    pub is_streaming: bool,
    /// Stream direction if streaming
    pub stream_direction: Option<StreamDirection>,
    /// Custom metadata/attributes
    pub attributes: HashMap<String, String>,
    /// When the request started (for latency tracking)
    pub started_at: Instant,
    /// Request size in bytes (if known)
    pub request_size: Option<usize>,
    /// Whether the method is marked as idempotent
    pub is_idempotent: bool,
    /// Whether the method is marked as real-time
    pub is_real_time: bool,
}

impl InterceptContext {
    /// Create a new context for a unary RPC call.
    pub fn new(service_name: impl Into<String>, method_name: impl Into<String>) -> Self {
        Self {
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            service_name: service_name.into(),
            method_name: method_name.into(),
            source_node: None,
            destination_node: None,
            is_streaming: false,
            stream_direction: None,
            attributes: HashMap::new(),
            started_at: Instant::now(),
            request_size: None,
            is_idempotent: false,
            is_real_time: false,
        }
    }

    /// Create a new context for a streaming RPC call.
    pub fn streaming(
        service_name: impl Into<String>,
        method_name: impl Into<String>,
        direction: StreamDirection,
    ) -> Self {
        Self {
            trace_id: None,
            span_id: None,
            parent_span_id: None,
            service_name: service_name.into(),
            method_name: method_name.into(),
            source_node: None,
            destination_node: None,
            is_streaming: true,
            stream_direction: Some(direction),
            attributes: HashMap::new(),
            started_at: Instant::now(),
            request_size: None,
            is_idempotent: false,
            is_real_time: false,
        }
    }

    /// Set trace context from headers.
    pub fn with_trace_context(
        mut self,
        trace_id: Option<String>,
        span_id: Option<String>,
        parent_span_id: Option<String>,
    ) -> Self {
        self.trace_id = trace_id;
        self.span_id = span_id;
        self.parent_span_id = parent_span_id;
        self
    }

    /// Set source node.
    pub fn with_source_node(mut self, node: impl Into<String>) -> Self {
        self.source_node = Some(node.into());
        self
    }

    /// Set destination node.
    pub fn with_destination_node(mut self, node: impl Into<String>) -> Self {
        self.destination_node = Some(node.into());
        self
    }

    /// Set request size.
    pub fn with_request_size(mut self, size: usize) -> Self {
        self.request_size = Some(size);
        self
    }

    /// Mark method as idempotent.
    pub fn with_idempotent(mut self, idempotent: bool) -> Self {
        self.is_idempotent = idempotent;
        self
    }

    /// Mark method as real-time.
    pub fn with_real_time(mut self, real_time: bool) -> Self {
        self.is_real_time = real_time;
        self
    }

    /// Add a custom attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Get elapsed time since the request started.
    pub fn elapsed(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Get the full method path (service/method).
    pub fn full_method(&self) -> String {
        format!("{}/{}", self.service_name, self.method_name)
    }
}

/// Direction of a streaming RPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDirection {
    /// Server streams responses to client (client sends one request).
    ServerStreaming,
    /// Client streams requests to server (server sends one response).
    ClientStreaming,
    /// Both client and server stream messages.
    Bidirectional,
}

impl StreamDirection {
    /// Check if the client sends multiple messages.
    pub fn client_streams(&self) -> bool {
        matches!(self, Self::ClientStreaming | Self::Bidirectional)
    }

    /// Check if the server sends multiple messages.
    pub fn server_streams(&self) -> bool {
        matches!(self, Self::ServerStreaming | Self::Bidirectional)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intercept_context_new() {
        let ctx = InterceptContext::new("my.service.v1.MyService", "GetThing");
        assert_eq!(ctx.service_name, "my.service.v1.MyService");
        assert_eq!(ctx.method_name, "GetThing");
        assert!(!ctx.is_streaming);
        assert!(ctx.stream_direction.is_none());
    }

    #[test]
    fn test_intercept_context_streaming() {
        let ctx = InterceptContext::streaming(
            "chat.v1.ChatService",
            "StreamMessages",
            StreamDirection::Bidirectional,
        );
        assert!(ctx.is_streaming);
        assert_eq!(ctx.stream_direction, Some(StreamDirection::Bidirectional));
    }

    #[test]
    fn test_intercept_context_builder() {
        let ctx = InterceptContext::new("svc", "method")
            .with_trace_context(
                Some("trace-123".into()),
                Some("span-456".into()),
                Some("parent-789".into()),
            )
            .with_source_node("node-a")
            .with_destination_node("node-b")
            .with_request_size(1024)
            .with_idempotent(true)
            .with_real_time(false)
            .with_attribute("user_id", "user-123");

        assert_eq!(ctx.trace_id, Some("trace-123".to_string()));
        assert_eq!(ctx.span_id, Some("span-456".to_string()));
        assert_eq!(ctx.parent_span_id, Some("parent-789".to_string()));
        assert_eq!(ctx.source_node, Some("node-a".to_string()));
        assert_eq!(ctx.destination_node, Some("node-b".to_string()));
        assert_eq!(ctx.request_size, Some(1024));
        assert!(ctx.is_idempotent);
        assert!(!ctx.is_real_time);
        assert_eq!(ctx.attributes.get("user_id"), Some(&"user-123".to_string()));
    }

    #[test]
    fn test_full_method() {
        let ctx = InterceptContext::new("foo.bar.BazService", "DoStuff");
        assert_eq!(ctx.full_method(), "foo.bar.BazService/DoStuff");
    }

    #[test]
    fn test_stream_direction() {
        assert!(StreamDirection::ClientStreaming.client_streams());
        assert!(!StreamDirection::ClientStreaming.server_streams());

        assert!(!StreamDirection::ServerStreaming.client_streams());
        assert!(StreamDirection::ServerStreaming.server_streams());

        assert!(StreamDirection::Bidirectional.client_streams());
        assert!(StreamDirection::Bidirectional.server_streams());
    }
}
