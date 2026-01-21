//! Comprehensive Metrics and Status Reporting
//!
//! This module provides detailed metrics, performance tracking, and status reporting
//! for the MVP Node according to Requirements 5.4, 5.5, and 6.4.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Comprehensive node status for health queries (Property 24: Requirements 5.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Node identifier
    pub node_id: String,
    /// Current operational state
    pub state: NodeState,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Version information
    pub version: String,
    /// Component health statuses
    pub components: ComponentHealth,
    /// Resource usage
    pub resources: ResourceUsage,
    /// Timestamp of status
    pub timestamp: i64,
}

impl NodeStatus {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            state: NodeState::Initializing,
            uptime_secs: 0,
            version: env!("CARGO_PKG_VERSION").to_string(),
            components: ComponentHealth::default(),
            resources: ResourceUsage::default(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Check if the node is in a healthy state
    pub fn is_healthy(&self) -> bool {
        matches!(self.state, NodeState::Running | NodeState::Idle)
            && self.components.is_healthy()
    }
}

/// Operational state of the node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    /// Node is starting up
    Initializing,
    /// Node is ready and waiting for work
    Idle,
    /// Node is actively processing jobs
    Running,
    /// Node is under heavy load
    Busy,
    /// Node is shutting down gracefully
    ShuttingDown,
    /// Node has encountered an error
    Error,
}

impl Default for NodeState {
    fn default() -> Self {
        Self::Initializing
    }
}

/// Health status of individual components
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub network: ComponentStatus,
    pub executor: ComponentStatus,
    pub inference: ComponentStatus,
    pub queue: ComponentStatus,
    pub monitoring: ComponentStatus,
}

impl ComponentHealth {
    pub fn is_healthy(&self) -> bool {
        self.network.is_healthy
            && self.executor.is_healthy
            && self.inference.is_healthy
            && self.queue.is_healthy
            && self.monitoring.is_healthy
    }

    /// Get a list of unhealthy components
    pub fn unhealthy_components(&self) -> Vec<String> {
        let mut unhealthy = Vec::new();
        if !self.network.is_healthy {
            unhealthy.push("network".to_string());
        }
        if !self.executor.is_healthy {
            unhealthy.push("executor".to_string());
        }
        if !self.inference.is_healthy {
            unhealthy.push("inference".to_string());
        }
        if !self.queue.is_healthy {
            unhealthy.push("queue".to_string());
        }
        if !self.monitoring.is_healthy {
            unhealthy.push("monitoring".to_string());
        }
        unhealthy
    }
}

/// Status of a single component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub is_healthy: bool,
    pub last_check: i64,
    pub message: Option<String>,
}

impl Default for ComponentStatus {
    fn default() -> Self {
        Self {
            is_healthy: true,
            last_check: chrono::Utc::now().timestamp(),
            message: None,
        }
    }
}

/// Resource usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub memory_available_mb: u64,
    pub disk_used_mb: u64,
    pub open_connections: u32,
}

/// Detailed queue status for clients (Property 29: Requirements 6.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedQueueStatus {
    /// Number of pending jobs
    pub pending_jobs: usize,
    /// Number of jobs currently processing
    pub processing_jobs: usize,
    /// Maximum queue capacity
    pub max_capacity: usize,
    /// Queue utilization percentage
    pub utilization_percent: f64,
    /// Estimated wait time in seconds
    pub estimated_wait_secs: f64,
    /// Jobs by priority level
    pub jobs_by_priority: HashMap<String, usize>,
    /// Average processing time (ms)
    pub avg_processing_time_ms: f64,
    /// Oldest job age (seconds)
    pub oldest_job_age_secs: Option<f64>,
    /// Is the queue accepting new jobs?
    pub accepting_jobs: bool,
    /// Timestamp
    pub timestamp: i64,
}

impl Default for DetailedQueueStatus {
    fn default() -> Self {
        Self {
            pending_jobs: 0,
            processing_jobs: 0,
            max_capacity: 100,
            utilization_percent: 0.0,
            estimated_wait_secs: 0.0,
            jobs_by_priority: HashMap::new(),
            avg_processing_time_ms: 0.0,
            oldest_job_age_secs: None,
            accepting_jobs: true,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

impl DetailedQueueStatus {
    /// Calculate utilization percentage
    pub fn calculate_utilization(&mut self) {
        if self.max_capacity > 0 {
            self.utilization_percent = 
                (self.pending_jobs + self.processing_jobs) as f64 / self.max_capacity as f64 * 100.0;
        }
    }

    /// Estimate wait time based on current processing rate
    pub fn estimate_wait_time(&mut self) {
        if self.avg_processing_time_ms > 0.0 && self.pending_jobs > 0 {
            self.estimated_wait_secs = 
                (self.pending_jobs as f64 * self.avg_processing_time_ms) / 1000.0;
        }
    }

    /// Check if queue can accept more jobs
    pub fn can_accept(&self) -> bool {
        self.accepting_jobs && self.pending_jobs < self.max_capacity
    }
}

/// Performance metrics (Property 25: Requirements 5.5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Job processing metrics
    pub jobs: JobPerformance,
    /// Network activity metrics
    pub network: NetworkPerformance,
    /// Inference performance
    pub inference: InferencePerformance,
    /// System performance
    pub system: SystemPerformance,
    /// Timestamp
    pub timestamp: i64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            jobs: JobPerformance::default(),
            network: NetworkPerformance::default(),
            inference: InferencePerformance::default(),
            system: SystemPerformance::default(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobPerformance {
    /// Total jobs processed
    pub total_processed: u64,
    /// Jobs completed successfully
    pub successful: u64,
    /// Jobs failed
    pub failed: u64,
    /// Success rate percentage
    pub success_rate: f64,
    /// Throughput (jobs per second)
    pub throughput: f64,
    /// Average latency (ms)
    pub avg_latency_ms: f64,
    /// P50 latency (ms)
    pub p50_latency_ms: f64,
    /// P95 latency (ms)
    pub p95_latency_ms: f64,
    /// P99 latency (ms)
    pub p99_latency_ms: f64,
}

impl JobPerformance {
    pub fn calculate_success_rate(&mut self) {
        if self.total_processed > 0 {
            self.success_rate = self.successful as f64 / self.total_processed as f64 * 100.0;
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkPerformance {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Current active peers
    pub active_peers: u32,
    /// Peak concurrent peers
    pub peak_peers: u32,
    /// Connection success rate
    pub connection_success_rate: f64,
    /// Average message latency (ms)
    pub avg_message_latency_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferencePerformance {
    /// Total inferences
    pub total_inferences: u64,
    /// Tokens processed
    pub tokens_processed: u64,
    /// Average tokens per second
    pub tokens_per_second: f64,
    /// Average inference time (ms)
    pub avg_inference_time_ms: f64,
    /// Model load time (ms)
    pub model_load_time_ms: u64,
    /// Peak memory usage (MB)
    pub peak_memory_mb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemPerformance {
    /// CPU usage samples
    pub cpu_usage_avg: f64,
    /// Memory usage samples
    pub memory_usage_avg: f64,
    /// Event loop latency (ms)
    pub event_loop_latency_ms: f64,
    /// Garbage collection pauses (if any)
    pub gc_pauses: u32,
}

/// Metrics collector with history
pub struct MetricsCollector {
    /// Node ID
    node_id: String,
    /// Start time
    start_time: Instant,
    /// Latency samples for percentile calculation
    latency_samples: Arc<RwLock<VecDeque<f64>>>,
    /// Max samples to keep
    max_samples: usize,
    /// Current metrics
    current_metrics: Arc<RwLock<PerformanceMetrics>>,
    /// Queue status
    queue_status: Arc<RwLock<DetailedQueueStatus>>,
}

impl MetricsCollector {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            start_time: Instant::now(),
            latency_samples: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            max_samples: 1000,
            current_metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            queue_status: Arc::new(RwLock::new(DetailedQueueStatus::default())),
        }
    }

    /// Record a job latency sample
    pub async fn record_latency(&self, latency_ms: f64) {
        let mut samples = self.latency_samples.write().await;
        if samples.len() >= self.max_samples {
            samples.pop_front();
        }
        samples.push_back(latency_ms);

        // Update percentiles
        let mut sorted: Vec<f64> = samples.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        
        if !sorted.is_empty() {
            let mut metrics = self.current_metrics.write().await;
            metrics.jobs.p50_latency_ms = sorted[sorted.len() / 2];
            metrics.jobs.p95_latency_ms = sorted[(sorted.len() as f64 * 0.95) as usize];
            metrics.jobs.p99_latency_ms = sorted[(sorted.len() as f64 * 0.99).min((sorted.len() - 1) as f64) as usize];
            
            let sum: f64 = sorted.iter().sum();
            metrics.jobs.avg_latency_ms = sum / sorted.len() as f64;
        }
    }

    /// Record a successful job
    pub async fn record_job_success(&self, latency_ms: f64) {
        let mut metrics = self.current_metrics.write().await;
        metrics.jobs.total_processed += 1;
        metrics.jobs.successful += 1;
        metrics.jobs.calculate_success_rate();
        
        drop(metrics);
        self.record_latency(latency_ms).await;
    }

    /// Record a failed job
    pub async fn record_job_failure(&self) {
        let mut metrics = self.current_metrics.write().await;
        metrics.jobs.total_processed += 1;
        metrics.jobs.failed += 1;
        metrics.jobs.calculate_success_rate();
    }

    /// Record network message
    pub async fn record_message(&self, sent: bool, bytes: u64) {
        let mut metrics = self.current_metrics.write().await;
        if sent {
            metrics.network.messages_sent += 1;
            metrics.network.bytes_sent += bytes;
        } else {
            metrics.network.messages_received += 1;
            metrics.network.bytes_received += bytes;
        }
    }

    /// Update peer count
    pub async fn update_peer_count(&self, count: u32) {
        let mut metrics = self.current_metrics.write().await;
        metrics.network.active_peers = count;
        if count > metrics.network.peak_peers {
            metrics.network.peak_peers = count;
        }
    }

    /// Record inference
    pub async fn record_inference(&self, tokens: u64, duration_ms: u64) {
        let mut metrics = self.current_metrics.write().await;
        metrics.inference.total_inferences += 1;
        metrics.inference.tokens_processed += tokens;
        
        let total_time_ms = metrics.inference.avg_inference_time_ms * (metrics.inference.total_inferences - 1) as f64 + duration_ms as f64;
        metrics.inference.avg_inference_time_ms = total_time_ms / metrics.inference.total_inferences as f64;
        
        // Calculate tokens per second
        if duration_ms > 0 {
            let elapsed_secs = self.start_time.elapsed().as_secs_f64();
            if elapsed_secs > 0.0 {
                metrics.inference.tokens_per_second = metrics.inference.tokens_processed as f64 / elapsed_secs;
            }
        }
    }

    /// Update queue status
    pub async fn update_queue_status(&self, pending: usize, processing: usize, max_capacity: usize) {
        let mut status = self.queue_status.write().await;
        status.pending_jobs = pending;
        status.processing_jobs = processing;
        status.max_capacity = max_capacity;
        status.calculate_utilization();
        status.timestamp = chrono::Utc::now().timestamp();
        
        // Update estimated wait time based on metrics
        let metrics = self.current_metrics.read().await;
        status.avg_processing_time_ms = metrics.jobs.avg_latency_ms;
        drop(metrics);
        status.estimate_wait_time();
    }

    /// Get current performance metrics
    pub async fn get_performance_metrics(&self) -> PerformanceMetrics {
        let mut metrics = self.current_metrics.read().await.clone();
        metrics.timestamp = chrono::Utc::now().timestamp();
        
        // Calculate throughput
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            metrics.jobs.throughput = metrics.jobs.total_processed as f64 / elapsed;
        }
        
        metrics
    }

    /// Get detailed queue status
    pub async fn get_queue_status(&self) -> DetailedQueueStatus {
        self.queue_status.read().await.clone()
    }

    /// Get node status
    pub async fn get_node_status(&self) -> NodeStatus {
        let metrics = self.get_performance_metrics().await;
        
        let state = if metrics.jobs.total_processed == 0 {
            NodeState::Idle
        } else {
            NodeState::Running
        };

        let mut status = NodeStatus::new(&self.node_id);
        status.uptime_secs = self.start_time.elapsed().as_secs();
        status.state = state;
        status.components.monitoring.is_healthy = true;
        status.components.monitoring.last_check = chrono::Utc::now().timestamp();
        
        status
    }

    /// Get uptime seconds
    pub fn get_uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector_latency() {
        let collector = MetricsCollector::new("test-node");
        
        collector.record_latency(100.0).await;
        collector.record_latency(200.0).await;
        collector.record_latency(150.0).await;
        
        let metrics = collector.get_performance_metrics().await;
        assert!(metrics.jobs.avg_latency_ms > 0.0);
    }

    #[tokio::test]
    async fn test_job_success_tracking() {
        let collector = MetricsCollector::new("test-node");
        
        collector.record_job_success(100.0).await;
        collector.record_job_success(200.0).await;
        collector.record_job_failure().await;
        
        let metrics = collector.get_performance_metrics().await;
        assert_eq!(metrics.jobs.total_processed, 3);
        assert_eq!(metrics.jobs.successful, 2);
        assert_eq!(metrics.jobs.failed, 1);
        assert!((metrics.jobs.success_rate - 66.666).abs() < 1.0);
    }

    #[tokio::test]
    async fn test_queue_status_updates() {
        let collector = MetricsCollector::new("test-node");
        
        collector.update_queue_status(10, 2, 100).await;
        
        let status = collector.get_queue_status().await;
        assert_eq!(status.pending_jobs, 10);
        assert_eq!(status.processing_jobs, 2);
        assert_eq!(status.utilization_percent, 12.0);
    }

    #[tokio::test]
    async fn test_node_status() {
        let collector = MetricsCollector::new("test-node");
        
        let status = collector.get_node_status().await;
        assert_eq!(status.node_id, "test-node");
        assert_eq!(status.state, NodeState::Idle);
    }

    #[test]
    fn test_detailed_queue_status_can_accept() {
        let mut status = DetailedQueueStatus::default();
        status.max_capacity = 10;
        status.pending_jobs = 5;
        
        assert!(status.can_accept());
        
        status.pending_jobs = 10;
        assert!(!status.can_accept());
    }

    #[test]
    fn test_component_health_unhealthy() {
        let mut health = ComponentHealth::default();
        health.network.is_healthy = false;
        health.inference.is_healthy = false;
        
        let unhealthy = health.unhealthy_components();
        assert_eq!(unhealthy.len(), 2);
        assert!(unhealthy.contains(&"network".to_string()));
        assert!(unhealthy.contains(&"inference".to_string()));
    }
}











