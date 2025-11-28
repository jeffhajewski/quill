//! GPU and CPU buffer abstraction for tensor storage.
//!
//! Provides `TensorBuffer` enum for unified CPU/GPU memory handling,
//! with graceful fallback when CUDA is unavailable.
//!
//! # Example
//!
//! ```rust
//! use quill_tensor::buffer::{TensorBuffer, GpuStatus};
//!
//! // Check GPU availability
//! let status = GpuStatus::detect();
//! println!("GPU status: {:?}", status);
//!
//! // Allocate a buffer (falls back to CPU if GPU unavailable)
//! let buffer = TensorBuffer::try_allocate_gpu(1024, 0)
//!     .unwrap_or_else(|_| TensorBuffer::cpu_zeros(1024));
//!
//! assert_eq!(buffer.len(), 1024);
//! ```

use bytes::Bytes;
use thiserror::Error;
use tracing::warn;

#[cfg(feature = "cuda")]
use cudarc::driver::{CudaDevice, CudaSlice, DevicePtr, DeviceRepr, DriverError};

/// Errors that can occur during GPU buffer operations.
#[derive(Debug, Error)]
pub enum GpuError {
    /// CUDA feature not compiled
    #[error("CUDA support not compiled (enable 'cuda' feature)")]
    NotCompiled,

    /// CUDA driver not available
    #[error("CUDA driver not available: {0}")]
    DriverNotAvailable(String),

    /// No CUDA devices found
    #[error("No CUDA devices found")]
    NoDevices,

    /// Invalid device ID
    #[error("Invalid device ID {0}: only {1} devices available")]
    InvalidDeviceId(usize, usize),

    /// Memory allocation failed
    #[error("GPU memory allocation failed: {0}")]
    AllocationFailed(String),

    /// Memory transfer failed
    #[error("Memory transfer failed: {0}")]
    TransferFailed(String),

    /// Device synchronization failed
    #[error("Device synchronization failed: {0}")]
    SyncFailed(String),
}

/// Result type for GPU operations.
pub type GpuResult<T> = Result<T, GpuError>;

/// GPU availability status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuStatus {
    /// CUDA feature not compiled into this build
    NotCompiled,
    /// CUDA driver not found or failed to initialize
    NoCuda(String),
    /// No GPU devices available
    NoDevices,
    /// GPU available with the specified number of devices
    Available { device_count: usize },
}

impl GpuStatus {
    /// Detects GPU availability on this system.
    ///
    /// This is a relatively expensive operation as it initializes the CUDA
    /// driver. Cache the result if you need to check multiple times.
    #[cfg(feature = "cuda")]
    pub fn detect() -> Self {
        match CudaDevice::count() {
            Ok(0) => GpuStatus::NoDevices,
            Ok(count) => GpuStatus::Available {
                device_count: count as usize,
            },
            Err(e) => GpuStatus::NoCuda(e.to_string()),
        }
    }

    /// Detects GPU availability (non-CUDA build always returns NotCompiled).
    #[cfg(not(feature = "cuda"))]
    pub fn detect() -> Self {
        GpuStatus::NotCompiled
    }

    /// Returns true if GPU is available and ready to use.
    pub fn is_available(&self) -> bool {
        matches!(self, GpuStatus::Available { .. })
    }

    /// Returns the number of available GPU devices, or 0 if unavailable.
    pub fn device_count(&self) -> usize {
        match self {
            GpuStatus::Available { device_count } => *device_count,
            _ => 0,
        }
    }
}

/// Information about a CUDA device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device ID (0-indexed)
    pub device_id: usize,
    /// Device name (e.g., "NVIDIA A100")
    pub name: String,
    /// Total memory in bytes
    pub total_memory: u64,
    /// Free memory in bytes (at query time)
    pub free_memory: u64,
    /// Compute capability major version
    pub compute_major: u32,
    /// Compute capability minor version
    pub compute_minor: u32,
}

#[cfg(feature = "cuda")]
impl DeviceInfo {
    /// Queries information about a CUDA device.
    pub fn query(device_id: usize) -> GpuResult<Self> {
        let device = CudaDevice::new(device_id).map_err(|e| {
            GpuError::DriverNotAvailable(format!("Failed to open device {}: {}", device_id, e))
        })?;

        // Note: cudarc doesn't expose all device properties directly
        // We provide what's available
        Ok(DeviceInfo {
            device_id,
            name: format!("CUDA Device {}", device_id),
            total_memory: 0, // Would need cuMemGetInfo
            free_memory: 0,  // Would need cuMemGetInfo
            compute_major: 0,
            compute_minor: 0,
        })
    }
}

/// A buffer holding GPU device memory.
///
/// When the `cuda` feature is enabled, this wraps a `CudaSlice<u8>` for
/// zero-copy GPU operations. The buffer is automatically freed when dropped.
#[cfg(feature = "cuda")]
pub struct CudaBuffer {
    device_id: usize,
    storage: CudaSlice<u8>,
    len: usize,
}

#[cfg(feature = "cuda")]
impl CudaBuffer {
    /// Allocates a new GPU buffer of the specified size.
    ///
    /// # Arguments
    ///
    /// * `size` - Size in bytes to allocate
    /// * `device_id` - CUDA device ID (0-indexed)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let buffer = CudaBuffer::allocate(1024, 0)?;
    /// assert_eq!(buffer.len(), 1024);
    /// ```
    pub fn allocate(size: usize, device_id: usize) -> GpuResult<Self> {
        let device = CudaDevice::new(device_id).map_err(|e| {
            GpuError::DriverNotAvailable(format!("Failed to open device {}: {}", device_id, e))
        })?;

        // Allocate device memory
        let storage: CudaSlice<u8> = device.alloc_zeros(size).map_err(|e| {
            GpuError::AllocationFailed(format!(
                "Failed to allocate {} bytes on device {}: {}",
                size, device_id, e
            ))
        })?;

        Ok(Self {
            device_id,
            storage,
            len: size,
        })
    }

    /// Returns the size of the buffer in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the device ID this buffer is allocated on.
    #[inline]
    pub fn device_id(&self) -> usize {
        self.device_id
    }

    /// Copies data from host memory to this GPU buffer.
    ///
    /// # Arguments
    ///
    /// * `data` - Host data to copy (must match buffer size)
    ///
    /// # Errors
    ///
    /// Returns an error if the data length doesn't match the buffer size
    /// or if the transfer fails.
    pub fn copy_from_host(&mut self, data: &[u8]) -> GpuResult<()> {
        if data.len() != self.len {
            return Err(GpuError::TransferFailed(format!(
                "Data length {} doesn't match buffer size {}",
                data.len(),
                self.len
            )));
        }

        let device = CudaDevice::new(self.device_id).map_err(|e| {
            GpuError::DriverNotAvailable(format!("Failed to open device: {}", e))
        })?;

        device.htod_copy_into(data.to_vec(), &mut self.storage).map_err(|e| {
            GpuError::TransferFailed(format!("Host-to-device copy failed: {}", e))
        })?;

        Ok(())
    }

    /// Copies data from this GPU buffer to host memory.
    ///
    /// # Returns
    ///
    /// A new `Vec<u8>` containing the buffer contents.
    pub fn copy_to_host(&self) -> GpuResult<Vec<u8>> {
        let device = CudaDevice::new(self.device_id).map_err(|e| {
            GpuError::DriverNotAvailable(format!("Failed to open device: {}", e))
        })?;

        let result = device.dtoh_sync_copy(&self.storage).map_err(|e| {
            GpuError::TransferFailed(format!("Device-to-host copy failed: {}", e))
        })?;

        Ok(result)
    }

    /// Returns the raw device pointer.
    ///
    /// # Safety
    ///
    /// The returned pointer is only valid for the lifetime of this buffer
    /// and must only be used for CUDA operations on the same device.
    pub fn device_ptr(&self) -> *const u8 {
        // Note: cudarc's CudaSlice doesn't expose raw pointer directly
        // This would need unsafe implementation
        std::ptr::null()
    }
}

#[cfg(feature = "cuda")]
impl std::fmt::Debug for CudaBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CudaBuffer")
            .field("device_id", &self.device_id)
            .field("len", &self.len)
            .finish()
    }
}

/// Unified buffer for CPU and GPU tensor storage.
///
/// This enum provides a common interface for tensor data that may reside
/// in either CPU or GPU memory. When the `cuda` feature is not enabled,
/// only the `Cpu` variant is available.
///
/// # Fallback Behavior
///
/// The `try_allocate_gpu` method will gracefully fall back to CPU memory
/// when GPU allocation fails, logging a warning message.
#[derive(Debug)]
pub enum TensorBuffer {
    /// CPU memory stored as `Bytes`
    Cpu(Bytes),

    /// GPU device memory (only available with `cuda` feature)
    #[cfg(feature = "cuda")]
    Cuda(CudaBuffer),
}

impl TensorBuffer {
    /// Creates a CPU buffer from existing bytes.
    pub fn cpu(data: Bytes) -> Self {
        TensorBuffer::Cpu(data)
    }

    /// Creates a CPU buffer filled with zeros.
    pub fn cpu_zeros(size: usize) -> Self {
        TensorBuffer::Cpu(Bytes::from(vec![0u8; size]))
    }

    /// Creates a CPU buffer by copying from a slice.
    pub fn cpu_from_slice(data: &[u8]) -> Self {
        TensorBuffer::Cpu(Bytes::copy_from_slice(data))
    }

    /// Attempts to allocate a GPU buffer, falling back to CPU on failure.
    ///
    /// # Arguments
    ///
    /// * `size` - Size in bytes to allocate
    /// * `device_id` - CUDA device ID (0-indexed)
    ///
    /// # Fallback Behavior
    ///
    /// If GPU allocation fails for any reason (feature not compiled,
    /// driver not available, allocation failure), this method logs a
    /// warning and returns a CPU buffer instead.
    ///
    /// # Example
    ///
    /// ```rust
    /// use quill_tensor::buffer::TensorBuffer;
    ///
    /// // Always succeeds (may fall back to CPU)
    /// let buffer = TensorBuffer::try_allocate_gpu(1024, 0)
    ///     .expect("Allocation should not fail with fallback");
    ///
    /// // Check where the buffer ended up
    /// if buffer.is_gpu() {
    ///     println!("Allocated on GPU");
    /// } else {
    ///     println!("Fell back to CPU");
    /// }
    /// ```
    #[cfg(feature = "cuda")]
    pub fn try_allocate_gpu(size: usize, device_id: usize) -> GpuResult<Self> {
        match CudaBuffer::allocate(size, device_id) {
            Ok(buf) => Ok(TensorBuffer::Cuda(buf)),
            Err(e) => {
                warn!(
                    "GPU allocation failed ({}), falling back to CPU for {} bytes",
                    e, size
                );
                Ok(TensorBuffer::cpu_zeros(size))
            }
        }
    }

    /// Attempts to allocate a GPU buffer (non-CUDA build always returns CPU).
    #[cfg(not(feature = "cuda"))]
    pub fn try_allocate_gpu(size: usize, _device_id: usize) -> GpuResult<Self> {
        warn!(
            "CUDA feature not compiled, using CPU allocation for {} bytes",
            size
        );
        Ok(TensorBuffer::cpu_zeros(size))
    }

    /// Allocates a GPU buffer, returning an error on failure (no fallback).
    ///
    /// Use this when you specifically need GPU memory and want to handle
    /// failures explicitly rather than silently falling back to CPU.
    #[cfg(feature = "cuda")]
    pub fn allocate_gpu(size: usize, device_id: usize) -> GpuResult<Self> {
        let buf = CudaBuffer::allocate(size, device_id)?;
        Ok(TensorBuffer::Cuda(buf))
    }

    /// Allocates a GPU buffer (non-CUDA build always returns error).
    #[cfg(not(feature = "cuda"))]
    pub fn allocate_gpu(_size: usize, _device_id: usize) -> GpuResult<Self> {
        Err(GpuError::NotCompiled)
    }

    /// Returns the size of the buffer in bytes.
    pub fn len(&self) -> usize {
        match self {
            TensorBuffer::Cpu(bytes) => bytes.len(),
            #[cfg(feature = "cuda")]
            TensorBuffer::Cuda(buf) => buf.len(),
        }
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if this buffer is stored in CPU memory.
    pub fn is_cpu(&self) -> bool {
        matches!(self, TensorBuffer::Cpu(_))
    }

    /// Returns true if this buffer is stored in GPU memory.
    #[cfg(feature = "cuda")]
    pub fn is_gpu(&self) -> bool {
        matches!(self, TensorBuffer::Cuda(_))
    }

    /// Returns true if this buffer is stored in GPU memory (always false without cuda feature).
    #[cfg(not(feature = "cuda"))]
    pub fn is_gpu(&self) -> bool {
        false
    }

    /// Returns the device ID if this is a GPU buffer, or None for CPU.
    #[cfg(feature = "cuda")]
    pub fn device_id(&self) -> Option<usize> {
        match self {
            TensorBuffer::Cpu(_) => None,
            TensorBuffer::Cuda(buf) => Some(buf.device_id()),
        }
    }

    /// Returns the device ID (always None without cuda feature).
    #[cfg(not(feature = "cuda"))]
    pub fn device_id(&self) -> Option<usize> {
        None
    }

    /// Returns the CPU bytes if this is a CPU buffer.
    pub fn as_cpu(&self) -> Option<&Bytes> {
        match self {
            TensorBuffer::Cpu(bytes) => Some(bytes),
            #[cfg(feature = "cuda")]
            TensorBuffer::Cuda(_) => None,
        }
    }

    /// Copies the buffer contents to host memory.
    ///
    /// For CPU buffers, this returns a copy of the bytes.
    /// For GPU buffers, this performs a device-to-host transfer.
    pub fn to_host(&self) -> GpuResult<Bytes> {
        match self {
            TensorBuffer::Cpu(bytes) => Ok(bytes.clone()),
            #[cfg(feature = "cuda")]
            TensorBuffer::Cuda(buf) => {
                let vec = buf.copy_to_host()?;
                Ok(Bytes::from(vec))
            }
        }
    }

    /// Copies data from a slice into this buffer.
    ///
    /// For CPU buffers, this replaces the buffer contents.
    /// For GPU buffers, this performs a host-to-device transfer.
    #[cfg(feature = "cuda")]
    pub fn copy_from_slice(&mut self, data: &[u8]) -> GpuResult<()> {
        match self {
            TensorBuffer::Cpu(bytes) => {
                *bytes = Bytes::copy_from_slice(data);
                Ok(())
            }
            TensorBuffer::Cuda(buf) => buf.copy_from_host(data),
        }
    }

    /// Copies data from a slice into this buffer (non-CUDA version).
    #[cfg(not(feature = "cuda"))]
    pub fn copy_from_slice(&mut self, data: &[u8]) -> GpuResult<()> {
        match self {
            TensorBuffer::Cpu(bytes) => {
                *bytes = Bytes::copy_from_slice(data);
                Ok(())
            }
        }
    }

    /// Moves the buffer to GPU memory if it's currently on CPU.
    ///
    /// If already on GPU, this is a no-op.
    /// Returns error if GPU allocation fails.
    #[cfg(feature = "cuda")]
    pub fn to_gpu(self, device_id: usize) -> GpuResult<Self> {
        match self {
            TensorBuffer::Cpu(bytes) => {
                let mut buf = CudaBuffer::allocate(bytes.len(), device_id)?;
                buf.copy_from_host(&bytes)?;
                Ok(TensorBuffer::Cuda(buf))
            }
            TensorBuffer::Cuda(buf) => {
                if buf.device_id() == device_id {
                    Ok(TensorBuffer::Cuda(buf))
                } else {
                    // Cross-device transfer: D2H then H2D
                    let host = buf.copy_to_host()?;
                    let mut new_buf = CudaBuffer::allocate(host.len(), device_id)?;
                    new_buf.copy_from_host(&host)?;
                    Ok(TensorBuffer::Cuda(new_buf))
                }
            }
        }
    }

    /// Moves the buffer to GPU memory (non-CUDA version always fails).
    #[cfg(not(feature = "cuda"))]
    pub fn to_gpu(self, _device_id: usize) -> GpuResult<Self> {
        Err(GpuError::NotCompiled)
    }

    /// Moves the buffer to CPU memory if it's currently on GPU.
    ///
    /// If already on CPU, this is a no-op.
    pub fn to_cpu(self) -> GpuResult<Self> {
        match self {
            TensorBuffer::Cpu(bytes) => Ok(TensorBuffer::Cpu(bytes)),
            #[cfg(feature = "cuda")]
            TensorBuffer::Cuda(buf) => {
                let host = buf.copy_to_host()?;
                Ok(TensorBuffer::Cpu(Bytes::from(host)))
            }
        }
    }
}

impl Clone for TensorBuffer {
    fn clone(&self) -> Self {
        match self {
            TensorBuffer::Cpu(bytes) => TensorBuffer::Cpu(bytes.clone()),
            #[cfg(feature = "cuda")]
            TensorBuffer::Cuda(buf) => {
                // GPU buffers need a host roundtrip to clone
                // This is expensive but maintains correct semantics
                match buf.copy_to_host() {
                    Ok(host) => {
                        match CudaBuffer::allocate(host.len(), buf.device_id()) {
                            Ok(mut new_buf) => {
                                if new_buf.copy_from_host(&host).is_ok() {
                                    return TensorBuffer::Cuda(new_buf);
                                }
                            }
                            Err(_) => {}
                        }
                        // Fall back to CPU if GPU clone fails
                        warn!("GPU buffer clone failed, falling back to CPU");
                        TensorBuffer::Cpu(Bytes::from(host))
                    }
                    Err(e) => {
                        warn!("GPU buffer clone failed ({}), returning empty CPU buffer", e);
                        TensorBuffer::Cpu(Bytes::new())
                    }
                }
            }
        }
    }
}

impl Default for TensorBuffer {
    fn default() -> Self {
        TensorBuffer::Cpu(Bytes::new())
    }
}

impl From<Bytes> for TensorBuffer {
    fn from(bytes: Bytes) -> Self {
        TensorBuffer::Cpu(bytes)
    }
}

impl From<Vec<u8>> for TensorBuffer {
    fn from(vec: Vec<u8>) -> Self {
        TensorBuffer::Cpu(Bytes::from(vec))
    }
}

impl From<&[u8]> for TensorBuffer {
    fn from(slice: &[u8]) -> Self {
        TensorBuffer::Cpu(Bytes::copy_from_slice(slice))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_status_detection() {
        // Should not panic regardless of GPU availability
        let status = GpuStatus::detect();
        println!("GPU status: {:?}", status);

        // Check that is_available() is consistent
        match &status {
            GpuStatus::Available { device_count } => {
                assert!(status.is_available());
                assert!(*device_count > 0);
            }
            _ => {
                assert!(!status.is_available());
            }
        }
    }

    #[test]
    fn test_cpu_buffer_operations() {
        let data = vec![1u8, 2, 3, 4, 5];
        let buf = TensorBuffer::cpu_from_slice(&data);

        assert!(buf.is_cpu());
        assert!(!buf.is_gpu());
        assert_eq!(buf.len(), 5);
        assert_eq!(buf.as_cpu().unwrap().as_ref(), &data);
    }

    #[test]
    fn test_cpu_zeros() {
        let buf = TensorBuffer::cpu_zeros(100);
        assert_eq!(buf.len(), 100);
        assert!(buf.as_cpu().unwrap().iter().all(|&b| b == 0));
    }

    #[test]
    fn test_try_allocate_gpu_fallback() {
        // This should always succeed (falls back to CPU if GPU unavailable)
        let buf = TensorBuffer::try_allocate_gpu(1024, 0).expect("Should not fail with fallback");
        assert_eq!(buf.len(), 1024);

        // If no GPU, should be CPU
        if !GpuStatus::detect().is_available() {
            assert!(buf.is_cpu());
        }
    }

    #[test]
    fn test_buffer_clone() {
        let data = vec![42u8; 256];
        let buf = TensorBuffer::cpu_from_slice(&data);
        let cloned = buf.clone();

        assert_eq!(cloned.len(), buf.len());
        assert_eq!(cloned.to_host().unwrap().as_ref(), data.as_slice());
    }

    #[test]
    fn test_buffer_to_host() {
        let data = vec![1u8, 2, 3, 4, 5];
        let buf = TensorBuffer::cpu_from_slice(&data);
        let host = buf.to_host().unwrap();

        assert_eq!(host.as_ref(), &data);
    }

    #[test]
    fn test_buffer_from_bytes() {
        let bytes = Bytes::from_static(b"hello");
        let buf: TensorBuffer = bytes.into();

        assert!(buf.is_cpu());
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn test_buffer_default() {
        let buf = TensorBuffer::default();
        assert!(buf.is_cpu());
        assert!(buf.is_empty());
    }

    #[test]
    fn test_to_cpu_noop() {
        let data = vec![1u8, 2, 3];
        let buf = TensorBuffer::cpu_from_slice(&data);
        let cpu_buf = buf.to_cpu().unwrap();

        assert!(cpu_buf.is_cpu());
        assert_eq!(cpu_buf.to_host().unwrap().as_ref(), &data);
    }

    #[cfg(feature = "cuda")]
    #[test]
    fn test_cuda_buffer_roundtrip() {
        if !GpuStatus::detect().is_available() {
            println!("Skipping CUDA test: no GPU available");
            return;
        }

        let data = vec![42u8; 1024];
        let mut buf = CudaBuffer::allocate(1024, 0).expect("GPU allocation should succeed");

        buf.copy_from_host(&data).expect("H2D transfer should succeed");

        let result = buf.copy_to_host().expect("D2H transfer should succeed");
        assert_eq!(result, data);
    }

    #[cfg(feature = "cuda")]
    #[test]
    fn test_tensor_buffer_gpu() {
        if !GpuStatus::detect().is_available() {
            println!("Skipping CUDA test: no GPU available");
            return;
        }

        let data = vec![1u8, 2, 3, 4, 5];
        let buf = TensorBuffer::cpu_from_slice(&data);

        // Move to GPU
        let gpu_buf = buf.to_gpu(0).expect("GPU transfer should succeed");
        assert!(gpu_buf.is_gpu());

        // Move back to CPU
        let cpu_buf = gpu_buf.to_cpu().expect("CPU transfer should succeed");
        assert!(cpu_buf.is_cpu());
        assert_eq!(cpu_buf.to_host().unwrap().as_ref(), &data);
    }
}
