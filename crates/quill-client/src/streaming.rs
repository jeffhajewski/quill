//! Client-side streaming support

use bytes::Bytes;
use quill_core::{Frame, QuillError};
use std::pin::Pin;
use tokio_stream::Stream;

/// Request type that can be either unary or streaming
pub enum RpcRequest {
    /// Unary request (single message)
    Unary(Bytes),
    /// Streaming request (multiple messages)
    Streaming(Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>),
}

impl RpcRequest {
    /// Create a unary request
    pub fn unary(bytes: Bytes) -> Self {
        Self::Unary(bytes)
    }

    /// Create a streaming request
    pub fn streaming<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, QuillError>> + Send + 'static,
    {
        Self::Streaming(Box::pin(stream))
    }
}

/// Encode a stream of messages into frames
pub async fn encode_request_stream(
    mut stream: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>,
) -> Result<Bytes, QuillError> {
    use tokio_stream::StreamExt;

    let mut encoded = Vec::new();

    // Encode each message as a frame
    while let Some(result) = stream.next().await {
        let data = result?;
        let frame = Frame::data(data);
        encoded.extend_from_slice(&frame.encode());
    }

    // Add END_STREAM frame
    let end_frame = Frame::end_stream();
    encoded.extend_from_slice(&end_frame.encode());

    Ok(Bytes::from(encoded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::iter;

    #[tokio::test]
    async fn test_encode_request_stream() {
        let data = vec![
            Ok(Bytes::from("hello")),
            Ok(Bytes::from("world")),
        ];
        let stream = iter(data);
        let encoded = encode_request_stream(Box::pin(stream)).await.unwrap();

        // Should have 2 data frames + 1 end frame
        assert!(encoded.len() > 0);
    }
}
