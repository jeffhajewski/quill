# Architecture

Quill is a modular RPC framework with clear separation of concerns.

## Crate Hierarchy

```
┌─────────────────────────────────────────────────────────────┐
│                       Application                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐              ┌─────────────────┐       │
│  │   quill-server  │              │   quill-client  │       │
│  │   Server SDK    │              │   Client SDK    │       │
│  └────────┬────────┘              └────────┬────────┘       │
│           └────────────┬───────────────────┘                │
│                        │                                    │
│               ┌────────▼────────┐                           │
│               │   quill-core    │                           │
│               │  Core Types     │                           │
│               └────────┬────────┘                           │
│                        │                                    │
│               ┌────────▼────────┐                           │
│               │ quill-transport │                           │
│               │ Classic│Turbo│  │                           │
│               │      Hyper      │                           │
│               └─────────────────┘                           │
└─────────────────────────────────────────────────────────────┘
```

## Core Components

| Crate | Description |
|-------|-------------|
| `quill-core` | Frame encoding, Problem Details, flow control |
| `quill-transport` | HTTP/1.1, HTTP/2, HTTP/3 transports |
| `quill-server` | Request routing, middleware, streaming |
| `quill-client` | Connection management, retries, streaming |
| `quill-tensor` | Tensor types for ML inference |
| `quill-python` | Python bindings via PyO3 |

## Request Flow

```
Client → Encode → Transport → Network → Transport → Decode → Middleware → Handler
```

## Frame Protocol

```
[Length (varint)][Flags (1 byte)][Payload (N bytes)]
```

Flags: `DATA (0x01)`, `END_STREAM (0x02)`, `CANCEL (0x04)`, `CREDIT (0x08)`

## Error Model

Quill uses Problem Details (RFC 7807):

```json
{
  "type": "https://quill.dev/errors/not-found",
  "title": "Not Found",
  "status": 404,
  "detail": "User 123 not found"
}
```

Real HTTP status codes—no 200-with-error-envelope.

## Design Principles

1. **Protobuf-First**: `.proto` is the source of truth
2. **Real HTTP Errors**: Proper status codes
3. **No Trailers Required**: Works through any proxy
4. **Progressive Enhancement**: HTTP/3 is optional
5. **Observable by Default**: Built-in tracing and metrics
