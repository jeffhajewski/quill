//! HTTP/3 Datagram Example
//!
//! This example demonstrates unreliable datagram messaging over HTTP/3/QUIC.
//! Datagrams are ideal for real-time sensor data, gaming updates, and telemetry
//! where occasional packet loss is acceptable.
//!
//! # Features Demonstrated
//!
//! - Sending datagrams from client to server
//! - Server-side datagram echo handler
//! - Flow IDs for multiplexing different data streams
//! - Datagram size limits and validation
//!
//! # Use Cases
//!
//! - Real-time sensor data streaming
//! - Gaming state updates
//! - Video/audio packets
//! - Telemetry and metrics collection

use bytes::Bytes;
use quill_transport::{
    Datagram, DatagramHandler, DatagramSender, FnDatagramHandler, H3ClientBuilder, H3ServerBuilder,
    HyperError,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Sensor data flow IDs for multiplexing
pub const FLOW_TEMPERATURE: u64 = 1;
pub const FLOW_HUMIDITY: u64 = 2;
pub const FLOW_PRESSURE: u64 = 3;

/// A simple sensor reading
#[derive(Debug, Clone)]
pub struct SensorReading {
    pub sensor_type: SensorType,
    pub value: f32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorType {
    Temperature,
    Humidity,
    Pressure,
}

impl SensorType {
    pub fn flow_id(&self) -> u64 {
        match self {
            SensorType::Temperature => FLOW_TEMPERATURE,
            SensorType::Humidity => FLOW_HUMIDITY,
            SensorType::Pressure => FLOW_PRESSURE,
        }
    }

    pub fn from_flow_id(flow_id: u64) -> Option<Self> {
        match flow_id {
            FLOW_TEMPERATURE => Some(SensorType::Temperature),
            FLOW_HUMIDITY => Some(SensorType::Humidity),
            FLOW_PRESSURE => Some(SensorType::Pressure),
            _ => None,
        }
    }
}

impl SensorReading {
    /// Create a new sensor reading
    pub fn new(sensor_type: SensorType, value: f32, timestamp: u64) -> Self {
        Self {
            sensor_type,
            value,
            timestamp,
        }
    }

    /// Encode the reading to bytes
    pub fn encode(&self) -> Bytes {
        let mut buf = Vec::with_capacity(12);
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        Bytes::from(buf)
    }

    /// Decode a reading from bytes
    pub fn decode(data: &[u8], sensor_type: SensorType) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }
        let value = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let timestamp = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);
        Some(Self {
            sensor_type,
            value,
            timestamp,
        })
    }

    /// Convert to a datagram with flow ID
    pub fn to_datagram(&self) -> Datagram {
        Datagram::with_flow_id(self.encode(), self.sensor_type.flow_id())
    }
}

/// Server-side statistics for received datagrams
#[derive(Default)]
pub struct DatagramStats {
    pub temperature_count: AtomicU64,
    pub humidity_count: AtomicU64,
    pub pressure_count: AtomicU64,
    pub unknown_count: AtomicU64,
}

impl DatagramStats {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn increment(&self, sensor_type: Option<SensorType>) {
        match sensor_type {
            Some(SensorType::Temperature) => {
                self.temperature_count.fetch_add(1, Ordering::Relaxed);
            }
            Some(SensorType::Humidity) => {
                self.humidity_count.fetch_add(1, Ordering::Relaxed);
            }
            Some(SensorType::Pressure) => {
                self.pressure_count.fetch_add(1, Ordering::Relaxed);
            }
            None => {
                self.unknown_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn total(&self) -> u64 {
        self.temperature_count.load(Ordering::Relaxed)
            + self.humidity_count.load(Ordering::Relaxed)
            + self.pressure_count.load(Ordering::Relaxed)
            + self.unknown_count.load(Ordering::Relaxed)
    }
}

/// Create an echo datagram handler that echoes back received datagrams
///
/// This is useful for testing round-trip latency and connectivity.
pub fn create_echo_handler(stats: Arc<DatagramStats>) -> impl DatagramHandler {
    FnDatagramHandler::new(move |dg: Datagram, sender: DatagramSender| {
        // Track statistics based on flow ID
        let sensor_type = dg.flow_id.and_then(SensorType::from_flow_id);
        stats.increment(sensor_type);

        // Echo the datagram back
        if let Err(e) = sender.send(dg) {
            tracing::warn!("Failed to echo datagram: {}", e);
        }
    })
}

/// Create a logging datagram handler that logs received sensor data
pub fn create_logging_handler(stats: Arc<DatagramStats>) -> impl DatagramHandler {
    FnDatagramHandler::new(move |dg: Datagram, _sender: DatagramSender| {
        if let Some(flow_id) = dg.flow_id {
            if let Some(sensor_type) = SensorType::from_flow_id(flow_id) {
                if let Some(reading) = SensorReading::decode(&dg.payload, sensor_type) {
                    tracing::info!(
                        "Received {:?} reading: {:.2} at timestamp {}",
                        reading.sensor_type,
                        reading.value,
                        reading.timestamp
                    );
                }
            }
        }
        stats.increment(dg.flow_id.and_then(SensorType::from_flow_id));
    })
}

/// Configuration for the datagram example
pub struct DatagramExampleConfig {
    pub bind_addr: SocketAddr,
    pub max_datagram_size: usize,
}

impl Default for DatagramExampleConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:4433".parse().unwrap(),
            max_datagram_size: 1200, // MTU-safe size
        }
    }
}

/// Build an H3 client configured for datagrams
pub fn build_datagram_client(_config: &DatagramExampleConfig) -> Result<quill_transport::H3Client, HyperError> {
    // Install crypto provider if not already installed
    let _ = rustls::crypto::ring::default_provider().install_default();

    H3ClientBuilder::new()
        .enable_datagrams(true)
        .enable_zero_rtt(false) // Datagrams don't need 0-RTT
        .build()
}

/// Build an H3 server configured for datagrams
pub fn build_datagram_server(
    config: &DatagramExampleConfig,
) -> Result<quill_transport::H3Server, HyperError> {
    H3ServerBuilder::new(config.bind_addr)
        .enable_datagrams(true)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_reading_encode_decode() {
        let reading = SensorReading::new(SensorType::Temperature, 72.5, 1234567890);
        let encoded = reading.encode();

        let decoded = SensorReading::decode(&encoded, SensorType::Temperature).unwrap();
        assert_eq!(decoded.value, 72.5);
        assert_eq!(decoded.timestamp, 1234567890);
    }

    #[test]
    fn test_sensor_reading_to_datagram() {
        let reading = SensorReading::new(SensorType::Humidity, 45.0, 1000);
        let dg = reading.to_datagram();

        assert_eq!(dg.flow_id, Some(FLOW_HUMIDITY));
        assert_eq!(dg.size(), 12); // 4 bytes value + 8 bytes timestamp
    }

    #[test]
    fn test_sensor_type_flow_id_roundtrip() {
        assert_eq!(
            SensorType::from_flow_id(SensorType::Temperature.flow_id()),
            Some(SensorType::Temperature)
        );
        assert_eq!(
            SensorType::from_flow_id(SensorType::Humidity.flow_id()),
            Some(SensorType::Humidity)
        );
        assert_eq!(
            SensorType::from_flow_id(SensorType::Pressure.flow_id()),
            Some(SensorType::Pressure)
        );
        assert_eq!(SensorType::from_flow_id(999), None);
    }

    #[test]
    fn test_datagram_stats() {
        let stats = DatagramStats::new();

        stats.increment(Some(SensorType::Temperature));
        stats.increment(Some(SensorType::Temperature));
        stats.increment(Some(SensorType::Humidity));
        stats.increment(None);

        assert_eq!(stats.temperature_count.load(Ordering::Relaxed), 2);
        assert_eq!(stats.humidity_count.load(Ordering::Relaxed), 1);
        assert_eq!(stats.pressure_count.load(Ordering::Relaxed), 0);
        assert_eq!(stats.unknown_count.load(Ordering::Relaxed), 1);
        assert_eq!(stats.total(), 4);
    }

    #[test]
    fn test_datagram_config_defaults() {
        let config = DatagramExampleConfig::default();

        assert_eq!(config.bind_addr.port(), 4433);
        assert_eq!(config.max_datagram_size, 1200);
    }

    #[tokio::test]
    async fn test_build_client() {
        let config = DatagramExampleConfig::default();
        let client = build_datagram_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(client.config().enable_datagrams);
    }

    #[test]
    fn test_build_server() {
        let config = DatagramExampleConfig::default();
        let server = build_datagram_server(&config);

        assert!(server.is_ok());
        let server = server.unwrap();
        assert!(server.config().enable_datagrams);
    }

    #[test]
    fn test_echo_handler_creation() {
        let stats = DatagramStats::new();
        let _handler = create_echo_handler(stats);
        // Handler creation should succeed
    }

    #[test]
    fn test_logging_handler_creation() {
        let stats = DatagramStats::new();
        let _handler = create_logging_handler(stats);
        // Handler creation should succeed
    }
}
