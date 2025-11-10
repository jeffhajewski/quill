//! Streaming examples for Quill RPC
//!
//! Demonstrates server-side streaming with a log-tailing service

use bytes::Bytes;
use prost::Message;
use quill_core::QuillError;
use quill_server::RpcResponse;
use tokio_stream::{Stream, StreamExt};

// Include generated protobuf code
pub mod log {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/log.v1.rs"));
    }
}

pub use log::v1::{LogEntry, TailRequest};

/// Handle a streaming log tail request
pub async fn handle_tail(request: Bytes) -> Result<RpcResponse, QuillError> {
    // Decode the request
    let req = TailRequest::decode(request)
        .map_err(|e| QuillError::Rpc(format!("Failed to decode request: {}", e)))?;

    let max_entries = if req.max_entries > 0 {
        req.max_entries as usize
    } else {
        10 // default
    };

    // Create a stream of log entries
    let entries = generate_log_entries(max_entries);

    // Convert to bytes stream
    let byte_stream = entries.map(|entry| {
        let mut buf = Vec::new();
        entry.encode(&mut buf)
            .map_err(|e| QuillError::Rpc(format!("Failed to encode entry: {}", e)))?;
        Ok(Bytes::from(buf))
    });

    Ok(RpcResponse::streaming(byte_stream))
}

/// Generate mock log entries
fn generate_log_entries(count: usize) -> impl Stream<Item = LogEntry> {
    let entries: Vec<LogEntry> = (0..count)
        .map(|i| LogEntry {
            timestamp: format!("2025-11-10T12:00:{:02}Z", i),
            level: if i % 3 == 0 { "ERROR" } else { "INFO" }.to_string(),
            message: format!("Log message #{}", i),
        })
        .collect();

    tokio_stream::iter(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_log_entries() {
        let entries: Vec<_> = generate_log_entries(5).collect().await;
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].message, "Log message #0");
    }

    #[tokio::test]
    async fn test_handle_tail() {
        let request = TailRequest { max_entries: 3 };
        let mut buf = Vec::new();
        request.encode(&mut buf).unwrap();

        let response = handle_tail(Bytes::from(buf)).await.unwrap();

        match response {
            RpcResponse::Streaming(_) => {
                // Expected
            }
            RpcResponse::Unary(_) => panic!("Expected streaming response"),
        }
    }
}
