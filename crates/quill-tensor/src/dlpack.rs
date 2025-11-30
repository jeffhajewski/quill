//! DLPack protocol implementation for tensor interchange.
//!
//! DLPack is a standard for sharing tensors between ML frameworks like
//! PyTorch, JAX, TensorFlow, and NumPy. This module provides types and
//! functions for converting Quill tensors to/from DLPack format.
//!
//! # Overview
//!
//! DLPack defines a minimal C ABI for tensor interchange:
//! - `DLDevice`: Device type (CPU, CUDA) and device ID
//! - `DLDataType`: Data type code, bits, and lanes
//! - `DLTensor`: Non-owning tensor descriptor
//! - `DLManagedTensor`: Owning wrapper with deleter callback
//!
//! # Example
//!
//! ```rust,ignore
//! use quill_tensor::{Tensor, TensorMeta, DType};
//! use quill_tensor::dlpack::DLManagedTensor;
//!
//! // Export a tensor to DLPack
//! let meta = TensorMeta::new(vec![2, 3], DType::Float32);
//! let tensor = Tensor::zeros(meta);
//! let capsule = tensor.to_dlpack();
//!
//! // Import from DLPack
//! let imported = Tensor::from_dlpack(capsule)?;
//! ```
//!
//! # Safety
//!
//! DLPack uses raw pointers for C ABI compatibility. The implementation
//! ensures memory safety through proper ownership tracking and the
//! `DLManagedTensor.deleter` callback.

use std::ffi::c_void;
use std::ptr;

use bytes::Bytes;

use crate::buffer::{GpuError, GpuResult, TensorBuffer};
use crate::dtype::DType;
use crate::tensor::{Device, TensorMeta};
use crate::Tensor;

/// DLPack version constants.
pub const DLPACK_MAJOR_VERSION: u32 = 1;
pub const DLPACK_MINOR_VERSION: u32 = 0;

/// Device type codes as defined by DLPack.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DLDeviceType {
    /// CPU device
    Cpu = 1,
    /// CUDA GPU device
    Cuda = 2,
    /// CUDA managed/unified memory
    CudaManaged = 13,
    /// CUDA pinned memory
    CudaHost = 3,
    /// OpenCL device
    OpenCL = 4,
    /// Vulkan device
    Vulkan = 7,
    /// Metal device
    Metal = 8,
    /// VPI device
    Vpi = 9,
    /// ROCm device
    Rocm = 10,
    /// ROCm host memory
    RocmHost = 11,
    /// WebGPU device
    WebGpu = 15,
    /// Hexagon DSP
    Hexagon = 16,
}

impl From<Device> for DLDeviceType {
    fn from(device: Device) -> Self {
        match device {
            Device::Cpu => DLDeviceType::Cpu,
            Device::Cuda => DLDeviceType::Cuda,
        }
    }
}

impl From<DLDeviceType> for Device {
    fn from(dt: DLDeviceType) -> Self {
        match dt {
            DLDeviceType::Cpu | DLDeviceType::CudaHost => Device::Cpu,
            DLDeviceType::Cuda | DLDeviceType::CudaManaged => Device::Cuda,
            // Metal, ROCm, and other devices default to CPU
            _ => Device::Cpu,
        }
    }
}

/// DLPack device descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DLDevice {
    /// Device type (CPU, CUDA, etc.)
    pub device_type: DLDeviceType,
    /// Device ID (0 for first device)
    pub device_id: i32,
}

impl DLDevice {
    /// Creates a CPU device descriptor.
    pub fn cpu() -> Self {
        Self {
            device_type: DLDeviceType::Cpu,
            device_id: 0,
        }
    }

    /// Creates a CUDA device descriptor.
    pub fn cuda(device_id: i32) -> Self {
        Self {
            device_type: DLDeviceType::Cuda,
            device_id,
        }
    }

    /// Creates a device descriptor from Quill Device type.
    pub fn from_device(device: Device, device_id: usize) -> Self {
        Self {
            device_type: device.into(),
            device_id: device_id as i32,
        }
    }
}

/// DLPack data type codes.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DLDataTypeCode {
    /// Signed integer
    Int = 0,
    /// Unsigned integer
    UInt = 1,
    /// IEEE floating point
    Float = 2,
    /// Opaque pointer (void*)
    OpaqueHandle = 3,
    /// Bfloat16
    Bfloat = 4,
    /// Complex floating point
    Complex = 5,
    /// Boolean
    Bool = 6,
}

/// DLPack data type descriptor.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DLDataType {
    /// Type code (Int, Float, etc.)
    pub code: DLDataTypeCode,
    /// Number of bits per element
    pub bits: u8,
    /// Number of lanes (for vector types, usually 1)
    pub lanes: u16,
}

impl DLDataType {
    /// Creates a data type descriptor from Quill DType.
    pub fn from_dtype(dtype: DType) -> Self {
        match dtype {
            DType::Float32 => Self {
                code: DLDataTypeCode::Float,
                bits: 32,
                lanes: 1,
            },
            DType::Float64 => Self {
                code: DLDataTypeCode::Float,
                bits: 64,
                lanes: 1,
            },
            DType::Float16 => Self {
                code: DLDataTypeCode::Float,
                bits: 16,
                lanes: 1,
            },
            DType::BFloat16 => Self {
                code: DLDataTypeCode::Bfloat,
                bits: 16,
                lanes: 1,
            },
            DType::Int8 => Self {
                code: DLDataTypeCode::Int,
                bits: 8,
                lanes: 1,
            },
            DType::Int32 => Self {
                code: DLDataTypeCode::Int,
                bits: 32,
                lanes: 1,
            },
            DType::Int64 => Self {
                code: DLDataTypeCode::Int,
                bits: 64,
                lanes: 1,
            },
            DType::UInt8 => Self {
                code: DLDataTypeCode::UInt,
                bits: 8,
                lanes: 1,
            },
            DType::Bool => Self {
                code: DLDataTypeCode::Bool,
                bits: 8,
                lanes: 1,
            },
        }
    }

    /// Converts to Quill DType.
    pub fn to_dtype(&self) -> Result<DType, DLPackError> {
        match (self.code, self.bits) {
            (DLDataTypeCode::Float, 32) => Ok(DType::Float32),
            (DLDataTypeCode::Float, 64) => Ok(DType::Float64),
            (DLDataTypeCode::Float, 16) => Ok(DType::Float16),
            (DLDataTypeCode::Bfloat, 16) => Ok(DType::BFloat16),
            (DLDataTypeCode::Int, 8) => Ok(DType::Int8),
            (DLDataTypeCode::Int, 32) => Ok(DType::Int32),
            (DLDataTypeCode::Int, 64) => Ok(DType::Int64),
            (DLDataTypeCode::UInt, 8) => Ok(DType::UInt8),
            (DLDataTypeCode::Bool, 8) | (DLDataTypeCode::Bool, 1) => Ok(DType::Bool),
            _ => Err(DLPackError::UnsupportedDataType {
                code: self.code as u8,
                bits: self.bits,
            }),
        }
    }
}

/// DLPack tensor descriptor (non-owning).
///
/// This is a non-owning view of tensor data. The actual data is managed
/// by the owner (e.g., `DLManagedTensor`).
#[repr(C)]
#[derive(Debug)]
pub struct DLTensor {
    /// Pointer to the data buffer.
    ///
    /// For GPU tensors, this is a device pointer.
    pub data: *mut c_void,

    /// Device where the data resides.
    pub device: DLDevice,

    /// Number of dimensions.
    pub ndim: i32,

    /// Data type descriptor.
    pub dtype: DLDataType,

    /// Shape array (length = ndim).
    ///
    /// Must remain valid for the lifetime of the tensor.
    pub shape: *mut i64,

    /// Strides array in number of elements (length = ndim).
    ///
    /// Can be NULL for contiguous tensors.
    pub strides: *mut i64,

    /// Byte offset from `data` pointer to the start of tensor data.
    pub byte_offset: u64,
}

/// Type of the deleter function for DLManagedTensor.
pub type DLManagedTensorDeleter = unsafe extern "C" fn(*mut DLManagedTensor);

/// DLPack managed tensor (owning).
///
/// This struct owns tensor data and metadata, providing a deleter callback
/// for proper cleanup.
#[repr(C)]
pub struct DLManagedTensor {
    /// The tensor descriptor.
    pub dl_tensor: DLTensor,

    /// Context for the deleter (usually the owning object).
    pub manager_ctx: *mut c_void,

    /// Deleter function called when the tensor is released.
    pub deleter: Option<DLManagedTensorDeleter>,
}

/// DLPack versioned managed tensor (DLPack 1.0).
#[repr(C)]
pub struct DLManagedTensorVersioned {
    /// Major version (should be 1 for DLPack 1.0).
    pub version_major: u32,
    /// Minor version (should be 0 for DLPack 1.0).
    pub version_minor: u32,
    /// Flags (reserved, should be 0).
    pub flags: u64,
    /// The managed tensor.
    pub manager: DLManagedTensor,
}

/// Errors that can occur during DLPack operations.
#[derive(Debug, thiserror::Error)]
pub enum DLPackError {
    /// Unsupported data type.
    #[error("Unsupported DLPack data type: code={code}, bits={bits}")]
    UnsupportedDataType { code: u8, bits: u8 },

    /// Unsupported device type.
    #[error("Unsupported DLPack device type: {0}")]
    UnsupportedDevice(i32),

    /// Invalid tensor (null data pointer).
    #[error("Invalid DLPack tensor: null data pointer")]
    NullData,

    /// Invalid shape.
    #[error("Invalid DLPack tensor: null shape pointer")]
    NullShape,

    /// GPU operation error.
    #[error("GPU error: {0}")]
    Gpu(#[from] GpuError),

    /// Non-contiguous tensor (strides not supported yet).
    #[error("Non-contiguous tensors not supported")]
    NonContiguous,
}

/// Internal context for managing DLPack tensor lifetime.
struct DLPackContext {
    /// Shape array (kept alive for DLTensor.shape)
    _shape: Vec<i64>,
    /// Strides array (kept alive for DLTensor.strides)
    _strides: Option<Vec<i64>>,
    /// Data buffer
    buffer: TensorBuffer,
}

/// A DLPack capsule that can be exchanged with other frameworks.
///
/// This struct owns the DLManagedTensor and ensures proper cleanup.
pub struct DLPackCapsule {
    /// Raw pointer to the managed tensor.
    ptr: *mut DLManagedTensor,
    /// Whether we own the tensor (for cleanup).
    owned: bool,
}

impl DLPackCapsule {
    /// Creates a new DLPack capsule from a Quill tensor.
    ///
    /// The returned capsule owns the tensor data and will clean it up
    /// when dropped (unless ownership is transferred via `into_raw`).
    pub fn from_tensor(tensor: &Tensor) -> GpuResult<Self> {
        // Convert tensor data to TensorBuffer
        let buffer = TensorBuffer::cpu(tensor.data.clone());

        // Build context
        let shape: Vec<i64> = tensor.meta.shape.iter().map(|&x| x as i64).collect();
        let strides: Option<Vec<i64>> = tensor.meta.strides.as_ref().map(|s| {
            s.iter().map(|&x| x as i64).collect()
        });

        let ctx = Box::new(DLPackContext {
            _shape: shape.clone(),
            _strides: strides.clone(),
            buffer,
        });

        // Create shape/strides pointers from boxed context
        let shape_ptr = ctx._shape.as_ptr() as *mut i64;
        let strides_ptr = ctx._strides.as_ref().map(|s| s.as_ptr() as *mut i64).unwrap_or(ptr::null_mut());

        // Get data pointer
        let data_ptr = match &ctx.buffer {
            TensorBuffer::Cpu(bytes) => bytes.as_ptr() as *mut c_void,
            #[cfg(feature = "cuda")]
            TensorBuffer::Cuda(cuda_buf) => cuda_buf.as_device_ptr() as *mut c_void,
        };

        // Create DLTensor
        let dl_tensor = DLTensor {
            data: data_ptr,
            device: DLDevice::from_device(tensor.meta.device, 0),
            ndim: tensor.meta.shape.len() as i32,
            dtype: DLDataType::from_dtype(tensor.meta.dtype),
            shape: shape_ptr,
            strides: strides_ptr,
            byte_offset: 0,
        };

        // Create DLManagedTensor
        let managed = Box::new(DLManagedTensor {
            dl_tensor,
            manager_ctx: Box::into_raw(ctx) as *mut c_void,
            deleter: Some(dlpack_deleter),
        });

        Ok(Self {
            ptr: Box::into_raw(managed),
            owned: true,
        })
    }

    /// Converts a DLPack capsule back to a Quill tensor.
    ///
    /// This consumes the capsule and takes ownership of the data.
    pub fn to_tensor(self) -> Result<Tensor, DLPackError> {
        if self.ptr.is_null() {
            return Err(DLPackError::NullData);
        }

        let managed = unsafe { &*self.ptr };
        let dl_tensor = &managed.dl_tensor;

        // Validate
        if dl_tensor.data.is_null() {
            return Err(DLPackError::NullData);
        }
        if dl_tensor.shape.is_null() {
            return Err(DLPackError::NullShape);
        }

        // Check for non-contiguous tensors
        if !dl_tensor.strides.is_null() {
            // TODO: Support non-contiguous tensors
            return Err(DLPackError::NonContiguous);
        }

        // Convert dtype
        let dtype = dl_tensor.dtype.to_dtype()?;

        // Convert shape
        let shape: Vec<usize> = unsafe {
            std::slice::from_raw_parts(dl_tensor.shape, dl_tensor.ndim as usize)
                .iter()
                .map(|&x| x as usize)
                .collect()
        };

        // Convert device
        let device = Device::from(dl_tensor.device.device_type);

        // Calculate size
        let meta = TensorMeta::new(shape.clone(), dtype).with_device(device);
        let byte_size = meta.byte_size();

        // Copy data
        let data = unsafe {
            let data_ptr = (dl_tensor.data as *const u8).add(dl_tensor.byte_offset as usize);
            std::slice::from_raw_parts(data_ptr, byte_size).to_vec()
        };

        // Don't drop self normally - we consumed the data
        std::mem::forget(self);

        Ok(Tensor::new(meta, Bytes::from(data)))
    }

    /// Returns the raw pointer to the DLManagedTensor.
    ///
    /// The capsule retains ownership unless you call `into_raw`.
    pub fn as_ptr(&self) -> *mut DLManagedTensor {
        self.ptr
    }

    /// Consumes the capsule and returns the raw pointer.
    ///
    /// The caller is responsible for calling the deleter or managing memory.
    pub fn into_raw(mut self) -> *mut DLManagedTensor {
        self.owned = false;
        self.ptr
    }

    /// Creates a capsule from a raw pointer (takes ownership).
    ///
    /// # Safety
    ///
    /// The pointer must be a valid DLManagedTensor created by DLPack.
    pub unsafe fn from_raw(ptr: *mut DLManagedTensor) -> Self {
        Self { ptr, owned: true }
    }
}

impl Drop for DLPackCapsule {
    fn drop(&mut self) {
        if self.owned && !self.ptr.is_null() {
            unsafe {
                let managed = &*self.ptr;
                if let Some(deleter) = managed.deleter {
                    deleter(self.ptr);
                }
            }
        }
    }
}

/// Deleter function for DLManagedTensor created by Quill.
unsafe extern "C" fn dlpack_deleter(tensor: *mut DLManagedTensor) {
    if tensor.is_null() {
        return;
    }

    let managed = Box::from_raw(tensor);

    // Clean up the context
    if !managed.manager_ctx.is_null() {
        let _ = Box::from_raw(managed.manager_ctx as *mut DLPackContext);
    }
}

// CUDA Array Interface support
/// CUDA Array Interface descriptor.
///
/// This is the Python `__cuda_array_interface__` protocol for GPU array interop.
#[derive(Debug, Clone)]
pub struct CudaArrayInterface {
    /// Shape of the array.
    pub shape: Vec<usize>,
    /// Strides in bytes (None for C-contiguous).
    pub strides: Option<Vec<usize>>,
    /// Type string (numpy format, e.g., "<f4" for little-endian float32).
    pub typestr: String,
    /// Data pointer and read-only flag tuple: (ptr, readonly).
    pub data: (usize, bool),
    /// Version (should be 3).
    pub version: u32,
    /// Optional stream pointer for synchronization.
    pub stream: Option<usize>,
}

impl CudaArrayInterface {
    /// Creates a CUDA Array Interface descriptor from a TensorBuffer.
    #[cfg(feature = "cuda")]
    pub fn from_buffer(buffer: &TensorBuffer, meta: &TensorMeta) -> Option<Self> {
        match buffer {
            TensorBuffer::Cuda(cuda_buf) => {
                let typestr = dtype_to_typestr(meta.dtype);
                Some(Self {
                    shape: meta.shape.clone(),
                    strides: meta.strides.as_ref().map(|s| {
                        s.iter().map(|&x| x * meta.dtype.element_size()).collect()
                    }),
                    typestr,
                    data: (cuda_buf.as_device_ptr() as usize, false),
                    version: 3,
                    stream: None,
                })
            }
            TensorBuffer::Cpu(_) => None,
        }
    }

    #[cfg(not(feature = "cuda"))]
    pub fn from_buffer(_buffer: &TensorBuffer, _meta: &TensorMeta) -> Option<Self> {
        None
    }
}

/// Converts a DType to a numpy typestring.
pub fn dtype_to_typestr(dtype: DType) -> String {
    // Little-endian format codes
    match dtype {
        DType::Float32 => "<f4".to_string(),
        DType::Float64 => "<f8".to_string(),
        DType::Float16 => "<f2".to_string(),
        DType::BFloat16 => "<V2".to_string(), // BFloat16 as void/raw
        DType::Int8 => "|i1".to_string(),
        DType::Int32 => "<i4".to_string(),
        DType::Int64 => "<i8".to_string(),
        DType::UInt8 => "|u1".to_string(),
        DType::Bool => "|b1".to_string(),
    }
}

/// Converts a numpy typestring to DType.
pub fn typestr_to_dtype(typestr: &str) -> Result<DType, DLPackError> {
    // Handle both big and little endian
    let normalized = typestr.trim_start_matches(['<', '>', '|', '=']);
    match normalized {
        "f4" | "float32" => Ok(DType::Float32),
        "f8" | "float64" => Ok(DType::Float64),
        "f2" | "float16" => Ok(DType::Float16),
        "i1" | "int8" => Ok(DType::Int8),
        "i4" | "int32" => Ok(DType::Int32),
        "i8" | "int64" => Ok(DType::Int64),
        "u1" | "uint8" => Ok(DType::UInt8),
        "b1" | "bool" => Ok(DType::Bool),
        _ => Err(DLPackError::UnsupportedDataType {
            code: 0,
            bits: 0,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dl_device_cpu() {
        let dev = DLDevice::cpu();
        assert_eq!(dev.device_type, DLDeviceType::Cpu);
        assert_eq!(dev.device_id, 0);
    }

    #[test]
    fn test_dl_device_cuda() {
        let dev = DLDevice::cuda(1);
        assert_eq!(dev.device_type, DLDeviceType::Cuda);
        assert_eq!(dev.device_id, 1);
    }

    #[test]
    fn test_dl_dtype_conversion() {
        // Float32
        let dl_f32 = DLDataType::from_dtype(DType::Float32);
        assert_eq!(dl_f32.code, DLDataTypeCode::Float);
        assert_eq!(dl_f32.bits, 32);
        assert_eq!(dl_f32.to_dtype().unwrap(), DType::Float32);

        // Int64
        let dl_i64 = DLDataType::from_dtype(DType::Int64);
        assert_eq!(dl_i64.code, DLDataTypeCode::Int);
        assert_eq!(dl_i64.bits, 64);
        assert_eq!(dl_i64.to_dtype().unwrap(), DType::Int64);

        // BFloat16
        let dl_bf16 = DLDataType::from_dtype(DType::BFloat16);
        assert_eq!(dl_bf16.code, DLDataTypeCode::Bfloat);
        assert_eq!(dl_bf16.bits, 16);
        assert_eq!(dl_bf16.to_dtype().unwrap(), DType::BFloat16);
    }

    #[test]
    fn test_dlpack_roundtrip() {
        let meta = TensorMeta::new(vec![2, 3], DType::Float32);
        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let tensor = Tensor::from_f32(&meta, &data);

        // Export to DLPack
        let capsule = DLPackCapsule::from_tensor(&tensor).unwrap();

        // Import from DLPack
        let imported = capsule.to_tensor().unwrap();

        assert_eq!(imported.meta.shape, vec![2, 3]);
        assert_eq!(imported.meta.dtype, DType::Float32);
        assert_eq!(imported.as_f32(), &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_dlpack_i64_tensor() {
        let meta = TensorMeta::new(vec![4], DType::Int64);
        let data = vec![100i64, 200, 300, 400];
        let tensor = Tensor::from_i64(&meta, &data);

        let capsule = DLPackCapsule::from_tensor(&tensor).unwrap();
        let imported = capsule.to_tensor().unwrap();

        assert_eq!(imported.meta.dtype, DType::Int64);
        assert_eq!(imported.as_i64(), &[100, 200, 300, 400]);
    }

    #[test]
    fn test_typestr_conversion() {
        assert_eq!(typestr_to_dtype("<f4").unwrap(), DType::Float32);
        assert_eq!(typestr_to_dtype(">f8").unwrap(), DType::Float64);
        assert_eq!(typestr_to_dtype("|i1").unwrap(), DType::Int8);
        assert_eq!(typestr_to_dtype("=i4").unwrap(), DType::Int32);
    }

    #[test]
    fn test_dtype_to_typestr() {
        assert_eq!(dtype_to_typestr(DType::Float32), "<f4");
        assert_eq!(dtype_to_typestr(DType::Int64), "<i8");
        assert_eq!(dtype_to_typestr(DType::UInt8), "|u1");
    }

    #[test]
    fn test_device_conversion() {
        assert_eq!(Device::from(DLDeviceType::Cpu), Device::Cpu);
        assert_eq!(Device::from(DLDeviceType::Cuda), Device::Cuda);
        // Metal and other unsupported devices default to CPU
        assert_eq!(Device::from(DLDeviceType::Metal), Device::Cpu);

        assert_eq!(DLDeviceType::from(Device::Cpu), DLDeviceType::Cpu);
        assert_eq!(DLDeviceType::from(Device::Cuda), DLDeviceType::Cuda);
    }
}
