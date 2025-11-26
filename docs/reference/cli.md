# CLI Reference

The `quill` CLI provides tools for code generation, RPC calls, benchmarking, and debugging.

## Installation

```bash
# From source
cargo install --path crates/quill-cli

# Verify installation
quill --version
```

## Commands Overview

| Command | Description |
|---------|-------------|
| `quill gen` | Generate client/server code from .proto files |
| `quill call` | Make RPC calls (curl-for-proto) |
| `quill bench` | Run benchmarks against services |
| `quill compat` | Check protobuf compatibility |
| `quill explain` | Decode protobuf payloads |

## quill gen

Generate client and server stubs from protobuf definitions.

### Usage

```bash
quill gen [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--lang <LANG>` | Target language(s): `go`, `ts`, `rust` (comma-separated) |
| `--out <DIR>` | Output directory (default: `./gen`) |
| `--proto <PATH>` | Proto file or directory (default: `./proto`) |
| `--include <PATH>` | Additional include paths |

### Examples

```bash
# Generate Go and TypeScript
quill gen --lang go,ts --out ./gen

# Generate from specific proto
quill gen --lang rust --proto ./api/v1/service.proto

# With include paths
quill gen --lang go --include ./vendor/proto
```

## quill call

Make RPC calls to Quill services. Think "curl for protobuf".

### Usage

```bash
quill call <URL> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<URL>` | Full URL: `http://host/package.Service/Method` |

### Options

| Option | Description |
|--------|-------------|
| `--in <DATA>` | Request body as JSON or `@file.json` |
| `--header <K:V>` | Add request header (can be repeated) |
| `--stream` | Enable streaming mode |
| `--prism <PROFILE>` | Transport profile: `classic`, `turbo`, `hyper` |
| `--accept <TYPE>` | Accept content type |
| `--timeout <SECS>` | Request timeout in seconds |
| `--compress` | Enable request compression |

### Examples

```bash
# Simple unary call
quill call http://localhost:8080/users.v1.UserService/GetUser \
  --in '{"user_id": "123"}'

# With headers
quill call http://localhost:8080/api.v1.Service/Method \
  --in '{"key": "value"}' \
  --header "Authorization: Bearer token123"

# Server streaming
quill call http://localhost:8080/logs.v1.LogService/TailLogs \
  --in '{"filter": "level=ERROR"}' \
  --stream

# Using a specific transport profile
quill call http://localhost:8080/service/method \
  --in '{"data": "test"}' \
  --prism turbo

# Read input from file
quill call http://localhost:8080/service/method \
  --in @request.json

# With timeout
quill call http://localhost:8080/service/method \
  --in '{}' \
  --timeout 60
```

### Output Format

Responses are printed as JSON:

```json
{
  "user_id": "123",
  "name": "Alice",
  "email": "alice@example.com"
}
```

For streaming responses, each message is printed on a separate line:

```json
{"timestamp": "2024-01-01T00:00:00Z", "level": "ERROR", "message": "First"}
{"timestamp": "2024-01-01T00:00:01Z", "level": "ERROR", "message": "Second"}
```

## quill bench

Run benchmarks against Quill services.

### Usage

```bash
quill bench [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--config <FILE>` | Benchmark configuration (default: `benchmarks.yaml`) |
| `--output <FILE>` | Output file for results |
| `--format <FMT>` | Output format: `json`, `csv`, `markdown` |
| `--duration <SECS>` | Test duration per scenario |
| `--connections <N>` | Concurrent connections |

### Configuration File

```yaml
# benchmarks.yaml
scenarios:
  - name: "unary-small"
    method: "users.v1.UserService/GetUser"
    request: '{"user_id": "123"}'
    duration: 30s
    connections: 10
    rate: 1000  # requests per second

  - name: "streaming"
    method: "logs.v1.LogService/TailLogs"
    request: '{"limit": 100}'
    streaming: true
    duration: 60s
    connections: 5

targets:
  - name: "local"
    url: "http://localhost:8080"
  - name: "staging"
    url: "https://staging.example.com"
```

### Examples

```bash
# Run with default config
quill bench

# Custom config
quill bench --config ./performance/benchmarks.yaml

# Quick benchmark
quill bench --duration 10 --connections 5

# Output results
quill bench --output results.json --format json
```

### Output

```
Benchmark Results
================

Target: http://localhost:8080
Duration: 30s
Connections: 10

Scenario: unary-small
  Requests:     30,245
  Throughput:   1,008 req/s
  Latency:
    p50:        2.3ms
    p90:        4.1ms
    p99:        8.7ms
  Errors:       0 (0.00%)
```

## quill compat

Check protobuf compatibility between versions.

### Usage

```bash
quill compat [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--against <REF>` | Compare against: git ref, tag, or registry |
| `--proto <PATH>` | Proto files to check |
| `--breaking` | Only report breaking changes |

### Examples

```bash
# Compare against git tag
quill compat --against v1.0.0

# Compare against main branch
quill compat --against main

# Check specific proto
quill compat --against v1.0.0 --proto ./api/v1/users.proto

# Breaking changes only
quill compat --against v1.0.0 --breaking
```

### Output

```
Compatibility Report
====================

File: api/v1/users.proto

BREAKING:
  - Field 'email' removed from message 'User'
  - RPC 'DeleteUser' removed from service 'UserService'

COMPATIBLE:
  - Field 'phone' added to message 'User' (field 4)
  - RPC 'UpdateUser' added to service 'UserService'

Summary: 2 breaking, 2 compatible changes
```

## quill explain

Decode and explain protobuf payloads for debugging.

### Usage

```bash
quill explain [OPTIONS] <PAYLOAD>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<PAYLOAD>` | Payload as hex, base64, or `@file` |

### Options

| Option | Description |
|--------|-------------|
| `--descriptor <FILE>` | Descriptor set file (`.pb`) |
| `--type <NAME>` | Expected message type |
| `--format <FMT>` | Output: `json`, `text`, `binary` |

### Examples

```bash
# Decode hex payload
quill explain --descriptor api.pb \
  --type users.v1.User \
  0a05416c696365

# Decode base64
quill explain --descriptor api.pb \
  --type users.v1.User \
  CgVBbGljZQ==

# From file
quill explain --descriptor api.pb \
  --type users.v1.User \
  @payload.bin
```

### Output

```json
{
  "name": "Alice",
  "email": "alice@example.com",
  "created_at": "2024-01-01T00:00:00Z"
}
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Invalid input (bad arguments, malformed request) |
| 3 | Network/protocol error (connection failed, timeout) |
| 4 | Server error (non-2xx response) |

### Example Error Handling

```bash
quill call http://localhost:8080/service/method --in '{}'
status=$?

case $status in
  0) echo "Success" ;;
  2) echo "Invalid input" ;;
  3) echo "Network error" ;;
  4) echo "Server error" ;;
esac
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `QUILL_URL` | Default base URL |
| `QUILL_TOKEN` | Default bearer token |
| `QUILL_TIMEOUT` | Default timeout (seconds) |
| `QUILL_PRISM` | Default transport profile |
| `QUILL_PROTO_PATH` | Default proto include paths |

### Example

```bash
export QUILL_URL=http://localhost:8080
export QUILL_TOKEN=my-api-token
export QUILL_TIMEOUT=30

# Now just use method path
quill call /users.v1.UserService/GetUser --in '{"user_id": "123"}'
```

## Shell Completion

Generate shell completions:

```bash
# Bash
quill completions bash > /etc/bash_completion.d/quill

# Zsh
quill completions zsh > ~/.zfunc/_quill

# Fish
quill completions fish > ~/.config/fish/completions/quill.fish
```

## Next Steps

- [Quick Start](../getting-started/quickstart.md) - Get started with Quill
- [Configuration](configuration.md) - Configuration reference
- [Server Guide](../guides/server.md) - Server development
