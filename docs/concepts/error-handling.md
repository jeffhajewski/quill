# Error Handling

Quill uses **Problem Details** (RFC 7807) for structured, machine-readable errors with real HTTP status codes.

## Design Philosophy

1. **Real HTTP status codes** - No 200-with-error-envelope
2. **Structured errors** - Machine-readable JSON format
3. **Typed extensions** - Include protobuf error details
4. **Trace correlation** - Link errors to distributed traces

## Problem Details Format

```json
{
  "type": "https://quill.dev/errors/not-found",
  "title": "Not Found",
  "status": 404,
  "detail": "User with ID 'abc123' was not found",
  "instance": "/users/abc123",
  "trace_id": "4bf92f3577b34da6"
}
```

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `type` | URI | Error type identifier |
| `title` | string | Short, human-readable summary |
| `status` | integer | HTTP status code |

### Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `detail` | string | Detailed explanation |
| `instance` | URI | Specific occurrence identifier |
| `trace_id` | string | Distributed trace ID |

## Creating Errors

### Basic Error

```rust
use quill_core::{QuillError, ProblemDetails};

fn get_user(id: &str) -> Result<User, QuillError> {
    match db.find_user(id) {
        Some(user) => Ok(user),
        None => Err(QuillError::not_found(
            format!("User '{}' not found", id)
        ))
    }
}
```

### With Problem Details

```rust
use quill_core::ProblemDetails;

let error = ProblemDetails::new()
    .with_type("https://api.example.com/errors/quota-exceeded")
    .with_title("Quota Exceeded")
    .with_status(429)
    .with_detail("You have exceeded your API quota for this month")
    .with_extension("quota_limit", 1000)
    .with_extension("quota_used", 1042);
```

## Standard Error Types

| Type | Status | Usage |
|------|--------|-------|
| `invalid-argument` | 400 | Malformed request |
| `unauthenticated` | 401 | Missing/invalid auth |
| `permission-denied` | 403 | Insufficient permissions |
| `not-found` | 404 | Resource not found |
| `conflict` | 409 | Resource conflict |
| `failed-precondition` | 412 | Precondition failed |
| `resource-exhausted` | 429 | Rate limited/quota exceeded |
| `cancelled` | 499 | Client cancelled |
| `internal` | 500 | Server error |
| `not-implemented` | 501 | Method not implemented |
| `unavailable` | 503 | Service unavailable |
| `deadline-exceeded` | 504 | Timeout |

## Typed Error Extensions

Include protobuf error details for typed error handling:

```json
{
  "type": "https://api.example.com/errors/validation-failed",
  "title": "Validation Failed",
  "status": 400,
  "detail": "Request validation failed",
  "quill_proto_type": "example.v1.ValidationError",
  "quill_proto_detail_base64": "CgRuYW1lEhNOYW1lIGlzIHJlcXVpcmVk"
}
```

### Extracting Typed Details

```rust
use prost::Message;
use example::v1::ValidationError;

if let Some(proto_detail) = problem.extension::<String>("quill_proto_detail_base64") {
    let bytes = base64::decode(proto_detail)?;
    let validation_error = ValidationError::decode(&bytes[..])?;

    for field_error in validation_error.field_errors {
        println!("Field '{}': {}", field_error.field, field_error.message);
    }
}
```

## Client Error Handling

```rust
use quill_client::QuillClient;

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .build()?;

match client.call("users.v1.UserService/GetUser", request).await {
    Ok(response) => {
        let user = User::decode(response)?;
        println!("Found user: {}", user.name);
    }
    Err(QuillError::NotFound(detail)) => {
        println!("User not found: {}", detail);
    }
    Err(QuillError::PermissionDenied(detail)) => {
        println!("Access denied: {}", detail);
    }
    Err(QuillError::Problem(problem)) => {
        println!("Error {}: {}", problem.status, problem.title);
        if let Some(trace_id) = problem.trace_id {
            println!("Trace ID: {}", trace_id);
        }
    }
    Err(e) => {
        println!("Unexpected error: {}", e);
    }
}
```

## Server Error Responses

```rust
use quill_server::{QuillServer, Response};
use quill_core::ProblemDetails;

async fn get_user(request: Bytes) -> Result<Bytes, QuillError> {
    let req = GetUserRequest::decode(request)?;

    // Validation error
    if req.user_id.is_empty() {
        return Err(QuillError::invalid_argument("user_id is required"));
    }

    // Not found error
    let user = db.find_user(&req.user_id)
        .ok_or_else(|| QuillError::not_found(
            format!("User '{}' not found", req.user_id)
        ))?;

    Ok(user.encode_to_vec().into())
}
```

## HTTP Status Mapping

Quill automatically maps errors to appropriate HTTP status codes:

```
QuillError::InvalidArgument  → 400 Bad Request
QuillError::Unauthenticated  → 401 Unauthorized
QuillError::PermissionDenied → 403 Forbidden
QuillError::NotFound         → 404 Not Found
QuillError::Conflict         → 409 Conflict
QuillError::RateLimited      → 429 Too Many Requests
QuillError::Internal         → 500 Internal Server Error
QuillError::Unavailable      → 503 Service Unavailable
```

## Security Considerations

- **Never leak secrets** in error details
- **Sanitize stack traces** in production
- **Include trace IDs** for debugging (when safe)
- **Log detailed errors server-side**, return sanitized versions to clients

```rust
// Bad: Leaks internal details
Err(QuillError::internal(format!(
    "Database connection failed: {}", db_error
)))

// Good: Sanitized error with trace ID
Err(QuillError::internal("Service temporarily unavailable")
    .with_trace_id(current_trace_id()))
```

## gRPC Compatibility

When using the [gRPC Bridge](../grpc-bridge.md), errors are mapped:

| Quill Status | gRPC Code |
|--------------|-----------|
| 400 | INVALID_ARGUMENT |
| 401 | UNAUTHENTICATED |
| 403 | PERMISSION_DENIED |
| 404 | NOT_FOUND |
| 409 | ALREADY_EXISTS |
| 429 | RESOURCE_EXHAUSTED |
| 499 | CANCELLED |
| 500 | INTERNAL |
| 501 | UNIMPLEMENTED |
| 503 | UNAVAILABLE |
| 504 | DEADLINE_EXCEEDED |

## Next Steps

- [gRPC Bridge](../grpc-bridge.md) - Error mapping with gRPC
- [Middleware](../middleware.md) - Error handling middleware
- [Observability](../observability.md) - Error tracking and alerting
