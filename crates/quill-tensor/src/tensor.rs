//! Tensor types for ML data representation.
//!
//! Provides `Tensor`, `TensorMeta`, and `TensorView` types for efficient
//! tensor storage and zero-copy access.
//!
//! # GPU Support
//!
//! Tensors can be stored on GPU when the `cuda` feature is enabled:
//!
//! ```rust,ignore
//! use quill_tensor::{TensorMeta, Device, TensorBuffer, GpuStatus};
//!
//! // Check GPU availability
//! if GpuStatus::detect().is_available() {
//!     let meta = TensorMeta::new(vec![1024, 768], DType::Float32)
//!         .with_device(Device::Cuda);
//!
//!     // Allocate on GPU (falls back to CPU if unavailable)
//!     let buffer = TensorBuffer::try_allocate_gpu(meta.byte_size(), 0)?;
//! }
//! ```

use bytes::{Bytes, BytesMut};

use crate::buffer::{GpuResult, TensorBuffer};
use crate::dtype::{DType, Element};

/// Device where the tensor data is located.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Device {
    /// CPU memory
    #[default]
    Cpu = 0,
    /// CUDA GPU memory
    Cuda = 1,
}

impl Device {
    /// Converts from protobuf Device enum value.
    pub fn from_proto(value: i32) -> Option<Self> {
        match value {
            0 => Some(Device::Cpu),
            1 => Some(Device::Cuda),
            _ => None,
        }
    }

    /// Converts to protobuf Device enum value.
    #[inline]
    pub const fn to_proto(&self) -> i32 {
        *self as i32
    }

    /// Returns true if this is a GPU device.
    #[inline]
    pub const fn is_gpu(&self) -> bool {
        matches!(self, Device::Cuda)
    }

    /// Returns true if this is a CPU device.
    #[inline]
    pub const fn is_cpu(&self) -> bool {
        matches!(self, Device::Cpu)
    }

    /// Allocates a buffer appropriate for this device.
    ///
    /// For CPU devices, allocates in host memory.
    /// For CUDA devices, attempts GPU allocation with fallback to CPU.
    ///
    /// # Arguments
    ///
    /// * `size` - Size in bytes to allocate
    /// * `device_id` - GPU device ID (ignored for CPU)
    pub fn allocate_buffer(&self, size: usize, device_id: usize) -> GpuResult<TensorBuffer> {
        match self {
            Device::Cpu => Ok(TensorBuffer::cpu_zeros(size)),
            Device::Cuda => TensorBuffer::try_allocate_gpu(size, device_id),
        }
    }
}

/// Metadata describing a tensor's shape, dtype, and layout.
///
/// This is sent as a `TENSOR_META` frame to allow receivers to pre-allocate
/// memory before the tensor payload arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorMeta {
    /// Shape of the tensor (e.g., `[batch, height, width, channels]`)
    pub shape: Vec<usize>,
    /// Data type of tensor elements
    pub dtype: DType,
    /// Device where tensor is located
    pub device: Device,
    /// Optional strides for non-contiguous tensors (in elements, not bytes)
    pub strides: Option<Vec<usize>>,
    /// Optional human-readable name
    pub name: Option<String>,
    /// Whether this tensor requires gradient computation
    pub requires_grad: bool,
}

impl TensorMeta {
    /// Creates new tensor metadata with the given shape and dtype.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use quill_tensor::{TensorMeta, DType};
    ///
    /// let meta = TensorMeta::new(vec![32, 768], DType::Float32);
    /// assert_eq!(meta.numel(), 32 * 768);
    /// assert_eq!(meta.byte_size(), 32 * 768 * 4);
    /// ```
    pub fn new(shape: Vec<usize>, dtype: DType) -> Self {
        Self {
            shape,
            dtype,
            device: Device::Cpu,
            strides: None,
            name: None,
            requires_grad: false,
        }
    }

    /// Sets the device for this tensor.
    pub fn with_device(mut self, device: Device) -> Self {
        self.device = device;
        self
    }

    /// Sets a name for this tensor.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets custom strides for non-contiguous layout.
    pub fn with_strides(mut self, strides: Vec<usize>) -> Self {
        self.strides = Some(strides);
        self
    }

    /// Sets whether this tensor requires gradient computation.
    pub fn with_requires_grad(mut self, requires_grad: bool) -> Self {
        self.requires_grad = requires_grad;
        self
    }

    /// Returns the total number of elements in the tensor.
    #[inline]
    pub fn numel(&self) -> usize {
        self.shape.iter().product()
    }

    /// Returns the total size in bytes of the tensor data.
    #[inline]
    pub fn byte_size(&self) -> usize {
        self.numel() * self.dtype.element_size()
    }

    /// Returns the number of dimensions.
    #[inline]
    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    /// Returns whether the tensor has a contiguous memory layout.
    pub fn is_contiguous(&self) -> bool {
        match &self.strides {
            None => true,
            Some(strides) => {
                if strides.len() != self.shape.len() {
                    return false;
                }
                // Check if strides match row-major (C) order
                let mut expected_stride = 1;
                for (i, &dim) in self.shape.iter().enumerate().rev() {
                    if strides[i] != expected_stride {
                        return false;
                    }
                    expected_stride *= dim;
                }
                true
            }
        }
    }

    /// Computes default row-major (C-order) strides for this shape.
    pub fn default_strides(&self) -> Vec<usize> {
        let mut strides = vec![1; self.shape.len()];
        for i in (0..self.shape.len().saturating_sub(1)).rev() {
            strides[i] = strides[i + 1] * self.shape[i + 1];
        }
        strides
    }

    /// Allocates a buffer appropriate for this tensor's device.
    ///
    /// For CPU devices, allocates in host memory.
    /// For CUDA devices, attempts GPU allocation with fallback to CPU.
    ///
    /// # Arguments
    ///
    /// * `device_id` - GPU device ID (ignored for CPU)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use quill_tensor::{TensorMeta, DType, Device};
    ///
    /// let meta = TensorMeta::new(vec![1024, 768], DType::Float32)
    ///     .with_device(Device::Cuda);
    ///
    /// let buffer = meta.allocate_buffer(0)?;  // Allocate on GPU 0
    /// ```
    pub fn allocate_buffer(&self, device_id: usize) -> GpuResult<TensorBuffer> {
        self.device.allocate_buffer(self.byte_size(), device_id)
    }
}

/// A tensor with owned data.
///
/// The data is stored in row-major (C) order as raw bytes, allowing
/// zero-copy transfer over the wire.
#[derive(Debug, Clone)]
pub struct Tensor {
    /// Tensor metadata
    pub meta: TensorMeta,
    /// Raw tensor data
    pub data: Bytes,
}

impl Tensor {
    /// Creates a new tensor from metadata and raw bytes.
    ///
    /// # Panics
    ///
    /// Panics if the data length doesn't match the expected byte size.
    pub fn new(meta: TensorMeta, data: Bytes) -> Self {
        assert_eq!(
            data.len(),
            meta.byte_size(),
            "Data length {} doesn't match expected byte size {}",
            data.len(),
            meta.byte_size()
        );
        Self { meta, data }
    }

    /// Creates a new tensor from a slice of f32 values.
    pub fn from_f32(meta: &TensorMeta, data: &[f32]) -> Self {
        assert_eq!(meta.dtype, DType::Float32, "Metadata dtype must be Float32");
        assert_eq!(
            data.len(),
            meta.numel(),
            "Data length doesn't match tensor shape"
        );

        let bytes = f32::as_bytes(data);
        Self {
            meta: meta.clone(),
            data: Bytes::copy_from_slice(bytes),
        }
    }

    /// Creates a new tensor from a slice of f64 values.
    pub fn from_f64(meta: &TensorMeta, data: &[f64]) -> Self {
        assert_eq!(meta.dtype, DType::Float64, "Metadata dtype must be Float64");
        assert_eq!(
            data.len(),
            meta.numel(),
            "Data length doesn't match tensor shape"
        );

        let bytes = f64::as_bytes(data);
        Self {
            meta: meta.clone(),
            data: Bytes::copy_from_slice(bytes),
        }
    }

    /// Creates a new tensor from a slice of i32 values.
    pub fn from_i32(meta: &TensorMeta, data: &[i32]) -> Self {
        assert_eq!(meta.dtype, DType::Int32, "Metadata dtype must be Int32");
        assert_eq!(
            data.len(),
            meta.numel(),
            "Data length doesn't match tensor shape"
        );

        let bytes = i32::as_bytes(data);
        Self {
            meta: meta.clone(),
            data: Bytes::copy_from_slice(bytes),
        }
    }

    /// Creates a new tensor from a slice of i64 values.
    pub fn from_i64(meta: &TensorMeta, data: &[i64]) -> Self {
        assert_eq!(meta.dtype, DType::Int64, "Metadata dtype must be Int64");
        assert_eq!(
            data.len(),
            meta.numel(),
            "Data length doesn't match tensor shape"
        );

        let bytes = i64::as_bytes(data);
        Self {
            meta: meta.clone(),
            data: Bytes::copy_from_slice(bytes),
        }
    }

    /// Creates a new tensor filled with zeros.
    pub fn zeros(meta: TensorMeta) -> Self {
        let data = Bytes::from(vec![0u8; meta.byte_size()]);
        Self { meta, data }
    }

    /// Returns the total number of elements.
    #[inline]
    pub fn numel(&self) -> usize {
        self.meta.numel()
    }

    /// Returns the total size in bytes.
    #[inline]
    pub fn byte_size(&self) -> usize {
        self.data.len()
    }

    /// Returns the shape of the tensor.
    #[inline]
    pub fn shape(&self) -> &[usize] {
        &self.meta.shape
    }

    /// Returns the data type of the tensor.
    #[inline]
    pub fn dtype(&self) -> DType {
        self.meta.dtype
    }

    /// Returns a view of the tensor data as the specified element type.
    ///
    /// # Safety
    ///
    /// The caller must ensure the dtype matches the element type.
    pub unsafe fn as_slice<T: Element>(&self) -> &[T] {
        T::from_bytes(&self.data)
    }

    /// Returns the data as f32 slice.
    ///
    /// # Panics
    ///
    /// Panics if dtype is not Float32.
    pub fn as_f32(&self) -> &[f32] {
        assert_eq!(self.meta.dtype, DType::Float32, "Tensor dtype must be Float32");
        unsafe { self.as_slice::<f32>() }
    }

    /// Returns the data as f64 slice.
    ///
    /// # Panics
    ///
    /// Panics if dtype is not Float64.
    pub fn as_f64(&self) -> &[f64] {
        assert_eq!(self.meta.dtype, DType::Float64, "Tensor dtype must be Float64");
        unsafe { self.as_slice::<f64>() }
    }

    /// Returns the data as i32 slice.
    ///
    /// # Panics
    ///
    /// Panics if dtype is not Int32.
    pub fn as_i32(&self) -> &[i32] {
        assert_eq!(self.meta.dtype, DType::Int32, "Tensor dtype must be Int32");
        unsafe { self.as_slice::<i32>() }
    }

    /// Returns the data as i64 slice.
    ///
    /// # Panics
    ///
    /// Panics if dtype is not Int64.
    pub fn as_i64(&self) -> &[i64] {
        assert_eq!(self.meta.dtype, DType::Int64, "Tensor dtype must be Int64");
        unsafe { self.as_slice::<i64>() }
    }

    /// Splits this tensor into chunks for streaming.
    ///
    /// Each chunk will be at most `max_chunk_bytes` in size.
    pub fn into_chunks(self, max_chunk_bytes: usize) -> Vec<TensorDataChunk> {
        let total_bytes = self.data.len();
        if total_bytes <= max_chunk_bytes {
            return vec![TensorDataChunk {
                sequence: 0,
                total_chunks: 1,
                data: self.data,
                is_final: true,
            }];
        }

        let num_chunks = (total_bytes + max_chunk_bytes - 1) / max_chunk_bytes;
        let mut chunks = Vec::with_capacity(num_chunks);
        let mut offset = 0;

        for i in 0..num_chunks {
            let end = std::cmp::min(offset + max_chunk_bytes, total_bytes);
            let chunk_data = self.data.slice(offset..end);
            chunks.push(TensorDataChunk {
                sequence: i as u32,
                total_chunks: num_chunks as u32,
                data: chunk_data,
                is_final: i == num_chunks - 1,
            });
            offset = end;
        }

        chunks
    }

    /// Creates a tensor view without copying data.
    pub fn view(&self) -> TensorView<'_> {
        TensorView {
            meta: &self.meta,
            data: &self.data,
        }
    }
}

/// A chunk of tensor data for streaming large tensors.
#[derive(Debug, Clone)]
pub struct TensorDataChunk {
    /// Sequence number for reassembly (0-indexed)
    pub sequence: u32,
    /// Total number of chunks
    pub total_chunks: u32,
    /// Raw chunk data
    pub data: Bytes,
    /// Whether this is the final chunk
    pub is_final: bool,
}

impl TensorDataChunk {
    /// Encodes this chunk to bytes for transmission.
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(9 + self.data.len());
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        buf.extend_from_slice(&self.total_chunks.to_le_bytes());
        buf.extend_from_slice(&[self.is_final as u8]);
        buf.extend_from_slice(&self.data);
        buf.freeze()
    }

    /// Decodes a chunk from bytes.
    pub fn decode(data: Bytes) -> Option<Self> {
        if data.len() < 9 {
            return None;
        }
        let sequence = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let total_chunks = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let is_final = data[8] != 0;
        let chunk_data = data.slice(9..);

        Some(Self {
            sequence,
            total_chunks,
            data: chunk_data,
            is_final,
        })
    }
}

/// A view into tensor data without ownership.
#[derive(Debug, Clone, Copy)]
pub struct TensorView<'a> {
    /// Reference to tensor metadata
    pub meta: &'a TensorMeta,
    /// Reference to raw tensor data
    pub data: &'a Bytes,
}

impl<'a> TensorView<'a> {
    /// Returns the total number of elements.
    #[inline]
    pub fn numel(&self) -> usize {
        self.meta.numel()
    }

    /// Returns the shape of the tensor.
    #[inline]
    pub fn shape(&self) -> &[usize] {
        &self.meta.shape
    }

    /// Returns the data type.
    #[inline]
    pub fn dtype(&self) -> DType {
        self.meta.dtype
    }

    /// Returns a view of the data as the specified element type.
    ///
    /// # Safety
    ///
    /// The caller must ensure the dtype matches the element type.
    pub unsafe fn as_slice<T: Element>(&self) -> &[T] {
        T::from_bytes(self.data)
    }
}

/// Builder for reassembling tensor chunks.
#[derive(Debug)]
pub struct TensorReassembler {
    meta: TensorMeta,
    chunks: Vec<Option<Bytes>>,
    received_count: usize,
}

impl TensorReassembler {
    /// Creates a new reassembler with the given metadata.
    pub fn new(meta: TensorMeta, total_chunks: u32) -> Self {
        Self {
            meta,
            chunks: vec![None; total_chunks as usize],
            received_count: 0,
        }
    }

    /// Adds a chunk to the reassembler.
    ///
    /// Returns `true` if all chunks have been received.
    pub fn add_chunk(&mut self, chunk: TensorDataChunk) -> bool {
        let idx = chunk.sequence as usize;
        if idx < self.chunks.len() && self.chunks[idx].is_none() {
            self.chunks[idx] = Some(chunk.data);
            self.received_count += 1;
        }
        self.received_count == self.chunks.len()
    }

    /// Returns whether all chunks have been received.
    pub fn is_complete(&self) -> bool {
        self.received_count == self.chunks.len()
    }

    /// Reassembles the tensor from received chunks.
    ///
    /// Returns `None` if not all chunks have been received.
    pub fn reassemble(self) -> Option<Tensor> {
        if !self.is_complete() {
            return None;
        }

        let total_size: usize = self.chunks.iter().map(|c| c.as_ref().unwrap().len()).sum();
        let mut buf = BytesMut::with_capacity(total_size);
        for chunk in self.chunks {
            buf.extend_from_slice(&chunk?);
        }

        Some(Tensor::new(self.meta, buf.freeze()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_meta() {
        let meta = TensorMeta::new(vec![2, 3, 4], DType::Float32);
        assert_eq!(meta.numel(), 24);
        assert_eq!(meta.byte_size(), 96);
        assert_eq!(meta.ndim(), 3);
        assert!(meta.is_contiguous());
    }

    #[test]
    fn test_tensor_from_f32() {
        let meta = TensorMeta::new(vec![2, 3], DType::Float32);
        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = Tensor::from_f32(&meta, &data);

        assert_eq!(tensor.numel(), 6);
        assert_eq!(tensor.byte_size(), 24);
        assert_eq!(tensor.as_f32(), &data);
    }

    #[test]
    fn test_tensor_zeros() {
        let meta = TensorMeta::new(vec![4, 4], DType::Float32);
        let tensor = Tensor::zeros(meta);

        assert_eq!(tensor.numel(), 16);
        let data = tensor.as_f32();
        assert!(data.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_tensor_chunking() {
        let meta = TensorMeta::new(vec![100], DType::Float32);
        let data: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let tensor = Tensor::from_f32(&meta, &data);

        // Split into chunks of 100 bytes (25 f32s)
        let chunks = tensor.into_chunks(100);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].sequence, 0);
        assert_eq!(chunks[3].sequence, 3);
        assert!(chunks[3].is_final);
    }

    #[test]
    fn test_tensor_reassembly() {
        let meta = TensorMeta::new(vec![100], DType::Float32);
        let data: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let tensor = Tensor::from_f32(&meta, &data);

        // Split and reassemble
        let chunks = tensor.clone().into_chunks(100);
        let mut reassembler = TensorReassembler::new(meta.clone(), chunks.len() as u32);

        for chunk in chunks {
            reassembler.add_chunk(chunk);
        }

        let reassembled = reassembler.reassemble().unwrap();
        assert_eq!(reassembled.as_f32(), &data);
    }

    #[test]
    fn test_default_strides() {
        let meta = TensorMeta::new(vec![2, 3, 4], DType::Float32);
        let strides = meta.default_strides();
        assert_eq!(strides, vec![12, 4, 1]);
    }

    #[test]
    fn test_tensor_meta_builder() {
        let meta = TensorMeta::new(vec![32, 768], DType::Float16)
            .with_name("embedding")
            .with_device(Device::Cuda)
            .with_requires_grad(true);

        assert_eq!(meta.name, Some("embedding".to_string()));
        assert_eq!(meta.device, Device::Cuda);
        assert!(meta.requires_grad);
    }

    #[test]
    fn test_device_is_gpu() {
        assert!(Device::Cuda.is_gpu());
        assert!(!Device::Cpu.is_gpu());
        assert!(Device::Cpu.is_cpu());
        assert!(!Device::Cuda.is_cpu());
    }

    #[test]
    fn test_device_allocate_buffer() {
        // CPU allocation should always work
        let buffer = Device::Cpu.allocate_buffer(1024, 0).unwrap();
        assert_eq!(buffer.len(), 1024);
        assert!(buffer.is_cpu());
    }

    #[test]
    fn test_tensor_meta_allocate_buffer() {
        let meta = TensorMeta::new(vec![10, 10], DType::Float32); // 400 bytes
        let buffer = meta.allocate_buffer(0).unwrap();
        assert_eq!(buffer.len(), 400);
    }

    #[test]
    fn test_cuda_device_allocate_with_fallback() {
        // CUDA allocation should fall back to CPU if GPU unavailable
        let buffer = Device::Cuda.allocate_buffer(1024, 0).unwrap();
        assert_eq!(buffer.len(), 1024);
        // May be CPU or GPU depending on hardware
    }
}
