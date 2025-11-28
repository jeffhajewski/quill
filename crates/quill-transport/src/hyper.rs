//! Hyper profile (HTTP/3 over QUIC) transport implementation
//!
//! This module provides HTTP/3 transport over QUIC with support for:
//! - Multiplexed streams
//! - 0-RTT connection resumption
//! - HTTP/3 datagrams for unreliable messaging
//! - Connection migration

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
use std::time::Duration;
#[cfg(feature = "http3")]
use thiserror::Error;
#[cfg(feature = "http3")]
use tokio::sync::mpsc;
#[cfg(feature = "http3")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "http3")]
use h3::quic;

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

// ============================================================================
// Datagram Types
// ============================================================================

/// A datagram message for unreliable, unordered delivery
///
/// HTTP/3 datagrams provide low-latency messaging without delivery guarantees.
/// Use cases include:
/// - Real-time sensor data
/// - Gaming state updates
/// - Video/audio packets
/// - Telemetry and metrics
#[cfg(feature = "http3")]
#[derive(Debug, Clone)]
pub struct Datagram {
    /// The datagram payload
    pub payload: Bytes,
    /// Optional metadata/identifier for routing
    pub flow_id: Option<u64>,
}

#[cfg(feature = "http3")]
impl Datagram {
    /// Create a new datagram with payload
    pub fn new(payload: Bytes) -> Self {
        Self {
            payload,
            flow_id: None,
        }
    }

    /// Create a new datagram with payload and flow ID
    pub fn with_flow_id(payload: Bytes, flow_id: u64) -> Self {
        Self {
            payload,
            flow_id: Some(flow_id),
        }
    }

    /// Get the size of the datagram payload
    pub fn size(&self) -> usize {
        self.payload.len()
    }

    /// Encode the datagram for transmission
    ///
    /// Format: [flow_id (optional varint)][payload]
    pub fn encode(&self) -> Bytes {
        if let Some(flow_id) = self.flow_id {
            // Encode flow_id as varint followed by payload
            let mut buf = Vec::with_capacity(8 + self.payload.len());
            encode_varint(flow_id, &mut buf);
            buf.extend_from_slice(&self.payload);
            Bytes::from(buf)
        } else {
            self.payload.clone()
        }
    }

    /// Decode a datagram from received bytes
    ///
    /// If `expect_flow_id` is true, parses the first varint as flow_id
    pub fn decode(data: Bytes, expect_flow_id: bool) -> Result<Self, HyperError> {
        if expect_flow_id {
            let (flow_id, consumed) = decode_varint(&data)
                .map_err(|e| HyperError::Datagram(format!("Failed to decode flow_id: {}", e)))?;
            let payload = data.slice(consumed..);
            Ok(Self {
                payload,
                flow_id: Some(flow_id),
            })
        } else {
            Ok(Self {
                payload: data,
                flow_id: None,
            })
        }
    }
}

/// Encode a u64 as a variable-length integer (QUIC varint format)
#[cfg(feature = "http3")]
fn encode_varint(value: u64, buf: &mut Vec<u8>) {
    if value < 64 {
        buf.push(value as u8);
    } else if value < 16384 {
        buf.push(0x40 | ((value >> 8) as u8));
        buf.push(value as u8);
    } else if value < 1073741824 {
        buf.push(0x80 | ((value >> 24) as u8));
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    } else {
        buf.push(0xC0 | ((value >> 56) as u8));
        buf.push((value >> 48) as u8);
        buf.push((value >> 40) as u8);
        buf.push((value >> 32) as u8);
        buf.push((value >> 24) as u8);
        buf.push((value >> 16) as u8);
        buf.push((value >> 8) as u8);
        buf.push(value as u8);
    }
}

/// Decode a variable-length integer from bytes
/// Returns (value, bytes_consumed)
#[cfg(feature = "http3")]
fn decode_varint(data: &[u8]) -> Result<(u64, usize), &'static str> {
    if data.is_empty() {
        return Err("Empty data");
    }

    let first = data[0];
    let length = 1 << (first >> 6);

    if data.len() < length {
        return Err("Insufficient data");
    }

    let value = match length {
        1 => (first & 0x3F) as u64,
        2 => {
            let v = ((first & 0x3F) as u64) << 8 | data[1] as u64;
            v
        }
        4 => {
            let v = ((first & 0x3F) as u64) << 24
                | (data[1] as u64) << 16
                | (data[2] as u64) << 8
                | data[3] as u64;
            v
        }
        8 => {
            let v = ((first & 0x3F) as u64) << 56
                | (data[1] as u64) << 48
                | (data[2] as u64) << 40
                | (data[3] as u64) << 32
                | (data[4] as u64) << 24
                | (data[5] as u64) << 16
                | (data[6] as u64) << 8
                | data[7] as u64;
            v
        }
        _ => return Err("Invalid varint length"),
    };

    Ok((value, length))
}

/// Receiver for incoming datagrams
#[cfg(feature = "http3")]
pub struct DatagramReceiver {
    rx: mpsc::Receiver<Datagram>,
}

#[cfg(feature = "http3")]
impl DatagramReceiver {
    /// Receive the next datagram
    ///
    /// Returns `None` if the connection is closed
    pub async fn recv(&mut self) -> Option<Datagram> {
        self.rx.recv().await
    }

    /// Try to receive a datagram without blocking
    pub fn try_recv(&mut self) -> Option<Datagram> {
        self.rx.try_recv().ok()
    }
}

/// Sender for outgoing datagrams
#[cfg(feature = "http3")]
#[derive(Clone)]
pub struct DatagramSender {
    conn: quinn::Connection,
    max_size: usize,
}

#[cfg(feature = "http3")]
impl DatagramSender {
    /// Create a new datagram sender
    fn new(conn: quinn::Connection, max_size: usize) -> Self {
        Self { conn, max_size }
    }

    /// Send a datagram
    ///
    /// Returns an error if the datagram is too large or the connection is closed
    pub fn send(&self, datagram: Datagram) -> Result<(), HyperError> {
        let encoded = datagram.encode();
        if encoded.len() > self.max_size {
            return Err(HyperError::Datagram(format!(
                "Datagram too large: {} > {} bytes",
                encoded.len(),
                self.max_size
            )));
        }
        self.conn
            .send_datagram(encoded)
            .map_err(|e| HyperError::Datagram(format!("Failed to send datagram: {}", e)))
    }

    /// Send raw bytes as a datagram
    pub fn send_bytes(&self, data: Bytes) -> Result<(), HyperError> {
        if data.len() > self.max_size {
            return Err(HyperError::Datagram(format!(
                "Datagram too large: {} > {} bytes",
                data.len(),
                self.max_size
            )));
        }
        self.conn
            .send_datagram(data)
            .map_err(|e| HyperError::Datagram(format!("Failed to send datagram: {}", e)))
    }

    /// Get the maximum datagram size
    pub fn max_size(&self) -> usize {
        self.max_size
    }
}

/// A persistent HTTP/3 connection with datagram support
#[cfg(feature = "http3")]
pub struct H3Connection {
    conn: quinn::Connection,
    datagram_sender: DatagramSender,
    datagram_rx: Option<mpsc::Receiver<Datagram>>,
    config: Arc<HyperConfig>,
}

#[cfg(feature = "http3")]
impl H3Connection {
    /// Get the remote address of this connection
    pub fn remote_address(&self) -> SocketAddr {
        self.conn.remote_address()
    }

    /// Get the datagram sender for this connection
    pub fn datagram_sender(&self) -> DatagramSender {
        self.datagram_sender.clone()
    }

    /// Take the datagram receiver
    ///
    /// Can only be called once; returns None on subsequent calls
    pub fn take_datagram_receiver(&mut self) -> Option<DatagramReceiver> {
        self.datagram_rx.take().map(|rx| DatagramReceiver { rx })
    }

    /// Send a datagram on this connection
    pub fn send_datagram(&self, datagram: Datagram) -> Result<(), HyperError> {
        self.datagram_sender.send(datagram)
    }

    /// Check if datagrams are enabled on this connection
    pub fn datagrams_enabled(&self) -> bool {
        self.config.enable_datagrams
    }

    /// Get connection statistics
    pub fn stats(&self) -> quinn::ConnectionStats {
        self.conn.stats()
    }

    /// Close the connection gracefully
    pub fn close(&self, code: u32, reason: &str) {
        self.conn.close(
            quinn::VarInt::from_u32(code),
            reason.as_bytes(),
        );
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

/// Trait for handling incoming datagrams on the server
#[cfg(feature = "http3")]
pub trait DatagramHandler: Clone + Send + 'static {
    /// Handle an incoming datagram
    ///
    /// The handler receives the datagram and a sender to respond with datagrams.
    fn handle(&self, datagram: Datagram, sender: DatagramSender);
}

/// A simple datagram handler that uses a callback function
#[cfg(feature = "http3")]
#[derive(Clone)]
pub struct FnDatagramHandler<F> {
    handler: F,
}

#[cfg(feature = "http3")]
impl<F> FnDatagramHandler<F>
where
    F: Fn(Datagram, DatagramSender) + Clone + Send + 'static,
{
    /// Create a new function-based datagram handler
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

#[cfg(feature = "http3")]
impl<F> DatagramHandler for FnDatagramHandler<F>
where
    F: Fn(Datagram, DatagramSender) + Clone + Send + 'static,
{
    fn handle(&self, datagram: Datagram, sender: DatagramSender) {
        (self.handler)(datagram, sender);
    }
}

/// Server-side connection handle for datagram operations
#[cfg(feature = "http3")]
pub struct ServerConnection {
    conn: quinn::Connection,
    config: Arc<HyperConfig>,
}

#[cfg(feature = "http3")]
impl ServerConnection {
    /// Get a datagram sender for this connection
    pub fn datagram_sender(&self) -> DatagramSender {
        DatagramSender::new(self.conn.clone(), self.config.max_datagram_size)
    }

    /// Get the remote address
    pub fn remote_address(&self) -> SocketAddr {
        self.conn.remote_address()
    }

    /// Get connection statistics
    pub fn stats(&self) -> quinn::ConnectionStats {
        self.conn.stats()
    }
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
            endpoint: None,
        })
    }
}

/// HTTP/3 server
#[cfg(feature = "http3")]
pub struct H3Server {
    config: HyperConfig,
    bind_addr: SocketAddr,
    endpoint: Option<quinn::Endpoint>,
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

    /// Start the HTTP/3 server and accept connections
    ///
    /// # Arguments
    /// * `service` - The service to handle incoming requests
    pub async fn serve<S>(mut self, service: S) -> Result<(), HyperError>
    where
        S: H3Service,
    {
        info!("Starting HTTP/3 server on {}", self.bind_addr);

        // Create rustls server configuration
        let tls_config = self.create_server_tls_config()?;

        // Wrap in QuicServerConfig
        let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
            .map_err(|e| HyperError::Tls(format!("Failed to create QUIC server config: {}", e)))?;

        // Create quinn server configuration
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(crypto));

        // Configure transport
        let mut transport_config = quinn::TransportConfig::default();

        let max_streams = quinn::VarInt::from_u32(self.config.max_concurrent_streams as u32);
        transport_config.max_concurrent_bidi_streams(max_streams);
        transport_config.max_concurrent_uni_streams(max_streams);

        transport_config.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(Duration::from_millis(self.config.idle_timeout_ms))
                .map_err(|_| HyperError::Config("Invalid idle timeout".to_string()))?
        ));
        transport_config.keep_alive_interval(Some(Duration::from_millis(self.config.keep_alive_interval_ms)));

        if self.config.enable_datagrams {
            transport_config.datagram_receive_buffer_size(Some(self.config.max_datagram_size));
            transport_config.datagram_send_buffer_size(self.config.max_datagram_size);
        }

        server_config.transport_config(Arc::new(transport_config));

        // Create and bind endpoint
        let endpoint = quinn::Endpoint::server(server_config, self.bind_addr)
            .map_err(|e| HyperError::QuicConnection(format!("Failed to bind endpoint: {}", e)))?;

        info!("HTTP/3 server listening on {}", endpoint.local_addr().unwrap());
        self.endpoint = Some(endpoint.clone());

        // Accept connections
        while let Some(conn) = endpoint.accept().await {
            let service = service.clone();
            let config = self.config.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(conn, service, config).await {
                    error!("Connection error: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Start the HTTP/3 server with datagram support
    ///
    /// Similar to `serve()`, but also accepts a datagram handler for processing
    /// incoming datagrams.
    ///
    /// # Arguments
    /// * `service` - The service to handle incoming HTTP/3 requests
    /// * `datagram_handler` - Handler for incoming datagrams
    ///
    /// # Example
    /// ```ignore
    /// use quill_transport::{H3ServerBuilder, H3Service, FnDatagramHandler, Datagram};
    ///
    /// let server = H3ServerBuilder::new(addr)
    ///     .enable_datagrams(true)
    ///     .build()?;
    ///
    /// let datagram_handler = FnDatagramHandler::new(|dg, sender| {
    ///     println!("Received datagram: {:?}", dg.payload);
    ///     // Echo back
    ///     let _ = sender.send(dg);
    /// });
    ///
    /// server.serve_with_datagrams(my_service, datagram_handler).await?;
    /// ```
    pub async fn serve_with_datagrams<S, D>(
        mut self,
        service: S,
        datagram_handler: D,
    ) -> Result<(), HyperError>
    where
        S: H3Service,
        D: DatagramHandler,
    {
        info!("Starting HTTP/3 server with datagram support on {}", self.bind_addr);

        // Create rustls server configuration
        let tls_config = self.create_server_tls_config()?;

        // Wrap in QuicServerConfig
        let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
            .map_err(|e| HyperError::Tls(format!("Failed to create QUIC server config: {}", e)))?;

        // Create quinn server configuration
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(crypto));

        // Configure transport
        let mut transport_config = quinn::TransportConfig::default();

        let max_streams = quinn::VarInt::from_u32(self.config.max_concurrent_streams as u32);
        transport_config.max_concurrent_bidi_streams(max_streams);
        transport_config.max_concurrent_uni_streams(max_streams);

        transport_config.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(Duration::from_millis(self.config.idle_timeout_ms))
                .map_err(|_| HyperError::Config("Invalid idle timeout".to_string()))?
        ));
        transport_config.keep_alive_interval(Some(Duration::from_millis(self.config.keep_alive_interval_ms)));

        // Enable datagrams
        transport_config.datagram_receive_buffer_size(Some(self.config.max_datagram_size));
        transport_config.datagram_send_buffer_size(self.config.max_datagram_size);

        server_config.transport_config(Arc::new(transport_config));

        // Create and bind endpoint
        let endpoint = quinn::Endpoint::server(server_config, self.bind_addr)
            .map_err(|e| HyperError::QuicConnection(format!("Failed to bind endpoint: {}", e)))?;

        info!("HTTP/3 server with datagrams listening on {}", endpoint.local_addr().unwrap());
        self.endpoint = Some(endpoint.clone());

        let config = Arc::new(self.config);

        // Accept connections
        while let Some(conn) = endpoint.accept().await {
            let service = service.clone();
            let datagram_handler = datagram_handler.clone();
            let config = config.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection_with_datagrams(
                    conn,
                    service,
                    datagram_handler,
                    config,
                ).await {
                    error!("Connection error: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Handle a single QUIC connection with datagram support
    async fn handle_connection_with_datagrams<S, D>(
        conn: quinn::Incoming,
        service: S,
        datagram_handler: D,
        config: Arc<HyperConfig>,
    ) -> Result<(), HyperError>
    where
        S: H3Service,
        D: DatagramHandler,
    {
        let remote_addr = conn.remote_address();
        debug!("Accepting connection with datagram support from {}", remote_addr);

        let quinn_conn = conn
            .await
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?;

        debug!("Connection established with {}", remote_addr);

        // Spawn datagram handler task
        let datagram_conn = quinn_conn.clone();
        let dg_handler = datagram_handler.clone();
        let dg_config = config.clone();
        tokio::spawn(async move {
            Self::datagram_handler_task(datagram_conn, dg_handler, dg_config).await;
        });

        // Create h3 connection for HTTP/3 streams
        let mut h3_conn = h3::server::Connection::new(h3_quinn::Connection::new(quinn_conn))
            .await
            .map_err(|e| HyperError::H3Stream(format!("H3 connection failed: {}", e)))?;

        // Handle HTTP/3 requests
        loop {
            match h3_conn.accept().await {
                Ok(Some(resolver)) => {
                    let service = service.clone();
                    tokio::spawn(async move {
                        match resolver.resolve_request().await {
                            Ok((req, stream)) => {
                                if let Err(e) = Self::handle_request(req, stream, service).await {
                                    error!("Request error: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to resolve request: {}", e);
                            }
                        }
                    });
                }
                Ok(None) => {
                    debug!("Connection closed by client");
                    break;
                }
                Err(e) => {
                    error!("Error accepting request: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Task that handles incoming datagrams for a connection
    async fn datagram_handler_task<D>(
        conn: quinn::Connection,
        handler: D,
        config: Arc<HyperConfig>,
    )
    where
        D: DatagramHandler,
    {
        let sender = DatagramSender::new(conn.clone(), config.max_datagram_size);

        loop {
            match conn.read_datagram().await {
                Ok(data) => {
                    let datagram = Datagram::new(data);
                    handler.handle(datagram, sender.clone());
                }
                Err(e) => {
                    match e {
                        quinn::ConnectionError::ApplicationClosed(_) => {
                            debug!("Datagram connection closed by application");
                        }
                        quinn::ConnectionError::ConnectionClosed(_) => {
                            debug!("Datagram connection closed");
                        }
                        _ => {
                            warn!("Error receiving datagram: {}", e);
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Handle a single QUIC connection
    async fn handle_connection<S>(
        conn: quinn::Incoming,
        service: S,
        _config: HyperConfig,
    ) -> Result<(), HyperError>
    where
        S: H3Service,
    {
        let remote_addr = conn.remote_address();
        debug!("Accepting connection from {}", remote_addr);

        let quinn_conn = conn
            .await
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?;

        debug!("Connection established with {}", remote_addr);

        // Create h3 connection
        let mut h3_conn = h3::server::Connection::new(h3_quinn::Connection::new(quinn_conn))
            .await
            .map_err(|e| HyperError::H3Stream(format!("H3 connection failed: {}", e)))?;

        // Handle requests
        loop {
            match h3_conn.accept().await {
                Ok(Some(resolver)) => {
                    let service = service.clone();
                    tokio::spawn(async move {
                        // Resolve the request headers
                        match resolver.resolve_request().await {
                            Ok((req, stream)) => {
                                if let Err(e) = Self::handle_request(req, stream, service).await {
                                    error!("Request error: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to resolve request: {}", e);
                            }
                        }
                    });
                }
                Ok(None) => {
                    debug!("Connection closed by client");
                    break;
                }
                Err(e) => {
                    error!("Error accepting request: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single HTTP/3 request
    async fn handle_request<S, B>(
        req: Request<()>,
        mut stream: h3::server::RequestStream<B, Bytes>,
        service: S,
    ) -> Result<(), HyperError>
    where
        S: H3Service,
        B: quic::BidiStream<Bytes>,
    {
        debug!("Handling request: {} {}", req.method(), req.uri());

        // Call the service
        let response = service.call(req).await;

        // Send response
        match response {
            Ok(resp) => {
                let (parts, body) = resp.into_parts();
                let resp = Response::from_parts(parts, ());

                stream
                    .send_response(resp)
                    .await
                    .map_err(|e| HyperError::H3Stream(format!("Failed to send response: {}", e)))?;

                stream
                    .send_data(body)
                    .await
                    .map_err(|e| HyperError::H3Stream(format!("Failed to send body: {}", e)))?;

                stream
                    .finish()
                    .await
                    .map_err(|e| HyperError::H3Stream(format!("Failed to finish stream: {}", e)))?;

                debug!("Response sent successfully");
            }
            Err(status) => {
                let resp = Response::builder()
                    .status(status)
                    .body(())
                    .unwrap();

                stream
                    .send_response(resp)
                    .await
                    .map_err(|e| HyperError::H3Stream(format!("Failed to send error response: {}", e)))?;

                stream
                    .finish()
                    .await
                    .map_err(|e| HyperError::H3Stream(format!("Failed to finish stream: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Create server TLS configuration
    fn create_server_tls_config(&self) -> Result<rustls::ServerConfig, HyperError> {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};

        // TODO: Load certificates from configuration
        // For now, create a self-signed certificate for testing
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .map_err(|e| HyperError::Tls(format!("Failed to generate certificate: {}", e)))?;

        let cert_der = cert.serialize_der()
            .map_err(|e| HyperError::Tls(format!("Failed to serialize certificate: {}", e)))?;
        let key_der = cert.serialize_private_key_der();

        let cert_chain = vec![CertificateDer::from(cert_der)];
        let key = PrivateKeyDer::try_from(key_der)
            .map_err(|_| HyperError::Tls("Failed to parse private key".to_string()))?;

        let mut tls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .map_err(|e| HyperError::Tls(format!("Certificate error: {}", e)))?;

        tls_config.alpn_protocols = vec![b"h3".to_vec()];
        // Note: 0-RTT is controlled at the QUIC layer via max_early_data_size

        Ok(tls_config)
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
        H3Client::new(self.config)
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
    endpoint: quinn::Endpoint,
}

#[cfg(feature = "http3")]
impl H3Client {
    /// Get the configuration
    pub fn config(&self) -> &HyperConfig {
        &self.config
    }

    /// Create a new H3Client with endpoint
    pub fn new(config: HyperConfig) -> Result<Self, HyperError> {
        // Create client TLS configuration
        let tls_config = Self::create_client_tls_config(&config)?;

        // Wrap in QuicClientConfig
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
            .map_err(|e| HyperError::Tls(format!("Failed to create QUIC client config: {}", e)))?;

        // Create quinn client configuration
        let mut client_config = quinn::ClientConfig::new(Arc::new(crypto));

        // Configure transport
        let mut transport_config = quinn::TransportConfig::default();

        let max_streams = quinn::VarInt::from_u32(config.max_concurrent_streams as u32);
        transport_config.max_concurrent_bidi_streams(max_streams);
        transport_config.max_concurrent_uni_streams(max_streams);

        transport_config.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(Duration::from_millis(config.idle_timeout_ms))
                .map_err(|_| HyperError::Config("Invalid idle timeout".to_string()))?
        ));
        transport_config.keep_alive_interval(Some(Duration::from_millis(config.keep_alive_interval_ms)));

        if config.enable_datagrams {
            transport_config.datagram_receive_buffer_size(Some(config.max_datagram_size));
            transport_config.datagram_send_buffer_size(config.max_datagram_size);
        }

        client_config.transport_config(Arc::new(transport_config));

        // Create endpoint
        let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| HyperError::QuicConnection(format!("Failed to create endpoint: {}", e)))?;

        endpoint.set_default_client_config(client_config);

        Ok(Self {
            config: Arc::new(config),
            endpoint,
        })
    }

    /// Send an HTTP/3 request
    ///
    /// # Arguments
    /// * `addr` - The server address to connect to
    /// * `req` - The HTTP request to send (body as Bytes)
    ///
    /// # Returns
    /// The HTTP response with body as Bytes
    pub async fn send_request(
        &self,
        addr: SocketAddr,
        req: Request<Bytes>,
    ) -> Result<Response<Bytes>, HyperError> {
        info!("Connecting to {}", addr);

        // Connect to server
        let conn = self
            .endpoint
            .connect(addr, "localhost")
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?
            .await
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?;

        debug!("QUIC connection established");

        // Create h3 connection
        let quinn_conn = h3_quinn::Connection::new(conn);
        let (mut driver, mut send_request) = h3::client::new(quinn_conn)
            .await
            .map_err(|e| HyperError::H3Stream(format!("H3 connection failed: {}", e)))?;

        // Spawn driver task
        tokio::spawn(async move {
            // drive() runs the connection until it completes
            futures::future::poll_fn(|cx| driver.poll_close(cx)).await;
        });

        // Convert request
        let (parts, body) = req.into_parts();
        let req = Request::from_parts(parts, ());

        // Send request
        let mut stream = send_request
            .send_request(req)
            .await
            .map_err(|e| HyperError::H3Stream(format!("Failed to send request: {}", e)))?;

        // Send body
        stream
            .send_data(body)
            .await
            .map_err(|e| HyperError::H3Stream(format!("Failed to send body: {}", e)))?;

        stream
            .finish()
            .await
            .map_err(|e| HyperError::H3Stream(format!("Failed to finish request: {}", e)))?;

        debug!("Request sent, waiting for response");

        // Receive response
        let resp = stream
            .recv_response()
            .await
            .map_err(|e| HyperError::H3Stream(format!("Failed to receive response: {}", e)))?;

        // Read body
        let mut body_data = Vec::new();
        while let Some(mut chunk) = stream
            .recv_data()
            .await
            .map_err(|e| HyperError::H3Stream(format!("Failed to receive body: {}", e)))?
        {
            use bytes::Buf;
            body_data.extend_from_slice(chunk.chunk());
            chunk.advance(chunk.remaining());
        }

        debug!("Response received: {} bytes", body_data.len());

        Ok(resp.map(|_| Bytes::from(body_data)))
    }

    /// Establish a persistent connection with datagram support
    ///
    /// Returns an `H3Connection` that can be used for both HTTP/3 streams
    /// and unreliable datagrams.
    ///
    /// # Arguments
    /// * `addr` - The server address to connect to
    /// * `server_name` - The server name for TLS (SNI)
    ///
    /// # Example
    /// ```ignore
    /// let client = H3ClientBuilder::new()
    ///     .enable_datagrams(true)
    ///     .build()?;
    ///
    /// let mut conn = client.connect(addr, "example.com").await?;
    ///
    /// // Send datagrams
    /// conn.send_datagram(Datagram::new(Bytes::from("hello")))?;
    ///
    /// // Receive datagrams
    /// let mut rx = conn.take_datagram_receiver().unwrap();
    /// while let Some(dg) = rx.recv().await {
    ///     println!("Received: {:?}", dg.payload);
    /// }
    /// ```
    pub async fn connect(
        &self,
        addr: SocketAddr,
        server_name: &str,
    ) -> Result<H3Connection, HyperError> {
        info!("Connecting to {} ({})", addr, server_name);

        // Connect to server
        let conn = self
            .endpoint
            .connect(addr, server_name)
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?
            .await
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?;

        debug!("QUIC connection established with {}", addr);

        // Create datagram channel
        let (datagram_tx, datagram_rx) = mpsc::channel(256);

        // Spawn datagram receiver task if datagrams are enabled
        if self.config.enable_datagrams {
            let conn_clone = conn.clone();
            tokio::spawn(async move {
                Self::datagram_receiver_task(conn_clone, datagram_tx).await;
            });
        }

        let max_datagram_size = self.config.max_datagram_size;
        let datagram_sender = DatagramSender::new(conn.clone(), max_datagram_size);

        Ok(H3Connection {
            conn,
            datagram_sender,
            datagram_rx: Some(datagram_rx),
            config: self.config.clone(),
        })
    }

    /// Internal task that receives datagrams from the QUIC connection
    async fn datagram_receiver_task(
        conn: quinn::Connection,
        tx: mpsc::Sender<Datagram>,
    ) {
        loop {
            match conn.read_datagram().await {
                Ok(data) => {
                    let datagram = Datagram::new(data);
                    if tx.send(datagram).await.is_err() {
                        debug!("Datagram receiver channel closed");
                        break;
                    }
                }
                Err(e) => {
                    match e {
                        quinn::ConnectionError::ApplicationClosed(_) => {
                            debug!("Connection closed by application");
                        }
                        quinn::ConnectionError::ConnectionClosed(_) => {
                            debug!("Connection closed");
                        }
                        _ => {
                            warn!("Error receiving datagram: {}", e);
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Send a datagram on a one-shot connection
    ///
    /// This method establishes a connection, sends the datagram, and returns.
    /// For repeated datagram sends, use `connect()` to get a persistent connection.
    pub async fn send_datagram_oneshot(
        &self,
        addr: SocketAddr,
        datagram: Datagram,
    ) -> Result<(), HyperError> {
        if !self.config.enable_datagrams {
            return Err(HyperError::Datagram("Datagrams are disabled".to_string()));
        }

        let conn = self
            .endpoint
            .connect(addr, "localhost")
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?
            .await
            .map_err(|e| HyperError::QuicConnection(format!("Connection failed: {}", e)))?;

        let encoded = datagram.encode();
        if encoded.len() > self.config.max_datagram_size {
            return Err(HyperError::Datagram(format!(
                "Datagram too large: {} > {} bytes",
                encoded.len(),
                self.config.max_datagram_size
            )));
        }

        conn.send_datagram(encoded)
            .map_err(|e| HyperError::Datagram(format!("Failed to send datagram: {}", e)))?;

        debug!("Datagram sent to {}", addr);
        Ok(())
    }

    /// Create client TLS configuration
    fn create_client_tls_config(config: &HyperConfig) -> Result<rustls::ClientConfig, HyperError> {
        let mut tls_config = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        tls_config.alpn_protocols = vec![b"h3".to_vec()];
        tls_config.enable_early_data = config.enable_zero_rtt;

        Ok(tls_config)
    }
}

/// Skip server certificate verification (for testing only!)
#[cfg(feature = "http3")]
#[derive(Debug)]
struct SkipServerVerification;

#[cfg(feature = "http3")]
impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer,
        _intermediates: &[rustls::pki_types::CertificateDer],
        _server_name: &rustls::pki_types::ServerName,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
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

#[cfg(not(feature = "http3"))]
impl Default for HyperTransport {
    fn default() -> Self {
        Self::new()
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

    #[tokio::test]
    async fn test_client_builder() {
        // Install the ring crypto provider for rustls
        let _ = rustls::crypto::ring::default_provider().install_default();

        let client = H3ClientBuilder::new()
            .enable_zero_rtt(true)
            .enable_datagrams(false)
            .build()
            .unwrap();

        assert!(client.config().enable_zero_rtt);
        assert!(!client.config().enable_datagrams);
    }

    // ========================================================================
    // Datagram Tests
    // ========================================================================

    #[test]
    fn test_datagram_new() {
        let payload = Bytes::from("hello world");
        let dg = Datagram::new(payload.clone());

        assert_eq!(dg.payload, payload);
        assert!(dg.flow_id.is_none());
        assert_eq!(dg.size(), 11);
    }

    #[test]
    fn test_datagram_with_flow_id() {
        let payload = Bytes::from("test data");
        let dg = Datagram::with_flow_id(payload.clone(), 42);

        assert_eq!(dg.payload, payload);
        assert_eq!(dg.flow_id, Some(42));
    }

    #[test]
    fn test_datagram_encode_decode_no_flow_id() {
        let original = Datagram::new(Bytes::from("hello"));
        let encoded = original.encode();

        // Without flow_id, encode just returns the payload
        assert_eq!(encoded, Bytes::from("hello"));

        let decoded = Datagram::decode(encoded, false).unwrap();
        assert_eq!(decoded.payload, original.payload);
        assert!(decoded.flow_id.is_none());
    }

    #[test]
    fn test_datagram_encode_decode_with_flow_id() {
        let original = Datagram::with_flow_id(Bytes::from("hello"), 42);
        let encoded = original.encode();

        // With flow_id, should be: [varint flow_id][payload]
        assert!(encoded.len() > original.payload.len());

        let decoded = Datagram::decode(encoded, true).unwrap();
        assert_eq!(decoded.payload, original.payload);
        assert_eq!(decoded.flow_id, Some(42));
    }

    #[test]
    fn test_varint_encode_decode() {
        // Test various varint values
        let test_values = vec![
            0u64,
            1,
            63,      // Max 1-byte
            64,      // Min 2-byte
            16383,   // Max 2-byte
            16384,   // Min 4-byte
            1073741823, // Max 4-byte
        ];

        for value in test_values {
            let mut buf = Vec::new();
            encode_varint(value, &mut buf);
            let (decoded, consumed) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, value, "Failed for value {}", value);
            assert_eq!(consumed, buf.len());
        }
    }

    #[test]
    fn test_datagram_large_flow_id() {
        // Test with a large flow_id that requires 8-byte encoding
        let flow_id = 1u64 << 60;
        let dg = Datagram::with_flow_id(Bytes::from("data"), flow_id);
        let encoded = dg.encode();

        let decoded = Datagram::decode(encoded, true).unwrap();
        assert_eq!(decoded.flow_id, Some(flow_id));
        assert_eq!(decoded.payload, Bytes::from("data"));
    }

    #[test]
    fn test_fn_datagram_handler() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let handler = FnDatagramHandler::new(move |_dg, _sender| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        // The handler can be cloned
        let handler2 = handler.clone();

        // We can't easily test the handler without a real connection,
        // but we can verify it compiles and the structure is correct
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Verify the handler is Clone
        drop(handler);
        drop(handler2);
    }

    #[test]
    fn test_hyper_config_datagram_defaults() {
        let config = HyperConfig::default();

        assert!(config.enable_datagrams);
        assert_eq!(config.max_datagram_size, 65536);
    }
}
