# Quill

A modern, protobuf-first RPC framework with adaptive HTTP/1–3 transport (Prism), real HTTP errors, streaming that just works, and a single CLI for gen, call, and bench.

## Overview

Quill is a Rust implementation of a next-generation RPC framework featuring:

- **Prism Transport Profiles**: Adaptive HTTP/1.1, HTTP/2 (Turbo), and HTTP/3 (Hyper) support
- **Protobuf-First**: `.proto` as the single source of truth
- **Real HTTP Errors**: Problem Details (RFC 7807) instead of 200-with-error-envelope
- **Stream Framing**: Custom framing with flow control for efficient streaming
- **Zero Trailers**: No trailers required for correctness

## Architecture

### Crates

- **quill-core**: Core types (framing, Problem Details, Prism profiles)
- **quill-proto**: Protobuf integration and Quill annotations
- **quill-transport**: Transport layer implementations (Classic/Turbo/Hyper)
- **quill-server**: Server SDK with routing and middleware
- **quill-client**: Client SDK with retry and backpressure
- **quill-codegen**: Code generation (protoc plugin) - *coming soon*
- **quill-cli**: CLI tool for gen/call/bench - *coming soon*

### Prism Transport Profiles

1. **Classic** (HTTP/1.1 + basic HTTP/2): Legacy/enterprise proxies
2. **Turbo** (HTTP/2 end-to-end): Cluster-internal traffic
3. **Hyper** (HTTP/3 over QUIC): Browser/mobile, lossy networks - *coming soon*

Profile negotiation via `Prefer: prism=hyper,turbo,classic` header.

## Quick Start

See the [Echo example](examples/echo/) for a complete working example.

### Server

```rust
use quill_server::QuillServer;
use bytes::Bytes;
use quill_core::QuillError;

async fn handle_echo(request: Bytes) -> Result<Bytes, QuillError> {
    // Decode request, process, encode response
    Ok(request) // Echo back
}

#[tokio::main]
async fn main() {
    let server = QuillServer::builder()
        .register("echo.v1.EchoService/Echo", handle_echo)
        .build();

    server.serve("127.0.0.1:8080".parse().unwrap()).await.unwrap();
}
```

### Client

```rust
use quill_client::QuillClient;
use bytes::Bytes;

#[tokio::main]
async fn main() {
    let client = QuillClient::builder()
        .base_url("http://127.0.0.1:8080")
        .build()
        .unwrap();

    let response = client
        .call("echo.v1.EchoService", "Echo", Bytes::from("Hello"))
        .await
        .unwrap();

    println!("Response: {:?}", response);
}
```

## Development

### Build

```bash
cargo build --workspace
```

### Test

```bash
cargo test --workspace
```

### Run Echo Example

```bash
cargo test --package echo-example
```

## Project Status

**Phase 1: Foundation - "Hello Quill" Milestone**

- [x] Workspace structure
- [x] quill-core implementation (framing, Problem Details, profiles)
- [x] quill-proto with annotations
- [x] quill-transport (HTTP/2 Turbo profile)
- [x] quill-server (router, handlers, middleware)
- [x] quill-client (unary calls)
- [x] Echo example with integration test
- [x] GitHub CI workflow

### Next Steps

- [ ] Add streaming support (server/client/bidi)
- [ ] Add middleware (auth, compression, tracing)
- [ ] Implement code generation (quill-codegen)
- [ ] Build CLI tool (quill-cli)
- [ ] Add HTTP/3 Hyper profile support
- [ ] Performance benchmarks

## License

MIT

