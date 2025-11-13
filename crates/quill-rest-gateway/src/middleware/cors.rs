//! CORS middleware for REST gateway

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// CORS configuration
#[derive(Clone, Debug)]
pub struct CorsConfig {
    /// Allowed origins (e.g., ["https://example.com", "*"])
    pub allow_origins: Vec<String>,
    /// Allowed methods (e.g., ["GET", "POST"])
    pub allow_methods: Vec<Method>,
    /// Allowed headers (e.g., ["Content-Type", "Authorization"])
    pub allow_headers: Vec<String>,
    /// Expose headers to browser
    pub expose_headers: Vec<String>,
    /// Allow credentials (cookies, authorization headers)
    pub allow_credentials: bool,
    /// Max age for preflight cache (seconds)
    pub max_age: Option<u64>,
}

impl CorsConfig {
    /// Create a new CORS config
    pub fn new() -> Self {
        Self {
            allow_origins: vec!["*".to_string()],
            allow_methods: vec![
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ],
            allow_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
            ],
            expose_headers: Vec::new(),
            allow_credentials: false,
            max_age: Some(3600),
        }
    }

    /// Set allowed origins
    pub fn allow_origins(mut self, origins: Vec<String>) -> Self {
        self.allow_origins = origins;
        self
    }

    /// Allow all origins (*)
    pub fn allow_any_origin(mut self) -> Self {
        self.allow_origins = vec!["*".to_string()];
        self
    }

    /// Set allowed methods
    pub fn allow_methods(mut self, methods: Vec<Method>) -> Self {
        self.allow_methods = methods;
        self
    }

    /// Set allowed headers
    pub fn allow_headers(mut self, headers: Vec<String>) -> Self {
        self.allow_headers = headers;
        self
    }

    /// Set expose headers
    pub fn expose_headers(mut self, headers: Vec<String>) -> Self {
        self.expose_headers = headers;
        self
    }

    /// Allow credentials
    pub fn allow_credentials(mut self, allow: bool) -> Self {
        self.allow_credentials = allow;
        self
    }

    /// Set max age for preflight cache
    pub fn max_age(mut self, seconds: u64) -> Self {
        self.max_age = Some(seconds);
        self
    }

    /// Create permissive CORS config (allow all)
    pub fn permissive() -> Self {
        Self::new()
            .allow_any_origin()
            .allow_credentials(false)
            .max_age(86400) // 24 hours
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// CORS middleware
#[derive(Clone)]
pub struct CorsMiddleware {
    config: CorsConfig,
}

impl CorsMiddleware {
    /// Create a new CORS middleware
    pub fn new(config: CorsConfig) -> Self {
        Self { config }
    }

    /// Check if origin is allowed
    fn is_origin_allowed(&self, origin: &str) -> bool {
        if self.config.allow_origins.contains(&"*".to_string()) {
            return true;
        }
        self.config.allow_origins.contains(&origin.to_string())
    }

    /// Add CORS headers to response
    fn add_cors_headers(&self, headers: &mut HeaderMap, request_origin: Option<&str>) {
        // Access-Control-Allow-Origin
        if let Some(origin) = request_origin {
            if self.is_origin_allowed(origin) {
                if self.config.allow_origins.contains(&"*".to_string()) {
                    headers.insert(
                        "access-control-allow-origin",
                        HeaderValue::from_static("*"),
                    );
                } else {
                    if let Ok(value) = HeaderValue::from_str(origin) {
                        headers.insert("access-control-allow-origin", value);
                    }
                }
            }
        }

        // Access-Control-Allow-Methods
        let methods: Vec<String> = self
            .config
            .allow_methods
            .iter()
            .map(|m| m.as_str().to_string())
            .collect();
        if let Ok(value) = HeaderValue::from_str(&methods.join(", ")) {
            headers.insert("access-control-allow-methods", value);
        }

        // Access-Control-Allow-Headers
        if !self.config.allow_headers.is_empty() {
            if let Ok(value) = HeaderValue::from_str(&self.config.allow_headers.join(", ")) {
                headers.insert("access-control-allow-headers", value);
            }
        }

        // Access-Control-Expose-Headers
        if !self.config.expose_headers.is_empty() {
            if let Ok(value) = HeaderValue::from_str(&self.config.expose_headers.join(", ")) {
                headers.insert("access-control-expose-headers", value);
            }
        }

        // Access-Control-Allow-Credentials
        if self.config.allow_credentials {
            headers.insert(
                "access-control-allow-credentials",
                HeaderValue::from_static("true"),
            );
        }

        // Access-Control-Max-Age
        if let Some(max_age) = self.config.max_age {
            if let Ok(value) = HeaderValue::from_str(&max_age.to_string()) {
                headers.insert("access-control-max-age", value);
            }
        }
    }

    /// Create middleware handler
    pub async fn handle(
        config: Arc<CorsConfig>,
        request: Request,
        next: Next,
    ) -> Response {
        let middleware = CorsMiddleware::new((*config).clone());
        let origin = request
            .headers()
            .get("origin")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        // Handle preflight request
        if request.method() == Method::OPTIONS {
            let mut response = StatusCode::NO_CONTENT.into_response();
            middleware.add_cors_headers(response.headers_mut(), origin.as_deref());
            return response;
        }

        // Handle actual request
        let mut response = next.run(request).await;
        middleware.add_cors_headers(response.headers_mut(), origin.as_deref());
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_config_default() {
        let config = CorsConfig::new();
        assert_eq!(config.allow_origins, vec!["*"]);
        assert!(config.allow_methods.contains(&Method::GET));
        assert!(config.allow_methods.contains(&Method::POST));
        assert_eq!(config.max_age, Some(3600));
    }

    #[test]
    fn test_cors_config_builder() {
        let config = CorsConfig::new()
            .allow_origins(vec!["https://example.com".to_string()])
            .allow_methods(vec![Method::GET, Method::POST])
            .allow_credentials(true)
            .max_age(7200);

        assert_eq!(config.allow_origins, vec!["https://example.com"]);
        assert_eq!(config.allow_methods.len(), 2);
        assert!(config.allow_credentials);
        assert_eq!(config.max_age, Some(7200));
    }

    #[test]
    fn test_origin_validation() {
        let config = CorsConfig::new().allow_origins(vec![
            "https://example.com".to_string(),
            "https://api.example.com".to_string(),
        ]);
        let middleware = CorsMiddleware::new(config);

        assert!(middleware.is_origin_allowed("https://example.com"));
        assert!(middleware.is_origin_allowed("https://api.example.com"));
        assert!(!middleware.is_origin_allowed("https://evil.com"));
    }

    #[test]
    fn test_wildcard_origin() {
        let config = CorsConfig::new().allow_any_origin();
        let middleware = CorsMiddleware::new(config);

        assert!(middleware.is_origin_allowed("https://any-domain.com"));
        assert!(middleware.is_origin_allowed("http://localhost:3000"));
    }

    #[test]
    fn test_permissive_config() {
        let config = CorsConfig::permissive();
        assert_eq!(config.allow_origins, vec!["*"]);
        assert!(!config.allow_credentials); // Can't use * with credentials
        assert_eq!(config.max_age, Some(86400));
    }
}
