//! Hyper profile (HTTP/3 over QUIC) transport implementation

#[cfg(feature = "http3")]
use bytes::Bytes;
#[cfg(feature = "http3")]
use http::{Request, Response, StatusCode};
#[cfg(feature = "http3")]
use quill_core::PrismProfile;
#[cfg(feature = "http3")]
use std::future::Future;
#[cfg(feature = "http3")]
use std::net::SocketAddr;
#[cfg(feature = "http3")]
use std::pin::Pin;
#[cfg(feature = "http3")]
use std::sync::Arc;
#[cfg(feature = "http3")]
use thiserror::Error;

/// HTTP/3 transport for the Hyper profile
#[cfg(feature = "http3")]
pub struct HyperTransport {
    profile: PrismProfile,
    config: HyperConfig,
}

/// Configuration for HTTP/3 transport
#[cfg(feature = "http3")]
#[derive(Debug, Clone)]
pub struct HyperConfig {
    /// Enable 0-RTT for idempotent requests
    pub enable_zero_rtt: bool,
    /// Enable HTTP/3 datagrams
    pub enable_datagrams: bool,
    /// Enable connection migration
    pub enable_connection_migration: bool,
    /// Initial max concurrent streams
    pub max_concurrent_streams: u64,
    /// Max datagram size (bytes)
    pub max_datagram_size: usize,
    /// Keep-alive interval (milliseconds)
    pub keep_alive_interval_ms: u64,
    /// Idle timeout (milliseconds)
    pub idle_timeout_ms: u64,
}

#[cfg(feature = "http3")]
impl Default for HyperConfig {
    fn default() -> Self {
        Self {
            enable_zero_rtt: false, // Disabled by default for safety
            enable_datagrams: true,
            enable_connection_migration: true,
            max_concurrent_streams: 100,
            max_datagram_size: 65536,
            keep_alive_interval_ms: 30000,
            idle_timeout_ms: 60000,
        }
    }
}

#[cfg(feature = "http3")]
impl HyperTransport {
    /// Create a new Hyper transport with default configuration
    pub fn new() -> Self {
        Self {
            profile: PrismProfile::Hyper,
            config: HyperConfig::default(),
        }
    }

    /// Create a new Hyper transport with custom configuration
    pub fn with_config(config: HyperConfig) -> Self {
        Self {
            profile: PrismProfile::Hyper,
            config,
        }
    }

    /// Get the profile this transport implements
    pub fn profile(&self) -> PrismProfile {
        self.profile
    }

    /// Get the configuration
    pub fn config(&self) -> &HyperConfig {
        &self.config
    }

    /// Check if 0-RTT is enabled
    pub fn is_zero_rtt_enabled(&self) -> bool {
        self.config.enable_zero_rtt
    }

    /// Check if datagrams are enabled
    pub fn is_datagrams_enabled(&self) -> bool {
        self.config.enable_datagrams
    }
}

#[cfg(feature = "http3")]
impl Default for HyperTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP/3 connection handler
#[cfg(feature = "http3")]
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// HTTP/3 service trait for handling requests
#[cfg(feature = "http3")]
pub trait H3Service: Clone + Send + 'static {
    fn call(&self, req: Request<()>) -> BoxFuture<Result<Response<Bytes>, StatusCode>>;
}

/// HTTP/3 server builder
#[cfg(feature = "http3")]
pub struct H3ServerBuilder {
    config: HyperConfig,
    bind_addr: SocketAddr,
}

#[cfg(feature = "http3")]
impl H3ServerBuilder {
    /// Create a new HTTP/3 server builder
    pub fn new(bind_addr: SocketAddr) -> Self {
        Self {
            config: HyperConfig::default(),
            bind_addr,
        }
    }

    /// Enable 0-RTT
    pub fn enable_zero_rtt(mut self, enable: bool) -> Self {
        self.config.enable_zero_rtt = enable;
        self
    }

    /// Enable datagrams
    pub fn enable_datagrams(mut self, enable: bool) -> Self {
        self.config.enable_datagrams = enable;
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

    /// Build the HTTP/3 server
    pub fn build(self) -> Result<H3Server, HyperError> {
        Ok(H3Server {
            config: self.config,
            bind_addr: self.bind_addr,
        })
    }
}

/// HTTP/3 server
#[cfg(feature = "http3")]
pub struct H3Server {
    config: HyperConfig,
    bind_addr: SocketAddr,
}

#[cfg(feature = "http3")]
impl H3Server {
    /// Get the bind address
    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    /// Get the configuration
    pub fn config(&self) -> &HyperConfig {
        &self.config
    }
}

/// HTTP/3 client builder
#[cfg(feature = "http3")]
pub struct H3ClientBuilder {
    config: HyperConfig,
}

#[cfg(feature = "http3")]
impl H3ClientBuilder {
    /// Create a new HTTP/3 client builder
    pub fn new() -> Self {
        Self {
            config: HyperConfig::default(),
        }
    }

    /// Enable 0-RTT for idempotent requests
    pub fn enable_zero_rtt(mut self, enable: bool) -> Self {
        self.config.enable_zero_rtt = enable;
        self
    }

    /// Enable datagrams
    pub fn enable_datagrams(mut self, enable: bool) -> Self {
        self.config.enable_datagrams = enable;
        self
    }

    /// Build the HTTP/3 client
    pub fn build(self) -> Result<H3Client, HyperError> {
        Ok(H3Client {
            config: Arc::new(self.config),
        })
    }
}

#[cfg(feature = "http3")]
impl Default for H3ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP/3 client
#[cfg(feature = "http3")]
pub struct H3Client {
    config: Arc<HyperConfig>,
}

#[cfg(feature = "http3")]
impl H3Client {
    /// Get the configuration
    pub fn config(&self) -> &HyperConfig {
        &self.config
    }
}

/// HTTP/3 transport errors
#[cfg(feature = "http3")]
#[derive(Debug, Error)]
pub enum HyperError {
    #[error("QUIC connection error: {0}")]
    QuicConnection(String),

    #[error("HTTP/3 stream error: {0}")]
    H3Stream(String),

    #[error("0-RTT rejected: {0}")]
    ZeroRttRejected(String),

    #[error("Datagram error: {0}")]
    Datagram(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

// Stub implementations when http3 feature is disabled
#[cfg(not(feature = "http3"))]
pub struct HyperTransport;

#[cfg(not(feature = "http3"))]
impl HyperTransport {
    pub fn new() -> Self {
        panic!("HTTP/3 support requires the 'http3' feature to be enabled");
    }
}

#[cfg(test)]
#[cfg(feature = "http3")]
mod tests {
    use super::*;

    #[test]
    fn test_hyper_transport() {
        let transport = HyperTransport::new();
        assert_eq!(transport.profile(), PrismProfile::Hyper);
        assert!(!transport.is_zero_rtt_enabled()); // Disabled by default
        assert!(transport.is_datagrams_enabled()); // Enabled by default
    }

    #[test]
    fn test_hyper_config() {
        let config = HyperConfig {
            enable_zero_rtt: true,
            enable_datagrams: true,
            enable_connection_migration: true,
            max_concurrent_streams: 200,
            max_datagram_size: 32768,
            keep_alive_interval_ms: 15000,
            idle_timeout_ms: 30000,
        };

        let transport = HyperTransport::with_config(config);
        assert!(transport.is_zero_rtt_enabled());
        assert_eq!(transport.config().max_concurrent_streams, 200);
    }

    #[test]
    fn test_server_builder() {
        let addr = "127.0.0.1:4433".parse().unwrap();
        let server = H3ServerBuilder::new(addr)
            .enable_zero_rtt(true)
            .enable_datagrams(true)
            .max_concurrent_streams(150)
            .idle_timeout_ms(45000)
            .build()
            .unwrap();

        assert_eq!(server.bind_addr(), addr);
        assert!(server.config().enable_zero_rtt);
        assert_eq!(server.config().max_concurrent_streams, 150);
    }

    #[test]
    fn test_client_builder() {
        let client = H3ClientBuilder::new()
            .enable_zero_rtt(true)
            .enable_datagrams(false)
            .build()
            .unwrap();

        assert!(client.config().enable_zero_rtt);
        assert!(!client.config().enable_datagrams);
    }
}
