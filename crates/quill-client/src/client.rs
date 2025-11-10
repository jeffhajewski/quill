//! Quill client implementation

use bytes::Bytes;
use http::{Method, Request};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use quill_core::{CreditTracker, FrameParser, ProfilePreference, QuillError};
use crate::streaming::encode_request_stream;
use std::fmt;
use std::pin::Pin;
use tokio_stream::Stream;

/// Quill RPC client
pub struct QuillClient {
    base_url: String,
    client: Client<HttpConnector, Full<Bytes>>,
    profile_preference: ProfilePreference,
}

impl QuillClient {
    /// Create a new client with the given base URL
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();

        Self {
            base_url: base_url.into(),
            client,
            profile_preference: ProfilePreference::default_preference(),
        }
    }

    /// Create a builder for configuring the client
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Make a unary RPC call
    ///
    /// # Arguments
    /// * `service` - The service path (e.g., "echo.v1.EchoService")
    /// * `method` - The method name (e.g., "Echo")
    /// * `request` - The protobuf-encoded request bytes
    ///
    /// # Returns
    /// The protobuf-encoded response bytes
    pub async fn call(
        &self,
        service: &str,
        method: &str,
        request: Bytes,
    ) -> Result<Bytes, QuillError> {
        // Build the full URL
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/proto")
            .header("Accept", "application/proto")
            .header("Prefer", self.profile_preference.to_header_value())
            .body(Full::new(request))
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request
        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to send request: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            // Try to parse Problem Details
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| QuillError::Transport(format!("Failed to read error response: {}", e)))?
                .to_bytes();

            // Try to parse as JSON Problem Details
            if let Ok(pd) = serde_json::from_slice(&body_bytes) {
                return Err(QuillError::ProblemDetails(pd));
            }

            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        // Read response body
        let body_bytes = resp
            .into_body()
            .collect()
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to read response: {}", e)))?
            .to_bytes();

        Ok(body_bytes)
    }

    /// Make a streaming RPC call (client streaming)
    ///
    /// # Arguments
    /// * `service` - The service path (e.g., "upload.v1.UploadService")
    /// * `method` - The method name (e.g., "Upload")
    /// * `request` - Stream of request messages
    ///
    /// # Returns
    /// The protobuf-encoded response bytes
    pub async fn call_client_streaming(
        &self,
        service: &str,
        method: &str,
        request: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>,
    ) -> Result<Bytes, QuillError> {
        // Encode the stream into frames
        let encoded = encode_request_stream(request).await?;

        // Use regular call with encoded frames
        self.call(service, method, encoded).await
    }

    /// Receive a streaming response (server streaming)
    ///
    /// # Arguments
    /// * `service` - The service path
    /// * `method` - The method name
    /// * `request` - The protobuf-encoded request bytes
    ///
    /// # Returns
    /// A stream of response messages
    pub async fn call_server_streaming(
        &self,
        service: &str,
        method: &str,
        request: Bytes,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>, QuillError> {
        // Build the full URL
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/proto")
            .header("Accept", "application/proto")
            .header("Prefer", self.profile_preference.to_header_value())
            .body(Full::new(request))
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request
        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to send request: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| QuillError::Transport(format!("Failed to read error response: {}", e)))?
                .to_bytes();

            if let Ok(pd) = serde_json::from_slice(&body_bytes) {
                return Err(QuillError::ProblemDetails(pd));
            }

            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        // Create a stream that parses frames from the response
        let body = resp.into_body();
        let frame_stream = ResponseFrameStream::new(body);

        Ok(Box::pin(frame_stream))
    }

    /// Make a bidirectional streaming RPC call
    ///
    /// # Arguments
    /// * `service` - The service path
    /// * `method` - The method name
    /// * `request` - Stream of request messages
    ///
    /// # Returns
    /// A stream of response messages
    pub async fn call_bidi_streaming(
        &self,
        service: &str,
        method: &str,
        request: Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, QuillError>> + Send>>, QuillError> {
        // Build the full URL
        let url = format!("{}/{}/{}", self.base_url, service, method);

        // Encode the request stream into frames
        let encoded = encode_request_stream(request).await?;

        // Build the HTTP request
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Content-Type", "application/proto")
            .header("Accept", "application/proto")
            .header("Prefer", self.profile_preference.to_header_value())
            .body(Full::new(encoded))
            .map_err(|e| QuillError::Transport(format!("Failed to build request: {}", e)))?;

        // Send the request
        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| QuillError::Transport(format!("Failed to send request: {}", e)))?;

        // Check status code
        let status = resp.status();
        if !status.is_success() {
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .map_err(|e| QuillError::Transport(format!("Failed to read error response: {}", e)))?
                .to_bytes();

            if let Ok(pd) = serde_json::from_slice(&body_bytes) {
                return Err(QuillError::ProblemDetails(pd));
            }

            return Err(QuillError::Rpc(format!(
                "RPC failed with status {}: {}",
                status,
                String::from_utf8_lossy(&body_bytes)
            )));
        }

        // Create a stream that parses frames from the response
        let body = resp.into_body();
        let frame_stream = ResponseFrameStream::new(body);

        Ok(Box::pin(frame_stream))
    }
}

/// Stream adapter that parses frames from HTTP response body
struct ResponseFrameStream {
    body: hyper::body::Incoming,
    parser: FrameParser,
    credits: CreditTracker,
    messages_received: u32,
}

impl ResponseFrameStream {
    fn new(body: hyper::body::Incoming) -> Self {
        Self {
            body,
            parser: FrameParser::new(),
            credits: CreditTracker::with_defaults(),
            messages_received: 0,
        }
    }
}

impl Stream for ResponseFrameStream {
    type Item = Result<Bytes, QuillError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
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
                        // Server is granting us credits to send more requests
                        // (Useful for true bidirectional streaming in the future)
                        if let Some(amount) = frame.decode_credit() {
                            self.credits.grant(amount);
                        }
                        // Continue to next frame
                        continue;
                    }
                    if frame.flags.is_data() {
                        self.messages_received += 1;

                        // In a future HTTP/2 implementation, we would send credit frames
                        // back to the server here to grant more send credits.
                        // For now, we just track locally.
                        if self.messages_received % DEFAULT_CREDIT_REFILL == 0 {
                            // Would send credit frame to server here
                            tracing::debug!(
                                "Would grant {} credits to server (received {} messages)",
                                DEFAULT_CREDIT_REFILL,
                                self.messages_received
                            );
                        }

                        return Poll::Ready(Some(Ok(frame.payload)));
                    }
                    if frame.flags.is_cancel() {
                        // Stream was cancelled by server
                        return Poll::Ready(Some(Err(QuillError::Rpc(
                            "Stream cancelled by server".to_string()
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
                    // Body ended, but we might have buffered data
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

impl fmt::Debug for QuillClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QuillClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// Builder for configuring a Quill client
pub struct ClientBuilder {
    base_url: Option<String>,
    profile_preference: Option<ProfilePreference>,
}

impl ClientBuilder {
    /// Create a new client builder
    pub fn new() -> Self {
        Self {
            base_url: None,
            profile_preference: None,
        }
    }

    /// Set the base URL for the client
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into());
        self
    }

    /// Set the profile preference
    pub fn profile_preference(mut self, pref: ProfilePreference) -> Self {
        self.profile_preference = Some(pref);
        self
    }

    /// Build the client
    pub fn build(self) -> Result<QuillClient, String> {
        let base_url = self
            .base_url
            .ok_or_else(|| "base_url is required".to_string())?;

        let client = Client::builder(TokioExecutor::new()).build_http();

        Ok(QuillClient {
            base_url,
            client,
            profile_preference: self
                .profile_preference
                .unwrap_or_else(ProfilePreference::default_preference),
        })
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builder() {
        let client = QuillClient::builder()
            .base_url("http://localhost:8080")
            .build()
            .unwrap();

        assert_eq!(client.base_url, "http://localhost:8080");
    }
}
