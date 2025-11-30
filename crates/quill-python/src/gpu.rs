//! Python bindings for GPU tensor support and DLPack interop.
//!
//! This module provides Python access to:
//! - GPU status detection
//! - Tensor buffers (CPU and GPU)
//! - DLPack protocol for PyTorch/JAX interop
//! - CUDA Array Interface for CuPy/Numba interop

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use quill_tensor::{DLPackCapsule, GpuStatus, TensorBuffer, TensorMeta};

/// GPU availability status.
///
/// Use `GpuStatus.detect()` to check if CUDA GPU is available.
#[pyclass(name = "GpuStatus")]
#[derive(Clone, Debug)]
pub struct PyGpuStatus {
    inner: GpuStatus,
}

#[pymethods]
impl PyGpuStatus {
    /// Detects GPU availability.
    ///
    /// Returns:
    ///     GpuStatus indicating GPU availability
    ///
    /// Example:
    ///     >>> status = quill.GpuStatus.detect()
    ///     >>> if status.is_available:
    ///     ...     print(f"Found {status.device_count} GPU(s)")
    #[staticmethod]
    fn detect() -> Self {
        Self {
            inner: GpuStatus::detect(),
        }
    }

    /// Returns True if GPU is available.
    #[getter]
    fn is_available(&self) -> bool {
        self.inner.is_available()
    }

    /// Returns the number of available GPU devices.
    #[getter]
    fn device_count(&self) -> usize {
        self.inner.device_count()
    }

    /// Returns True if CUDA feature was compiled.
    #[getter]
    fn cuda_compiled(&self) -> bool {
        !matches!(self.inner, GpuStatus::NotCompiled)
    }

    /// Returns a human-readable status message.
    fn message(&self) -> String {
        match &self.inner {
            GpuStatus::NotCompiled => "CUDA feature not compiled".to_string(),
            GpuStatus::NoCuda(reason) => format!("CUDA not available: {}", reason),
            GpuStatus::NoDevices => "No GPU devices found".to_string(),
            GpuStatus::Available { device_count } => {
                format!("{} GPU(s) available", device_count)
            }
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "GpuStatus(available={}, devices={})",
            self.inner.is_available(),
            self.inner.device_count()
        )
    }
}

/// A tensor buffer that can reside on CPU or GPU.
///
/// Use `TensorBuffer.cpu()` or `TensorBuffer.gpu()` to create buffers.
#[pyclass(name = "TensorBuffer")]
#[derive(Clone)]
pub struct PyTensorBuffer {
    inner: TensorBuffer,
}

#[pymethods]
impl PyTensorBuffer {
    /// Creates a CPU buffer filled with zeros.
    ///
    /// Args:
    ///     size: Size in bytes
    ///
    /// Returns:
    ///     TensorBuffer on CPU
    #[staticmethod]
    fn cpu_zeros(size: usize) -> Self {
        Self {
            inner: TensorBuffer::cpu_zeros(size),
        }
    }

    /// Creates a CPU buffer from bytes.
    ///
    /// Args:
    ///     data: Raw bytes
    ///
    /// Returns:
    ///     TensorBuffer on CPU
    #[staticmethod]
    fn from_bytes(data: &[u8]) -> Self {
        Self {
            inner: TensorBuffer::cpu(bytes::Bytes::copy_from_slice(data)),
        }
    }

    /// Allocates a GPU buffer (falls back to CPU if unavailable).
    ///
    /// Args:
    ///     size: Size in bytes
    ///     device_id: GPU device ID (default: 0)
    ///
    /// Returns:
    ///     TensorBuffer on GPU (or CPU if GPU unavailable)
    #[staticmethod]
    #[pyo3(signature = (size, device_id=0))]
    fn try_allocate_gpu(size: usize, device_id: usize) -> PyResult<Self> {
        let buffer = TensorBuffer::try_allocate_gpu(size, device_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner: buffer })
    }

    /// Returns buffer size in bytes.
    #[getter]
    fn size(&self) -> usize {
        self.inner.len()
    }

    /// Returns True if buffer is on GPU.
    #[getter]
    fn is_gpu(&self) -> bool {
        self.inner.is_gpu()
    }

    /// Returns True if buffer is on CPU.
    #[getter]
    fn is_cpu(&self) -> bool {
        self.inner.is_cpu()
    }

    /// Copies buffer contents to CPU memory.
    ///
    /// Returns:
    ///     bytes: Buffer contents
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let data = self
            .inner
            .to_host()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(PyBytes::new_bound(py, &data))
    }

    /// Moves buffer to GPU.
    ///
    /// Args:
    ///     device_id: GPU device ID
    ///
    /// Returns:
    ///     New TensorBuffer on GPU
    fn to_gpu(&self, device_id: usize) -> PyResult<Self> {
        let buffer = self
            .inner
            .clone()
            .to_gpu(device_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner: buffer })
    }

    /// Moves buffer to CPU.
    ///
    /// Returns:
    ///     New TensorBuffer on CPU
    fn to_cpu(&self) -> PyResult<Self> {
        let buffer = self
            .inner
            .clone()
            .to_cpu()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { inner: buffer })
    }

    fn __repr__(&self) -> String {
        let location = if self.inner.is_gpu() { "GPU" } else { "CPU" };
        format!("TensorBuffer({} bytes on {})", self.inner.len(), location)
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }
}

impl PyTensorBuffer {
    pub fn inner(&self) -> &TensorBuffer {
        &self.inner
    }

    pub fn from_inner(inner: TensorBuffer) -> Self {
        Self { inner }
    }
}

/// DLPack capsule for tensor interchange with PyTorch, JAX, etc.
///
/// Use `Tensor.to_dlpack()` to export and `Tensor.from_dlpack()` to import.
///
/// Note: This class is unsendable (can only be used on the thread it was created).
#[pyclass(name = "DLPackCapsule", unsendable)]
pub struct PyDLPackCapsule {
    capsule: Option<DLPackCapsule>,
}

impl PyDLPackCapsule {
    pub fn new(capsule: DLPackCapsule) -> Self {
        Self {
            capsule: Some(capsule),
        }
    }

    pub fn take(&mut self) -> Option<DLPackCapsule> {
        self.capsule.take()
    }
}

#[pymethods]
impl PyDLPackCapsule {
    fn __repr__(&self) -> String {
        if self.capsule.is_some() {
            "DLPackCapsule(valid)".to_string()
        } else {
            "DLPackCapsule(consumed)".to_string()
        }
    }
}

// DLPack helper functions for use by tensor.rs
impl PyDLPackCapsule {
    /// Creates a DLPack capsule from tensor data.
    pub fn from_tensor_data(meta: &TensorMeta, data: &[u8]) -> PyResult<Self> {
        use quill_tensor::Tensor;

        let tensor = Tensor::new(meta.clone(), bytes::Bytes::copy_from_slice(data));

        let capsule = DLPackCapsule::from_tensor(&tensor)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self::new(capsule))
    }
}

#[cfg(all(test, feature = "python-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_status() {
        let status = PyGpuStatus::detect();
        // Should always return a valid status
        let _ = status.is_available();
        let _ = status.device_count();
        let _ = status.cuda_compiled();
        let _ = status.message();
    }

    #[test]
    fn test_tensor_buffer_cpu() {
        let buf = PyTensorBuffer::cpu_zeros(1024);
        assert_eq!(buf.size(), 1024);
        assert!(buf.is_cpu());
        assert!(!buf.is_gpu());
    }

    #[test]
    fn test_tensor_buffer_from_bytes() {
        let data = vec![1u8, 2, 3, 4];
        let buf = PyTensorBuffer::from_bytes(&data);
        assert_eq!(buf.size(), 4);
    }
}
