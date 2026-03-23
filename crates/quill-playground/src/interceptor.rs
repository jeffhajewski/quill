//! Interceptor trait and chain for request/response interception.
//!
//! Interceptors allow you to modify requests and responses as they
//! pass through the RPC pipeline. The playground uses interceptors for:
//!
//! - Latency injection
//! - Partition simulation
//! - Telemetry collection
//!
//! # Example
//!
//! ```ignore
//! use quill_playground::{Interceptor, InterceptorChain, InterceptContext};
//! use async_trait::async_trait;
//!
//! struct LoggingInterceptor;
//!
//! #[async_trait]
//! impl Interceptor for LoggingInterceptor {
//!     async fn intercept_request(
//!         &self,
//!         ctx: &InterceptContext,
//!         request: Bytes,
//!     ) -> InterceptResult<Bytes> {
//!         println!("Request to {}/{}", ctx.service_name, ctx.method_name);
//!         Ok(request)
//!     }
//! }
//!
//! let mut chain = InterceptorChain::new();
//! chain.add(LoggingInterceptor);
//! ```

use crate::error::PlaygroundError;
use async_trait::async_trait;
use bytes::Bytes;
use quill_core::playground::InterceptContext;
use quill_core::QuillError;
use std::sync::Arc;

/// Result type for interceptor operations.
pub type InterceptResult<T> = Result<T, InterceptError>;

/// Errors that can occur during interception.
#[derive(Debug)]
pub enum InterceptError {
    /// Playground-specific error (partition, timeout, etc.)
    Playground(PlaygroundError),
    /// Underlying RPC error
    Rpc(QuillError),
    /// Request should be aborted
    Abort(String),
}

impl From<PlaygroundError> for InterceptError {
    fn from(err: PlaygroundError) -> Self {
        Self::Playground(err)
    }
}

impl From<QuillError> for InterceptError {
    fn from(err: QuillError) -> Self {
        Self::Rpc(err)
    }
}

impl std::fmt::Display for InterceptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Playground(e) => write!(f, "Playground error: {}", e),
            Self::Rpc(e) => write!(f, "RPC error: {}", e),
            Self::Abort(msg) => write!(f, "Request aborted: {}", msg),
        }
    }
}

impl std::error::Error for InterceptError {}

/// Trait for intercepting RPC requests and responses.
///
/// Interceptors form a chain that processes requests before they are sent
/// and responses after they are received. Each interceptor can:
///
/// - Modify the request/response
/// - Short-circuit the chain by returning an error
/// - Emit telemetry events
/// - Inject delays
///
/// # Implementation Notes
///
/// - Interceptors should be stateless or use interior mutability
/// - `intercept_request` is called in order (first to last)
/// - `intercept_response` is called in reverse order (last to first)
/// - Errors from `intercept_request` abort the request
/// - Errors from `intercept_response` replace the original response
#[async_trait]
pub trait Interceptor: Send + Sync {
    /// Intercept an outgoing request.
    ///
    /// Called before the request is sent. Return the (possibly modified)
    /// request bytes, or an error to abort the request.
    async fn intercept_request(
        &self,
        ctx: &InterceptContext,
        request: Bytes,
    ) -> InterceptResult<Bytes> {
        // Default: pass through unchanged
        let _ = ctx;
        Ok(request)
    }

    /// Intercept an incoming response.
    ///
    /// Called after the response is received. Return the (possibly modified)
    /// response, or an error to replace the response.
    async fn intercept_response(
        &self,
        ctx: &InterceptContext,
        response: Result<Bytes, QuillError>,
    ) -> Result<Bytes, QuillError> {
        // Default: pass through unchanged
        let _ = ctx;
        response
    }

    /// Get the name of this interceptor (for debugging).
    fn name(&self) -> &'static str {
        "unnamed"
    }
}

/// A chain of interceptors that processes requests and responses.
///
/// The chain executes interceptors in order for requests and in reverse
/// order for responses, following the "onion" model common in middleware.
pub struct InterceptorChain {
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl Default for InterceptorChain {
    fn default() -> Self {
        Self::new()
    }
}

impl InterceptorChain {
    /// Create a new empty interceptor chain.
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    /// Add an interceptor to the end of the chain.
    pub fn add<I: Interceptor + 'static>(&mut self, interceptor: I) {
        self.interceptors.push(Arc::new(interceptor));
    }

    /// Add a shared interceptor to the chain.
    pub fn add_shared(&mut self, interceptor: Arc<dyn Interceptor>) {
        self.interceptors.push(interceptor);
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.interceptors.is_empty()
    }

    /// Get the number of interceptors in the chain.
    pub fn len(&self) -> usize {
        self.interceptors.len()
    }

    /// Process a request through all interceptors.
    ///
    /// Interceptors are called in order. If any interceptor returns an
    /// error, processing stops and the error is returned.
    pub async fn intercept_request(
        &self,
        ctx: &InterceptContext,
        mut request: Bytes,
    ) -> InterceptResult<Bytes> {
        for interceptor in &self.interceptors {
            request = interceptor.intercept_request(ctx, request).await?;
        }
        Ok(request)
    }

    /// Process a response through all interceptors.
    ///
    /// Interceptors are called in reverse order. Each interceptor receives
    /// the result of the previous one.
    pub async fn intercept_response(
        &self,
        ctx: &InterceptContext,
        mut response: Result<Bytes, QuillError>,
    ) -> Result<Bytes, QuillError> {
        for interceptor in self.interceptors.iter().rev() {
            response = interceptor.intercept_response(ctx, response).await;
        }
        response
    }
}

impl Clone for InterceptorChain {
    fn clone(&self) -> Self {
        Self {
            interceptors: self.interceptors.clone(),
        }
    }
}

/// A function-based interceptor for simple use cases.
pub struct FnInterceptor<F, R>
where
    F: Fn(&InterceptContext, Bytes) -> R + Send + Sync,
    R: std::future::Future<Output = InterceptResult<Bytes>> + Send,
{
    name: &'static str,
    request_fn: F,
}

impl<F, R> FnInterceptor<F, R>
where
    F: Fn(&InterceptContext, Bytes) -> R + Send + Sync,
    R: std::future::Future<Output = InterceptResult<Bytes>> + Send,
{
    /// Create a new function-based interceptor.
    pub fn new(name: &'static str, request_fn: F) -> Self {
        Self { name, request_fn }
    }
}

#[async_trait]
impl<F, R> Interceptor for FnInterceptor<F, R>
where
    F: Fn(&InterceptContext, Bytes) -> R + Send + Sync,
    R: std::future::Future<Output = InterceptResult<Bytes>> + Send,
{
    async fn intercept_request(
        &self,
        ctx: &InterceptContext,
        request: Bytes,
    ) -> InterceptResult<Bytes> {
        (self.request_fn)(ctx, request).await
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingInterceptor {
        name: &'static str,
        request_count: AtomicUsize,
        response_count: AtomicUsize,
    }

    impl CountingInterceptor {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                request_count: AtomicUsize::new(0),
                response_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl Interceptor for CountingInterceptor {
        async fn intercept_request(
            &self,
            _ctx: &InterceptContext,
            request: Bytes,
        ) -> InterceptResult<Bytes> {
            self.request_count.fetch_add(1, Ordering::SeqCst);
            Ok(request)
        }

        async fn intercept_response(
            &self,
            _ctx: &InterceptContext,
            response: Result<Bytes, QuillError>,
        ) -> Result<Bytes, QuillError> {
            self.response_count.fetch_add(1, Ordering::SeqCst);
            response
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn test_interceptor_chain_empty() {
        let chain = InterceptorChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        let ctx = InterceptContext::new("test.Service", "Method");
        let request = Bytes::from("test");

        let result = chain.intercept_request(&ctx, request.clone()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), request);
    }

    #[tokio::test]
    async fn test_interceptor_chain_single() {
        let mut chain = InterceptorChain::new();
        let interceptor = Arc::new(CountingInterceptor::new("test"));
        chain.add_shared(interceptor.clone());

        let ctx = InterceptContext::new("test.Service", "Method");
        let request = Bytes::from("test");

        let result = chain.intercept_request(&ctx, request).await;
        assert!(result.is_ok());
        assert_eq!(interceptor.request_count.load(Ordering::SeqCst), 1);

        let response = chain
            .intercept_response(&ctx, Ok(Bytes::from("response")))
            .await;
        assert!(response.is_ok());
        assert_eq!(interceptor.response_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_interceptor_chain_order() {
        use std::sync::Mutex;

        struct OrderTrackingInterceptor {
            name: &'static str,
            request_order: Arc<Mutex<Vec<&'static str>>>,
            response_order: Arc<Mutex<Vec<&'static str>>>,
        }

        #[async_trait]
        impl Interceptor for OrderTrackingInterceptor {
            async fn intercept_request(
                &self,
                _ctx: &InterceptContext,
                request: Bytes,
            ) -> InterceptResult<Bytes> {
                self.request_order.lock().unwrap().push(self.name);
                Ok(request)
            }

            async fn intercept_response(
                &self,
                _ctx: &InterceptContext,
                response: Result<Bytes, QuillError>,
            ) -> Result<Bytes, QuillError> {
                self.response_order.lock().unwrap().push(self.name);
                response
            }

            fn name(&self) -> &'static str {
                self.name
            }
        }

        let request_order = Arc::new(Mutex::new(Vec::new()));
        let response_order = Arc::new(Mutex::new(Vec::new()));

        let mut chain = InterceptorChain::new();
        chain.add(OrderTrackingInterceptor {
            name: "first",
            request_order: request_order.clone(),
            response_order: response_order.clone(),
        });
        chain.add(OrderTrackingInterceptor {
            name: "second",
            request_order: request_order.clone(),
            response_order: response_order.clone(),
        });
        chain.add(OrderTrackingInterceptor {
            name: "third",
            request_order: request_order.clone(),
            response_order: response_order.clone(),
        });

        let ctx = InterceptContext::new("test.Service", "Method");

        chain
            .intercept_request(&ctx, Bytes::from("test"))
            .await
            .unwrap();
        chain
            .intercept_response(&ctx, Ok(Bytes::from("response")))
            .await
            .unwrap();

        // Request order: first -> second -> third
        assert_eq!(
            *request_order.lock().unwrap(),
            vec!["first", "second", "third"]
        );

        // Response order: third -> second -> first (reverse)
        assert_eq!(
            *response_order.lock().unwrap(),
            vec!["third", "second", "first"]
        );
    }

    #[tokio::test]
    async fn test_interceptor_error_stops_chain() {
        struct ErrorInterceptor;

        #[async_trait]
        impl Interceptor for ErrorInterceptor {
            async fn intercept_request(
                &self,
                _ctx: &InterceptContext,
                _request: Bytes,
            ) -> InterceptResult<Bytes> {
                Err(InterceptError::Abort("test error".to_string()))
            }

            fn name(&self) -> &'static str {
                "error"
            }
        }

        let counter = Arc::new(CountingInterceptor::new("counter"));

        let mut chain = InterceptorChain::new();
        chain.add(ErrorInterceptor);
        chain.add_shared(counter.clone());

        let ctx = InterceptContext::new("test.Service", "Method");
        let result = chain.intercept_request(&ctx, Bytes::from("test")).await;

        assert!(result.is_err());
        // Counter should not have been called
        assert_eq!(counter.request_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_fn_interceptor() {
        let interceptor = FnInterceptor::new("test", |_ctx, request| async move {
            let mut modified = request.to_vec();
            modified.extend_from_slice(b"-modified");
            Ok(Bytes::from(modified))
        });

        let ctx = InterceptContext::new("test.Service", "Method");
        let result = interceptor
            .intercept_request(&ctx, Bytes::from("test"))
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_ref(), b"test-modified");
    }

    #[test]
    fn test_intercept_error_display() {
        let err = InterceptError::Abort("test message".to_string());
        assert!(err.to_string().contains("test message"));

        let err = InterceptError::Playground(PlaygroundError::NotEnabled);
        assert!(err.to_string().contains("not enabled"));
    }
}
