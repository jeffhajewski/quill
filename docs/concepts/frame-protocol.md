# Frame Protocol

Quill uses a simple, efficient binary frame protocol for streaming RPCs.

## Frame Layout

```
┌─────────────────┬──────────────┬─────────────────┐
│ Length (varint) │ Flags (1 B)  │ Payload (N B)   │
└─────────────────┴──────────────┴─────────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| Length | 1-5 bytes | Varint-encoded payload length |
| Flags | 1 byte | Frame type and control flags |
| Payload | N bytes | Message data |

## Flags

| Flag | Value | Description |
|------|-------|-------------|
| `DATA` | `0x01` | Frame contains data payload |
| `END_STREAM` | `0x02` | Last frame in stream |
| `CANCEL` | `0x04` | Cancel the stream |
| `CREDIT` | `0x08` | Flow control credit grant |

Flags can be combined. For example, `DATA | END_STREAM` (`0x03`) indicates a final data frame.

## Varint Encoding

Lengths use protobuf-style varint encoding:
- Values 0-127: 1 byte
- Values 128-16383: 2 bytes
- Values up to 2^28-1: 4 bytes

```rust
// Encoding example
fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}
```

## Frame Types

### Data Frame

Carries message payload.

```
Flags: DATA (0x01)
Payload: Protobuf-encoded message
```

### End Stream Frame

Signals stream completion.

```
Flags: END_STREAM (0x02)
Payload: Empty or final data
```

### Cancel Frame

Aborts the stream.

```
Flags: CANCEL (0x04)
Payload: Empty
```

### Credit Frame

Grants flow control credits to the sender.

```
Flags: CREDIT (0x08)
Payload: Credit amount (varint)
```

## Usage Examples

### Encoding a Frame

```rust
use quill_core::framing::{Frame, FrameFlags, encode_frame};

// Create a data frame
let frame = Frame::new(
    FrameFlags::DATA,
    b"Hello, World!".to_vec(),
);

// Encode to bytes
let mut buffer = Vec::new();
encode_frame(&frame, &mut buffer);
```

### Decoding a Frame

```rust
use quill_core::framing::{Frame, decode_frame};

let bytes = &[13, 0x01, /* payload... */];
let frame = decode_frame(bytes)?;

if frame.flags.contains(FrameFlags::DATA) {
    println!("Received data: {:?}", frame.payload);
}
```

### Streaming with Frames

```rust
// Server streaming - send multiple frames
sender.send(Frame::data(response1)).await?;
sender.send(Frame::data(response2)).await?;
sender.send(Frame::end_stream()).await?;

// Client receiving
while let Some(frame) = receiver.next().await {
    if frame.flags.contains(FrameFlags::END_STREAM) {
        break;
    }
    process(frame.payload);
}
```

## Resource Limits

| Limit | Value | Description |
|-------|-------|-------------|
| `max_frame_bytes` | 4 MB | Maximum frame size |
| `max_streams_per_connection` | 100 | Concurrent streams limit |

Frames exceeding `max_frame_bytes` are rejected with an error.

## Wire Format Example

A "Hello" message with END_STREAM:

```
05        # Length: 5 bytes
03        # Flags: DATA | END_STREAM
48 65 6C 6C 6F  # Payload: "Hello"
```

## Comparison with gRPC

| Feature | Quill | gRPC |
|---------|-------|------|
| Frame header | 2-6 bytes | 5 bytes fixed |
| Compression flag | In flags byte | Separate byte |
| Trailers | Not required | Required for errors |
| Flow control | Credit-based | WINDOW_UPDATE |

## Next Steps

- [Flow Control](../flow-control.md) - Credit-based flow control
- [Streaming Guide](../guides/streaming.md) - Streaming patterns
- [Error Handling](error-handling.md) - Problem Details errors
