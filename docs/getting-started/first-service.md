# Your First Service

Build a complete Quill service with protobuf and streaming.

## Define Your Service

Create `proto/greeter.proto`:

```protobuf
syntax = "proto3";
package greeter.v1;

service Greeter {
  rpc SayHello (HelloRequest) returns (HelloResponse);
  rpc SayHelloStream (HelloRequest) returns (stream HelloResponse);
}

message HelloRequest {
  string name = 1;
  int32 count = 2;
}

message HelloResponse {
  string message = 1;
}
```

## Code Generation

Add `build.rs`:

```rust
fn main() {
    quill_codegen::Config::new()
        .proto_path("proto")
        .compile(&["proto/greeter.proto"])
        .unwrap();
}
```

## Server Implementation

```rust
use bytes::Bytes;
use prost::Message;
use quill_core::QuillError;
use quill_server::{QuillServer, ServerStreamSender};

pub mod greeter {
    include!(concat!(env!("OUT_DIR"), "/greeter.v1.rs"));
}

use greeter::{HelloRequest, HelloResponse};

async fn say_hello(request: Bytes) -> Result<Bytes, QuillError> {
    let req = HelloRequest::decode(request)?;
    let response = HelloResponse {
        message: format!("Hello, {}!", req.name),
    };
    Ok(response.encode_to_vec().into())
}

async fn say_hello_stream(
    request: Bytes,
    sender: ServerStreamSender,
) -> Result<(), QuillError> {
    let req = HelloRequest::decode(request)?;

    for i in 0..req.count {
        let response = HelloResponse {
            message: format!("Hello #{} to {}!", i + 1, req.name),
        };
        sender.send(response.encode_to_vec().into()).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let server = QuillServer::builder()
        .register("greeter.v1.Greeter/SayHello", say_hello)
        .register_server_stream("greeter.v1.Greeter/SayHelloStream", say_hello_stream)
        .build();

    server.serve("127.0.0.1:8080".parse().unwrap()).await.unwrap();
}
```

## Client Usage

```rust
let client = QuillClient::builder()
    .base_url("http://127.0.0.1:8080")
    .build()?;

// Unary call
let response = client
    .call("greeter.v1.Greeter/SayHello", request.encode_to_vec().into())
    .await?;

// Streaming call
let mut stream = client
    .call_server_streaming("greeter.v1.Greeter/SayHelloStream", request.encode_to_vec().into())
    .await?;

while let Some(response) = stream.next().await {
    println!("Received: {:?}", response?);
}
```

## CLI Usage

```bash
quill call http://127.0.0.1:8080/greeter.v1.Greeter/SayHello \
  --input '{"name": "World"}'

quill call http://127.0.0.1:8080/greeter.v1.Greeter/SayHelloStream \
  --input '{"name": "Streamer", "count": 5}' \
  --stream
```

## Next Steps

- [Streaming Guide](../guides/streaming.md) - All streaming patterns
- [Server Guide](../guides/server.md) - Middleware and configuration
- [Error Handling](../concepts/error-handling.md) - Problem Details
