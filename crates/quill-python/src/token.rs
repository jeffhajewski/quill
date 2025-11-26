//! Python bindings for Token and TokenBatch types for LLM inference.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use quill_tensor::token::{Token, TokenBatch};
use std::collections::HashMap;

/// A single token from LLM generation.
///
/// Contains the token ID, optional text, log probability, position, and special flag.
#[pyclass(name = "Token")]
#[derive(Clone, Debug)]
pub struct PyToken {
    inner: Token,
}

#[pymethods]
impl PyToken {
    /// Create a new token.
    ///
    /// Args:
    ///     id: The token ID from the vocabulary
    ///     position: Position in the sequence (0-indexed)
    ///     text: Optional decoded text for this token
    ///     logprob: Optional log probability of this token
    ///     is_special: Whether this is a special token (BOS, EOS, etc.)
    #[new]
    #[pyo3(signature = (id, position, text=None, logprob=None, is_special=false))]
    fn new(id: u32, position: u32, text: Option<String>, logprob: Option<f32>, is_special: bool) -> Self {
        let mut token = Token::new(id, position);
        token.text = text;
        token.logprob = logprob;
        token.is_special = is_special;
        Self { inner: token }
    }

    /// Create a token with text.
    #[staticmethod]
    #[pyo3(signature = (id, text, position, logprob=None))]
    fn with_text(id: u32, text: String, position: u32, logprob: Option<f32>) -> Self {
        let mut token = Token::with_text(id, text, position);
        if let Some(lp) = logprob {
            token = token.with_logprob(lp);
        }
        Self { inner: token }
    }

    /// Get token ID
    #[getter]
    fn id(&self) -> u32 {
        self.inner.id
    }

    /// Get token text (if available)
    #[getter]
    fn text(&self) -> Option<&str> {
        self.inner.text.as_deref()
    }

    /// Get log probability (if available)
    #[getter]
    fn logprob(&self) -> Option<f32> {
        self.inner.logprob
    }

    /// Get position in sequence
    #[getter]
    fn position(&self) -> u32 {
        self.inner.position
    }

    /// Check if this is a special token
    #[getter]
    fn is_special(&self) -> bool {
        self.inner.is_special
    }

    /// Check if this is an end-of-sequence token (by convention, id 0, 1, or 2)
    fn is_eos(&self) -> bool {
        self.inner.is_special && matches!(self.inner.id, 0 | 1 | 2)
    }

    /// Get probability (exp of logprob)
    fn prob(&self) -> Option<f64> {
        self.inner.logprob.map(|lp| (lp as f64).exp())
    }

    /// Convert to dictionary
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new_bound(py);
        dict.set_item("id", self.inner.id)?;
        dict.set_item("text", &self.inner.text)?;
        dict.set_item("logprob", self.inner.logprob)?;
        dict.set_item("position", self.inner.position)?;
        dict.set_item("is_special", self.inner.is_special)?;
        Ok(dict)
    }

    /// Encode token to bytes
    fn encode<'py>(&self, py: Python<'py>) -> Bound<'py, pyo3::types::PyBytes> {
        let bytes = self.inner.encode();
        pyo3::types::PyBytes::new_bound(py, &bytes)
    }

    fn __repr__(&self) -> String {
        match &self.inner.text {
            Some(text) => format!("Token(id={}, text='{}', pos={})", self.inner.id, text, self.inner.position),
            None => format!("Token(id={}, pos={})", self.inner.id, self.inner.position),
        }
    }

    fn __str__(&self) -> String {
        self.inner.text.clone().unwrap_or_else(|| format!("<{}>", self.inner.id))
    }
}

impl PyToken {
    pub fn inner(&self) -> &Token {
        &self.inner
    }

    pub fn from_inner(inner: Token) -> Self {
        Self { inner }
    }
}

/// A batch of tokens for efficient processing.
///
/// Used for batched LLM generation where multiple tokens are streamed together.
#[pyclass(name = "TokenBatch")]
#[derive(Clone, Debug)]
pub struct PyTokenBatch {
    inner: TokenBatch,
    metadata: HashMap<String, String>,
}

#[pymethods]
impl PyTokenBatch {
    /// Create a new empty token batch.
    #[new]
    #[pyo3(signature = (sequence_id=None))]
    fn new(sequence_id: Option<u32>) -> Self {
        let mut batch = TokenBatch::new();
        if let Some(id) = sequence_id {
            batch.sequence_id = Some(id);
        }
        Self {
            inner: batch,
            metadata: HashMap::new(),
        }
    }

    /// Create a batch from a list of tokens.
    #[staticmethod]
    #[pyo3(signature = (tokens, sequence_id=None))]
    fn from_tokens(tokens: Vec<PyToken>, sequence_id: Option<u32>) -> Self {
        let rust_tokens: Vec<Token> = tokens.into_iter().map(|t| t.inner.clone()).collect();
        let mut batch = TokenBatch::with_tokens(rust_tokens);
        if let Some(id) = sequence_id {
            batch.sequence_id = Some(id);
        }
        Self {
            inner: batch,
            metadata: HashMap::new(),
        }
    }

    /// Add a token to the batch.
    fn add(&mut self, token: PyToken) {
        self.inner.push(token.inner.clone());
    }

    /// Add multiple tokens to the batch.
    fn extend(&mut self, tokens: Vec<PyToken>) {
        for token in tokens {
            self.inner.push(token.inner.clone());
        }
    }

    /// Get all tokens in the batch.
    fn tokens(&self) -> Vec<PyToken> {
        self.inner.tokens.iter().map(|t| PyToken::from_inner(t.clone())).collect()
    }

    /// Get token at index.
    fn get(&self, index: usize) -> Option<PyToken> {
        self.inner.tokens.get(index).map(|t| PyToken::from_inner(t.clone()))
    }

    /// Get number of tokens in batch.
    fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Check if batch is empty.
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the sequence ID (if set)
    #[getter]
    fn sequence_id(&self) -> Option<u32> {
        self.inner.sequence_id
    }

    /// Set the sequence ID
    #[setter]
    fn set_sequence_id(&mut self, id: Option<u32>) {
        self.inner.sequence_id = id;
    }

    /// Check if this is the final batch
    #[getter]
    fn is_final(&self) -> bool {
        self.inner.is_final
    }

    /// Mark this as the final batch
    #[setter]
    fn set_is_final(&mut self, is_final: bool) {
        self.inner.is_final = is_final;
    }

    /// Concatenate text from all tokens.
    fn text(&self) -> String {
        self.inner.tokens
            .iter()
            .filter_map(|t| t.text.as_ref())
            .cloned()
            .collect()
    }

    /// Get all token IDs as a list.
    fn token_ids(&self) -> Vec<u32> {
        self.inner.tokens.iter().map(|t| t.id).collect()
    }

    /// Get all positions as a list.
    fn positions(&self) -> Vec<u32> {
        self.inner.tokens.iter().map(|t| t.position).collect()
    }

    /// Set metadata key-value pair.
    fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }

    /// Get metadata value by key.
    fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }

    /// Get all metadata as a dictionary.
    fn metadata_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new_bound(py);
        for (k, v) in &self.metadata {
            dict.set_item(k, v)?;
        }
        Ok(dict)
    }

    /// Clear all tokens from the batch.
    fn clear(&mut self) {
        self.inner.tokens.clear();
    }

    /// Encode batch to bytes
    fn encode<'py>(&self, py: Python<'py>) -> Bound<'py, pyo3::types::PyBytes> {
        let bytes = self.inner.encode();
        pyo3::types::PyBytes::new_bound(py, &bytes)
    }

    /// Convert batch to list of dictionaries.
    fn to_dicts<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        self.inner.tokens
            .iter()
            .map(|t| {
                let dict = PyDict::new_bound(py);
                dict.set_item("id", t.id)?;
                dict.set_item("text", &t.text)?;
                dict.set_item("logprob", t.logprob)?;
                dict.set_item("position", t.position)?;
                dict.set_item("is_special", t.is_special)?;
                Ok(dict)
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "TokenBatch(len={}, seq_id={:?}, is_final={})",
            self.inner.len(),
            self.inner.sequence_id,
            self.inner.is_final
        )
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyTokenBatchIterator {
        PyTokenBatchIterator {
            tokens: slf.inner.tokens.clone(),
            index: 0,
        }
    }
}

/// Iterator for TokenBatch
#[pyclass]
pub struct PyTokenBatchIterator {
    tokens: Vec<Token>,
    index: usize,
}

#[pymethods]
impl PyTokenBatchIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<PyToken> {
        if slf.index < slf.tokens.len() {
            let token = PyToken::from_inner(slf.tokens[slf.index].clone());
            slf.index += 1;
            Some(token)
        } else {
            None
        }
    }
}

#[cfg(all(test, feature = "python-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let token = PyToken::new(42, 0, Some("hello".to_string()), Some(-0.5), false);

        assert_eq!(token.id(), 42);
        assert_eq!(token.text(), Some("hello"));
        assert_eq!(token.logprob(), Some(-0.5));
        assert_eq!(token.position(), 0);
        assert!(!token.is_special());
    }

    #[test]
    fn test_token_with_text() {
        let token = PyToken::with_text(1, "world".to_string(), 5, Some(-1.0));

        assert_eq!(token.id(), 1);
        assert_eq!(token.text(), Some("world"));
        assert_eq!(token.logprob(), Some(-1.0));
        assert_eq!(token.position(), 5);
    }

    #[test]
    fn test_token_prob() {
        let token = PyToken::new(1, 0, None, Some(-1.0), false);
        let prob = token.prob().unwrap();
        assert!((prob - 0.3678794).abs() < 0.001); // e^-1 ≈ 0.368
    }

    #[test]
    fn test_token_batch() {
        let mut batch = PyTokenBatch::new(None);

        batch.add(PyToken::with_text(1, "Hello".to_string(), 0, None));
        batch.add(PyToken::with_text(2, " ".to_string(), 1, None));
        batch.add(PyToken::with_text(3, "World".to_string(), 2, None));

        assert_eq!(batch.__len__(), 3);
        assert_eq!(batch.text(), "Hello World");
        assert_eq!(batch.token_ids(), vec![1, 2, 3]);
        assert_eq!(batch.positions(), vec![0, 1, 2]);
    }

    #[test]
    fn test_token_batch_sequence() {
        let mut batch = PyTokenBatch::new(Some(42));
        batch.add(PyToken::new(1, 0, None, None, false));
        batch.set_is_final(true);

        assert_eq!(batch.sequence_id(), Some(42));
        assert!(batch.is_final());
    }

    #[test]
    fn test_token_batch_from_tokens() {
        let tokens = vec![
            PyToken::with_text(1, "a".to_string(), 0, None),
            PyToken::with_text(2, "b".to_string(), 1, None),
        ];

        let batch = PyTokenBatch::from_tokens(tokens, Some(1));
        assert_eq!(batch.__len__(), 2);
        assert_eq!(batch.text(), "ab");
        assert_eq!(batch.sequence_id(), Some(1));
    }

    #[test]
    fn test_token_batch_metadata() {
        let mut batch = PyTokenBatch::new(None);
        batch.set_metadata("model".to_string(), "llama-7b".to_string());
        batch.set_metadata("temperature".to_string(), "0.7".to_string());

        assert_eq!(batch.get_metadata("model"), Some("llama-7b"));
        assert_eq!(batch.get_metadata("temperature"), Some("0.7"));
        assert_eq!(batch.get_metadata("missing"), None);
    }
}
