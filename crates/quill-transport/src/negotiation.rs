//! Profile negotiation logic

use quill_core::{PrismProfile, ProfilePreference};

pub struct ProfileNegotiator {
    supported: Vec<PrismProfile>,
}

impl ProfileNegotiator {
    pub fn new(supported: Vec<PrismProfile>) -> Self {
        Self { supported }
    }

    /// Negotiate profile based on client preference
    pub fn negotiate(&self, client_pref: &ProfilePreference) -> Option<PrismProfile> {
        client_pref.negotiate(&self.supported)
    }
}

impl Default for ProfileNegotiator {
    /// Default negotiator supports all profiles
    fn default() -> Self {
        Self {
            supported: vec![
                PrismProfile::Hyper,
                PrismProfile::Turbo,
                PrismProfile::Classic,
            ],
        }
    }
}

/// Parse "Prefer" header and return selected profile
pub fn negotiate_profile(
    prefer_header: Option<&str>,
    server_supported: &[PrismProfile],
) -> PrismProfile {
    if let Some(header_value) = prefer_header {
        if let Some(pref) = ProfilePreference::from_header_value(header_value) {
            if let Some(selected) = pref.negotiate(server_supported) {
                return selected;
            }
        }
    }

    // Default to best supported profile
    if server_supported.contains(&PrismProfile::Hyper) {
        PrismProfile::Hyper
    } else if server_supported.contains(&PrismProfile::Turbo) {
        PrismProfile::Turbo
    } else {
        PrismProfile::Classic
    }
}
