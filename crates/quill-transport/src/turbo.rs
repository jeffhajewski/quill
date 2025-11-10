//! Turbo profile (HTTP/2) transport implementation

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use hyper::body::Incoming;
use quill_core::PrismProfile;
use std::future::Future;
use std::pin::Pin;

/// HTTP/2 transport for the Turbo profile
pub struct TurboTransport {
    profile: PrismProfile,
}

impl TurboTransport {
    /// Create a new Turbo transport
    pub fn new() -> Self {
        Self {
            profile: PrismProfile::Turbo,
        }
    }

    /// Get the profile this transport implements
    pub fn profile(&self) -> PrismProfile {
        self.profile
    }
}

impl Default for TurboTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP/2 connection handler
pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Service trait for handling HTTP/2 requests
pub trait H2Service: Clone + Send + 'static {
    fn call(&self, req: Request<Incoming>) -> BoxFuture<Result<Response<Bytes>, StatusCode>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turbo_transport() {
        let transport = TurboTransport::new();
        assert_eq!(transport.profile(), PrismProfile::Turbo);
    }
}
