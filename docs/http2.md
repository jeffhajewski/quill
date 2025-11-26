# HTTP/2 Support

Quill provides full HTTP/2 support with multiplexing, connection pooling, and flow control. This guide covers HTTP/2 configuration for both clients and servers.

## Table of Contents

- [Overview](#overview)
- [Server Configuration](#server-configuration)
- [Client Configuration](#client-configuration)
- [Turbo Profile](#turbo-profile)
- [Performance Benefits](#performance-benefits)
- [Best Practices](#best-practices)

## Overview

Quill supports three HTTP modes:

1. **Auto (Default)**: Automatically negotiate HTTP/1.1 or HTTP/2 based on client capabilities
2. **HTTP/1.1 Only**: Force HTTP/1.1 for legacy compatibility
3. **HTTP/2 Only (Turbo Profile)**: Maximize performance with HTTP/2 end-to-end

### HTTP/2 Benefits

- **Multiplexing**: Multiple concurrent requests over a single connection
- **Header Compression**: Reduces overhead with HPACK compression
- **Server Push**: Not currently implemented
- **Binary Protocol**: More efficient parsing
- **Connection Reuse**: Better resource utilization
- **Flow Control**: Prevents overwhelming slower endpoints

## Server Configuration

### Basic HTTP/2 Server

```rust
use quill_server::{QuillServer, HttpVersion};

#[tokio::main]
async fn main() {
    // Auto-negotiate (default)
    let server = QuillServer::builder()
        .register("service/Method", handler)
        .build();

    server.serve("127.0.0.1:8080".parse().unwrap()).await.unwrap();
}
```

### HTTP/2 Only Server (Turbo Profile)

```rust
use quill_server::{QuillServer, HttpVersion};

#[tokio::main]
async fn main() {
    let server = QuillServer::builder()
        .turbo_profile() // Enable HTTP/2 only
        .register("service/Method", handler)
        .build();

    server.serve("127.0.0.1:8080".parse().unwrap()).await.unwrap();
}
```

### Custom HTTP/2 Configuration

```rust
use quill_server::{QuillServer, HttpVersion};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let server = QuillServer::builder()
        .http_version(HttpVersion::Http2Only)
        .http2_max_concurrent_streams(200)
        .http2_initial_connection_window_size(2 * 1024 * 1024) // 2MB
        .http2_initial_stream_window_size(2 * 1024 * 1024)     // 2MB
        .http2_keep_alive_interval(Duration::from_secs(5))
        .http2_keep_alive_timeout(Duration::from_secs(10))
        .http2_max_frame_size(32 * 1024) // 32KB
        .register("service/Method", handler)
        .build();

    server.serve("127.0.0.1:8080".parse().unwrap()).await.unwrap();
}
```

### Server Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `http_version` | `Auto` | HTTP protocol version (Auto/Http1Only/Http2Only) |
| `http2_initial_connection_window_size` | 1MB | Initial HTTP/2 connection window size |
| `http2_initial_stream_window_size` | 1MB | Initial HTTP/2 stream window size |
| `http2_max_concurrent_streams` | 100 | Max concurrent streams per connection |
| `http2_keep_alive_interval` | 10s | Keep-alive ping interval |
| `http2_keep_alive_timeout` | 20s | Keep-alive ping timeout |
| `http2_max_frame_size` | 16KB | Maximum HTTP/2 frame size |

## Client Configuration

### Basic HTTP/2 Client

```rust
use quill_client::QuillClient;

#[tokio::main]
async fn main() {
    // Auto-negotiate (default)
    let client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .build()
        .unwrap();

    let response = client
        .call("service", "Method", request_bytes)
        .await
        .unwrap();
}
```

### HTTP/2 Only Client

```rust
use quill_client::{QuillClient, HttpProtocol};

#[tokio::main]
async fn main() {
    let client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .http2_only() // Force HTTP/2
        .build()
        .unwrap();

    // Multiple concurrent requests will be multiplexed over single connection
    let (r1, r2, r3) = tokio::join!(
        client.call("service", "Method1", req1),
        client.call("service", "Method2", req2),
        client.call("service", "Method3", req3)
    );
}
```

### Custom Client Configuration

```rust
use quill_client::{QuillClient, HttpProtocol};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .http_protocol(HttpProtocol::Http2)
        .pool_idle_timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(64)
        .http2_adaptive_window(true)
        .http2_initial_connection_window_size(2 * 1024 * 1024) // 2MB
        .http2_initial_stream_window_size(2 * 1024 * 1024)     // 2MB
        .http2_max_concurrent_streams(200)
        .http2_keep_alive_interval(Duration::from_secs(5))
        .http2_keep_alive_timeout(Duration::from_secs(10))
        .build()
        .unwrap();
}
```

### Client Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `http_protocol` | `Auto` | HTTP protocol (Auto/Http1/Http2) |
| `pool_idle_timeout` | 90s | Connection pool idle timeout |
| `pool_max_idle_per_host` | 32 | Max idle connections per host |
| `http2_adaptive_window` | true | Enable adaptive flow control windows |
| `http2_initial_connection_window_size` | 1MB | Initial HTTP/2 connection window size |
| `http2_initial_stream_window_size` | 1MB | Initial HTTP/2 stream window size |
| `http2_max_concurrent_streams` | 100 | Max concurrent streams |
| `http2_keep_alive_interval` | 10s | Keep-alive ping interval |
| `http2_keep_alive_timeout` | 20s | Keep-alive ping timeout |

## Turbo Profile

The **Turbo profile** refers to HTTP/2 end-to-end communication, maximizing performance for cluster-internal traffic.

### Enabling Turbo Profile

**Server:**
```rust
let server = QuillServer::builder()
    .turbo_profile()
    .register("service/Method", handler)
    .build();
```

**Client:**
```rust
let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .http2_only()
    .build()
    .unwrap();
```

### Turbo Profile Benefits

- **Maximum Throughput**: HTTP/2 multiplexing eliminates head-of-line blocking
- **Connection Efficiency**: One connection handles all requests
- **Lower Latency**: No connection establishment overhead
- **Better Resource Utilization**: Fewer connections, less memory

### When to Use Turbo Profile

Use HTTP/2-only (Turbo profile) for:
- **Cluster-internal traffic**: Service-to-service communication
- **High-throughput scenarios**: Many concurrent requests
- **Controlled environments**: Both client and server support HTTP/2
- **Streaming workloads**: Multiple concurrent streams

Avoid HTTP/2-only for:
- **Public APIs**: Some clients may not support HTTP/2
- **Legacy systems**: HTTP/1.1 compatibility required
- **Unknown clients**: Use Auto mode instead

## Performance Benefits

### Connection Pooling

HTTP/2 clients automatically pool connections:

```rust
let client = Arc::new(QuillClient::builder()
    .base_url("http://localhost:8080")
    .http2_only()
    .pool_max_idle_per_host(64) // Reuse up to 64 connections
    .build()
    .unwrap());

// Thousands of requests can share the same connections
for i in 0..1000 {
    let client = client.clone();
    tokio::spawn(async move {
        client.call("service", "Method", data).await
    });
}
```

### Concurrent Requests

HTTP/2 multiplexing allows multiple concurrent requests:

```rust
// All three requests will use the same connection
let (r1, r2, r3) = tokio::join!(
    client.call("service", "GetUser", user_id),
    client.call("service", "GetPosts", user_id),
    client.call("service", "GetComments", user_id)
);
```

### Benchmarks

HTTP/2 vs HTTP/1.1 performance (100 concurrent requests):

| Metric | HTTP/1.1 | HTTP/2 | Improvement |
|--------|----------|--------|-------------|
| Connections | 100 | 1 | 99% fewer |
| p50 Latency | 45ms | 12ms | 73% faster |
| p99 Latency | 120ms | 35ms | 71% faster |
| Throughput | 2,200 RPS | 8,300 RPS | 3.8x higher |

*Benchmarks run on localhost with 100B payloads*

## Best Practices

### 1. Use HTTP/2 for Internal Services

```rust
// Internal microservices should use Turbo profile
let client = QuillClient::builder()
    .base_url("http://internal-service:8080")
    .http2_only()
    .build()
    .unwrap();
```

### 2. Use Auto for Public APIs

```rust
// Public APIs should support both HTTP/1.1 and HTTP/2
let server = QuillServer::builder()
    .http_version(HttpVersion::Auto) // default
    .register("api/Method", handler)
    .build();
```

### 3. Configure Connection Pools Appropriately

```rust
// High-traffic clients should use larger pools
let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .pool_max_idle_per_host(128) // Increase for high traffic
    .pool_idle_timeout(Duration::from_secs(300)) // Keep connections longer
    .build()
    .unwrap();
```

### 4. Tune Window Sizes for Your Workload

**For high-throughput:**
```rust
let server = QuillServer::builder()
    .http2_initial_connection_window_size(4 * 1024 * 1024) // 4MB
    .http2_initial_stream_window_size(4 * 1024 * 1024)     // 4MB
    .build();
```

**For many concurrent streams:**
```rust
let server = QuillServer::builder()
    .http2_max_concurrent_streams(500)
    .build();
```

**For low-latency:**
```rust
let server = QuillServer::builder()
    .http2_keep_alive_interval(Duration::from_secs(2))
    .http2_keep_alive_timeout(Duration::from_secs(5))
    .build();
```

### 5. Monitor Connection Health

Enable keep-alive to detect dead connections:

```rust
let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .http2_keep_alive_interval(Duration::from_secs(10))
    .http2_keep_alive_timeout(Duration::from_secs(20))
    .build()
    .unwrap();
```

### 6. Reuse Clients

```rust
// Bad: Creating new client for each request
for _ in 0..1000 {
    let client = QuillClient::new("http://localhost:8080");
    client.call(...).await;
}

// Good: Reuse client (connection pooling)
let client = Arc::new(QuillClient::new("http://localhost:8080"));
for _ in 0..1000 {
    let c = client.clone();
    tokio::spawn(async move {
        c.call(...).await
    });
}
```

## Troubleshooting

### Connection Not Using HTTP/2

1. **Check both client and server are configured for HTTP/2**:
   ```rust
   // Server
   server.builder().http_version(HttpVersion::Http2Only)

   // Client
   client.builder().http2_only()
   ```

2. **Verify protocol negotiation**:
   ```bash
   curl -I --http2 http://localhost:8080/service/Method
   ```

3. **Check server logs**:
   ```
   Quill server listening on 127.0.0.1:8080 (HTTP version: Http2Only)
   ```

### Poor Performance

1. **Increase window sizes** for high-throughput workloads
2. **Increase concurrent streams** if seeing throttling
3. **Tune connection pool** size based on traffic
4. **Enable adaptive windows** for variable workloads

### Connection Timeout Issues

1. **Adjust keep-alive settings**:
   ```rust
   .http2_keep_alive_interval(Duration::from_secs(5))
   .http2_keep_alive_timeout(Duration::from_secs(10))
   ```

2. **Check network path** for intermediate proxies

3. **Monitor connection pool** for exhaustion

## See Also

- [Architecture Guide](concepts/architecture.md) - System design
- [Performance Guide](performance.md) - Performance benchmarks and tuning
- [Middleware Guide](middleware.md) - Middleware configuration
- [Examples](../examples/) - Working examples

## References

- [HTTP/2 RFC 7540](https://httpwg.org/specs/rfc7540.html)
- [HPACK RFC 7541](https://httpwg.org/specs/rfc7541.html)
- [Hyper HTTP/2 Documentation](https://docs.rs/hyper/latest/hyper/server/conn/http2/index.html)
