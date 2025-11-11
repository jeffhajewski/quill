# Observability Guide

This guide covers comprehensive observability for Quill services, including metrics, health checks, tracing, and alerting.

## Table of Contents

- [Overview](#overview)
- [Metrics Collection](#metrics-collection)
- [Health Checks](#health-checks)
- [Grafana Dashboards](#grafana-dashboards)
- [Alerting](#alerting)
- [Tracing](#tracing)
- [Best Practices](#best-practices)

## Overview

Quill provides built-in observability features:

- **Prometheus-compatible metrics** - Request rates, latency, errors, throughput
- **Health checks** - Service and dependency health monitoring
- **Distributed tracing** - OpenTelemetry integration
- **Grafana dashboards** - Pre-built visualization dashboards
- **Alerting rules** - Production-ready Prometheus alerts

## Metrics Collection

### Using the ObservabilityCollector

```rust
use quill_server::{ObservabilityCollector, ServerBuilder};
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create observability collector
    let metrics = Arc::new(ObservabilityCollector::new());

    // Register metrics endpoint
    let server = ServerBuilder::new()
        .register("metrics", {
            let m = metrics.clone();
            move |_| {
                let m = m.clone();
                async move {
                    // Export Prometheus text format
                    let prometheus_text = m.export_prometheus().await;

                    Ok(http::Response::builder()
                        .status(200)
                        .header("Content-Type", "text/plain; version=0.0.4")
                        .body(bytes::Bytes::from(prometheus_text))
                        .unwrap())
                }
            }
        })
        .register("echo.v1.EchoService/Echo", {
            let m = metrics.clone();
            move |req| {
                let m = m.clone();
                async move {
                    let start = Instant::now();
                    let endpoint = "echo.v1.EchoService/Echo";

                    // Record request start
                    m.record_request_start(endpoint, req.len());

                    // Handle request
                    let result = handle_echo(req).await;

                    // Record completion
                    let success = result.is_ok();
                    let response_size = result.as_ref().map(|r| r.len()).unwrap_or(0);
                    m.record_request_complete(
                        endpoint,
                        start.elapsed(),
                        response_size,
                        success
                    ).await;

                    result
                }
            }
        })
        .build();

    server.serve("0.0.0.0:8080").await?;
    Ok(())
}
```

### Available Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `quill_requests_total` | Counter | Total number of requests |
| `quill_requests_in_flight` | Gauge | Current active requests |
| `quill_requests_failed_total` | Counter | Total failed requests |
| `quill_request_duration_ms` | Gauge | Average request duration |
| `quill_request_bytes_total` | Counter | Total request bytes received |
| `quill_response_bytes_total` | Counter | Total response bytes sent |
| `quill_uptime_seconds` | Counter | Server uptime |
| `quill_endpoint_requests_total` | Counter | Requests per endpoint |
| `quill_endpoint_errors_total` | Counter | Errors per endpoint |
| `quill_endpoint_latency_ms` | Gauge | Average latency per endpoint |
| `quill_health_status` | Gauge | Overall health (1=healthy, 0=unhealthy) |
| `quill_dependency_health` | Gauge | Dependency health status |

### JSON Metrics Export

For custom monitoring systems, export metrics as JSON:

```rust
// Export metrics as JSON
let json_metrics = metrics.export_json().await;
println!("{}", serde_json::to_string_pretty(&json_metrics)?);
```

Example JSON output:

```json
{
  "requests": {
    "total": 1523,
    "in_flight": 5,
    "failed": 12
  },
  "latency": {
    "average_ms": 45.2
  },
  "bytes": {
    "request_total": 152300,
    "response_total": 304600
  },
  "uptime_seconds": 3600,
  "endpoints": [
    {
      "name": "echo.v1.EchoService/Echo",
      "requests": 1200,
      "errors": 5,
      "average_latency_ms": 42.1
    }
  ],
  "health": {
    "healthy": true,
    "dependencies": []
  }
}
```

## Health Checks

### Implementing Health Endpoints

```rust
use quill_server::{check_dependency, DependencyStatus, ObservabilityCollector};
use std::collections::HashMap;

async fn health_handler(
    metrics: Arc<ObservabilityCollector>,
) -> Result<http::Response<bytes::Bytes>, quill_core::QuillError> {
    let health = metrics.get_health().await;

    let status = if health.healthy { 200 } else { 503 };

    Ok(http::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(bytes::Bytes::from(serde_json::to_string(&health).unwrap()))
        .unwrap())
}

async fn readiness_handler(
    metrics: Arc<ObservabilityCollector>,
) -> Result<http::Response<bytes::Bytes>, quill_core::QuillError> {
    // Check all dependencies
    let mut dependencies = HashMap::new();

    // Check database
    dependencies.insert(
        "database".to_string(),
        check_dependency("database", check_database_connection()).await
    );

    // Check Redis
    dependencies.insert(
        "cache".to_string(),
        check_dependency("cache", check_redis_connection()).await
    );

    // Check external API
    dependencies.insert(
        "external_api".to_string(),
        check_dependency("external_api", check_external_api()).await
    );

    // Update health status
    let all_healthy = dependencies.values().all(|d| d.healthy);
    metrics.update_health(all_healthy, dependencies.clone()).await;

    let status = if all_healthy { 200 } else { 503 };

    Ok(http::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(bytes::Bytes::from(serde_json::json!({
            "healthy": all_healthy,
            "dependencies": dependencies
        }).to_string()))
        .unwrap())
}

async fn check_database_connection() -> Result<(), String> {
    // Implement your database check
    // Example: execute a simple query
    Ok(())
}

async fn check_redis_connection() -> Result<(), String> {
    // Implement your Redis check
    // Example: PING command
    Ok(())
}

async fn check_external_api() -> Result<(), String> {
    // Implement your external API check
    // Example: HTTP HEAD request
    Ok(())
}
```

### Kubernetes Health Check Configuration

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 15
  periodSeconds: 20
  timeoutSeconds: 3
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 10
  timeoutSeconds: 3
  failureThreshold: 3
```

## Grafana Dashboards

### Pre-built Dashboard

Quill provides a comprehensive Grafana dashboard at `deployment/examples/monitoring/grafana/dashboards/quill-overview.json`.

**Features:**
- Request rate and error rate
- Latency graphs (average, p95, p99)
- Active connections gauge
- Health status indicator
- Network throughput
- Per-endpoint statistics
- Requests by endpoint pie chart

### Importing the Dashboard

1. **Docker Compose**: Dashboards auto-load from `grafana/dashboards/` directory

2. **Manual Import**:
   - Open Grafana (default: http://localhost:3000)
   - Navigate to Dashboards → Import
   - Upload `quill-overview.json`
   - Select Prometheus datasource

3. **Via API**:
   ```bash
   curl -X POST \
     -H "Content-Type: application/json" \
     -d @deployment/examples/monitoring/grafana/dashboards/quill-overview.json \
     http://admin:admin@localhost:3000/api/dashboards/db
   ```

### Dashboard Panels

| Panel | Visualization | Purpose |
|-------|---------------|---------|
| Request Rate | Time series | Monitor traffic patterns |
| Error Rate | Gauge | Quick error rate overview |
| Request Latency | Time series | Track response times |
| Active Connections | Gauge | Current load indicator |
| Health Status | Stat | Service health at-a-glance |
| Network Throughput | Time series | Bandwidth usage |
| Requests by Endpoint | Pie chart | Traffic distribution |
| Endpoint Statistics | Table | Detailed per-endpoint metrics |

### Custom Metrics

Add custom metrics to your service:

```rust
// In your handler
async fn handle_custom_metric(metrics: Arc<ObservabilityCollector>) {
    // Record custom event
    metrics.record_request_start("custom.event", 0);

    // ... do work ...

    metrics.record_request_complete(
        "custom.event",
        Duration::from_millis(100),
        0,
        true
    ).await;
}
```

## Alerting

### Prometheus Alert Rules

Load alert rules in Prometheus:

```yaml
# prometheus.yml
rule_files:
  - 'alerts.yml'
```

### Available Alerts

**Service Health Alerts:**
- `HighErrorRate` - Error rate > 5% for 5 minutes
- `CriticalErrorRate` - Error rate > 20% for 2 minutes
- `HighLatency` - Average latency > 500ms for 5 minutes
- `VeryHighLatency` - Average latency > 2000ms for 2 minutes
- `ServiceDown` - Service unreachable for 1 minute
- `HealthCheckFailing` - Health check failing for 2 minutes
- `DependencyUnhealthy` - Dependency unhealthy for 5 minutes

**Resource Alerts:**
- `HighMemoryUsage` - Memory usage > 85% for 5 minutes
- `HighCPUUsage` - CPU usage > 85% for 5 minutes
- `FrequentPodRestarts` - Frequent pod restarts detected

**SLO Alerts:**
- `SLOAvailabilityBreach` - Availability < 99.9% for 10 minutes
- `SLOLatencyBreach` - P99 latency > 500ms for 10 minutes

### Alert Manager Configuration

Configure Alertmanager to route alerts:

```yaml
# alertmanager.yml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname', 'component']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 12h
  receiver: 'team-quill'

  routes:
    - match:
        severity: critical
      receiver: 'pager-duty'
      continue: true

    - match:
        severity: warning
      receiver: 'slack'

receivers:
  - name: 'team-quill'
    email_configs:
      - to: 'team@example.com'

  - name: 'pager-duty'
    pagerduty_configs:
      - service_key: '<your-key>'

  - name: 'slack'
    slack_configs:
      - api_url: '<your-webhook>'
        channel: '#alerts'
```

## Tracing

### OpenTelemetry Integration

Quill has built-in OpenTelemetry tracing support:

```rust
use opentelemetry::global;
use opentelemetry::sdk::trace::{Config, Tracer};
use opentelemetry::sdk::Resource;
use opentelemetry_otlp::WithExportConfig;

fn init_tracing() -> Result<Tracer, Box<dyn std::error::Error>> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint("http://jaeger:4317")
        )
        .with_trace_config(
            Config::default()
                .with_resource(Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", "quill-service"),
                    opentelemetry::KeyValue::new("service.version", "1.0.0"),
                ]))
        )
        .install_batch(opentelemetry::runtime::Tokio)?;

    Ok(tracer)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _tracer = init_tracing()?;

    // Tracing is now automatic for all RPC calls

    Ok(())
}
```

### Viewing Traces

Access Jaeger UI (Docker Compose):
- URL: http://localhost:16686
- Select service: "quill-service"
- View traces, spans, and dependencies

## Best Practices

### 1. Always Expose Metrics

```rust
// Always register a /metrics endpoint
server.register("metrics", metrics_handler);
```

### 2. Implement Comprehensive Health Checks

Check all critical dependencies:

```rust
async fn readiness_check() {
    check_dependency("database", db_check()).await;
    check_dependency("cache", redis_check()).await;
    check_dependency("message_queue", kafka_check()).await;
}
```

### 3. Set Appropriate Alert Thresholds

Balance sensitivity vs. noise:

```yaml
# Warning: Early indicator
- alert: HighErrorRate
  expr: error_rate > 0.05  # 5%
  for: 5m  # Wait 5 minutes

# Critical: Immediate action
- alert: CriticalErrorRate
  expr: error_rate > 0.20  # 20%
  for: 2m  # Wait only 2 minutes
```

### 4. Use Structured Logging

```rust
use tracing::{info, error, warn};

info!(
    endpoint = "echo.v1.EchoService/Echo",
    latency_ms = 45,
    "Request completed"
);

error!(
    endpoint = "user.v1.UserService/GetUser",
    error = %err,
    "Request failed"
);
```

### 5. Monitor Golden Signals

Focus on the four golden signals:

1. **Latency** - How long requests take
2. **Traffic** - How many requests you're getting
3. **Errors** - Rate of failed requests
4. **Saturation** - How "full" your service is

### 6. Dashboard Organization

Organize dashboards by:
- **Overview** - High-level health
- **Service-specific** - Per-service details
- **Infrastructure** - Resource usage
- **SLOs** - Service level objectives

### 7. Alert Routing

Route alerts based on severity:

| Severity | Action | Channel |
|----------|--------|---------|
| Critical | Page on-call | PagerDuty |
| Warning | Notify team | Slack |
| Info | Log only | Logs |

### 8. Retention Policies

Set appropriate retention:

```yaml
# Prometheus
storage:
  tsdb:
    retention.time: 15d
    retention.size: 50GB

# Grafana
# Short-term: 15 days
# Long-term: Downsample to 1h and keep 1 year
```

### 9. Label Cardinality

Keep label cardinality low:

```rust
// Good: Low cardinality
quill_endpoint_requests_total{endpoint="/api/users"}

// Bad: High cardinality (user IDs change frequently)
quill_requests_total{user_id="12345"}
```

### 10. Regular Review

Review metrics and alerts regularly:
- Weekly: Dashboard relevance
- Monthly: Alert effectiveness
- Quarterly: SLO compliance

## Troubleshooting

### Metrics Not Appearing

1. Check Prometheus targets:
   ```
   http://localhost:9090/targets
   ```

2. Verify metrics endpoint:
   ```bash
   curl http://localhost:8080/metrics
   ```

3. Check Prometheus scrape config:
   ```yaml
   scrape_configs:
     - job_name: 'quill-services'
       static_configs:
         - targets: ['quill-service:8080']
   ```

### Grafana Dashboard Not Loading

1. Verify datasource connection
2. Check Prometheus query syntax
3. Ensure metrics exist in Prometheus

### Alerts Not Firing

1. Check alert rules syntax:
   ```bash
   promtool check rules alerts.yml
   ```

2. Verify alert is loaded in Prometheus:
   ```
   http://localhost:9090/alerts
   ```

3. Check Alertmanager configuration

## See Also

- [Deployment Guide](deployment.md)
- [Performance Guide](performance.md)
- [Resilience Guide](resilience.md)
- [Middleware Guide](middleware.md)

## References

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [OpenTelemetry Documentation](https://opentelemetry.io/docs/)
- [Google SRE Book - Monitoring](https://sre.google/sre-book/monitoring-distributed-systems/)
