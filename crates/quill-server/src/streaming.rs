//! Streaming support for Quill server

use bytes::Bytes;
use hyper::body::Frame as HyperFrame;
use quill_core::{Frame, QuillError};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::Stream;

/// Response type that can be either unary or streaming
pub enum RpcResponse {
    /// Unary response (single message)
    Unary(Bytes),
    /// Streaming response (multiple messages)
    Streaming(Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>),
}

impl RpcResponse {
    /// Create a unary response
    pub fn unary(bytes: Bytes) -> Self {
        Self::Unary(bytes)
    }

    /// Create a streaming response
    pub fn streaming<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, QuillError>> + Send + 'static,
    {
        Self::Streaming(Box::pin(stream))
    }
}

/// Stream adapter that wraps Quill frames in HTTP frames
pub struct FramedResponseStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>,
    ended: bool,
}

impl FramedResponseStream {
    pub fn new(stream: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>) -> Self {
        Self {
            inner: stream,
            ended: false,
        }
    }
}

impl Stream for FramedResponseStream {
    type Item = Result<HyperFrame<Bytes>, QuillError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }

        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(data))) => {
                // Wrap data in a Quill frame
                let frame = Frame::data(data);
                let encoded = frame.encode();
                Poll::Ready(Some(Ok(HyperFrame::data(encoded))))
            }
            Poll::Ready(Some(Err(e))) => {
                // Error in stream
                self.ended = true;
                Poll::Ready(Some(Err(e)))
            }
            Poll::Ready(None) => {
                // Stream ended, send END_STREAM frame
                self.ended = true;
                let frame = Frame::end_stream();
                let encoded = frame.encode();
                Poll::Ready(Some(Ok(HyperFrame::data(encoded))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::iter;

    #[tokio::test]
    async fn test_framed_response_stream() {
        use tokio_stream::StreamExt;

        let data = vec![
            Ok(Bytes::from("hello")),
            Ok(Bytes::from("world")),
        ];
        let stream = iter(data);
        let mut framed = FramedResponseStream::new(Box::pin(stream));

        // Should get 2 data frames + 1 end frame
        let _frame1 = framed.next().await.unwrap().unwrap();
        let _frame2 = framed.next().await.unwrap().unwrap();
        let _frame3 = framed.next().await.unwrap().unwrap();
        let end = framed.next().await;

        assert!(end.is_none());
    }
}
