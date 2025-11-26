# Quick Start

Get up and running with Quill in under 5 minutes.

## Prerequisites

- Rust 1.75 or later
- Cargo

## Create a New Project

```bash
cargo new quill-demo
cd quill-demo
```

Add Quill to `Cargo.toml`:

```toml
[dependencies]
quill-server = "0.1"
quill-client = "0.1"
quill-core = "0.1"
tokio = { version = "1", features = ["full"] }
bytes = "1"
```

## Create an Echo Server

Replace `src/main.rs`:

```rust
use bytes::Bytes;
use quill_core::QuillError;
use quill_server::QuillServer;

async fn echo(request: Bytes) -> Result<Bytes, QuillError> {
    Ok(request)
}

#[tokio::main]
async fn main() {
    println!("Starting Echo Server on http://127.0.0.1:8080");

    let server = QuillServer::builder()
        .register("echo.v1.EchoService/Echo", echo)
        .build();

    server.serve("127.0.0.1:8080".parse().unwrap()).await.unwrap();
}
```

## Run the Server

```bash
cargo run
```

## Test with curl

```bash
curl -X POST http://127.0.0.1:8080/echo.v1.EchoService/Echo \
  -H "Content-Type: application/proto" \
  -d "Hello, Quill!"
```

## Using the Quill CLI

```bash
cargo install --path crates/quill-cli

quill call http://127.0.0.1:8080/echo.v1.EchoService/Echo \
  --input "Hello from CLI!"
```

## Add a Client

Create `src/client.rs`:

```rust
use quill_client::QuillClient;

#[tokio::main]
async fn main() {
    let client = QuillClient::builder()
        .base_url("http://127.0.0.1:8080")
        .build()
        .unwrap();

    let response = client
        .call("echo.v1.EchoService/Echo", b"Hello!".to_vec().into())
        .await
        .unwrap();

    println!("Response: {}", String::from_utf8_lossy(&response));
}
```

## Next Steps

- [Installation](installation.md) - Full installation guide
- [Your First Service](first-service.md) - Build a complete service with protobuf
- [Server Guide](../guides/server.md) - Deep dive into server development
