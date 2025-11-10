//! HTTP router for RPC methods
//!
//! Routes match the pattern: /{package}.{Service}/{Method}

use bytes::Bytes;
use http::{Method, Request, Response, StatusCode};
use http_body_util::Full;
use hyper::body::Incoming;
use quill_core::{ProblemDetails, QuillError};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Type alias for async handler functions
pub type HandlerFn =
    Arc<dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<Bytes, QuillError>> + Send>> + Send + Sync>;

/// RPC Router
pub struct RpcRouter {
    routes: HashMap<String, HandlerFn>,
}

impl RpcRouter {
    /// Create a new router
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Register a handler for a specific service method
    /// Path format: "{package}.{Service}/{Method}"
    pub fn register<F, Fut>(&mut self, path: impl Into<String>, handler: F)
    where
        F: Fn(Bytes) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Bytes, QuillError>> + Send + 'static,
    {
        let handler = Arc::new(move |req: Bytes| Box::pin(handler(req)) as Pin<Box<_>>);
        self.routes.insert(path.into(), handler);
    }

    /// Route an incoming request
    pub async fn route(&self, req: Request<Incoming>) -> Response<Full<Bytes>> {
        // Parse the path
        let path = req.uri().path();

        // Validate HTTP method (should be POST for RPC)
        if req.method() != Method::POST {
            return Self::error_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "Method not allowed",
                Some("Only POST is supported for RPC calls"),
            );
        }

        // Strip leading slash
        let path = path.strip_prefix('/').unwrap_or(path);

        // Find handler
        let handler = match self.routes.get(path) {
            Some(h) => h,
            None => {
                return Self::error_response(
                    StatusCode::NOT_FOUND,
                    "Method not found",
                    Some(&format!("No handler registered for path: /{}", path)),
                )
            }
        };

        // Read request body
        let body = match Self::read_body(req.into_body()).await {
            Ok(b) => b,
            Err(e) => {
                return Self::error_response(
                    StatusCode::BAD_REQUEST,
                    "Failed to read request body",
                    Some(&e.to_string()),
                )
            }
        };

        // Call handler
        match handler(body).await {
            Ok(response_bytes) => Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/proto")
                .body(Full::new(response_bytes))
                .unwrap(),
            Err(QuillError::ProblemDetails(pd)) => {
                // Return Problem Details as JSON
                let json = pd.to_json().unwrap_or_else(|_| "{}".to_string());
                Response::builder()
                    .status(StatusCode::from_u16(pd.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR))
                    .header("Content-Type", "application/problem+json")
                    .body(Full::new(Bytes::from(json)))
                    .unwrap()
            }
            Err(e) => Self::error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
                Some(&e.to_string()),
            ),
        }
    }

    /// Helper to read body bytes
    async fn read_body(body: Incoming) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        use http_body_util::BodyExt;
        let collected = body.collect().await?;
        Ok(collected.to_bytes())
    }

    /// Helper to create error responses
    fn error_response(status: StatusCode, title: &str, detail: Option<&str>) -> Response<Full<Bytes>> {
        let mut pd = ProblemDetails::new(status, title);
        if let Some(d) = detail {
            pd = pd.with_detail(d);
        }

        let json = pd.to_json().unwrap_or_else(|_| "{}".to_string());

        Response::builder()
            .status(status)
            .header("Content-Type", "application/problem+json")
            .body(Full::new(Bytes::from(json)))
            .unwrap()
    }
}

impl Default for RpcRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a Quill RPC path into (service, method)
/// Expected format: "{package}.{Service}/{Method}"
pub fn parse_rpc_path(path: &str) -> Option<(String, String)> {
    // Strip leading slash
    let path = path.strip_prefix('/').unwrap_or(path);

    // Split on '/'
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 2 {
        return None;
    }

    let service = parts[0].to_string();
    let method = parts[1].to_string();

    Some((service, method))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rpc_path() {
        let (service, method) = parse_rpc_path("/echo.v1.EchoService/Echo").unwrap();
        assert_eq!(service, "echo.v1.EchoService");
        assert_eq!(method, "Echo");

        let (service, method) = parse_rpc_path("media.v1.ImageService/GetMetadata").unwrap();
        assert_eq!(service, "media.v1.ImageService");
        assert_eq!(method, "GetMetadata");

        assert!(parse_rpc_path("/invalid").is_none());
    }
}
