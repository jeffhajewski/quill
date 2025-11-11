# Resilience: Retry Policies and Circuit Breakers

Quill provides built-in support for resilience patterns to handle transient failures and protect against cascading failures. This guide covers retry policies and circuit breakers.

## Table of Contents

- [Overview](#overview)
- [Retry Policies](#retry-policies)
- [Circuit Breakers](#circuit-breakers)
- [Configuration](#configuration)
- [Best Practices](#best-practices)
- [Examples](#examples)

## Overview

Resilience patterns help your application handle failures gracefully:

- **Retry Policies**: Automatically retry failed requests with exponential backoff
- **Circuit Breakers**: Fail fast when a service is unavailable, preventing cascading failures

Both patterns can be configured independently or used together for maximum resilience.

## Retry Policies

### What is a Retry Policy?

A retry policy defines how the client should retry failed requests. Quill supports:

- Configurable maximum attempts
- Exponential backoff with jitter
- Selective retries based on error type
- Customizable backoff parameters

### Default Retry Behavior

By default, retry policies will retry on:
- **408** - Request Timeout
- **429** - Too Many Requests
- **500** - Internal Server Error
- **502** - Bad Gateway
- **503** - Service Unavailable
- **504** - Gateway Timeout
- Network/transport errors

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `max_attempts` | 3 | Maximum number of retry attempts (0 = no retries) |
| `initial_backoff` | 100ms | Initial backoff duration |
| `max_backoff` | 30s | Maximum backoff duration |
| `backoff_multiplier` | 2.0 | Backoff multiplier (exponential) |
| `jitter` | 0.1 (10%) | Random jitter factor (0.0 to 1.0) |
| `retryable_status_codes` | See above | HTTP status codes to retry |

### Example: Basic Retry

```rust
use quill_client::{QuillClient, RetryPolicy};
use std::time::Duration;

// Enable retries with default policy (3 attempts, exponential backoff)
let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .enable_retries()
    .build()
    .unwrap();
```

### Example: Custom Retry Policy

```rust
use quill_client::{QuillClient, RetryPolicy};
use std::time::Duration;

// Custom retry policy
let retry_policy = RetryPolicy::new()
    .max_attempts(5)
    .initial_backoff(Duration::from_millis(50))
    .max_backoff(Duration::from_secs(10))
    .backoff_multiplier(2.0)
    .jitter(0.2);

let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .retry_policy(retry_policy)
    .build()
    .unwrap();
```

### Example: Custom Retryable Errors

```rust
use quill_client::RetryPolicy;

// Only retry on rate limiting and service unavailable
let policy = RetryPolicy::new()
    .max_attempts(3)
    .retryable_status_codes(vec![429, 503]);
```

### Backoff Calculation

Quill uses exponential backoff with jitter:

```
backoff = initial_backoff * (multiplier ^ attempt) * jitter_factor
```

Where `jitter_factor` is randomly chosen from `[1 - jitter, 1 + jitter]`.

**Example backoff sequence** (initial=100ms, multiplier=2.0, jitter=0.1):
- Attempt 1: ~200ms (180ms - 220ms)
- Attempt 2: ~400ms (360ms - 440ms)
- Attempt 3: ~800ms (720ms - 880ms)

### Manual Retry

You can also manually retry operations using `retry_with_policy`:

```rust
use quill_client::retry::{retry_with_policy, RetryPolicy};
use bytes::Bytes;

let policy = RetryPolicy::default();

let result = retry_with_policy(&policy, || async {
    client.call("service", "Method", request.clone()).await
}).await?;
```

## Circuit Breakers

### What is a Circuit Breaker?

A circuit breaker prevents requests to a failing service, allowing it time to recover. It has three states:

1. **Closed**: Requests pass through normally
2. **Open**: Requests are immediately rejected (fail fast)
3. **Half-Open**: Testing if the service has recovered

### State Transitions

```
Closed --[failure threshold]-> Open
Open --[timeout elapsed]--> Half-Open
Half-Open --[success threshold]--> Closed
Half-Open --[any failure]--> Open
```

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `failure_threshold` | 5 | Failures before opening circuit |
| `success_threshold` | 2 | Successes to close from half-open |
| `timeout` | 60s | Wait time before half-open |
| `window_duration` | 60s | Rolling window for failures |

### Example: Basic Circuit Breaker

```rust
use quill_client::QuillClient;

// Enable circuit breaker with default configuration
let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .enable_circuit_breaker()
    .build()
    .unwrap();
```

### Example: Custom Circuit Breaker

```rust
use quill_client::{QuillClient, CircuitBreakerConfig};
use std::time::Duration;

let circuit_breaker_config = CircuitBreakerConfig {
    failure_threshold: 3,
    success_threshold: 2,
    timeout: Duration::from_secs(30),
    window_duration: Duration::from_secs(60),
};

let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .circuit_breaker(circuit_breaker_config)
    .build()
    .unwrap();
```

### Circuit Breaker Behavior

**Closed State:**
- All requests pass through
- Failures are counted in a rolling window
- Opens after reaching `failure_threshold`

**Open State:**
- All requests fail immediately with "Circuit breaker is open"
- No requests reach the backend
- Transitions to half-open after `timeout`

**Half-Open State:**
- Limited requests pass through
- Any failure immediately reopens the circuit
- Success count must reach `success_threshold` to close

## Configuration

### Combining Retry and Circuit Breaker

```rust
use quill_client::{QuillClient, RetryPolicy, CircuitBreakerConfig};
use std::time::Duration;

let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .enable_retries()
    .enable_circuit_breaker()
    .build()
    .unwrap();
```

**Execution order:**
1. Check circuit breaker (fail fast if open)
2. Execute request with retry policy
3. Record success/failure in circuit breaker

### Tuning for Your Use Case

**High-traffic services:**
```rust
let policy = RetryPolicy::new()
    .max_attempts(2)  // Fewer retries to avoid overload
    .initial_backoff(Duration::from_millis(50));

let breaker = CircuitBreakerConfig {
    failure_threshold: 10,  // Higher threshold
    timeout: Duration::from_secs(30),  // Shorter recovery
    ..Default::default()
};
```

**Critical operations:**
```rust
let policy = RetryPolicy::new()
    .max_attempts(5)  // More retries
    .max_backoff(Duration::from_secs(60));  // Longer backoff

let breaker = CircuitBreakerConfig {
    failure_threshold: 3,  // Quick to open
    success_threshold: 3,  // Conservative recovery
    timeout: Duration::from_secs(120),
    ..Default::default()
};
```

**Latency-sensitive:**
```rust
let policy = RetryPolicy::new()
    .max_attempts(2)
    .initial_backoff(Duration::from_millis(10))
    .max_backoff(Duration::from_millis(100));
```

## Best Practices

### 1. Use Retries for Transient Failures

Retry policies are ideal for:
- Network hiccups
- Temporary service overload
- Deployment-related disruptions

```rust
// Good: Retry on transient errors
let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .enable_retries()
    .build()
    .unwrap();
```

### 2. Don't Retry Non-Idempotent Operations Without Care

Be careful retrying operations that aren't idempotent:

```rust
// Risky: Creating resources may succeed even if response fails
let result = client.call("payment", "ProcessPayment", request).await;

// Better: Use idempotency keys (if supported by your service)
// Or only retry on network errors, not 5xx
let policy = RetryPolicy::new()
    .retryable_status_codes(vec![408, 429]);  // Only retry timeouts and rate limits
```

### 3. Add Jitter to Prevent Thundering Herd

Always use jitter to avoid synchronized retries:

```rust
// Good: Jitter prevents thundering herd
let policy = RetryPolicy::new()
    .jitter(0.1);  // 10% jitter
```

### 4. Circuit Breakers for Dependent Services

Use circuit breakers when calling external dependencies:

```rust
// Protect your service from cascading failures
let client = QuillClient::builder()
    .base_url("http://external-service.com")
    .enable_circuit_breaker()
    .build()
    .unwrap();
```

### 5. Monitor Circuit Breaker State

Check circuit breaker state for observability:

```rust
use quill_client::CircuitState;

if let Some(breaker) = &client.config.circuit_breaker {
    match breaker.state().await {
        CircuitState::Open => {
            // Alert: Service is down
            tracing::warn!("Circuit breaker is open");
        }
        CircuitState::HalfOpen => {
            // Info: Testing recovery
            tracing::info!("Circuit breaker is half-open");
        }
        CircuitState::Closed => {
            // Normal operation
        }
    }
}
```

### 6. Set Appropriate Timeouts

Combine with HTTP timeouts for better control:

```rust
let client = QuillClient::builder()
    .base_url("http://localhost:8080")
    .enable_retries()
    .build()
    .unwrap();

// Use tokio::time::timeout for request-level timeouts
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(5),
    client.call("service", "Method", request)
).await??;
```

### 7. Different Policies for Different Services

Create separate clients for different dependency profiles:

```rust
// Critical service: aggressive retries
let critical_client = QuillClient::builder()
    .base_url("http://critical-service")
    .retry_policy(RetryPolicy::new().max_attempts(5))
    .build()
    .unwrap();

// Best-effort service: fail fast
let optional_client = QuillClient::builder()
    .base_url("http://optional-service")
    .retry_policy(RetryPolicy::new().max_attempts(1))
    .enable_circuit_breaker()
    .build()
    .unwrap();
```

## Examples

### Example: Resilient API Client

```rust
use quill_client::{QuillClient, RetryPolicy, CircuitBreakerConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a resilient client
    let client = QuillClient::builder()
        .base_url("http://api.example.com")
        .retry_policy(
            RetryPolicy::new()
                .max_attempts(3)
                .initial_backoff(Duration::from_millis(100))
                .jitter(0.2)
        )
        .circuit_breaker(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            window_duration: Duration::from_secs(60),
        })
        .build()?;

    // Make requests - retries and circuit breaking are automatic
    let response = client
        .call("user.v1.UserService", "GetUser", request_bytes)
        .await?;

    println!("User: {:?}", response);
    Ok(())
}
```

### Example: Manual Retry with Custom Logic

```rust
use quill_client::retry::{retry_with_policy, RetryPolicy};
use std::time::Duration;

let policy = RetryPolicy::new()
    .max_attempts(3)
    .initial_backoff(Duration::from_millis(100));

let result = retry_with_policy(&policy, || async {
    // Custom pre-retry logic
    tracing::info!("Attempting API call");

    let response = client
        .call("service", "Method", request.clone())
        .await?;

    // Custom validation
    if response.is_empty() {
        return Err(QuillError::Transport("Empty response".to_string()));
    }

    Ok(response)
}).await?;
```

### Example: Circuit Breaker State Monitoring

```rust
use quill_client::CircuitState;
use tokio::time::{interval, Duration};

// Monitor circuit breaker state
let mut check_interval = interval(Duration::from_secs(10));

loop {
    check_interval.tick().await;

    if let Some(breaker) = &client.config.circuit_breaker {
        match breaker.state().await {
            CircuitState::Open => {
                tracing::error!("Circuit breaker OPEN - service degraded");
            }
            CircuitState::HalfOpen => {
                tracing::warn!("Circuit breaker HALF-OPEN - testing recovery");
            }
            CircuitState::Closed => {
                tracing::info!("Circuit breaker CLOSED - service healthy");
            }
        }
    }
}
```

## Troubleshooting

### Retries Not Working

1. **Check retry policy is configured**:
   ```rust
   let client = QuillClient::builder()
       .base_url("http://localhost:8080")
       .enable_retries()  // Don't forget this!
       .build()?;
   ```

2. **Verify error is retryable**:
   ```rust
   let policy = RetryPolicy::default();
   if policy.is_retryable(&error) {
       println!("Error is retryable");
   }
   ```

3. **Check max attempts**:
   ```rust
   let policy = RetryPolicy::new()
       .max_attempts(0);  // This disables retries!
   ```

### Circuit Breaker Opening Too Quickly

Increase the failure threshold:

```rust
let breaker = CircuitBreakerConfig {
    failure_threshold: 10,  // Increase from default 5
    ..Default::default()
};
```

### Circuit Breaker Not Recovering

Check the timeout and success threshold:

```rust
let breaker = CircuitBreakerConfig {
    failure_threshold: 5,
    success_threshold: 1,  // Decrease for faster recovery
    timeout: Duration::from_secs(30),  // Decrease for faster recovery
    ..Default::default()
};
```

### Too Many Retries Causing Delays

Reduce max attempts or backoff:

```rust
let policy = RetryPolicy::new()
    .max_attempts(2)
    .max_backoff(Duration::from_secs(1));
```

## See Also

- [Client Documentation](../crates/quill-client/README.md)
- [Performance Guide](performance.md)
- [Middleware Guide](middleware.md)

## References

- [Circuit Breaker Pattern (Martin Fowler)](https://martinfowler.com/bliki/CircuitBreaker.html)
- [Retry Pattern](https://docs.microsoft.com/en-us/azure/architecture/patterns/retry)
- [Exponential Backoff and Jitter](https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/)
