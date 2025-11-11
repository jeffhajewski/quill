//! Enhanced observability module for Quill
//!
//! Provides comprehensive metrics, health checks, and monitoring capabilities

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Prometheus-compatible metrics collector
#[derive(Clone)]
pub struct ObservabilityCollector {
    inner: Arc<ObservabilityInner>,
}

struct ObservabilityInner {
    // Request metrics
    requests_total: AtomicU64,
    requests_in_flight: AtomicU64,
    requests_failed: AtomicU64,

    // Latency tracking
    latency_sum_ms: AtomicU64,
    latency_count: AtomicU64,

    // Response size tracking
    response_bytes_total: AtomicU64,
    request_bytes_total: AtomicU64,

    // Per-endpoint metrics
    endpoint_metrics: RwLock<HashMap<String, EndpointMetrics>>,

    // Health status
    health_status: RwLock<HealthStatus>,

    // Start time
    start_time: Instant,
}

#[derive(Debug, Clone)]
struct EndpointMetrics {
    requests: u64,
    errors: u64,
    latency_sum_ms: u64,
    latency_count: u64,
}

#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub dependencies: HashMap<String, DependencyStatus>,
    pub last_check: Instant,
}

#[derive(Debug, Clone)]
pub struct DependencyStatus {
    pub name: String,
    pub healthy: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

impl ObservabilityCollector {
    /// Create a new observability collector
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ObservabilityInner {
                requests_total: AtomicU64::new(0),
                requests_in_flight: AtomicU64::new(0),
                requests_failed: AtomicU64::new(0),
                latency_sum_ms: AtomicU64::new(0),
                latency_count: AtomicU64::new(0),
                response_bytes_total: AtomicU64::new(0),
                request_bytes_total: AtomicU64::new(0),
                endpoint_metrics: RwLock::new(HashMap::new()),
                health_status: RwLock::new(HealthStatus {
                    healthy: true,
                    dependencies: HashMap::new(),
                    last_check: Instant::now(),
                }),
                start_time: Instant::now(),
            }),
        }
    }

    /// Record a request start
    pub fn record_request_start(&self, endpoint: &str, request_bytes: usize) {
        self.inner.requests_total.fetch_add(1, Ordering::Relaxed);
        self.inner.requests_in_flight.fetch_add(1, Ordering::Relaxed);
        self.inner.request_bytes_total.fetch_add(request_bytes as u64, Ordering::Relaxed);
    }

    /// Record a request completion
    pub async fn record_request_complete(
        &self,
        endpoint: &str,
        duration: Duration,
        response_bytes: usize,
        success: bool,
    ) {
        self.inner.requests_in_flight.fetch_sub(1, Ordering::Relaxed);
        self.inner.response_bytes_total.fetch_add(response_bytes as u64, Ordering::Relaxed);

        let latency_ms = duration.as_millis() as u64;
        self.inner.latency_sum_ms.fetch_add(latency_ms, Ordering::Relaxed);
        self.inner.latency_count.fetch_add(1, Ordering::Relaxed);

        if !success {
            self.inner.requests_failed.fetch_add(1, Ordering::Relaxed);
        }

        // Update per-endpoint metrics
        let mut metrics = self.inner.endpoint_metrics.write().await;
        let endpoint_metric = metrics.entry(endpoint.to_string()).or_insert(EndpointMetrics {
            requests: 0,
            errors: 0,
            latency_sum_ms: 0,
            latency_count: 0,
        });

        endpoint_metric.requests += 1;
        endpoint_metric.latency_sum_ms += latency_ms;
        endpoint_metric.latency_count += 1;
        if !success {
            endpoint_metric.errors += 1;
        }
    }

    /// Update health status
    pub async fn update_health(&self, healthy: bool, dependencies: HashMap<String, DependencyStatus>) {
        let mut health = self.inner.health_status.write().await;
        health.healthy = healthy;
        health.dependencies = dependencies;
        health.last_check = Instant::now();
    }

    /// Get current health status
    pub async fn get_health(&self) -> HealthStatus {
        self.inner.health_status.read().await.clone()
    }

    /// Export metrics in Prometheus text format
    pub async fn export_prometheus(&self) -> String {
        let mut output = String::new();

        // Help and type declarations
        output.push_str("# HELP quill_requests_total Total number of requests\n");
        output.push_str("# TYPE quill_requests_total counter\n");
        output.push_str(&format!(
            "quill_requests_total {}\n",
            self.inner.requests_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP quill_requests_in_flight Current number of requests being processed\n");
        output.push_str("# TYPE quill_requests_in_flight gauge\n");
        output.push_str(&format!(
            "quill_requests_in_flight {}\n",
            self.inner.requests_in_flight.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP quill_requests_failed_total Total number of failed requests\n");
        output.push_str("# TYPE quill_requests_failed_total counter\n");
        output.push_str(&format!(
            "quill_requests_failed_total {}\n",
            self.inner.requests_failed.load(Ordering::Relaxed)
        ));

        // Average latency
        let latency_sum = self.inner.latency_sum_ms.load(Ordering::Relaxed);
        let latency_count = self.inner.latency_count.load(Ordering::Relaxed);
        let avg_latency = if latency_count > 0 {
            latency_sum as f64 / latency_count as f64
        } else {
            0.0
        };

        output.push_str("# HELP quill_request_duration_ms Average request duration in milliseconds\n");
        output.push_str("# TYPE quill_request_duration_ms gauge\n");
        output.push_str(&format!("quill_request_duration_ms {:.2}\n", avg_latency));

        // Bytes transferred
        output.push_str("# HELP quill_request_bytes_total Total request bytes received\n");
        output.push_str("# TYPE quill_request_bytes_total counter\n");
        output.push_str(&format!(
            "quill_request_bytes_total {}\n",
            self.inner.request_bytes_total.load(Ordering::Relaxed)
        ));

        output.push_str("# HELP quill_response_bytes_total Total response bytes sent\n");
        output.push_str("# TYPE quill_response_bytes_total counter\n");
        output.push_str(&format!(
            "quill_response_bytes_total {}\n",
            self.inner.response_bytes_total.load(Ordering::Relaxed)
        ));

        // Uptime
        let uptime_seconds = self.inner.start_time.elapsed().as_secs();
        output.push_str("# HELP quill_uptime_seconds Server uptime in seconds\n");
        output.push_str("# TYPE quill_uptime_seconds counter\n");
        output.push_str(&format!("quill_uptime_seconds {}\n", uptime_seconds));

        // Per-endpoint metrics
        let endpoint_metrics = self.inner.endpoint_metrics.read().await;
        if !endpoint_metrics.is_empty() {
            output.push_str("# HELP quill_endpoint_requests_total Requests per endpoint\n");
            output.push_str("# TYPE quill_endpoint_requests_total counter\n");
            for (endpoint, metrics) in endpoint_metrics.iter() {
                output.push_str(&format!(
                    "quill_endpoint_requests_total{{endpoint=\"{}\"}} {}\n",
                    endpoint, metrics.requests
                ));
            }

            output.push_str("# HELP quill_endpoint_errors_total Errors per endpoint\n");
            output.push_str("# TYPE quill_endpoint_errors_total counter\n");
            for (endpoint, metrics) in endpoint_metrics.iter() {
                output.push_str(&format!(
                    "quill_endpoint_errors_total{{endpoint=\"{}\"}} {}\n",
                    endpoint, metrics.errors
                ));
            }

            output.push_str("# HELP quill_endpoint_latency_ms Average latency per endpoint\n");
            output.push_str("# TYPE quill_endpoint_latency_ms gauge\n");
            for (endpoint, metrics) in endpoint_metrics.iter() {
                let avg = if metrics.latency_count > 0 {
                    metrics.latency_sum_ms as f64 / metrics.latency_count as f64
                } else {
                    0.0
                };
                output.push_str(&format!(
                    "quill_endpoint_latency_ms{{endpoint=\"{}\"}} {:.2}\n",
                    endpoint, avg
                ));
            }
        }

        // Health status
        let health = self.inner.health_status.read().await;
        output.push_str("# HELP quill_health_status Overall health status (1=healthy, 0=unhealthy)\n");
        output.push_str("# TYPE quill_health_status gauge\n");
        output.push_str(&format!("quill_health_status {}\n", if health.healthy { 1 } else { 0 }));

        // Dependency health
        if !health.dependencies.is_empty() {
            output.push_str("# HELP quill_dependency_health Dependency health status\n");
            output.push_str("# TYPE quill_dependency_health gauge\n");
            for (name, dep) in health.dependencies.iter() {
                output.push_str(&format!(
                    "quill_dependency_health{{dependency=\"{}\"}} {}\n",
                    name,
                    if dep.healthy { 1 } else { 0 }
                ));
            }
        }

        output
    }

    /// Export metrics as JSON
    pub async fn export_json(&self) -> serde_json::Value {
        let endpoint_metrics = self.inner.endpoint_metrics.read().await;
        let health = self.inner.health_status.read().await;

        let latency_sum = self.inner.latency_sum_ms.load(Ordering::Relaxed);
        let latency_count = self.inner.latency_count.load(Ordering::Relaxed);
        let avg_latency = if latency_count > 0 {
            latency_sum as f64 / latency_count as f64
        } else {
            0.0
        };

        serde_json::json!({
            "requests": {
                "total": self.inner.requests_total.load(Ordering::Relaxed),
                "in_flight": self.inner.requests_in_flight.load(Ordering::Relaxed),
                "failed": self.inner.requests_failed.load(Ordering::Relaxed),
            },
            "latency": {
                "average_ms": avg_latency,
            },
            "bytes": {
                "request_total": self.inner.request_bytes_total.load(Ordering::Relaxed),
                "response_total": self.inner.response_bytes_total.load(Ordering::Relaxed),
            },
            "uptime_seconds": self.inner.start_time.elapsed().as_secs(),
            "endpoints": endpoint_metrics.iter().map(|(name, m)| {
                let avg = if m.latency_count > 0 {
                    m.latency_sum_ms as f64 / m.latency_count as f64
                } else {
                    0.0
                };
                serde_json::json!({
                    "name": name,
                    "requests": m.requests,
                    "errors": m.errors,
                    "average_latency_ms": avg,
                })
            }).collect::<Vec<_>>(),
            "health": {
                "healthy": health.healthy,
                "dependencies": health.dependencies.iter().map(|(name, dep)| {
                    serde_json::json!({
                        "name": name,
                        "healthy": dep.healthy,
                        "latency_ms": dep.latency_ms,
                        "error": dep.error,
                    })
                }).collect::<Vec<_>>(),
            }
        })
    }
}

impl Default for ObservabilityCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to check dependency health
pub async fn check_dependency(
    name: &str,
    check_fn: impl std::future::Future<Output = Result<(), String>>,
) -> DependencyStatus {
    let start = Instant::now();
    match check_fn.await {
        Ok(()) => DependencyStatus {
            name: name.to_string(),
            healthy: true,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(e) => DependencyStatus {
            name: name.to_string(),
            healthy: false,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            error: Some(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = ObservabilityCollector::new();

        collector.record_request_start("/test", 100);
        collector
            .record_request_complete("/test", Duration::from_millis(50), 200, true)
            .await;

        let prometheus = collector.export_prometheus().await;
        assert!(prometheus.contains("quill_requests_total 1"));
        assert!(prometheus.contains("quill_requests_in_flight 0"));
    }

    #[tokio::test]
    async fn test_endpoint_metrics() {
        let collector = ObservabilityCollector::new();

        collector.record_request_start("/endpoint1", 100);
        collector
            .record_request_complete("/endpoint1", Duration::from_millis(50), 200, true)
            .await;

        collector.record_request_start("/endpoint2", 150);
        collector
            .record_request_complete("/endpoint2", Duration::from_millis(75), 250, false)
            .await;

        let json = collector.export_json().await;
        assert_eq!(json["requests"]["total"], 2);
        assert_eq!(json["requests"]["failed"], 1);
        assert_eq!(json["endpoints"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_health_status() {
        let collector = ObservabilityCollector::new();

        let mut deps = HashMap::new();
        deps.insert(
            "database".to_string(),
            DependencyStatus {
                name: "database".to_string(),
                healthy: true,
                latency_ms: Some(10),
                error: None,
            },
        );

        collector.update_health(true, deps).await;

        let health = collector.get_health().await;
        assert!(health.healthy);
        assert_eq!(health.dependencies.len(), 1);
    }

    #[tokio::test]
    async fn test_dependency_check() {
        let dep = check_dependency("test", async { Ok(()) }).await;
        assert!(dep.healthy);
        assert!(dep.latency_ms.is_some());

        let dep = check_dependency("test", async { Err("connection failed".to_string()) }).await;
        assert!(!dep.healthy);
        assert!(dep.error.is_some());
    }
}
