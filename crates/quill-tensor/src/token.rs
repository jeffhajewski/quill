//! Token types for LLM streaming.
//!
//! Provides efficient token batch streaming for language model inference.

use bytes::{BufMut, Bytes, BytesMut};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use pin_project_lite::pin_project;

/// A single token from a language model.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token ID from the vocabulary.
    pub id: u32,
    /// Optional decoded text representation.
    pub text: Option<String>,
    /// Optional log probability of this token.
    pub logprob: Option<f32>,
    /// Position in the sequence (0-indexed).
    pub position: u32,
    /// Whether this is a special token (BOS, EOS, PAD, etc.).
    pub is_special: bool,
}

impl Token {
    /// Creates a new token with just an ID.
    pub fn new(id: u32, position: u32) -> Self {
        Self {
            id,
            text: None,
            logprob: None,
            position,
            is_special: false,
        }
    }

    /// Creates a new token with text.
    pub fn with_text(id: u32, text: impl Into<String>, position: u32) -> Self {
        Self {
            id,
            text: Some(text.into()),
            logprob: None,
            position,
            is_special: false,
        }
    }

    /// Sets the log probability.
    pub fn with_logprob(mut self, logprob: f32) -> Self {
        self.logprob = Some(logprob);
        self
    }

    /// Marks this as a special token.
    pub fn as_special(mut self) -> Self {
        self.is_special = true;
        self
    }

    /// Encodes this token to bytes.
    ///
    /// Wire format:
    /// - id: u32 (4 bytes)
    /// - position: u32 (4 bytes)
    /// - flags: u8 (1 byte) - bit 0: has_text, bit 1: has_logprob, bit 2: is_special
    /// - logprob: f32 (4 bytes, optional)
    /// - text_len: u16 (2 bytes, optional)
    /// - text: [u8; text_len] (optional)
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(32);

        buf.put_u32(self.id);
        buf.put_u32(self.position);

        let flags = (self.text.is_some() as u8)
            | ((self.logprob.is_some() as u8) << 1)
            | ((self.is_special as u8) << 2);
        buf.put_u8(flags);

        if let Some(logprob) = self.logprob {
            buf.put_f32(logprob);
        }

        if let Some(ref text) = self.text {
            let text_bytes = text.as_bytes();
            buf.put_u16(text_bytes.len() as u16);
            buf.put_slice(text_bytes);
        }

        buf.freeze()
    }

    /// Decodes a token from bytes.
    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        if data.len() < 9 {
            return None;
        }

        let id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let position = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let flags = data[8];

        let has_text = (flags & 0x01) != 0;
        let has_logprob = (flags & 0x02) != 0;
        let is_special = (flags & 0x04) != 0;

        let mut offset = 9;

        let logprob = if has_logprob {
            if data.len() < offset + 4 {
                return None;
            }
            let lp = f32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;
            Some(lp)
        } else {
            None
        };

        let text = if has_text {
            if data.len() < offset + 2 {
                return None;
            }
            let text_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if data.len() < offset + text_len {
                return None;
            }
            let text_str = String::from_utf8_lossy(&data[offset..offset + text_len]).into_owned();
            offset += text_len;
            Some(text_str)
        } else {
            None
        };

        Some((
            Self {
                id,
                text,
                logprob,
                position,
                is_special,
            },
            offset,
        ))
    }
}

/// A batch of tokens for efficient streaming.
#[derive(Debug, Clone, Default)]
pub struct TokenBatch {
    /// Tokens in this batch.
    pub tokens: Vec<Token>,
    /// Optional sequence ID for multi-sequence generation.
    pub sequence_id: Option<u32>,
    /// Whether this batch is the final one in the stream.
    pub is_final: bool,
}

impl TokenBatch {
    /// Creates a new empty batch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a batch with tokens.
    pub fn with_tokens(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            sequence_id: None,
            is_final: false,
        }
    }

    /// Creates a final batch.
    pub fn final_batch(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            sequence_id: None,
            is_final: true,
        }
    }

    /// Sets the sequence ID.
    pub fn with_sequence_id(mut self, id: u32) -> Self {
        self.sequence_id = Some(id);
        self
    }

    /// Marks this as the final batch.
    pub fn as_final(mut self) -> Self {
        self.is_final = true;
        self
    }

    /// Adds a token to the batch.
    pub fn push(&mut self, token: Token) {
        self.tokens.push(token);
    }

    /// Returns the number of tokens in this batch.
    #[inline]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Returns whether this batch is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Encodes this batch to bytes.
    ///
    /// Wire format:
    /// - flags: u8 (bit 0: has_sequence_id, bit 1: is_final)
    /// - sequence_id: u32 (4 bytes, optional)
    /// - token_count: u16 (2 bytes)
    /// - tokens: [encoded Token; token_count]
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(64 + self.tokens.len() * 32);

        let flags = (self.sequence_id.is_some() as u8) | ((self.is_final as u8) << 1);
        buf.put_u8(flags);

        if let Some(seq_id) = self.sequence_id {
            buf.put_u32(seq_id);
        }

        buf.put_u16(self.tokens.len() as u16);
        for token in &self.tokens {
            buf.extend_from_slice(&token.encode());
        }

        buf.freeze()
    }

    /// Decodes a batch from bytes.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let flags = data[0];
        let has_sequence_id = (flags & 0x01) != 0;
        let is_final = (flags & 0x02) != 0;

        let mut offset = 1;

        let sequence_id = if has_sequence_id {
            if data.len() < offset + 4 {
                return None;
            }
            let id = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;
            Some(id)
        } else {
            None
        };

        if data.len() < offset + 2 {
            return None;
        }
        let token_count = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        let mut tokens = Vec::with_capacity(token_count);
        for _ in 0..token_count {
            let (token, consumed) = Token::decode(&data[offset..])?;
            tokens.push(token);
            offset += consumed;
        }

        Some(Self {
            tokens,
            sequence_id,
            is_final,
        })
    }

    /// Returns an iterator over the tokens.
    pub fn iter(&self) -> impl Iterator<Item = &Token> {
        self.tokens.iter()
    }
}

impl IntoIterator for TokenBatch {
    type Item = Token;
    type IntoIter = std::vec::IntoIter<Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.into_iter()
    }
}

impl<'a> IntoIterator for &'a TokenBatch {
    type Item = &'a Token;
    type IntoIter = std::slice::Iter<'a, Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.iter()
    }
}

pin_project! {
    /// A stream of token batches for LLM generation.
    pub struct TokenStream<S> {
        #[pin]
        inner: S,
    }
}

impl<S> TokenStream<S> {
    /// Creates a new token stream from an inner stream.
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// Consumes this wrapper and returns the inner stream.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S, E> Stream for TokenStream<S>
where
    S: Stream<Item = Result<TokenBatch, E>>,
{
    type Item = Result<TokenBatch, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

/// Builder for creating token batches incrementally.
#[derive(Debug, Default)]
pub struct TokenBatchBuilder {
    tokens: Vec<Token>,
    sequence_id: Option<u32>,
    max_size: usize,
}

impl TokenBatchBuilder {
    /// Creates a new builder with default max batch size.
    pub fn new() -> Self {
        Self {
            tokens: Vec::new(),
            sequence_id: None,
            max_size: 32,
        }
    }

    /// Creates a builder with the specified max batch size.
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            tokens: Vec::with_capacity(max_size),
            sequence_id: None,
            max_size,
        }
    }

    /// Sets the sequence ID for batches from this builder.
    pub fn with_sequence_id(mut self, id: u32) -> Self {
        self.sequence_id = Some(id);
        self
    }

    /// Adds a token, returning a batch if the max size is reached.
    pub fn push(&mut self, token: Token) -> Option<TokenBatch> {
        self.tokens.push(token);

        if self.tokens.len() >= self.max_size {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Flushes accumulated tokens into a batch.
    pub fn flush(&mut self) -> TokenBatch {
        TokenBatch {
            tokens: std::mem::take(&mut self.tokens),
            sequence_id: self.sequence_id,
            is_final: false,
        }
    }

    /// Creates the final batch with remaining tokens.
    pub fn finish(mut self) -> TokenBatch {
        TokenBatch {
            tokens: std::mem::take(&mut self.tokens),
            sequence_id: self.sequence_id,
            is_final: true,
        }
    }

    /// Returns the number of pending tokens.
    pub fn pending_count(&self) -> usize {
        self.tokens.len()
    }

    /// Returns whether there are pending tokens.
    pub fn has_pending(&self) -> bool {
        !self.tokens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let token = Token::new(42, 0);
        assert_eq!(token.id, 42);
        assert_eq!(token.position, 0);
        assert!(token.text.is_none());

        let token_with_text = Token::with_text(42, "hello", 1).with_logprob(-0.5);
        assert_eq!(token_with_text.text, Some("hello".to_string()));
        assert_eq!(token_with_text.logprob, Some(-0.5));
    }

    #[test]
    fn test_token_encode_decode() {
        let token = Token::with_text(1234, "test", 5)
            .with_logprob(-1.5)
            .as_special();

        let encoded = token.encode();
        let (decoded, _) = Token::decode(&encoded).unwrap();

        assert_eq!(decoded.id, token.id);
        assert_eq!(decoded.text, token.text);
        assert_eq!(decoded.logprob, token.logprob);
        assert_eq!(decoded.position, token.position);
        assert_eq!(decoded.is_special, token.is_special);
    }

    #[test]
    fn test_token_batch() {
        let mut batch = TokenBatch::new();
        batch.push(Token::new(1, 0));
        batch.push(Token::new(2, 1));
        batch.push(Token::new(3, 2));

        assert_eq!(batch.len(), 3);
        assert!(!batch.is_final);

        let encoded = batch.encode();
        let decoded = TokenBatch::decode(&encoded).unwrap();

        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded.tokens[0].id, 1);
        assert_eq!(decoded.tokens[2].id, 3);
    }

    #[test]
    fn test_token_batch_with_sequence() {
        let batch = TokenBatch::with_tokens(vec![Token::new(1, 0), Token::new(2, 1)])
            .with_sequence_id(42)
            .as_final();

        let encoded = batch.encode();
        let decoded = TokenBatch::decode(&encoded).unwrap();

        assert_eq!(decoded.sequence_id, Some(42));
        assert!(decoded.is_final);
    }

    #[test]
    fn test_batch_builder() {
        let mut builder = TokenBatchBuilder::with_max_size(3);

        // Should not return batch until max size
        assert!(builder.push(Token::new(1, 0)).is_none());
        assert!(builder.push(Token::new(2, 1)).is_none());

        // Third push should trigger batch
        let batch = builder.push(Token::new(3, 2));
        assert!(batch.is_some());
        assert_eq!(batch.unwrap().len(), 3);

        // Add more and finish
        builder.push(Token::new(4, 3));
        let final_batch = builder.finish();
        assert!(final_batch.is_final);
        assert_eq!(final_batch.len(), 1);
    }
}
