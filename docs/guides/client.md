# Client Development Guide

Build robust Quill RPC clients with connection pooling, retries, and streaming.

## Basic Client

```rust
use quill_client::QuillClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = QuillClient::builder()
        .base_url("http://api.example.com")
        .build()?;

    let response = client
        .call("echo.v1.EchoService/Echo", b"Hello!".to_vec().into())
        .await?;

    println!("Response: {}", String::from_utf8_lossy(&response));
    Ok(())
}
```

## Client Configuration

### Timeouts

```rust
use std::time::Duration;

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(5))
    .build()?;
```

### Transport Profile Selection

```rust
use quill_core::PrismProfile;

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .prefer_profiles(&[
        PrismProfile::Hyper,   // HTTP/3 preferred
        PrismProfile::Turbo,   // HTTP/2 fallback
        PrismProfile::Classic, // HTTP/1.1 fallback
    ])
    .build()?;
```

### Compression

```rust
let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .enable_compression(true)  // Enable zstd compression
    .build()?;
```

### HTTP/2 Settings

```rust
use quill_client::Http2Config;

let http2_config = Http2Config::default()
    .initial_stream_window_size(1024 * 1024)
    .initial_connection_window_size(2 * 1024 * 1024)
    .keep_alive_interval(Duration::from_secs(30))
    .keep_alive_timeout(Duration::from_secs(10))
    .adaptive_window(true);

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .http2_config(http2_config)
    .build()?;
```

### Connection Pooling

```rust
use quill_client::PoolConfig;

let pool_config = PoolConfig::default()
    .max_idle_connections(10)
    .idle_timeout(Duration::from_secs(90))
    .max_connections_per_host(20);

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .pool_config(pool_config)
    .build()?;
```

## Call Types

### Unary Call

Single request, single response:

```rust
use prost::Message;

let request = GetUserRequest { user_id: "123".into() };
let response = client
    .call("users.v1.UserService/GetUser", request.encode_to_vec().into())
    .await?;

let user = User::decode(&response[..])?;
println!("User: {}", user.name);
```

### Server Streaming

Single request, multiple responses:

```rust
let request = ListUsersRequest { limit: 100 };
let mut stream = client
    .call_server_streaming(
        "users.v1.UserService/ListUsers",
        request.encode_to_vec().into(),
    )
    .await?;

while let Some(response) = stream.next().await {
    let user = User::decode(&response?[..])?;
    println!("User: {}", user.name);
}
```

### Client Streaming

Multiple requests, single response:

```rust
let sender = client
    .call_client_streaming("files.v1.FileService/Upload")
    .await?;

// Send file chunks
for chunk in file.chunks(64 * 1024) {
    let request = UploadChunk { data: chunk.to_vec() };
    sender.send(request.encode_to_vec().into()).await?;
}

// Complete and get response
let response = sender.finish().await?;
let result = UploadResponse::decode(&response[..])?;
println!("Uploaded {} bytes", result.bytes_written);
```

### Bidirectional Streaming

Multiple requests, multiple responses:

```rust
let (sender, mut receiver) = client
    .call_bidi_streaming("chat.v1.ChatService/Chat")
    .await?;

// Spawn receiver task
let receive_task = tokio::spawn(async move {
    while let Some(response) = receiver.next().await {
        let msg = ChatMessage::decode(&response?[..])?;
        println!("{}: {}", msg.user, msg.text);
    }
    Ok::<_, QuillError>(())
});

// Send messages
for line in stdin.lines() {
    let msg = ChatMessage { user: "me".into(), text: line? };
    sender.send(msg.encode_to_vec().into()).await?;
}

sender.finish().await?;
receive_task.await??;
```

## Resilience

### Retry Policies

```rust
use quill_client::{RetryPolicy, BackoffConfig};

let retry_policy = RetryPolicy::new()
    .max_retries(3)
    .backoff(BackoffConfig::exponential(
        Duration::from_millis(100),  // Initial delay
        Duration::from_secs(10),     // Max delay
        2.0,                          // Multiplier
    ))
    .jitter(0.1)  // 10% jitter
    .retry_on_status(&[503, 429]);  // Retry these status codes

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .retry_policy(retry_policy)
    .build()?;
```

### Circuit Breaker

```rust
use quill_client::CircuitBreaker;

let circuit_breaker = CircuitBreaker::new()
    .failure_threshold(5)           // Open after 5 failures
    .success_threshold(3)           // Close after 3 successes
    .timeout(Duration::from_secs(30))  // Half-open timeout
    .rolling_window(Duration::from_secs(60));

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .circuit_breaker(circuit_breaker)
    .build()?;
```

### Combined Resilience

```rust
let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .retry_policy(RetryPolicy::default())
    .circuit_breaker(CircuitBreaker::default())
    .timeout(Duration::from_secs(30))
    .build()?;
```

## Authentication

### Bearer Token

```rust
let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .bearer_token("your-jwt-token")
    .build()?;
```

### API Key

```rust
let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .header("X-API-Key", "your-api-key")
    .build()?;
```

### Custom Headers

```rust
let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .header("X-Request-ID", "unique-id")
    .header("X-Client-Version", "1.0.0")
    .build()?;
```

## Error Handling

```rust
use quill_core::QuillError;

match client.call("service/method", request).await {
    Ok(response) => {
        // Handle success
    }
    Err(QuillError::NotFound(detail)) => {
        println!("Not found: {}", detail);
    }
    Err(QuillError::PermissionDenied(detail)) => {
        println!("Access denied: {}", detail);
    }
    Err(QuillError::RateLimited(retry_after)) => {
        println!("Rate limited, retry after {:?}", retry_after);
    }
    Err(QuillError::Timeout) => {
        println!("Request timed out");
    }
    Err(QuillError::CircuitOpen) => {
        println!("Circuit breaker is open");
    }
    Err(e) => {
        println!("Error: {}", e);
    }
}
```

## Tracing

```rust
use tracing::instrument;

#[instrument(skip(client))]
async fn get_user(client: &QuillClient, user_id: &str) -> Result<User, QuillError> {
    let request = GetUserRequest { user_id: user_id.into() };

    let response = client
        .call("users.v1.UserService/GetUser", request.encode_to_vec().into())
        .await?;

    Ok(User::decode(&response[..])?)
}
```

## Complete Example

```rust
use bytes::Bytes;
use prost::Message;
use quill_client::{QuillClient, RetryPolicy, CircuitBreaker, Http2Config};
use quill_core::PrismProfile;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Build client with full configuration
    let client = QuillClient::builder()
        .base_url("http://api.example.com")
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .prefer_profiles(&[PrismProfile::Turbo, PrismProfile::Classic])
        .http2_config(Http2Config::default().adaptive_window(true))
        .enable_compression(true)
        .retry_policy(
            RetryPolicy::new()
                .max_retries(3)
                .retry_on_status(&[503, 429])
        )
        .circuit_breaker(
            CircuitBreaker::new()
                .failure_threshold(5)
                .timeout(Duration::from_secs(30))
        )
        .bearer_token(std::env::var("API_TOKEN")?)
        .build()?;

    // Make RPC call
    let request = GetUserRequest { user_id: "123".into() };
    let response = client
        .call("users.v1.UserService/GetUser", request.encode_to_vec().into())
        .await?;

    let user = User::decode(&response[..])?;
    println!("User: {} ({})", user.name, user.email);

    Ok(())
}
```

## Next Steps

- [Server Development](server.md) - Build Quill servers
- [Streaming Guide](streaming.md) - Streaming patterns
- [Resilience](../resilience.md) - Retry and circuit breaker patterns
- [HTTP/2 Guide](../http2.md) - HTTP/2 configuration
