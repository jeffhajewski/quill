# Installation

## Rust Crates

Add to `Cargo.toml`:

### Server

```toml
[dependencies]
quill-server = "0.1"
quill-core = "0.1"
tokio = { version = "1", features = ["full"] }
bytes = "1"
```

### Client

```toml
[dependencies]
quill-client = "0.1"
quill-core = "0.1"
tokio = { version = "1", features = ["full"] }
bytes = "1"
```

### Full Stack with Code Generation

```toml
[dependencies]
quill-server = "0.1"
quill-client = "0.1"
quill-core = "0.1"
tokio = { version = "1", features = ["full"] }
bytes = "1"

[build-dependencies]
quill-codegen = "0.1"
```

## Optional Features

### HTTP/3 Support

```toml
quill-transport = { version = "0.1", features = ["http3"] }
```

### WebTransport

```toml
quill-transport = { version = "0.1", features = ["webtransport"] }
```

### Tensor/ML Support

```toml
quill-tensor = "0.1"
```

## CLI Tool

```bash
# From source
git clone https://github.com/quill/quill
cd quill
cargo install --path crates/quill-cli

# Verify
quill --version
```

## Python Bindings

```bash
cd crates/quill-python
pip install maturin
maturin develop
```

Requirements: Python 3.8+, NumPy >= 1.20

## Development Setup

```bash
git clone https://github.com/quill/quill
cd quill
cargo build --workspace
cargo test --workspace
```

### Build Documentation

```bash
cargo install mdbook
mdbook serve  # Opens at http://localhost:3000
```
