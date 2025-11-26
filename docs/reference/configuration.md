# Configuration Reference

Comprehensive configuration options for Quill servers and clients.

## Server Configuration

### QuillServer Builder

```rust
use quill_server::{QuillServer, HttpVersion, Http2Config};
use std::time::Duration;

let server = QuillServer::builder()
    // HTTP version
    .http_version(HttpVersion::Auto)

    // HTTP/2 settings
    .http2_config(Http2Config::default())

    // Timeouts
    .request_timeout(Duration::from_secs(30))
    .idle_timeout(Duration::from_secs(60))

    // Flow control
    .initial_stream_credits(1000)
    .credit_grant_size(500)

    // Limits
    .max_frame_bytes(4 * 1024 * 1024)  // 4 MB
    .max_concurrent_streams(100)

    // Register handlers
    .register("service/method", handler)
    .build();
```

### HTTP Version Options

```rust
pub enum HttpVersion {
    /// Auto-negotiate between HTTP/1.1 and HTTP/2
    Auto,
    /// HTTP/1.1 only (Classic profile)
    Http1Only,
    /// HTTP/2 only (Turbo profile)
    Http2Only,
}
```

### HTTP/2 Configuration

```rust
pub struct Http2Config {
    /// Initial stream-level flow control window (default: 65535)
    pub initial_stream_window_size: u32,

    /// Initial connection-level flow control window (default: 65535)
    pub initial_connection_window_size: u32,

    /// Maximum concurrent streams (default: 100)
    pub max_concurrent_streams: u32,

    /// Max frame size (default: 16384, max: 16777215)
    pub max_frame_size: u32,

    /// Max header list size (default: 16384)
    pub max_header_list_size: u32,

    /// Enable adaptive window sizing (default: false)
    pub adaptive_window: bool,

    /// Keep-alive interval (default: disabled)
    pub keep_alive_interval: Option<Duration>,

    /// Keep-alive timeout (default: 20s)
    pub keep_alive_timeout: Duration,
}
```

### HTTP/3 Configuration (Hyper Profile)

```rust
pub struct HyperConfig {
    /// Enable 0-RTT for idempotent requests (default: true)
    pub enable_0rtt: bool,

    /// Enable HTTP/3 datagrams (default: false)
    pub enable_datagrams: bool,

    /// Enable connection migration (default: true)
    pub enable_migration: bool,

    /// Max idle timeout (default: 30s)
    pub max_idle_timeout: Duration,

    /// Initial RTT estimate (default: 100ms)
    pub initial_rtt: Duration,
}
```

## Client Configuration

### QuillClient Builder

```rust
use quill_client::{QuillClient, Http2Config, PoolConfig, RetryPolicy, CircuitBreaker};
use quill_core::PrismProfile;
use std::time::Duration;

let client = QuillClient::builder()
    // Base URL
    .base_url("http://api.example.com")

    // Timeouts
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(5))

    // Transport profile preference
    .prefer_profiles(&[PrismProfile::Turbo, PrismProfile::Classic])

    // HTTP/2 settings
    .http2_config(Http2Config::default())

    // Connection pooling
    .pool_config(PoolConfig::default())

    // Compression
    .enable_compression(true)

    // Resilience
    .retry_policy(RetryPolicy::default())
    .circuit_breaker(CircuitBreaker::default())

    // Authentication
    .bearer_token("token")
    // or .header("X-API-Key", "key")

    .build()?;
```

### Connection Pool Configuration

```rust
pub struct PoolConfig {
    /// Maximum idle connections per host (default: 10)
    pub max_idle_connections: usize,

    /// Idle connection timeout (default: 90s)
    pub idle_timeout: Duration,

    /// Maximum connections per host (default: unlimited)
    pub max_connections_per_host: Option<usize>,
}
```

### Retry Policy Configuration

```rust
pub struct RetryPolicy {
    /// Maximum retry attempts (default: 3)
    pub max_retries: u32,

    /// Backoff configuration
    pub backoff: BackoffConfig,

    /// Jitter factor 0.0-1.0 (default: 0.1)
    pub jitter: f64,

    /// Status codes to retry (default: [503, 429])
    pub retry_on_status: Vec<u16>,

    /// Retry on connection errors (default: true)
    pub retry_on_connection_error: bool,
}

pub struct BackoffConfig {
    /// Initial delay (default: 100ms)
    pub initial: Duration,

    /// Maximum delay (default: 10s)
    pub max: Duration,

    /// Multiplier (default: 2.0)
    pub multiplier: f64,
}
```

### Circuit Breaker Configuration

```rust
pub struct CircuitBreaker {
    /// Failures before opening (default: 5)
    pub failure_threshold: u32,

    /// Successes before closing (default: 3)
    pub success_threshold: u32,

    /// Time in open state before half-open (default: 30s)
    pub timeout: Duration,

    /// Rolling window for failure tracking (default: 60s)
    pub rolling_window: Duration,
}
```

## Transport Profiles

### Profile Selection

```rust
use quill_core::PrismProfile;

// Client preference (highest to lowest)
.prefer_profiles(&[
    PrismProfile::Hyper,   // HTTP/3 over QUIC
    PrismProfile::Turbo,   // HTTP/2 end-to-end
    PrismProfile::Classic, // HTTP/1.1 + H2
])
```

### Profile Characteristics

| Profile | Protocol | Features |
|---------|----------|----------|
| **Classic** | HTTP/1.1 + H2 | Best proxy compatibility |
| **Turbo** | HTTP/2 e2e | Multiplexing, flow control |
| **Hyper** | HTTP/3/QUIC | 0-RTT, connection migration |

## Middleware Configuration

### Authentication

```rust
// Bearer token
let auth = AuthMiddleware::bearer(|token| async move {
    validate_jwt(token).await
});

// API key
let auth = AuthMiddleware::api_key("X-API-Key", |key| async move {
    validate_api_key(key).await
});

// Basic auth
let auth = AuthMiddleware::basic(|username, password| async move {
    validate_credentials(username, password).await
});

// Custom
let auth = AuthMiddleware::custom(|request| async move {
    // Custom validation logic
    Ok(AuthContext { user_id: "..." })
});
```

### Rate Limiting

```rust
let rate_limit = RateLimitMiddleware::new()
    .requests_per_second(1000)
    .burst_size(100)
    .key_extractor(|req| {
        // Rate limit by API key
        req.headers().get("X-API-Key")
            .map(|h| h.to_str().unwrap_or_default().to_string())
    });
```

### Logging

```rust
let logging = LoggingMiddleware::new()
    .log_request_headers(true)
    .log_response_headers(true)
    .log_body(false)
    .sanitize_headers(&["authorization", "cookie", "x-api-key"]);
```

### Metrics

```rust
let metrics = MetricsMiddleware::new()
    .namespace("quill")
    .subsystem("api")
    .buckets(&[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]);
```

## Resource Limits

### Default Limits

| Limit | Value | Description |
|-------|-------|-------------|
| `max_frame_bytes` | 4 MB | Maximum frame payload |
| `max_streams_per_connection` | 100 | Concurrent streams |
| `zstd_threshold_bytes` | 1024 | Min size for compression |
| `max_header_size` | 16 KB | Maximum header list size |

### Configuring Limits

```rust
// Server
let server = QuillServer::builder()
    .max_frame_bytes(8 * 1024 * 1024)  // 8 MB
    .max_concurrent_streams(200)
    .build();

// Client
let client = QuillClient::builder()
    .max_frame_bytes(8 * 1024 * 1024)
    .build()?;
```

## Compression

### Server Configuration

```rust
// Enable zstd compression
let server = QuillServer::builder()
    .enable_compression(true)
    .compression_threshold(1024)  // Compress responses > 1KB
    .compression_level(3)         // zstd level (1-22)
    .build();
```

### Client Configuration

```rust
let client = QuillClient::builder()
    .enable_compression(true)  // Send Accept-Encoding: zstd
    .build()?;
```

## Observability

### Tracing Configuration

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// JSON structured logging
tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().json())
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

### Metrics Exporter

```rust
use quill_server::observability::ObservabilityCollector;

let collector = ObservabilityCollector::new("my_service");

// Prometheus format
let prometheus_metrics = collector.prometheus_metrics();

// JSON format
let json_metrics = collector.json_metrics();
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Tracing log level | `info` |
| `QUILL_MAX_CONNECTIONS` | Max server connections | unlimited |
| `QUILL_REQUEST_TIMEOUT` | Default request timeout | `30s` |
| `QUILL_IDLE_TIMEOUT` | Idle connection timeout | `60s` |

## Configuration Files

### Example YAML Configuration

```yaml
# quill.yaml
server:
  address: "0.0.0.0:8080"
  http_version: auto
  request_timeout: 30s
  idle_timeout: 60s

  http2:
    max_concurrent_streams: 100
    initial_stream_window_size: 1048576
    keep_alive_interval: 30s

  limits:
    max_frame_bytes: 4194304
    max_header_size: 16384

  compression:
    enabled: true
    threshold: 1024
    level: 3

  middleware:
    auth:
      type: bearer
      jwks_url: "https://auth.example.com/.well-known/jwks.json"

    rate_limit:
      requests_per_second: 1000
      burst_size: 100

    logging:
      request_headers: true
      sanitize:
        - authorization
        - cookie
```

### Loading Configuration

```rust
use quill_server::config::Config;

let config = Config::from_file("quill.yaml")?;

let server = QuillServer::builder()
    .from_config(&config)
    .register("service/method", handler)
    .build();
```

## Next Steps

- [CLI Reference](cli.md) - Command-line tools
- [Server Guide](../guides/server.md) - Server development
- [Client Guide](../guides/client.md) - Client development
- [Performance](../performance.md) - Performance tuning
