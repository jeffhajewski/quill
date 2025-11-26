//! Python bindings for Tensor types with NumPy integration.

use crate::dtype::PyDType;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use quill_tensor::{DType, TensorMeta};
use std::sync::Arc;

/// Tensor metadata describing shape and data type.
#[pyclass(name = "TensorMeta")]
#[derive(Clone, Debug)]
pub struct PyTensorMeta {
    inner: TensorMeta,
}

#[pymethods]
impl PyTensorMeta {
    /// Create new tensor metadata.
    ///
    /// Args:
    ///     shape: Tensor dimensions as a list
    ///     dtype: Data type (DType)
    ///     name: Optional tensor name/identifier
    #[new]
    #[pyo3(signature = (shape, dtype, name=None))]
    fn new(shape: Vec<usize>, dtype: PyDType, name: Option<String>) -> Self {
        let mut meta = TensorMeta::new(shape, dtype.inner());
        if let Some(n) = name {
            meta = meta.with_name(n);
        }
        Self { inner: meta }
    }

    /// Get tensor name (if set)
    #[getter]
    fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    /// Get tensor shape as a list
    #[getter]
    fn shape(&self) -> Vec<usize> {
        self.inner.shape.clone()
    }

    /// Get tensor data type
    #[getter]
    fn dtype(&self) -> PyDType {
        PyDType::from_inner(self.inner.dtype)
    }

    /// Get total number of elements
    #[getter]
    fn num_elements(&self) -> usize {
        self.inner.numel()
    }

    /// Get size in bytes
    #[getter]
    fn size_bytes(&self) -> usize {
        self.inner.byte_size()
    }

    /// Get number of dimensions
    #[getter]
    fn ndim(&self) -> usize {
        self.inner.shape.len()
    }

    fn __repr__(&self) -> String {
        let name_str = self.inner.name.as_deref().unwrap_or("unnamed");
        format!(
            "TensorMeta(name='{}', shape={:?}, dtype={})",
            name_str,
            self.inner.shape,
            PyDType::from_inner(self.inner.dtype).name()
        )
    }
}

impl PyTensorMeta {
    pub fn inner(&self) -> &TensorMeta {
        &self.inner
    }

    pub fn from_inner(inner: TensorMeta) -> Self {
        Self { inner }
    }
}

/// A multi-dimensional tensor with data.
///
/// Tensors can be created from NumPy arrays and converted back to NumPy.
/// This provides efficient interop for ML inference.
#[pyclass(name = "Tensor")]
#[derive(Clone)]
pub struct PyTensor {
    meta: TensorMeta,
    data: Arc<Vec<u8>>,
}

#[pymethods]
impl PyTensor {
    /// Create a tensor from a NumPy array.
    ///
    /// Args:
    ///     array: NumPy array (supports float32, float64, int32, int64, uint8, bool)
    ///     name: Optional tensor name (defaults to "tensor")
    ///
    /// Returns:
    ///     Tensor containing the array data
    #[staticmethod]
    #[pyo3(signature = (array, name=None))]
    fn from_numpy(_py: Python<'_>, array: &Bound<'_, PyAny>, name: Option<String>) -> PyResult<Self> {
        // Get array shape
        let shape_obj = array.getattr("shape")?;
        let shape: Vec<usize> = shape_obj.extract()?;

        // Get dtype string
        let dtype_obj = array.getattr("dtype")?;
        let dtype_name: String = dtype_obj.getattr("name")?.extract()?;

        // Map numpy dtype to quill dtype
        let dtype = match dtype_name.as_str() {
            "float32" => DType::Float32,
            "float64" => DType::Float64,
            "float16" => DType::Float16,
            "int8" => DType::Int8,
            "int32" => DType::Int32,
            "int64" => DType::Int64,
            "uint8" => DType::UInt8,
            "bool" => DType::Bool,
            other => {
                return Err(PyTypeError::new_err(format!(
                    "Unsupported numpy dtype: {}. Supported: float32, float64, float16, int8, int32, int64, uint8, bool",
                    other
                )));
            }
        };

        // Get raw bytes from the array
        let tobytes = array.call_method0("tobytes")?;
        let data: Vec<u8> = tobytes.extract()?;

        let mut meta = TensorMeta::new(shape, dtype);
        if let Some(n) = name {
            meta = meta.with_name(n);
        }

        Ok(Self {
            meta,
            data: Arc::new(data),
        })
    }

    /// Create a tensor with zeros.
    ///
    /// Args:
    ///     shape: Tensor dimensions
    ///     dtype: Data type
    ///     name: Optional tensor name
    #[staticmethod]
    #[pyo3(signature = (shape, dtype, name=None))]
    fn zeros(shape: Vec<usize>, dtype: PyDType, name: Option<String>) -> Self {
        let mut meta = TensorMeta::new(shape, dtype.inner());
        if let Some(n) = name {
            meta = meta.with_name(n);
        }
        let size = meta.byte_size();
        let data = vec![0u8; size];

        Self {
            meta,
            data: Arc::new(data),
        }
    }

    /// Create a tensor from raw bytes.
    ///
    /// Args:
    ///     data: Raw byte data
    ///     shape: Tensor dimensions
    ///     dtype: Data type
    ///     name: Optional tensor name
    #[staticmethod]
    #[pyo3(signature = (data, shape, dtype, name=None))]
    fn from_bytes(data: Vec<u8>, shape: Vec<usize>, dtype: PyDType, name: Option<String>) -> PyResult<Self> {
        let mut meta = TensorMeta::new(shape, dtype.inner());
        if let Some(n) = name {
            meta = meta.with_name(n);
        }

        if data.len() != meta.byte_size() {
            return Err(PyValueError::new_err(format!(
                "Data size {} does not match expected size {} for shape {:?} and dtype {}",
                data.len(),
                meta.byte_size(),
                meta.shape,
                dtype.name()
            )));
        }

        Ok(Self {
            meta,
            data: Arc::new(data),
        })
    }

    /// Convert tensor to a NumPy array.
    ///
    /// Returns:
    ///     NumPy array with the tensor data
    fn to_numpy<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        let numpy = py.import_bound("numpy")?;
        let frombuffer = numpy.getattr("frombuffer")?;

        // Map quill dtype to numpy dtype string
        let np_dtype = match self.meta.dtype {
            DType::Float32 => "float32",
            DType::Float64 => "float64",
            DType::Float16 => "float16",
            DType::Int8 => "int8",
            DType::Int32 => "int32",
            DType::Int64 => "int64",
            DType::UInt8 => "uint8",
            DType::Bool => "bool",
            DType::BFloat16 => {
                return Err(PyTypeError::new_err(
                    "bfloat16 is not directly supported by NumPy. Use view as uint16 instead."
                ));
            }
        };

        // Create numpy array from bytes
        let data_bytes = pyo3::types::PyBytes::new_bound(py, &self.data);
        let array = frombuffer.call1((data_bytes, np_dtype))?;

        // Reshape to original shape
        let shape_tuple = PyTuple::new_bound(
            py,
            self.meta.shape.iter().map(|&x| x as i64),
        );
        let reshaped = array.call_method1("reshape", (shape_tuple,))?;

        // Return a copy to ensure memory safety
        let copied = reshaped.call_method0("copy")?;
        Ok(copied.unbind())
    }

    /// Get tensor metadata
    #[getter]
    fn meta(&self) -> PyTensorMeta {
        PyTensorMeta::from_inner(self.meta.clone())
    }

    /// Get tensor name (if set)
    #[getter]
    fn name(&self) -> Option<&str> {
        self.meta.name.as_deref()
    }

    /// Get tensor shape
    #[getter]
    fn shape(&self) -> Vec<usize> {
        self.meta.shape.clone()
    }

    /// Get tensor data type
    #[getter]
    fn dtype(&self) -> PyDType {
        PyDType::from_inner(self.meta.dtype)
    }

    /// Get number of elements
    #[getter]
    fn num_elements(&self) -> usize {
        self.meta.numel()
    }

    /// Get size in bytes
    #[getter]
    fn size_bytes(&self) -> usize {
        self.data.len()
    }

    /// Get number of dimensions
    #[getter]
    fn ndim(&self) -> usize {
        self.meta.shape.len()
    }

    /// Get raw bytes (as Python bytes)
    fn tobytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyBytes>> {
        Ok(pyo3::types::PyBytes::new_bound(py, &self.data))
    }

    fn __repr__(&self) -> String {
        let name_str = self.meta.name.as_deref().unwrap_or("unnamed");
        format!(
            "Tensor(name='{}', shape={:?}, dtype={})",
            name_str,
            self.meta.shape,
            PyDType::from_inner(self.meta.dtype).name()
        )
    }

    fn __len__(&self) -> usize {
        self.meta.numel()
    }
}

impl PyTensor {
    pub fn meta_inner(&self) -> &TensorMeta {
        &self.meta
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn from_parts(meta: TensorMeta, data: Vec<u8>) -> Self {
        Self {
            meta,
            data: Arc::new(data),
        }
    }
}

#[cfg(all(test, feature = "python-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_meta_creation() {
        let meta = PyTensorMeta::new(
            vec![2, 3, 4],
            PyDType::from_inner(DType::Float32),
            Some("test".to_string()),
        );

        assert_eq!(meta.name(), Some("test"));
        assert_eq!(meta.shape(), vec![2, 3, 4]);
        assert_eq!(meta.num_elements(), 24);
        assert_eq!(meta.size_bytes(), 96); // 24 * 4 bytes
        assert_eq!(meta.ndim(), 3);
    }

    #[test]
    fn test_tensor_zeros() {
        let tensor = PyTensor::zeros(
            vec![2, 3],
            PyDType::from_inner(DType::Float32),
            Some("zeros".to_string()),
        );

        assert_eq!(tensor.name(), Some("zeros"));
        assert_eq!(tensor.shape(), vec![2, 3]);
        assert_eq!(tensor.num_elements(), 6);
        assert_eq!(tensor.size_bytes(), 24);
        assert!(tensor.data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_tensor_from_bytes() {
        let data = vec![0u8; 16]; // 4 float32 values
        let tensor = PyTensor::from_bytes(
            data,
            vec![2, 2],
            PyDType::from_inner(DType::Float32),
            None,
        ).unwrap();

        assert_eq!(tensor.shape(), vec![2, 2]);
        assert_eq!(tensor.size_bytes(), 16);
    }

    #[test]
    fn test_tensor_from_bytes_size_mismatch() {
        let data = vec![0u8; 10]; // Wrong size
        let result = PyTensor::from_bytes(
            data,
            vec![2, 2],
            PyDType::from_inner(DType::Float32),
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_tensor_meta_repr() {
        let meta = PyTensorMeta::new(
            vec![768],
            PyDType::from_inner(DType::Float16),
            Some("embedding".to_string()),
        );

        let repr = meta.__repr__();
        assert!(repr.contains("embedding"));
        assert!(repr.contains("768"));
        assert!(repr.contains("float16"));
    }
}
