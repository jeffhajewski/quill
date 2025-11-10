//! Client SDK for the Quill RPC framework.
//!
//! This crate provides client-side components:
//! - Client builder and connection management
//! - Unary and streaming calls
//! - Retry logic
//! - Backpressure handling

pub mod client;
pub mod retry;
pub mod streaming;

pub use client::QuillClient;
pub use streaming::RpcRequest;
