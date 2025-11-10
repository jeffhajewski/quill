//! Integration tests for log tailing example

use bytes::Bytes;
use prost::Message;
use streaming_example::{handle_tail, LogEntry, TailRequest};
use quill_server::streaming::RpcResponse;
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_log_entry_encode_decode() {
    let entry = LogEntry {
        timestamp: "2025-11-10T12:00:00Z".to_string(),
        level: "INFO".to_string(),
        message: "Test log message".to_string(),
    };

    let mut buf = Vec::new();
    entry.encode(&mut buf).unwrap();
    let encoded = Bytes::from(buf);

    let decoded = LogEntry::decode(encoded).unwrap();

    assert_eq!(decoded.timestamp, entry.timestamp);
    assert_eq!(decoded.level, entry.level);
    assert_eq!(decoded.message, entry.message);
}

#[tokio::test]
async fn test_tail_request_encode_decode() {
    let request = TailRequest {
        max_entries: 50,
    };

    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();
    let encoded = Bytes::from(buf);

    let decoded = TailRequest::decode(encoded).unwrap();

    assert_eq!(decoded.max_entries, request.max_entries);
}

#[tokio::test]
async fn test_handle_tail_stream() {
    let request = TailRequest { max_entries: 5 };
    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();

    let response = handle_tail(Bytes::from(buf)).await.unwrap();

    // Should return streaming response
    match response {
        RpcResponse::Streaming(mut stream) => {
            let mut count = 0;
            while let Some(result) = stream.next().await {
                assert!(result.is_ok());
                count += 1;
            }
            assert_eq!(count, 5);
        }
        _ => panic!("Expected streaming response"),
    }
}

#[tokio::test]
async fn test_handle_tail_large_count() {
    let request = TailRequest { max_entries: 100 };
    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();

    let response = handle_tail(Bytes::from(buf)).await.unwrap();

    match response {
        RpcResponse::Streaming(mut stream) => {
            let mut count = 0;
            while let Some(result) = stream.next().await {
                assert!(result.is_ok());
                count += 1;
            }
            assert_eq!(count, 100);
        }
        _ => panic!("Expected streaming response"),
    }
}

#[tokio::test]
async fn test_handle_tail_default() {
    let request = TailRequest { max_entries: 0 }; // Should use default
    let mut buf = Vec::new();
    request.encode(&mut buf).unwrap();

    let response = handle_tail(Bytes::from(buf)).await.unwrap();

    match response {
        RpcResponse::Streaming(mut stream) => {
            let mut count = 0;
            while let Some(result) = stream.next().await {
                assert!(result.is_ok());
                count += 1;
            }
            assert_eq!(count, 10); // Default is 10
        }
        _ => panic!("Expected streaming response"),
    }
}

#[tokio::test]
async fn test_log_entry_special_characters() {
    let entry = LogEntry {
        timestamp: "2025-11-10T12:00:00Z".to_string(),
        level: "INFO".to_string(),
        message: r#"Message with "quotes" and 'apostrophes'"#.to_string(),
    };

    let mut buf = Vec::new();
    entry.encode(&mut buf).unwrap();
    let encoded = Bytes::from(buf);

    let decoded = LogEntry::decode(encoded).unwrap();

    assert_eq!(decoded.message, entry.message);
}

#[tokio::test]
async fn test_empty_log_message() {
    let entry = LogEntry {
        timestamp: "2025-11-10T12:00:00Z".to_string(),
        level: "INFO".to_string(),
        message: "".to_string(),
    };

    let mut buf = Vec::new();
    entry.encode(&mut buf).unwrap();
    let encoded = Bytes::from(buf);

    let decoded = LogEntry::decode(encoded).unwrap();

    assert_eq!(decoded.message, "");
}
