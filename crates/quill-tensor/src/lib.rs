//! Tensor types and streaming support for the Quill RPC framework.
//!
//! This crate provides first-class tensor and token streaming support for
//! LLM inference and agent-to-agent communication.
//!
//! # Features
//!
//! - **Zero-copy streaming**: Pre-allocate buffers based on tensor metadata
//! - **ML data types**: f32, f16, bf16, i8, i32, i64, u8, bool
//! - **Tensor streaming**: Chunk large tensors for efficient transfer
//! - **Token batching**: Efficient LLM token generation streaming
//!
//! # Example
//!
//! ```rust
//! use quill_tensor::{Tensor, TensorMeta, DType};
//! use bytes::Bytes;
//!
//! // Create a tensor
//! let meta = TensorMeta::new(vec![2, 3], DType::Float32);
//! let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
//! let tensor = Tensor::from_f32(&meta, &data);
//!
//! assert_eq!(tensor.numel(), 6);
//! assert_eq!(tensor.byte_size(), 24);
//! ```

pub mod dtype;
pub mod frame;
pub mod stream;
pub mod tensor;
pub mod token;

pub use dtype::DType;
pub use frame::{FrameType, TensorFrame, TensorFrameError};
pub use stream::{TensorChunk, TensorReceiver, TensorSender, TensorStream};
pub use tensor::{Tensor, TensorMeta, TensorView};
pub use token::{Token, TokenBatch, TokenStream};

/// Re-export half crate types for convenience
pub use half::{bf16, f16};
