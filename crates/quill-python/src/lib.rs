//! Python bindings for Quill RPC framework.
//!
//! This crate provides Python bindings via PyO3 for the Quill RPC framework,
//! enabling Python applications to use Quill's tensor streaming, RPC client,
//! and LLM inference capabilities.
//!
//! # Installation
//!
//! Build the Python wheel:
//! ```bash
//! cd crates/quill-python
//! maturin build --release
//! pip install target/wheels/quill-*.whl
//! ```
//!
//! # Usage
//!
//! ```python
//! import quill
//! import numpy as np
//!
//! # Create a tensor from numpy array
//! arr = np.array([[1.0, 2.0], [3.0, 4.0]], dtype=np.float32)
//! tensor = quill.Tensor.from_numpy(arr)
//!
//! # Get tensor metadata
//! print(f"Shape: {tensor.shape}")
//! print(f"DType: {tensor.dtype}")
//!
//! # Convert back to numpy
//! result = tensor.to_numpy()
//! ```

use pyo3::prelude::*;

mod client;
mod dtype;
mod gpu;
mod tensor;
mod token;

pub use client::PyQuillClient;
pub use dtype::PyDType;
pub use gpu::{PyDLPackCapsule, PyGpuStatus, PyTensorBuffer};
pub use tensor::{PyTensor, PyTensorMeta};
pub use token::{PyToken, PyTokenBatch};

/// Quill Python module
#[pymodule]
fn quill(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Data types
    m.add_class::<PyDType>()?;

    // Tensor types
    m.add_class::<PyTensor>()?;
    m.add_class::<PyTensorMeta>()?;

    // GPU support
    m.add_class::<PyGpuStatus>()?;
    m.add_class::<PyTensorBuffer>()?;
    m.add_class::<PyDLPackCapsule>()?;

    // Token types for LLM inference
    m.add_class::<PyToken>()?;
    m.add_class::<PyTokenBatch>()?;

    // Client
    m.add_class::<PyQuillClient>()?;

    // Version info
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
