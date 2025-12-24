//! Monitoring and Logging System for MVP Node
//!
//! This module provides structured logging, metrics collection, and health monitoring
//! for the MVP Node according to Requirements 5.1-5.5.

pub mod metrics;

pub use metrics::*;

// Note: HealthMonitor trait is defined in lib.rs and imported here
// We implement it for DefaultHealthMonitor
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn, Level};

/// Metrics for job processing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JobMetrics {
    pub jobs_submitted: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub total_processing_time_ms: u64,
    pub average_processing_time_ms: f64,
    pub total_tokens_processed: u64,
}

/// Metrics for network activity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub peer_connections: u64,
    pub peer_disconnections: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub active_peers: u64,
}

/// Metrics for inference engine
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceMetrics {
    pub inferences_executed: u64,
    pub inferences_failed: u64,
    pub total_inference_time_ms: u64,
    pub average_inference_time_ms: f64,
    pub peak_memory_mb: u64,
    pub model_loaded: bool,
}

/// Health status of the node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub uptime_seconds: u64,
    pub model_loaded: bool,
    pub network_connected: bool,
    pub queue_size: usize,
    pub error_count: u64,
    pub last_error: Option<String>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            is_healthy: true,
            uptime_seconds: 0,
            model_loaded: false,
            network_connected: false,
            queue_size: 0,
            error_count: 0,
            last_error: None,
        }
    }
}

/// Event types for logging
#[derive(Debug, Clone, Serialize)]
pub enum MonitoringEvent {
    /// Job-related events
    JobSubmitted { job_id: String, model: String },
    JobStarted { job_id: String },
    JobCompleted { job_id: String, duration_ms: u64, tokens: u64 },
    JobFailed { job_id: String, error: String, duration_ms: u64 },
    
    /// Network-related events
    PeerConnected { peer_id: String },
    PeerDisconnected { peer_id: String, reason: Option<String> },
    MessageSent { topic: String, size_bytes: usize },
    MessageReceived { topic: String, peer_id: String, size_bytes: usize },
    
    /// Inference-related events
    ModelLoaded { model_name: String, parameters: u64 },
    ModelUnloaded { model_name: String },
    InferenceStarted { job_id: String },
    InferenceCompleted { job_id: String, duration_ms: u64, tokens: u64 },
    InferenceFailed { job_id: String, error: String },
    
    /// System events
    NodeStarted { node_id: String },
    NodeShutdown { reason: String },
    ErrorOccurred { component: String, error: String },
    HealthCheck { status: HealthStatus },
}

/// Default implementation of health monitoring and metrics collection
pub struct DefaultHealthMonitor {
    /// Start time of the node
    start_time: Instant,
    
    /// Job metrics
    job_metrics: Arc<RwLock<JobMetrics>>,
    
    /// Network metrics
    network_metrics: Arc<RwLock<NetworkMetrics>>,
    
    /// Inference metrics
    inference_metrics: Arc<RwLock<InferenceMetrics>>,
    
    /// Current health status
    health_status: Arc<RwLock<HealthStatus>>,
    
    /// Error counter
    error_count: AtomicU64,
    
    /// Last error message
    last_error: Arc<RwLock<Option<String>>>,
    
    /// Custom metrics storage
    custom_metrics: Arc<RwLock<HashMap<String, f64>>>,
}

impl DefaultHealthMonitor {
    /// Create a new health monitor
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            job_metrics: Arc::new(RwLock::new(JobMetrics::default())),
            network_metrics: Arc::new(RwLock::new(NetworkMetrics::default())),
            inference_metrics: Arc::new(RwLock::new(InferenceMetrics::default())),
            health_status: Arc::new(RwLock::new(HealthStatus::default())),
            error_count: AtomicU64::new(0),
            last_error: Arc::new(RwLock::new(None)),
            custom_metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get uptime in seconds
    pub fn get_uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Log a monitoring event (Property 21, 22, 23)
    pub fn log_event(&self, event: MonitoringEvent) {
        match &event {
            // Job events (Property 21: Requirements 5.1)
            MonitoringEvent::JobSubmitted { job_id, model } => {
                info!(
                    target: "mvp_node::jobs",
                    job_id = %job_id,
                    model = %model,
                    "Job submitted"
                );
            }
            MonitoringEvent::JobStarted { job_id } => {
                info!(
                    target: "mvp_node::jobs",
                    job_id = %job_id,
                    "Job processing started"
                );
            }
            MonitoringEvent::JobCompleted { job_id, duration_ms, tokens } => {
                info!(
                    target: "mvp_node::jobs",
                    job_id = %job_id,
                    duration_ms = %duration_ms,
                    tokens_processed = %tokens,
                    "Job completed successfully"
                );
            }
            MonitoringEvent::JobFailed { job_id, error, duration_ms } => {
                warn!(
                    target: "mvp_node::jobs",
                    job_id = %job_id,
                    error = %error,
                    duration_ms = %duration_ms,
                    "Job failed"
                );
            }

            // Network events (Property 22: Requirements 5.2)
            MonitoringEvent::PeerConnected { peer_id } => {
                info!(
                    target: "mvp_node::network",
                    peer_id = %peer_id,
                    "Peer connected"
                );
            }
            MonitoringEvent::PeerDisconnected { peer_id, reason } => {
                info!(
                    target: "mvp_node::network",
                    peer_id = %peer_id,
                    reason = ?reason,
                    "Peer disconnected"
                );
            }
            MonitoringEvent::MessageSent { topic, size_bytes } => {
                debug!(
                    target: "mvp_node::network",
                    topic = %topic,
                    size_bytes = %size_bytes,
                    "Message sent"
                );
            }
            MonitoringEvent::MessageReceived { topic, peer_id, size_bytes } => {
                debug!(
                    target: "mvp_node::network",
                    topic = %topic,
                    peer_id = %peer_id,
                    size_bytes = %size_bytes,
                    "Message received"
                );
            }

            // Inference events
            MonitoringEvent::ModelLoaded { model_name, parameters } => {
                info!(
                    target: "mvp_node::inference",
                    model_name = %model_name,
                    parameters = %parameters,
                    "Model loaded"
                );
            }
            MonitoringEvent::ModelUnloaded { model_name } => {
                info!(
                    target: "mvp_node::inference",
                    model_name = %model_name,
                    "Model unloaded"
                );
            }
            MonitoringEvent::InferenceStarted { job_id } => {
                debug!(
                    target: "mvp_node::inference",
                    job_id = %job_id,
                    "Inference started"
                );
            }
            MonitoringEvent::InferenceCompleted { job_id, duration_ms, tokens } => {
                info!(
                    target: "mvp_node::inference",
                    job_id = %job_id,
                    duration_ms = %duration_ms,
                    tokens = %tokens,
                    "Inference completed"
                );
            }
            MonitoringEvent::InferenceFailed { job_id, error } => {
                warn!(
                    target: "mvp_node::inference",
                    job_id = %job_id,
                    error = %error,
                    "Inference failed"
                );
            }

            // System events (Property 23: Requirements 5.3)
            MonitoringEvent::NodeStarted { node_id } => {
                info!(
                    target: "mvp_node::system",
                    node_id = %node_id,
                    "Node started"
                );
            }
            MonitoringEvent::NodeShutdown { reason } => {
                info!(
                    target: "mvp_node::system",
                    reason = %reason,
                    "Node shutdown"
                );
            }
            MonitoringEvent::ErrorOccurred { component, error } => {
                error!(
                    target: "mvp_node::system",
                    component = %component,
                    error = %error,
                    "Error occurred"
                );
            }
            MonitoringEvent::HealthCheck { status } => {
                debug!(
                    target: "mvp_node::system",
                    is_healthy = %status.is_healthy,
                    uptime_seconds = %status.uptime_seconds,
                    "Health check"
                );
            }
        }
    }

    /// Record a job submission
    pub async fn record_job_submitted(&self) {
        let mut metrics = self.job_metrics.write().await;
        metrics.jobs_submitted += 1;
    }

    /// Record a job completion
    pub async fn record_job_completed(&self, duration_ms: u64, tokens: u64) {
        let mut metrics = self.job_metrics.write().await;
        metrics.jobs_completed += 1;
        metrics.total_processing_time_ms += duration_ms;
        metrics.total_tokens_processed += tokens;
        
        if metrics.jobs_completed > 0 {
            metrics.average_processing_time_ms = 
                metrics.total_processing_time_ms as f64 / metrics.jobs_completed as f64;
        }
    }

    /// Record a job failure
    pub async fn record_job_failed(&self) {
        let mut metrics = self.job_metrics.write().await;
        metrics.jobs_failed += 1;
    }

    /// Record a peer connection
    pub async fn record_peer_connected(&self) {
        let mut metrics = self.network_metrics.write().await;
        metrics.peer_connections += 1;
        metrics.active_peers += 1;
    }

    /// Record a peer disconnection
    pub async fn record_peer_disconnected(&self) {
        let mut metrics = self.network_metrics.write().await;
        metrics.peer_disconnections += 1;
        if metrics.active_peers > 0 {
            metrics.active_peers -= 1;
        }
    }

    /// Record a message sent
    pub async fn record_message_sent(&self, bytes: u64) {
        let mut metrics = self.network_metrics.write().await;
        metrics.messages_sent += 1;
        metrics.bytes_sent += bytes;
    }

    /// Record a message received
    pub async fn record_message_received(&self, bytes: u64) {
        let mut metrics = self.network_metrics.write().await;
        metrics.messages_received += 1;
        metrics.bytes_received += bytes;
    }

    /// Record an inference execution
    pub async fn record_inference_completed(&self, duration_ms: u64) {
        let mut metrics = self.inference_metrics.write().await;
        metrics.inferences_executed += 1;
        metrics.total_inference_time_ms += duration_ms;
        
        if metrics.inferences_executed > 0 {
            metrics.average_inference_time_ms = 
                metrics.total_inference_time_ms as f64 / metrics.inferences_executed as f64;
        }
    }

    /// Record an inference failure
    pub async fn record_inference_failed(&self) {
        let mut metrics = self.inference_metrics.write().await;
        metrics.inferences_failed += 1;
    }

    /// Record an error (Property 23)
    pub async fn record_error(&self, component: &str, error: &str) {
        self.error_count.fetch_add(1, Ordering::SeqCst);
        
        let mut last = self.last_error.write().await;
        *last = Some(format!("[{}] {}", component, error));
        
        self.log_event(MonitoringEvent::ErrorOccurred {
            component: component.to_string(),
            error: error.to_string(),
        });
    }

    /// Set model loaded status
    pub async fn set_model_loaded(&self, loaded: bool) {
        let mut metrics = self.inference_metrics.write().await;
        metrics.model_loaded = loaded;
    }

    /// Set network connected status
    pub async fn set_network_connected(&self, connected: bool) {
        let mut status = self.health_status.write().await;
        status.network_connected = connected;
    }

    /// Update queue size
    pub async fn set_queue_size(&self, size: usize) {
        let mut status = self.health_status.write().await;
        status.queue_size = size;
    }

    /// Get job metrics
    pub async fn get_job_metrics(&self) -> JobMetrics {
        self.job_metrics.read().await.clone()
    }

    /// Get network metrics
    pub async fn get_network_metrics(&self) -> NetworkMetrics {
        self.network_metrics.read().await.clone()
    }

    /// Get inference metrics
    pub async fn get_inference_metrics(&self) -> InferenceMetrics {
        self.inference_metrics.read().await.clone()
    }

    /// Get full health status (Property 24: Requirements 5.4)
    pub async fn get_full_health_status(&self) -> HealthStatus {
        let mut status = self.health_status.read().await.clone();
        status.uptime_seconds = self.get_uptime_seconds();
        status.error_count = self.error_count.load(Ordering::SeqCst);
        status.last_error = self.last_error.read().await.clone();
        
        let inference = self.inference_metrics.read().await;
        status.model_loaded = inference.model_loaded;
        
        // Determine overall health
        status.is_healthy = status.error_count < 100 && status.model_loaded;
        
        status
    }
}

impl Default for DefaultHealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultHealthMonitor {
    /// Get current health status as key-value pairs (Property 24)
    pub fn get_health_status_map(&self) -> HashMap<String, serde_json::Value> {
        let mut status = HashMap::new();
        
        status.insert("uptime_seconds".to_string(), 
            serde_json::json!(self.get_uptime_seconds()));
        status.insert("error_count".to_string(), 
            serde_json::json!(self.error_count.load(Ordering::SeqCst)));
        status.insert("is_healthy".to_string(), 
            serde_json::json!(self.is_healthy()));
        
        status
    }

    /// Get all metrics as key-value pairs (Property 25: Requirements 5.5)
    pub fn get_metrics(&self) -> HashMap<String, f64> {
        let mut metrics = HashMap::new();
        
        // Try to get metrics, use defaults if locked
        if let Ok(job) = self.job_metrics.try_read() {
            metrics.insert("jobs_submitted".to_string(), job.jobs_submitted as f64);
            metrics.insert("jobs_completed".to_string(), job.jobs_completed as f64);
            metrics.insert("jobs_failed".to_string(), job.jobs_failed as f64);
            metrics.insert("avg_processing_time_ms".to_string(), job.average_processing_time_ms);
            metrics.insert("total_tokens_processed".to_string(), job.total_tokens_processed as f64);
        }
        
        if let Ok(net) = self.network_metrics.try_read() {
            metrics.insert("peer_connections".to_string(), net.peer_connections as f64);
            metrics.insert("peer_disconnections".to_string(), net.peer_disconnections as f64);
            metrics.insert("active_peers".to_string(), net.active_peers as f64);
            metrics.insert("messages_sent".to_string(), net.messages_sent as f64);
            metrics.insert("messages_received".to_string(), net.messages_received as f64);
        }
        
        if let Ok(inf) = self.inference_metrics.try_read() {
            metrics.insert("inferences_executed".to_string(), inf.inferences_executed as f64);
            metrics.insert("inferences_failed".to_string(), inf.inferences_failed as f64);
            metrics.insert("avg_inference_time_ms".to_string(), inf.average_inference_time_ms);
        }
        
        // Add custom metrics
        if let Ok(custom) = self.custom_metrics.try_read() {
            for (key, value) in custom.iter() {
                metrics.insert(key.clone(), *value);
            }
        }
        
        metrics.insert("uptime_seconds".to_string(), self.get_uptime_seconds() as f64);
        
        metrics
    }

    /// Record a custom metric
    pub fn record_metric(&self, name: &str, value: f64) {
        if let Ok(mut custom) = self.custom_metrics.try_write() {
            custom.insert(name.to_string(), value);
        }
    }

    /// Check if node is healthy
    pub fn is_healthy(&self) -> bool {
        // Simple health check: less than 100 errors
        self.error_count.load(Ordering::SeqCst) < 100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_creation() {
        let monitor = DefaultHealthMonitor::new();
        assert!(monitor.is_healthy());
        assert!(monitor.get_uptime_seconds() < 1);
    }

    #[tokio::test]
    async fn test_job_metrics() {
        let monitor = DefaultHealthMonitor::new();
        
        monitor.record_job_submitted().await;
        monitor.record_job_submitted().await;
        monitor.record_job_completed(100, 50).await;
        monitor.record_job_failed().await;
        
        let metrics = monitor.get_job_metrics().await;
        assert_eq!(metrics.jobs_submitted, 2);
        assert_eq!(metrics.jobs_completed, 1);
        assert_eq!(metrics.jobs_failed, 1);
        assert_eq!(metrics.total_tokens_processed, 50);
    }

    #[tokio::test]
    async fn test_network_metrics() {
        let monitor = DefaultHealthMonitor::new();
        
        monitor.record_peer_connected().await;
        monitor.record_peer_connected().await;
        monitor.record_peer_disconnected().await;
        monitor.record_message_sent(100).await;
        monitor.record_message_received(200).await;
        
        let metrics = monitor.get_network_metrics().await;
        assert_eq!(metrics.peer_connections, 2);
        assert_eq!(metrics.peer_disconnections, 1);
        assert_eq!(metrics.active_peers, 1);
        assert_eq!(metrics.bytes_sent, 100);
        assert_eq!(metrics.bytes_received, 200);
    }

    #[tokio::test]
    async fn test_error_recording() {
        let monitor = DefaultHealthMonitor::new();
        
        monitor.record_error("test", "Test error 1").await;
        monitor.record_error("test", "Test error 2").await;
        
        let status = monitor.get_full_health_status().await;
        assert_eq!(status.error_count, 2);
        assert!(status.last_error.unwrap().contains("Test error 2"));
    }

    #[tokio::test]
    async fn test_health_status() {
        let monitor = DefaultHealthMonitor::new();
        
        monitor.set_model_loaded(true).await;
        monitor.set_network_connected(true).await;
        monitor.set_queue_size(5).await;
        
        let status = monitor.get_full_health_status().await;
        assert!(status.is_healthy);
        assert!(status.model_loaded);
        assert!(status.network_connected);
        assert_eq!(status.queue_size, 5);
    }

    #[test]
    fn test_get_metrics() {
        let monitor = DefaultHealthMonitor::new();
        
        let metrics = monitor.get_metrics();
        assert!(metrics.contains_key("uptime_seconds"));
        assert!(metrics.contains_key("jobs_submitted"));
        assert!(metrics.contains_key("active_peers"));
    }

    #[test]
    fn test_event_logging() {
        let monitor = DefaultHealthMonitor::new();
        
        // These should not panic
        monitor.log_event(MonitoringEvent::JobSubmitted {
            job_id: "test-1".to_string(),
            model: "test-model".to_string(),
        });
        
        monitor.log_event(MonitoringEvent::PeerConnected {
            peer_id: "peer-1".to_string(),
        });
        
        monitor.log_event(MonitoringEvent::ErrorOccurred {
            component: "test".to_string(),
            error: "Test error".to_string(),
        });
    }
}
