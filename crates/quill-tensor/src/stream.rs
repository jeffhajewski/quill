//! Tensor streaming support for zero-copy transfer.
//!
//! Provides types for streaming tensor data with pre-allocation
//! and zero-copy semantics.

use bytes::{Bytes, BytesMut};
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use pin_project_lite::pin_project;

use crate::frame::{FrameType, TensorFrame, TensorFrameError, TensorFrameParser};
use crate::tensor::{Tensor, TensorMeta};

/// Error type for tensor streaming operations.
#[derive(Debug, thiserror::Error)]
pub enum TensorStreamError {
    /// Frame parsing error.
    #[error("frame error: {0}")]
    Frame(#[from] TensorFrameError),

    /// Received unexpected frame type.
    #[error("unexpected frame type: expected {expected}, got {actual}")]
    UnexpectedFrame {
        expected: &'static str,
        actual: &'static str,
    },

    /// Missing tensor metadata.
    #[error("missing tensor metadata: TENSOR_PAYLOAD received before TENSOR_META")]
    MissingMetadata,

    /// Tensor size mismatch.
    #[error("tensor size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch { expected: usize, actual: usize },

    /// Stream was cancelled.
    #[error("stream cancelled: {0}")]
    Cancelled(String),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// A chunk of tensor data for streaming.
#[derive(Debug, Clone)]
pub struct TensorChunk {
    /// Offset in bytes from the start of the tensor.
    pub offset: usize,
    /// Raw chunk data.
    pub data: Bytes,
}

impl TensorChunk {
    /// Creates a new chunk.
    pub fn new(offset: usize, data: Bytes) -> Self {
        Self { offset, data }
    }

    /// Returns the size of this chunk in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns whether this chunk is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

pin_project! {
    /// A stream of tensor chunks for receiving large tensors.
    pub struct TensorStream<S> {
        #[pin]
        inner: S,
        meta: Option<TensorMeta>,
    }
}

impl<S> TensorStream<S> {
    /// Creates a new tensor stream.
    pub fn new(inner: S) -> Self {
        Self { inner, meta: None }
    }

    /// Creates a tensor stream with known metadata.
    pub fn with_meta(inner: S, meta: TensorMeta) -> Self {
        Self {
            inner,
            meta: Some(meta),
        }
    }

    /// Returns the tensor metadata if available.
    pub fn meta(&self) -> Option<&TensorMeta> {
        self.meta.as_ref()
    }

    /// Consumes this wrapper and returns the inner stream.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S, E> Stream for TensorStream<S>
where
    S: Stream<Item = Result<TensorChunk, E>>,
{
    type Item = Result<TensorChunk, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

/// Sender for streaming tensor data.
///
/// Encodes tensor data as frames for efficient transfer.
pub struct TensorSender {
    chunk_size: usize,
}

impl TensorSender {
    /// Default chunk size (64 KB).
    pub const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

    /// Creates a new sender with default chunk size.
    pub fn new() -> Self {
        Self {
            chunk_size: Self::DEFAULT_CHUNK_SIZE,
        }
    }

    /// Creates a sender with custom chunk size.
    pub fn with_chunk_size(chunk_size: usize) -> Self {
        Self { chunk_size }
    }

    /// Encodes a tensor as a sequence of frames.
    ///
    /// Returns:
    /// 1. TENSOR_META frame with tensor metadata
    /// 2. One or more TENSOR_PAYLOAD frames with raw data
    /// 3. END_STREAM frame
    pub fn encode_tensor(&self, tensor: &Tensor) -> Vec<TensorFrame> {
        let mut frames = Vec::new();

        // Encode metadata as protobuf-like format
        let meta_payload = self.encode_meta(&tensor.meta);
        frames.push(TensorFrame::tensor_meta(meta_payload));

        // Split data into chunks
        let data = &tensor.data;
        let mut offset = 0;
        while offset < data.len() {
            let end = std::cmp::min(offset + self.chunk_size, data.len());
            let chunk = data.slice(offset..end);
            frames.push(TensorFrame::tensor_payload(chunk));
            offset = end;
        }

        // End stream
        frames.push(TensorFrame::end_stream());

        frames
    }

    /// Encodes tensor metadata to bytes.
    ///
    /// Simple binary format (not protobuf for efficiency):
    /// - ndim: u8
    /// - shape: [u64; ndim]
    /// - dtype: u8
    /// - device: u8
    /// - byte_size: u64
    /// - name_len: u16
    /// - name: [u8; name_len] (optional)
    fn encode_meta(&self, meta: &TensorMeta) -> Bytes {
        let name_bytes = meta.name.as_ref().map(|n| n.as_bytes()).unwrap_or(&[]);
        let capacity = 1 + meta.shape.len() * 8 + 1 + 1 + 8 + 2 + name_bytes.len();
        let mut buf = BytesMut::with_capacity(capacity);

        buf.extend_from_slice(&[meta.shape.len() as u8]);
        for &dim in &meta.shape {
            buf.extend_from_slice(&(dim as u64).to_le_bytes());
        }
        buf.extend_from_slice(&[meta.dtype as u8]);
        buf.extend_from_slice(&[meta.device as u8]);
        buf.extend_from_slice(&(meta.byte_size() as u64).to_le_bytes());
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        buf.freeze()
    }
}

impl Default for TensorSender {
    fn default() -> Self {
        Self::new()
    }
}

/// Receiver for streaming tensor data.
///
/// Decodes frames and assembles tensor data with zero-copy where possible.
pub struct TensorReceiver {
    parser: TensorFrameParser,
    meta: Option<TensorMeta>,
    buffer: BytesMut,
    expected_size: usize,
    received_size: usize,
}

impl TensorReceiver {
    /// Creates a new receiver.
    pub fn new() -> Self {
        Self {
            parser: TensorFrameParser::new(),
            meta: None,
            buffer: BytesMut::new(),
            expected_size: 0,
            received_size: 0,
        }
    }

    /// Creates a receiver with known metadata (enables pre-allocation).
    pub fn with_meta(meta: TensorMeta) -> Self {
        let byte_size = meta.byte_size();
        Self {
            parser: TensorFrameParser::new(),
            meta: Some(meta),
            buffer: BytesMut::with_capacity(byte_size),
            expected_size: byte_size,
            received_size: 0,
        }
    }

    /// Feeds raw bytes into the receiver.
    pub fn feed(&mut self, data: &[u8]) {
        self.parser.feed(data);
    }

    /// Feeds a Bytes buffer into the receiver.
    pub fn feed_bytes(&mut self, data: Bytes) {
        self.parser.feed_bytes(data);
    }

    /// Processes available frames and returns the next event.
    pub fn poll(&mut self) -> Result<ReceiverEvent, TensorStreamError> {
        match self.parser.parse_frame()? {
            None => Ok(ReceiverEvent::NeedMoreData),
            Some(frame) => self.handle_frame(frame),
        }
    }

    /// Returns the tensor metadata if received.
    pub fn meta(&self) -> Option<&TensorMeta> {
        self.meta.as_ref()
    }

    /// Returns whether all expected data has been received.
    pub fn is_complete(&self) -> bool {
        self.expected_size > 0 && self.received_size >= self.expected_size
    }

    /// Takes the completed tensor, returning None if not complete.
    pub fn take_tensor(&mut self) -> Option<Tensor> {
        if !self.is_complete() {
            return None;
        }

        let meta = self.meta.take()?;
        let data = std::mem::take(&mut self.buffer).freeze();
        self.received_size = 0;
        self.expected_size = 0;

        Some(Tensor::new(meta, data))
    }

    fn handle_frame(&mut self, frame: TensorFrame) -> Result<ReceiverEvent, TensorStreamError> {
        match frame.frame_type {
            FrameType::TensorMeta => {
                let meta = self.decode_meta(&frame.payload)?;
                self.expected_size = meta.byte_size();
                self.buffer = BytesMut::with_capacity(self.expected_size);
                self.received_size = 0;
                self.meta = Some(meta.clone());
                Ok(ReceiverEvent::Metadata(meta))
            }
            FrameType::TensorPayload => {
                if self.meta.is_none() {
                    return Err(TensorStreamError::MissingMetadata);
                }
                let chunk_size = frame.payload.len();
                self.buffer.extend_from_slice(&frame.payload);
                self.received_size += chunk_size;
                Ok(ReceiverEvent::Data(TensorChunk::new(
                    self.received_size - chunk_size,
                    frame.payload,
                )))
            }
            FrameType::EndStream => {
                if self.expected_size > 0 && self.received_size != self.expected_size {
                    return Err(TensorStreamError::SizeMismatch {
                        expected: self.expected_size,
                        actual: self.received_size,
                    });
                }
                Ok(ReceiverEvent::End)
            }
            FrameType::Cancel => {
                let reason = String::from_utf8_lossy(&frame.payload).into_owned();
                Ok(ReceiverEvent::Cancelled(reason))
            }
            _ => Err(TensorStreamError::UnexpectedFrame {
                expected: "TENSOR_META, TENSOR_PAYLOAD, END_STREAM, or CANCEL",
                actual: frame.frame_type.name(),
            }),
        }
    }

    fn decode_meta(&self, data: &[u8]) -> Result<TensorMeta, TensorStreamError> {
        if data.is_empty() {
            return Err(TensorStreamError::Internal("empty metadata".to_string()));
        }

        let ndim = data[0] as usize;
        let mut offset = 1;

        if data.len() < offset + ndim * 8 + 1 + 1 + 8 + 2 {
            return Err(TensorStreamError::Internal("metadata too short".to_string()));
        }

        let mut shape = Vec::with_capacity(ndim);
        for _ in 0..ndim {
            let dim = u64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]) as usize;
            shape.push(dim);
            offset += 8;
        }

        let dtype = crate::dtype::DType::try_from(data[offset]).map_err(|_| {
            TensorStreamError::Internal(format!("unknown dtype: {}", data[offset]))
        })?;
        offset += 1;

        let device = crate::tensor::Device::from_proto(data[offset] as i32)
            .ok_or_else(|| TensorStreamError::Internal(format!("unknown device: {}", data[offset])))?;
        offset += 1;

        // Skip byte_size (we compute it from shape)
        offset += 8;

        let name_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        let name = if name_len > 0 && data.len() >= offset + name_len {
            Some(String::from_utf8_lossy(&data[offset..offset + name_len]).into_owned())
        } else {
            None
        };

        Ok(TensorMeta {
            shape,
            dtype,
            device,
            strides: None,
            name,
            requires_grad: false,
        })
    }
}

impl Default for TensorReceiver {
    fn default() -> Self {
        Self::new()
    }
}

/// Events produced by the tensor receiver.
#[derive(Debug)]
pub enum ReceiverEvent {
    /// Tensor metadata received - receiver can now pre-allocate.
    Metadata(TensorMeta),
    /// Tensor data chunk received.
    Data(TensorChunk),
    /// Stream ended successfully.
    End,
    /// Stream was cancelled.
    Cancelled(String),
    /// Need more data to parse next frame.
    NeedMoreData,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DType;

    #[test]
    fn test_tensor_sender_small() {
        let meta = TensorMeta::new(vec![2, 3], DType::Float32);
        let tensor = Tensor::from_f32(&meta, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

        let sender = TensorSender::new();
        let frames = sender.encode_tensor(&tensor);

        // Should have: TENSOR_META, TENSOR_PAYLOAD, END_STREAM
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].frame_type, FrameType::TensorMeta);
        assert_eq!(frames[1].frame_type, FrameType::TensorPayload);
        assert_eq!(frames[2].frame_type, FrameType::EndStream);
    }

    #[test]
    fn test_tensor_sender_chunked() {
        let meta = TensorMeta::new(vec![1024], DType::Float32);
        let data: Vec<f32> = (0..1024).map(|i| i as f32).collect();
        let tensor = Tensor::from_f32(&meta, &data);

        // Use small chunk size to force multiple chunks
        let sender = TensorSender::with_chunk_size(1024);
        let frames = sender.encode_tensor(&tensor);

        // 4096 bytes / 1024 = 4 payload frames
        assert!(frames.len() > 3);
        assert_eq!(frames[0].frame_type, FrameType::TensorMeta);
        assert_eq!(frames.last().unwrap().frame_type, FrameType::EndStream);
    }

    #[test]
    fn test_tensor_receiver() {
        let meta = TensorMeta::new(vec![4], DType::Float32);
        let tensor = Tensor::from_f32(&meta, &[1.0, 2.0, 3.0, 4.0]);

        let sender = TensorSender::new();
        let frames = sender.encode_tensor(&tensor);

        let mut receiver = TensorReceiver::new();

        // Feed all frames
        for frame in frames {
            receiver.feed(&frame.encode());
        }

        // Process frames
        let mut got_meta = false;
        let mut got_data = false;
        let mut got_end = false;

        loop {
            match receiver.poll().unwrap() {
                ReceiverEvent::Metadata(m) => {
                    assert_eq!(m.shape, vec![4]);
                    assert_eq!(m.dtype, DType::Float32);
                    got_meta = true;
                }
                ReceiverEvent::Data(chunk) => {
                    assert_eq!(chunk.len(), 16); // 4 * f32
                    got_data = true;
                }
                ReceiverEvent::End => {
                    got_end = true;
                    break;
                }
                ReceiverEvent::NeedMoreData => break,
                ReceiverEvent::Cancelled(_) => panic!("unexpected cancel"),
            }
        }

        assert!(got_meta);
        assert!(got_data);
        assert!(got_end);

        // Take the tensor
        let received = receiver.take_tensor().unwrap();
        assert_eq!(received.as_f32(), &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_receiver_with_prealloc() {
        let meta = TensorMeta::new(vec![100], DType::Float32);
        let data: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let tensor = Tensor::from_f32(&meta, &data);

        let sender = TensorSender::new();
        let frames = sender.encode_tensor(&tensor);

        // Create receiver with known metadata (enables pre-allocation)
        let mut receiver = TensorReceiver::with_meta(meta);

        for frame in frames {
            receiver.feed(&frame.encode());
        }

        // Process until complete
        loop {
            match receiver.poll().unwrap() {
                ReceiverEvent::End => break,
                ReceiverEvent::NeedMoreData => break,
                _ => continue,
            }
        }

        let received = receiver.take_tensor().unwrap();
        assert_eq!(received.numel(), 100);
    }
}
