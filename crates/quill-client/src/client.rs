//! Quill client implementation

use bytes::Bytes;
use http::{Method, Request};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use quill_core::{ProfilePreference, QuillError};
use std::fmt;

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
