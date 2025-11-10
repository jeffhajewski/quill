//! Quill server implementation

use crate::router::RpcRouter;
use crate::streaming::RpcResponse;
use bytes::Bytes;
use http::Request;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use quill_core::QuillError;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};

/// Quill RPC server
pub struct QuillServer {
    router: Arc<RpcRouter>,
}

impl QuillServer {
    /// Create a new server with a router
    pub fn new(router: RpcRouter) -> Self {
        Self {
            router: Arc::new(router),
        }
    }

    /// Create a builder for configuring the server
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    /// Serve the server on the given address
    pub async fn serve(self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        info!("Quill server listening on {}", addr);

        loop {
            let (stream, remote_addr) = listener.accept().await?;
            let router = Arc::clone(&self.router);

            tokio::spawn(async move {
                let io = TokioIo::new(stream);

                let service = hyper::service::service_fn(move |req: Request<Incoming>| {
                    let router = Arc::clone(&router);
                    async move { Ok::<_, hyper::Error>(router.route(req).await) }
                });

                if let Err(err) = auto::Builder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await
                {
                    error!("Error serving connection from {}: {}", remote_addr, err);
                }
            });
        }
    }
}

/// Builder for creating a Quill server
pub struct ServerBuilder {
    router: RpcRouter,
}

impl ServerBuilder {
    /// Create a new server builder
    pub fn new() -> Self {
        Self {
            router: RpcRouter::new(),
        }
    }

    /// Register a unary handler for an RPC method
    /// Path format: "{package}.{Service}/{Method}"
    pub fn register<F, Fut>(mut self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Bytes) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Bytes, QuillError>> + Send + 'static,
    {
        self.router.register_unary(path, handler);
        self
    }

    /// Register a streaming handler for an RPC method
    /// Path format: "{package}.{Service}/{Method}"
    pub fn register_streaming<F, Fut>(mut self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Bytes) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<RpcResponse, QuillError>> + Send + 'static,
    {
        self.router.register(path, handler);
        self
    }

    /// Build the server
    pub fn build(self) -> QuillServer {
        QuillServer::new(self.router)
    }
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}
