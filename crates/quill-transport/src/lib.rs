//! Transport layer implementations for Quill RPC.
//!
//! This crate provides the transport layer for different Prism profiles:
//! - Classic: HTTP/1.1 and basic HTTP/2
//! - Turbo: Full HTTP/2
//! - Hyper: HTTP/3 over QUIC

pub mod classic;
pub mod hyper;
pub mod negotiation;
pub mod turbo;

pub use classic::ClassicTransport;
pub use negotiation::{negotiate_profile, ProfileNegotiator};
pub use turbo::TurboTransport;

#[cfg(feature = "http3")]
pub use hyper::{
    BoxFuture, Datagram, DatagramHandler, DatagramReceiver, DatagramSender, FnDatagramHandler,
    H3Client, H3ClientBuilder, H3Connection, H3Server, H3ServerBuilder, H3Service, HyperConfig,
    HyperError, HyperTransport, ServerConnection,
};
