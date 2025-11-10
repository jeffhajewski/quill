//! Server SDK for the Quill RPC framework.
//!
//! This crate provides server-side components:
//! - HTTP router for RPC methods
//! - Handler traits
//! - Middleware (Problem Details, compression, tracing)
//! - Server runtime
//! - Streaming support

pub mod handler;
pub mod middleware;
pub mod request_stream;
pub mod router;
pub mod server;
pub mod streaming;

pub use handler::RpcHandler;
pub use request_stream::RequestFrameStream;
pub use router::{parse_rpc_path, RpcRouter};
pub use server::{QuillServer, ServerBuilder};
pub use streaming::{FramedResponseStream, RpcResponse};
