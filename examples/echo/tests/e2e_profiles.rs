//! End-to-end tests for Prism profile negotiation
//!
//! Tests verify that all three profiles (Classic, Turbo, Hyper) work correctly
//! and that profile negotiation follows the specification.

use http::HeaderMap;
use quill_client::QuillClient;
use quill_core::{PrismProfile, ProfilePreference};
use quill_server::{negotiate_profile, NegotiationResult, ProfileSupport, PREFER_HEADER};

// ============================================================================
// Profile Negotiation Tests
// ============================================================================

#[test]
fn test_profile_preference_header_format() {
    let pref = ProfilePreference::default_preference();
    let header = pref.to_header_value();
    assert_eq!(header, "prism=hyper,turbo,classic");
}

#[test]
fn test_profile_preference_parsing() {
    let header = "prism=turbo,classic";
    let pref = ProfilePreference::from_header_value(header).expect("Should parse");
    assert_eq!(pref.profiles().len(), 2);
    assert_eq!(pref.profiles()[0], PrismProfile::Turbo);
    assert_eq!(pref.profiles()[1], PrismProfile::Classic);
}

#[test]
fn test_negotiate_client_prefers_hyper() {
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=hyper,turbo,classic".parse().unwrap());

    let support = ProfileSupport::all();
    let result = negotiate_profile(&headers, &support);

    assert_eq!(result, NegotiationResult::Negotiated(PrismProfile::Hyper));
    assert_eq!(result.profile(), Some(PrismProfile::Hyper));
}

#[test]
fn test_negotiate_client_prefers_turbo_server_no_hyper() {
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=hyper,turbo".parse().unwrap());

    // Server doesn't support Hyper
    let support = ProfileSupport::classic_and_turbo();
    let result = negotiate_profile(&headers, &support);

    // Should fall back to Turbo
    assert_eq!(result, NegotiationResult::Negotiated(PrismProfile::Turbo));
}

#[test]
fn test_negotiate_no_common_profile() {
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=hyper".parse().unwrap());

    // Server only supports Classic
    let support = ProfileSupport::classic_only();
    let result = negotiate_profile(&headers, &support);

    assert_eq!(result, NegotiationResult::NoCommonProfile);
    assert!(result.profile().is_none());
}

#[test]
fn test_negotiate_no_prefer_header() {
    let headers = HeaderMap::new();
    let support = ProfileSupport::all();
    let result = negotiate_profile(&headers, &support);

    // Should use server's best profile
    assert!(matches!(result, NegotiationResult::Default(PrismProfile::Hyper)));
}

#[test]
fn test_negotiate_minimum_profile_enforced() {
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=classic".parse().unwrap());

    // Server requires at least Turbo
    let support = ProfileSupport::all().with_minimum(PrismProfile::Turbo);
    let result = negotiate_profile(&headers, &support);

    match result {
        NegotiationResult::MinimumNotMet { requested, minimum } => {
            assert_eq!(requested, PrismProfile::Classic);
            assert_eq!(minimum, PrismProfile::Turbo);
        }
        _ => panic!("Expected MinimumNotMet"),
    }
}

#[test]
fn test_selected_prism_header_value() {
    let result = NegotiationResult::Negotiated(PrismProfile::Turbo);
    let header = result.to_header_value().expect("Should create header");
    assert_eq!(header.to_str().unwrap(), "turbo");
}

// ============================================================================
// Profile Support Configuration Tests
// ============================================================================

#[test]
fn test_profile_support_all() {
    let support = ProfileSupport::all();
    assert!(support.supports(PrismProfile::Classic));
    assert!(support.supports(PrismProfile::Turbo));
    assert!(support.supports(PrismProfile::Hyper));
    assert!(support.minimum().is_none());
}

#[test]
fn test_profile_support_classic_only() {
    let support = ProfileSupport::classic_only();
    assert!(support.supports(PrismProfile::Classic));
    assert!(!support.supports(PrismProfile::Turbo));
    assert!(!support.supports(PrismProfile::Hyper));
    assert_eq!(support.minimum(), Some(PrismProfile::Classic));
}

#[test]
fn test_profile_support_custom() {
    let support = ProfileSupport::custom(vec![PrismProfile::Turbo, PrismProfile::Classic]);
    assert!(support.supports(PrismProfile::Classic));
    assert!(support.supports(PrismProfile::Turbo));
    assert!(!support.supports(PrismProfile::Hyper));
}

// ============================================================================
// Client Configuration Tests
// ============================================================================

#[tokio::test]
async fn test_client_with_default_profile_preference() {
    let _client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .build()
        .expect("Should build client");

    // Client should be configured (actual connection not tested here)
    assert!(true);
}

#[tokio::test]
async fn test_client_with_custom_profile_preference() {
    let pref = ProfilePreference::new(vec![PrismProfile::Turbo, PrismProfile::Classic]);

    let _client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .profile_preference(pref)
        .build()
        .expect("Should build client");

    assert!(true);
}

#[tokio::test]
async fn test_client_with_classic_only() {
    let pref = ProfilePreference::new(vec![PrismProfile::Classic]);

    let _client = QuillClient::builder()
        .base_url("http://localhost:8080")
        .profile_preference(pref)
        .build()
        .expect("Should build client");

    assert!(true);
}

// ============================================================================
// Profile Weight Tests
// ============================================================================

#[test]
fn test_profile_weights() {
    // Hyper should have highest weight (most preferred)
    assert!(PrismProfile::Hyper.weight() > PrismProfile::Turbo.weight());
    assert!(PrismProfile::Turbo.weight() > PrismProfile::Classic.weight());
}

#[test]
fn test_profile_features() {
    // Only Hyper supports datagrams and 0-RTT
    assert!(PrismProfile::Hyper.supports_datagrams());
    assert!(PrismProfile::Hyper.supports_zero_rtt());

    assert!(!PrismProfile::Turbo.supports_datagrams());
    assert!(!PrismProfile::Turbo.supports_zero_rtt());

    assert!(!PrismProfile::Classic.supports_datagrams());
    assert!(!PrismProfile::Classic.supports_zero_rtt());
}

// ============================================================================
// Profile String Conversion Tests
// ============================================================================

#[test]
fn test_profile_display() {
    assert_eq!(PrismProfile::Classic.to_string(), "classic");
    assert_eq!(PrismProfile::Turbo.to_string(), "turbo");
    assert_eq!(PrismProfile::Hyper.to_string(), "hyper");
}

#[test]
fn test_profile_from_str() {
    use std::str::FromStr;

    assert_eq!(PrismProfile::from_str("classic").unwrap(), PrismProfile::Classic);
    assert_eq!(PrismProfile::from_str("TURBO").unwrap(), PrismProfile::Turbo);
    assert_eq!(PrismProfile::from_str("Hyper").unwrap(), PrismProfile::Hyper);
    assert!(PrismProfile::from_str("unknown").is_err());
}

// ============================================================================
// Integration Scenario Tests
// ============================================================================

#[test]
fn test_scenario_enterprise_proxy() {
    // Enterprise proxy scenario: client behind legacy proxy
    // Client prefers Turbo but accepts Classic
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=turbo,classic".parse().unwrap());

    // Server in enterprise environment only offers Classic
    let support = ProfileSupport::classic_only();
    let result = negotiate_profile(&headers, &support);

    assert_eq!(result, NegotiationResult::Negotiated(PrismProfile::Classic));
}

#[test]
fn test_scenario_internal_cluster() {
    // Internal cluster scenario: high-performance internal traffic
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=turbo".parse().unwrap());

    // Server optimized for Turbo
    let support = ProfileSupport::custom(vec![PrismProfile::Turbo]);
    let result = negotiate_profile(&headers, &support);

    assert_eq!(result, NegotiationResult::Negotiated(PrismProfile::Turbo));
}

#[test]
fn test_scenario_mobile_client() {
    // Mobile client scenario: prefer Hyper for better mobile performance
    let mut headers = HeaderMap::new();
    headers.insert(PREFER_HEADER, "prism=hyper,turbo,classic".parse().unwrap());

    // Edge server supports all profiles
    let support = ProfileSupport::all();
    let result = negotiate_profile(&headers, &support);

    assert_eq!(result, NegotiationResult::Negotiated(PrismProfile::Hyper));
}

#[test]
fn test_scenario_browser_to_edge_to_internal() {
    // Browser -> Edge (H3) -> Internal (H2) scenario
    // Edge to browser: Hyper
    let mut browser_headers = HeaderMap::new();
    browser_headers.insert(PREFER_HEADER, "prism=hyper,turbo".parse().unwrap());

    let edge_support = ProfileSupport::all();
    let edge_result = negotiate_profile(&browser_headers, &edge_support);
    assert_eq!(edge_result.profile(), Some(PrismProfile::Hyper));

    // Edge to internal: Turbo (H2)
    let mut edge_headers = HeaderMap::new();
    edge_headers.insert(PREFER_HEADER, "prism=turbo".parse().unwrap());

    let internal_support = ProfileSupport::classic_and_turbo();
    let internal_result = negotiate_profile(&edge_headers, &internal_support);
    assert_eq!(internal_result.profile(), Some(PrismProfile::Turbo));
}
