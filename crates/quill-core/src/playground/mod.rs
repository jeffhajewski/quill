//! Playground mode types for distributed systems visualization and control.
//!
//! This module provides core types used by the playground feature:
//! - Configuration for simulation rules (latency, partitions, clock drift)
//! - Telemetry events for dashboard visualization
//! - Intercept context for request metadata
//! - ToDebugJson trait for message serialization

pub mod config;
pub mod context;
pub mod debug;
pub mod event;

pub use config::{
    ClockDirection, ClockDriftConfig, LatencyRule, PartitionBehavior, PartitionError,
    PartitionRule, PlaygroundConfig, RuleSchedule, TelemetryConfig,
};
pub use context::InterceptContext;
pub use debug::ToDebugJson;
pub use event::PlaygroundEvent;
