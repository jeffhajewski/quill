//! Server SDK for the Quill RPC framework.
//!
//! This crate provides server-side components:
//! - HTTP router for RPC methods
//! - Handler traits
//! - Middleware (Problem Details, compression, tracing)
//! - Server runtime

pub mod handler;
pub mod middleware;
pub mod router;
pub mod server;

pub use handler::RpcHandler;
pub use router::{parse_rpc_path, RpcRouter};
pub use server::{QuillServer, ServerBuilder};
