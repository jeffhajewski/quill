# REST Gateway

This guide covers the REST gateway for Quill RPC services, which provides RESTful HTTP access with OpenAPI documentation.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [URL Mapping](#url-mapping)
- [HTTP Method Routing](#http-method-routing)
- [OpenAPI Specification](#openapi-specification)
- [Error Handling](#error-handling)
- [Examples](#examples)
- [Best Practices](#best-practices)

## Overview

The REST gateway provides a bridge between REST/HTTP clients and Quill RPC services:

- **Clean REST URLs**: `/api/v1/users/123` instead of `/media.v1.ImageService/GetMetadata`
- **HTTP Method Routing**: GET, POST, PUT, PATCH, DELETE
- **Automatic JSON Conversion**: JSON ↔ Protobuf
- **OpenAPI 3.0 Generation**: Auto-generated API documentation
- **Problem Details Errors**: RFC 7807-compliant error responses

### Use Cases

- **Browser/Mobile Clients**: REST APIs are more familiar than RPC
- **Third-Party Integrations**: REST is the de facto standard for public APIs
- **API Documentation**: OpenAPI provides interactive documentation (Swagger UI)
- **HTTP Caching**: GET requests can leverage HTTP caching

## Quick Start

### Basic Setup

```rust
use quill_rest_gateway::{RestGatewayBuilder, RouteMapping, HttpMethod};
use quill_client::client::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create Quill client
    let client = ClientBuilder::new()
        .base_url("http://localhost:8080")
        .build()?;

    // Define REST routes
    let user_routes = vec![
        RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/v1/users/{id}")?,
        RouteMapping::new("users.v1.UserService", "CreateUser")
            .add_mapping(HttpMethod::Post, "/v1/users")?,
        RouteMapping::new("users.v1.UserService", "UpdateUser")
            .add_mapping(HttpMethod::Put, "/v1/users/{id}")?,
        RouteMapping::new("users.v1.UserService", "DeleteUser")
            .add_mapping(HttpMethod::Delete, "/v1/users/{id}")?,
    ];

    // Build REST gateway
    let gateway = RestGatewayBuilder::new(client)
        .title("User API")
        .version("1.0.0")
        .description("REST API for user management")
        .base_path("/api")
        .routes(user_routes)
        .build();

    // Get Axum router
    let app = gateway.router();

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
```

### Making REST Calls

```bash
# GET request
curl http://localhost:3000/api/v1/users/123

# POST request
curl -X POST http://localhost:3000/api/v1/users \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "email": "alice@example.com"}'

# PUT request
curl -X PUT http://localhost:3000/api/v1/users/123 \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice Updated", "email": "alice@example.com"}'

# DELETE request
curl -X DELETE http://localhost:3000/api/v1/users/123

# Get OpenAPI spec
curl http://localhost:3000/openapi.json
```

## URL Mapping

### URL Templates

URL templates use `{param}` syntax for path parameters:

```rust
// Single parameter
RouteMapping::new("users.v1.UserService", "GetUser")
    .add_mapping(HttpMethod::Get, "/v1/users/{id}")?

// Multiple parameters
RouteMapping::new("posts.v1.PostService", "GetComment")
    .add_mapping(HttpMethod::Get, "/v1/posts/{post_id}/comments/{comment_id}")?

// Nested resources
RouteMapping::new("orgs.v1.OrgService", "GetTeamMember")
    .add_mapping(HttpMethod::Get, "/v1/orgs/{org_id}/teams/{team_id}/members/{user_id}")?
```

### Parameter Extraction

Path parameters are automatically extracted and passed to the RPC method:

```rust
// URL: /v1/users/123
// Extracted: {"id": "123"}

// URL: /v1/posts/456/comments/789
// Extracted: {"post_id": "456", "comment_id": "789"}
```

### Base Path

Set a base path for all routes:

```rust
let gateway = RestGatewayBuilder::new(client)
    .base_path("/api")  // All routes prefixed with /api
    .routes(routes)
    .build();

// Routes become: /api/v1/users/{id}, /api/v1/posts, etc.
```

## HTTP Method Routing

### HTTP Method Semantics

Map RPC methods to appropriate HTTP methods:

| HTTP Method | Semantics | RPC Examples |
|-------------|-----------|--------------|
| GET | Read, idempotent | GetUser, ListUsers, SearchPosts |
| POST | Create, non-idempotent | CreateUser, UploadFile, ProcessOrder |
| PUT | Full replace | UpdateUser, ReplaceSettings |
| PATCH | Partial update | UpdateUserEmail, PatchProfile |
| DELETE | Remove | DeleteUser, RemovePost |

### Routing Examples

```rust
// Read operations (GET)
RouteMapping::new("users.v1.UserService", "GetUser")
    .add_mapping(HttpMethod::Get, "/v1/users/{id}")?

RouteMapping::new("users.v1.UserService", "ListUsers")
    .add_mapping(HttpMethod::Get, "/v1/users")?

// Create operations (POST)
RouteMapping::new("users.v1.UserService", "CreateUser")
    .add_mapping(HttpMethod::Post, "/v1/users")?

// Update operations (PUT/PATCH)
RouteMapping::new("users.v1.UserService", "UpdateUser")
    .add_mapping(HttpMethod::Put, "/v1/users/{id}")?

RouteMapping::new("users.v1.UserService", "PatchUser")
    .add_mapping(HttpMethod::Patch, "/v1/users/{id}")?

// Delete operations (DELETE)
RouteMapping::new("users.v1.UserService", "DeleteUser")
    .add_mapping(HttpMethod::Delete, "/v1/users/{id}")?
```

### Multiple Mappings

One RPC method can have multiple REST mappings:

```rust
RouteMapping::new("users.v1.UserService", "GetUser")
    .add_mapping(HttpMethod::Get, "/v1/users/{id}")?
    .add_mapping(HttpMethod::Get, "/v1/profiles/{id}")?  // Alias
```

## OpenAPI Specification

### Automatic Generation

The gateway automatically generates OpenAPI 3.0 specs:

```rust
let gateway = RestGatewayBuilder::new(client)
    .title("My API")
    .version("1.0.0")
    .description("Complete API for my application")
    .routes(routes)
    .build();

// Get OpenAPI spec as JSON
let spec_json = gateway.openapi_json()?;
println!("{}", spec_json);
```

### OpenAPI Endpoint

The gateway exposes the spec at `/openapi.json`:

```bash
curl http://localhost:3000/openapi.json
```

### Swagger UI Integration

Serve Swagger UI to browse the API:

```rust
use axum::Router;
use tower_http::services::ServeDir;

let app = gateway.router()
    .nest_service("/swagger", ServeDir::new("./swagger-ui"));

// Visit: http://localhost:3000/swagger
```

### Example OpenAPI Output

```json
{
  "openapi": "3.0.0",
  "info": {
    "title": "User API",
    "version": "1.0.0",
    "description": "REST API for user management"
  },
  "paths": {
    "/api/v1/users/{id}": {
      "get": {
        "summary": "users.v1.UserService.GetUser",
        "operationId": "users_v1_UserService_GetUser",
        "parameters": [
          {
            "name": "id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Successful response",
            "content": {
              "application/json": {
                "schema": {
                  "type": "object"
                }
              }
            }
          },
          "default": {
            "description": "Error response (Problem Details)",
            "content": {
              "application/problem+json": {
                "schema": {
                  "type": "object"
                }
              }
            }
          }
        }
      }
    }
  }
}
```

## Error Handling

### Problem Details (RFC 7807)

All errors return Problem Details JSON:

```json
{
  "type": "urn:quill:rest-gateway:route-not-found",
  "title": "Route Not Found",
  "status": 404,
  "detail": "No route found for path: /api/v1/unknown"
}
```

### Error Types

| Error Type | HTTP Status | Description |
|------------|-------------|-------------|
| `route-not-found` | 404 | No matching route |
| `method-not-allowed` | 405 | HTTP method not supported for route |
| `invalid-request` | 400 | Malformed request body |
| `invalid-path-param` | 400 | Invalid path parameter |
| `missing-field` | 400 | Required field missing |
| `rpc-error` | 500 | RPC call failed |
| `internal-error` | 500 | Gateway internal error |

### Error Handling Example

```bash
# Route not found
$ curl http://localhost:3000/api/v1/unknown
{
  "type": "urn:quill:rest-gateway:route-not-found",
  "title": "Route Not Found",
  "status": 404,
  "detail": "No route found for path: /api/v1/unknown"
}

# Method not allowed
$ curl -X POST http://localhost:3000/api/v1/users/123
{
  "type": "urn:quill:rest-gateway:method-not-allowed",
  "title": "Method Not Allowed",
  "status": 405,
  "detail": "POST not allowed for path: /api/v1/users/123"
}
```

## Examples

### CRUD Service

Complete CRUD API for a user service:

```rust
use quill_rest_gateway::{RestGatewayBuilder, RouteMapping, HttpMethod};
use quill_client::client::ClientBuilder;

async fn setup_user_api() -> RestGatewayBuilder {
    let client = ClientBuilder::new()
        .base_url("http://localhost:8080")
        .build()
        .unwrap();

    let routes = vec![
        // Create
        RouteMapping::new("users.v1.UserService", "CreateUser")
            .add_mapping(HttpMethod::Post, "/v1/users").unwrap(),

        // Read
        RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/v1/users/{id}").unwrap(),

        RouteMapping::new("users.v1.UserService", "ListUsers")
            .add_mapping(HttpMethod::Get, "/v1/users").unwrap(),

        // Update
        RouteMapping::new("users.v1.UserService", "UpdateUser")
            .add_mapping(HttpMethod::Put, "/v1/users/{id}").unwrap(),

        // Delete
        RouteMapping::new("users.v1.UserService", "DeleteUser")
            .add_mapping(HttpMethod::Delete, "/v1/users/{id}").unwrap(),
    ];

    RestGatewayBuilder::new(client)
        .title("User API")
        .version("1.0.0")
        .base_path("/api")
        .routes(routes)
}
```

### Nested Resources

API with nested resources:

```rust
let routes = vec![
    // Posts
    RouteMapping::new("posts.v1.PostService", "GetPost")
        .add_mapping(HttpMethod::Get, "/v1/posts/{post_id}").unwrap(),

    // Comments (nested under posts)
    RouteMapping::new("posts.v1.PostService", "ListComments")
        .add_mapping(HttpMethod::Get, "/v1/posts/{post_id}/comments").unwrap(),

    RouteMapping::new("posts.v1.PostService", "GetComment")
        .add_mapping(HttpMethod::Get, "/v1/posts/{post_id}/comments/{comment_id}").unwrap(),

    RouteMapping::new("posts.v1.PostService", "CreateComment")
        .add_mapping(HttpMethod::Post, "/v1/posts/{post_id}/comments").unwrap(),
];
```

## Best Practices

### 1. RESTful URL Design

Use nouns, not verbs:

```rust
// Good
"/v1/users/{id}"
"/v1/posts/{id}/comments"

// Bad
"/v1/getUser/{id}"
"/v1/createPost"
```

### 2. HTTP Method Semantics

Use correct HTTP methods:

```rust
// Good
GET /v1/users/{id}        // Read
POST /v1/users            // Create
PUT /v1/users/{id}        // Full replace
DELETE /v1/users/{id}     // Delete

// Bad
POST /v1/users/{id}       // Should be GET
GET /v1/users/delete/{id} // Should be DELETE
```

### 3. Versioning

Include version in URL:

```rust
let gateway = RestGatewayBuilder::new(client)
    .base_path("/api")  // /api/v1/users, /api/v1/posts, etc.
    .routes(routes)
    .build();
```

### 4. Error Handling

Always return Problem Details for errors.

### 5. Documentation

Use descriptive titles and descriptions:

```rust
let gateway = RestGatewayBuilder::new(client)
    .title("User Management API")
    .version("1.0.0")
    .description("Complete REST API for managing users, profiles, and authentication")
    .routes(routes)
    .build();
```

### 6. OpenAPI Integration

Expose OpenAPI spec and integrate with Swagger UI for interactive documentation.

## Security Considerations

### Authentication

Add authentication middleware to the Axum router:

```rust
use axum::middleware;

let app = gateway.router()
    .layer(middleware::from_fn(auth_middleware));
```

### Rate Limiting

Use tower middleware for rate limiting:

```rust
use tower::ServiceBuilder;
use tower_http::limit::RequestBodyLimitLayer;

let app = gateway.router()
    .layer(
        ServiceBuilder::new()
            .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1 MB
    );
```

### CORS

Enable CORS for browser clients:

```rust
use tower_http::cors::{CorsLayer, Any};

let cors = CorsLayer::new()
    .allow_origin(Any)
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE]);

let app = gateway.router().layer(cors);
```

## See Also

- [gRPC Bridge](grpc-bridge.md) - Bridge to gRPC services
- [Middleware Guide](middleware.md) - Authentication, rate limiting, etc.
- [OpenAPI Specification](https://swagger.io/specification/) - OpenAPI 3.0 standard
- [RFC 7807 Problem Details](https://www.rfc-editor.org/rfc/rfc7807.html) - Error response format
