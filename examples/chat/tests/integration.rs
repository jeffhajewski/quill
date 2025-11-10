//! Integration tests for chat example

use bytes::Bytes;
use chat_example::{ChatMessage, ChatRoom, create_welcome_message, handle_chat};
use quill_core::QuillError;
use tokio_stream::{iter, StreamExt};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_chat_message_roundtrip() {
    let msg = ChatMessage {
        user: "Alice".to_string(),
        message: "Hello!".to_string(),
        timestamp: 1234567890,
    };

    let encoded = msg.encode();
    let decoded = ChatMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.user, msg.user);
    assert_eq!(decoded.message, msg.message);
    assert_eq!(decoded.timestamp, msg.timestamp);
}

#[tokio::test]
async fn test_chat_room_single_subscriber() {
    let room = ChatRoom::new();
    let mut rx = room.subscribe();

    let msg = ChatMessage {
        user: "Bob".to_string(),
        message: "Test".to_string(),
        timestamp: 1000,
    };

    room.broadcast(msg.clone()).unwrap();

    let received = rx.try_recv().unwrap();
    assert_eq!(received.user, msg.user);
    assert_eq!(received.message, msg.message);
}

#[tokio::test]
async fn test_chat_room_multiple_subscribers() {
    let room = ChatRoom::new();
    let mut rx1 = room.subscribe();
    let mut rx2 = room.subscribe();
    let mut rx3 = room.subscribe();

    let msg = ChatMessage {
        user: "Charlie".to_string(),
        message: "Broadcast test".to_string(),
        timestamp: 2000,
    };

    room.broadcast(msg.clone()).unwrap();

    // All subscribers should receive the message
    let msg1 = rx1.try_recv().unwrap();
    let msg2 = rx2.try_recv().unwrap();
    let msg3 = rx3.try_recv().unwrap();

    assert_eq!(msg1.message, msg.message);
    assert_eq!(msg2.message, msg.message);
    assert_eq!(msg3.message, msg.message);
}

#[tokio::test]
async fn test_welcome_message() {
    let msg = create_welcome_message("Dave");

    assert_eq!(msg.user, "System");
    assert!(msg.message.contains("Dave"));
    assert!(msg.message.contains("Welcome"));
}

#[tokio::test]
async fn test_chat_room_message_ordering() {
    let room = ChatRoom::new();
    let mut rx = room.subscribe();

    for i in 0..5 {
        let msg = ChatMessage {
            user: "Eve".to_string(),
            message: format!("Message {}", i),
            timestamp: i as u64,
        };
        room.broadcast(msg).unwrap();
    }

    for i in 0..5 {
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.message, format!("Message {}", i));
    }
}

#[tokio::test]
async fn test_chat_room_concurrent_broadcasts() {
    let room = Arc::new(ChatRoom::new());
    let mut rx = room.subscribe();

    let mut handles = vec![];

    for i in 0..10 {
        let room = room.clone();
        let handle = tokio::spawn(async move {
            let msg = ChatMessage {
                user: format!("User{}", i),
                message: format!("Message from user {}", i),
                timestamp: i,
            };
            room.broadcast(msg).unwrap();
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Should receive all 10 messages
    let mut count = 0;
    while let Ok(_) = rx.try_recv() {
        count += 1;
    }

    assert_eq!(count, 10);
}

#[tokio::test]
async fn test_chat_message_simple() {
    let msg = ChatMessage {
        user: "Frank".to_string(),
        message: "Simple message without special chars".to_string(),
        timestamp: 3000,
    };

    let encoded = msg.encode();
    let decoded = ChatMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.message, msg.message);
    assert_eq!(decoded.user, msg.user);
}

#[tokio::test]
async fn test_chat_room_late_subscriber() {
    let room = ChatRoom::new();

    // Subscribe to room
    let mut rx = room.subscribe();

    // Send a message
    let msg = ChatMessage {
        user: "Grace".to_string(),
        message: "Test message".to_string(),
        timestamp: 2000,
    };
    room.broadcast(msg.clone()).unwrap();

    // Subscriber should receive the message
    let received = rx.try_recv().unwrap();
    assert_eq!(received.message, msg.message);
}

#[tokio::test]
async fn test_empty_message() {
    let msg = ChatMessage {
        user: "".to_string(),
        message: "".to_string(),
        timestamp: 0,
    };

    let encoded = msg.encode();
    let decoded = ChatMessage::decode(&encoded).unwrap();

    assert_eq!(decoded.user, "");
    assert_eq!(decoded.message, "");
}
