# Quill Python Bindings

Python bindings for the Quill RPC framework, providing efficient tensor streaming and LLM inference capabilities.

## Installation

### From PyPI (coming soon)

```bash
pip install quill
```

### From Source

Requires [maturin](https://www.maturin.rs/) and Rust toolchain:

```bash
cd crates/quill-python
pip install maturin
maturin develop  # For development
# or
maturin build --release  # For wheel
pip install target/wheels/quill-*.whl
```

## Quick Start

### Tensor Operations

```python
import quill
import numpy as np

# Create a tensor from NumPy array
arr = np.array([[1.0, 2.0], [3.0, 4.0]], dtype=np.float32)
tensor = quill.Tensor.from_numpy(arr, name="input")

# Access tensor properties
print(f"Shape: {tensor.shape}")      # [2, 2]
print(f"DType: {tensor.dtype}")      # float32
print(f"Elements: {tensor.num_elements}")  # 4
print(f"Bytes: {tensor.size_bytes}")  # 16

# Convert back to NumPy
result = tensor.to_numpy()

# Create zero tensor
zeros = quill.Tensor.zeros([3, 4], quill.DType.float32(), name="zeros")

# Create from raw bytes
data = bytes(16)  # 4 float32 zeros
tensor = quill.Tensor.from_bytes(data, [2, 2], quill.DType.float32())
```

### Data Types

```python
import quill

# Available data types
dtypes = [
    quill.DType.float32(),   # 32-bit float
    quill.DType.float64(),   # 64-bit float
    quill.DType.float16(),   # 16-bit IEEE float
    quill.DType.bfloat16(),  # Brain floating point
    quill.DType.int8(),      # 8-bit signed integer
    quill.DType.int32(),     # 32-bit signed integer
    quill.DType.int64(),     # 64-bit signed integer
    quill.DType.uint8(),     # 8-bit unsigned integer
    quill.DType.bool_(),     # Boolean
]

# Check properties
dtype = quill.DType.float32()
print(f"Element size: {dtype.element_size} bytes")  # 4
print(f"Is float: {dtype.is_float()}")  # True
print(f"Is integer: {dtype.is_integer()}")  # False
```

### LLM Token Handling

```python
import quill

# Create individual tokens
token = quill.Token(id=42, position=0, text="hello", logprob=-0.5)
print(f"Token: {token}")  # Token(id=42, text='hello', pos=0)
print(f"Probability: {token.prob()}")  # ~0.607

# Create token batches for streaming
batch = quill.TokenBatch(sequence_id=1)
batch.add(quill.Token.with_text(1, "Hello", 0))
batch.add(quill.Token.with_text(2, " ", 1))
batch.add(quill.Token.with_text(3, "World", 2))

# Get concatenated text
print(batch.text())  # "Hello World"

# Get token IDs
print(batch.token_ids())  # [1, 2, 3]

# Mark final batch
batch.is_final = True

# Add metadata
batch.set_metadata("model", "llama-7b")
batch.set_metadata("temperature", "0.7")

# Iterate over tokens
for token in batch:
    print(f"  {token.id}: {token.text}")
```

### RPC Client

```python
import quill

# Create client
client = quill.QuillClient(
    "http://localhost:8080",
    timeout_ms=30000,
    enable_compression=True
)

# Set authentication
client.set_bearer_token("my-auth-token")
# or
client.set_api_key("api-key-123")

# Make RPC call with bytes
request = b'{"prompt": "Hello"}'
response = client.call(
    "inference.v1.LLMService",
    "Generate",
    request
)

# Make RPC call with JSON (automatic serialization)
response = client.call_json(
    "inference.v1.LLMService",
    "Generate",
    {"prompt": "Hello", "max_tokens": 100}
)
```

## API Reference

### DType

Data type enumeration for tensor elements.

| Method | Description |
|--------|-------------|
| `float32()` | 32-bit IEEE floating point |
| `float64()` | 64-bit IEEE floating point |
| `float16()` | 16-bit IEEE floating point |
| `bfloat16()` | 16-bit brain floating point |
| `int8()` | 8-bit signed integer |
| `int32()` | 32-bit signed integer |
| `int64()` | 64-bit signed integer |
| `uint8()` | 8-bit unsigned integer |
| `bool_()` | Boolean |

Properties: `element_size`, `name`, `is_float()`, `is_integer()`, `is_signed()`

### Tensor

Multi-dimensional tensor with data.

| Method | Description |
|--------|-------------|
| `from_numpy(array, name=None)` | Create from NumPy array |
| `zeros(shape, dtype, name=None)` | Create zero tensor |
| `from_bytes(data, shape, dtype, name=None)` | Create from raw bytes |
| `to_numpy()` | Convert to NumPy array |
| `tobytes()` | Get raw bytes |

Properties: `shape`, `dtype`, `name`, `ndim`, `num_elements`, `size_bytes`, `meta`

### TensorMeta

Tensor metadata (shape and type without data).

| Method | Description |
|--------|-------------|
| `TensorMeta(shape, dtype, name=None)` | Create metadata |

Properties: `shape`, `dtype`, `name`, `ndim`, `num_elements`, `size_bytes`

### Token

Single token from LLM generation.

| Method | Description |
|--------|-------------|
| `Token(id, position, text=None, logprob=None, is_special=False)` | Create token |
| `with_text(id, text, position, logprob=None)` | Create with text |
| `prob()` | Get probability (exp of logprob) |
| `is_eos()` | Check if end-of-sequence |
| `to_dict()` | Convert to dictionary |
| `encode()` | Encode to bytes |

Properties: `id`, `text`, `logprob`, `position`, `is_special`

### TokenBatch

Batch of tokens for streaming.

| Method | Description |
|--------|-------------|
| `TokenBatch(sequence_id=None)` | Create empty batch |
| `from_tokens(tokens, sequence_id=None)` | Create from token list |
| `add(token)` | Add single token |
| `extend(tokens)` | Add multiple tokens |
| `tokens()` | Get all tokens |
| `get(index)` | Get token at index |
| `text()` | Get concatenated text |
| `token_ids()` | Get list of token IDs |
| `positions()` | Get list of positions |
| `clear()` | Remove all tokens |
| `encode()` | Encode to bytes |
| `to_dicts()` | Convert to list of dicts |

Properties: `sequence_id`, `is_final`, `is_empty()`, `len(batch)`

### QuillClient

RPC client for Quill services.

| Method | Description |
|--------|-------------|
| `QuillClient(base_url, timeout_ms=30000, enable_compression=False)` | Create client |
| `call(service, method, request)` | Make RPC call (bytes) |
| `call_json(service, method, request)` | Make RPC call (JSON dict) |
| `set_bearer_token(token)` | Set auth bearer token |
| `set_api_key(key, header_name=None)` | Set API key header |
| `set_header(name, value)` | Set custom header |
| `remove_header(name)` | Remove header |
| `get_headers()` | Get all headers |
| `health_check()` | Check server health |

Properties: `base_url`, `timeout_ms`, `compression_enabled`

## Development

### Running Tests

Tests require Python to be linked, which requires building with maturin:

```bash
# Build and install in development mode
maturin develop

# Run Python tests
pytest tests/
```

For Rust-only tests (without Python linking):

```bash
# This will show 0 tests (tests are behind feature flag)
cargo test -p quill-python

# To run Rust tests with Python linking (requires Python):
cargo test -p quill-python --features python-tests
```

### Building Wheels

```bash
# Development build
maturin build

# Release build
maturin build --release

# Build for specific Python version
maturin build --interpreter python3.11
```

## License

This project is dual-licensed under MIT and Apache 2.0.
