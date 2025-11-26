//! LLM Inference Example
//!
//! This example demonstrates tensor and token streaming for LLM inference
//! using Quill's tensor support.
//!
//! # Features Demonstrated
//!
//! - Token streaming for text generation
//! - Tensor streaming for embeddings
//! - Zero-copy frame protocol (TENSOR_META, TENSOR_PAYLOAD, TOKEN_BATCH)
//! - Byte-based flow control with TensorCreditTracker
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐         ┌─────────────────────┐
//! │   Client    │────────▶│   LLM Server        │
//! │             │         │                     │
//! │ GenerateReq │────────▶│ Token Generation    │
//! │             │◀────────│ stream TokenBatch   │
//! │             │         │                     │
//! │ EmbedReq    │────────▶│ Embedding Extract   │
//! │             │◀────────│ TENSOR_META+PAYLOAD │
//! └─────────────┘         └─────────────────────┘
//! ```

use bytes::{Bytes, BytesMut};
use quill_core::QuillError;
use quill_tensor::{
    DType, FrameType, Tensor, TensorFrame, TensorFrameParser, TensorMeta, TensorReceiver,
    TensorSender, Token, TokenBatch, TokenBatchBuilder,
};
use std::time::Duration;

/// Vocabulary for our mock LLM
pub const VOCAB: &[&str] = &[
    "<bos>", // 0 - beginning of sequence
    "<eos>", // 1 - end of sequence
    "<pad>", // 2 - padding
    "Hello", // 3
    "World", // 4
    "!",     // 5
    "The",   // 6
    "quick", // 7
    "brown", // 8
    "fox",   // 9
    "jumps", // 10
    "over",  // 11
    "the",   // 12
    "lazy",  // 13
    "dog",   // 14
    ".",     // 15
    "AI",    // 16
    "is",    // 17
    "amazing", // 18
    "and",   // 19
    "powerful", // 20
];

/// Mock generation request
#[derive(Debug, Clone)]
pub struct GenerateRequest {
    pub prompt_ids: Vec<u32>,
    pub max_new_tokens: u32,
    pub temperature: f32,
    pub stream_embeddings: bool,
}

impl GenerateRequest {
    pub fn new(prompt: &str) -> Self {
        // Simple tokenization: map known words to IDs
        let prompt_ids = prompt
            .split_whitespace()
            .filter_map(|word| {
                VOCAB.iter().position(|&v| v.eq_ignore_ascii_case(word)).map(|i| i as u32)
            })
            .collect();

        Self {
            prompt_ids,
            max_new_tokens: 10,
            temperature: 0.7,
            stream_embeddings: false,
        }
    }

    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_new_tokens = max;
        self
    }

    pub fn with_embeddings(mut self) -> Self {
        self.stream_embeddings = true;
        self
    }

    /// Encode request to bytes
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(64);

        // Encode prompt IDs
        buf.extend_from_slice(&(self.prompt_ids.len() as u16).to_le_bytes());
        for &id in &self.prompt_ids {
            buf.extend_from_slice(&id.to_le_bytes());
        }

        // Encode parameters
        buf.extend_from_slice(&self.max_new_tokens.to_le_bytes());
        buf.extend_from_slice(&self.temperature.to_le_bytes());
        buf.extend_from_slice(&[self.stream_embeddings as u8]);

        buf.freeze()
    }

    /// Decode request from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 2 {
            return None;
        }

        let prompt_len = u16::from_le_bytes([data[0], data[1]]) as usize;
        let mut offset = 2;

        if data.len() < offset + prompt_len * 4 + 9 {
            return None;
        }

        let mut prompt_ids = Vec::with_capacity(prompt_len);
        for _ in 0..prompt_len {
            let id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            prompt_ids.push(id);
            offset += 4;
        }

        let max_new_tokens = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        let temperature = f32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        let stream_embeddings = data[offset] != 0;

        Some(Self {
            prompt_ids,
            max_new_tokens,
            temperature,
            stream_embeddings,
        })
    }
}

/// Mock embedding request
#[derive(Debug, Clone)]
pub struct EmbedRequest {
    pub input_ids: Vec<u32>,
    pub pool: bool,
}

impl EmbedRequest {
    pub fn new(text: &str) -> Self {
        let input_ids = text
            .split_whitespace()
            .filter_map(|word| {
                VOCAB.iter().position(|&v| v.eq_ignore_ascii_case(word)).map(|i| i as u32)
            })
            .collect();

        Self {
            input_ids,
            pool: true,
        }
    }

    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(32);
        buf.extend_from_slice(&(self.input_ids.len() as u16).to_le_bytes());
        for &id in &self.input_ids {
            buf.extend_from_slice(&id.to_le_bytes());
        }
        buf.extend_from_slice(&[self.pool as u8]);
        buf.freeze()
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 3 {
            return None;
        }

        let len = u16::from_le_bytes([data[0], data[1]]) as usize;
        let mut offset = 2;

        if data.len() < offset + len * 4 + 1 {
            return None;
        }

        let mut input_ids = Vec::with_capacity(len);
        for _ in 0..len {
            let id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            input_ids.push(id);
            offset += 4;
        }

        let pool = data[offset] != 0;

        Some(Self { input_ids, pool })
    }
}

// ============================================================================
// Mock LLM Server
// ============================================================================

/// Mock LLM that generates tokens
pub struct MockLLM {
    embedding_dim: usize,
}

impl MockLLM {
    pub fn new(embedding_dim: usize) -> Self {
        Self { embedding_dim }
    }

    /// Generate tokens from a prompt (simulates LLM generation)
    pub async fn generate(&self, request: &GenerateRequest) -> Vec<TokenBatch> {
        let mut batches = Vec::new();
        let mut builder = TokenBatchBuilder::with_max_size(4); // Batch 4 tokens at a time
        let mut position = request.prompt_ids.len() as u32;

        // Simulate generating tokens one at a time
        // In a real LLM, this would be the autoregressive loop
        let generated_sequence = self.mock_generate_sequence(request);

        for (i, token_id) in generated_sequence.iter().enumerate() {
            let token = Token::with_text(
                *token_id,
                VOCAB.get(*token_id as usize).unwrap_or(&"<unk>").to_string(),
                position,
            )
            .with_logprob(-0.5 - (i as f32 * 0.1)); // Mock logprobs

            position += 1;

            if let Some(batch) = builder.push(token) {
                batches.push(batch);
            }

            // Simulate generation delay
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Flush remaining tokens as final batch
        if builder.has_pending() {
            batches.push(builder.finish());
        } else {
            // Add empty final batch to signal completion
            let mut final_batch = TokenBatch::new();
            final_batch.is_final = true;
            batches.push(final_batch);
        }

        // Mark the last batch as final
        if let Some(last) = batches.last_mut() {
            last.is_final = true;
        }

        batches
    }

    /// Generate embedding for input tokens
    pub fn embed(&self, request: &EmbedRequest) -> Tensor {
        let seq_len = request.input_ids.len();
        let shape = if request.pool {
            vec![self.embedding_dim]
        } else {
            vec![seq_len, self.embedding_dim]
        };

        let meta = TensorMeta::new(shape.clone(), DType::Float32)
            .with_name("embedding");

        // Generate mock embeddings (in reality, this would be from the model)
        let numel: usize = shape.iter().product();
        let data: Vec<f32> = (0..numel)
            .map(|i| {
                // Deterministic "embedding" based on token ID
                let token_idx = i / self.embedding_dim;
                let dim_idx = i % self.embedding_dim;
                let token_id = request.input_ids.get(token_idx).unwrap_or(&0);
                ((*token_id as f32) * 0.01 + (dim_idx as f32) * 0.001).sin()
            })
            .collect();

        Tensor::from_f32(&meta, &data)
    }

    fn mock_generate_sequence(&self, request: &GenerateRequest) -> Vec<u32> {
        // Simple mock: generate a fixed pattern based on prompt
        // In reality, this would be the LLM's output
        let mut seq = Vec::new();

        // Generate up to max_new_tokens
        let patterns = [
            vec![16, 17, 18, 19, 20, 15], // "AI is amazing and powerful ."
            vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15], // "The quick brown fox jumps over the lazy dog ."
            vec![3, 4, 5], // "Hello World !"
        ];

        // Pick pattern based on first prompt token
        let pattern_idx = request.prompt_ids.first().unwrap_or(&0) % 3;
        let pattern = &patterns[pattern_idx as usize];

        for (i, &token_id) in pattern.iter().enumerate() {
            if i >= request.max_new_tokens as usize {
                break;
            }
            seq.push(token_id);
        }

        // Add EOS token
        if seq.len() < request.max_new_tokens as usize {
            seq.push(1); // <eos>
        }

        seq
    }
}

// ============================================================================
// Server Handler
// ============================================================================

/// Handle generate request - returns stream of token batches
pub async fn handle_generate(request: Bytes) -> Result<Bytes, QuillError> {
    let req = GenerateRequest::decode(&request)
        .ok_or_else(|| QuillError::Framing("Invalid generate request".to_string()))?;

    let llm = MockLLM::new(768);
    let batches = llm.generate(&req).await;

    // Encode all batches as TOKEN_BATCH frames
    let mut buf = BytesMut::new();
    for batch in batches {
        let payload = batch.encode();
        let frame = TensorFrame::token_batch(payload);
        frame.encode_into(&mut buf);
    }

    // Add END_STREAM frame
    let end_frame = TensorFrame::end_stream();
    end_frame.encode_into(&mut buf);

    Ok(buf.freeze())
}

/// Handle embed request - returns tensor as TENSOR_META + TENSOR_PAYLOAD frames
pub async fn handle_embed(request: Bytes) -> Result<Bytes, QuillError> {
    let req = EmbedRequest::decode(&request)
        .ok_or_else(|| QuillError::Framing("Invalid embed request".to_string()))?;

    let llm = MockLLM::new(768);
    let tensor = llm.embed(&req);

    // Encode tensor using TensorSender
    let sender = TensorSender::new();
    let frames = sender.encode_tensor(&tensor);

    // Serialize all frames
    let mut buf = BytesMut::new();
    for frame in frames {
        frame.encode_into(&mut buf);
    }

    Ok(buf.freeze())
}

// ============================================================================
// Client Helpers
// ============================================================================

/// Parse token batches from response
pub fn parse_token_stream(data: &[u8]) -> Result<Vec<TokenBatch>, QuillError> {
    let mut parser = TensorFrameParser::new();
    parser.feed(data);

    let mut batches = Vec::new();

    loop {
        match parser.parse_frame() {
            Ok(Some(frame)) => match frame.frame_type {
                FrameType::TokenBatch => {
                    if let Some(batch) = TokenBatch::decode(&frame.payload) {
                        batches.push(batch);
                    }
                }
                FrameType::EndStream => break,
                FrameType::Cancel => {
                    let reason = String::from_utf8_lossy(&frame.payload);
                    return Err(QuillError::Rpc(format!("Cancelled: {}", reason)));
                }
                _ => continue,
            },
            Ok(None) => break,
            Err(e) => return Err(QuillError::Framing(e.to_string())),
        }
    }

    Ok(batches)
}

/// Parse tensor from response
pub fn parse_tensor_response(data: &[u8]) -> Result<Tensor, QuillError> {
    let mut receiver = TensorReceiver::new();
    receiver.feed(data);

    // Process all frames
    loop {
        match receiver.poll() {
            Ok(quill_tensor::stream::ReceiverEvent::End) => break,
            Ok(quill_tensor::stream::ReceiverEvent::NeedMoreData) => break,
            Ok(quill_tensor::stream::ReceiverEvent::Cancelled(reason)) => {
                return Err(QuillError::Rpc(format!("Cancelled: {}", reason)));
            }
            Ok(_) => continue,
            Err(e) => return Err(QuillError::Framing(e.to_string())),
        }
    }

    receiver
        .take_tensor()
        .ok_or_else(|| QuillError::Framing("Failed to reassemble tensor".to_string()))
}

/// Decode tokens to text
pub fn tokens_to_text(batches: &[TokenBatch]) -> String {
    batches
        .iter()
        .flat_map(|batch| batch.iter())
        .filter_map(|token| token.text.as_ref())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_request_encoding() {
        let req = GenerateRequest::new("Hello World")
            .with_max_tokens(5)
            .with_embeddings();

        let encoded = req.encode();
        let decoded = GenerateRequest::decode(&encoded).unwrap();

        assert_eq!(decoded.prompt_ids, req.prompt_ids);
        assert_eq!(decoded.max_new_tokens, 5);
        assert!(decoded.stream_embeddings);
    }

    #[test]
    fn test_embed_request_encoding() {
        let req = EmbedRequest::new("The quick fox");

        let encoded = req.encode();
        let decoded = EmbedRequest::decode(&encoded).unwrap();

        assert_eq!(decoded.input_ids, req.input_ids);
        assert!(decoded.pool);
    }

    #[tokio::test]
    async fn test_mock_llm_generate() {
        let llm = MockLLM::new(768);
        let req = GenerateRequest::new("Hello").with_max_tokens(5);

        let batches = llm.generate(&req).await;

        assert!(!batches.is_empty());

        let total_tokens: usize = batches.iter().map(|b| b.len()).sum();
        assert!(total_tokens > 0);
        assert!(total_tokens <= 6); // max_new_tokens + EOS

        // Check last batch is marked as final
        assert!(batches.last().unwrap().is_final);
    }

    #[test]
    fn test_mock_llm_embed() {
        let llm = MockLLM::new(768);
        let req = EmbedRequest::new("Hello World");

        let tensor = llm.embed(&req);

        // Pooled embedding should have shape [768]
        assert_eq!(tensor.shape(), &[768]);
        assert_eq!(tensor.dtype(), DType::Float32);
    }

    #[test]
    fn test_mock_llm_embed_unpooled() {
        let llm = MockLLM::new(768);
        let mut req = EmbedRequest::new("Hello World");
        req.pool = false;

        let tensor = llm.embed(&req);

        // Unpooled should have shape [seq_len, 768]
        assert_eq!(tensor.shape(), &[2, 768]); // "Hello", "World"
    }

    #[tokio::test]
    async fn test_handle_generate() {
        let req = GenerateRequest::new("The").with_max_tokens(5);
        let response = handle_generate(req.encode()).await.unwrap();

        let batches = parse_token_stream(&response).unwrap();
        assert!(!batches.is_empty());

        let text = tokens_to_text(&batches);
        assert!(!text.is_empty());
        println!("Generated text: {}", text);
    }

    #[tokio::test]
    async fn test_handle_embed() {
        let req = EmbedRequest::new("AI is amazing");
        let response = handle_embed(req.encode()).await.unwrap();

        let tensor = parse_tensor_response(&response).unwrap();

        assert_eq!(tensor.dtype(), DType::Float32);
        assert_eq!(tensor.shape(), &[768]); // Pooled embedding
    }

    #[test]
    fn test_tensor_frame_protocol() {
        // Create a tensor
        let meta = TensorMeta::new(vec![4, 4], DType::Float32);
        let data: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let tensor = Tensor::from_f32(&meta, &data);

        // Encode with TensorSender
        let sender = TensorSender::new();
        let frames = sender.encode_tensor(&tensor);

        // Should have: TENSOR_META, TENSOR_PAYLOAD(s), END_STREAM
        assert!(frames.len() >= 3);
        assert_eq!(frames[0].frame_type, FrameType::TensorMeta);
        assert_eq!(frames.last().unwrap().frame_type, FrameType::EndStream);

        // Decode with TensorReceiver
        let mut buf = BytesMut::new();
        for frame in frames {
            frame.encode_into(&mut buf);
        }

        let tensor_back = parse_tensor_response(&buf).unwrap();
        assert_eq!(tensor_back.shape(), &[4, 4]);
        assert_eq!(tensor_back.as_f32(), &data);
    }

    #[test]
    fn test_token_batch_frame_protocol() {
        // Create a batch directly instead of using builder
        // (builder returns batch when max_size is reached)
        let batch = TokenBatch::with_tokens(vec![
            Token::with_text(3, "Hello", 0),
            Token::with_text(4, "World", 1),
        ]).as_final();
        assert!(batch.is_final);

        // Encode as TOKEN_BATCH frame
        let frame = TensorFrame::token_batch(batch.encode());
        let encoded = frame.encode();

        // Parse it back
        let mut parser = TensorFrameParser::new();
        parser.feed(&encoded);

        let parsed_frame = parser.parse_frame().unwrap().unwrap();
        assert_eq!(parsed_frame.frame_type, FrameType::TokenBatch);

        let parsed_batch = TokenBatch::decode(&parsed_frame.payload).unwrap();
        assert_eq!(parsed_batch.len(), 2);
        assert_eq!(parsed_batch.tokens[0].text, Some("Hello".to_string()));
        assert_eq!(parsed_batch.tokens[1].text, Some("World".to_string()));
    }

    #[test]
    fn test_vocabulary_lookup() {
        let req = GenerateRequest::new("Hello World The quick");

        // Should tokenize to IDs: 3, 4, 6, 7
        assert_eq!(req.prompt_ids, vec![3, 4, 6, 7]);
    }

    #[test]
    fn test_tokens_to_text() {
        let batches = vec![
            TokenBatch::with_tokens(vec![
                Token::with_text(3, "Hello", 0),
                Token::with_text(4, "World", 1),
            ]),
            TokenBatch::with_tokens(vec![
                Token::with_text(5, "!", 2),
            ]).as_final(),
        ];

        let text = tokens_to_text(&batches);
        assert_eq!(text, "Hello World !");
    }
}
