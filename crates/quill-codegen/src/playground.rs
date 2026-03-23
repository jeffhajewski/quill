//! Playground-specific code generation.
//!
//! When playground mode is enabled, this module generates:
//! - Method metadata structs for runtime inspection
//! - Helper code for interceptor integration

use crate::{method_type, MethodType, QuillConfig};
use heck::ToSnakeCase;
use prost_build::{Method, Service};
use quote::{format_ident, quote};

/// Generate playground method metadata for a service.
///
/// This generates a constant struct containing metadata about each method
/// that can be used by interceptors and the playground controller.
pub fn generate_playground_metadata(service: &Service, _config: &QuillConfig) -> proc_macro2::TokenStream {
    let metadata_mod_name = format_ident!("{}_playground", service.name.to_snake_case());
    let service_name = &service.name;
    let package = &service.package;
    let full_service_name = format!("{}.{}", package, service_name);

    let method_metadata: Vec<proc_macro2::TokenStream> = service
        .methods
        .iter()
        .map(|method| generate_method_metadata(method))
        .collect();

    let method_count = service.methods.len();
    let method_names: Vec<String> = service.methods.iter().map(|m| m.name.clone()).collect();

    quote! {
        /// Playground metadata for #service_name service
        pub mod #metadata_mod_name {
            use quill_core::playground::InterceptContext;

            /// Full service name including package
            pub const SERVICE_NAME: &str = #full_service_name;

            /// Number of methods in this service
            pub const METHOD_COUNT: usize = #method_count;

            /// List of all method names
            pub const METHOD_NAMES: &[&str] = &[#(#method_names),*];

            /// Metadata for each method
            #(#method_metadata)*

            /// Get method metadata by name
            pub fn get_method_metadata(method_name: &str) -> Option<MethodMetadata> {
                match method_name {
                    #(#method_names => Some(#method_names::METADATA),)*
                    _ => None,
                }
            }

            /// Create an intercept context for a method call
            pub fn create_context(method_name: &str) -> Option<InterceptContext> {
                get_method_metadata(method_name).map(|meta| {
                    let ctx = if meta.is_streaming {
                        let direction = match (meta.client_streaming, meta.server_streaming) {
                            (true, true) => quill_core::playground::context::StreamDirection::Bidirectional,
                            (true, false) => quill_core::playground::context::StreamDirection::ClientStreaming,
                            (false, true) => quill_core::playground::context::StreamDirection::ServerStreaming,
                            _ => unreachable!(),
                        };
                        InterceptContext::streaming(SERVICE_NAME, method_name, direction)
                    } else {
                        InterceptContext::new(SERVICE_NAME, method_name)
                    };
                    ctx.with_idempotent(meta.idempotent)
                       .with_real_time(meta.real_time)
                })
            }

            /// Method metadata structure
            #[derive(Debug, Clone, Copy)]
            pub struct MethodMetadata {
                /// Method name
                pub name: &'static str,
                /// Input type name (proto type)
                pub input_type: &'static str,
                /// Output type name (proto type)
                pub output_type: &'static str,
                /// Whether this method is idempotent (safe for 0-RTT)
                pub idempotent: bool,
                /// Whether this method is real-time (skip latency injection)
                pub real_time: bool,
                /// Whether this is a streaming method
                pub is_streaming: bool,
                /// Whether client streams
                pub client_streaming: bool,
                /// Whether server streams
                pub server_streaming: bool,
            }
        }
    }
}

/// Generate metadata for a single method.
fn generate_method_metadata(method: &Method) -> proc_macro2::TokenStream {
    let method_mod_name = format_ident!("{}", method.name);
    let method_name = &method.name;
    let input_type = &method.input_proto_type;
    let output_type = &method.output_proto_type;

    // Default values - in future these could be read from proto options
    let idempotent = false;
    let real_time = false;

    let method_ty = method_type(method);
    let is_streaming = method_ty != MethodType::Unary;
    let client_streaming = method.client_streaming;
    let server_streaming = method.server_streaming;

    quote! {
        /// Metadata for #method_name method
        pub mod #method_mod_name {
            use super::MethodMetadata;

            /// Static metadata for this method
            pub const METADATA: MethodMetadata = MethodMetadata {
                name: #method_name,
                input_type: #input_type,
                output_type: #output_type,
                idempotent: #idempotent,
                real_time: #real_time,
                is_streaming: #is_streaming,
                client_streaming: #client_streaming,
                server_streaming: #server_streaming,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_build::{Method, Service};

    fn make_test_service() -> Service {
        Service {
            name: "TestService".to_string(),
            proto_name: "TestService".to_string(),
            package: "test.v1".to_string(),
            comments: Default::default(),
            options: Default::default(),
            methods: vec![
                Method {
                    name: "UnaryCall".to_string(),
                    proto_name: "UnaryCall".to_string(),
                    comments: Default::default(),
                    input_type: "Request".to_string(),
                    output_type: "Response".to_string(),
                    input_proto_type: "test.v1.Request".to_string(),
                    output_proto_type: "test.v1.Response".to_string(),
                    options: Default::default(),
                    client_streaming: false,
                    server_streaming: false,
                },
                Method {
                    name: "ServerStream".to_string(),
                    proto_name: "ServerStream".to_string(),
                    comments: Default::default(),
                    input_type: "Request".to_string(),
                    output_type: "Response".to_string(),
                    input_proto_type: "test.v1.Request".to_string(),
                    output_proto_type: "test.v1.Response".to_string(),
                    options: Default::default(),
                    client_streaming: false,
                    server_streaming: true,
                },
            ],
        }
    }

    #[test]
    fn test_generate_playground_metadata() {
        let service = make_test_service();
        let config = QuillConfig::default().with_playground(true);
        let code = generate_playground_metadata(&service, &config);

        let code_str = code.to_string();
        assert!(code_str.contains("test_service_playground"));
        assert!(code_str.contains("SERVICE_NAME"));
        assert!(code_str.contains("test.v1.TestService"));
        assert!(code_str.contains("METHOD_COUNT"));
        assert!(code_str.contains("MethodMetadata"));
    }

    #[test]
    fn test_generate_method_metadata() {
        let method = Method {
            name: "TestMethod".to_string(),
            proto_name: "TestMethod".to_string(),
            comments: Default::default(),
            input_type: "Request".to_string(),
            output_type: "Response".to_string(),
            input_proto_type: "test.v1.Request".to_string(),
            output_proto_type: "test.v1.Response".to_string(),
            options: Default::default(),
            client_streaming: false,
            server_streaming: false,
        };

        let code = generate_method_metadata(&method);
        let code_str = code.to_string();

        assert!(code_str.contains("TestMethod"));
        assert!(code_str.contains("test.v1.Request"));
        assert!(code_str.contains("test.v1.Response"));
        assert!(code_str.contains("idempotent"));
        assert!(code_str.contains("real_time"));
    }

    #[test]
    fn test_streaming_method_metadata() {
        let method = Method {
            name: "BidiStream".to_string(),
            proto_name: "BidiStream".to_string(),
            comments: Default::default(),
            input_type: "Request".to_string(),
            output_type: "Response".to_string(),
            input_proto_type: "test.v1.Request".to_string(),
            output_proto_type: "test.v1.Response".to_string(),
            options: Default::default(),
            client_streaming: true,
            server_streaming: true,
        };

        let code = generate_method_metadata(&method);
        let code_str = code.to_string();

        assert!(code_str.contains("is_streaming : true"));
        assert!(code_str.contains("client_streaming : true"));
        assert!(code_str.contains("server_streaming : true"));
    }
}
