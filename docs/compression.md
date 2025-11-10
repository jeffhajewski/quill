# Compression in Quill

## Overview

Quill supports zstd compression for both requests and responses to reduce bandwidth usage and improve performance for large payloads.

## Client-Side Compression

### Enabling Compression

Compression is disabled by default. Enable it when building the client:

```rust
use quill_client::QuillClient;

let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .enable_compression(true)
    .compression_level(3)  // Optional: 0-22, default is 3
    .build()?;
```

### How It Works

When compression is enabled:

1. **Request Compression**: The client automatically compresses request bodies using zstd
   - Adds `Content-Encoding: zstd` header
   - Adds `Accept-Encoding: zstd` header to indicate support for compressed responses

2. **Response Decompression**: The client automatically decompresses responses
   - Checks `Content-Encoding` header
   - Decompresses if the response is zstd-compressed

### Example

```rust
use quill_client::QuillClient;
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client with compression enabled
    let client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .enable_compression(true)
        .build()?;

    // Make a call - request will be compressed, response will be decompressed
    let request = Bytes::from("large request data...");
    let response = client
        .call("myservice.v1.MyService", "MyMethod", request)
        .await?;

    println!("Response: {:?}", response);
    Ok(())
}
```

## Server-Side Compression

### Decompressing Requests

The server automatically handles compressed requests:

```rust
use quill_server::middleware::decompress_request_body;
use hyper::Request;

async fn handle_request(req: Request<hyper::body::Incoming>) -> Result<(), Box<dyn std::error::Error>> {
    // Decompress request if it has Content-Encoding: zstd
    let (parts, body_bytes) = decompress_request_body(req).await?;

    // Process decompressed body
    println!("Received {} bytes", body_bytes.len());

    Ok(())
}
```

### Compressing Responses

Response compression is currently planned but not yet implemented for streaming responses. For unary responses, you can manually compress:

```rust
use quill_server::middleware::{compress_zstd, accepts_zstd};
use bytes::Bytes;
use http::{Response, Request};

fn create_response(
    req: &Request<hyper::body::Incoming>,
    data: Bytes,
) -> Result<Response<impl http_body::Body>, Box<dyn std::error::Error>> {
    // Check if client accepts zstd
    if accepts_zstd(req) && data.len() > 1024 {
        let compressed = compress_zstd(&data, 3)?;
        Ok(Response::builder()
            .header("Content-Encoding", "zstd")
            .header("Content-Type", "application/proto")
            .body(compressed)?)
    } else {
        Ok(Response::builder()
            .header("Content-Type", "application/proto")
            .body(data)?)
    }
}
```

## Compression Utilities

### Functions

#### `compress_zstd(data: &[u8], level: i32) -> Result<Bytes, QuillError>`

Compress data using zstd.

- **Parameters**:
  - `data`: Input bytes to compress
  - `level`: Compression level (0-22, recommended 3-6)
    - 0-3: Fast compression, lower ratio
    - 4-9: Balanced
    - 10-22: High compression, slower

- **Returns**: Compressed bytes

#### `decompress_zstd(data: &[u8]) -> Result<Bytes, QuillError>`

Decompress zstd-compressed data.

- **Parameters**:
  - `data`: Compressed bytes

- **Returns**: Decompressed bytes

#### `accepts_zstd(req: &Request<Incoming>) -> bool`

Check if the client accepts zstd compression by examining the `Accept-Encoding` header.

### Constants

```rust
/// Default compression level (balanced speed/ratio)
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Minimum body size to compress (1KB)
pub const MIN_COMPRESS_SIZE: usize = 1024;
```

## Performance Considerations

### When to Use Compression

**Good candidates**:
- Large payloads (> 1KB)
- Text-based protobuf messages
- Repetitive data structures
- High-latency networks

**Poor candidates**:
- Small payloads (< 1KB)
- Already compressed data (images, video)
- Binary data with high entropy
- Low-latency, high-bandwidth networks

### Compression Levels

| Level | Speed | Ratio | Use Case |
|-------|-------|-------|----------|
| 0-3 | Very fast | Good | Real-time applications, low CPU |
| 4-6 | Fast | Better | General purpose (recommended) |
| 7-12 | Medium | Very good | Batch processing |
| 13-22 | Slow | Excellent | Archival, one-time compression |

**Recommendation**: Use level 3 (default) for most applications.

### Benchmarks

Example compression ratios for protobuf messages:

| Payload Type | Original Size | Compressed | Ratio | Level |
|--------------|---------------|------------|-------|-------|
| User list (100 users) | 15KB | 3.2KB | 78.7% | 3 |
| Log entries (1000) | 250KB | 18KB | 92.8% | 3 |
| Binary metrics | 8KB | 7.5KB | 6.3% | 3 |
| Small request | 200B | 250B | -25% | 3 |

## HTTP Headers

### Request Headers

- `Content-Encoding: zstd` - Indicates the request body is zstd-compressed
- `Accept-Encoding: zstd` - Indicates the client accepts zstd-compressed responses

### Response Headers

- `Content-Encoding: zstd` - Indicates the response body is zstd-compressed

## Error Handling

Compression errors are returned as `QuillError::Transport`:

```rust
match client.call("service", "method", request).await {
    Ok(response) => println!("Success: {:?}", response),
    Err(QuillError::Transport(msg)) if msg.contains("Compression failed") => {
        eprintln!("Compression error: {}", msg);
    }
    Err(QuillError::Transport(msg)) if msg.contains("Decompression failed") => {
        eprintln!("Decompression error: {}", msg);
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Future Enhancements

- [ ] Streaming compression for server responses
- [ ] Automatic compression threshold configuration
- [ ] Compression statistics and metrics
- [ ] Additional algorithms (gzip, brotli) for HTTP compatibility
- [ ] Per-method compression configuration
- [ ] Compression negotiation via Accept-Encoding

## Testing

Compression is tested in `crates/quill-server/src/middleware.rs`:

```rust
#[test]
fn test_zstd_roundtrip() {
    let original = b"Hello, world! This is a test message. ".repeat(10);
    let compressed = compress_zstd(&original, 3).unwrap();
    let decompressed = decompress_zstd(&compressed).unwrap();

    assert_eq!(original, &decompressed[..]);
    assert!(compressed.len() < original.len());
}
```

## See Also

- [zstd Documentation](https://facebook.github.io/zstd/)
- [HTTP Content-Encoding](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Content-Encoding)
- `crates/quill-server/src/middleware.rs` - Server compression utilities
- `crates/quill-client/src/client.rs` - Client compression implementation
