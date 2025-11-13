//! Rate limiting middleware for REST gateway

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use quill_core::ProblemDetails;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Token bucket for rate limiting
#[derive(Clone)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    capacity: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            last_refill: Instant::now(),
            capacity,
            refill_rate,
        }
    }

    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;

        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    fn remaining(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

/// Rate limit configuration
#[derive(Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Time window duration
    pub window: Duration,
    /// Key extractor (e.g., IP address, API key)
    pub key_fn: Option<Arc<dyn Fn(&Request) -> Option<String> + Send + Sync>>,
}

impl RateLimitConfig {
    /// Create a new rate limit config
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            key_fn: None,
        }
    }

    /// Set custom key extractor
    pub fn key_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(&Request) -> Option<String> + Send + Sync + 'static,
    {
        self.key_fn = Some(Arc::new(f));
        self
    }

    /// Rate limit by IP address
    pub fn by_ip() -> Self {
        Self::new(100, Duration::from_secs(60)).key_fn(|req| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
                .or_else(|| Some("unknown".to_string()))
        })
    }

    /// Rate limit by API key
    pub fn by_api_key(header_name: &'static str) -> Self {
        Self::new(1000, Duration::from_secs(60)).key_fn(move |req| {
            req.headers()
                .get(header_name)
                .and_then(|h| h.to_str().ok())
                .map(|s| s.to_string())
        })
    }
}

/// Rate limiting middleware
pub struct RateLimitMiddleware {
    config: RateLimitConfig,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
}

impl RateLimitMiddleware {
    /// Create a new rate limiting middleware
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check rate limit for a key
    fn check_limit(&self, key: &str) -> Result<(), (f64, Duration)> {
        let mut buckets = self.buckets.lock().unwrap();

        let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
            let refill_rate = self.config.max_requests as f64 / self.config.window.as_secs_f64();
            TokenBucket::new(self.config.max_requests as f64, refill_rate)
        });

        if bucket.try_consume(1.0) {
            Ok(())
        } else {
            let retry_after = self.config.window;
            Err((bucket.remaining(), retry_after))
        }
    }

    /// Create middleware handler
    pub async fn handle(
        middleware: Arc<RateLimitMiddleware>,
        request: Request,
        next: Next,
    ) -> Result<Response, Response> {
        // Extract key (default to "global" if no key function)
        let key = if let Some(key_fn) = &middleware.config.key_fn {
            key_fn(&request).unwrap_or_else(|| "anonymous".to_string())
        } else {
            "global".to_string()
        };

        match middleware.check_limit(&key) {
            Ok(()) => Ok(next.run(request).await),
            Err((remaining, retry_after)) => {
                let problem = ProblemDetails {
                    type_uri: "urn:quill:rest-gateway:rate-limit-exceeded".to_string(),
                    title: "Rate Limit Exceeded".to_string(),
                    status: 429,
                    detail: Some(format!(
                        "Rate limit exceeded. Retry after {} seconds",
                        retry_after.as_secs()
                    )),
                    instance: None,
                    quill_proto_type: None,
                    quill_proto_detail_base64: None,
                };

                let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(problem)).into_response();

                // Add Retry-After header
                if let Ok(value) = retry_after.as_secs().to_string().parse() {
                    response.headers_mut().insert("retry-after", value);
                }

                // Add X-RateLimit headers
                response.headers_mut().insert(
                    "x-ratelimit-limit",
                    middleware.config.max_requests.into(),
                );
                if let Ok(value) = (remaining as u32).to_string().parse() {
                    response.headers_mut().insert("x-ratelimit-remaining", value);
                }

                Err(response)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(10.0, 1.0);
        assert_eq!(bucket.capacity, 10.0);
        assert_eq!(bucket.tokens, 10.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(10.0, 1.0);

        assert!(bucket.try_consume(5.0));
        assert!((bucket.tokens - 5.0).abs() < 0.1);

        assert!(bucket.try_consume(5.0));
        assert!(bucket.tokens < 0.1); // Close to 0

        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10.0, 10.0); // 10 tokens/second
        bucket.tokens = 0.0;

        std::thread::sleep(Duration::from_millis(500));
        bucket.refill();

        // Should have ~5 tokens after 0.5 seconds
        assert!(bucket.tokens >= 4.0 && bucket.tokens <= 6.0);
    }

    #[test]
    fn test_rate_limit_config_by_ip() {
        let config = RateLimitConfig::by_ip();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window, Duration::from_secs(60));
        assert!(config.key_fn.is_some());
    }

    #[test]
    fn test_rate_limit_config_by_api_key() {
        let config = RateLimitConfig::by_api_key("x-api-key");
        assert_eq!(config.max_requests, 1000);
        assert!(config.key_fn.is_some());
    }

    #[test]
    fn test_rate_limit_middleware() {
        let config = RateLimitConfig::new(5, Duration::from_secs(60));
        let middleware = RateLimitMiddleware::new(config);

        // Should allow 5 requests
        for _ in 0..5 {
            assert!(middleware.check_limit("test-key").is_ok());
        }

        // 6th request should fail
        assert!(middleware.check_limit("test-key").is_err());
    }

    #[test]
    fn test_rate_limit_different_keys() {
        let config = RateLimitConfig::new(2, Duration::from_secs(60));
        let middleware = RateLimitMiddleware::new(config);

        // Each key should have its own bucket
        assert!(middleware.check_limit("key1").is_ok());
        assert!(middleware.check_limit("key2").is_ok());
        assert!(middleware.check_limit("key1").is_ok());
        assert!(middleware.check_limit("key2").is_ok());

        // Both should be at limit now
        assert!(middleware.check_limit("key1").is_err());
        assert!(middleware.check_limit("key2").is_err());
    }
}
