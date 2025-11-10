//! Middleware implementations for Quill server
//!
//! This module provides middleware for:
//! - Compression (zstd)
//! - Decompression of incoming requests
//! - Content negotiation

use bytes::Bytes;
use http::{header, Request, Response};
use http_body_util::BodyExt;
use hyper::body::Incoming;
use quill_core::QuillError;

/// Compression level for zstd
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Minimum body size to compress (in bytes)
pub const MIN_COMPRESS_SIZE: usize = 1024; // 1KB

/// Check if the client accepts zstd compression
pub fn accepts_zstd(req: &Request<Incoming>) -> bool {
    req.headers()
        .get(header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("zstd"))
        .unwrap_or(false)
}

/// Compress bytes using zstd
pub fn compress_zstd(data: &[u8], level: i32) -> Result<Bytes, QuillError> {
    zstd::encode_all(data, level)
        .map(Bytes::from)
        .map_err(|e| QuillError::Transport(format!("Compression failed: {}", e)))
}

/// Decompress bytes using zstd
pub fn decompress_zstd(data: &[u8]) -> Result<Bytes, QuillError> {
    zstd::decode_all(data)
        .map(Bytes::from)
        .map_err(|e| QuillError::Transport(format!("Decompression failed: {}", e)))
}

/// Decompress request body if it's compressed
///
/// Returns the request parts and the decompressed body bytes
pub async fn decompress_request_body(
    req: Request<Incoming>,
) -> Result<(http::request::Parts, Bytes), QuillError> {
    let (parts, body) = req.into_parts();

    // Read body
    let body_bytes = body
        .collect()
        .await
        .map_err(|e| QuillError::Transport(format!("Failed to read request body: {}", e)))?
        .to_bytes();

    // Check if compressed
    let decompressed = if let Some(encoding) = parts.headers.get(header::CONTENT_ENCODING) {
        if encoding == "zstd" {
            decompress_zstd(&body_bytes)?
        } else {
            body_bytes
        }
    } else {
        body_bytes
    };

    Ok((parts, decompressed))
}

/// Compress response body if appropriate
///
/// Note: This is a placeholder for future implementation.
/// Compressing streaming responses requires a compression stream adapter.
pub fn compress_response<B>(
    response: Response<B>,
    _accept_zstd: bool,
) -> Response<B>
where
    B: http_body::Body<Data = Bytes, Error = QuillError> + Send + 'static,
{
    // For now, we'll return the response as-is
    // In a real implementation, we would:
    // 1. Check if body is large enough to compress
    // 2. Compress the body
    // 3. Add Content-Encoding header
    // 4. Return compressed response
    //
    // This is tricky because we need to consume the body to compress it,
    // but we want to stream responses. For streaming responses, we'd need
    // a compression stream adapter.
    response
}

/// Middleware layer for compression
pub struct CompressionLayer {
    level: i32,
}

impl CompressionLayer {
    pub fn new() -> Self {
        Self {
            level: DEFAULT_COMPRESSION_LEVEL,
        }
    }

    pub fn with_level(level: i32) -> Self {
        Self { level }
    }

    pub fn level(&self) -> i32 {
        self.level
    }
}

impl Default for CompressionLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zstd_roundtrip() {
        // Use a larger message with repetition for good compression
        let original = b"Hello, world! This is a test message. ".repeat(10);
        let compressed = compress_zstd(&original, 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();

        assert_eq!(original, &decompressed[..]);
        // With repetition, compression should be effective
        assert!(compressed.len() < original.len());
    }

    #[test]
    fn test_compress_empty() {
        let original = b"";
        let compressed = compress_zstd(original, 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();

        assert_eq!(original, &decompressed[..]);
    }

    #[test]
    fn test_compress_large() {
        // Create a large repeating pattern (should compress well)
        let original = vec![b'a'; 10000];
        let compressed = compress_zstd(&original, 3).unwrap();
        let decompressed = decompress_zstd(&compressed).unwrap();

        assert_eq!(original, &decompressed[..]);
        // Should achieve good compression on repeating data
        assert!(compressed.len() < original.len() / 10);
    }
}
