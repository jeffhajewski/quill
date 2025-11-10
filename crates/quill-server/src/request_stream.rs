//! Server-side request streaming support

use bytes::Bytes;
use hyper::body::Incoming;
use quill_core::{CreditTracker, FrameParser, QuillError};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::Stream;

/// Stream adapter that parses frames from incoming request body
pub struct RequestFrameStream {
    body: Incoming,
    parser: FrameParser,
    credits: CreditTracker,
    messages_received: u32,
}

impl RequestFrameStream {
    pub fn new(body: Incoming) -> Self {
        Self {
            body,
            parser: FrameParser::new(),
            credits: CreditTracker::with_defaults(),
            messages_received: 0,
        }
    }
}

impl Stream for RequestFrameStream {
    type Item = Result<Bytes, QuillError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use http_body::Body;
        use quill_core::DEFAULT_CREDIT_REFILL;

        loop {
            // Try to parse a frame from buffered data
            match self.parser.parse_frame() {
                Ok(Some(frame)) => {
                    if frame.flags.is_end_stream() {
                        // Stream ended
                        return Poll::Ready(None);
                    }
                    if frame.flags.is_credit() {
                        // Client is granting us credits to send more responses
                        // (Useful for true bidirectional streaming)
                        if let Some(amount) = frame.decode_credit() {
                            self.credits.grant(amount);
                        }
                        // Continue to next frame
                        continue;
                    }
                    if frame.flags.is_data() {
                        self.messages_received += 1;

                        // In a future HTTP/2 implementation, we would send credit frames
                        // back to the client here to grant more send credits.
                        // For now, we just track locally.
                        if self.messages_received % DEFAULT_CREDIT_REFILL == 0 {
                            // Would send credit frame to client here
                            tracing::debug!(
                                "Would grant {} credits to client (received {} messages)",
                                DEFAULT_CREDIT_REFILL,
                                self.messages_received
                            );
                        }

                        return Poll::Ready(Some(Ok(frame.payload)));
                    }
                    if frame.flags.is_cancel() {
                        // Stream was cancelled by client
                        return Poll::Ready(Some(Err(QuillError::Rpc(
                            "Stream cancelled by client".to_string()
                        ))));
                    }
                    // Other frame types, continue
                }
                Ok(None) => {
                    // Need more data
                }
                Err(e) => {
                    return Poll::Ready(Some(Err(QuillError::Framing(e.to_string()))));
                }
            }

            // Read more data from body
            match Pin::new(&mut self.body).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    if let Ok(data) = frame.into_data() {
                        self.parser.feed(&data);
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(QuillError::Transport(e.to_string()))));
                }
                Poll::Ready(None) => {
                    // Body ended
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}
