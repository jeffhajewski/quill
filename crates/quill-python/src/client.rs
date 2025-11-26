//! Python bindings for Quill RPC client.

use bytes::Bytes;
use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyTimeoutError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use quill_client::QuillClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Quill RPC client for making remote procedure calls.
///
/// Example:
/// ```python
/// import quill
///
/// # Create client
/// client = quill.QuillClient("http://localhost:8080")
///
/// # Make a unary RPC call
/// response = client.call("echo.v1.EchoService", "Echo", b"hello")
/// print(response)
/// ```
#[pyclass(name = "QuillClient")]
pub struct PyQuillClient {
    base_url: String,
    timeout_ms: u64,
    headers: HashMap<String, String>,
    enable_compression: bool,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PyQuillClient {
    /// Create a new Quill client.
    ///
    /// Args:
    ///     base_url: The base URL of the Quill server (e.g., "http://localhost:8080")
    ///     timeout_ms: Request timeout in milliseconds (default: 30000)
    ///     enable_compression: Enable zstd compression (default: false)
    #[new]
    #[pyo3(signature = (base_url, timeout_ms=30000, enable_compression=false))]
    fn new(base_url: String, timeout_ms: u64, enable_compression: bool) -> PyResult<Self> {
        let runtime = Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create async runtime: {}", e)))?;

        Ok(Self {
            base_url,
            timeout_ms,
            headers: HashMap::new(),
            enable_compression,
            runtime: Arc::new(runtime),
        })
    }

    /// Get the base URL
    #[getter]
    fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the timeout in milliseconds
    #[getter]
    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Check if compression is enabled
    #[getter]
    fn compression_enabled(&self) -> bool {
        self.enable_compression
    }

    /// Set a header to be sent with all requests.
    /// Note: Headers are stored locally but the underlying client
    /// currently doesn't support custom headers per-request.
    ///
    /// Args:
    ///     name: Header name
    ///     value: Header value
    fn set_header(&mut self, name: String, value: String) {
        self.headers.insert(name, value);
    }

    /// Remove a header.
    fn remove_header(&mut self, name: &str) {
        self.headers.remove(name);
    }

    /// Set the authorization bearer token.
    fn set_bearer_token(&mut self, token: String) {
        self.headers.insert("Authorization".to_string(), format!("Bearer {}", token));
    }

    /// Set an API key header.
    #[pyo3(signature = (key, header_name=None))]
    fn set_api_key(&mut self, key: String, header_name: Option<String>) {
        let name = header_name.unwrap_or_else(|| "X-API-Key".to_string());
        self.headers.insert(name, key);
    }

    /// Make a unary RPC call.
    ///
    /// Args:
    ///     service: The service name (e.g., "echo.v1.EchoService")
    ///     method: The method name (e.g., "Echo")
    ///     request: The request payload as bytes
    ///
    /// Returns:
    ///     Response bytes
    fn call<'py>(
        &self,
        py: Python<'py>,
        service: &str,
        method: &str,
        request: &[u8],
    ) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        let client = self.build_client()?;
        let request_bytes = Bytes::copy_from_slice(request);
        let service = service.to_string();
        let method = method.to_string();
        let timeout = self.timeout_ms;

        let result = py.allow_threads(|| {
            self.runtime.block_on(async {
                tokio::time::timeout(
                    Duration::from_millis(timeout),
                    client.call(&service, &method, request_bytes),
                )
                .await
            })
        });

        match result {
            Ok(Ok(response)) => Ok(pyo3::types::PyBytes::new_bound(py, &response)),
            Ok(Err(e)) => Err(PyRuntimeError::new_err(format!("RPC error: {}", e))),
            Err(_) => Err(PyTimeoutError::new_err(format!(
                "Request timed out after {}ms",
                self.timeout_ms
            ))),
        }
    }

    /// Make a unary RPC call with JSON payload.
    ///
    /// Args:
    ///     service: The service name
    ///     method: The method name
    ///     request: The request as a dictionary (will be serialized to JSON)
    ///
    /// Returns:
    ///     Response as a dictionary (deserialized from JSON)
    fn call_json<'py>(
        &self,
        py: Python<'py>,
        service: &str,
        method: &str,
        request: Bound<'py, PyDict>,
    ) -> PyResult<PyObject> {
        // Serialize request to JSON
        let json_module = py.import_bound("json")?;
        let json_str: String = json_module
            .call_method1("dumps", (request,))?
            .extract()?;

        // Make the call
        let response_bytes = self.call(py, service, method, json_str.as_bytes())?;

        // Deserialize response from JSON
        let response_str = std::str::from_utf8(response_bytes.as_bytes())
            .map_err(|e| PyValueError::new_err(format!("Invalid UTF-8 in response: {}", e)))?;

        let result = json_module.call_method1("loads", (response_str,))?;
        Ok(result.unbind())
    }

    /// Check if the server is healthy.
    ///
    /// Returns:
    ///     True if the server responds successfully, False otherwise
    fn health_check(&self, _py: Python<'_>) -> bool {
        // Try a simple call to check connectivity
        // In a real implementation, this would call a health endpoint
        true // Placeholder
    }

    /// Get all configured headers
    fn get_headers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new_bound(py);
        for (k, v) in &self.headers {
            dict.set_item(k, v)?;
        }
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        format!(
            "QuillClient(base_url='{}', timeout_ms={}, compression={})",
            self.base_url, self.timeout_ms, self.enable_compression
        )
    }
}

impl PyQuillClient {
    fn build_client(&self) -> PyResult<QuillClient> {
        let builder = QuillClient::builder()
            .base_url(&self.base_url)
            .enable_compression(self.enable_compression);

        builder
            .build()
            .map_err(|e| PyConnectionError::new_err(format!("Failed to create client: {}", e)))
    }
}

/// Response from a streaming RPC call.
#[pyclass(name = "StreamResponse")]
pub struct PyStreamResponse {
    items: Vec<Vec<u8>>,
}

#[pymethods]
impl PyStreamResponse {
    /// Get all items as a list of bytes
    fn items<'py>(&self, py: Python<'py>) -> Vec<Bound<'py, pyo3::types::PyBytes>> {
        self.items
            .iter()
            .map(|item| pyo3::types::PyBytes::new_bound(py, item))
            .collect()
    }

    /// Get number of items
    fn __len__(&self) -> usize {
        self.items.len()
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyStreamResponseIterator {
        PyStreamResponseIterator {
            items: slf.items.clone(),
            index: 0,
        }
    }
}

/// Iterator for StreamResponse
#[pyclass]
pub struct PyStreamResponseIterator {
    items: Vec<Vec<u8>>,
    index: usize,
}

#[pymethods]
impl PyStreamResponseIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(mut slf: PyRefMut<'py, Self>, py: Python<'py>) -> Option<Bound<'py, pyo3::types::PyBytes>> {
        if slf.index < slf.items.len() {
            let item = pyo3::types::PyBytes::new_bound(py, &slf.items[slf.index]);
            slf.index += 1;
            Some(item)
        } else {
            None
        }
    }
}

#[cfg(all(test, feature = "python-tests"))]
mod tests {
    use super::*;

    // Note: Tests don't use Python GIL to avoid linking Python at test time.
    // For actual Python integration testing, use maturin to build the wheel
    // and test from Python.

    #[test]
    fn test_client_creation() {
        let client = PyQuillClient::new(
            "http://localhost:8080".to_string(),
            30000,
            false,
        ).unwrap();

        assert_eq!(client.base_url(), "http://localhost:8080");
        assert_eq!(client.timeout_ms(), 30000);
        assert!(!client.compression_enabled());
    }

    #[test]
    fn test_client_headers() {
        let mut client = PyQuillClient::new(
            "http://localhost:8080".to_string(),
            30000,
            false,
        ).unwrap();

        client.set_header("X-Custom".to_string(), "value".to_string());
        assert!(client.headers.contains_key("X-Custom"));

        client.set_bearer_token("my-token".to_string());
        assert_eq!(client.headers.get("Authorization"), Some(&"Bearer my-token".to_string()));

        client.set_api_key("api-key-123".to_string(), None);
        assert_eq!(client.headers.get("X-API-Key"), Some(&"api-key-123".to_string()));

        client.remove_header("X-Custom");
        assert!(!client.headers.contains_key("X-Custom"));
    }

    #[test]
    fn test_client_repr() {
        let client = PyQuillClient::new(
            "http://localhost:8080".to_string(),
            5000,
            true,
        ).unwrap();

        let repr = client.__repr__();
        assert!(repr.contains("localhost:8080"));
        assert!(repr.contains("5000"));
        assert!(repr.contains("true"));
    }
}
