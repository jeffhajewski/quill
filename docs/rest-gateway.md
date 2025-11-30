# REST Gateway

This guide covers the REST gateway for Quill RPC services, which provides RESTful HTTP access with OpenAPI documentation.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [URL Mapping](#url-mapping)
- [HTTP Method Routing](#http-method-routing)
- [Streaming Support](#streaming-support)
- [OpenAPI Specification](#openapi-specification)
- [Error Handling](#error-handling)
- [Message Converter](#message-converter)
- [Examples](#examples)
- [Best Practices](#best-practices)
- [Middleware](#middleware)
- [Security Considerations](#security-considerations)

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

### With JSON ↔ Protobuf Conversion

To enable automatic JSON to Protobuf conversion, provide a protobuf descriptor set:

```rust
use quill_rest_gateway::{RestGatewayBuilder, RouteMapping, HttpMethod};
use quill_client::client::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load descriptor set (generated with protoc --descriptor_set_out=api.pb)
    let descriptor_bytes = std::fs::read("api.pb")?;

    // Create Quill client
    let client = ClientBuilder::new()
        .base_url("http://localhost:8080")
        .build()?;

    // Define REST routes
    let routes = vec![
        RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/v1/users/{id}")?,
    ];

    // Build REST gateway with converter
    let gateway = RestGatewayBuilder::new(client)
        .with_descriptor_bytes(&descriptor_bytes)?  // Enable JSON ↔ Protobuf
        .title("User API")
        .version("1.0.0")
        .routes(routes)
        .build();

    let app = gateway.router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
```

**Generating Descriptor Sets**:

```bash
# Single file
protoc --descriptor_set_out=api.pb --include_imports api.proto

# Multiple files
protoc --descriptor_set_out=all.pb --include_imports \
  -I./proto ./proto/**/*.proto

# With buf
buf build -o api.pb
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

## Streaming Support

The REST gateway supports streaming RPCs via Server-Sent Events (SSE) and NDJSON.

### Server-Sent Events (SSE)

Use SSE for server-streaming RPCs (real-time updates, event streams):

```rust
use quill_rest_gateway::{RestGatewayBuilder, RouteMapping, HttpMethod, StreamingConfig};

// Configure a server-streaming route with SSE
let route = RouteMapping::new("events.v1.EventService", "StreamEvents")
    .add_mapping(HttpMethod::Get, "/v1/events/stream")?
    .server_streaming();  // Enables SSE by default

// Or with custom streaming configuration
let custom_route = RouteMapping::new("logs.v1.LogService", "TailLogs")
    .add_mapping(HttpMethod::Get, "/v1/logs/tail")?
    .with_streaming_config(StreamingConfig::sse());
```

**Client Usage**:

```javascript
// JavaScript EventSource
const events = new EventSource('/api/v1/events/stream');

events.onmessage = (event) => {
    const data = JSON.parse(event.data);
    console.log('Received:', data);
};

events.onerror = (error) => {
    console.error('SSE error:', error);
    events.close();
};
```

```bash
# curl
curl -N http://localhost:3000/api/v1/events/stream \
  -H "Accept: text/event-stream"
```

**SSE Response Format**:

```
event: update
id: msg-1
data: {"type": "user_joined", "user": "alice"}

event: update
id: msg-2
data: {"type": "message", "content": "Hello!"}

```

### NDJSON Streaming

Use NDJSON (Newline-Delimited JSON) for streaming:

```rust
use quill_rest_gateway::{StreamingConfig, StreamingFormat};

// Configure NDJSON streaming
let route = RouteMapping::new("logs.v1.LogService", "TailLogs")
    .add_mapping(HttpMethod::Get, "/v1/logs/tail")?
    .with_streaming_config(StreamingConfig::ndjson());
```

**Client Usage**:

```bash
# Each line is a complete JSON object
curl http://localhost:3000/api/v1/logs/tail \
  -H "Accept: application/x-ndjson"

# Response:
{"timestamp": "2024-01-15T10:00:00Z", "message": "Log entry 1"}
{"timestamp": "2024-01-15T10:00:01Z", "message": "Log entry 2"}
{"timestamp": "2024-01-15T10:00:02Z", "message": "Log entry 3"}
```

```javascript
// JavaScript fetch with streaming
const response = await fetch('/api/v1/logs/tail', {
    headers: { 'Accept': 'application/x-ndjson' }
});

const reader = response.body.getReader();
const decoder = new TextDecoder();
let buffer = '';

while (true) {
    const { value, done } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split('\n');
    buffer = lines.pop(); // Keep incomplete line

    for (const line of lines) {
        if (line.trim()) {
            const data = JSON.parse(line);
            console.log('Received:', data);
        }
    }
}
```

### Client Streaming

For client-streaming RPCs (file uploads, batch processing):

```rust
// Configure client streaming route
let route = RouteMapping::new("upload.v1.UploadService", "Upload")
    .add_mapping(HttpMethod::Post, "/v1/upload")?
    .client_streaming();
```

**Client Usage with NDJSON**:

```bash
# Stream NDJSON data
curl -X POST http://localhost:3000/api/v1/upload \
  -H "Content-Type: application/x-ndjson" \
  -d '{"chunk": 1, "data": "base64..."}
{"chunk": 2, "data": "base64..."}
{"chunk": 3, "data": "base64..."}'
```

**Client Usage with Multipart**:

```bash
# Multipart form data
curl -X POST http://localhost:3000/api/v1/upload \
  -F "file=@document.pdf" \
  -F "metadata={\"name\": \"document.pdf\"};type=application/json"
```

### Bidirectional Streaming

For bidirectional streaming (chat, real-time collaboration):

```rust
// Configure bidirectional streaming
let route = RouteMapping::new("chat.v1.ChatService", "Chat")
    .add_mapping(HttpMethod::Post, "/v1/chat")?
    .bidirectional_streaming();
```

Note: Full bidirectional streaming over HTTP requires WebSocket or HTTP/2 push.
For REST, this typically means client sends requests and receives SSE responses.

### Streaming Configuration Options

```rust
use quill_rest_gateway::{StreamingConfig, StreamingFormat};

// SSE configuration
let sse_config = StreamingConfig {
    enable_sse: true,
    enable_ndjson: false,
    enable_client_streaming: false,
    default_format: Some(StreamingFormat::Sse),
    keep_alive_secs: Some(30),  // SSE keep-alive ping interval
};

// NDJSON configuration
let ndjson_config = StreamingConfig {
    enable_sse: false,
    enable_ndjson: true,
    enable_client_streaming: false,
    default_format: Some(StreamingFormat::Ndjson),
    keep_alive_secs: None,
};

// Client streaming configuration
let client_config = StreamingConfig {
    enable_sse: false,
    enable_ndjson: false,
    enable_client_streaming: true,
    default_format: None,
    keep_alive_secs: None,
};

// Full bidirectional (for WebSocket upgrade or SSE + NDJSON)
let bidi_config = StreamingConfig::bidirectional();
```

### Streaming Response Builder

Build streaming responses programmatically:

```rust
use quill_rest_gateway::{StreamingResponse, StreamingFormat, SseEvent};
use futures_util::stream;

// Build SSE response
let values = vec![
    serde_json::json!({"event": "start"}),
    serde_json::json!({"event": "data", "value": 42}),
    serde_json::json!({"event": "end"}),
];
let stream = stream::iter(values);

let response = StreamingResponse::new(StreamingFormat::Sse)
    .with_keep_alive(30)
    .build(stream);
```

### Chunked Request Reader

Parse streaming client requests:

```rust
use quill_rest_gateway::{ChunkedRequestReader, ContentType};

// Create reader from Content-Type header
let mut reader = ChunkedRequestReader::from_content_type("application/x-ndjson");

// Feed chunks as they arrive
let chunks = reader.feed(b"{\"msg\":\"hello\"}\n{\"msg\":\"world\"}\n");
for chunk in chunks {
    if let Some(json) = chunk.to_json() {
        println!("Received: {}", json);
    }
}

// Get any remaining data
if let Some(remaining) = reader.finish() {
    println!("Final chunk: {:?}", remaining.to_json());
}
```

### Streaming Modes

| Mode | Direction | Format | Use Case |
|------|-----------|--------|----------|
| Server Streaming | Server → Client | SSE, NDJSON | Real-time updates, log tailing |
| Client Streaming | Client → Server | NDJSON, Multipart | File uploads, batch imports |
| Bidirectional | Both | SSE + NDJSON | Chat, collaborative editing |

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
| `rpc-not-found` | 404 | RPC service or method not found |
| `rpc-error` | 500 | RPC call failed |
| `no-converter` | 500 | No message converter configured |
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

## Message Converter

The `MessageConverter` enables automatic JSON ↔ Protobuf conversion using dynamic message reflection via `prost-reflect`.

### How It Works

1. **Request Flow**: JSON request body → Protobuf bytes → RPC call
2. **Response Flow**: RPC response (Protobuf) → JSON response

### Configuration

```rust
use quill_rest_gateway::MessageConverter;

// Create from descriptor bytes
let converter = MessageConverter::from_bytes(&descriptor_bytes)?;

// Use with gateway builder
let gateway = RestGatewayBuilder::new(client)
    .with_converter(converter)  // Or use with_descriptor_bytes()
    .routes(routes)
    .build();
```

### Parameter Handling

**Path Parameters**: Automatically merged into the request JSON:

```
URL: /v1/users/123
Request body: {"name": "Alice"}
Merged request: {"id": "123", "name": "Alice"}
```

**Query Parameters** (GET requests): Also merged into request:

```
URL: /v1/users?limit=10&offset=0
Merged request: {"limit": "10", "offset": "0"}
```

### Without Converter

Without a converter configured, the gateway will return a `no-converter` error:

```json
{
  "type": "urn:quill:rest-gateway:no-converter",
  "title": "Converter Not Configured",
  "status": 500,
  "detail": "No message converter configured for JSON/Protobuf conversion"
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

## Middleware

The REST gateway includes built-in middleware for authentication, CORS, and rate limiting.

### Authentication Middleware

Protect your API with multiple authentication schemes:

```rust
use quill_rest_gateway::{AuthConfig, AuthMiddleware};
use axum::middleware;
use std::sync::Arc;

// Bearer token authentication
let auth_config = AuthConfig::new()
    .bearer("your-secret-token-here");

let app = gateway.router()
    .layer(middleware::from_fn({
        let config = Arc::new(auth_config);
        move |req, next| AuthMiddleware::handle(config.clone(), req, next)
    }));
```

#### Authentication Schemes

**Bearer Token** (e.g., JWT):
```rust
let config = AuthConfig::new()
    .bearer("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...");

// Request: Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**API Key**:
```rust
let config = AuthConfig::new()
    .api_key("X-API-Key", "secret-key-123");

// Request: X-API-Key: secret-key-123
```

**Basic Authentication**:
```rust
let config = AuthConfig::new()
    .basic("username", "password");

// Request: Authorization: Basic dXNlcm5hbWU6cGFzc3dvcmQ=
```

**Custom Validator**:
```rust
let config = AuthConfig::new()
    .custom(|headers| {
        // Custom validation logic
        headers.get("x-custom-header").is_some()
    });
```

**Multiple Schemes** (any valid):
```rust
let config = AuthConfig::new()
    .bearer("token1")
    .api_key("X-API-Key", "key1")
    .basic("user", "pass");
// Client can use any of the above schemes
```

### CORS Middleware

Enable Cross-Origin Resource Sharing for browser clients:

```rust
use quill_rest_gateway::{CorsConfig, CorsMiddleware};
use axum::middleware;
use std::sync::Arc;

// Permissive CORS (allow all origins)
let cors_config = CorsConfig::permissive();

// Or configure specific origins
let cors_config = CorsConfig::new()
    .allow_origins(vec![
        "https://app.example.com".to_string(),
        "https://admin.example.com".to_string(),
    ])
    .allow_methods(vec![Method::GET, Method::POST, Method::PUT, Method::DELETE])
    .allow_credentials(true)
    .max_age(86400); // 24 hours

let app = gateway.router()
    .layer(middleware::from_fn({
        let config = Arc::new(cors_config);
        move |req, next| CorsMiddleware::handle(config.clone(), req, next)
    }));
```

**CORS Headers Set**:
- `Access-Control-Allow-Origin`
- `Access-Control-Allow-Methods`
- `Access-Control-Allow-Headers`
- `Access-Control-Allow-Credentials`
- `Access-Control-Max-Age`

### Rate Limiting Middleware

Protect your API from abuse with token bucket rate limiting:

```rust
use quill_rest_gateway::{RateLimitConfig, RateLimitMiddleware};
use axum::middleware;
use std::sync::Arc;
use std::time::Duration;

// Rate limit by IP address (100 requests per minute)
let rate_limit_config = RateLimitConfig::by_ip();

// Or by API key (1000 requests per minute)
let rate_limit_config = RateLimitConfig::by_api_key("x-api-key");

// Or custom configuration
let rate_limit_config = RateLimitConfig::new(100, Duration::from_secs(60))
    .key_fn(|req| {
        // Extract key from request (e.g., user ID from token)
        Some("user-123".to_string())
    });

let middleware_instance = Arc::new(RateLimitMiddleware::new(rate_limit_config));

let app = gateway.router()
    .layer(middleware::from_fn({
        let mw = middleware_instance.clone();
        move |req, next| RateLimitMiddleware::handle(mw.clone(), req, next)
    }));
```

**Rate Limit Headers** (returned on 429):
- `Retry-After`: Seconds until reset
- `X-RateLimit-Limit`: Max requests per window
- `X-RateLimit-Remaining`: Remaining requests

**Rate Limit Error Response** (429 Too Many Requests):
```json
{
  "type": "urn:quill:rest-gateway:rate-limit-exceeded",
  "title": "Rate Limit Exceeded",
  "status": 429,
  "detail": "Rate limit exceeded. Retry after 60 seconds"
}
```

### Combining Middleware

Stack multiple middleware layers:

```rust
use axum::middleware;
use std::sync::Arc;

let auth_config = Arc::new(AuthConfig::new().bearer("token"));
let cors_config = Arc::new(CorsConfig::permissive());
let rate_limit = Arc::new(RateLimitMiddleware::new(RateLimitConfig::by_ip()));

let app = gateway.router()
    // CORS first (handles preflight)
    .layer(middleware::from_fn({
        let config = cors_config.clone();
        move |req, next| CorsMiddleware::handle(config.clone(), req, next)
    }))
    // Then rate limiting
    .layer(middleware::from_fn({
        let mw = rate_limit.clone();
        move |req, next| RateLimitMiddleware::handle(mw.clone(), req, next)
    }))
    // Finally authentication
    .layer(middleware::from_fn({
        let config = auth_config.clone();
        move |req, next| AuthMiddleware::handle(config.clone(), req, next)
    }));
```

## Security Considerations

### Best Practices

1. **Always use HTTPS** in production
2. **Enable authentication** for all non-public endpoints
3. **Use rate limiting** to prevent abuse
4. **Enable CORS** only for trusted origins
5. **Validate API keys** server-side (never expose in client code)
6. **Rotate secrets** regularly (tokens, API keys)
7. **Monitor for suspicious activity** (failed auth attempts, rate limit hits)

## See Also

- [gRPC Bridge](grpc-bridge.md) - Bridge to gRPC services
- [Middleware Guide](middleware.md) - Authentication, rate limiting, etc.
- [OpenAPI Specification](https://swagger.io/specification/) - OpenAPI 3.0 standard
- [RFC 7807 Problem Details](https://www.rfc-editor.org/rfc/rfc7807.html) - Error response format
