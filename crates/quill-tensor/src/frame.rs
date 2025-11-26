//! Zero-copy tensor frame protocol.
//!
//! Implements the 9-byte frame header format for efficient tensor streaming:
//!
//! ```text
//! ┌────────────┬────────────┬────────────┐
//! │ Frame Type │  Reserved  │   Length   │
//! │   (1 byte) │  (4 bytes) │  (4 bytes) │
//! └────────────┴────────────┴────────────┘
//! ```
//!
//! This format enables zero-copy tensor streaming by separating metadata
//! from payload, allowing receivers to pre-allocate buffers.

use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;

/// Size of the tensor frame header in bytes.
pub const TENSOR_FRAME_HEADER_SIZE: usize = 9;

/// Maximum payload size (4 GB - 1).
pub const MAX_PAYLOAD_SIZE: u32 = u32::MAX;

/// Frame types for the tensor streaming protocol.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameType {
    /// Standard protobuf message (existing Quill behavior).
    /// Used for backward compatibility with non-tensor payloads.
    ProtoMsg = 0x01,

    /// Stream termination marker.
    /// Signals that no more frames will be sent on this stream.
    EndStream = 0x02,

    /// Cancellation signal.
    /// Indicates the stream should be terminated due to error or client request.
    Cancel = 0x04,

    /// Flow control credit frame.
    /// Grants send permission to the peer (byte-based for tensors).
    Credit = 0x08,

    /// Tensor metadata frame (small protobuf).
    /// Contains shape, dtype, device info for pre-allocation.
    /// Receiver should allocate buffer upon receiving this frame.
    TensorMeta = 0x10,

    /// Raw tensor payload frame.
    /// Contains uncompressed tensor bytes that can be written directly
    /// into pre-allocated memory without parsing.
    TensorPayload = 0x11,

    /// Token batch frame for LLM streaming.
    /// Contains a batch of tokens with optional logprobs.
    TokenBatch = 0x20,
}

impl FrameType {
    /// Returns a human-readable name for this frame type.
    pub const fn name(&self) -> &'static str {
        match self {
            FrameType::ProtoMsg => "PROTO_MSG",
            FrameType::EndStream => "END_STREAM",
            FrameType::Cancel => "CANCEL",
            FrameType::Credit => "CREDIT",
            FrameType::TensorMeta => "TENSOR_META",
            FrameType::TensorPayload => "TENSOR_PAYLOAD",
            FrameType::TokenBatch => "TOKEN_BATCH",
        }
    }

    /// Returns whether this frame type carries tensor data.
    pub const fn is_tensor_frame(&self) -> bool {
        matches!(self, FrameType::TensorMeta | FrameType::TensorPayload)
    }

    /// Returns whether this frame type signals stream end or cancellation.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, FrameType::EndStream | FrameType::Cancel)
    }
}

impl TryFrom<u8> for FrameType {
    type Error = TensorFrameError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(FrameType::ProtoMsg),
            0x02 => Ok(FrameType::EndStream),
            0x04 => Ok(FrameType::Cancel),
            0x08 => Ok(FrameType::Credit),
            0x10 => Ok(FrameType::TensorMeta),
            0x11 => Ok(FrameType::TensorPayload),
            0x20 => Ok(FrameType::TokenBatch),
            _ => Err(TensorFrameError::UnknownFrameType(value)),
        }
    }
}

/// Errors that can occur when working with tensor frames.
#[derive(Debug, Error)]
pub enum TensorFrameError {
    /// Not enough bytes to parse a complete frame.
    #[error("incomplete frame: need at least {0} more bytes")]
    Incomplete(usize),

    /// Unknown frame type byte.
    #[error("unknown frame type: 0x{0:02x}")]
    UnknownFrameType(u8),

    /// Payload size exceeds maximum.
    #[error("payload too large: {0} bytes (max {MAX_PAYLOAD_SIZE})")]
    PayloadTooLarge(usize),

    /// Invalid frame structure.
    #[error("invalid frame: {0}")]
    Invalid(String),
}

/// A frame in the tensor streaming protocol.
///
/// # Wire Format
///
/// ```text
/// ┌────────────┬────────────┬────────────┬─────────────────┐
/// │ Frame Type │  Reserved  │   Length   │     Payload     │
/// │   (1 byte) │  (4 bytes) │  (4 bytes) │  (Length bytes) │
/// └────────────┴────────────┴────────────┴─────────────────┘
/// ```
///
/// The reserved bytes can be used for future extensions like:
/// - Compression hints
/// - Checksum flags
/// - Version information
#[derive(Debug, Clone)]
pub struct TensorFrame {
    /// Type of this frame.
    pub frame_type: FrameType,
    /// Reserved bytes for future use.
    pub reserved: [u8; 4],
    /// Frame payload.
    pub payload: Bytes,
}

impl TensorFrame {
    /// Creates a new frame with the given type and payload.
    pub fn new(frame_type: FrameType, payload: Bytes) -> Self {
        Self {
            frame_type,
            reserved: [0u8; 4],
            payload,
        }
    }

    /// Creates a new frame with reserved bytes set.
    pub fn with_reserved(frame_type: FrameType, reserved: [u8; 4], payload: Bytes) -> Self {
        Self {
            frame_type,
            reserved,
            payload,
        }
    }

    /// Creates a PROTO_MSG frame.
    pub fn proto_msg(payload: Bytes) -> Self {
        Self::new(FrameType::ProtoMsg, payload)
    }

    /// Creates a TENSOR_META frame.
    pub fn tensor_meta(payload: Bytes) -> Self {
        Self::new(FrameType::TensorMeta, payload)
    }

    /// Creates a TENSOR_PAYLOAD frame.
    pub fn tensor_payload(payload: Bytes) -> Self {
        Self::new(FrameType::TensorPayload, payload)
    }

    /// Creates a TOKEN_BATCH frame.
    pub fn token_batch(payload: Bytes) -> Self {
        Self::new(FrameType::TokenBatch, payload)
    }

    /// Creates an END_STREAM frame.
    pub fn end_stream() -> Self {
        Self::new(FrameType::EndStream, Bytes::new())
    }

    /// Creates a CANCEL frame with optional reason.
    pub fn cancel(reason: Option<&str>) -> Self {
        let payload = reason.map(|r| Bytes::copy_from_slice(r.as_bytes())).unwrap_or_default();
        Self::new(FrameType::Cancel, payload)
    }

    /// Creates a CREDIT frame granting the specified number of bytes.
    pub fn credit(bytes: u64) -> Self {
        let payload = Bytes::copy_from_slice(&bytes.to_le_bytes());
        Self::new(FrameType::Credit, payload)
    }

    /// Returns the total size of this frame when encoded.
    #[inline]
    pub fn encoded_size(&self) -> usize {
        TENSOR_FRAME_HEADER_SIZE + self.payload.len()
    }

    /// Encodes this frame to bytes.
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.encoded_size());
        self.encode_into(&mut buf);
        buf.freeze()
    }

    /// Encodes this frame into the given buffer.
    pub fn encode_into(&self, buf: &mut BytesMut) {
        buf.put_u8(self.frame_type as u8);
        buf.put_slice(&self.reserved);
        buf.put_u32(self.payload.len() as u32);
        buf.put_slice(&self.payload);
    }

    /// Decodes a frame from bytes.
    ///
    /// Returns the frame and the number of bytes consumed.
    pub fn decode(data: &[u8]) -> Result<(Self, usize), TensorFrameError> {
        if data.len() < TENSOR_FRAME_HEADER_SIZE {
            return Err(TensorFrameError::Incomplete(
                TENSOR_FRAME_HEADER_SIZE - data.len(),
            ));
        }

        let frame_type = FrameType::try_from(data[0])?;
        let reserved = [data[1], data[2], data[3], data[4]];
        let length = u32::from_be_bytes([data[5], data[6], data[7], data[8]]) as usize;

        let total_size = TENSOR_FRAME_HEADER_SIZE + length;
        if data.len() < total_size {
            return Err(TensorFrameError::Incomplete(total_size - data.len()));
        }

        let payload = Bytes::copy_from_slice(&data[TENSOR_FRAME_HEADER_SIZE..total_size]);

        Ok((
            Self {
                frame_type,
                reserved,
                payload,
            },
            total_size,
        ))
    }

    /// Decodes a frame from a Bytes buffer, advancing it.
    pub fn decode_from_bytes(data: &mut Bytes) -> Result<Self, TensorFrameError> {
        if data.len() < TENSOR_FRAME_HEADER_SIZE {
            return Err(TensorFrameError::Incomplete(
                TENSOR_FRAME_HEADER_SIZE - data.len(),
            ));
        }

        let frame_type = FrameType::try_from(data[0])?;
        let reserved = [data[1], data[2], data[3], data[4]];
        let length = u32::from_be_bytes([data[5], data[6], data[7], data[8]]) as usize;

        let total_size = TENSOR_FRAME_HEADER_SIZE + length;
        if data.len() < total_size {
            return Err(TensorFrameError::Incomplete(total_size - data.len()));
        }

        // Advance past header
        data.advance(TENSOR_FRAME_HEADER_SIZE);

        // Split off payload
        let payload = data.split_to(length);

        Ok(Self {
            frame_type,
            reserved,
            payload,
        })
    }
}

/// Parser for streaming tensor frames.
///
/// Handles partial frame data and buffers until complete frames
/// can be parsed.
#[derive(Debug, Default)]
pub struct TensorFrameParser {
    buffer: BytesMut,
}

impl TensorFrameParser {
    /// Creates a new parser.
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
        }
    }

    /// Creates a new parser with the specified buffer capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(capacity),
        }
    }

    /// Feeds data into the parser.
    pub fn feed(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Feeds a Bytes buffer into the parser.
    pub fn feed_bytes(&mut self, data: Bytes) {
        self.buffer.extend_from_slice(&data);
    }

    /// Attempts to parse the next frame.
    ///
    /// Returns `Ok(None)` if there isn't enough data for a complete frame.
    pub fn parse_frame(&mut self) -> Result<Option<TensorFrame>, TensorFrameError> {
        if self.buffer.len() < TENSOR_FRAME_HEADER_SIZE {
            return Ok(None);
        }

        // Peek at the length without consuming
        let length = u32::from_be_bytes([
            self.buffer[5],
            self.buffer[6],
            self.buffer[7],
            self.buffer[8],
        ]) as usize;

        let total_size = TENSOR_FRAME_HEADER_SIZE + length;
        if self.buffer.len() < total_size {
            return Ok(None);
        }

        // We have enough data, parse the frame
        let frame_type = FrameType::try_from(self.buffer[0])?;
        let reserved = [
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
            self.buffer[4],
        ];

        // Split off the frame data
        let frame_data = self.buffer.split_to(total_size);
        let payload = Bytes::copy_from_slice(&frame_data[TENSOR_FRAME_HEADER_SIZE..]);

        Ok(Some(TensorFrame {
            frame_type,
            reserved,
            payload,
        }))
    }

    /// Returns the number of buffered bytes.
    #[inline]
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns whether the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clears the internal buffer.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Reserved byte flags for future use.
pub mod reserved_flags {
    /// Indicates the payload is compressed.
    pub const COMPRESSED: u8 = 0x01;
    /// Indicates a checksum follows the payload.
    pub const HAS_CHECKSUM: u8 = 0x02;
    /// Indicates this is a continuation of a previous frame.
    pub const CONTINUATION: u8 = 0x04;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_type_conversion() {
        assert_eq!(FrameType::try_from(0x01).unwrap(), FrameType::ProtoMsg);
        assert_eq!(FrameType::try_from(0x10).unwrap(), FrameType::TensorMeta);
        assert_eq!(FrameType::try_from(0x11).unwrap(), FrameType::TensorPayload);
        assert!(FrameType::try_from(0xFF).is_err());
    }

    #[test]
    fn test_frame_encode_decode() {
        let payload = Bytes::from_static(b"hello tensor");
        let frame = TensorFrame::tensor_payload(payload.clone());

        let encoded = frame.encode();
        assert_eq!(encoded.len(), TENSOR_FRAME_HEADER_SIZE + payload.len());

        let (decoded, consumed) = TensorFrame::decode(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.frame_type, FrameType::TensorPayload);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_frame_parser() {
        let frame1 = TensorFrame::tensor_meta(Bytes::from_static(b"meta"));
        let frame2 = TensorFrame::tensor_payload(Bytes::from_static(b"payload data"));
        let frame3 = TensorFrame::end_stream();

        let mut buf = BytesMut::new();
        frame1.encode_into(&mut buf);
        frame2.encode_into(&mut buf);
        frame3.encode_into(&mut buf);

        let mut parser = TensorFrameParser::new();
        parser.feed(&buf);

        let parsed1 = parser.parse_frame().unwrap().unwrap();
        assert_eq!(parsed1.frame_type, FrameType::TensorMeta);
        assert_eq!(parsed1.payload, Bytes::from_static(b"meta"));

        let parsed2 = parser.parse_frame().unwrap().unwrap();
        assert_eq!(parsed2.frame_type, FrameType::TensorPayload);
        assert_eq!(parsed2.payload, Bytes::from_static(b"payload data"));

        let parsed3 = parser.parse_frame().unwrap().unwrap();
        assert_eq!(parsed3.frame_type, FrameType::EndStream);
        assert!(parsed3.payload.is_empty());

        // No more frames
        assert!(parser.parse_frame().unwrap().is_none());
    }

    #[test]
    fn test_partial_frame() {
        let frame = TensorFrame::tensor_payload(Bytes::from_static(b"test data"));
        let encoded = frame.encode();

        // Feed partial data
        let mut parser = TensorFrameParser::new();
        parser.feed(&encoded[..5]);

        // Should return None (incomplete)
        assert!(parser.parse_frame().unwrap().is_none());

        // Feed the rest
        parser.feed(&encoded[5..]);

        // Now should parse successfully
        let parsed = parser.parse_frame().unwrap().unwrap();
        assert_eq!(parsed.frame_type, FrameType::TensorPayload);
    }

    #[test]
    fn test_credit_frame() {
        let credit = TensorFrame::credit(1024 * 1024);
        let encoded = credit.encode();

        let (decoded, _) = TensorFrame::decode(&encoded).unwrap();
        assert_eq!(decoded.frame_type, FrameType::Credit);

        let granted = u64::from_le_bytes(decoded.payload[..8].try_into().unwrap());
        assert_eq!(granted, 1024 * 1024);
    }

    #[test]
    fn test_frame_type_properties() {
        assert!(FrameType::TensorMeta.is_tensor_frame());
        assert!(FrameType::TensorPayload.is_tensor_frame());
        assert!(!FrameType::ProtoMsg.is_tensor_frame());

        assert!(FrameType::EndStream.is_terminal());
        assert!(FrameType::Cancel.is_terminal());
        assert!(!FrameType::TensorPayload.is_terminal());
    }

    #[test]
    fn test_cancel_with_reason() {
        let frame = TensorFrame::cancel(Some("timeout"));
        assert_eq!(frame.frame_type, FrameType::Cancel);
        assert_eq!(frame.payload, Bytes::from_static(b"timeout"));

        let frame_no_reason = TensorFrame::cancel(None);
        assert!(frame_no_reason.payload.is_empty());
    }
}
