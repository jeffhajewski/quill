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

Check protobuf compatibility between versions using [buf](https://buf.build).

### Prerequisites

Install the `buf` CLI for full functionality:

```bash
# macOS
brew install bufbuild/buf/buf

# Linux
curl -sSL https://github.com/bufbuild/buf/releases/latest/download/buf-Linux-x86_64 -o /usr/local/bin/buf
chmod +x /usr/local/bin/buf
```

### Usage

```bash
quill compat --against <REF> [INPUT...] [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `INPUT` | Proto files or directories to check (default: `.`) |

### Options

| Option | Description |
|--------|-------------|
| `-a, --against <REF>` | Compare against: git ref, local path, or buf registry |
| `--strict` | Exit with code 2 on breaking changes |
| `--config <FILE>` | Path to buf.yaml configuration |
| `-f, --format <FMT>` | Output format: `text`, `json` |
| `--error-limit <N>` | Limit number of errors (0 = unlimited) |

### Examples

```bash
# Compare against git tag
quill compat --against v1.0.0

# Compare against main branch
quill compat --against .git#branch=main

# Compare against local directory
quill compat --against ../old-version

# Check specific directory with JSON output
quill compat --against v1.0.0 ./proto --format json

# Fail CI on breaking changes
quill compat --against HEAD~1 --strict

# Use custom buf configuration
quill compat --against main --config buf.yaml
```

### Breaking Change Categories

The compatibility check detects:

- **Field removals** - Removing a field breaks deserialization
- **Field renumbering** - Changing field numbers breaks wire format
- **Field type changes** - Changing types (e.g., int32 to string)
- **Required field additions** - Adding required fields to existing messages
- **Enum value removals** - Removing enum values breaks existing data
- **Service/method removals** - Removing RPCs breaks clients
- **Method signature changes** - Changing input/output types

### Output

```
Checking compatibility...
  Input:   ./proto
  Against: v1.0.0

Breaking changes detected:

  proto/api.proto:10:3: Field "email" was removed from message "User".
  proto/api.proto:25:3: RPC "DeleteUser" was removed from service "UserService".

Found 2 breaking change(s)
```

JSON output (with `--format json`):

```json
[
  {
    "file": "proto/api.proto",
    "line": 10,
    "column": 3,
    "message": "Field \"email\" was removed from message \"User\".",
    "rule": "FIELD_SAME_NAME"
  }
]
```

## quill explain

Decode and explain protobuf payloads for debugging using dynamic message reflection.

### Prerequisites

Generate a descriptor set from your proto files:

```bash
protoc --descriptor_set_out=api.pb --include_imports your.proto
```

### Usage

```bash
quill explain --descriptor-set <FILE> --payload <DATA> [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `-d, --descriptor-set <FILE>` | Descriptor set file (`.pb` or `.binpb`) |
| `-p, --payload <DATA>` | Payload as hex, base64, or file path |
| `-m, --message-type <NAME>` | Message type (e.g., `users.v1.User`) |
| `-f, --input-format <FMT>` | Input format: `hex`, `base64`, `file`, `auto` (default) |
| `-o, --output-format <FMT>` | Output format: `json`, `json-pretty`, `text`, `debug` |
| `--list-types` | List all message types in descriptor set |
| `--show-field-numbers` | Show field numbers in text output |

### Examples

```bash
# Decode hex payload
quill explain \
  --descriptor-set api.pb \
  --message-type users.v1.User \
  --payload 0a05416c696365

# Decode base64 (auto-detected)
quill explain -d api.pb -m users.v1.User -p CgVBbGljZQ==

# Read from binary file
quill explain \
  --descriptor-set api.pb \
  --message-type users.v1.User \
  --payload response.bin \
  --input-format file

# List available types
quill explain --descriptor-set api.pb --list-types

# Text output with field numbers
quill explain -d api.pb -m users.v1.User -p 0a05416c696365 \
  --output-format text --show-field-numbers
```

### Generating Descriptor Sets

```bash
# Single file
protoc --descriptor_set_out=api.pb --include_imports api.proto

# Multiple files
protoc --descriptor_set_out=all.pb --include_imports \
  -I./proto \
  ./proto/**/*.proto

# With buf
buf build -o api.pb
```

### Output Formats

**JSON (default)**:
```json
{"name":"Alice","email":"alice@example.com"}
```

**JSON Pretty**:
```json
{
  "name": "Alice",
  "email": "alice@example.com",
  "createdAt": "2024-01-01T00:00:00Z"
}
```

**Text** (with `--show-field-numbers`):
```
name [1]: "Alice"
email [2]: "alice@example.com"
created_at [3]: {
  seconds [1]: 1704067200
}
```

### List Types Output

```
Available message types:

  users.v1
    - User
    - CreateUserRequest
    - CreateUserResponse

Services:
  users.v1.UserService
    - GetUser (GetUserRequest -> User)
    - CreateUser (CreateUserRequest -> CreateUserResponse)
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
