# HTTP/3 and QUIC Support

This guide covers HTTP/3 (Hyper profile) support in Quill, built on QUIC transport using the `quinn` library.

## Table of Contents

- [Overview](#overview)
- [Enabling HTTP/3](#enabling-http3)
- [Hyper Profile Configuration](#hyper-profile-configuration)
- [0-RTT Support](#0-rtt-support)
- [HTTP/3 Datagrams](#http3-datagrams)
- [Connection Migration](#connection-migration)
- [Server Setup](#server-setup)
- [Client Setup](#client-setup)
- [TLS Configuration](#tls-configuration)
- [Performance Considerations](#performance-considerations)
- [Troubleshooting](#troubleshooting)

## Overview

The Hyper profile provides HTTP/3 transport over QUIC, offering:

- **Multiplexed Streams**: Independent streams over a single QUIC connection
- **0-RTT Support**: Fast connection resumption for idempotent requests
- **Connection Migration**: Seamless handoff between networks (WiFi ↔ cellular)
- **HTTP/3 Datagrams**: Unreliable, unordered messaging for real-time data
- **Improved Loss Recovery**: QUIC's built-in loss detection and recovery
- **Head-of-Line Blocking Elimination**: Stream-level independence

### Why HTTP/3?

HTTP/3 is ideal for:
- **Mobile Clients**: Connection migration maintains connectivity during network changes
- **Browser Applications**: Modern browsers support HTTP/3 natively
- **Lossy Networks**: Better performance on networks with packet loss
- **Real-Time Applications**: Datagrams for low-latency messaging
- **Edge-to-Client Communication**: CDN to end-user connections

## Enabling HTTP/3

HTTP/3 support is behind a feature flag. Enable it in your `Cargo.toml`:

```toml
[dependencies]
quill-transport = { version = "0.1", features = ["http3"] }
quill-client = { version = "0.1", features = ["http3"] }
quill-server = { version = "0.1", features = ["http3"] }
```

### Dependencies

The HTTP/3 implementation uses:
- **quinn**: Pure Rust QUIC implementation (v0.11)
- **h3**: HTTP/3 protocol implementation (v0.0.8)
- **h3-quinn**: Integration layer between h3 and quinn
- **rustls**: TLS 1.3 implementation for encryption

**Note**: The h3 crate is still experimental. While functional, expect potential API changes and bugs.

## Hyper Profile Configuration

### Basic Configuration

```rust
use quill_transport::{HyperConfig, HyperTransport};

let config = HyperConfig {
    enable_zero_rtt: false,        // Disabled by default for safety
    enable_datagrams: true,         // Enable HTTP/3 datagrams
    enable_connection_migration: true,  // Enable connection migration
    max_concurrent_streams: 100,    // Max concurrent HTTP/3 streams
    max_datagram_size: 65536,       // 64 KB datagram limit
    keep_alive_interval_ms: 30000,  // 30-second keep-alive
    idle_timeout_ms: 60000,         // 60-second idle timeout
};

let transport = HyperTransport::with_config(config);
```

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `enable_zero_rtt` | `false` | Enable 0-RTT for faster connection resumption |
| `enable_datagrams` | `true` | Enable HTTP/3 datagrams for unreliable messaging |
| `enable_connection_migration` | `true` | Allow connections to migrate between networks |
| `max_concurrent_streams` | `100` | Maximum number of concurrent streams |
| `max_datagram_size` | `65536` | Maximum datagram payload size (bytes) |
| `keep_alive_interval_ms` | `30000` | Interval for sending keep-alive packets |
| `idle_timeout_ms` | `60000` | Connection idle timeout before closing |

## 0-RTT Support

### Overview

0-RTT (Zero Round-Trip Time) allows clients to send application data in the first flight of packets, reducing latency for connection resumption.

**Security Considerations**:
- Only use 0-RTT for **idempotent** operations (GET, HEAD, safe RPCs)
- Servers must protect against replay attacks
- 0-RTT is disabled by default for safety

### Enabling 0-RTT

Mark RPC methods as idempotent in your `.proto` files:

```protobuf
service ImageService {
  rpc GetMetadata(GetRequest) returns (ImageMetadata) {
    option (quill.rpc) = { idempotent: true };
  }
}
```

Enable 0-RTT in the client:

```rust
use quill_transport::{H3ClientBuilder};

let client = H3ClientBuilder::new()
    .enable_zero_rtt(true)  // Enable 0-RTT for idempotent requests
    .enable_datagrams(true)
    .build()?;
```

### Server 0-RTT Handling

Servers automatically detect 0-RTT requests and can reject replays:

```rust
use quill_transport::{H3ServerBuilder};

let server = H3ServerBuilder::new("0.0.0.0:4433".parse()?)
    .enable_zero_rtt(true)
    .build()?;

// The server will return 425 Too Early for replayed 0-RTT requests
```

### 0-RTT Replay Protection

Quill implements replay protection:
1. Server maintains a rolling window of accepted 0-RTT tickets
2. Non-idempotent methods reject 0-RTT automatically
3. Returns `425 Too Early` status code on suspected replays

## HTTP/3 Datagrams

### Overview

HTTP/3 datagrams provide **unreliable, unordered** message delivery:
- No delivery guarantees (packets may be lost)
- No ordering guarantees (packets may arrive out of order)
- Lower latency than streams (no head-of-line blocking)

Use cases:
- Real-time sensor data
- Gaming updates
- Video/audio packets with FEC
- Telemetry and metrics

### Sending Datagrams

```rust
use quill_transport::H3Client;
use bytes::Bytes;

let client = H3ClientBuilder::new()
    .enable_datagrams(true)
    .build()?;

// Send unreliable datagram
let datagram_data = Bytes::from("sensor:temp=72.5");
client.send_datagram(datagram_data).await?;
```

### Receiving Datagrams

```rust
use quill_transport::H3Server;

let server = H3ServerBuilder::new(bind_addr)
    .enable_datagrams(true)
    .build()?;

// Receive datagrams in handler
while let Some(datagram) = server.recv_datagram().await? {
    // Process unreliable datagram
    process_sensor_data(datagram);
}
```

### Datagram Size Limits

```rust
let config = HyperConfig {
    max_datagram_size: 32768,  // 32 KB limit
    ..Default::default()
};

// Datagrams larger than max_datagram_size will be rejected
```

**Recommendations**:
- Keep datagrams < 1200 bytes to avoid fragmentation
- Use datagrams for data that can tolerate loss
- Consider Forward Error Correction (FEC) for important datagram data

## Connection Migration

### Overview

Connection migration allows QUIC connections to survive network changes:
- WiFi ↔ Cellular handoff
- IP address changes
- NAT rebinding

This is critical for mobile applications and improving user experience.

### Enabling Connection Migration

```rust
let client = H3ClientBuilder::new()
    .enable_connection_migration(true)
    .build()?;

// Connection will automatically migrate when network changes
```

### How It Works

1. Client detects network change (new IP address)
2. Client sends PATH_CHALLENGE on new path
3. Server responds with PATH_RESPONSE
4. Connection migrates to new path seamlessly
5. Application code sees no interruption

### Migration Events

Monitor connection migration:

```rust
// Connection migration is transparent to application code
// QUIC handles all PATH_CHALLENGE/PATH_RESPONSE exchanges
```

## Quill RPC over HTTP/3

### QuillH3Client

The `QuillH3Client` provides the same API as `QuillClient` but uses HTTP/3 transport:

```rust
use quill_client::{QuillH3Client, H3ClientConfig};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:4433".parse()?;

    // Create HTTP/3 client with configuration
    let client = QuillH3Client::builder(addr)
        .enable_zero_rtt(true)           // Fast resumption
        .enable_compression(true)         // zstd compression
        .max_concurrent_streams(200)
        .build()?;

    // Make a unary RPC call
    let request = Bytes::from("hello");
    let response = client.call("echo.v1.EchoService", "Echo", request).await?;

    println!("Response: {:?}", response);
    Ok(())
}
```

### Server Streaming over HTTP/3

```rust
use quill_client::QuillH3Client;
use tokio_stream::StreamExt;

let client = QuillH3Client::builder(addr).build()?;

// Make a server streaming call
let request = Bytes::from(r#"{"query": "logs"}"#);
let mut stream = client
    .call_server_streaming("logging.v1.LogService", "TailLogs", request)
    .await?;

// Process streaming responses
while let Some(result) = stream.next().await {
    match result {
        Ok(log_entry) => println!("Log: {:?}", log_entry),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### Client Streaming over HTTP/3

```rust
use quill_client::QuillH3Client;
use tokio_stream::iter;

let client = QuillH3Client::builder(addr).build()?;

// Create a stream of file chunks
let chunks = vec![
    Ok(Bytes::from("chunk1")),
    Ok(Bytes::from("chunk2")),
    Ok(Bytes::from("chunk3")),
];
let request_stream = Box::pin(iter(chunks));

// Make a client streaming call
let response = client
    .call_client_streaming("upload.v1.UploadService", "Upload", request_stream)
    .await?;
```

### Bidirectional Streaming over HTTP/3

```rust
use quill_client::QuillH3Client;
use tokio_stream::StreamExt;

let client = QuillH3Client::builder(addr).build()?;

// Create request stream
let messages = vec![
    Ok(Bytes::from(r#"{"text": "Hello"}"#)),
    Ok(Bytes::from(r#"{"text": "World"}"#)),
];
let request_stream = Box::pin(tokio_stream::iter(messages));

// Make bidirectional streaming call
let mut response_stream = client
    .call_bidi_streaming("chat.v1.ChatService", "Chat", request_stream)
    .await?;

while let Some(result) = response_stream.next().await {
    match result {
        Ok(msg) => println!("Received: {:?}", msg),
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

### QuillH3Server

The `QuillH3Server` serves Quill RPC methods over HTTP/3:

```rust
use quill_server::{QuillH3Server, RpcRouter};
use bytes::Bytes;
use quill_core::QuillError;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:4433".parse()?;

    // Create server with handlers
    let server = QuillH3Server::builder(addr)
        .enable_zero_rtt(true)
        .enable_datagrams(true)
        .max_concurrent_streams(200)
        .idle_timeout_ms(120000)
        .register("echo.v1.EchoService/Echo", |req: Bytes| async move {
            Ok(req) // Echo back
        })
        .build();

    println!("Quill HTTP/3 server listening on {}", server.bind_addr());

    // Start serving
    server.serve().await?;

    Ok(())
}
```

## Server Setup

### Basic HTTP/3 Server

```rust
use quill_transport::{H3ServerBuilder, H3Service};
use bytes::Bytes;
use http::{Request, Response, StatusCode};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:4433".parse()?;

    let server = H3ServerBuilder::new(addr)
        .enable_zero_rtt(true)
        .enable_datagrams(true)
        .max_concurrent_streams(200)
        .idle_timeout_ms(120000)  // 2 minutes
        .build()?;

    println!("HTTP/3 server listening on {}", server.bind_addr());

    // Server loop would go here
    // (Full implementation requires quinn endpoint setup)

    Ok(())
}
```

### Production Server Configuration

```rust
let server = H3ServerBuilder::new(addr)
    .enable_zero_rtt(true)
    .enable_datagrams(true)
    .enable_connection_migration(true)
    .max_concurrent_streams(1000)
    .max_datagram_size: 65536,
    .keep_alive_interval_ms(20000)   // 20 seconds
    .idle_timeout_ms(300000)         // 5 minutes
    .build()?;
```

## Client Setup

### Basic HTTP/3 Client

```rust
use quill_transport::{H3ClientBuilder};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = H3ClientBuilder::new()
        .enable_zero_rtt(false)      // Start with 0-RTT disabled
        .enable_datagrams(true)
        .build()?;

    // Use client for HTTP/3 requests
    // (Full implementation requires quinn connection setup)

    Ok(())
}
```

### Mobile Client Configuration

Optimize for mobile networks:

```rust
let client = H3ClientBuilder::new()
    .enable_zero_rtt(true)           // Fast resumption
    .enable_datagrams(true)          // Real-time updates
    .enable_connection_migration(true)  // Network handoff
    .build()?;
```

## TLS Configuration

### Server TLS

HTTP/3 requires TLS 1.3. Use `rustls` for certificate configuration:

```rust
use rustls::ServerConfig;
use std::sync::Arc;

// Load server certificate and private key
let certs = load_certs("server.crt")?;
let key = load_private_key("server.key")?;

let mut server_crypto = ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(certs, key)?;

// Enable 0-RTT (requires session storage)
server_crypto.max_early_data_size = 16384;
```

### Client TLS

```rust
use rustls::ClientConfig;

let mut client_crypto = ClientConfig::builder()
    .with_root_certificates(root_store)
    .with_no_client_auth();

// Enable 0-RTT
client_crypto.enable_early_data = true;
```

### Self-Signed Certificates (Development)

For development, accept self-signed certificates:

```rust
// WARNING: Only use in development!
let client_crypto = ClientConfig::builder()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
    .with_no_client_auth();
```

## Performance Considerations

### Latency Benefits

HTTP/3 provides significant latency improvements:

| Scenario | HTTP/2 | HTTP/3 | Improvement |
|----------|---------|---------|-------------|
| First connection | 2-3 RTT | 1-2 RTT | ~33-50% |
| Resumption (0-RTT) | 1 RTT | 0 RTT | ~100% |
| Packet loss (1%) | +200ms | +50ms | ~75% |

### Throughput

QUIC/HTTP/3 throughput is comparable to HTTP/2:
- **Low loss**: ~same throughput
- **High loss (>1%)**: HTTP/3 is 20-50% faster
- **Mobile networks**: HTTP/3 significantly better

### Resource Usage

- **Memory**: ~10-20% more than HTTP/2 (QUIC state)
- **CPU**: ~5-15% more for encryption (TLS 1.3 per packet)
- **Battery**: Connection migration reduces reconnections

### Optimization Tips

1. **Enable 0-RTT** for idempotent requests (50-100ms saved)
2. **Use datagrams** for real-time data (lower latency)
3. **Tune max_concurrent_streams** based on workload
4. **Adjust keep_alive_interval** for mobile (conserve battery)
5. **Monitor connection migration** events

## Troubleshooting

### Common Issues

#### 1. QUIC Blocked by Firewall

**Problem**: QUIC uses UDP on port 443/4433, often blocked by corporate firewalls.

**Solution**:
- Fallback to HTTP/2 (Turbo profile) if QUIC fails
- Use QUIC on alternate ports (e.g., 8443)
- Configure firewall to allow UDP

####2. 0-RTT Replay Attacks

**Problem**: 0-RTT requests are replayed by attackers.

**Solution**:
- Only enable 0-RTT for idempotent operations
- Return `425 Too Early` for suspected replays
- Implement replay detection (ticket window)

#### 3. Datagram Loss

**Problem**: Datagrams are lost frequently.

**Solution**:
- Reduce datagram size (< 1200 bytes)
- Implement application-level reliability (FEC, retransmission)
- Use streams for reliable delivery

#### 4. Connection Migration Failures

**Problem**: Connections fail to migrate between networks.

**Solution**:
- Verify `enable_connection_migration` is true
- Check NAT traversal (some NATs block migration)
- Increase `idle_timeout_ms` to prevent premature closure

### Debugging

Enable QUIC debugging logs:

```rust
use tracing::Level;
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG)
    .init();

// QUIC events will be logged to stdout
```

Monitor QUIC metrics:

```rust
// Connection statistics
let stats = connection.stats();
println!("Path MTU: {}", stats.path.mtu);
println!("RTT: {:?}", stats.path.rtt);
println!("Lost packets: {}", stats.path.lost_packets);
```

## Browser Compatibility

Modern browsers support HTTP/3:
- Chrome 87+ (stable support)
- Firefox 88+ (enabled by default)
- Safari 14+ (experimental)
- Edge 87+ (same as Chrome)

Check HTTP/3 in browser DevTools:
- **Chrome**: Protocol column shows "h3" or "h3-29"
- **Firefox**: Protocol column shows "HTTP/3"

## Security Best Practices

1. **Always use TLS 1.3** (QUIC requires it)
2. **Disable 0-RTT for non-idempotent operations**
3. **Implement replay detection** for 0-RTT
4. **Validate certificates properly** (no self-signed in production)
5. **Monitor for QUIC-specific attacks** (amplification, migration abuse)

## Migration from HTTP/2

### Profile Negotiation

Quill automatically negotiates the best profile:

```
Client: Prefer: prism=hyper,turbo,classic
Server: Selected-Prism: hyper (or turbo if HTTP/3 unavailable)
```

### Gradual Rollout

1. **Phase 1**: Enable HTTP/3 for development/staging
2. **Phase 2**: Enable for small percentage of production traffic
3. **Phase 3**: Monitor latency, throughput, error rates
4. **Phase 4**: Gradually increase HTTP/3 traffic
5. **Phase 5**: Make HTTP/3 default, fallback to HTTP/2

## See Also

- [HTTP/2 Configuration](http2.md) - Turbo profile documentation
- [Resilience Guide](resilience.md) - Retry and circuit breaker patterns
- [Performance Guide](performance.md) - Performance optimization

## References

- [HTTP/3 Specification](https://www.rfc-editor.org/rfc/rfc9114.html)
- [QUIC Specification](https://www.rfc-editor.org/rfc/rfc9000.html)
- [Quinn Documentation](https://docs.rs/quinn/latest/quinn/)
- [h3 Crate](https://docs.rs/h3/latest/h3/)
