//! Greeter example demonstrating Quill code generation
//!
//! This example shows how to:
//! - Define a service in protobuf
//! - Generate client and server code using quill-codegen
//! - Implement the generated server trait
//! - Use the generated client

use bytes::Bytes;
use quill_core::QuillError;
use std::pin::Pin;
use futures::Stream;

// Include the generated protobuf code
pub mod greeter {
    include!(concat!(env!("OUT_DIR"), "/greeter.v1.rs"));
}

// Re-export generated types for convenience
pub use greeter::{HelloReply, HelloRequest};

use greeter::greeter_server::{Greeter, add_service};

/// Implementation of the Greeter service
pub struct GreeterService;

#[async_trait::async_trait]
impl Greeter for GreeterService {
    async fn say_hello(&self, request: HelloRequest) -> Result<HelloReply, QuillError> {
        let message = format!("Hello, {}!", request.name);
        Ok(HelloReply { message })
    }

    async fn say_hello_stream(
        &self,
        request: HelloRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<HelloReply, QuillError>> + Send>>, QuillError> {
        use futures::stream;

        let greetings = vec![
            format!("Hello, {}!", request.name),
            format!("Welcome, {}!", request.name),
            format!("Greetings, {}!", request.name),
            format!("Nice to meet you, {}!", request.name),
        ];

        let stream = stream::iter(greetings.into_iter().map(|message| {
            Ok(HelloReply { message })
        }));

        Ok(Box::pin(stream))
    }
}

/// Create a server with the greeter service
pub fn create_server() -> quill_server::QuillServer {
    let builder = quill_server::QuillServer::builder();
    let service = GreeterService;
    add_service(builder, service).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_request_creation() {
        let request = HelloRequest {
            name: "World".to_string(),
        };
        assert_eq!(request.name, "World");
    }

    #[test]
    fn test_hello_reply_creation() {
        let reply = HelloReply {
            message: "Hello, World!".to_string(),
        };
        assert_eq!(reply.message, "Hello, World!");
    }

    #[tokio::test]
    async fn test_greeter_service() {
        let service = GreeterService;
        let request = HelloRequest {
            name: "Alice".to_string(),
        };

        let reply = service.say_hello(request).await.unwrap();
        assert_eq!(reply.message, "Hello, Alice!");
    }

    #[tokio::test]
    async fn test_greeter_stream() {
        use futures::StreamExt;

        let service = GreeterService;
        let request = HelloRequest {
            name: "Bob".to_string(),
        };

        let mut stream = service.say_hello_stream(request).await.unwrap();

        let mut count = 0;
        while let Some(result) = stream.next().await {
            let reply = result.unwrap();
            assert!(reply.message.contains("Bob"));
            count += 1;
        }

        assert_eq!(count, 4);
    }
}
