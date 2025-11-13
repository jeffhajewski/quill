//! OpenAPI 3.0 specification generation

use crate::mapping::{HttpMethod, RouteMapping};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// OpenAPI 3.0 specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub servers: Vec<OpenApiServer>,
    pub paths: HashMap<String, OpenApiPathItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<OpenApiComponents>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiServer {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiPathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<OpenApiOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parameters: Vec<OpenApiParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<OpenApiRequestBody>,
    pub responses: HashMap<String, OpenApiResponse>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiParameter {
    pub name: String,
    #[serde(rename = "in")]
    pub location: String, // "path", "query", "header"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    pub schema: OpenApiSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    pub content: HashMap<String, OpenApiMediaType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiResponse {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<HashMap<String, OpenApiMediaType>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiMediaType {
    pub schema: OpenApiSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApiSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiComponents {
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub schemas: HashMap<String, OpenApiSchema>,
}

/// OpenAPI spec builder
pub struct OpenApiSpecBuilder {
    title: String,
    version: String,
    description: Option<String>,
    servers: Vec<OpenApiServer>,
    routes: Vec<RouteMapping>,
}

impl OpenApiSpecBuilder {
    /// Create a new OpenAPI spec builder
    pub fn new(title: &str, version: &str) -> Self {
        Self {
            title: title.to_string(),
            version: version.to_string(),
            description: None,
            servers: Vec::new(),
            routes: Vec::new(),
        }
    }

    /// Set description
    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Add a server
    pub fn server(mut self, url: &str, description: Option<&str>) -> Self {
        self.servers.push(OpenApiServer {
            url: url.to_string(),
            description: description.map(|s| s.to_string()),
        });
        self
    }

    /// Add routes
    pub fn routes(mut self, routes: Vec<RouteMapping>) -> Self {
        self.routes = routes;
        self
    }

    /// Build the OpenAPI specification
    pub fn build(self) -> OpenApiSpec {
        let mut paths = HashMap::new();

        // Group routes by URL template
        for route in &self.routes {
            for http_mapping in &route.http_mappings {
                let path_template = http_mapping.url_template.template();
                let path_item = paths.entry(path_template.to_string()).or_insert_with(|| OpenApiPathItem {
                    get: None,
                    post: None,
                    put: None,
                    patch: None,
                    delete: None,
                });

                let operation = OpenApiOperation {
                    summary: Some(format!("{}.{}", route.service, route.method)),
                    description: None,
                    operation_id: Some(format!("{}_{}", route.service.replace('.', "_"), route.method)),
                    parameters: http_mapping
                        .url_template
                        .parameter_names()
                        .into_iter()
                        .map(|name| OpenApiParameter {
                            name: name.clone(),
                            location: "path".to_string(),
                            description: None,
                            required: true,
                            schema: OpenApiSchema {
                                schema_type: "string".to_string(),
                                format: None,
                                description: None,
                            },
                        })
                        .collect(),
                    request_body: if http_mapping.http_method != HttpMethod::Get && http_mapping.http_method != HttpMethod::Delete {
                        Some(OpenApiRequestBody {
                            description: Some("Request body".to_string()),
                            required: true,
                            content: {
                                let mut content = HashMap::new();
                                content.insert(
                                    "application/json".to_string(),
                                    OpenApiMediaType {
                                        schema: OpenApiSchema {
                                            schema_type: "object".to_string(),
                                            format: None,
                                            description: None,
                                        },
                                    },
                                );
                                content
                            },
                        })
                    } else {
                        None
                    },
                    responses: {
                        let mut responses = HashMap::new();
                        responses.insert(
                            "200".to_string(),
                            OpenApiResponse {
                                description: "Successful response".to_string(),
                                content: Some({
                                    let mut content = HashMap::new();
                                    content.insert(
                                        "application/json".to_string(),
                                        OpenApiMediaType {
                                            schema: OpenApiSchema {
                                                schema_type: "object".to_string(),
                                                format: None,
                                                description: None,
                                            },
                                        },
                                    );
                                    content
                                }),
                            },
                        );
                        responses.insert(
                            "default".to_string(),
                            OpenApiResponse {
                                description: "Error response (Problem Details)".to_string(),
                                content: Some({
                                    let mut content = HashMap::new();
                                    content.insert(
                                        "application/problem+json".to_string(),
                                        OpenApiMediaType {
                                            schema: OpenApiSchema {
                                                schema_type: "object".to_string(),
                                                format: None,
                                                description: Some("RFC 7807 Problem Details".to_string()),
                                            },
                                        },
                                    );
                                    content
                                }),
                            },
                        );
                        responses
                    },
                    tags: vec![route.service.clone()],
                };

                match http_mapping.http_method {
                    HttpMethod::Get => path_item.get = Some(operation),
                    HttpMethod::Post => path_item.post = Some(operation),
                    HttpMethod::Put => path_item.put = Some(operation),
                    HttpMethod::Patch => path_item.patch = Some(operation),
                    HttpMethod::Delete => path_item.delete = Some(operation),
                }
            }
        }

        OpenApiSpec {
            openapi: "3.0.0".to_string(),
            info: OpenApiInfo {
                title: self.title,
                version: self.version,
                description: self.description,
            },
            servers: self.servers,
            paths,
            components: None,
        }
    }
}

impl OpenApiSpec {
    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Convert to YAML string (requires serde_yaml)
    pub fn to_yaml(&self) -> Result<String, serde_json::Error> {
        // For now, just use JSON; YAML can be added as a feature
        self.to_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::RouteMapping;

    #[test]
    fn test_openapi_spec_builder() {
        let route = RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/api/v1/users/{id}")
            .unwrap();

        let spec = OpenApiSpecBuilder::new("My API", "1.0.0")
            .description("Test API")
            .server("https://api.example.com", Some("Production"))
            .routes(vec![route])
            .build();

        assert_eq!(spec.openapi, "3.0.0");
        assert_eq!(spec.info.title, "My API");
        assert_eq!(spec.info.version, "1.0.0");
        assert_eq!(spec.servers.len(), 1);
        assert!(!spec.paths.is_empty());
    }

    #[test]
    fn test_openapi_json_generation() {
        let route = RouteMapping::new("users.v1.UserService", "GetUser")
            .add_mapping(HttpMethod::Get, "/api/v1/users/{id}")
            .unwrap();

        let spec = OpenApiSpecBuilder::new("My API", "1.0.0")
            .routes(vec![route])
            .build();

        let json = spec.to_json();
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("\"openapi\": \"3.0.0\""));
        assert!(json_str.contains("/api/v1/users/{id}"));
    }
}
