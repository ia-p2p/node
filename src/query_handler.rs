//! Query Handler for Processing CLI Queries
//!
//! Handles incoming queries from CLI and other peers via the request-response protocol.
//!
//! Implements Requirements 6.1, 6.2, 6.3, 6.4, 6.5

use crate::monitoring::DefaultHealthMonitor;
use crate::job_store::JobStore;
use crate::protocol::queries::{
    Query, QueryResponse, NodeInfo, NodeMetrics, 
    HealthStatus as QueryHealthStatus, ComponentHealth,
    JobStatusInfo, JobInfo, JobResultInfo, JobExecutionMetrics, JobFilter,
    JobSubmitResponse,
};
use libp2p::PeerId;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Handles incoming queries from CLI and other peers
pub struct QueryHandler {
    /// Health monitor for system metrics
    health_monitor: Arc<DefaultHealthMonitor>,
    
    /// Job store for tracking jobs
    job_store: Arc<JobStore>,
    
    /// Node identifier
    node_id: String,
    
    /// Peer ID of this node
    peer_id: PeerId,
    
    /// Maximum queue size
    max_queue_size: usize,
    
    /// Node start time for uptime calculation
    start_time: std::time::Instant,
}

impl QueryHandler {
    /// Create a new QueryHandler
    pub fn new(
        health_monitor: Arc<DefaultHealthMonitor>,
        node_id: String,
        peer_id: PeerId,
        max_queue_size: usize,
    ) -> Self {
        let job_store = Arc::new(JobStore::new(node_id.clone(), 1000));
        
        Self {
            health_monitor,
            job_store,
            node_id,
            peer_id,
            max_queue_size,
            start_time: std::time::Instant::now(),
        }
    }
    
    /// Create a QueryHandler with an existing JobStore
    pub fn with_job_store(
        health_monitor: Arc<DefaultHealthMonitor>,
        job_store: Arc<JobStore>,
        node_id: String,
        peer_id: PeerId,
        max_queue_size: usize,
    ) -> Self {
        Self {
            health_monitor,
            job_store,
            node_id,
            peer_id,
            max_queue_size,
            start_time: std::time::Instant::now(),
        }
    }
    
    /// Get reference to job store
    pub fn job_store(&self) -> Arc<JobStore> {
        Arc::clone(&self.job_store)
    }
    
    /// Handle an incoming query
    /// 
    /// Property 10: Node Query Response Time - responds within 1 second
    pub async fn handle_query(&self, query: Query) -> QueryResponse {
        debug!("Handling query: {:?}", query);
        
        match query {
            Query::GetNodeInfo { node_id, .. } => {
                self.handle_get_node_info(&node_id).await
            }
            Query::GetNodeMetrics { node_id, .. } => {
                self.handle_get_metrics(&node_id).await
            }
            Query::GetNodeHealth { node_id, .. } => {
                self.handle_get_health(&node_id).await
            }
            Query::ListNodes { .. } => {
                self.handle_list_nodes().await
            }
            Query::GetJobStatus { job_id, .. } => {
                self.handle_get_job_status(&job_id).await
            }
            Query::ListJobs { filter, .. } => {
                self.handle_list_jobs(filter).await
            }
            Query::GetJobResult { job_id, .. } => {
                self.handle_get_job_result(&job_id).await
            }
            Query::Ping { .. } => {
                QueryResponse::pong()
            }
            Query::SubmitJob { model, input, .. } => {
                self.handle_submit_job(&model, &input).await
            }
            Query::CancelJob { job_id, .. } => {
                self.handle_cancel_job(&job_id).await
            }
        }
    }
    
    /// Handle GetNodeInfo query
    /// Property 10: responds within 1 second
    async fn handle_get_node_info(&self, _node_id: &str) -> QueryResponse {
        let health_status = self.health_monitor.get_full_health_status().await;
        let uptime = self.start_time.elapsed().as_secs();
        
        let info = NodeInfo {
            node_id: self.node_id.clone(),
            peer_id: self.peer_id.to_string(),
            addresses: vec![], // TODO: Get from network layer
            status: if health_status.is_healthy { "online".to_string() } else { "degraded".to_string() },
            models: vec!["tinyllama".to_string()], // TODO: Get from inference engine
            uptime_seconds: uptime,
            peer_count: 0, // TODO: Get from network layer
            queue_capacity: self.max_queue_size,
            queue_size: health_status.queue_size,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        
        QueryResponse::NodeInfo(info)
    }
    
    /// Handle GetNodeMetrics query
    /// Property 10: responds within 1 second
    async fn handle_get_metrics(&self, _node_id: &str) -> QueryResponse {
        let health_status = self.health_monitor.get_full_health_status().await;
        let job_metrics = self.health_monitor.get_job_metrics().await;
        let inference_metrics = self.health_monitor.get_inference_metrics().await;
        let uptime = self.start_time.elapsed().as_secs();
        
        let metrics = NodeMetrics {
            jobs_processed: job_metrics.jobs_submitted,
            jobs_queued: health_status.queue_size,
            jobs_completed: job_metrics.jobs_completed,
            jobs_failed: job_metrics.jobs_failed,
            avg_inference_latency_ms: inference_metrics.average_inference_time_ms,
            tokens_processed: job_metrics.total_tokens_processed,
            memory_usage_bytes: inference_metrics.peak_memory_mb * 1024 * 1024, // Convert MB to bytes
            cpu_usage_percent: 0.0, // TODO: Get actual CPU usage
            network_bytes_sent: 0, // TODO: Track network bytes
            network_bytes_received: 0,
            uptime_seconds: uptime,
        };
        
        QueryResponse::NodeMetrics(metrics)
    }
    
    /// Handle GetNodeHealth query
    /// Property 10: responds within 1 second
    async fn handle_get_health(&self, _node_id: &str) -> QueryResponse {
        let health_status = self.health_monitor.get_full_health_status().await;
        
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        // Determine queue health based on utilization
        let queue_utilization = if self.max_queue_size > 0 {
            (health_status.queue_size as f64 / self.max_queue_size as f64) * 100.0
        } else {
            0.0
        };
        
        let queue_health = if queue_utilization < 50.0 {
            ComponentHealth::healthy_with_usage(queue_utilization)
        } else if queue_utilization < 80.0 {
            ComponentHealth {
                status: "degraded".to_string(),
                message: Some("High queue depth".to_string()),
                usage_percent: Some(queue_utilization),
            }
        } else {
            ComponentHealth {
                status: "unhealthy".to_string(),
                message: Some("Queue nearly full".to_string()),
                usage_percent: Some(queue_utilization),
            }
        };
        
        // Network health based on connection status
        let network_health = if health_status.network_connected {
            ComponentHealth::healthy()
        } else {
            ComponentHealth::degraded("Not connected to network")
        };
        
        // Model health based on model loaded status
        let model_health = if health_status.model_loaded {
            ComponentHealth::healthy()
        } else {
            ComponentHealth::degraded("Model not loaded")
        };
        
        let overall = if health_status.is_healthy {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };
        
        let status = QueryHealthStatus {
            overall,
            network: network_health,
            model: model_health,
            queue: queue_health,
            memory: ComponentHealth::healthy_with_usage(50.0), // TODO: Real memory usage
            cpu: ComponentHealth::healthy_with_usage(30.0), // TODO: Real CPU usage
            timestamp,
        };
        
        QueryResponse::NodeHealth(status)
    }
    
    /// Handle ListNodes query
    async fn handle_list_nodes(&self) -> QueryResponse {
        // For now, just return info about this node
        // In a full implementation, this would query the network for all known nodes
        let health_status = self.health_monitor.get_full_health_status().await;
        let uptime = self.start_time.elapsed().as_secs();
        
        let info = NodeInfo {
            node_id: self.node_id.clone(),
            peer_id: self.peer_id.to_string(),
            addresses: vec![],
            status: if health_status.is_healthy { "online".to_string() } else { "degraded".to_string() },
            models: vec!["tinyllama".to_string()],
            uptime_seconds: uptime,
            peer_count: 0,
            queue_capacity: self.max_queue_size,
            queue_size: health_status.queue_size,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        
        QueryResponse::NodeList(vec![info])
    }
    
    /// Handle GetJobStatus query
    /// Property 11: responds with job status or error if not found
    async fn handle_get_job_status(&self, job_id: &str) -> QueryResponse {
        debug!("Getting status for job: {}", job_id);
        
        match self.job_store.get_job_status(job_id).await {
            Some(status) => QueryResponse::JobStatus(status),
            None => QueryResponse::not_found("Job", job_id),
        }
    }
    
    /// Handle ListJobs query
    async fn handle_list_jobs(&self, filter: Option<JobFilter>) -> QueryResponse {
        debug!("Listing jobs with filter: {:?}", filter);
        
        let jobs = self.job_store.list_jobs(filter).await;
        QueryResponse::JobList(jobs)
    }
    
    /// Handle GetJobResult query
    async fn handle_get_job_result(&self, job_id: &str) -> QueryResponse {
        debug!("Getting result for job: {}", job_id);
        
        match self.job_store.get_job_result(job_id).await {
            Some(result) => QueryResponse::JobResult(result),
            None => QueryResponse::not_found("Job", job_id),
        }
    }
    
    /// Handle SubmitJob query
    async fn handle_submit_job(&self, model: &str, input: &str) -> QueryResponse {
        debug!("Received job submission: model={}, input_len={}", model, input.len());
        
        // Generate a unique job ID
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let job_id = format!("job-{}-{}", self.node_id, timestamp);
        
        // Track job in job store
        let tracked_job = self.job_store.submit_job(
            job_id.clone(),
            model.to_string(),
            input.to_string(),
        ).await;
        
        // Record job submission in health monitor
        self.health_monitor.record_job_submitted().await;
        
        // Get queue position
        let queue_position = self.job_store.get_queue_position(&job_id).await;
        
        info!("Job {} submitted: model={}, queue_position={:?}", job_id, model, queue_position);
        
        let mut response = JobSubmitResponse::new(&job_id, &self.node_id);
        if let Some(pos) = queue_position {
            response = response.with_queue_position(pos);
        }
        
        QueryResponse::JobSubmitted(response)
    }
    
    /// Handle CancelJob query
    async fn handle_cancel_job(&self, job_id: &str) -> QueryResponse {
        debug!("Received job cancellation request: {}", job_id);
        
        // Check if job exists first
        if self.job_store.get_job(job_id).await.is_none() {
            return QueryResponse::not_found("Job", job_id);
        }
        
        // Try to cancel
        let success = self.job_store.cancel_job(job_id).await;
        
        let message = if success {
            info!("Job {} cancelled successfully", job_id);
            Some("Job cancelled successfully".to_string())
        } else {
            warn!("Job {} could not be cancelled (already completed/failed)", job_id);
            Some("Job could not be cancelled (already completed or failed)".to_string())
        };
        
        QueryResponse::JobCancelled {
            job_id: job_id.to_string(),
            success,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_handler() -> QueryHandler {
        let monitor = Arc::new(DefaultHealthMonitor::new());
        let peer_id = PeerId::random();
        
        QueryHandler::new(
            monitor,
            "test-node".to_string(),
            peer_id,
            100,
        )
    }
    
    #[tokio::test]
    async fn test_handle_get_node_info() {
        let handler = create_test_handler();
        let query = Query::get_node_info("test-node");
        
        let response = handler.handle_query(query).await;
        
        if let QueryResponse::NodeInfo(info) = response {
            assert_eq!(info.node_id, "test-node");
            assert!(!info.peer_id.is_empty());
        } else {
            panic!("Expected NodeInfo response");
        }
    }
    
    #[tokio::test]
    async fn test_handle_get_metrics() {
        let handler = create_test_handler();
        let query = Query::get_node_metrics("test-node");
        
        let response = handler.handle_query(query).await;
        
        if let QueryResponse::NodeMetrics(metrics) = response {
            assert!(metrics.uptime_seconds >= 0);
        } else {
            panic!("Expected NodeMetrics response");
        }
    }
    
    #[tokio::test]
    async fn test_handle_get_health() {
        let handler = create_test_handler();
        let query = Query::get_node_health("test-node");
        
        let response = handler.handle_query(query).await;
        
        if let QueryResponse::NodeHealth(health) = response {
            assert!(health.timestamp > 0);
            assert!(!health.overall.is_empty());
        } else {
            panic!("Expected NodeHealth response");
        }
    }
    
    #[tokio::test]
    async fn test_handle_list_nodes() {
        let handler = create_test_handler();
        let query = Query::list_nodes();
        
        let response = handler.handle_query(query).await;
        
        if let QueryResponse::NodeList(nodes) = response {
            assert!(!nodes.is_empty());
        } else {
            panic!("Expected NodeList response");
        }
    }
    
    #[tokio::test]
    async fn test_handle_ping() {
        let handler = create_test_handler();
        let query = Query::ping();
        
        let response = handler.handle_query(query).await;
        
        if let QueryResponse::Pong { timestamp } = response {
            assert!(timestamp > 0);
        } else {
            panic!("Expected Pong response");
        }
    }
    
    #[tokio::test]
    async fn test_handle_job_not_found() {
        let handler = create_test_handler();
        let query = Query::get_job_status("nonexistent-job");
        
        let response = handler.handle_query(query).await;
        
        assert!(response.is_error());
        if let QueryResponse::Error { code, .. } = response {
            assert_eq!(code, "NOT_FOUND");
        }
    }
    
    #[tokio::test]
    async fn test_all_query_types() {
        let handler = create_test_handler();
        
        let queries = vec![
            Query::get_node_info("node-1"),
            Query::get_node_metrics("node-1"),
            Query::get_node_health("node-1"),
            Query::list_nodes(),
            Query::get_job_status("job-1"),
            Query::list_jobs(None),
            Query::get_job_result("job-1"),
            Query::ping(),
            Query::submit_job("tinyllama", "Hello"),
            Query::cancel_job("job-1"),
        ];
        
        for query in queries {
            let response = handler.handle_query(query).await;
            // All queries should return a valid response (not panic)
            assert!(matches!(
                response,
                QueryResponse::NodeInfo(_) |
                QueryResponse::NodeMetrics(_) |
                QueryResponse::NodeHealth(_) |
                QueryResponse::NodeList(_) |
                QueryResponse::JobStatus(_) |
                QueryResponse::JobList(_) |
                QueryResponse::JobResult(_) |
                QueryResponse::Pong { .. } |
                QueryResponse::JobSubmitted(_) |
                QueryResponse::JobCancelled { .. } |
                QueryResponse::Error { .. }
            ));
        }
    }
    
    #[tokio::test]
    async fn test_handle_submit_job() {
        let handler = create_test_handler();
        let query = Query::submit_job("tinyllama", "Hello world");
        
        let response = handler.handle_query(query).await;
        
        if let QueryResponse::JobSubmitted(result) = response {
            assert!(!result.job_id.is_empty());
            assert_eq!(result.node_id, "test-node");
            assert_eq!(result.status, "pending");
        } else {
            panic!("Expected JobSubmitted response, got {:?}", response);
        }
    }
    
    #[tokio::test]
    async fn test_handle_cancel_job() {
        let handler = create_test_handler();
        
        // First submit a job so we can cancel it
        let submit_query = Query::submit_job("tinyllama", "Hello");
        let submit_response = handler.handle_query(submit_query).await;
        
        let job_id = if let QueryResponse::JobSubmitted(result) = submit_response {
            result.job_id
        } else {
            panic!("Expected JobSubmitted response");
        };
        
        // Now cancel it
        let cancel_query = Query::cancel_job(&job_id);
        let response = handler.handle_query(cancel_query).await;
        
        if let QueryResponse::JobCancelled { job_id: cancelled_id, success, .. } = response {
            assert_eq!(cancelled_id, job_id);
            assert!(success);
        } else {
            panic!("Expected JobCancelled response, got {:?}", response);
        }
    }
    
    #[tokio::test]
    async fn test_handle_cancel_nonexistent_job() {
        let handler = create_test_handler();
        let query = Query::cancel_job("nonexistent-job");
        
        let response = handler.handle_query(query).await;
        
        assert!(response.is_error());
        if let QueryResponse::Error { code, .. } = response {
            assert_eq!(code, "NOT_FOUND");
        }
    }
    
    #[tokio::test]
    async fn test_job_lifecycle() {
        let handler = create_test_handler();
        
        // Submit a job
        let submit_response = handler.handle_query(Query::submit_job("tinyllama", "Test")).await;
        let job_id = if let QueryResponse::JobSubmitted(result) = submit_response {
            result.job_id
        } else {
            panic!("Expected JobSubmitted");
        };
        
        // Check status
        let status_response = handler.handle_query(Query::get_job_status(&job_id)).await;
        if let QueryResponse::JobStatus(status) = status_response {
            assert_eq!(status.job_id, job_id);
            assert_eq!(status.status, "pending");
        } else {
            panic!("Expected JobStatus, got {:?}", status_response);
        }
        
        // List jobs should include it
        let list_response = handler.handle_query(Query::list_jobs(None)).await;
        if let QueryResponse::JobList(jobs) = list_response {
            assert!(jobs.iter().any(|j| j.job_id == job_id));
        } else {
            panic!("Expected JobList");
        }
        
        // Complete the job via job_store directly
        handler.job_store().complete_job(&job_id, "Output".to_string(), 100, Some(50)).await;
        
        // Get result
        let result_response = handler.handle_query(Query::get_job_result(&job_id)).await;
        if let QueryResponse::JobResult(result) = result_response {
            assert_eq!(result.job_id, job_id);
            assert_eq!(result.status, "completed");
            assert_eq!(result.output, Some("Output".to_string()));
        } else {
            panic!("Expected JobResult, got {:?}", result_response);
        }
    }
}
