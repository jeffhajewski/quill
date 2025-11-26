//! Python bindings for DType (data type enumeration).

use pyo3::prelude::*;
use quill_tensor::DType;

/// Data type for tensor elements.
///
/// Supported types:
/// - `Float32`: 32-bit floating point (f32)
/// - `Float64`: 64-bit floating point (f64)
/// - `Float16`: 16-bit IEEE floating point (f16)
/// - `BFloat16`: 16-bit brain floating point (bf16)
/// - `Int8`: 8-bit signed integer
/// - `Int32`: 32-bit signed integer
/// - `Int64`: 64-bit signed integer
/// - `UInt8`: 8-bit unsigned integer
/// - `Bool`: Boolean
#[pyclass(name = "DType")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PyDType {
    inner: DType,
}

#[pymethods]
impl PyDType {
    /// Create Float32 dtype
    #[staticmethod]
    fn float32() -> Self {
        Self { inner: DType::Float32 }
    }

    /// Create Float64 dtype
    #[staticmethod]
    fn float64() -> Self {
        Self { inner: DType::Float64 }
    }

    /// Create Float16 dtype (IEEE half-precision)
    #[staticmethod]
    fn float16() -> Self {
        Self { inner: DType::Float16 }
    }

    /// Create BFloat16 dtype (brain float)
    #[staticmethod]
    fn bfloat16() -> Self {
        Self { inner: DType::BFloat16 }
    }

    /// Create Int8 dtype
    #[staticmethod]
    fn int8() -> Self {
        Self { inner: DType::Int8 }
    }

    /// Create Int32 dtype
    #[staticmethod]
    fn int32() -> Self {
        Self { inner: DType::Int32 }
    }

    /// Create Int64 dtype
    #[staticmethod]
    fn int64() -> Self {
        Self { inner: DType::Int64 }
    }

    /// Create UInt8 dtype
    #[staticmethod]
    fn uint8() -> Self {
        Self { inner: DType::UInt8 }
    }

    /// Create Bool dtype
    #[staticmethod]
    fn bool_() -> Self {
        Self { inner: DType::Bool }
    }

    /// Get the size of one element in bytes
    #[getter]
    fn element_size(&self) -> usize {
        self.inner.element_size()
    }

    /// Get the name of this dtype
    #[getter]
    pub fn name(&self) -> &'static str {
        match self.inner {
            DType::Float32 => "float32",
            DType::Float64 => "float64",
            DType::Float16 => "float16",
            DType::BFloat16 => "bfloat16",
            DType::Int8 => "int8",
            DType::Int32 => "int32",
            DType::Int64 => "int64",
            DType::UInt8 => "uint8",
            DType::Bool => "bool",
        }
    }

    /// Check if this dtype is a floating point type
    fn is_float(&self) -> bool {
        matches!(
            self.inner,
            DType::Float32 | DType::Float64 | DType::Float16 | DType::BFloat16
        )
    }

    /// Check if this dtype is an integer type
    fn is_integer(&self) -> bool {
        matches!(
            self.inner,
            DType::Int8 | DType::Int32 | DType::Int64 | DType::UInt8
        )
    }

    /// Check if this dtype is a signed type
    fn is_signed(&self) -> bool {
        matches!(
            self.inner,
            DType::Float32 | DType::Float64 | DType::Float16 | DType::BFloat16 |
            DType::Int8 | DType::Int32 | DType::Int64
        )
    }

    fn __repr__(&self) -> String {
        format!("DType.{}", self.name())
    }

    fn __str__(&self) -> &'static str {
        self.name()
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::mem::discriminant(&self.inner).hash(&mut hasher);
        hasher.finish()
    }
}

impl PyDType {
    pub fn inner(&self) -> DType {
        self.inner
    }

    pub fn from_inner(inner: DType) -> Self {
        Self { inner }
    }
}

impl From<DType> for PyDType {
    fn from(dtype: DType) -> Self {
        Self { inner: dtype }
    }
}

impl From<PyDType> for DType {
    fn from(py_dtype: PyDType) -> Self {
        py_dtype.inner
    }
}

#[cfg(all(test, feature = "python-tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_dtype_creation() {
        let f32 = PyDType::from_inner(DType::Float32);
        assert_eq!(f32.name(), "float32");
        assert_eq!(f32.element_size(), 4);
        assert!(f32.is_float());
        assert!(!f32.is_integer());
    }

    #[test]
    fn test_dtype_integer() {
        let i32 = PyDType::from_inner(DType::Int32);
        assert_eq!(i32.name(), "int32");
        assert_eq!(i32.element_size(), 4);
        assert!(!i32.is_float());
        assert!(i32.is_integer());
        assert!(i32.is_signed());
    }

    #[test]
    fn test_dtype_unsigned() {
        let u8 = PyDType::from_inner(DType::UInt8);
        assert_eq!(u8.name(), "uint8");
        assert_eq!(u8.element_size(), 1);
        assert!(!u8.is_signed());
    }

    #[test]
    fn test_dtype_equality() {
        let f32_a = PyDType::from_inner(DType::Float32);
        let f32_b = PyDType::from_inner(DType::Float32);
        let f64 = PyDType::from_inner(DType::Float64);

        assert_eq!(f32_a, f32_b);
        assert_ne!(f32_a, f64);
    }

    #[test]
    fn test_dtype_half_precision() {
        let f16 = PyDType::from_inner(DType::Float16);
        let bf16 = PyDType::from_inner(DType::BFloat16);

        assert_eq!(f16.element_size(), 2);
        assert_eq!(bf16.element_size(), 2);
        assert!(f16.is_float());
        assert!(bf16.is_float());
    }
}
