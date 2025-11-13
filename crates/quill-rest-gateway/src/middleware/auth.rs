//! Authentication middleware for REST gateway

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use quill_core::ProblemDetails;
use std::sync::Arc;

/// Authentication scheme
#[derive(Clone)]
pub enum AuthScheme {
    /// Bearer token (e.g., JWT)
    Bearer { token: String },
    /// API key in header
    ApiKey { header_name: String, key: String },
    /// Basic authentication
    Basic { username: String, password: String },
    /// Custom validator function
    Custom {
        validator: Arc<dyn Fn(&HeaderMap) -> bool + Send + Sync>,
    },
}

/// Authentication configuration
#[derive(Clone)]
pub struct AuthConfig {
    schemes: Vec<AuthScheme>,
    require_auth: bool,
}

impl AuthConfig {
    /// Create a new auth config
    pub fn new() -> Self {
        Self {
            schemes: Vec::new(),
            require_auth: true,
        }
    }

    /// Add a Bearer token scheme
    pub fn bearer(mut self, token: impl Into<String>) -> Self {
        self.schemes.push(AuthScheme::Bearer {
            token: token.into(),
        });
        self
    }

    /// Add an API key scheme
    pub fn api_key(mut self, header_name: impl Into<String>, key: impl Into<String>) -> Self {
        self.schemes.push(AuthScheme::ApiKey {
            header_name: header_name.into(),
            key: key.into(),
        });
        self
    }

    /// Add Basic authentication
    pub fn basic(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.schemes.push(AuthScheme::Basic {
            username: username.into(),
            password: password.into(),
        });
        self
    }

    /// Add custom validator
    pub fn custom<F>(mut self, validator: F) -> Self
    where
        F: Fn(&HeaderMap) -> bool + Send + Sync + 'static,
    {
        self.schemes.push(AuthScheme::Custom {
            validator: Arc::new(validator),
        });
        self
    }

    /// Set whether authentication is required
    pub fn require_auth(mut self, require: bool) -> Self {
        self.require_auth = require;
        self
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Authentication middleware
#[derive(Clone)]
pub struct AuthMiddleware {
    config: AuthConfig,
}

impl AuthMiddleware {
    /// Create a new auth middleware
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    /// Validate authentication
    fn validate(&self, headers: &HeaderMap) -> bool {
        if self.config.schemes.is_empty() {
            return !self.config.require_auth;
        }

        for scheme in &self.config.schemes {
            if self.validate_scheme(scheme, headers) {
                return true;
            }
        }

        false
    }

    fn validate_scheme(&self, scheme: &AuthScheme, headers: &HeaderMap) -> bool {
        match scheme {
            AuthScheme::Bearer { token } => {
                if let Some(auth_header) = headers.get("authorization") {
                    if let Ok(value) = auth_header.to_str() {
                        if let Some(bearer_token) = value.strip_prefix("Bearer ") {
                            return bearer_token == token;
                        }
                    }
                }
                false
            }
            AuthScheme::ApiKey { header_name, key } => {
                if let Some(header_value) = headers.get(header_name.as_str()) {
                    if let Ok(value) = header_value.to_str() {
                        return value == key;
                    }
                }
                false
            }
            AuthScheme::Basic { username, password } => {
                if let Some(auth_header) = headers.get("authorization") {
                    if let Ok(value) = auth_header.to_str() {
                        if let Some(basic_creds) = value.strip_prefix("Basic ") {
                            let expected = format!("{}:{}", username, password);
                            let expected_b64 = base64_encode(expected.as_bytes()).unwrap();
                            return basic_creds == expected_b64;
                        }
                    }
                }
                false
            }
            AuthScheme::Custom { validator } => validator(headers),
        }
    }

    /// Create middleware handler
    pub async fn handle(
        config: Arc<AuthConfig>,
        request: Request,
        next: Next,
    ) -> Result<Response, Response> {
        let middleware = AuthMiddleware::new((*config).clone());

        if !middleware.validate(request.headers()) {
            let problem = ProblemDetails {
                type_uri: "urn:quill:rest-gateway:unauthorized".to_string(),
                title: "Unauthorized".to_string(),
                status: 401,
                detail: Some("Authentication required".to_string()),
                instance: None,
                quill_proto_type: None,
                quill_proto_detail_base64: None,
            };

            return Err((StatusCode::UNAUTHORIZED, Json(problem)).into_response());
        }

        Ok(next.run(request).await)
    }
}

// Helper function for base64 encoding
fn base64_encode(data: &[u8]) -> Result<String, ()> {
    Ok(base64::engine::general_purpose::STANDARD.encode(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_bearer_token_validation() {
        let config = AuthConfig::new().bearer("test-token-123");
        let middleware = AuthMiddleware::new(config);

        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer test-token-123"),
        );

        assert!(middleware.validate(&headers));

        // Wrong token
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer wrong-token"),
        );

        assert!(!middleware.validate(&headers));
    }

    #[test]
    fn test_api_key_validation() {
        let config = AuthConfig::new().api_key("X-API-Key", "secret-key");
        let middleware = AuthMiddleware::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("secret-key"));

        assert!(middleware.validate(&headers));

        // Wrong key
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("wrong-key"));

        assert!(!middleware.validate(&headers));
    }

    #[test]
    fn test_multiple_schemes() {
        let config = AuthConfig::new()
            .bearer("token1")
            .api_key("X-API-Key", "key1");
        let middleware = AuthMiddleware::new(config);

        // Bearer token should work
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer token1"));
        assert!(middleware.validate(&headers));

        // API key should work
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("key1"));
        assert!(middleware.validate(&headers));
    }

    #[test]
    fn test_no_auth_required() {
        let config = AuthConfig::new().require_auth(false);
        let middleware = AuthMiddleware::new(config);

        let headers = HeaderMap::new();
        assert!(middleware.validate(&headers));
    }

    #[test]
    fn test_custom_validator() {
        let config = AuthConfig::new().custom(|headers| {
            headers.get("x-custom-header").is_some()
        });
        let middleware = AuthMiddleware::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("x-custom-header", HeaderValue::from_static("value"));
        assert!(middleware.validate(&headers));

        let headers = HeaderMap::new();
        assert!(!middleware.validate(&headers));
    }
}
