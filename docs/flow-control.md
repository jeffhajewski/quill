# Flow Control in Quill

## Overview

Quill implements credit-based flow control to prevent buffer overflow in streaming RPCs. This ensures that fast senders don't overwhelm slow receivers.

## Architecture

### Credit Frames

Credit frames (flag bit 3) are used to grant permission to send messages. The payload of a credit frame contains a varint-encoded u32 representing the number of credits being granted.

```rust
// Create a credit frame granting 16 credits
let frame = Frame::credit(16);

// Decode credits from a frame
if let Some(credits) = frame.decode_credit() {
    println!("Received {} credits", credits);
}
```

### Credit Tracker

The `CreditTracker` provides thread-safe credit tracking using atomic operations:

```rust
use quill_core::CreditTracker;

// Create tracker with default credits (16)
let tracker = CreditTracker::with_defaults();

// Try to consume a credit before sending
if tracker.try_consume() {
    // Send message
    println!("Sent message");
} else {
    println!("No credits available, waiting...");
}

// Grant more credits (from receiver)
tracker.grant(8);
```

## Flow Control in Different Streaming Modes

### Server Streaming

In server streaming, the client receives a stream of messages from the server:

1. **Server** (sender) tracks send credits
2. **Client** (receiver) grants credits by sending CREDIT frames
3. Client grants initial credits when starting the stream
4. Client grants more credits as it consumes messages

**Current Implementation**: The client tracks received messages and logs when it would grant credits. In a future HTTP/2 implementation, the client will send actual CREDIT frames back to the server.

### Client Streaming

In client streaming, the server receives a stream of messages from the client:

1. **Client** (sender) tracks send credits
2. **Server** (receiver) grants credits by sending CREDIT frames
3. Server grants initial credits when accepting the stream
4. Server grants more credits as it consumes messages

**Current Implementation**: The server tracks received messages and logs when it would grant credits. The current implementation buffers all client messages before sending, so credits aren't enforced yet.

### Bidirectional Streaming

In bidirectional streaming, both sides act as sender and receiver:

1. Each side maintains two `CreditTracker` instances:
   - One for tracking credits granted to us (for sending)
   - One for tracking credits we've granted to the peer (for receiving)
2. Both sides send and receive CREDIT frames independently

**Current Implementation**: Both client and server track credits and handle CREDIT frames. The request stream is currently buffered, but the response stream handles credits properly.

## Configuration

Default configuration constants:

```rust
// Initial credits granted to senders
pub const DEFAULT_INITIAL_CREDITS: u32 = 16;

// Credits to grant when buffer space becomes available
pub const DEFAULT_CREDIT_REFILL: u32 = 8;
```

## Frame Protocol

### Frame Format

All frames follow this format:
```
[length varint][flags byte][payload bytes]
```

### Flags

- `DATA` (bit 0): Frame contains data
- `END_STREAM` (bit 1): Stream has ended
- `CANCEL` (bit 2): Stream was cancelled
- `CREDIT` (bit 3): Frame contains credit grant

### Credit Frame Example

A credit frame granting 100 credits:
```
[payload_length][0b0000_1000][100 as varint]
```

## Implementation Status

### ✅ Completed

- [x] Credit frame format
- [x] `CreditTracker` with atomic operations
- [x] Credit handling in `ResponseFrameStream` (client receives)
- [x] Credit handling in `RequestFrameStream` (server receives)
- [x] Credit frame tests
- [x] Documentation

### 🚧 Future Work

- [ ] Actual credit frame transmission over HTTP/2
- [ ] Dynamic credit adjustment based on buffer size
- [ ] Credit-based backpressure in client request streaming
- [ ] Configurable credit windows per RPC method
- [ ] Credit exhaustion metrics and monitoring

## Testing

Credit tracking is tested in `crates/quill-core/src/flow_control.rs`:

```rust
#[test]
fn test_credit_consumption() {
    let tracker = CreditTracker::new(5);
    assert!(tracker.try_consume()); // 4 remaining
    assert!(tracker.try_consume()); // 3 remaining
    assert_eq!(tracker.available(), 3);
}

#[test]
fn test_credit_exhaustion() {
    let tracker = CreditTracker::new(2);
    assert!(tracker.try_consume()); // 1 remaining
    assert!(tracker.try_consume()); // 0 remaining
    assert!(!tracker.try_consume()); // Should fail
}
```

Credit frame encoding/decoding is tested in `crates/quill-core/src/framing.rs`:

```rust
#[test]
fn test_credit_frame_roundtrip() {
    let original = Frame::credit(100);
    let encoded = original.encode();

    let mut parser = FrameParser::new();
    parser.feed(&encoded);

    let decoded = parser.parse_frame().unwrap().unwrap();
    assert_eq!(decoded.decode_credit(), Some(100));
}
```

## HTTP Transport Considerations

### HTTP/1.1

HTTP/1.1 doesn't support true bidirectional streaming. The current implementation:
- Buffers client request streams before sending
- Uses chunked transfer encoding for server response streams
- Cannot send credit frames from client to server

### HTTP/2

HTTP/2 provides true bidirectional streaming with multiplexing:
- Client can send credit frames while receiving response stream
- Server can send credit frames while receiving request stream
- Credit frames can flow independently in both directions

### HTTP/3 (QUIC)

HTTP/3 over QUIC provides additional benefits:
- Native stream multiplexing
- Stream-level flow control at transport layer
- Application-level credit control on top of QUIC flow control

## Example Usage

```rust
use quill_client::QuillClient;
use bytes::Bytes;

// Server streaming with flow control
let client = QuillClient::new("http://localhost:8080");
let request = Bytes::from("request data");

let mut stream = client
    .call_server_streaming("log.v1.LogService", "Tail", request)
    .await?;

// Client automatically tracks received messages
// and grants credits to server
while let Some(response) = stream.next().await {
    let response = response?;
    // Process response
    // Credits are granted automatically every DEFAULT_CREDIT_REFILL messages
}
```

## See Also

- [RFC 9113](https://www.rfc-editor.org/rfc/rfc9113.html) - HTTP/2 Flow Control
- [gRPC Flow Control](https://grpc.io/docs/guides/flow-control/)
- `crates/quill-core/src/framing.rs` - Frame protocol implementation
- `crates/quill-core/src/flow_control.rs` - Credit tracking implementation
