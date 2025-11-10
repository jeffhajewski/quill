//! Error types and Problem Details implementation.

use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Quill error type
#[derive(Debug, thiserror::Error)]
pub enum QuillError {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Framing error: {0}")]
    Framing(String),

    #[error("Problem details: {0:?}")]
    ProblemDetails(ProblemDetails),
}

/// Problem Details per RFC 7807
/// Used for structured error responses in Quill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProblemDetails {
    /// URI reference identifying the problem type
    #[serde(rename = "type")]
    pub type_uri: String,

    /// Short, human-readable summary
    pub title: String,

    /// HTTP status code
    pub status: u16,

    /// Human-readable explanation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    /// URI reference identifying the specific occurrence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,

    /// Quill-specific: protobuf type name for typed errors
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quill_proto_type: Option<String>,

    /// Quill-specific: base64-encoded protobuf bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quill_proto_detail_base64: Option<String>,
}

impl ProblemDetails {
    /// Create a new Problem Details with the given status and title
    pub fn new(status: StatusCode, title: impl Into<String>) -> Self {
        Self {
            type_uri: format!("urn:quill:error:{}", status.as_u16()),
            title: title.into(),
            status: status.as_u16(),
            detail: None,
            instance: None,
            quill_proto_type: None,
            quill_proto_detail_base64: None,
        }
    }

    /// Set the detail field
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl fmt::Display for ProblemDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.status, self.title)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {}", detail)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_problem_details_json() {
        let pd = ProblemDetails::new(StatusCode::NOT_FOUND, "Resource not found")
            .with_detail("The requested image does not exist");

        let json = pd.to_json().unwrap();
        assert!(json.contains("\"status\":404"));
        assert!(json.contains("\"title\":\"Resource not found\""));
    }
}
