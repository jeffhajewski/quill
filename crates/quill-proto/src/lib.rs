//! Protobuf integration for the Quill RPC framework.
//!
//! This crate provides utilities for working with Protocol Buffers in Quill,
//! including support for Quill-specific annotations.

pub mod annotations {
    //! Quill protobuf annotations
    //!
    //! Generated from proto/quill/annotations.proto

    include!(concat!(env!("OUT_DIR"), "/quill.rs"));
}

pub use annotations::*;

/// Utilities for working with Quill RPC options
pub mod options {
    use super::*;

    /// Check if an RPC is marked as idempotent
    pub fn is_idempotent(opts: &RpcOptions) -> bool {
        opts.idempotent
    }

    /// Check if an RPC is marked as real-time
    pub fn is_real_time(opts: &RpcOptions) -> bool {
        opts.real_time
    }

    /// Get the cache TTL in milliseconds
    pub fn cache_ttl_ms(opts: &RpcOptions) -> Option<i64> {
        opts.cache_ttl_ms
    }

    /// Get the list of error types this RPC may throw
    pub fn throws(opts: &RpcOptions) -> &[String] {
        &opts.throws
    }
}
