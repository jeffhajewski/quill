//! HTTP/3 client for Quill RPC
//!
//! This module provides an HTTP/3 client for making Quill RPC calls over QUIC.
//! It uses the Hyper profile from quill-transport for the underlying HTTP/3 connection.

#[cfg(feature = "http3")]
use bytes::Bytes;
#[cfg(feature = "http3")]
use http::{Method, Request};
#[cfg(feature = "http3")]
use quill_core::{CreditTracker, FrameParser, ProfilePreference, QuillError};
#[cfg(feature = "http3")]
use std::fmt;
#[cfg(feature = "http3")]
use std::net::SocketAddr;
#[cfg(feature = "http3")]
use std::pin::Pin;
#[cfg(feature = "http3")]
use tokio_stream::Stream;
#[cfg(feature = "http3")]
use tracing::instrument;

#[cfg(feature = "http3")]
use crate::streaming::encode_request_stream;

/// HTTP/3 client configuration
#[cfg(feature = "http3")]
#[derive(Debug, Clone)]
pub struct H3ClientConfig {
    /// Enable 0-RTT for idempotent requests
    pub enable_zero_rtt: bool,
    /// Enable HTTP/3 datagrams
    pub enable_datagrams: bool,
    /// Enable connection migration
    pub enable_connection_migration: bool,
    /// Max concurrent streams
    pub max_concurrent_streams: u64,
    /// Idle timeout in milliseconds
    pub idle_timeout_ms: u64,
    /// Enable zstd compression
    pub enable_compression: bool,
    /// Compression level (0-22)
    pub compression_level: i32,
}

#[cfg(feature = "http3")]
impl Default for H3ClientConfig {
    fn default() -> Self {
        Self {
            enable_zero_rtt: false,
            enable_datagrams: true,
            enable_connection_migration: true,
            max_concurrent_streams: 100,
            idle_timeout_ms: 60000,
            enable_compression: false,
            compression_level: 3,
        }
    }
}

/// Quill RPC client using HTTP/3 transport
#[cfg(feature = "http3")]
pub struct QuillH3Client {
    server_addr: SocketAddr,
    client: quill_transport::H3Client,
    profile_preference: ProfilePreference,
    config: H3ClientConfig,
}

#[cfg(feature = "http3")]
impl QuillH3Client {
    /// Create a new HTTP/3 client
    pub fn new(server_addr: SocketAddr) -> Result<Self, QuillError> {
        let config = H3ClientConfig::default();
        Self::with_config(server_addr, config)
    }

    /// Create a new HTTP/3 client with custom configuration
    pub fn with_config(server_addr: SocketAddr, config: H3ClientConfig) -> Result<Self, QuillError> {
        let transport_config = quill_transport::HyperConfig {
            enable_zero_rtt: config.enable_zero_rtt,
            enable_datagrams: config.enable_datagrams,
            enable_connection_migration: config.enable_connection_migration,
            max_concurrent_streams: config.max_concurrent_streams,
            max_datagram_size: 65536,
            keep_alive_interval_ms: 30000,
            idle_timeout_ms: config.idle_timeout_ms,
        };

        let client = quill_transport::H3Client::new(transport_config)
            .map_err(|e| QuillError::Transport(format!("Failed to create HTTP/3 client: {}", e)))?;

        Ok(Self {
            server_addr,
            client,
            profile_preference: ProfilePreference::default_preference(),
            config,
        })
    }

    /// Create a builder for configuring the HTTP/3 client
    pub fn builder(server_addr: SocketAddr) -> H3ClientBuilder {
        H3ClientBuilder::new(server_addr)
    }

    /// Compress data using zstd if compression is enabled
    fn maybe_compress(&self, data: Bytes) -> Result<Bytes, QuillError> {
        if !self.config.enable_compression {
            return Ok(data);
        }

        zstd::encode_all(&data[..], self.config.compression_level)
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

    /// Make a unary RPC call over HTTP/3
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
            rpc.transport = "http3",
            otel.kind = "client"
        )
    )]
    pub async fn call(
        &self,
        service: &str,
        method: &str,
        request: Bytes,
    ) -> Result<Bytes, QuillError> {
        // Build the URI path
        let uri = format!("https://localhost/{}/{}", service, method);

        // Compress request if enabled
        let (request_body, content_encoding) = if self.config.enable_compression {
            let compressed = self.maybe_compress(request)?;
            (compressed, Some("zstd"))
        } else {
            (request, None)
        };

        // Build the HTTP request
        let mut req_builder = Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("content-type", "application/proto")
            .header("accept", "application/proto")
            .header("prefer", self.profile_preference.to_header_value());

        // Add compression headers if enabled
        if self.config.enable_compression {
            req_builder = req_builder.header("accept-encoding", "zstd");
        }
        if let Some(encoding) = content_encoding {
            req_builder = req_builder.header("content-encoding", encoding);
        }

        let req = req_builder
            .body(request_body)
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request over HTTP/3
        let resp = self
            .client
            .send_request(self.server_addr, req)
            .await
            .map_err(|e| QuillError::Transport(format!("HTTP/3 request failed: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body = resp.into_body();
            // Try to parse as Problem Details
            if let Ok(pd) = serde_json::from_slice(&body) {
                return Err(QuillError::ProblemDetails(pd));
            }
            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body)
            )));
        }

        // Get content encoding before consuming response
        let resp_content_encoding = resp
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // Get response body
        let body_bytes = resp.into_body();

        // Decompress if needed
        let body_bytes = self.maybe_decompress(body_bytes, resp_content_encoding.as_deref())?;

        Ok(body_bytes)
    }

    /// Make a client streaming RPC call over HTTP/3
    ///
    /// # Arguments
    /// * `service` - The service path
    /// * `method` - The method name
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
            rpc.transport = "http3",
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

    /// Receive a streaming response over HTTP/3 (server streaming)
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
            rpc.transport = "http3",
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
        // Build the URI path
        let uri = format!("https://localhost/{}/{}", service, method);

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("content-type", "application/proto")
            .header("accept", "application/proto")
            .header("prefer", self.profile_preference.to_header_value())
            .body(request)
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request over HTTP/3
        let resp = self
            .client
            .send_request(self.server_addr, req)
            .await
            .map_err(|e| QuillError::Transport(format!("HTTP/3 request failed: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body = resp.into_body();
            if let Ok(pd) = serde_json::from_slice(&body) {
                return Err(QuillError::ProblemDetails(pd));
            }
            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body)
            )));
        }

        // Parse response body as framed stream
        let body = resp.into_body();
        let stream = H3ResponseFrameStream::new(body);

        Ok(Box::pin(stream))
    }

    /// Make a bidirectional streaming RPC call over HTTP/3
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
            rpc.transport = "http3",
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
        // Encode the request stream into frames
        let encoded = encode_request_stream(request).await?;

        // Build the URI path
        let uri = format!("https://localhost/{}/{}", service, method);

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&uri)
            .header("content-type", "application/proto")
            .header("accept", "application/proto")
            .header("prefer", self.profile_preference.to_header_value())
            .body(encoded)
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request over HTTP/3
        let resp = self
            .client
            .send_request(self.server_addr, req)
            .await
            .map_err(|e| QuillError::Transport(format!("HTTP/3 request failed: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body = resp.into_body();
            if let Ok(pd) = serde_json::from_slice(&body) {
                return Err(QuillError::ProblemDetails(pd));
            }
            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body)
            )));
        }

        // Parse response body as framed stream
        let body = resp.into_body();
        let stream = H3ResponseFrameStream::new(body);

        Ok(Box::pin(stream))
    }

    /// Get the server address
    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    /// Check if 0-RTT is enabled
    pub fn is_zero_rtt_enabled(&self) -> bool {
        self.config.enable_zero_rtt
    }
}

#[cfg(feature = "http3")]
impl fmt::Debug for QuillH3Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuillH3Client")
            .field("server_addr", &self.server_addr)
            .field("config", &self.config)
            .finish()
    }
}

/// Stream adapter that parses frames from HTTP/3 response body
#[cfg(feature = "http3")]
struct H3ResponseFrameStream {
    parser: FrameParser,
    credits: CreditTracker,
    messages_received: u32,
}

#[cfg(feature = "http3")]
impl H3ResponseFrameStream {
    fn new(body: Bytes) -> Self {
        let mut parser = FrameParser::new();
        parser.feed(&body);
        Self {
            parser,
            credits: CreditTracker::with_defaults(),
            messages_received: 0,
        }
    }
}

#[cfg(feature = "http3")]
impl Stream for H3ResponseFrameStream {
    type Item = Result<Bytes, QuillError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use quill_core::DEFAULT_CREDIT_REFILL;
        use std::task::Poll;

        loop {
            // Try to parse a frame from buffered data
            match self.parser.parse_frame() {
                Ok(Some(frame)) => {
                    if frame.flags.is_end_stream() {
                        return Poll::Ready(None);
                    }
                    if frame.flags.is_credit() {
                        // Server is granting us credits
                        if let Some(amount) = frame.decode_credit() {
                            self.credits.grant(amount);
                        }
                        continue;
                    }
                    if frame.flags.is_data() {
                        self.messages_received += 1;

                        // Track credit grants
                        if self.messages_received % DEFAULT_CREDIT_REFILL == 0 {
                            tracing::debug!(
                                "Would grant {} credits to server (received {} messages)",
                                DEFAULT_CREDIT_REFILL,
                                self.messages_received
                            );
                        }

                        return Poll::Ready(Some(Ok(frame.payload)));
                    }
                    if frame.flags.is_cancel() {
                        return Poll::Ready(Some(Err(QuillError::Rpc(
                            "Stream cancelled by server".to_string(),
                        ))));
                    }
                }
                Ok(None) => {
                    // No more frames
                    return Poll::Ready(None);
                }
                Err(e) => {
                    return Poll::Ready(Some(Err(QuillError::Framing(e.to_string()))));
                }
            }
        }
    }
}

/// Builder for configuring an HTTP/3 Quill client
#[cfg(feature = "http3")]
pub struct H3ClientBuilder {
    server_addr: SocketAddr,
    config: H3ClientConfig,
    profile_preference: Option<ProfilePreference>,
}

#[cfg(feature = "http3")]
impl H3ClientBuilder {
    /// Create a new HTTP/3 client builder
    pub fn new(server_addr: SocketAddr) -> Self {
        Self {
            server_addr,
            config: H3ClientConfig::default(),
            profile_preference: None,
        }
    }

    /// Enable 0-RTT for idempotent requests
    pub fn enable_zero_rtt(mut self, enable: bool) -> Self {
        self.config.enable_zero_rtt = enable;
        self
    }

    /// Enable HTTP/3 datagrams
    pub fn enable_datagrams(mut self, enable: bool) -> Self {
        self.config.enable_datagrams = enable;
        self
    }

    /// Enable connection migration
    pub fn enable_connection_migration(mut self, enable: bool) -> Self {
        self.config.enable_connection_migration = enable;
        self
    }

    /// Set max concurrent streams
    pub fn max_concurrent_streams(mut self, max: u64) -> Self {
        self.config.max_concurrent_streams = max;
        self
    }

    /// Set idle timeout
    pub fn idle_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.config.idle_timeout_ms = timeout_ms;
        self
    }

    /// Enable zstd compression
    pub fn enable_compression(mut self, enable: bool) -> Self {
        self.config.enable_compression = enable;
        self
    }

    /// Set compression level (0-22)
    pub fn compression_level(mut self, level: i32) -> Self {
        self.config.compression_level = level;
        self
    }

    /// Set profile preference
    pub fn profile_preference(mut self, pref: ProfilePreference) -> Self {
        self.profile_preference = Some(pref);
        self
    }

    /// Build the HTTP/3 client
    pub fn build(self) -> Result<QuillH3Client, QuillError> {
        let mut client = QuillH3Client::with_config(self.server_addr, self.config)?;

        if let Some(pref) = self.profile_preference {
            client.profile_preference = pref;
        }

        Ok(client)
    }
}

// Stub implementation when http3 feature is disabled
#[cfg(not(feature = "http3"))]
pub struct QuillH3Client;

#[cfg(not(feature = "http3"))]
impl QuillH3Client {
    pub fn new(_server_addr: std::net::SocketAddr) -> Result<Self, quill_core::QuillError> {
        Err(quill_core::QuillError::Transport(
            "HTTP/3 support requires the 'http3' feature to be enabled".to_string(),
        ))
    }
}

#[cfg(test)]
#[cfg(feature = "http3")]
mod tests {
    use super::*;
    use quill_core::Frame;

    #[tokio::test]
    async fn test_h3_client_builder() {
        // Install rustls crypto provider
        let _ = rustls::crypto::ring::default_provider().install_default();

        let addr: SocketAddr = "127.0.0.1:4433".parse().unwrap();
        let client = QuillH3Client::builder(addr)
            .enable_zero_rtt(true)
            .enable_compression(true)
            .compression_level(5)
            .max_concurrent_streams(200)
            .build()
            .unwrap();

        assert_eq!(client.server_addr(), addr);
        assert!(client.is_zero_rtt_enabled());
        assert!(client.config.enable_compression);
        assert_eq!(client.config.compression_level, 5);
        assert_eq!(client.config.max_concurrent_streams, 200);
    }

    #[test]
    fn test_h3_client_config_default() {
        let config = H3ClientConfig::default();
        assert!(!config.enable_zero_rtt);
        assert!(config.enable_datagrams);
        assert!(config.enable_connection_migration);
        assert_eq!(config.max_concurrent_streams, 100);
        assert!(!config.enable_compression);
    }

    #[tokio::test]
    async fn test_frame_stream_parsing() {
        // Create some test frames
        let frame1 = Frame::data(Bytes::from("hello"));
        let frame2 = Frame::data(Bytes::from("world"));
        let end_frame = Frame::end_stream();

        let mut body = Vec::new();
        body.extend_from_slice(&frame1.encode());
        body.extend_from_slice(&frame2.encode());
        body.extend_from_slice(&end_frame.encode());

        let stream = H3ResponseFrameStream::new(Bytes::from(body));
        let mut pinned = Box::pin(stream);

        use tokio_stream::StreamExt;

        let msg1 = pinned.next().await.unwrap().unwrap();
        assert_eq!(msg1, Bytes::from("hello"));

        let msg2 = pinned.next().await.unwrap().unwrap();
        assert_eq!(msg2, Bytes::from("world"));

        // Stream should end
        assert!(pinned.next().await.is_none());
    }
}
