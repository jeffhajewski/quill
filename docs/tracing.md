# OpenTelemetry Tracing in Quill

## Overview

Quill provides built-in OpenTelemetry tracing support for observability of RPC calls. Traces are automatically created for all RPC operations following OpenTelemetry semantic conventions for RPC systems.

## Features

- **Automatic instrumentation**: All RPC calls are automatically traced
- **Distributed tracing**: W3C Trace Context propagation via HTTP headers
- **Semantic conventions**: Follows OpenTelemetry RPC semantic conventions
- **Streaming support**: Traces client, server, and bidirectional streaming
- **Rich attributes**: Service, method, compression, and status information

## Client-Side Tracing

### Automatic Instrumentation

The Quill client automatically creates spans for all RPC calls:

```rust
use quill_client::QuillClient;
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt::init();

    let client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .build()?;

    // This call is automatically traced
    let request = Bytes::from("hello");
    let response = client
        .call("echo.v1.EchoService", "Echo", request)
        .await?;

    Ok(())
}
```

### Span Attributes

Client spans include the following attributes:

- `rpc.service` - The service name (e.g., "echo.v1.EchoService")
- `rpc.method` - The method name (e.g., "Echo")
- `rpc.system` - Always "quill"
- `otel.kind` - Always "client"
- `rpc.streaming` - Type of streaming ("client", "server", "bidirectional") for streaming calls

### Streaming Calls

All streaming modes are automatically instrumented:

```rust
// Client streaming
let stream = /* create stream */;
let response = client
    .call_client_streaming("upload.v1.UploadService", "Upload", stream)
    .await?;

// Server streaming
let mut response_stream = client
    .call_server_streaming("log.v1.LogService", "Tail", request)
    .await?;

// Bidirectional streaming
let response_stream = client
    .call_bidi_streaming("chat.v1.ChatService", "Chat", request_stream)
    .await?;
```

Each streaming mode adds an `rpc.streaming` attribute to identify the type.

## Server-Side Tracing

### Creating Spans

The server provides utilities to create spans for RPC handlers:

```rust
use quill_server::middleware::{create_rpc_span, record_rpc_result};
use tracing::Instrument;

async fn handle_echo(request: Bytes) -> Result<RpcResponse, QuillError> {
    // Create a span for this RPC call
    let span = create_rpc_span("echo.v1.EchoService", "Echo");

    async move {
        // Your handler logic
        let response = process_request(request)?;

        // Record success
        record_rpc_result(&span, true, None);

        Ok(RpcResponse::unary(response))
    }
    .instrument(span)
    .await
}
```

### Extracting Trace Context

Extract distributed trace context from incoming requests:

```rust
use quill_server::middleware::extract_trace_context;

async fn handle_request(req: Request<Incoming>) -> Result<Response<_>, QuillError> {
    // Extract trace context from headers
    let trace_context = extract_trace_context(&req);

    // The context includes:
    // - traceparent (W3C Trace Context)
    // - tracestate (vendor-specific trace state)
    // - baggage (cross-cutting concerns)

    // Process request...
    Ok(response)
}
```

### Recording Attributes

Add additional attributes to spans:

```rust
use quill_server::middleware::record_rpc_attributes;
use tracing::Span;

let span = Span::current();
record_rpc_attributes(&span, "myservice.v1.MyService", "MyMethod", true);
```

### Recording Results

Record the outcome of RPC calls:

```rust
use quill_server::middleware::record_rpc_result;

match handle_request(request).await {
    Ok(response) => {
        record_rpc_result(&span, true, None);
        Ok(response)
    }
    Err(e) => {
        record_rpc_result(&span, false, Some(&e.to_string()));
        Err(e)
    }
}
```

## OpenTelemetry Setup

### Basic Setup with Console Exporter

For development, use the console exporter to print traces:

```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

fn init_tracing() {
    let subscriber = Registry::default()
        .with(tracing_subscriber::fmt::layer());

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set subscriber");
}
```

### Production Setup with OTLP

For production, export to an OpenTelemetry collector:

```toml
[dependencies]
opentelemetry = "0.24"
opentelemetry-otlp = { version = "0.17", features = ["tonic"] }
opentelemetry_sdk = { version = "0.24", features = ["rt-tokio", "trace"] }
tracing-opentelemetry = "0.25"
tracing-subscriber = "0.3"
```

```rust
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure OTLP exporter
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://localhost:4317")
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    // Create OpenTelemetry layer
    let telemetry = tracing_opentelemetry::layer()
        .with_tracer(tracer.tracer("quill-server"));

    // Combine with fmt layer for console logging
    let subscriber = Registry::default()
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer());

    tracing::subscriber::set_global_default(subscriber)?;

    // Your application code
    run_server().await?;

    // Shutdown tracer to flush remaining spans
    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}
```

## Distributed Tracing

### W3C Trace Context

Quill supports W3C Trace Context propagation via HTTP headers:

- `traceparent`: Contains trace ID, span ID, and trace flags
- `tracestate`: Vendor-specific trace state
- `baggage`: Cross-cutting concerns (user ID, session ID, etc.)

### Trace Context Flow

```
Client                          Server
  |                              |
  | HTTP Request                 |
  | traceparent: 00-xxx-yyy-01   |
  | tracestate: vendor=value     |
  |---------------------------->|
  |                              |
  |                              | (Extract context)
  |                              | (Create child span)
  |                              | (Process request)
  |                              |
  | HTTP Response                |
  |<-----------------------------|
  |                              |
```

### Example: Full Distributed Trace

```rust
// Service A (client)
async fn call_service_b() -> Result<(), Box<dyn std::error::Error>> {
    let client = QuillClient::builder()
        .base_url("http://service-b:8080")
        .build()?;

    // Automatically injects traceparent header
    let response = client
        .call("serviceB.v1.ServiceB", "Process", request)
        .await?;

    Ok(())
}

// Service B (server)
async fn handle_process(req: Request<Incoming>) -> Result<Response<_>, QuillError> {
    // Extract parent trace context
    let trace_context = extract_trace_context(&req);

    // Create span as child of parent trace
    let span = create_rpc_span("serviceB.v1.ServiceB", "Process");

    // Process request within span
    async {
        // Handler logic
        Ok(response)
    }
    .instrument(span)
    .await
}
```

## Semantic Conventions

Quill follows [OpenTelemetry RPC Semantic Conventions](https://opentelemetry.io/docs/specs/semconv/rpc/rpc-spans/):

### Required Attributes

- `rpc.system` - RPC system (always "quill")
- `rpc.service` - Full service name
- `rpc.method` - Method name

### Optional Attributes

- `rpc.compression` - Compression algorithm ("zstd")
- `rpc.streaming` - Streaming type ("client", "server", "bidirectional")
- `rpc.status` - Call status ("ok", "error")
- `rpc.error` - Error message (if failed)

### Span Naming

Spans are named "rpc.request" with attributes providing context.

## Integration with Observability Platforms

### Jaeger

```bash
# Start Jaeger with Docker
docker run -d --name jaeger \
  -p 4317:4317 \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest

# Configure OTLP endpoint
OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# View traces at http://localhost:16686
```

### Grafana Tempo

```yaml
# tempo.yaml
server:
  http_listen_port: 3200

distributor:
  receivers:
    otlp:
      protocols:
        grpc:
          endpoint: 0.0.0.0:4317

storage:
  trace:
    backend: local
    local:
      path: /tmp/tempo/traces
```

### Honeycomb

```rust
let tracer = opentelemetry_otlp::new_pipeline()
    .tracing()
    .with_exporter(
        opentelemetry_otlp::new_exporter()
            .http()
            .with_endpoint("https://api.honeycomb.io")
            .with_headers(std::collections::HashMap::from([
                ("x-honeycomb-team".to_string(), "YOUR_API_KEY".to_string()),
                ("x-honeycomb-dataset".to_string(), "quill".to_string()),
            ]))
    )
    .install_batch(opentelemetry_sdk::runtime::Tokio)?;
```

## Best Practices

### 1. Always Initialize Tracing

Initialize tracing at application startup:

```rust
#[tokio::main]
async fn main() {
    init_tracing();
    // Your application
}
```

### 2. Add Custom Attributes

Enhance spans with business context:

```rust
use tracing::info_span;

let span = info_span!(
    "process_order",
    order.id = order_id,
    user.id = user_id,
    payment.method = "credit_card"
);
```

### 3. Record Errors

Always record error information:

```rust
use tracing::error;

match result {
    Ok(_) => {}
    Err(e) => {
        error!(error = %e, "Request failed");
        record_rpc_result(&span, false, Some(&e.to_string()));
    }
}
```

### 4. Use Sampling

In high-throughput services, use sampling to reduce overhead:

```rust
use opentelemetry_sdk::trace::Sampler;

let tracer = opentelemetry_otlp::new_pipeline()
    .tracing()
    .with_trace_config(
        opentelemetry_sdk::trace::config()
            .with_sampler(Sampler::TraceIdRatioBased(0.1)) // Sample 10%
    )
    .install_batch(opentelemetry_sdk::runtime::Tokio)?;
```

### 5. Flush on Shutdown

Always flush traces before shutdown:

```rust
// Shutdown tracer provider
opentelemetry::global::shutdown_tracer_provider();
```

## Troubleshooting

### No Traces Appearing

1. Check subscriber initialization:
   ```rust
   tracing::subscriber::set_global_default(subscriber)?;
   ```

2. Verify OTLP endpoint is reachable:
   ```bash
   curl http://localhost:4317
   ```

3. Check sampling configuration

### Missing Span Attributes

Ensure you're calling `record_rpc_attributes`:

```rust
record_rpc_attributes(&span, service, method, compressed);
```

### Broken Trace Context

Verify headers are being propagated:

```rust
let trace_context = extract_trace_context(&req);
assert!(trace_context.contains_key("traceparent"));
```

## Performance Considerations

- **Overhead**: ~1-5% CPU overhead with OTLP export
- **Sampling**: Use sampling in production (10-50%)
- **Batching**: Spans are batched before export (default 512)
- **Async export**: Traces exported asynchronously, not blocking RPCs

## See Also

- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [W3C Trace Context](https://www.w3.org/TR/trace-context/)
- [RPC Semantic Conventions](https://opentelemetry.io/docs/specs/semconv/rpc/rpc-spans/)
- `crates/quill-server/src/middleware.rs` - Server tracing utilities
- `crates/quill-client/src/client.rs` - Client instrumentation
