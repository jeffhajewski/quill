//! Transport layer implementations for Quill RPC.
//!
//! This crate provides the transport layer for different Prism profiles:
//! - Classic: HTTP/1.1 and basic HTTP/2
//! - Turbo: Full HTTP/2
//! - Hyper: HTTP/3 over QUIC (future)

pub mod classic;
pub mod negotiation;
pub mod turbo;

pub use classic::ClassicTransport;
pub use negotiation::{negotiate_profile, ProfileNegotiator};
pub use turbo::TurboTransport;
