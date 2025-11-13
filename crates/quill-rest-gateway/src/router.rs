//! REST gateway router

use crate::error::{GatewayError, GatewayResult};
use crate::mapping::{HttpMethod, RouteMapping};
use crate::openapi::{OpenApiSpec, OpenApiSpecBuilder};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, MethodRouter},
    Json, Router,
};
use quill_client::QuillClient;
use quill_core::ProblemDetails;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info};

/// REST gateway state
#[derive(Clone)]
struct GatewayState {
    client: Arc<QuillClient>,
    routes: Arc<Vec<RouteMapping>>,
}

/// REST gateway for Quill RPC services
pub struct RestGateway {
    router: Router,
    openapi_spec: OpenApiSpec,
}

impl RestGateway {
    /// Get the Axum router
    pub fn router(self) -> Router {
        self.router
    }

    /// Get the OpenAPI specification
    pub fn openapi_spec(&self) -> &OpenApiSpec {
        &self.openapi_spec
    }

    /// Get OpenAPI spec as JSON
    pub fn openapi_json(&self) -> Result<String, serde_json::Error> {
        self.openapi_spec.to_json()
    }
}

/// REST gateway builder
pub struct RestGatewayBuilder {
    client: Arc<QuillClient>,
    routes: Vec<RouteMapping>,
    title: String,
    version: String,
    description: Option<String>,
    base_path: String,
}

impl RestGatewayBuilder {
    /// Create a new REST gateway builder
    pub fn new(client: QuillClient) -> Self {
        Self {
            client: Arc::new(client),
            routes: Vec::new(),
            title: "Quill REST API".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            base_path: "/api".to_string(),
        }
    }

    /// Set API title
    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// Set API version
    pub fn version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    /// Set API description
    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Set base path for all routes
    pub fn base_path(mut self, base_path: &str) -> Self {
        self.base_path = base_path.to_string();
        self
    }

    /// Add a route mapping
    pub fn route(mut self, route: RouteMapping) -> Self {
        self.routes.push(route);
        self
    }

    /// Add multiple routes
    pub fn routes(mut self, routes: Vec<RouteMapping>) -> Self {
        self.routes.extend(routes);
        self
    }

    /// Build the REST gateway
    pub fn build(self) -> RestGateway {
        let state = GatewayState {
            client: self.client.clone(),
            routes: Arc::new(self.routes.clone()),
        };

        // Build router with all routes
        let mut router = Router::new();

        // Add OpenAPI spec endpoint
        let openapi_spec = self.build_openapi_spec();
        let openapi_json = openapi_spec.to_json().unwrap_or_else(|_| "{}".to_string());
        let openapi_router = Router::new().route(
            "/openapi.json",
            get(move || async move { Json(openapi_json) }),
        );

        router = router.merge(openapi_router);

        // Add routes
        for route in &self.routes {
            for http_mapping in &route.http_mappings {
                let path_template = format!("{}{}", self.base_path, http_mapping.url_template.template());
                let method_router = create_method_router(http_mapping.http_method, state.clone());

                router = router.route(&path_template, method_router);
            }
        }

        RestGateway {
            router,
            openapi_spec,
        }
    }

    fn build_openapi_spec(&self) -> OpenApiSpec {
        let mut builder = OpenApiSpecBuilder::new(&self.title, &self.version);

        if let Some(desc) = &self.description {
            builder = builder.description(desc);
        }

        builder.routes(self.routes.clone()).build()
    }
}

/// Create method router for specific HTTP method
fn create_method_router(http_method: HttpMethod, state: GatewayState) -> MethodRouter {
    match http_method {
        HttpMethod::Get => get(handle_get).with_state(state),
        HttpMethod::Post => axum::routing::post(handle_post).with_state(state),
        HttpMethod::Put => axum::routing::put(handle_put).with_state(state),
        HttpMethod::Patch => axum::routing::patch(handle_patch).with_state(state),
        HttpMethod::Delete => axum::routing::delete(handle_delete).with_state(state),
    }
}

/// Handle GET requests
async fn handle_get(
    State(state): State<GatewayState>,
    Path(params): Path<HashMap<String, String>>,
    req: Request<Body>,
) -> Result<Response, GatewayResponse> {
    handle_request(state, HttpMethod::Get, params, req).await
}

/// Handle POST requests
async fn handle_post(
    State(state): State<GatewayState>,
    Path(params): Path<HashMap<String, String>>,
    req: Request<Body>,
) -> Result<Response, GatewayResponse> {
    handle_request(state, HttpMethod::Post, params, req).await
}

/// Handle PUT requests
async fn handle_put(
    State(state): State<GatewayState>,
    Path(params): Path<HashMap<String, String>>,
    req: Request<Body>,
) -> Result<Response, GatewayResponse> {
    handle_request(state, HttpMethod::Put, params, req).await
}

/// Handle PATCH requests
async fn handle_patch(
    State(state): State<GatewayState>,
    Path(params): Path<HashMap<String, String>>,
    req: Request<Body>,
) -> Result<Response, GatewayResponse> {
    handle_request(state, HttpMethod::Patch, params, req).await
}

/// Handle DELETE requests
async fn handle_delete(
    State(state): State<GatewayState>,
    Path(params): Path<HashMap<String, String>>,
    req: Request<Body>,
) -> Result<Response, GatewayResponse> {
    handle_request(state, HttpMethod::Delete, params, req).await
}

/// Handle request and route to RPC
async fn handle_request(
    _state: GatewayState,
    http_method: HttpMethod,
    params: HashMap<String, String>,
    _req: Request<Body>,
) -> Result<Response, GatewayResponse> {
    debug!("Handling {} request with params: {:?}", http_method.as_str(), params);

    // TODO: Implement actual RPC call logic
    // For now, return a placeholder response

    let response = serde_json::json!({
        "message": "REST gateway placeholder",
        "method": http_method.as_str(),
        "params": params,
    });

    Ok(Json(response).into_response())
}

/// Gateway response wrapper for error handling
struct GatewayResponse(GatewayError);

impl IntoResponse for GatewayResponse {
    fn into_response(self) -> Response {
        let problem = self.0.to_problem_details();
        let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        (status, Json(problem)).into_response()
    }
}

impl From<GatewayError> for GatewayResponse {
    fn from(err: GatewayError) -> Self {
        error!("Gateway error: {}", err);
        GatewayResponse(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill_client::client::ClientBuilder;

    #[test]
    fn test_gateway_builder() {
        let client = ClientBuilder::new()
            .base_url("http://localhost:8080")
            .build()
            .unwrap();

        let route = RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/v1/users/{id}")
            .unwrap();

        let gateway = RestGatewayBuilder::new(client)
            .title("Test API")
            .version("1.0.0")
            .description("Test REST API")
            .route(route)
            .build();

        assert_eq!(gateway.openapi_spec().info.title, "Test API");
        assert_eq!(gateway.openapi_spec().info.version, "1.0.0");
    }

    #[test]
    fn test_openapi_json_generation() {
        let client = ClientBuilder::new()
            .base_url("http://localhost:8080")
            .build()
            .unwrap();

        let route = RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/v1/users/{id}")
            .unwrap();

        let gateway = RestGatewayBuilder::new(client)
            .route(route)
            .build();

        let json = gateway.openapi_json();
        assert!(json.is_ok());
        assert!(json.unwrap().contains("openapi"));
    }
}
