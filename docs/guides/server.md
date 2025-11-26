# Server Development Guide

Build production-ready Quill RPC servers with middleware, streaming, and observability.

## Basic Server

```rust
use bytes::Bytes;
use quill_core::QuillError;
use quill_server::QuillServer;

async fn echo(request: Bytes) -> Result<Bytes, QuillError> {
    Ok(request)
}

#[tokio::main]
async fn main() {
    let server = QuillServer::builder()
        .register("echo.v1.EchoService/Echo", echo)
        .build();

    server.serve("0.0.0.0:8080".parse().unwrap()).await.unwrap();
}
```

## Handler Types

### Unary Handler

Single request, single response:

```rust
async fn get_user(request: Bytes) -> Result<Bytes, QuillError> {
    let req = GetUserRequest::decode(request)?;
    let user = db.find_user(&req.user_id)?;
    Ok(user.encode_to_vec().into())
}

server.register("users.v1.UserService/GetUser", get_user)
```

### Server Streaming Handler

Single request, multiple responses:

```rust
use quill_server::ServerStreamSender;

async fn list_users(
    request: Bytes,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    let req = ListUsersRequest::decode(request)?;

    for user in db.list_users(&req.filter) {
        sender.send(user.encode_to_vec().into()).await?;
    }

    Ok(())
}

server.register_server_stream(
    "users.v1.UserService/ListUsers",
    list_users
)
```

### Client Streaming Handler

Multiple requests, single response:

```rust
use quill_server::RequestStream;

async fn upload_file(
    mut stream: RequestStream,
) -> Result<Bytes, QuillError> {
    let mut total_bytes = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        storage.write(&chunk)?;
        total_bytes += chunk.len();
    }

    let response = UploadResponse { bytes_written: total_bytes };
    Ok(response.encode_to_vec().into())
}

server.register_client_stream(
    "files.v1.FileService/Upload",
    upload_file
)
```

### Bidirectional Streaming Handler

Multiple requests, multiple responses:

```rust
async fn chat(
    mut stream: RequestStream,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    while let Some(message) = stream.next().await {
        let msg = ChatMessage::decode(message?)?;

        // Broadcast to other clients
        let response = ChatMessage {
            user: msg.user,
            text: format!("Echo: {}", msg.text),
        };
        sender.send(response.encode_to_vec().into()).await?;
    }

    Ok(())
}

server.register_bidi_stream(
    "chat.v1.ChatService/Chat",
    chat
)
```

## Server Configuration

### HTTP Version Selection

```rust
use quill_server::{QuillServer, HttpVersion};

let server = QuillServer::builder()
    .http_version(HttpVersion::Auto)  // Default: negotiate H1/H2
    // .http_version(HttpVersion::Http1Only)
    // .http_version(HttpVersion::Http2Only)
    .build();
```

### HTTP/2 Configuration

```rust
use quill_server::Http2Config;

let http2_config = Http2Config::default()
    .initial_stream_window_size(1024 * 1024)  // 1 MB
    .initial_connection_window_size(2 * 1024 * 1024)  // 2 MB
    .max_concurrent_streams(100)
    .keep_alive_interval(Duration::from_secs(30))
    .keep_alive_timeout(Duration::from_secs(10));

let server = QuillServer::builder()
    .http2_config(http2_config)
    .build();
```

### Timeouts

```rust
let server = QuillServer::builder()
    .request_timeout(Duration::from_secs(30))
    .idle_timeout(Duration::from_secs(60))
    .build();
```

## Middleware

### Authentication

```rust
use quill_server::middleware::AuthMiddleware;

let auth = AuthMiddleware::bearer(|token| async move {
    validate_jwt(token).await
});

let server = QuillServer::builder()
    .middleware(auth)
    .register("api.v1.Service/Method", handler)
    .build();
```

### Rate Limiting

```rust
use quill_server::middleware::RateLimitMiddleware;

let rate_limit = RateLimitMiddleware::new()
    .requests_per_second(100)
    .burst_size(20);

let server = QuillServer::builder()
    .middleware(rate_limit)
    .build();
```

### Compression

```rust
use quill_server::middleware;

async fn handler(request: Bytes) -> Result<Bytes, QuillError> {
    // Decompress request if needed
    let request = middleware::decompress_request(&headers, request)?;

    // ... process request ...

    // Compress response
    let (response, content_encoding) =
        middleware::compress_response(&accept_encoding, response)?;

    Ok(response)
}
```

### Tracing

```rust
use quill_server::middleware;
use tracing::instrument;

#[instrument(skip(request))]
async fn handler(request: Bytes) -> Result<Bytes, QuillError> {
    // Extract trace context from headers
    let context = middleware::extract_trace_context(&headers);

    // Create span for this RPC
    let span = middleware::create_rpc_span(
        "UserService",
        "GetUser",
        context,
    );

    // ... handle request ...
}
```

### Request Logging

```rust
use quill_server::middleware::LoggingMiddleware;

let logging = LoggingMiddleware::new()
    .log_request_headers(true)
    .log_response_headers(true)
    .sanitize_headers(&["authorization", "cookie"]);

let server = QuillServer::builder()
    .middleware(logging)
    .build();
```

## Error Handling

Return structured Problem Details errors:

```rust
use quill_core::{QuillError, ProblemDetails};

async fn handler(request: Bytes) -> Result<Bytes, QuillError> {
    let req = Request::decode(request)
        .map_err(|_| QuillError::invalid_argument("Invalid request format"))?;

    if req.name.is_empty() {
        return Err(QuillError::invalid_argument("name is required"));
    }

    let user = db.find_user(&req.user_id)
        .ok_or_else(|| QuillError::not_found(
            format!("User '{}' not found", req.user_id)
        ))?;

    Ok(user.encode_to_vec().into())
}
```

## Health Checks

```rust
use quill_server::health::{HealthCheck, HealthStatus};

let health = HealthCheck::new()
    .add_check("database", || async {
        if db.ping().await.is_ok() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy("Database connection failed".into())
        }
    })
    .add_check("cache", || async {
        if cache.ping().await.is_ok() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded("Cache unavailable".into())
        }
    });

let server = QuillServer::builder()
    .health_check(health)
    .build();
```

## Graceful Shutdown

```rust
use tokio::signal;

#[tokio::main]
async fn main() {
    let server = QuillServer::builder()
        .register("service/method", handler)
        .build();

    let addr = "0.0.0.0:8080".parse().unwrap();

    // Spawn server
    let server_handle = tokio::spawn(async move {
        server.serve(addr).await
    });

    // Wait for shutdown signal
    signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    println!("Shutting down...");

    // Graceful shutdown with timeout
    tokio::time::timeout(
        Duration::from_secs(30),
        server_handle,
    ).await.ok();
}
```

## Observability

### Metrics

```rust
use quill_server::observability::ObservabilityCollector;

let collector = ObservabilityCollector::new("my_service");

// Expose Prometheus metrics endpoint
server.register("metrics", |_| async {
    Ok(collector.prometheus_metrics().into())
});
```

### Structured Logging

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().json())
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

## Complete Example

```rust
use bytes::Bytes;
use quill_core::QuillError;
use quill_server::{QuillServer, HttpVersion, Http2Config};
use quill_server::middleware::{AuthMiddleware, RateLimitMiddleware};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Configure middleware
    let auth = AuthMiddleware::bearer(validate_token);
    let rate_limit = RateLimitMiddleware::new()
        .requests_per_second(1000)
        .burst_size(100);

    // Build server
    let server = QuillServer::builder()
        .http_version(HttpVersion::Auto)
        .http2_config(
            Http2Config::default()
                .max_concurrent_streams(100)
        )
        .request_timeout(Duration::from_secs(30))
        .middleware(auth)
        .middleware(rate_limit)
        .register("users.v1.UserService/GetUser", get_user)
        .register("users.v1.UserService/CreateUser", create_user)
        .register_server_stream("users.v1.UserService/ListUsers", list_users)
        .build();

    println!("Server listening on 0.0.0.0:8080");
    server.serve("0.0.0.0:8080".parse()?).await?;

    Ok(())
}
```

## Next Steps

- [Client Development](client.md) - Build Quill clients
- [Streaming Guide](streaming.md) - Streaming patterns in depth
- [Middleware](../middleware.md) - Complete middleware reference
- [Observability](../observability.md) - Metrics and tracing
