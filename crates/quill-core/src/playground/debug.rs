//! ToDebugJson trait for message serialization.
//!
//! This trait is implemented by generated code to serialize protobuf
//! messages to JSON for visualization in the playground dashboard.

use serde_json::Value;

/// Trait for converting protobuf messages to JSON for debugging.
///
/// This trait is automatically implemented for generated message types
/// when the `playground` feature is enabled in code generation. It
/// provides a consistent way to serialize messages for the dashboard.
///
/// # Example
///
/// ```ignore
/// // Generated code (when playground feature is enabled):
/// impl ToDebugJson for HelloRequest {
///     fn to_debug_json(&self) -> serde_json::Value {
///         serde_json::json!({
///             "name": self.name
///         })
///     }
/// }
///
/// // Usage in playground interceptor:
/// let request: HelloRequest = ...;
/// let json = request.to_debug_json();
/// controller.emit_event(PlaygroundEvent::rpc_send(metadata, Some(json), size));
/// ```
pub trait ToDebugJson {
    /// Convert this message to a JSON value for debugging.
    ///
    /// The implementation should include all fields that are safe
    /// to display in the dashboard. Sensitive fields (passwords,
    /// tokens, etc.) should be redacted.
    fn to_debug_json(&self) -> Value;

    /// Convert to a JSON value, redacting sensitive fields.
    ///
    /// Default implementation calls `to_debug_json()`. Override this
    /// if your message contains fields that should be masked.
    fn to_debug_json_redacted(&self) -> Value {
        self.to_debug_json()
    }

    /// Get the message type name for display.
    ///
    /// Default implementation returns `None`. Override this to provide
    /// the protobuf message type name (e.g., "my.package.HelloRequest").
    fn debug_type_name(&self) -> Option<&'static str> {
        None
    }
}

/// Blanket implementation for types that implement serde::Serialize.
///
/// This allows any serializable type to be used with ToDebugJson
/// without explicit implementation.
impl<T: serde::Serialize> ToDebugJson for T {
    fn to_debug_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Helper to redact sensitive fields from a JSON value.
///
/// Replaces values of fields matching sensitive patterns with "[REDACTED]".
pub fn redact_sensitive_fields(mut value: Value, patterns: &[&str]) -> Value {
    if let Value::Object(ref mut map) = value {
        for (key, val) in map.iter_mut() {
            let key_lower = key.to_lowercase();
            let should_redact = patterns
                .iter()
                .any(|p| key_lower.contains(&p.to_lowercase()));

            if should_redact {
                *val = Value::String("[REDACTED]".to_string());
            } else if val.is_object() || val.is_array() {
                *val = redact_sensitive_fields(val.take(), patterns);
            }
        }
    } else if let Value::Array(ref mut arr) = value {
        for item in arr.iter_mut() {
            *item = redact_sensitive_fields(item.take(), patterns);
        }
    }
    value
}

/// Default sensitive field patterns for redaction.
pub const DEFAULT_SENSITIVE_PATTERNS: &[&str] = &[
    "password",
    "secret",
    "token",
    "api_key",
    "apikey",
    "auth",
    "credential",
    "private",
    "ssn",
    "credit_card",
    "creditcard",
];

/// Wrapper for message body in events that handles redaction.
#[derive(Debug, Clone)]
pub struct DebugBody {
    /// The JSON value (potentially redacted)
    pub value: Value,
    /// Whether redaction was applied
    pub redacted: bool,
    /// Original message type name
    pub type_name: Option<String>,
}

impl DebugBody {
    /// Create a new debug body from a ToDebugJson implementation.
    pub fn from_message<T: ToDebugJson>(msg: &T, redact: bool) -> Self {
        let value = if redact {
            msg.to_debug_json_redacted()
        } else {
            msg.to_debug_json()
        };

        Self {
            value,
            redacted: redact,
            type_name: msg.debug_type_name().map(|s| s.to_string()),
        }
    }

    /// Create from a JSON value with optional redaction.
    pub fn from_value(value: Value, redact: bool) -> Self {
        let value = if redact {
            redact_sensitive_fields(value, DEFAULT_SENSITIVE_PATTERNS)
        } else {
            value
        };

        Self {
            value,
            redacted: redact,
            type_name: None,
        }
    }

    /// Get the JSON value.
    pub fn into_value(self) -> Value {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_redact_sensitive_fields() {
        let value = json!({
            "username": "alice",
            "password": "secret123",
            "data": {
                "api_key": "key-abc",
                "count": 42
            }
        });

        let redacted = redact_sensitive_fields(value, DEFAULT_SENSITIVE_PATTERNS);

        assert_eq!(redacted["username"], "alice");
        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["data"]["api_key"], "[REDACTED]");
        assert_eq!(redacted["data"]["count"], 42);
    }

    #[test]
    fn test_redact_array_fields() {
        let value = json!({
            "users": [
                {"name": "alice", "token": "tok-1"},
                {"name": "bob", "token": "tok-2"}
            ]
        });

        let redacted = redact_sensitive_fields(value, DEFAULT_SENSITIVE_PATTERNS);

        assert_eq!(redacted["users"][0]["name"], "alice");
        assert_eq!(redacted["users"][0]["token"], "[REDACTED]");
        assert_eq!(redacted["users"][1]["name"], "bob");
        assert_eq!(redacted["users"][1]["token"], "[REDACTED]");
    }

    #[test]
    fn test_redact_case_insensitive() {
        let value = json!({
            "PASSWORD": "secret",
            "ApiKey": "key",
            "AUTH_TOKEN": "token"
        });

        let redacted = redact_sensitive_fields(value, DEFAULT_SENSITIVE_PATTERNS);

        assert_eq!(redacted["PASSWORD"], "[REDACTED]");
        assert_eq!(redacted["ApiKey"], "[REDACTED]");
        assert_eq!(redacted["AUTH_TOKEN"], "[REDACTED]");
    }

    #[test]
    fn test_debug_body_from_value() {
        let value = json!({
            "name": "test",
            "secret": "hidden"
        });

        let body = DebugBody::from_value(value.clone(), false);
        assert_eq!(body.value["secret"], "hidden");
        assert!(!body.redacted);

        let body_redacted = DebugBody::from_value(value, true);
        assert_eq!(body_redacted.value["secret"], "[REDACTED]");
        assert!(body_redacted.redacted);
    }

    #[test]
    fn test_to_debug_json_blanket_impl() {
        #[derive(serde::Serialize)]
        struct TestMessage {
            name: String,
            count: i32,
        }

        let msg = TestMessage {
            name: "test".into(),
            count: 42,
        };

        let json = msg.to_debug_json();
        assert_eq!(json["name"], "test");
        assert_eq!(json["count"], 42);
    }

    #[test]
    fn test_nested_redaction() {
        let value = json!({
            "level1": {
                "level2": {
                    "level3": {
                        "password": "deep-secret"
                    }
                }
            }
        });

        let redacted = redact_sensitive_fields(value, DEFAULT_SENSITIVE_PATTERNS);
        assert_eq!(
            redacted["level1"]["level2"]["level3"]["password"],
            "[REDACTED]"
        );
    }
}
