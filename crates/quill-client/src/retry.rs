//! Retry logic and circuit breaker for client
//!
//! This module provides:
//! - Configurable retry policies with exponential backoff
//! - Circuit breaker pattern for fault tolerance
//! - Idempotency key support for safe retries

use quill_core::QuillError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_attempts: u32,
    /// Initial backoff duration
    pub initial_backoff: Duration,
    /// Maximum backoff duration
    pub max_backoff: Duration,
    /// Backoff multiplier (e.g., 2.0 for exponential)
    pub backoff_multiplier: f64,
    /// Add random jitter to backoff (0.0 to 1.0)
    pub jitter: f64,
    /// Only retry on specific errors
    pub retryable_status_codes: Vec<u16>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter: 0.1,
            retryable_status_codes: vec![
                408, // Request Timeout
                429, // Too Many Requests
                500, // Internal Server Error
                502, // Bad Gateway
                503, // Service Unavailable
                504, // Gateway Timeout
            ],
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum number of retry attempts
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = attempts;
        self
    }

    /// Set initial backoff duration
    pub fn initial_backoff(mut self, duration: Duration) -> Self {
        self.initial_backoff = duration;
        self
    }

    /// Set maximum backoff duration
    pub fn max_backoff(mut self, duration: Duration) -> Self {
        self.max_backoff = duration;
        self
    }

    /// Set backoff multiplier
    pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Set jitter factor (0.0 to 1.0)
    pub fn jitter(mut self, jitter: f64) -> Self {
        self.jitter = jitter.clamp(0.0, 1.0);
        self
    }

    /// Set retryable status codes
    pub fn retryable_status_codes(mut self, codes: Vec<u16>) -> Self {
        self.retryable_status_codes = codes;
        self
    }

    /// Check if an error is retryable
    pub fn is_retryable(&self, error: &QuillError) -> bool {
        match error {
            QuillError::ProblemDetails(details) => {
                self.retryable_status_codes.contains(&details.status)
            }
            QuillError::Transport(_) => true, // Network errors are retryable
            _ => false,
        }
    }

    /// Calculate backoff duration for the given attempt
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let base_millis = self.initial_backoff.as_millis() as f64;
        let multiplier = self.backoff_multiplier.powi(attempt as i32);
        let backoff_millis = base_millis * multiplier;

        // Apply jitter
        let jitter_factor = if self.jitter > 0.0 {
            1.0 + (rand::random::<f64>() * self.jitter * 2.0 - self.jitter)
        } else {
            1.0
        };

        let final_millis = (backoff_millis * jitter_factor) as u64;
        let duration = Duration::from_millis(final_millis);

        // Cap at max_backoff
        if duration > self.max_backoff {
            self.max_backoff
        } else {
            duration
        }
    }
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests pass through
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Number of successes to close circuit from half-open
    pub success_threshold: u32,
    /// Duration to wait before transitioning from open to half-open
    pub timeout: Duration,
    /// Rolling window for tracking failures
    pub window_duration: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            window_duration: Duration::from_secs(60),
        }
    }
}

/// Circuit breaker for fault tolerance
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitBreakerState>>,
}

#[derive(Debug)]
struct CircuitBreakerState {
    current_state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(CircuitBreakerState {
                current_state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                opened_at: None,
            })),
        }
    }

    /// Check if a request can proceed
    pub async fn allow_request(&self) -> Result<(), QuillError> {
        let mut state = self.state.write().await;

        match state.current_state {
            CircuitState::Closed => Ok(()),
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed() >= self.config.timeout {
                        // Transition to half-open
                        state.current_state = CircuitState::HalfOpen;
                        state.success_count = 0;
                        state.failure_count = 0;
                        Ok(())
                    } else {
                        Err(QuillError::Transport(
                            "Circuit breaker is open".to_string(),
                        ))
                    }
                } else {
                    Err(QuillError::Transport(
                        "Circuit breaker is open".to_string(),
                    ))
                }
            }
            CircuitState::HalfOpen => Ok(()),
        }
    }

    /// Record a successful request
    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        match state.current_state {
            CircuitState::Closed => {
                // Reset failure count on success
                state.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    // Transition to closed
                    state.current_state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.opened_at = None;
                }
            }
            CircuitState::Open => {
                // Should not happen, but reset if it does
                state.current_state = CircuitState::Closed;
                state.failure_count = 0;
                state.success_count = 0;
                state.opened_at = None;
            }
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self) {
        let mut state = self.state.write().await;
        let now = Instant::now();

        // Check if we should reset the window
        if let Some(last_failure) = state.last_failure_time {
            if now.duration_since(last_failure) >= self.config.window_duration {
                state.failure_count = 0;
            }
        }

        state.last_failure_time = Some(now);

        match state.current_state {
            CircuitState::Closed => {
                state.failure_count += 1;
                if state.failure_count >= self.config.failure_threshold {
                    // Transition to open
                    state.current_state = CircuitState::Open;
                    state.opened_at = Some(now);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open transitions back to open
                state.current_state = CircuitState::Open;
                state.opened_at = Some(now);
                state.success_count = 0;
            }
            CircuitState::Open => {
                // Already open, nothing to do
            }
        }
    }

    /// Get the current circuit state
    pub async fn state(&self) -> CircuitState {
        self.state.read().await.current_state
    }
}

/// Execute a function with retry logic
pub async fn retry_with_policy<F, Fut, T>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, QuillError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, QuillError>>,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                attempt += 1;

                // Check if we should retry
                if attempt >= policy.max_attempts || !policy.is_retryable(&error) {
                    return Err(error);
                }

                // Calculate and wait for backoff
                let backoff = policy.backoff_duration(attempt);
                tokio::time::sleep(backoff).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_defaults() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff, Duration::from_millis(100));
        assert_eq!(policy.max_backoff, Duration::from_secs(30));
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_policy_builder() {
        let policy = RetryPolicy::new()
            .max_attempts(5)
            .initial_backoff(Duration::from_millis(50))
            .max_backoff(Duration::from_secs(10))
            .backoff_multiplier(3.0)
            .jitter(0.2);

        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_backoff, Duration::from_millis(50));
        assert_eq!(policy.max_backoff, Duration::from_secs(10));
        assert_eq!(policy.backoff_multiplier, 3.0);
        assert_eq!(policy.jitter, 0.2);
    }

    #[test]
    fn test_backoff_duration() {
        let policy = RetryPolicy::new()
            .initial_backoff(Duration::from_millis(100))
            .backoff_multiplier(2.0)
            .jitter(0.0); // No jitter for predictable testing

        let backoff1 = policy.backoff_duration(1);
        let backoff2 = policy.backoff_duration(2);
        let backoff3 = policy.backoff_duration(3);

        assert_eq!(backoff1, Duration::from_millis(200));
        assert_eq!(backoff2, Duration::from_millis(400));
        assert_eq!(backoff3, Duration::from_millis(800));
    }

    #[test]
    fn test_backoff_max_cap() {
        let policy = RetryPolicy::new()
            .initial_backoff(Duration::from_secs(1))
            .max_backoff(Duration::from_secs(5))
            .backoff_multiplier(10.0)
            .jitter(0.0);

        let backoff = policy.backoff_duration(5);
        assert!(backoff <= Duration::from_secs(5));
    }

    #[test]
    fn test_is_retryable() {
        use quill_core::ProblemDetails;
        let policy = RetryPolicy::default();

        assert!(policy.is_retryable(&QuillError::Transport("network error".to_string())));

        let retryable_error = QuillError::ProblemDetails(ProblemDetails {
            type_uri: "urn:quill:error:503".to_string(),
            title: "Service Unavailable".to_string(),
            status: 503,
            detail: None,
            instance: None,
            quill_proto_type: None,
            quill_proto_detail_base64: None,
        });
        assert!(policy.is_retryable(&retryable_error));

        let non_retryable_error = QuillError::ProblemDetails(ProblemDetails {
            type_uri: "urn:quill:error:400".to_string(),
            title: "Bad Request".to_string(),
            status: 400,
            detail: None,
            instance: None,
            quill_proto_type: None,
            quill_proto_detail_base64: None,
        });
        assert!(!policy.is_retryable(&non_retryable_error));
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_to_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(1),
            window_duration: Duration::from_secs(60),
        };

        let breaker = CircuitBreaker::new(config);
        assert_eq!(breaker.state().await, CircuitState::Closed);

        // Record failures
        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);

        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_to_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            window_duration: Duration::from_secs(60),
        };

        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);

        // Should reject requests
        assert!(breaker.allow_request().await.is_err());

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open
        assert!(breaker.allow_request().await.is_ok());
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_to_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            window_duration: Duration::from_secs(60),
        };

        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        breaker.record_failure().await;

        // Wait for timeout to transition to half-open
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = breaker.allow_request().await;

        // Record successes
        breaker.record_success().await;
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);

        breaker.record_success().await;
        assert_eq!(breaker.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_to_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
            window_duration: Duration::from_secs(60),
        };

        let breaker = CircuitBreaker::new(config);

        // Open the circuit
        breaker.record_failure().await;
        breaker.record_failure().await;

        // Wait for timeout to transition to half-open
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = breaker.allow_request().await;
        assert_eq!(breaker.state().await, CircuitState::HalfOpen);

        // Any failure in half-open goes back to open
        breaker.record_failure().await;
        assert_eq!(breaker.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_retry_with_policy_success() {
        let policy = RetryPolicy::new().max_attempts(3);
        let mut call_count = 0;

        let result = retry_with_policy(&policy, || {
            call_count += 1;
            async { Ok::<i32, QuillError>(42) }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count, 1);
    }

    #[tokio::test]
    async fn test_retry_with_policy_eventual_success() {
        let policy = RetryPolicy::new()
            .max_attempts(3)
            .initial_backoff(Duration::from_millis(10))
            .jitter(0.0);

        let mut call_count = 0;

        let result = retry_with_policy(&policy, || {
            call_count += 1;
            async move {
                if call_count < 3 {
                    Err(QuillError::Transport("network error".to_string()))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count, 3);
    }

    #[tokio::test]
    async fn test_retry_with_policy_max_attempts() {
        let policy = RetryPolicy::new()
            .max_attempts(3)
            .initial_backoff(Duration::from_millis(10))
            .jitter(0.0);

        let mut call_count = 0;

        let result = retry_with_policy(&policy, || {
            call_count += 1;
            async { Err::<i32, QuillError>(QuillError::Transport("network error".to_string())) }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count, 3);
    }
}
