//! Client SDK for the Quill RPC framework.
//!
//! This crate provides client-side components:
//! - Client builder and connection management
//! - Unary and streaming calls
//! - Retry logic
//! - Backpressure handling
//! - HTTP/3 support (with `http3` feature)

pub mod client;
#[cfg(feature = "http3")]
pub mod h3_client;
pub mod retry;
pub mod streaming;

pub use client::{ClientConfig, HttpProtocol, QuillClient};
#[cfg(feature = "http3")]
pub use h3_client::{H3ClientBuilder, H3ClientConfig, QuillH3Client};
pub use retry::{CircuitBreaker, CircuitBreakerConfig, CircuitState, RetryPolicy};
pub use streaming::RpcRequest;
