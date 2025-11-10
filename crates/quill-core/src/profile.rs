//! Prism transport profile types.

use std::fmt;
use std::str::FromStr;

/// Prism transport profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrismProfile {
    /// HTTP/1.1 (chunked) or basic HTTP/2 - for legacy/enterprise proxies
    Classic,

    /// HTTP/2 end-to-end - for cluster-internal traffic
    Turbo,

    /// HTTP/3 over QUIC - for browser/mobile, lossy networks, edge-to-client
    Hyper,
}

impl PrismProfile {
    /// Get the profile name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Turbo => "turbo",
            Self::Hyper => "hyper",
        }
    }

    /// Get the negotiation weight (higher = more preferred)
    pub fn weight(&self) -> f32 {
        match self {
            Self::Classic => 0.5,
            Self::Turbo => 0.8,
            Self::Hyper => 1.0,
        }
    }

    /// Check if this profile supports HTTP/3 datagrams
    pub fn supports_datagrams(&self) -> bool {
        matches!(self, Self::Hyper)
    }

    /// Check if this profile supports 0-RTT
    pub fn supports_zero_rtt(&self) -> bool {
        matches!(self, Self::Hyper)
    }
}

impl fmt::Display for PrismProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for PrismProfile {
    type Err = ProfileParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "classic" => Ok(Self::Classic),
            "turbo" => Ok(Self::Turbo),
            "hyper" => Ok(Self::Hyper),
            _ => Err(ProfileParseError(s.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Unknown profile: {0}")]
pub struct ProfileParseError(String);

/// Profile preference list for negotiation
#[derive(Debug, Clone)]
pub struct ProfilePreference {
    profiles: Vec<PrismProfile>,
}

impl ProfilePreference {
    /// Create a new preference list
    pub fn new(profiles: Vec<PrismProfile>) -> Self {
        Self { profiles }
    }

    /// Default preference: hyper > turbo > classic
    pub fn default_preference() -> Self {
        Self {
            profiles: vec![
                PrismProfile::Hyper,
                PrismProfile::Turbo,
                PrismProfile::Classic,
            ],
        }
    }

    /// Get the profiles in order of preference
    pub fn profiles(&self) -> &[PrismProfile] {
        &self.profiles
    }

    /// Format as "Prefer" header value: "prism=hyper,turbo,classic"
    pub fn to_header_value(&self) -> String {
        let profiles: Vec<_> = self.profiles.iter().map(|p| p.as_str()).collect();
        format!("prism={}", profiles.join(","))
    }

    /// Parse from "Prefer" header value
    pub fn from_header_value(value: &str) -> Option<Self> {
        // Expected format: "prism=hyper,turbo,classic"
        let value = value.trim();

        if let Some(prism_part) = value.strip_prefix("prism=") {
            let profiles: Result<Vec<_>, _> = prism_part
                .split(',')
                .map(|s| s.trim().parse::<PrismProfile>())
                .collect();

            profiles.ok().map(|profiles| Self { profiles })
        } else {
            None
        }
    }

    /// Select the best mutually supported profile
    pub fn negotiate(&self, server_supported: &[PrismProfile]) -> Option<PrismProfile> {
        // Pick the first profile from client preference that server supports
        self.profiles
            .iter()
            .find(|p| server_supported.contains(p))
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_from_str() {
        assert_eq!("classic".parse::<PrismProfile>().unwrap(), PrismProfile::Classic);
        assert_eq!("turbo".parse::<PrismProfile>().unwrap(), PrismProfile::Turbo);
        assert_eq!("hyper".parse::<PrismProfile>().unwrap(), PrismProfile::Hyper);
        assert!("unknown".parse::<PrismProfile>().is_err());
    }

    #[test]
    fn test_preference_header() {
        let pref = ProfilePreference::default_preference();
        let header = pref.to_header_value();
        assert_eq!(header, "prism=hyper,turbo,classic");

        let parsed = ProfilePreference::from_header_value(&header).unwrap();
        assert_eq!(parsed.profiles().len(), 3);
    }

    #[test]
    fn test_negotiation() {
        let client = ProfilePreference::new(vec![
            PrismProfile::Hyper,
            PrismProfile::Turbo,
        ]);

        // Server only supports Turbo
        let server = vec![PrismProfile::Turbo, PrismProfile::Classic];
        let selected = client.negotiate(&server).unwrap();
        assert_eq!(selected, PrismProfile::Turbo);
    }
}
