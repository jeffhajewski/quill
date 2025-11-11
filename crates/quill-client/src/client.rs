//! Quill client implementation

use bytes::Bytes;
use http::{Method, Request};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use quill_core::{CreditTracker, FrameParser, ProfilePreference, QuillError};
use crate::retry::{CircuitBreaker, RetryPolicy};
use crate::streaming::encode_request_stream;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::Stream;
use tracing::instrument;

/// HTTP protocol version preference
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpProtocol {
    /// HTTP/1.1 only
    Http1,
    /// HTTP/2 only
    Http2,
    /// Automatically negotiate (default)
    Auto,
}

impl Default for HttpProtocol {
    fn default() -> Self {
        Self::Auto
    }
}

/// Client configuration
#[derive(Clone)]
pub struct ClientConfig {
    /// HTTP protocol version
    pub http_protocol: HttpProtocol,
    /// Connection pool idle timeout
    pub pool_idle_timeout: Option<Duration>,
    /// Max idle connections per host
    pub pool_max_idle_per_host: usize,
    /// HTTP/2 only: enable HTTP/2 adaptive window
    pub http2_adaptive_window: bool,
    /// HTTP/2 only: initial connection window size
    pub http2_initial_connection_window_size: Option<u32>,
    /// HTTP/2 only: initial stream window size
    pub http2_initial_stream_window_size: Option<u32>,
    /// HTTP/2 only: max concurrent streams
    pub http2_max_concurrent_streams: Option<usize>,
    /// HTTP/2 only: keep alive interval
    pub http2_keep_alive_interval: Option<Duration>,
    /// HTTP/2 only: keep alive timeout
    pub http2_keep_alive_timeout: Option<Duration>,
    /// Retry policy (None = no retries)
    pub retry_policy: Option<RetryPolicy>,
    /// Circuit breaker (None = no circuit breaking)
    pub circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientConfig")
            .field("http_protocol", &self.http_protocol)
            .field("pool_idle_timeout", &self.pool_idle_timeout)
            .field("pool_max_idle_per_host", &self.pool_max_idle_per_host)
            .field("http2_adaptive_window", &self.http2_adaptive_window)
            .field("retry_policy", &self.retry_policy.as_ref().map(|_| "<RetryPolicy>"))
            .field("circuit_breaker", &self.circuit_breaker.as_ref().map(|_| "<CircuitBreaker>"))
            .finish()
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            http_protocol: HttpProtocol::Auto,
            pool_idle_timeout: Some(Duration::from_secs(90)),
            pool_max_idle_per_host: 32,
            http2_adaptive_window: true,
            http2_initial_connection_window_size: Some(1024 * 1024), // 1MB
            http2_initial_stream_window_size: Some(1024 * 1024),     // 1MB
            http2_max_concurrent_streams: Some(100),
            http2_keep_alive_interval: Some(Duration::from_secs(10)),
            http2_keep_alive_timeout: Some(Duration::from_secs(20)),
            retry_policy: None,
            circuit_breaker: None,
        }
    }
}

/// Quill RPC client
pub struct QuillClient {
    base_url: String,
    client: Client<HttpConnector, Full<Bytes>>,
    profile_preference: ProfilePreference,
    enable_compression: bool,
    compression_level: i32,
    config: ClientConfig,
}

impl QuillClient {
    /// Create a new client with the given base URL
    pub fn new(base_url: impl Into<String>) -> Self {
        let config = ClientConfig::default();
        let client = Self::build_client(&config);

        Self {
            base_url: base_url.into(),
            client,
            profile_preference: ProfilePreference::default_preference(),
            enable_compression: false,
            compression_level: 3,
            config,
        }
    }

    /// Create a new client with custom configuration
    pub fn with_config(base_url: impl Into<String>, config: ClientConfig) -> Self {
        let client = Self::build_client(&config);

        Self {
            base_url: base_url.into(),
            client,
            profile_preference: ProfilePreference::default_preference(),
            enable_compression: false,
            compression_level: 3,
            config,
        }
    }

    /// Build an HTTP client based on configuration
    fn build_client(config: &ClientConfig) -> Client<HttpConnector, Full<Bytes>> {
        let mut builder = Client::builder(TokioExecutor::new());

        // Configure connection pool
        builder.pool_idle_timeout(config.pool_idle_timeout.unwrap_or(Duration::from_secs(90)));
        builder.pool_max_idle_per_host(config.pool_max_idle_per_host);

        // Configure HTTP protocol
        match config.http_protocol {
            HttpProtocol::Http1 => {
                builder.http2_only(false);
            }
            HttpProtocol::Http2 => {
                builder.http2_only(true);

                // Configure HTTP/2 settings
                if config.http2_adaptive_window {
                    builder.http2_adaptive_window(true);
                }
                if let Some(size) = config.http2_initial_connection_window_size {
                    builder.http2_initial_connection_window_size(size);
                }
                if let Some(size) = config.http2_initial_stream_window_size {
                    builder.http2_initial_stream_window_size(size);
                }
                if let Some(max) = config.http2_max_concurrent_streams {
                    builder.http2_max_concurrent_reset_streams(max);
                }
                if let Some(interval) = config.http2_keep_alive_interval {
                    builder.http2_keep_alive_interval(interval);
                }
                if let Some(timeout) = config.http2_keep_alive_timeout {
                    builder.http2_keep_alive_timeout(timeout);
                }
            }
            HttpProtocol::Auto => {
                // Auto-negotiate, use HTTP/2 if available
                builder.http2_adaptive_window(config.http2_adaptive_window);

                if let Some(size) = config.http2_initial_connection_window_size {
                    builder.http2_initial_connection_window_size(size);
                }
                if let Some(size) = config.http2_initial_stream_window_size {
                    builder.http2_initial_stream_window_size(size);
                }
                if let Some(interval) = config.http2_keep_alive_interval {
                    builder.http2_keep_alive_interval(interval);
                }
                if let Some(timeout) = config.http2_keep_alive_timeout {
                    builder.http2_keep_alive_timeout(timeout);
                }
            }
        }

        builder.build_http()
    }

    /// Create a builder for configuring the client
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Compress data using zstd if compression is enabled
    fn maybe_compress(&self, data: Bytes) -> Result<Bytes, QuillError> {
        if !self.enable_compression {
            return Ok(data);
        }

        zstd::encode_all(&data[..], self.compression_level)
            .map(Bytes::from)
            .map_err(|e| QuillError::Transport(format!("Compression failed: {}", e)))
    }

    /// Decompress data using zstd if it was compressed
    fn maybe_decompress(&self, data: Bytes, content_encoding: Option<&str>) -> Result<Bytes, QuillError> {
        if let Some("zstd") = content_encoding {
            zstd::decode_all(&data[..])
                .map(Bytes::from)
                .map_err(|e| QuillError::Transport(format!("Decompression failed: {}", e)))
        } else {
            Ok(data)
        }
    }

    /// Execute an operation with retry and circuit breaker logic
    async fn with_resilience<F, Fut, T>(&self, operation: F) -> Result<T, QuillError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, QuillError>>,
    {
        // Check circuit breaker first
        if let Some(breaker) = &self.config.circuit_breaker {
            breaker.allow_request().await?;
        }

        // Execute with retry if configured
        let result = if let Some(policy) = &self.config.retry_policy {
            crate::retry::retry_with_policy(policy, operation).await
        } else {
            operation().await
        };

        // Record result in circuit breaker
        if let Some(breaker) = &self.config.circuit_breaker {
            match &result {
                Ok(_) => breaker.record_success().await,
                Err(_) => breaker.record_failure().await,
            }
        }

        result
    }

    /// Make a unary RPC call
    ///
    /// # Arguments
    /// * `service` - The service path (e.g., "echo.v1.EchoService")
    /// * `method` - The method name (e.g., "Echo")
    /// * `request` - The protobuf-encoded request bytes
    ///
    /// # Returns
    /// The protobuf-encoded response bytes
    #[instrument(
        skip(self, request),
        fields(
            rpc.service = service,
            rpc.method = method,
            rpc.system = "quill",
            otel.kind = "client"
        )
    )]
    pub async fn call(
        &self,
        service: &str,
        method: &str,
        request: Bytes,
    ) -> Result<Bytes, QuillError> {
        // Build the full URL
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Compress request if enabled
        let (request_body, content_encoding) = if self.enable_compression {
            let compressed = self.maybe_compress(request)?;
            (compressed, Some("zstd"))
        } else {
            (request, None)
        };

        // Build the HTTP request
        let mut req_builder = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/proto")
            .header("Accept", "application/proto")
            .header("Prefer", self.profile_preference.to_header_value());

        // Add compression headers if enabled
        if self.enable_compression {
            req_builder = req_builder.header("Accept-Encoding", "zstd");
        }
        if let Some(encoding) = content_encoding {
            req_builder = req_builder.header("Content-Encoding", encoding);
        }

        let req = req_builder
            .body(Full::new(request_body))
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request
        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to send request: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            // Try to parse Problem Details
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| QuillError::Transport(format!("Failed to read error response: {}", e)))?
                .to_bytes();

            // Try to parse as JSON Problem Details
            if let Ok(pd) = serde_json::from_slice(&body_bytes) {
                return Err(QuillError::ProblemDetails(pd));
            }

            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        // Get content encoding before consuming response
        let content_encoding = resp
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Read response body
        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to read response: {}", e)))?
            .to_bytes();

        // Decompress if needed
        let body_bytes = self.maybe_decompress(body_bytes, content_encoding.as_deref())?;

        Ok(body_bytes)
    }

    /// Make a streaming RPC call (client streaming)
    ///
    /// # Arguments
    /// * `service` - The service path (e.g., "upload.v1.UploadService")
    /// * `method` - The method name (e.g., "Upload")
    /// * `request` - Stream of request messages
    ///
    /// # Returns
    /// The protobuf-encoded response bytes
    #[instrument(
        skip(self, request),
        fields(
            rpc.service = service,
            rpc.method = method,
            rpc.system = "quill",
            rpc.streaming = "client",
            otel.kind = "client"
        )
    )]
    pub async fn call_client_streaming(
        &self,
        service: &str,
        method: &str,
        request: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>,
    ) -> Result<Bytes, QuillError> {
        // Encode the stream into frames
        let encoded = encode_request_stream(request).await?;

        // Use regular call with encoded frames
        self.call(service, method, encoded).await
    }

    /// Receive a streaming response (server streaming)
    ///
    /// # Arguments
    /// * `service` - The service path
    /// * `method` - The method name
    /// * `request` - The protobuf-encoded request bytes
    ///
    /// # Returns
    /// A stream of response messages
    #[instrument(
        skip(self, request),
        fields(
            rpc.service = service,
            rpc.method = method,
            rpc.system = "quill",
            rpc.streaming = "server",
            otel.kind = "client"
        )
    )]
    pub async fn call_server_streaming(
        &self,
        service: &str,
        method: &str,
        request: Bytes,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>, QuillError> {
        // Build the full URL
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/proto")
            .header("Accept", "application/proto")
            .header("Prefer", self.profile_preference.to_header_value())
            .body(Full::new(request))
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request
        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to send request: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| QuillError::Transport(format!("Failed to read error response: {}", e)))?
                .to_bytes();

            if let Ok(pd) = serde_json::from_slice(&body_bytes) {
                return Err(QuillError::ProblemDetails(pd));
            }

            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        // Create a stream that parses frames from the response
        let body = resp.into_body();
        let frame_stream = ResponseFrameStream::new(body);

        Ok(Box::pin(frame_stream))
    }

    /// Make a bidirectional streaming RPC call
    ///
    /// # Arguments
    /// * `service` - The service path
    /// * `method` - The method name
    /// * `request` - Stream of request messages
    ///
    /// # Returns
    /// A stream of response messages
    #[instrument(
        skip(self, request),
        fields(
            rpc.service = service,
            rpc.method = method,
            rpc.system = "quill",
            rpc.streaming = "bidirectional",
            otel.kind = "client"
        )
    )]
    pub async fn call_bidi_streaming(
        &self,
        service: &str,
        method: &str,
        request: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>, QuillError> {
        // Build the full URL
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Encode the request stream into frames
        let encoded = encode_request_stream(request).await?;

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/proto")
            .header("Accept", "application/proto")
            .header("Prefer", self.profile_preference.to_header_value())
            .body(Full::new(encoded))
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request
        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to send request: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| QuillError::Transport(format!("Failed to read error response: {}", e)))?
                .to_bytes();

            if let Ok(pd) = serde_json::from_slice(&body_bytes) {
                return Err(QuillError::ProblemDetails(pd));
            }

            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        // Create a stream that parses frames from the response
        let body = resp.into_body();
        let frame_stream = ResponseFrameStream::new(body);

        Ok(Box::pin(frame_stream))
    }
}

/// Stream adapter that parses frames from HTTP response body
struct ResponseFrameStream {
    body: hyper::body::Incoming,
    parser: FrameParser,
    credits: CreditTracker,
    messages_received: u32,
}

impl ResponseFrameStream {
    fn new(body: hyper::body::Incoming) -> Self {
        Self {
            body,
            parser: FrameParser::new(),
            credits: CreditTracker::with_defaults(),
            messages_received: 0,
        }
    }
}

impl Stream for ResponseFrameStream {
    type Item = Result<Bytes, QuillError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
        use http_body::Body;
        use quill_core::DEFAULT_CREDIT_REFILL;

        loop {
            // Try to parse a frame from buffered data
            match self.parser.parse_frame() {
                Ok(Some(frame)) => {
                    if frame.flags.is_end_stream() {
                        // Stream ended
                        return Poll::Ready(None);
                    }
                    if frame.flags.is_credit() {
                        // Server is granting us credits to send more requests
                        // (Useful for true bidirectional streaming in the future)
                        if let Some(amount) = frame.decode_credit() {
                            self.credits.grant(amount);
                        }
                        // Continue to next frame
                        continue;
                    }
                    if frame.flags.is_data() {
                        self.messages_received += 1;

                        // In a future HTTP/2 implementation, we would send credit frames
                        // back to the server here to grant more send credits.
                        // For now, we just track locally.
                        if self.messages_received % DEFAULT_CREDIT_REFILL == 0 {
                            // Would send credit frame to server here
                            tracing::debug!(
                                "Would grant {} credits to server (received {} messages)",
                                DEFAULT_CREDIT_REFILL,
                                self.messages_received
                            );
                        }

                        return Poll::Ready(Some(Ok(frame.payload)));
                    }
                    if frame.flags.is_cancel() {
                        // Stream was cancelled by server
                        return Poll::Ready(Some(Err(QuillError::Rpc(
                            "Stream cancelled by server".to_string()
                        ))));
                    }
                    // Other frame types, continue
                }
                Ok(None) => {
                    // Need more data
                }
                Err(e) => {
                    return Poll::Ready(Some(Err(QuillError::Framing(e.to_string()))));
                }
            }

            // Read more data from body
            match Pin::new(&mut self.body).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    if let Ok(data) = frame.into_data() {
                        self.parser.feed(&data);
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(QuillError::Transport(e.to_string()))));
                }
                Poll::Ready(None) => {
                    // Body ended, but we might have buffered data
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

impl fmt::Debug for QuillClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuillClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Builder for configuring a Quill client
pub struct ClientBuilder {
    base_url: Option<String>,
    profile_preference: Option<ProfilePreference>,
    enable_compression: bool,
    compression_level: i32,
    config: ClientConfig,
}

impl ClientBuilder {
    /// Create a new client builder
    pub fn new() -> Self {
        Self {
            base_url: None,
            profile_preference: None,
            enable_compression: false,
            compression_level: 3,
            config: ClientConfig::default(),
        }
    }

    /// Set the base URL for the client
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set the profile preference
    pub fn profile_preference(mut self, pref: ProfilePreference) -> Self {
        self.profile_preference = Some(pref);
        self
    }

    /// Enable zstd compression for requests and responses
    pub fn enable_compression(mut self, enable: bool) -> Self {
        self.enable_compression = enable;
        self
    }

    /// Set the compression level (0-22, default 3)
    pub fn compression_level(mut self, level: i32) -> Self {
        self.compression_level = level;
        self
    }

    /// Set HTTP protocol version
    pub fn http_protocol(mut self, protocol: HttpProtocol) -> Self {
        self.config.http_protocol = protocol;
        self
    }

    /// Enable HTTP/2 only (Turbo profile)
    pub fn http2_only(self) -> Self {
        self.http_protocol(HttpProtocol::Http2)
    }

    /// Set connection pool idle timeout
    pub fn pool_idle_timeout(mut self, timeout: Duration) -> Self {
        self.config.pool_idle_timeout = Some(timeout);
        self
    }

    /// Set max idle connections per host
    pub fn pool_max_idle_per_host(mut self, max: usize) -> Self {
        self.config.pool_max_idle_per_host = max;
        self
    }

    /// Enable HTTP/2 adaptive window
    pub fn http2_adaptive_window(mut self, enable: bool) -> Self {
        self.config.http2_adaptive_window = enable;
        self
    }

    /// Set HTTP/2 initial connection window size
    pub fn http2_initial_connection_window_size(mut self, size: u32) -> Self {
        self.config.http2_initial_connection_window_size = Some(size);
        self
    }

    /// Set HTTP/2 initial stream window size
    pub fn http2_initial_stream_window_size(mut self, size: u32) -> Self {
        self.config.http2_initial_stream_window_size = Some(size);
        self
    }

    /// Set HTTP/2 max concurrent streams
    pub fn http2_max_concurrent_streams(mut self, max: usize) -> Self {
        self.config.http2_max_concurrent_streams = Some(max);
        self
    }

    /// Set HTTP/2 keep alive interval
    pub fn http2_keep_alive_interval(mut self, interval: Duration) -> Self {
        self.config.http2_keep_alive_interval = Some(interval);
        self
    }

    /// Set HTTP/2 keep alive timeout
    pub fn http2_keep_alive_timeout(mut self, timeout: Duration) -> Self {
        self.config.http2_keep_alive_timeout = Some(timeout);
        self
    }

    /// Enable retries with the given policy
    pub fn retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.config.retry_policy = Some(policy);
        self
    }

    /// Enable retries with default policy
    pub fn enable_retries(mut self) -> Self {
        self.config.retry_policy = Some(RetryPolicy::default());
        self
    }

    /// Enable circuit breaker with the given configuration
    pub fn circuit_breaker(mut self, config: crate::retry::CircuitBreakerConfig) -> Self {
        self.config.circuit_breaker = Some(Arc::new(CircuitBreaker::new(config)));
        self
    }

    /// Enable circuit breaker with default configuration
    pub fn enable_circuit_breaker(mut self) -> Self {
        self.config.circuit_breaker = Some(Arc::new(CircuitBreaker::new(
            crate::retry::CircuitBreakerConfig::default(),
        )));
        self
    }

    /// Build the client
    pub fn build(self) -> Result<QuillClient, String> {
        let base_url = self
            .base_url
            .ok_or_else(|| "base_url is required".to_string())?;

        let client = QuillClient::build_client(&self.config);

        Ok(QuillClient {
            base_url,
            client,
            profile_preference: self
                .profile_preference
                .unwrap_or_else(ProfilePreference::default_preference),
            enable_compression: self.enable_compression,
            compression_level: self.compression_level,
            config: self.config,
        })
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builder() {
        let client = QuillClient::builder()
            .base_url("http://localhost:8080")
            .build()
            .unwrap();

        assert_eq!(client.base_url, "http://localhost:8080");
    }
}
