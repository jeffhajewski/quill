# Transport Profiles (Prism)

Quill uses **Prism** transport profiles for adaptive protocol selection.

## Profiles

| Profile | Protocol | Best For |
|---------|----------|----------|
| **Classic** | HTTP/1.1 + H2 | Enterprise proxies, legacy networks |
| **Turbo** | HTTP/2 end-to-end | Internal cluster traffic |
| **Hyper** | HTTP/3 over QUIC | Browser/mobile, lossy networks |

## Negotiation

Client sends preference:
```http
Prefer: prism=hyper,turbo,classic
```

Server responds:
```http
Selected-Prism: turbo
```

## Configuration

```rust
use quill_client::QuillClient;
use quill_core::PrismProfile;

let client = QuillClient::builder()
    .base_url("http://api.example.com")
    .prefer_profiles(&[
        PrismProfile::Hyper,
        PrismProfile::Turbo,
        PrismProfile::Classic,
    ])
    .build()?;
```

## Profile Comparison

| Feature | Classic | Turbo | Hyper |
|---------|---------|-------|-------|
| Protocol | HTTP/1.1 | HTTP/2 | HTTP/3 |
| Multiplexing | No | Yes | Yes |
| Head-of-line blocking | Yes | Partial | No |
| Connection migration | No | No | Yes |
| 0-RTT | No | No | Yes |
| Proxy compatibility | Best | Good | Limited |

## Deployment Patterns

### Edge H3 → Interior H2
```
Browser ──[HTTP/3]──► Edge ──[HTTP/2]──► Services
         Hyper            Turbo
```

### H2 Everywhere
```
Client ──[HTTP/2]──► LB ──[HTTP/2]──► Services
        Turbo             Turbo
```
