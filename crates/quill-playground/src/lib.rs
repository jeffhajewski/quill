//! Quill Playground Mode
//!
//! Playground mode enables distributed systems visualization and control for
//! development and testing. Features include:
//!
//! - **Latency Injection**: Add artificial delays to RPC calls
//! - **Network Partition Simulation**: Drop or timeout requests between nodes
//! - **Clock Drift**: Simulate time skew for testing time-sensitive logic
//! - **Real-time Telemetry**: Stream events to a visualization dashboard
//!
//! # Quick Start
//!
//! ```ignore
//! use quill_playground::{init, PlaygroundConfig};
//!
//! let controller = quill_playground::init(PlaygroundConfig::builder()
//!     .enabled(true)
//!     .node_id("node-1")
//!     .build()
//! );
//!
//! // Use with QuillClient
//! let client = QuillClient::builder()
//!     .base_url("http://localhost:8080")
//!     .playground(controller.config().clone())
//!     .build()?;
//! ```
//!
//! # Architecture
//!
//! The playground system consists of:
//!
//! - **PlaygroundController**: Central coordinator that manages rules and emits events
//! - **Interceptors**: Request/response middleware for applying rules
//! - **TelemetrySidecar**: Background task that sends events to the dashboard
//! - **Time abstraction**: Functions that can simulate clock drift

pub mod controller;
pub mod error;
pub mod interceptor;
pub mod time;

#[cfg(feature = "websocket")]
pub mod sidecar;

pub use controller::PlaygroundController;
pub use error::PlaygroundError;
pub use interceptor::{Interceptor, InterceptorChain};

// Re-export core types for convenience
pub use quill_core::playground::{
    ClockDirection, ClockDriftConfig, InterceptContext, LatencyRule, PartitionBehavior,
    PartitionRule, PlaygroundConfig, PlaygroundEvent, RuleSchedule, TelemetryConfig, ToDebugJson,
};

/// Initialize playground mode with the given configuration.
///
/// This creates a `PlaygroundController` and optionally starts the telemetry
/// sidecar if a dashboard URL is configured.
///
/// # Example
///
/// ```
/// use quill_playground::{init, PlaygroundConfig};
///
/// let controller = init(PlaygroundConfig::builder()
///     .enabled(true)
///     .node_id("my-service-1")
///     .build()
/// );
///
/// // Check if playground is enabled
/// if controller.is_enabled() {
///     println!("Playground mode active!");
/// }
/// ```
pub fn init(config: PlaygroundConfig) -> PlaygroundController {
    PlaygroundController::new(config)
}
