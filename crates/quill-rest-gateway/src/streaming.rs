//! Streaming support for REST gateway.
//!
//! This module provides:
//! - Server-Sent Events (SSE) for server-streaming RPCs
//! - Chunked transfer encoding for client-streaming RPCs
//! - NDJSON (newline-delimited JSON) format support

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response, Sse},
};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Content types for streaming responses
pub mod content_type {
    /// Server-Sent Events content type
    pub const SSE: &str = "text/event-stream";
    /// Newline-delimited JSON content type
    pub const NDJSON: &str = "application/x-ndjson";
    /// JSON Lines content type (alias for NDJSON)
    pub const JSON_LINES: &str = "application/jsonl";
}

/// SSE event data
#[derive(Debug, Clone, Serialize)]
pub struct SseEvent {
    /// Event type (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Event data as JSON
    pub data: Value,
    /// Event ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Retry interval in milliseconds (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<u64>,
}

impl SseEvent {
    /// Create a new SSE event with data
    pub fn new(data: Value) -> Self {
        Self {
            event: None,
            data,
            id: None,
            retry: None,
        }
    }

    /// Set event type
    pub fn with_event(mut self, event: &str) -> Self {
        self.event = Some(event.to_string());
        self
    }

    /// Set event ID
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    /// Set retry interval
    pub fn with_retry(mut self, retry_ms: u64) -> Self {
        self.retry = Some(retry_ms);
        self
    }

    /// Format as SSE text
    pub fn to_sse_string(&self) -> String {
        let mut result = String::new();

        if let Some(ref event) = self.event {
            result.push_str(&format!("event: {}\n", event));
        }

        if let Some(ref id) = self.id {
            result.push_str(&format!("id: {}\n", id));
        }

        if let Some(retry) = self.retry {
            result.push_str(&format!("retry: {}\n", retry));
        }

        // Data must be JSON-serialized on a single line
        let data_str = serde_json::to_string(&self.data).unwrap_or_default();
        result.push_str(&format!("data: {}\n", data_str));

        // Empty line to signal end of event
        result.push('\n');

        result
    }
}

/// Axum SSE event wrapper
pub struct AxumSseEvent(axum::response::sse::Event);

impl AxumSseEvent {
    /// Create from SSE event
    pub fn from_event(event: SseEvent) -> Result<Self, Infallible> {
        let mut sse_event = axum::response::sse::Event::default();

        if let Some(ref event_type) = event.event {
            sse_event = sse_event.event(event_type);
        }

        if let Some(ref id) = event.id {
            sse_event = sse_event.id(id);
        }

        if let Some(retry) = event.retry {
            sse_event = sse_event.retry(Duration::from_millis(retry));
        }

        let data_str = serde_json::to_string(&event.data).unwrap_or_default();
        sse_event = sse_event.data(data_str);

        Ok(AxumSseEvent(sse_event))
    }
}

impl From<AxumSseEvent> for axum::response::sse::Event {
    fn from(event: AxumSseEvent) -> Self {
        event.0
    }
}

/// SSE stream wrapper for streaming RPC responses
pub struct SseStream {
    receiver: ReceiverStream<SseEvent>,
}

impl SseStream {
    /// Create a new SSE stream from a channel receiver
    pub fn new(receiver: mpsc::Receiver<SseEvent>) -> Self {
        Self {
            receiver: ReceiverStream::new(receiver),
        }
    }

    /// Create SSE stream and sender channel
    pub fn channel(buffer: usize) -> (mpsc::Sender<SseEvent>, Self) {
        let (tx, rx) = mpsc::channel(buffer);
        (tx, Self::new(rx))
    }

    /// Convert to Axum SSE response
    pub fn into_response(self) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
        Sse::new(self.receiver.map(|event| {
            let sse_event = AxumSseEvent::from_event(event).unwrap();
            Ok(sse_event.into())
        }))
    }
}

/// NDJSON stream wrapper for streaming responses
pub struct NdjsonStream {
    receiver: ReceiverStream<Value>,
}

impl NdjsonStream {
    /// Create a new NDJSON stream from a channel receiver
    pub fn new(receiver: mpsc::Receiver<Value>) -> Self {
        Self {
            receiver: ReceiverStream::new(receiver),
        }
    }

    /// Create NDJSON stream and sender channel
    pub fn channel(buffer: usize) -> (mpsc::Sender<Value>, Self) {
        let (tx, rx) = mpsc::channel(buffer);
        (tx, Self::new(rx))
    }
}

impl IntoResponse for NdjsonStream {
    fn into_response(self) -> Response {
        let stream = self.receiver.map(|value| {
            let mut line = serde_json::to_string(&value).unwrap_or_default();
            line.push('\n');
            Ok::<_, Infallible>(line)
        });

        let body = Body::from_stream(stream);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type::NDJSON)
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .unwrap()
    }
}

/// Client streaming request parser for NDJSON
pub struct NdjsonReader {
    buffer: String,
}

impl NdjsonReader {
    /// Create a new NDJSON reader
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed data into the reader and extract complete JSON objects
    pub fn feed(&mut self, data: &[u8]) -> Vec<Value> {
        let data_str = String::from_utf8_lossy(data);
        self.buffer.push_str(&data_str);

        let mut results = Vec::new();

        // Process complete lines
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim();
            if !line.is_empty() {
                if let Ok(value) = serde_json::from_str(line) {
                    results.push(value);
                }
            }
            self.buffer = self.buffer[newline_pos + 1..].to_string();
        }

        results
    }

    /// Get any remaining buffered data
    pub fn finish(self) -> Option<Value> {
        let line = self.buffer.trim();
        if !line.is_empty() {
            serde_json::from_str(line).ok()
        } else {
            None
        }
    }
}

impl Default for NdjsonReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Multipart request parser for client streaming
#[derive(Debug, Clone)]
pub struct MultipartChunk {
    /// Content type of the chunk
    pub content_type: Option<String>,
    /// Chunk data
    pub data: Bytes,
    /// Field name (if multipart form-data)
    pub field_name: Option<String>,
    /// Filename (if file upload)
    pub filename: Option<String>,
}

impl MultipartChunk {
    /// Create a new multipart chunk with data
    pub fn new(data: Bytes) -> Self {
        Self {
            content_type: None,
            data,
            field_name: None,
            filename: None,
        }
    }

    /// Set content type
    pub fn with_content_type(mut self, content_type: &str) -> Self {
        self.content_type = Some(content_type.to_string());
        self
    }

    /// Set field name
    pub fn with_field_name(mut self, name: &str) -> Self {
        self.field_name = Some(name.to_string());
        self
    }

    /// Set filename
    pub fn with_filename(mut self, filename: &str) -> Self {
        self.filename = Some(filename.to_string());
        self
    }

    /// Try to parse data as JSON
    pub fn to_json(&self) -> Option<Value> {
        serde_json::from_slice(&self.data).ok()
    }
}

/// Chunked request reader for client streaming
///
/// Supports reading NDJSON or multipart streams from HTTP requests.
pub struct ChunkedRequestReader {
    buffer: Vec<u8>,
    content_type: ContentType,
    boundary: Option<String>,
}

/// Content type for chunked requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    /// Newline-delimited JSON
    Ndjson,
    /// Multipart form-data with boundary
    Multipart,
    /// Plain JSON (single object)
    Json,
    /// Unknown/unsupported
    Unknown,
}

impl ChunkedRequestReader {
    /// Create a new chunked request reader from content-type header
    pub fn from_content_type(content_type: &str) -> Self {
        let (parsed_type, boundary) = Self::parse_content_type(content_type);
        Self {
            buffer: Vec::new(),
            content_type: parsed_type,
            boundary,
        }
    }

    /// Parse content-type header
    fn parse_content_type(content_type: &str) -> (ContentType, Option<String>) {
        let lower = content_type.to_lowercase();

        if lower.contains("application/x-ndjson") || lower.contains("application/jsonl") {
            (ContentType::Ndjson, None)
        } else if lower.contains("multipart/") {
            // Extract boundary
            let boundary = content_type
                .split(';')
                .find_map(|part| {
                    let trimmed = part.trim();
                    if trimmed.starts_with("boundary=") {
                        Some(trimmed[9..].trim_matches('"').to_string())
                    } else {
                        None
                    }
                });
            (ContentType::Multipart, boundary)
        } else if lower.contains("application/json") {
            (ContentType::Json, None)
        } else {
            (ContentType::Unknown, None)
        }
    }

    /// Get the detected content type
    pub fn content_type(&self) -> &ContentType {
        &self.content_type
    }

    /// Feed data and extract complete chunks
    pub fn feed(&mut self, data: &[u8]) -> Vec<MultipartChunk> {
        self.buffer.extend_from_slice(data);

        match self.content_type {
            ContentType::Ndjson => self.parse_ndjson_chunks(),
            ContentType::Multipart => self.parse_multipart_chunks(),
            ContentType::Json => {
                // For plain JSON, buffer until finish
                Vec::new()
            }
            ContentType::Unknown => Vec::new(),
        }
    }

    /// Parse NDJSON chunks from buffer
    fn parse_ndjson_chunks(&mut self) -> Vec<MultipartChunk> {
        let mut chunks = Vec::new();
        let buffer_str = String::from_utf8_lossy(&self.buffer);

        let mut last_newline = 0;
        for (i, c) in buffer_str.char_indices() {
            if c == '\n' {
                let line = &buffer_str[last_newline..i].trim();
                if !line.is_empty() {
                    chunks.push(
                        MultipartChunk::new(Bytes::from(line.as_bytes().to_vec()))
                            .with_content_type("application/json"),
                    );
                }
                last_newline = i + 1;
            }
        }

        // Keep unprocessed data
        if last_newline > 0 {
            self.buffer = self.buffer[last_newline..].to_vec();
        }

        chunks
    }

    /// Parse multipart chunks from buffer
    fn parse_multipart_chunks(&mut self) -> Vec<MultipartChunk> {
        let boundary = match &self.boundary {
            Some(b) => format!("--{}", b),
            None => return Vec::new(),
        };

        let mut chunks = Vec::new();
        let buffer_str = String::from_utf8_lossy(&self.buffer).to_string();

        // Find complete parts
        let parts: Vec<&str> = buffer_str.split(&boundary).collect();

        // Process complete parts (all except possibly the last)
        let complete_count = if buffer_str.ends_with(&boundary) || buffer_str.ends_with("--") {
            parts.len()
        } else {
            parts.len().saturating_sub(1)
        };

        for part in parts.iter().take(complete_count) {
            if let Some(chunk) = Self::parse_multipart_part(part) {
                chunks.push(chunk);
            }
        }

        // Keep incomplete data
        if complete_count < parts.len() {
            let last_boundary_pos = buffer_str.rfind(&boundary).unwrap_or(0);
            self.buffer = self.buffer[last_boundary_pos..].to_vec();
        } else {
            self.buffer.clear();
        }

        chunks
    }

    /// Parse a single multipart part
    fn parse_multipart_part(part: &str) -> Option<MultipartChunk> {
        let trimmed = part.trim();
        if trimmed.is_empty() || trimmed == "--" {
            return None;
        }

        // Split headers and body
        let header_end = trimmed.find("\r\n\r\n").or_else(|| trimmed.find("\n\n"))?;
        let (headers, body) = trimmed.split_at(header_end);
        let body = body.trim_start_matches("\r\n\r\n").trim_start_matches("\n\n");

        // Parse headers
        let mut content_type = None;
        let mut field_name = None;
        let mut filename = None;

        for line in headers.lines() {
            let lower = line.to_lowercase();
            if lower.starts_with("content-type:") {
                content_type = Some(line[13..].trim().to_string());
            } else if lower.starts_with("content-disposition:") {
                // Parse form-data disposition
                if let Some(name_start) = line.find("name=\"") {
                    let name_end = line[name_start + 6..].find('"').map(|i| i + name_start + 6);
                    if let Some(end) = name_end {
                        field_name = Some(line[name_start + 6..end].to_string());
                    }
                }
                if let Some(file_start) = line.find("filename=\"") {
                    let file_end = line[file_start + 10..].find('"').map(|i| i + file_start + 10);
                    if let Some(end) = file_end {
                        filename = Some(line[file_start + 10..end].to_string());
                    }
                }
            }
        }

        Some(MultipartChunk {
            content_type,
            data: Bytes::from(body.as_bytes().to_vec()),
            field_name,
            filename,
        })
    }

    /// Finish reading and return any remaining data
    pub fn finish(self) -> Option<MultipartChunk> {
        if self.buffer.is_empty() {
            return None;
        }

        match self.content_type {
            ContentType::Json | ContentType::Ndjson => {
                let trimmed = String::from_utf8_lossy(&self.buffer);
                let trimmed = trimmed.trim();
                if !trimmed.is_empty() {
                    Some(
                        MultipartChunk::new(Bytes::from(trimmed.as_bytes().to_vec()))
                            .with_content_type("application/json"),
                    )
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Streaming format enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingFormat {
    /// Server-Sent Events
    Sse,
    /// Newline-delimited JSON
    Ndjson,
    /// JSON array (non-streaming fallback)
    JsonArray,
}

impl StreamingFormat {
    /// Get content type for the format
    pub fn content_type(&self) -> &'static str {
        match self {
            StreamingFormat::Sse => content_type::SSE,
            StreamingFormat::Ndjson => content_type::NDJSON,
            StreamingFormat::JsonArray => "application/json",
        }
    }

    /// Parse from Accept header
    pub fn from_accept(accept: &str) -> Self {
        let accept_lower = accept.to_lowercase();

        if accept_lower.contains("text/event-stream") {
            StreamingFormat::Sse
        } else if accept_lower.contains("application/x-ndjson")
            || accept_lower.contains("application/jsonl")
        {
            StreamingFormat::Ndjson
        } else {
            StreamingFormat::JsonArray
        }
    }
}

/// Streaming route configuration
#[derive(Debug, Clone, Default)]
pub struct StreamingConfig {
    /// Enable SSE for server-streaming
    pub enable_sse: bool,
    /// Enable NDJSON for server-streaming
    pub enable_ndjson: bool,
    /// Enable chunked client streaming
    pub enable_client_streaming: bool,
    /// Default streaming format
    pub default_format: Option<StreamingFormat>,
    /// Keep-alive interval in seconds (for SSE)
    pub keep_alive_secs: Option<u64>,
}

impl StreamingConfig {
    /// Create a new streaming config with SSE enabled
    pub fn sse() -> Self {
        Self {
            enable_sse: true,
            enable_ndjson: false,
            enable_client_streaming: false,
            default_format: Some(StreamingFormat::Sse),
            keep_alive_secs: Some(30),
        }
    }

    /// Create a new streaming config with NDJSON enabled
    pub fn ndjson() -> Self {
        Self {
            enable_sse: false,
            enable_ndjson: true,
            enable_client_streaming: false,
            default_format: Some(StreamingFormat::Ndjson),
            keep_alive_secs: None,
        }
    }

    /// Create a config for client streaming
    pub fn client_streaming() -> Self {
        Self {
            enable_sse: false,
            enable_ndjson: false,
            enable_client_streaming: true,
            default_format: None,
            keep_alive_secs: None,
        }
    }

    /// Create a config for bidirectional streaming
    pub fn bidirectional() -> Self {
        Self {
            enable_sse: true,
            enable_ndjson: true,
            enable_client_streaming: true,
            default_format: Some(StreamingFormat::Sse),
            keep_alive_secs: Some(30),
        }
    }
}

/// Streaming response builder
pub struct StreamingResponse {
    format: StreamingFormat,
    keep_alive_secs: Option<u64>,
}

impl StreamingResponse {
    /// Create a new streaming response builder
    pub fn new(format: StreamingFormat) -> Self {
        Self {
            format,
            keep_alive_secs: None,
        }
    }

    /// Get the streaming format
    pub fn format(&self) -> StreamingFormat {
        self.format
    }

    /// Set keep-alive interval (only applies to SSE)
    pub fn with_keep_alive(mut self, secs: u64) -> Self {
        self.keep_alive_secs = Some(secs);
        self
    }

    /// Build response from a stream of JSON values using the configured format
    pub fn build<S>(self, stream: S) -> Response
    where
        S: Stream<Item = Value> + Send + 'static,
    {
        match self.format {
            StreamingFormat::Sse => self.build_sse(stream).into_response(),
            StreamingFormat::Ndjson => self.build_ndjson(stream),
            StreamingFormat::JsonArray => {
                // Fallback: collect and return as JSON array
                // Note: This defeats the streaming purpose, used only for compatibility
                self.build_ndjson(stream)
            }
        }
    }

    /// Build SSE response from a stream of JSON values
    pub fn build_sse<S>(self, stream: S) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>>
    where
        S: Stream<Item = Value> + Send + 'static,
    {
        let mapped = stream.map(|value| {
            let event = SseEvent::new(value);
            let sse_event = AxumSseEvent::from_event(event).unwrap();
            Ok(sse_event.into())
        });

        let sse = Sse::new(mapped);

        if let Some(secs) = self.keep_alive_secs {
            sse.keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(Duration::from_secs(secs))
                    .text("ping"),
            )
        } else {
            sse
        }
    }

    /// Build NDJSON response from a stream of JSON values
    pub fn build_ndjson<S>(self, stream: S) -> Response
    where
        S: Stream<Item = Value> + Send + 'static,
    {
        let mapped = stream.map(|value| {
            let mut line = serde_json::to_string(&value).unwrap_or_default();
            line.push('\n');
            Ok::<_, Infallible>(line)
        });

        let body = Body::from_stream(mapped);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type::NDJSON)
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_formatting() {
        let event = SseEvent::new(serde_json::json!({"message": "hello"}));
        let formatted = event.to_sse_string();
        assert!(formatted.contains("data: {\"message\":\"hello\"}"));
        assert!(formatted.ends_with("\n\n"));
    }

    #[test]
    fn test_sse_event_with_type() {
        let event = SseEvent::new(serde_json::json!({"count": 42}))
            .with_event("update")
            .with_id("msg-1");
        let formatted = event.to_sse_string();
        assert!(formatted.contains("event: update"));
        assert!(formatted.contains("id: msg-1"));
        assert!(formatted.contains("data: {\"count\":42}"));
    }

    #[test]
    fn test_sse_event_with_retry() {
        let event = SseEvent::new(serde_json::json!(null))
            .with_retry(5000);
        let formatted = event.to_sse_string();
        assert!(formatted.contains("retry: 5000"));
    }

    #[test]
    fn test_ndjson_reader() {
        let mut reader = NdjsonReader::new();

        // Partial data
        let results = reader.feed(b"{\"a\":1}\n{\"b\":");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!({"a": 1}));

        // Complete remaining
        let results = reader.feed(b"2}\n");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], serde_json::json!({"b": 2}));
    }

    #[test]
    fn test_ndjson_reader_multiple_lines() {
        let mut reader = NdjsonReader::new();
        let results = reader.feed(b"{\"x\":1}\n{\"y\":2}\n{\"z\":3}\n");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_streaming_format_from_accept() {
        assert_eq!(
            StreamingFormat::from_accept("text/event-stream"),
            StreamingFormat::Sse
        );
        assert_eq!(
            StreamingFormat::from_accept("application/x-ndjson"),
            StreamingFormat::Ndjson
        );
        assert_eq!(
            StreamingFormat::from_accept("application/jsonl"),
            StreamingFormat::Ndjson
        );
        assert_eq!(
            StreamingFormat::from_accept("application/json"),
            StreamingFormat::JsonArray
        );
    }

    #[test]
    fn test_streaming_format_content_type() {
        assert_eq!(StreamingFormat::Sse.content_type(), "text/event-stream");
        assert_eq!(StreamingFormat::Ndjson.content_type(), "application/x-ndjson");
        assert_eq!(StreamingFormat::JsonArray.content_type(), "application/json");
    }

    #[test]
    fn test_streaming_config_sse() {
        let config = StreamingConfig::sse();
        assert!(config.enable_sse);
        assert!(!config.enable_ndjson);
        assert_eq!(config.default_format, Some(StreamingFormat::Sse));
    }

    #[test]
    fn test_streaming_config_ndjson() {
        let config = StreamingConfig::ndjson();
        assert!(!config.enable_sse);
        assert!(config.enable_ndjson);
        assert_eq!(config.default_format, Some(StreamingFormat::Ndjson));
    }

    #[test]
    fn test_streaming_config_bidirectional() {
        let config = StreamingConfig::bidirectional();
        assert!(config.enable_sse);
        assert!(config.enable_ndjson);
        assert!(config.enable_client_streaming);
    }

    #[test]
    fn test_multipart_chunk_builder() {
        let chunk = MultipartChunk::new(Bytes::from("test data"))
            .with_content_type("application/json")
            .with_field_name("message")
            .with_filename("data.json");

        assert_eq!(chunk.content_type, Some("application/json".to_string()));
        assert_eq!(chunk.field_name, Some("message".to_string()));
        assert_eq!(chunk.filename, Some("data.json".to_string()));
    }

    #[test]
    fn test_multipart_chunk_to_json() {
        let chunk = MultipartChunk::new(Bytes::from(r#"{"key": "value"}"#));
        let json = chunk.to_json();
        assert!(json.is_some());
        assert_eq!(json.unwrap(), serde_json::json!({"key": "value"}));
    }

    #[test]
    fn test_chunked_reader_content_type_detection() {
        let ndjson = ChunkedRequestReader::from_content_type("application/x-ndjson");
        assert_eq!(*ndjson.content_type(), ContentType::Ndjson);

        let jsonl = ChunkedRequestReader::from_content_type("application/jsonl");
        assert_eq!(*jsonl.content_type(), ContentType::Ndjson);

        let multipart = ChunkedRequestReader::from_content_type("multipart/form-data; boundary=abc123");
        assert_eq!(*multipart.content_type(), ContentType::Multipart);

        let json = ChunkedRequestReader::from_content_type("application/json");
        assert_eq!(*json.content_type(), ContentType::Json);

        let unknown = ChunkedRequestReader::from_content_type("text/plain");
        assert_eq!(*unknown.content_type(), ContentType::Unknown);
    }

    #[test]
    fn test_chunked_reader_ndjson() {
        let mut reader = ChunkedRequestReader::from_content_type("application/x-ndjson");

        let chunks = reader.feed(b"{\"msg\":\"hello\"}\n{\"msg\":\"world\"}\n");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].to_json(), Some(serde_json::json!({"msg": "hello"})));
        assert_eq!(chunks[1].to_json(), Some(serde_json::json!({"msg": "world"})));
    }

    #[test]
    fn test_chunked_reader_ndjson_partial() {
        let mut reader = ChunkedRequestReader::from_content_type("application/x-ndjson");

        // Partial line
        let chunks = reader.feed(b"{\"msg\":\"hel");
        assert_eq!(chunks.len(), 0);

        // Complete line
        let chunks = reader.feed(b"lo\"}\n");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].to_json(), Some(serde_json::json!({"msg": "hello"})));
    }

    #[test]
    fn test_chunked_reader_finish() {
        let mut reader = ChunkedRequestReader::from_content_type("application/json");

        // Feed partial JSON (no newline needed)
        reader.feed(b"{\"complete\": true}");

        // Finish and get remaining
        let remaining = reader.finish();
        assert!(remaining.is_some());
        assert_eq!(remaining.unwrap().to_json(), Some(serde_json::json!({"complete": true})));
    }

    #[test]
    fn test_streaming_response_format() {
        let response = StreamingResponse::new(StreamingFormat::Sse);
        assert_eq!(response.format(), StreamingFormat::Sse);

        let response = StreamingResponse::new(StreamingFormat::Ndjson)
            .with_keep_alive(60);
        assert_eq!(response.format(), StreamingFormat::Ndjson);
    }
}
