//! Code generator (protoc plugin) for the Quill RPC framework.
//!
//! This crate provides code generation for Quill services from .proto files,
//! generating type-safe client and server stubs.

pub mod client;
pub mod server;
pub mod service;

use prost_build::{Config, Method, Service};
use std::io::Result;
use std::path::Path;

/// Configuration for Quill code generation
#[derive(Debug, Clone)]
pub struct QuillConfig {
    /// Generate client code
    pub generate_client: bool,
    /// Generate server code
    pub generate_server: bool,
    /// Package name prefix
    pub package_prefix: Option<String>,
}

impl Default for QuillConfig {
    fn default() -> Self {
        Self {
            generate_client: true,
            generate_server: true,
            package_prefix: None,
        }
    }
}

impl QuillConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn client_only() -> Self {
        Self {
            generate_client: true,
            generate_server: false,
            package_prefix: None,
        }
    }

    pub fn server_only() -> Self {
        Self {
            generate_client: false,
            generate_server: true,
            package_prefix: None,
        }
    }

    pub fn with_package_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.package_prefix = Some(prefix.into());
        self
    }
}

/// Generate Quill RPC code from protobuf files
///
/// This function integrates with prost-build to generate Quill client and server code.
///
/// # Example
///
/// ```no_run
/// # use quill_codegen::{QuillConfig, compile_protos};
/// # fn main() -> std::io::Result<()> {
/// let config = QuillConfig::new();
/// compile_protos(&["proto/myservice.proto"], &["proto"], config)?;
/// # Ok(())
/// # }
/// ```
pub fn compile_protos(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
    config: QuillConfig,
) -> Result<()> {
    let mut prost_config = Config::new();

    // Configure prost to generate code
    prost_config.service_generator(Box::new(QuillServiceGenerator::new(config)));

    // Compile the protos
    prost_config.compile_protos(protos, includes)?;

    Ok(())
}

/// Service generator for Quill RPC
struct QuillServiceGenerator {
    config: QuillConfig,
}

impl QuillServiceGenerator {
    fn new(config: QuillConfig) -> Self {
        Self { config }
    }
}

impl prost_build::ServiceGenerator for QuillServiceGenerator {
    fn generate(&mut self, service: Service, buf: &mut String) {
        // Generate client code
        if self.config.generate_client {
            if let Some(client_code) = client::generate_client(&service, &self.config) {
                buf.push_str(&client_code);
                buf.push('\n');
            }
        }

        // Generate server code
        if self.config.generate_server {
            if let Some(server_code) = server::generate_server(&service, &self.config) {
                buf.push_str(&server_code);
                buf.push('\n');
            }
        }
    }
}

/// Helper function to get method streaming type
pub fn method_type(method: &Method) -> MethodType {
    match (method.client_streaming, method.server_streaming) {
        (false, false) => MethodType::Unary,
        (false, true) => MethodType::ServerStreaming,
        (true, false) => MethodType::ClientStreaming,
        (true, true) => MethodType::BidirectionalStreaming,
    }
}

/// RPC method type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodType {
    Unary,
    ServerStreaming,
    ClientStreaming,
    BidirectionalStreaming,
}

impl MethodType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MethodType::Unary => "unary",
            MethodType::ServerStreaming => "server_streaming",
            MethodType::ClientStreaming => "client_streaming",
            MethodType::BidirectionalStreaming => "bidirectional_streaming",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = QuillConfig::default();
        assert!(config.generate_client);
        assert!(config.generate_server);
        assert!(config.package_prefix.is_none());
    }

    #[test]
    fn test_client_only_config() {
        let config = QuillConfig::client_only();
        assert!(config.generate_client);
        assert!(!config.generate_server);
    }

    #[test]
    fn test_server_only_config() {
        let config = QuillConfig::server_only();
        assert!(!config.generate_client);
        assert!(config.generate_server);
    }

    #[test]
    fn test_method_type() {
        assert_eq!(MethodType::Unary.as_str(), "unary");
        assert_eq!(MethodType::ServerStreaming.as_str(), "server_streaming");
    }
}
