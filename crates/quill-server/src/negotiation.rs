//! Profile negotiation for Quill RPC server
//!
//! Implements the Prism profile negotiation protocol:
//! - Client sends `Prefer: prism=hyper,turbo,classic` header
//! - Server responds with `Selected-Prism: <profile>` header

use http::{HeaderMap, HeaderValue};
use quill_core::{PrismProfile, ProfilePreference};

/// Header name for client profile preference
pub const PREFER_HEADER: &str = "Prefer";

/// Header name for server's selected profile
pub const SELECTED_PRISM_HEADER: &str = "Selected-Prism";

/// Server's supported profiles configuration
#[derive(Debug, Clone)]
pub struct ProfileSupport {
    /// Profiles supported by this server, in preference order
    supported: Vec<PrismProfile>,
    /// Minimum required profile (routes may pin to this)
    minimum: Option<PrismProfile>,
}

impl ProfileSupport {
    /// Create with default support (all profiles)
    pub fn all() -> Self {
        Self {
            supported: vec![
                PrismProfile::Classic,
                PrismProfile::Turbo,
                PrismProfile::Hyper,
            ],
            minimum: None,
        }
    }

    /// Create with Classic only
    pub fn classic_only() -> Self {
        Self {
            supported: vec![PrismProfile::Classic],
            minimum: Some(PrismProfile::Classic),
        }
    }

    /// Create with Classic and Turbo
    pub fn classic_and_turbo() -> Self {
        Self {
            supported: vec![PrismProfile::Classic, PrismProfile::Turbo],
            minimum: None,
        }
    }

    /// Create custom profile support
    pub fn custom(supported: Vec<PrismProfile>) -> Self {
        Self {
            supported,
            minimum: None,
        }
    }

    /// Set minimum required profile
    pub fn with_minimum(mut self, profile: PrismProfile) -> Self {
        self.minimum = Some(profile);
        self
    }

    /// Check if a profile is supported
    pub fn supports(&self, profile: PrismProfile) -> bool {
        self.supported.contains(&profile)
    }

    /// Get supported profiles
    pub fn profiles(&self) -> &[PrismProfile] {
        &self.supported
    }

    /// Get minimum required profile
    pub fn minimum(&self) -> Option<PrismProfile> {
        self.minimum
    }
}

impl Default for ProfileSupport {
    fn default() -> Self {
        Self::all()
    }
}

/// Negotiate the best profile between client and server
pub fn negotiate_profile(
    headers: &HeaderMap,
    server_support: &ProfileSupport,
) -> NegotiationResult {
    // Parse client preference from Prefer header
    let client_pref = headers
        .get(PREFER_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(ProfilePreference::from_header_value);

    match client_pref {
        Some(pref) => {
            // Find the best mutually supported profile
            if let Some(selected) = pref.negotiate(server_support.profiles()) {
                // Check minimum requirement
                if let Some(min) = server_support.minimum() {
                    if selected.weight() < min.weight() {
                        return NegotiationResult::MinimumNotMet {
                            requested: selected,
                            minimum: min,
                        };
                    }
                }
                NegotiationResult::Negotiated(selected)
            } else {
                NegotiationResult::NoCommonProfile
            }
        }
        None => {
            // No preference - use server's best supported
            let default = server_support
                .profiles()
                .iter()
                .max_by(|a, b| a.weight().partial_cmp(&b.weight()).unwrap())
                .copied()
                .unwrap_or(PrismProfile::Classic);
            NegotiationResult::Default(default)
        }
    }
}

/// Result of profile negotiation
#[derive(Debug, Clone, PartialEq)]
pub enum NegotiationResult {
    /// Successfully negotiated a profile
    Negotiated(PrismProfile),
    /// No Prefer header - using server default
    Default(PrismProfile),
    /// No common profile supported
    NoCommonProfile,
    /// Client's best profile doesn't meet minimum
    MinimumNotMet {
        requested: PrismProfile,
        minimum: PrismProfile,
    },
}

impl NegotiationResult {
    /// Get the selected profile, if any
    pub fn profile(&self) -> Option<PrismProfile> {
        match self {
            Self::Negotiated(p) | Self::Default(p) => Some(*p),
            Self::NoCommonProfile | Self::MinimumNotMet { .. } => None,
        }
    }

    /// Check if negotiation was successful
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Negotiated(_) | Self::Default(_))
    }

    /// Create the Selected-Prism response header value
    pub fn to_header_value(&self) -> Option<HeaderValue> {
        self.profile()
            .and_then(|p| HeaderValue::from_str(p.as_str()).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negotiate_with_preference() {
        let mut headers = HeaderMap::new();
        headers.insert(PREFER_HEADER, "prism=hyper,turbo".parse().unwrap());

        let support = ProfileSupport::all();
        let result = negotiate_profile(&headers, &support);

        assert_eq!(result, NegotiationResult::Negotiated(PrismProfile::Hyper));
    }

    #[test]
    fn test_negotiate_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert(PREFER_HEADER, "prism=hyper".parse().unwrap());

        // Server only supports Classic and Turbo
        let support = ProfileSupport::classic_and_turbo();
        let result = negotiate_profile(&headers, &support);

        assert_eq!(result, NegotiationResult::NoCommonProfile);
    }

    #[test]
    fn test_negotiate_default() {
        let headers = HeaderMap::new();
        let support = ProfileSupport::all();
        let result = negotiate_profile(&headers, &support);

        // Should pick the highest weighted profile
        assert!(matches!(result, NegotiationResult::Default(PrismProfile::Hyper)));
    }

    #[test]
    fn test_negotiate_with_minimum() {
        let mut headers = HeaderMap::new();
        headers.insert(PREFER_HEADER, "prism=classic".parse().unwrap());

        // Server requires at least Turbo
        let support = ProfileSupport::all().with_minimum(PrismProfile::Turbo);
        let result = negotiate_profile(&headers, &support);

        assert!(matches!(result, NegotiationResult::MinimumNotMet { .. }));
    }

    #[test]
    fn test_to_header_value() {
        let result = NegotiationResult::Negotiated(PrismProfile::Turbo);
        let header = result.to_header_value().unwrap();
        assert_eq!(header.to_str().unwrap(), "turbo");
    }

    #[test]
    fn test_profile_support_custom() {
        let support = ProfileSupport::custom(vec![PrismProfile::Turbo, PrismProfile::Classic]);
        assert!(support.supports(PrismProfile::Turbo));
        assert!(support.supports(PrismProfile::Classic));
        assert!(!support.supports(PrismProfile::Hyper));
    }
}
