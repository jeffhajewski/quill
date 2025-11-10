//! Echo service example demonstrating Quill RPC
//!
//! This example shows a simple unary RPC call over HTTP/2

use bytes::Bytes;
use prost::Message;
use quill_core::QuillError;

// Include generated protobuf code
pub mod echo {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/echo.v1.rs"));
    }
}

pub use echo::v1::{EchoRequest, EchoResponse};

/// Echo handler implementation
pub async fn handle_echo(request: Bytes) -> Result<Bytes, QuillError> {
    // Decode the protobuf request
    let req = EchoRequest::decode(request)
        .map_err(|e| QuillError::Rpc(format!("Failed to decode request: {}", e)))?;

    // Create response (echo back the message)
    let resp = EchoResponse {
        message: req.message,
    };

    // Encode the response
    let mut buf = Vec::new();
    resp.encode(&mut buf)
        .map_err(|e| QuillError::Rpc(format!("Failed to encode response: {}", e)))?;

    Ok(Bytes::from(buf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
    use quill_client::QuillClient;
    use quill_server::QuillServer;
    use std::net::SocketAddr;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_echo_integration() {
        // Initialize tracing
        let _ = tracing_subscriber::fmt::try_init();

        // Start server in background
        let addr: SocketAddr = "127.0.0.1:18080".parse().unwrap();

        let server = QuillServer::builder()
            .register("echo.v1.EchoService/Echo", handle_echo)
            .build();

        let server_handle = tokio::spawn(async move {
            if let Err(e) = server.serve(addr).await {
                eprintln!("Server error: {}", e);
            }
        });

        // Give server time to start
        sleep(Duration::from_millis(100)).await;

        // Create client
        let client = QuillClient::builder()
            .base_url("http://127.0.0.1:18080")
            .build()
            .unwrap();

        // Make RPC call
        let request = EchoRequest {
            message: "Hello, Quill!".to_string(),
        };

        let mut req_bytes = Vec::new();
        request.encode(&mut req_bytes).unwrap();

        let response_bytes = client
            .call("echo.v1.EchoService", "Echo", Bytes::from(req_bytes))
            .await
            .unwrap();

        // Decode response
        let response = EchoResponse::decode(response_bytes).unwrap();
        assert_eq!(response.message, "Hello, Quill!");

        // Cleanup
        server_handle.abort();
    }
}
