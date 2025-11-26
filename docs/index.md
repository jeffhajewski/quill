---
hide:
  - navigation
  - toc
---

# Quill

<div class="hero" markdown>

## Modern RPC for the Real World

A **protobuf-first RPC framework** with adaptive HTTP/1-3 transport, real HTTP errors, streaming, and a unified CLI.

[Get Started](getting-started/quickstart.md){ .md-button .md-button--primary }
[View on GitHub](https://github.com/quill/quill){ .md-button }

</div>

---

<div class="grid cards" markdown>

-   :material-lightning-bolt:{ .lg .middle } **Adaptive Transport (Prism)**

    ---

    Negotiate HTTP/1.1, HTTP/2, or HTTP/3 per-hop. Edge uses H3 for mobile, interior uses H2 for efficiency.

    [:octicons-arrow-right-24: Transport Profiles](concepts/transport-profiles.md)

-   :material-alert-circle-outline:{ .lg .middle } **Real HTTP Errors**

    ---

    No `200 OK` with error envelopes. Proper status codes + Problem Details (RFC 7807) for structured errors.

    [:octicons-arrow-right-24: Error Handling](concepts/error-handling.md)

-   :material-source-branch:{ .lg .middle } **Streaming Built-In**

    ---

    Server streaming, client streaming, and bidirectional streaming with credit-based flow control.

    [:octicons-arrow-right-24: Streaming Guide](guides/streaming.md)

-   :material-code-braces:{ .lg .middle } **Protobuf-First**

    ---

    `.proto` is the source of truth. Generate type-safe clients and servers for Go, TypeScript, Rust, and Python.

    [:octicons-arrow-right-24: First Service](getting-started/first-service.md)

</div>

---

## Quick Example

=== "Server (Rust)"

    ```rust
    use quill_server::QuillServer;
    use bytes::Bytes;

    async fn greet(request: Bytes) -> Result<Bytes, QuillError> {
        let req = GreetRequest::decode(request)?;
        let response = GreetResponse {
            message: format!("Hello, {}!", req.name),
        };
        Ok(response.encode_to_vec().into())
    }

    #[tokio::main]
    async fn main() {
        QuillServer::builder()
            .register("greeter.v1.Greeter/Greet", greet)
            .build()
            .serve("0.0.0.0:8080".parse().unwrap())
            .await
            .unwrap();
    }
    ```

=== "Client (Rust)"

    ```rust
    use quill_client::QuillClient;

    #[tokio::main]
    async fn main() -> Result<(), Box<dyn std::error::Error>> {
        let client = QuillClient::builder()
            .base_url("http://localhost:8080")
            .build()?;

        let request = GreetRequest { name: "World".into() };
        let response = client
            .call("greeter.v1.Greeter/Greet", request.encode_to_vec().into())
            .await?;

        let greet = GreetResponse::decode(&response[..])?;
        println!("{}", greet.message);
        Ok(())
    }
    ```

=== "CLI"

    ```bash
    quill call http://localhost:8080/greeter.v1.Greeter/Greet \
      --in '{"name": "World"}'

    # Output: {"message": "Hello, World!"}
    ```

---

## Transport Profiles (Prism)

Quill adapts to your network environment automatically.

| Profile | Protocol | Best For |
|---------|----------|----------|
| **Classic** | HTTP/1.1 + H2 | Enterprise proxies, legacy networks |
| **Turbo** | HTTP/2 end-to-end | Internal cluster traffic |
| **Hyper** | HTTP/3 over QUIC | Browser/mobile, lossy networks |

```
Browser ──[HTTP/3]──► Edge ──[HTTP/2]──► Services
         Hyper            Turbo
```

[:octicons-arrow-right-24: Learn more about Transport Profiles](concepts/transport-profiles.md)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                       Application                           │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐              ┌─────────────────┐       │
│  │   quill-server  │              │   quill-client  │       │
│  │   Server SDK    │              │   Client SDK    │       │
│  └────────┬────────┘              └────────┬────────┘       │
│           └────────────┬───────────────────┘                │
│                        │                                    │
│               ┌────────▼────────┐                           │
│               │   quill-core    │                           │
│               │  Core Types     │                           │
│               └────────┬────────┘                           │
│                        │                                    │
│               ┌────────▼────────┐                           │
│               │ quill-transport │                           │
│               │ Classic│Turbo│  │                           │
│               │      Hyper      │                           │
│               └─────────────────┘                           │
└─────────────────────────────────────────────────────────────┘
```

[:octicons-arrow-right-24: Architecture Overview](concepts/architecture.md)

---

## Design Principles

<div class="grid" markdown>

!!! info "Protobuf-First"
    `.proto` is the single source of truth. No code-first, no schema drift.

!!! success "Real HTTP Errors"
    Proper status codes. Never `200 OK` with an error envelope.

!!! tip "No Trailers Required"
    Works through any proxy. No HTTP/2 trailer dependency.

!!! example "Progressive Enhancement"
    HTTP/3 is optional. Gracefully degrades to HTTP/2 or HTTP/1.1.

</div>

---

## Get Started

<div class="grid cards" markdown>

-   :material-rocket-launch:{ .lg } **Quick Start**

    Get up and running in 5 minutes with a simple echo server.

    [:octicons-arrow-right-24: Quick Start](getting-started/quickstart.md)

-   :material-download:{ .lg } **Installation**

    Install the Rust crates, CLI tool, or Python bindings.

    [:octicons-arrow-right-24: Installation](getting-started/installation.md)

-   :material-book-open-variant:{ .lg } **First Service**

    Build a complete service with protobuf and streaming.

    [:octicons-arrow-right-24: First Service](getting-started/first-service.md)

</div>
