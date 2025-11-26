# Python Bindings for Quill

Quill provides Python bindings via [PyO3](https://pyo3.rs/), enabling Python applications to leverage Quill's tensor streaming, RPC capabilities, and LLM inference support.

## Overview

The `quill` Python package provides:

- **Tensor operations** with NumPy integration
- **Data types** matching ML framework conventions
- **Token handling** for LLM inference streaming
- **RPC client** for calling Quill services

## Installation

### Prerequisites

- Python 3.8+
- NumPy >= 1.20

### Building from Source

Building requires [maturin](https://www.maturin.rs/) and Rust:

```bash
# Install maturin
pip install maturin

# Build and install in development mode
cd crates/quill-python
maturin develop

# Or build a wheel for distribution
maturin build --release
pip install target/wheels/quill-*.whl
```

## Tensor Operations

### Creating Tensors

```python
import quill
import numpy as np

# From NumPy array
arr = np.random.randn(2, 3, 4).astype(np.float32)
tensor = quill.Tensor.from_numpy(arr, name="input")

# Create zeros
zeros = quill.Tensor.zeros([1, 768], quill.DType.float32(), name="embedding")

# From raw bytes
data = bytes(24)  # 6 float32 zeros
tensor = quill.Tensor.from_bytes(data, [2, 3], quill.DType.float32())
```

### Tensor Properties

```python
tensor = quill.Tensor.from_numpy(arr, name="input")

# Shape and dimensions
print(tensor.shape)       # [2, 3, 4]
print(tensor.ndim)        # 3
print(tensor.num_elements)  # 24

# Data type
print(tensor.dtype)       # DType.float32
print(tensor.size_bytes)  # 96

# Name
print(tensor.name)        # "input"

# Metadata object
meta = tensor.meta
```

### Converting to NumPy

```python
# Get a copy of the data as NumPy array
result = tensor.to_numpy()

# Get raw bytes
raw = tensor.tobytes()
```

### TensorMeta

Tensor metadata without the data payload:

```python
# Create metadata
meta = quill.TensorMeta([1, 768], quill.DType.float32(), name="embedding")

print(meta.shape)        # [1, 768]
print(meta.dtype)        # DType.float32
print(meta.num_elements) # 768
print(meta.size_bytes)   # 3072
```

## Data Types

Quill supports the following data types, matching common ML framework conventions:

| DType | Description | Element Size |
|-------|-------------|--------------|
| `float32()` | 32-bit IEEE floating point | 4 bytes |
| `float64()` | 64-bit IEEE floating point | 8 bytes |
| `float16()` | 16-bit IEEE floating point | 2 bytes |
| `bfloat16()` | Brain floating point | 2 bytes |
| `int8()` | 8-bit signed integer | 1 byte |
| `int32()` | 32-bit signed integer | 4 bytes |
| `int64()` | 64-bit signed integer | 8 bytes |
| `uint8()` | 8-bit unsigned integer | 1 byte |
| `bool_()` | Boolean | 1 byte |

### Type Checking

```python
dtype = quill.DType.float32()

# Properties
print(dtype.element_size)  # 4
print(dtype.name)          # "float32"

# Type predicates
dtype.is_float()    # True
dtype.is_integer()  # False
dtype.is_signed()   # True

# Comparison
dtype == quill.DType.float32()  # True
```

### NumPy Compatibility

Quill automatically maps NumPy dtypes:

| NumPy | Quill |
|-------|-------|
| `np.float32` | `DType.float32()` |
| `np.float64` | `DType.float64()` |
| `np.float16` | `DType.float16()` |
| `np.int8` | `DType.int8()` |
| `np.int32` | `DType.int32()` |
| `np.int64` | `DType.int64()` |
| `np.uint8` | `DType.uint8()` |
| `np.bool_` | `DType.bool_()` |

Note: `bfloat16` is not directly supported by NumPy. Use `float16` or view as `uint16`.

## Token Handling

For LLM inference, Quill provides Token and TokenBatch types:

### Individual Tokens

```python
# Create a token
token = quill.Token(
    id=42,
    position=0,
    text="hello",
    logprob=-0.5,
    is_special=False
)

# Alternative constructor
token = quill.Token.with_text(42, "hello", 0, logprob=-0.5)

# Properties
print(token.id)         # 42
print(token.text)       # "hello"
print(token.logprob)    # -0.5
print(token.position)   # 0
print(token.is_special) # False

# Derived values
print(token.prob())     # ~0.607 (exp of logprob)
print(token.is_eos())   # False

# Serialization
as_dict = token.to_dict()
as_bytes = token.encode()
```

### Token Batches

For efficient streaming of multiple tokens:

```python
# Create batch
batch = quill.TokenBatch(sequence_id=1)

# Add tokens
batch.add(quill.Token.with_text(1, "Hello", 0))
batch.add(quill.Token.with_text(2, " ", 1))
batch.add(quill.Token.with_text(3, "World", 2))

# Or create from list
tokens = [
    quill.Token.with_text(1, "Hello", 0),
    quill.Token.with_text(2, " ", 1),
    quill.Token.with_text(3, "World", 2),
]
batch = quill.TokenBatch.from_tokens(tokens, sequence_id=1)

# Properties
print(len(batch))          # 3
print(batch.sequence_id)   # 1
print(batch.is_final)      # False
print(batch.is_empty())    # False

# Get data
print(batch.text())        # "Hello World"
print(batch.token_ids())   # [1, 2, 3]
print(batch.positions())   # [0, 1, 2]

# Mark as final
batch.is_final = True

# Iterate
for token in batch:
    print(f"{token.id}: {token.text}")

# Metadata
batch.set_metadata("model", "llama-7b")
batch.set_metadata("temperature", "0.7")
print(batch.get_metadata("model"))  # "llama-7b"
```

## RPC Client

### Basic Usage

```python
client = quill.QuillClient(
    base_url="http://localhost:8080",
    timeout_ms=30000,
    enable_compression=True
)

# Make call with bytes
request = b'{"prompt": "Hello"}'
response = client.call(
    "inference.v1.LLMService",
    "Generate",
    request
)

# Make call with JSON (auto-serialization)
response = client.call_json(
    "inference.v1.LLMService",
    "Generate",
    {"prompt": "Hello", "max_tokens": 100}
)
```

### Authentication

```python
# Bearer token
client.set_bearer_token("my-auth-token")

# API key (default header: X-API-Key)
client.set_api_key("api-key-123")

# API key with custom header
client.set_api_key("api-key-123", header_name="Authorization")

# Custom headers
client.set_header("X-Request-ID", "abc123")
client.remove_header("X-Request-ID")

# Get all headers
headers = client.get_headers()
```

### Client Properties

```python
print(client.base_url)           # "http://localhost:8080"
print(client.timeout_ms)         # 30000
print(client.compression_enabled) # True
```

## Error Handling

Quill Python bindings raise standard Python exceptions:

```python
from quill import QuillClient

try:
    client = QuillClient("http://localhost:8080")
    response = client.call("service", "method", b"data")
except ConnectionError as e:
    print(f"Connection failed: {e}")
except TimeoutError as e:
    print(f"Request timed out: {e}")
except RuntimeError as e:
    print(f"RPC error: {e}")
except ValueError as e:
    print(f"Invalid input: {e}")
```

## Performance Considerations

### Memory Efficiency

- `Tensor.from_numpy()` copies data to avoid lifetime issues
- `Tensor.to_numpy()` returns a copy for memory safety
- Use `tobytes()` for zero-copy raw byte access

### Threading

The RPC client uses `allow_threads` to release the GIL during network operations, enabling concurrent Python threads.

### Compression

Enable zstd compression for large payloads:

```python
client = QuillClient(
    "http://localhost:8080",
    enable_compression=True
)
```

## Complete Example

```python
import quill
import numpy as np

# Create input tensor
input_data = np.random.randn(1, 768).astype(np.float32)
input_tensor = quill.Tensor.from_numpy(input_data, name="embedding")

# Create client
client = quill.QuillClient(
    "http://localhost:8080",
    timeout_ms=60000,
    enable_compression=True
)
client.set_bearer_token("my-token")

# Make inference request
request = {
    "embedding": list(input_tensor.to_numpy().flatten()),
    "max_tokens": 50,
    "temperature": 0.7
}

try:
    response = client.call_json(
        "inference.v1.LLMService",
        "Generate",
        request
    )

    # Process token response
    for token_data in response.get("tokens", []):
        print(token_data.get("text", ""), end="")
    print()

except Exception as e:
    print(f"Error: {e}")
```

## Development

### Running Tests

Tests require Python linking, so use maturin:

```bash
# Build and install
maturin develop

# Run Python tests
pytest tests/
```

For Rust-only development:

```bash
# Build library
cargo build -p quill-python

# Run Rust tests (requires python-tests feature)
cargo test -p quill-python --features python-tests
```

### Building Wheels

```bash
# Debug build
maturin build

# Release build
maturin build --release

# Specific Python version
maturin build --interpreter python3.11

# Upload to PyPI
maturin upload target/wheels/*.whl
```
