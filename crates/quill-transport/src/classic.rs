//! Classic profile (HTTP/1.1 and basic HTTP/2) transport implementation

use quill_core::PrismProfile;

/// HTTP/1.1 transport for the Classic profile
pub struct ClassicTransport {
    profile: PrismProfile,
}

impl ClassicTransport {
    /// Create a new Classic transport
    pub fn new() -> Self {
        Self {
            profile: PrismProfile::Classic,
        }
    }

    /// Get the profile this transport implements
    pub fn profile(&self) -> PrismProfile {
        self.profile
    }
}

impl Default for ClassicTransport {
    fn default() -> Self {
        Self::new()
    }
}
