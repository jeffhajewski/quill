//! Middleware implementations for Quill server
//!
//! This module provides middleware for:
//! - Compression (zstd)
//! - Decompression of incoming requests
//! - Content negotiation
//! - OpenTelemetry tracing

use bytes::Bytes;
use http::{header, Request, Response};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use quill_core::QuillError;
use tracing::{span, Level, Span};
use std::collections::HashMap;

/// Compression level for zstd
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Minimum body size to compress (in bytes)
pub const MIN_COMPRESS_SIZE: usize = 1024; // 1KB

/// Check if the client accepts zstd compression
pub fn accepts_zstd(req: &Request<Incoming>) -> bool {
    req.headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("zstd"))
        .unwrap_or(false)
}

/// Compress bytes using zstd
pub fn compress_zstd(data: &[u8], level: i32) -> Result<Bytes, QuillError> {
    zstd::encode_all(data, level)
        .map(Bytes::from)
        .map_err(|e| QuillError::Transport(format!("Compression failed: {}", e)))
}

/// Decompress bytes using zstd
pub fn decompress_zstd(data: &[u8]) -> Result<Bytes, QuillError> {
    zstd::decode_all(data)
        .map(Bytes::from)
        .map_err(|e| QuillError::Transport(format!("Decompression failed: {}", e)))
}

/// Decompress request body if it's compressed
///
/// Returns the request parts and the decompressed body bytes
pub async fn decompress_request_body(
    req: Request<Incoming>,
) -> Result<(http::request::Parts, Bytes), QuillError> {
    let (parts, body) = req.into_parts();

    // Read body
    let body_bytes = body
        .collect()
        .await
        .map_err(|e| QuillError::Transport(format!("Failed to read request body: {}", e)))?
        .to_bytes();

    // Check if compressed
    let decompressed = if let Some(encoding) = parts.headers.get(header::CONTENT_ENCODING) {
        if encoding == "zstd" {
            decompress_zstd(&body_bytes)?
        } else {
            body_bytes
        }
    } else {
        body_bytes
    };

    Ok((parts, decompressed))
}

/// Compress response body if appropriate
///
/// Note: This is a placeholder for future implementation.
/// Compressing streaming responses requires a compression stream adapter.
pub fn compress_response<B>(
    response: Response<B>,
    _accept_zstd: bool,
) -> Response<B>
where
    B: http_body::Body<Data = Bytes, Error = QuillError> + Send + 'static,
{
    // For now, we'll return the response as-is
    // In a real implementation, we would:
    // 1. Check if body is large enough to compress
    // 2. Compress the body
    // 3. Add Content-Encoding header
    // 4. Return compressed response
    //
    // This is tricky because we need to consume the body to compress it,
    // but we want to stream responses. For streaming responses, we'd need
    // a compression stream adapter.
    response
}

/// Middleware layer for compression
pub struct CompressionLayer {
    level: i32,
}

impl CompressionLayer {
    pub fn new() -> Self {
        Self {
            level: DEFAULT_COMPRESSION_LEVEL,
        }
    }

    pub fn with_level(level: i32) -> Self {
        Self { level }
    }

    pub fn level(&self) -> i32 {
        self.level
    }
}

impl Default for CompressionLayer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// OpenTelemetry Tracing
// ============================================================================

/// Create a tracing span for an RPC request
///
/// This creates a span with the RPC service and method as attributes,
/// following OpenTelemetry semantic conventions for RPC systems.
pub fn create_rpc_span(service: &str, method: &str) -> Span {
    span!(
        Level::INFO,
        "rpc.request",
        rpc.service = service,
        rpc.method = method,
        rpc.system = "quill",
        otel.kind = "server",
    )
}

/// Extract trace context from HTTP headers
///
/// This extracts distributed tracing context (traceparent, tracestate)
/// from HTTP headers following W3C Trace Context specification.
pub fn extract_trace_context(req: &Request<Incoming>) -> HashMap<String, String> {
    let mut context = HashMap::new();

    // Extract traceparent header (W3C Trace Context)
    if let Some(traceparent) = req.headers().get("traceparent") {
        if let Ok(value) = traceparent.to_str() {
            context.insert("traceparent".to_string(), value.to_string());
        }
    }

    // Extract tracestate header
    if let Some(tracestate) = req.headers().get("tracestate") {
        if let Ok(value) = tracestate.to_str() {
            context.insert("tracestate".to_string(), value.to_string());
        }
    }

    // Extract baggage header (for cross-cutting concerns)
    if let Some(baggage) = req.headers().get("baggage") {
        if let Ok(value) = baggage.to_str() {
            context.insert("baggage".to_string(), value.to_string());
        }
    }

    context
}

/// Record common RPC attributes on a span
pub fn record_rpc_attributes(span: &Span, service: &str, method: &str, compressed: bool) {
    span.record("rpc.service", service);
    span.record("rpc.method", method);
    span.record("rpc.system", "quill");
    if compressed {
        span.record("rpc.compression", "zstd");
    }
}

/// Record the RPC result on a span
pub fn record_rpc_result(span: &Span, success: bool, error: Option<&str>) {
    if success {
        span.record("rpc.status", "ok");
    } else {
        span.record("rpc.status", "error");
        if let Some(err) = error {
            span.record("rpc.error", err);
        }
    }
}

/// Tracing middleware layer
pub struct TracingLayer {
    enabled: bool,
}

impl TracingLayer {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    pub fn disabled() -> Self {
        Self { enabled: false }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for TracingLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zstd_roundtrip() {
        // Use a larger message with repetition for good compression
        let original = b"Hello, world! This is a test message. ".repeat(10);
        let compressed = compress_zstd(&original, 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();

        assert_eq!(original, &decompressed[..]);
        // With repetition, compression should be effective
        assert!(compressed.len() < original.len());
    }

    #[test]
    fn test_compress_empty() {
        let original = b"";
        let compressed = compress_zstd(original, 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();

        assert_eq!(original, &decompressed[..]);
    }

    #[test]
    fn test_compress_large() {
        // Create a large repeating pattern (should compress well)
        let original = vec![b'a'; 10000];
        let compressed = compress_zstd(&original, 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();

        assert_eq!(original, &decompressed[..]);
        // Should achieve good compression on repeating data
        assert!(compressed.len() < original.len() / 10);
    }

    #[test]
    fn test_create_rpc_span() {
        // Create a span - just verify it doesn't panic
        let _span = create_rpc_span("echo.v1.EchoService", "Echo");
        // Metadata might not be available without an active subscriber
        // The important part is that the span is created successfully
    }

    #[test]
    fn test_tracing_layer() {
        let layer = TracingLayer::new();
        assert!(layer.is_enabled());

        let disabled = TracingLayer::disabled();
        assert!(!disabled.is_enabled());
    }
}
