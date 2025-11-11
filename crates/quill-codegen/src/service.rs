//! Service-level utilities for Quill code generation

use prost_build::{Comments, Method, Service};

/// Format service path for RPC routing
pub fn format_service_path(service: &Service, prefix: Option<&str>) -> String {
    if let Some(p) = prefix {
        format!("{}.{}.{}", p, service.package, service.name)
    } else {
        format!("{}.{}", service.package, service.name)
    }
}

/// Format method path for RPC routing
pub fn format_method_path(service: &Service, method: &Method, prefix: Option<&str>) -> String {
    let service_path = format_service_path(service, prefix);
    format!("{}/{}", service_path, method.name)
}

/// Extract documentation from protobuf comments
pub fn format_comments(comments: &Comments) -> String {
    let mut result = String::new();

    for line in &comments.leading {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            result.push_str("/// ");
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result
}

/// Check if a method is streaming in any direction
pub fn is_streaming(method: &Method) -> bool {
    method.client_streaming || method.server_streaming
}

/// Get a human-readable description of the method streaming type
pub fn streaming_type_description(method: &Method) -> &'static str {
    match (method.client_streaming, method.server_streaming) {
        (false, false) => "unary",
        (false, true) => "server streaming",
        (true, false) => "client streaming",
        (true, true) => "bidirectional streaming",
    }
}

/// Validate service definition
pub fn validate_service(service: &Service) -> Result<(), String> {
    if service.name.is_empty() {
        return Err("Service name cannot be empty".to_string());
    }

    if service.package.is_empty() {
        return Err("Service package cannot be empty".to_string());
    }

    if service.methods.is_empty() {
        return Err(format!("Service {} has no methods", service.name));
    }

    for method in &service.methods {
        validate_method(method)?;
    }

    Ok(())
}

/// Validate method definition
pub fn validate_method(method: &Method) -> Result<(), String> {
    if method.name.is_empty() {
        return Err("Method name cannot be empty".to_string());
    }

    if method.input_type.is_empty() {
        return Err(format!("Method {} has no input type", method.name));
    }

    if method.output_type.is_empty() {
        return Err(format!("Method {} has no output type", method.name));
    }

    Ok(())
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
            comments: Comments::default(),
            options: Default::default(),
            methods: vec![Method {
                name: "UnaryCall".to_string(),
                proto_name: "UnaryCall".to_string(),
                comments: Comments::default(),
                input_type: "Request".to_string(),
                output_type: "Response".to_string(),
                input_proto_type: "Request".to_string(),
                output_proto_type: "Response".to_string(),
                options: Default::default(),
                client_streaming: false,
                server_streaming: false,
            }],
        }
    }

    #[test]
    fn test_format_service_path() {
        let service = make_test_service();
        let path = format_service_path(&service, None);
        assert_eq!(path, "test.v1.TestService");

        let path_with_prefix = format_service_path(&service, Some("myapp"));
        assert_eq!(path_with_prefix, "myapp.test.v1.TestService");
    }

    #[test]
    fn test_format_method_path() {
        let service = make_test_service();
        let method = &service.methods[0];
        let path = format_method_path(&service, method, None);
        assert_eq!(path, "test.v1.TestService/UnaryCall");
    }

    #[test]
    fn test_streaming_type_description() {
        let method = Method {
            name: "Test".to_string(),
            proto_name: "Test".to_string(),
            comments: Comments::default(),
            input_type: "In".to_string(),
            output_type: "Out".to_string(),
            input_proto_type: "In".to_string(),
            output_proto_type: "Out".to_string(),
            options: Default::default(),
            client_streaming: false,
            server_streaming: false,
        };

        assert_eq!(streaming_type_description(&method), "unary");

        let mut server_streaming = method.clone();
        server_streaming.server_streaming = true;
        assert_eq!(streaming_type_description(&server_streaming), "server streaming");
    }

    #[test]
    fn test_validate_service() {
        let service = make_test_service();
        assert!(validate_service(&service).is_ok());

        let mut invalid = service.clone();
        invalid.name = String::new();
        assert!(validate_service(&invalid).is_err());

        let mut no_methods = service.clone();
        no_methods.methods.clear();
        assert!(validate_service(&no_methods).is_err());
    }

    #[test]
    fn test_is_streaming() {
        let mut method = Method {
            name: "Test".to_string(),
            proto_name: "Test".to_string(),
            comments: Comments::default(),
            input_type: "In".to_string(),
            output_type: "Out".to_string(),
            input_proto_type: "In".to_string(),
            output_proto_type: "Out".to_string(),
            options: Default::default(),
            client_streaming: false,
            server_streaming: false,
        };

        assert!(!is_streaming(&method));

        method.server_streaming = true;
        assert!(is_streaming(&method));
    }
}
