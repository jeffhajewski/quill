//! Data types for tensor elements.
//!
//! Supports standard ML data types including half-precision floats (f16, bf16).

use half::{bf16, f16};

/// Data type for tensor elements.
///
/// Supports common ML data types including half-precision floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DType {
    /// 32-bit floating point
    Float32 = 1,
    /// 16-bit IEEE 754 floating point
    Float16 = 2,
    /// 16-bit brain floating point (bfloat16)
    BFloat16 = 3,
    /// 64-bit floating point
    Float64 = 4,
    /// 8-bit signed integer
    Int8 = 5,
    /// 32-bit signed integer
    Int32 = 6,
    /// 64-bit signed integer
    Int64 = 7,
    /// 8-bit unsigned integer
    UInt8 = 8,
    /// Boolean (1 byte per element)
    Bool = 9,
}

impl DType {
    /// Returns the size in bytes of a single element of this data type.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use quill_tensor::DType;
    ///
    /// assert_eq!(DType::Float32.element_size(), 4);
    /// assert_eq!(DType::Float16.element_size(), 2);
    /// assert_eq!(DType::Int8.element_size(), 1);
    /// ```
    #[inline]
    pub const fn element_size(&self) -> usize {
        match self {
            DType::Float64 | DType::Int64 => 8,
            DType::Float32 | DType::Int32 => 4,
            DType::Float16 | DType::BFloat16 => 2,
            DType::Int8 | DType::UInt8 | DType::Bool => 1,
        }
    }

    /// Returns a human-readable name for this data type.
    #[inline]
    pub const fn name(&self) -> &'static str {
        match self {
            DType::Float32 => "float32",
            DType::Float16 => "float16",
            DType::BFloat16 => "bfloat16",
            DType::Float64 => "float64",
            DType::Int8 => "int8",
            DType::Int32 => "int32",
            DType::Int64 => "int64",
            DType::UInt8 => "uint8",
            DType::Bool => "bool",
        }
    }

    /// Returns whether this is a floating-point type.
    #[inline]
    pub const fn is_floating_point(&self) -> bool {
        matches!(
            self,
            DType::Float32 | DType::Float16 | DType::BFloat16 | DType::Float64
        )
    }

    /// Returns whether this is a signed integer type.
    #[inline]
    pub const fn is_signed(&self) -> bool {
        matches!(
            self,
            DType::Int8
                | DType::Int32
                | DType::Int64
                | DType::Float32
                | DType::Float16
                | DType::BFloat16
                | DType::Float64
        )
    }

    /// Converts from protobuf DType enum value.
    pub fn from_proto(value: i32) -> Option<Self> {
        match value {
            1 => Some(DType::Float32),
            2 => Some(DType::Float16),
            3 => Some(DType::BFloat16),
            4 => Some(DType::Float64),
            5 => Some(DType::Int8),
            6 => Some(DType::Int32),
            7 => Some(DType::Int64),
            8 => Some(DType::UInt8),
            9 => Some(DType::Bool),
            _ => None,
        }
    }

    /// Converts to protobuf DType enum value.
    #[inline]
    pub const fn to_proto(&self) -> i32 {
        *self as i32
    }
}

impl std::fmt::Display for DType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl TryFrom<u8> for DType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(DType::Float32),
            2 => Ok(DType::Float16),
            3 => Ok(DType::BFloat16),
            4 => Ok(DType::Float64),
            5 => Ok(DType::Int8),
            6 => Ok(DType::Int32),
            7 => Ok(DType::Int64),
            8 => Ok(DType::UInt8),
            9 => Ok(DType::Bool),
            _ => Err(()),
        }
    }
}

/// Trait for types that can be used as tensor elements.
pub trait Element: Copy + Send + Sync + 'static {
    /// The DType corresponding to this element type.
    const DTYPE: DType;

    /// Convert a slice of bytes to a slice of this element type.
    ///
    /// # Safety
    /// The bytes must be properly aligned for this type and have a length
    /// that is a multiple of the element size.
    unsafe fn from_bytes(bytes: &[u8]) -> &[Self];

    /// Convert a mutable slice of bytes to a mutable slice of this element type.
    ///
    /// # Safety
    /// The bytes must be properly aligned for this type and have a length
    /// that is a multiple of the element size.
    unsafe fn from_bytes_mut(bytes: &mut [u8]) -> &mut [Self];

    /// Convert a slice of this element type to a slice of bytes.
    fn as_bytes(slice: &[Self]) -> &[u8];
}

macro_rules! impl_element {
    ($ty:ty, $dtype:expr) => {
        impl Element for $ty {
            const DTYPE: DType = $dtype;

            unsafe fn from_bytes(bytes: &[u8]) -> &[Self] {
                std::slice::from_raw_parts(
                    bytes.as_ptr() as *const Self,
                    bytes.len() / std::mem::size_of::<Self>(),
                )
            }

            unsafe fn from_bytes_mut(bytes: &mut [u8]) -> &mut [Self] {
                std::slice::from_raw_parts_mut(
                    bytes.as_mut_ptr() as *mut Self,
                    bytes.len() / std::mem::size_of::<Self>(),
                )
            }

            fn as_bytes(slice: &[Self]) -> &[u8] {
                unsafe {
                    std::slice::from_raw_parts(
                        slice.as_ptr() as *const u8,
                        slice.len() * std::mem::size_of::<Self>(),
                    )
                }
            }
        }
    };
}

impl_element!(f32, DType::Float32);
impl_element!(f64, DType::Float64);
impl_element!(f16, DType::Float16);
impl_element!(bf16, DType::BFloat16);
impl_element!(i8, DType::Int8);
impl_element!(i32, DType::Int32);
impl_element!(i64, DType::Int64);
impl_element!(u8, DType::UInt8);
impl_element!(bool, DType::Bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_element_sizes() {
        assert_eq!(DType::Float32.element_size(), 4);
        assert_eq!(DType::Float16.element_size(), 2);
        assert_eq!(DType::BFloat16.element_size(), 2);
        assert_eq!(DType::Float64.element_size(), 8);
        assert_eq!(DType::Int8.element_size(), 1);
        assert_eq!(DType::Int32.element_size(), 4);
        assert_eq!(DType::Int64.element_size(), 8);
        assert_eq!(DType::UInt8.element_size(), 1);
        assert_eq!(DType::Bool.element_size(), 1);
    }

    #[test]
    fn test_dtype_names() {
        assert_eq!(DType::Float32.name(), "float32");
        assert_eq!(DType::Float16.name(), "float16");
        assert_eq!(DType::BFloat16.name(), "bfloat16");
    }

    #[test]
    fn test_proto_conversion() {
        assert_eq!(DType::from_proto(1), Some(DType::Float32));
        assert_eq!(DType::from_proto(2), Some(DType::Float16));
        assert_eq!(DType::from_proto(100), None);

        assert_eq!(DType::Float32.to_proto(), 1);
        assert_eq!(DType::Float16.to_proto(), 2);
    }

    #[test]
    fn test_is_floating_point() {
        assert!(DType::Float32.is_floating_point());
        assert!(DType::Float16.is_floating_point());
        assert!(DType::BFloat16.is_floating_point());
        assert!(DType::Float64.is_floating_point());
        assert!(!DType::Int32.is_floating_point());
        assert!(!DType::Bool.is_floating_point());
    }

    #[test]
    fn test_element_trait() {
        assert_eq!(f32::DTYPE, DType::Float32);
        assert_eq!(f16::DTYPE, DType::Float16);
        assert_eq!(bf16::DTYPE, DType::BFloat16);

        let floats = [1.0f32, 2.0, 3.0, 4.0];
        let bytes = f32::as_bytes(&floats);
        assert_eq!(bytes.len(), 16);

        let recovered = unsafe { f32::from_bytes(bytes) };
        assert_eq!(recovered, &floats);
    }
}
