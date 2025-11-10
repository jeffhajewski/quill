//! Core types and utilities for the Quill RPC framework.
//!
//! This crate provides the foundation types used across all Quill components:
//! - Stream framing (varint encoding, frame parsing)
//! - Problem Details error model
//! - Prism transport profiles
//! - Flow control primitives

pub mod error;
pub mod framing;
pub mod profile;

pub use error::{ProblemDetails, QuillError};
pub use framing::{Frame, FrameFlags, FrameParser};
pub use profile::{PrismProfile, ProfilePreference};
