# Quill CLI

The official CLI tool for the Quill RPC framework. Provides unified tooling for code generation, making RPC calls, benchmarking, compatibility checking, and payload decoding.

## Installation

```bash
cargo install --path crates/quill-cli
```

Or build from source:

```bash
cargo build --release -p quill-cli
```

## Commands

### `quill gen` - Code Generation

Generate Rust code from .proto files:

```bash
quill gen proto/myservice.proto \
  -I proto \
  --out ./gen \
  --package-prefix myapp
```

**Options:**
- `<PROTOS>...` - Proto files to compile (required)
- `-I, --includes <DIR>` - Include directories for imports (default: `.`)
- `-o, --out <DIR>` - Output directory (defaults to `OUT_DIR`)
- `--client-only` - Generate only client code
- `--server-only` - Generate only server code
- `--package-prefix <PREFIX>` - Add prefix to service paths

**Note:** For build-time code generation, use `quill-codegen` directly in your `build.rs`:

```rust
use quill_codegen::{compile_protos, QuillConfig};

fn main() -> std::io::Result<()> {
    let config = QuillConfig::new();
    compile_protos(&["proto/service.proto"], &["proto"], config)?;
    Ok(())
}
```

### `quill call` - Make RPC Calls

Make RPC calls to Quill services (curl-for-proto):

```bash
# Unary call
quill call http://localhost:8080/greeter.v1.Greeter/SayHello \
  --input '{"name":"World"}' \
  --pretty

# Server streaming call
quill call http://localhost:8080/greeter.v1.Greeter/SayHelloStream \
  --input '{"name":"World"}' \
  --stream \
  --pretty

# Read input from file
quill call http://localhost:8080/service/Method \
  --input @request.json

# Add custom headers
quill call http://localhost:8080/service/Method \
  --input '{}' \
  -H "Authorization:Bearer token" \
  -H "X-Custom-Header:value"
```

**Options:**
- `<URL>` - RPC endpoint URL in format `http://host:port/package.Service/Method`
- `-i, --input <JSON>` - Input JSON string or `@file` path
- `-H, --headers <KEY:VALUE>` - Additional headers
- `--stream` - Enable server streaming mode
- `--compress` - Enable compression
- `--pretty` - Pretty-print JSON output
- `--timeout <SECONDS>` - Request timeout (default: 30)
- `--prism <PROFILE>` - Transport profile preference (hyper,turbo,classic)

**Exit Codes:**
- `0` - Success
- `2` - Invalid input
- `3` - Network/connection error
- `4` - Server error

### `quill bench` - Benchmarking

Run performance benchmarks against Quill services:

```bash
quill bench \
  --config benchmarks.yaml \
  --concurrency 50 \
  --duration 30 \
  --rps 1000
```

**Options:**
- `-c, --config <FILE>` - Benchmarks configuration file (default: `benchmarks.yaml`)
- `--concurrency <N>` - Number of concurrent requests (default: 50)
- `--duration <SECONDS>` - Benchmark duration (default: 10)
- `--rps <N>` - Target requests per second
- `-o, --output <FORMAT>` - Output format: text, json (default: text)

**Status:** Coming soon

### `quill compat` - Compatibility Checking

Check for breaking changes between proto versions:

```bash
# Compare against git ref
quill compat --against main proto/*.proto --strict

# Compare against registry
quill compat --against buf.build/myorg/myapi proto/*.proto
```

**Options:**
- `--against <REF>` - Reference to compare against (git ref or registry URL)
- `<PROTOS>...` - Proto files to check
- `--strict` - Fail on any breaking changes

**Status:** Coming soon

### `quill explain` - Payload Decoding

Decode protobuf payloads using file descriptor sets:

```bash
# Decode hex payload
quill explain \
  --descriptor-set service.pb \
  --payload 0a05776f726c64 \
  --message-type greeter.v1.HelloRequest \
  --format hex

# Decode from file
quill explain \
  --descriptor-set service.pb \
  --payload payload.bin \
  --format file \
  --output json
```

**Options:**
- `-d, --descriptor-set <FILE>` - Path to file descriptor set (.pb file)
- `-p, --payload <DATA>` - Payload to decode (hex, base64, or file path)
- `-m, --message-type <TYPE>` - Message type (e.g., `package.Message`)
- `-f, --format <FORMAT>` - Input format: hex, base64, file (default: hex)
- `-o, --output <FORMAT>` - Output format: json, text (default: json)

**Status:** Coming soon

## Examples

### Generate Code for a Service

```bash
# Basic generation
quill gen proto/greeter.proto -I proto

# Client-only with custom prefix
quill gen proto/api.proto \
  -I proto \
  --client-only \
  --package-prefix myapp
```

### Make an RPC Call

```bash
# Simple unary call
quill call http://localhost:8080/greeter.v1.Greeter/SayHello \
  --input '{"name":"Alice"}' \
  --pretty

# Streaming with compression
quill call http://localhost:8080/logs.v1.LogService/Tail \
  --input '{"service":"api"}' \
  --stream \
  --compress
```

### Run Benchmarks

```bash
# Quick 10-second benchmark
quill bench --duration 10

# Sustained load test
quill bench \
  --concurrency 100 \
  --duration 300 \
  --rps 5000 \
  --output json > results.json
```

## Configuration

The CLI respects the following environment variables:

- `QUILL_BASE_URL` - Default base URL for `call` command
- `QUILL_TIMEOUT` - Default timeout in seconds
- `OUT_DIR` - Default output directory for `gen` command

## See Also

- [Quill Documentation](../../docs/)
- [Examples](../../examples/)
- [Protocol Specification](../../docs/protocol.md)
