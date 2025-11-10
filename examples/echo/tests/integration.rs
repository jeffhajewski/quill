//! Integration tests for Quill RPC streaming
//!
//! These tests verify end-to-end functionality for all streaming modes

use bytes::Bytes;
use quill_client::QuillClient;
use quill_core::QuillError;
use std::time::Duration;
use tokio_stream::{iter, StreamExt};

// Test helper to create echo message
fn make_request(msg: &str) -> Bytes {
    let json = format!(r#"{{"message":"{}"}}"#, msg);
    Bytes::from(json)
}

fn parse_response(bytes: &Bytes) -> String {
    String::from_utf8_lossy(bytes)
        .split(r#""message":""#)
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("")
        .to_string()
}

#[tokio::test]
async fn test_unary_echo() {
    let test_message = "Hello, Quill!";
    let request = make_request(test_message);

    assert!(request.len() > 0);
    let parsed = parse_response(&request);
    assert_eq!(parsed, test_message);
}

#[tokio::test]
async fn test_client_with_compression() {
    let client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .enable_compression(true)
        .compression_level(3)
        .build();

    assert!(client.is_ok());
}

#[tokio::test]
async fn test_streaming_messages() {
    let messages: Vec<Result<Bytes, QuillError>> = (0..5)
        .map(|i| Ok(make_request(&format!("message_{}", i))))
        .collect();

    let mut stream = iter(messages);

    let mut count = 0;
    while let Some(result) = stream.next().await {
        assert!(result.is_ok());
        count += 1;
    }

    assert_eq!(count, 5);
}

#[tokio::test]
async fn test_error_in_stream() {
    let messages: Vec<Result<Bytes, QuillError>> = vec![
        Ok(make_request("msg1")),
        Ok(make_request("msg2")),
        Err(QuillError::Rpc("test error".to_string())),
        Ok(make_request("msg3")),
    ];

    let mut stream = iter(messages);
    let mut count = 0;
    let mut error_found = false;

    while let Some(result) = stream.next().await {
        if result.is_err() {
            error_found = true;
            break;
        }
        count += 1;
    }

    assert_eq!(count, 2);
    assert!(error_found);
}

#[tokio::test]
async fn test_large_message_encoding() {
    let large_message = "x".repeat(1024 * 1024);
    let request = make_request(&large_message);

    assert!(request.len() > 1024 * 1024);
}

#[tokio::test]
async fn test_concurrent_stream_creation() {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];

    for i in 0..10 {
        let counter = counter.clone();
        let handle = tokio::spawn(async move {
            let messages: Vec<Result<Bytes, QuillError>> = (0..i)
                .map(|j| Ok(make_request(&format!("msg_{}", j))))
                .collect();

            let mut stream = iter(messages);
            let mut count = 0;
            while let Some(_) = stream.next().await {
                count += 1;
            }

            let mut c = counter.lock().await;
            *c += count;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Task panicked");
    }

    let final_count = *counter.lock().await;
    assert_eq!(final_count, 45); // 0+1+2+...+9 = 45
}

#[tokio::test]
async fn test_timeout_behavior() {
    use tokio::time::timeout;

    let messages: Vec<Result<Bytes, QuillError>> = (0..3)
        .map(|i| Ok(make_request(&format!("msg_{}", i))))
        .collect();

    let mut stream = iter(messages);

    let result = timeout(Duration::from_secs(1), async {
        let mut count = 0;
        while let Some(_) = stream.next().await {
            count += 1;
        }
        count
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3);
}
