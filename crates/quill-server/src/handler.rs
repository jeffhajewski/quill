//! RPC handler trait

use bytes::Bytes;
use quill_core::QuillError;
use std::future::Future;

/// Trait for RPC handlers
pub trait RpcHandler: Send + Sync + 'static {
    /// Handle a unary RPC call
    fn handle_unary(
        &self,
        method: &str,
        request: Bytes,
    ) -> impl Future<Output = Result<Bytes, QuillError>> + Send;
}
