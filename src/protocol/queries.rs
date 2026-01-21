//! Query Protocol Types for Request-Response Communication
//!
//! This module defines the query and response types used for direct
//! peer-to-peer communication between CLI and nodes.
//!
//! Protocol ID: /mvp/query/1.0.0
//!
//! Implements Requirements 1.3, 9.1, 9.4

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Protocol identifier for request-response
pub const QUERY_PROTOCOL_ID: &str = "/mvp/query/1.0.0";

/// Protocol version
pub const QUERY_PROTOCOL_VERSION: &str = "1.0.0";

/// Maximum query size in bytes (1MB)
pub const MAX_QUERY_SIZE: usize = 1_048_576;

/// Query types for request-response protocol
/// 
/// These are the requests that can be sent from CLI to nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Query {
    /// Get detailed information about a specific node
    GetNodeInfo {
        node_id: String,
        #[serde(default)]
        version: String,
    },
    
    /// Get metrics from a specific node
    GetNodeMetrics {
        node_id: String,
        #[serde(default)]
        version: String,
    },
    
    /// Get health status from a specific node
    GetNodeHealth {
        node_id: String,
        #[serde(default)]
        version: String,
    },
    
    /// List all known nodes
    ListNodes {
        #[serde(default)]
        version: String,
    },
    
    /// Get status of a specific job
    GetJobStatus {
        job_id: String,
        #[serde(default)]
        version: String,
    },
    
    /// List jobs with optional filter
    ListJobs {
        #[serde(default)]
        filter: Option<JobFilter>,
        #[serde(default)]
        version: String,
    },
    
    /// Get result of a completed job
    GetJobResult {
        job_id: String,
        #[serde(default)]
        version: String,
    },
    
    /// Ping request for health check
    Ping {
        #[serde(default)]
        version: String,
    },
    
    /// Submit a new job
    SubmitJob {
        model: String,
        input: String,
        #[serde(default)]
        version: String,
    },
    
    /// Cancel an existing job
    CancelJob {
        job_id: String,
        #[serde(default)]
        version: String,
    },
}

impl Query {
    /// Get the version field from any query variant
    pub fn version(&self) -> &str {
        match self {
            Query::GetNodeInfo { version, .. } => version,
            Query::GetNodeMetrics { version, .. } => version,
            Query::GetNodeHealth { version, .. } => version,
            Query::ListNodes { version, .. } => version,
            Query::GetJobStatus { version, .. } => version,
            Query::ListJobs { version, .. } => version,
            Query::GetJobResult { version, .. } => version,
            Query::Ping { version, .. } => version,
            Query::SubmitJob { version, .. } => version,
            Query::CancelJob { version, .. } => version,
        }
    }
    
    /// Create a GetNodeInfo query with default version
    pub fn get_node_info(node_id: impl Into<String>) -> Self {
        Query::GetNodeInfo {
            node_id: node_id.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a GetNodeMetrics query with default version
    pub fn get_node_metrics(node_id: impl Into<String>) -> Self {
        Query::GetNodeMetrics {
            node_id: node_id.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a GetNodeHealth query with default version
    pub fn get_node_health(node_id: impl Into<String>) -> Self {
        Query::GetNodeHealth {
            node_id: node_id.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a ListNodes query with default version
    pub fn list_nodes() -> Self {
        Query::ListNodes {
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a GetJobStatus query with default version
    pub fn get_job_status(job_id: impl Into<String>) -> Self {
        Query::GetJobStatus {
            job_id: job_id.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a ListJobs query with optional filter
    pub fn list_jobs(filter: Option<JobFilter>) -> Self {
        Query::ListJobs {
            filter,
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a GetJobResult query with default version
    pub fn get_job_result(job_id: impl Into<String>) -> Self {
        Query::GetJobResult {
            job_id: job_id.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a Ping query
    pub fn ping() -> Self {
        Query::Ping {
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a SubmitJob query
    pub fn submit_job(model: impl Into<String>, input: impl Into<String>) -> Self {
        Query::SubmitJob {
            model: model.into(),
            input: input.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
    
    /// Create a CancelJob query
    pub fn cancel_job(job_id: impl Into<String>) -> Self {
        Query::CancelJob {
            job_id: job_id.into(),
            version: QUERY_PROTOCOL_VERSION.to_string(),
        }
    }
}

/// Filter options for job listing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JobFilter {
    /// Filter by job status
    #[serde(default)]
    pub status: Option<String>,
    
    /// Filter by model name
    #[serde(default)]
    pub model: Option<String>,
    
    /// Maximum number of results
    #[serde(default)]
    pub limit: Option<usize>,
    
    /// Offset for pagination
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Response types for queries
/// 
/// These are the responses sent from nodes back to CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryResponse {
    /// Node information response
    NodeInfo(NodeInfo),
    
    /// Node metrics response
    NodeMetrics(NodeMetrics),
    
    /// Node health status response
    NodeHealth(HealthStatus),
    
    /// List of nodes response
    NodeList(Vec<NodeInfo>),
    
    /// Job status response
    JobStatus(JobStatusInfo),
    
    /// List of jobs response
    JobList(Vec<JobInfo>),
    
    /// Job result response
    JobResult(JobResultInfo),
    
    /// Pong response (reply to ping)
    Pong {
        timestamp: u64,
    },
    
    /// Job submitted response
    JobSubmitted(JobSubmitResponse),
    
    /// Job cancelled response
    JobCancelled {
        job_id: String,
        success: bool,
        message: Option<String>,
    },
    
    /// Error response
    Error {
        message: String,
        code: String,
    },
}

impl QueryResponse {
    /// Create an error response
    pub fn error(message: impl Into<String>, code: impl Into<String>) -> Self {
        QueryResponse::Error {
            message: message.into(),
            code: code.into(),
        }
    }
    
    /// Create a "not found" error
    pub fn not_found(entity: &str, id: &str) -> Self {
        QueryResponse::Error {
            message: format!("{} not found: {}", entity, id),
            code: "NOT_FOUND".to_string(),
        }
    }
    
    /// Create an "unsupported" error
    pub fn unsupported(message: impl Into<String>) -> Self {
        QueryResponse::Error {
            message: message.into(),
            code: "UNSUPPORTED_QUERY".to_string(),
        }
    }
    
    /// Create a pong response
    pub fn pong() -> Self {
        QueryResponse::Pong {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }
    
    /// Check if this is an error response
    pub fn is_error(&self) -> bool {
        matches!(self, QueryResponse::Error { .. })
    }
}

/// Node information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeInfo {
    /// Unique node identifier
    pub node_id: String,
    
    /// libp2p peer ID
    pub peer_id: String,
    
    /// Listening addresses
    pub addresses: Vec<String>,
    
    /// Current node status (online, busy, offline)
    pub status: String,
    
    /// Available models on this node
    pub models: Vec<String>,
    
    /// Uptime in seconds
    pub uptime_seconds: u64,
    
    /// Number of connected peers
    pub peer_count: usize,
    
    /// Queue capacity (max jobs)
    pub queue_capacity: usize,
    
    /// Current queue size
    pub queue_size: usize,
    
    /// Node version
    pub version: String,
}

impl Default for NodeInfo {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            peer_id: String::new(),
            addresses: Vec::new(),
            status: "unknown".to_string(),
            models: Vec::new(),
            uptime_seconds: 0,
            peer_count: 0,
            queue_capacity: 0,
            queue_size: 0,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Node metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeMetrics {
    /// Total jobs processed
    pub jobs_processed: u64,
    
    /// Jobs currently in queue
    pub jobs_queued: usize,
    
    /// Jobs completed successfully
    pub jobs_completed: u64,
    
    /// Jobs that failed
    pub jobs_failed: u64,
    
    /// Average inference latency in milliseconds
    pub avg_inference_latency_ms: f64,
    
    /// Total tokens processed
    pub tokens_processed: u64,
    
    /// Current memory usage in bytes
    pub memory_usage_bytes: u64,
    
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    
    /// Network bytes sent
    pub network_bytes_sent: u64,
    
    /// Network bytes received
    pub network_bytes_received: u64,
    
    /// Uptime in seconds
    pub uptime_seconds: u64,
}

impl Default for NodeMetrics {
    fn default() -> Self {
        Self {
            jobs_processed: 0,
            jobs_queued: 0,
            jobs_completed: 0,
            jobs_failed: 0,
            avg_inference_latency_ms: 0.0,
            tokens_processed: 0,
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
            network_bytes_sent: 0,
            network_bytes_received: 0,
            uptime_seconds: 0,
        }
    }
}

/// Health status for a node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthStatus {
    /// Overall health status
    pub overall: String,
    
    /// Network health
    pub network: ComponentHealth,
    
    /// Model/inference health
    pub model: ComponentHealth,
    
    /// Queue health
    pub queue: ComponentHealth,
    
    /// Memory health
    pub memory: ComponentHealth,
    
    /// CPU health
    pub cpu: ComponentHealth,
    
    /// Timestamp of health check
    pub timestamp: u64,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            overall: "unknown".to_string(),
            network: ComponentHealth::default(),
            model: ComponentHealth::default(),
            queue: ComponentHealth::default(),
            memory: ComponentHealth::default(),
            cpu: ComponentHealth::default(),
            timestamp: 0,
        }
    }
}

/// Health status for a single component
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentHealth {
    /// Component status (healthy, degraded, unhealthy)
    pub status: String,
    
    /// Optional message
    pub message: Option<String>,
    
    /// Usage percentage (0-100) if applicable
    pub usage_percent: Option<f64>,
}

impl Default for ComponentHealth {
    fn default() -> Self {
        Self {
            status: "unknown".to_string(),
            message: None,
            usage_percent: None,
        }
    }
}

impl ComponentHealth {
    /// Create a healthy component status
    pub fn healthy() -> Self {
        Self {
            status: "healthy".to_string(),
            message: None,
            usage_percent: None,
        }
    }
    
    /// Create a healthy component with usage
    pub fn healthy_with_usage(usage: f64) -> Self {
        Self {
            status: "healthy".to_string(),
            message: None,
            usage_percent: Some(usage),
        }
    }
    
    /// Create a degraded component status
    pub fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: "degraded".to_string(),
            message: Some(message.into()),
            usage_percent: None,
        }
    }
    
    /// Create an unhealthy component status
    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            status: "unhealthy".to_string(),
            message: Some(message.into()),
            usage_percent: None,
        }
    }
}

/// Job status information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobStatusInfo {
    /// Job identifier
    pub job_id: String,
    
    /// Current status (pending, running, completed, failed, cancelled)
    pub status: String,
    
    /// Progress percentage (0-100)
    pub progress: Option<f64>,
    
    /// Node processing this job
    pub node_id: Option<String>,
    
    /// Submission timestamp
    pub submitted_at: u64,
    
    /// Start timestamp (when processing began)
    pub started_at: Option<u64>,
    
    /// Completion timestamp
    pub completed_at: Option<u64>,
    
    /// Error message if failed
    pub error: Option<String>,
}

impl Default for JobStatusInfo {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            status: "unknown".to_string(),
            progress: None,
            node_id: None,
            submitted_at: 0,
            started_at: None,
            completed_at: None,
            error: None,
        }
    }
}

/// Brief job information for listing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobInfo {
    /// Job identifier
    pub job_id: String,
    
    /// Current status
    pub status: String,
    
    /// Model used
    pub model: String,
    
    /// Node processing this job
    pub node_id: Option<String>,
    
    /// Submission timestamp
    pub submitted_at: u64,
}

/// Job result information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobResultInfo {
    /// Job identifier
    pub job_id: String,
    
    /// Completion status
    pub status: String,
    
    /// Output result
    pub output: Option<String>,
    
    /// Execution metrics
    pub metrics: JobExecutionMetrics,
    
    /// Error message if failed
    pub error: Option<String>,
}

/// Execution metrics for a job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JobExecutionMetrics {
    /// Duration in milliseconds
    pub duration_ms: u64,
    
    /// Tokens processed
    pub tokens_processed: Option<u64>,
    
    /// Input tokens
    pub input_tokens: Option<u64>,
    
    /// Output tokens
    pub output_tokens: Option<u64>,
}

/// Response for job submission
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobSubmitResponse {
    /// Assigned job ID
    pub job_id: String,
    
    /// Node that accepted the job
    pub node_id: String,
    
    /// Initial status
    pub status: String,
    
    /// Estimated queue position
    pub queue_position: Option<usize>,
    
    /// Submission timestamp
    pub submitted_at: u64,
}

impl JobSubmitResponse {
    /// Create a new job submission response
    pub fn new(job_id: impl Into<String>, node_id: impl Into<String>) -> Self {
        Self {
            job_id: job_id.into(),
            node_id: node_id.into(),
            status: "pending".to_string(),
            queue_position: None,
            submitted_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }
    
    /// Set queue position
    pub fn with_queue_position(mut self, position: usize) -> Self {
        self.queue_position = Some(position);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_query_serialization() {
        let query = Query::get_node_info("node-123");
        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("\"type\":\"get_node_info\""));
        assert!(json.contains("\"node_id\":\"node-123\""));
        
        let deserialized: Query = serde_json::from_str(&json).unwrap();
        assert_eq!(query, deserialized);
    }
    
    #[test]
    fn test_query_response_serialization() {
        let response = QueryResponse::error("Something failed", "ERROR_CODE");
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"Something failed\""));
        
        let deserialized: QueryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }
    
    #[test]
    fn test_node_info_serialization() {
        let info = NodeInfo {
            node_id: "node-1".to_string(),
            peer_id: "12D3KooW...".to_string(),
            addresses: vec!["/ip4/127.0.0.1/tcp/4001".to_string()],
            status: "online".to_string(),
            models: vec!["tinyllama".to_string()],
            uptime_seconds: 3600,
            peer_count: 5,
            queue_capacity: 100,
            queue_size: 10,
            version: "0.1.0".to_string(),
        };
        
        let response = QueryResponse::NodeInfo(info.clone());
        let json = serde_json::to_string(&response).unwrap();
        
        let deserialized: QueryResponse = serde_json::from_str(&json).unwrap();
        if let QueryResponse::NodeInfo(deserialized_info) = deserialized {
            assert_eq!(info, deserialized_info);
        } else {
            panic!("Expected NodeInfo response");
        }
    }
    
    #[test]
    fn test_health_status_serialization() {
        let health = HealthStatus {
            overall: "healthy".to_string(),
            network: ComponentHealth::healthy(),
            model: ComponentHealth::healthy_with_usage(50.0),
            queue: ComponentHealth::degraded("High queue depth"),
            memory: ComponentHealth::healthy_with_usage(75.0),
            cpu: ComponentHealth::healthy_with_usage(30.0),
            timestamp: 1234567890,
        };
        
        let response = QueryResponse::NodeHealth(health);
        let json = serde_json::to_string(&response).unwrap();
        
        assert!(json.contains("\"type\":\"node_health\""));
        assert!(json.contains("\"overall\":\"healthy\""));
    }
    
    #[test]
    fn test_job_filter_serialization() {
        let filter = JobFilter {
            status: Some("pending".to_string()),
            model: None,
            limit: Some(10),
            offset: None,
        };
        
        let query = Query::list_jobs(Some(filter));
        let json = serde_json::to_string(&query).unwrap();
        
        assert!(json.contains("\"type\":\"list_jobs\""));
        assert!(json.contains("\"status\":\"pending\""));
        assert!(json.contains("\"limit\":10"));
    }
    
    #[test]
    fn test_all_query_types() {
        // Test all query constructors
        let queries = vec![
            Query::get_node_info("node-1"),
            Query::get_node_metrics("node-1"),
            Query::get_node_health("node-1"),
            Query::list_nodes(),
            Query::get_job_status("job-1"),
            Query::list_jobs(None),
            Query::get_job_result("job-1"),
            Query::ping(),
            Query::submit_job("tinyllama", "Hello world"),
            Query::cancel_job("job-1"),
        ];
        
        for query in queries {
            let json = serde_json::to_string(&query).unwrap();
            let deserialized: Query = serde_json::from_str(&json).unwrap();
            assert_eq!(query, deserialized);
        }
    }
    
    #[test]
    fn test_submit_job_query() {
        let query = Query::submit_job("tinyllama", "Test input");
        let json = serde_json::to_string(&query).unwrap();
        
        assert!(json.contains("\"type\":\"submit_job\""));
        assert!(json.contains("\"model\":\"tinyllama\""));
        assert!(json.contains("\"input\":\"Test input\""));
        
        let deserialized: Query = serde_json::from_str(&json).unwrap();
        assert_eq!(query, deserialized);
    }
    
    #[test]
    fn test_cancel_job_query() {
        let query = Query::cancel_job("job-123");
        let json = serde_json::to_string(&query).unwrap();
        
        assert!(json.contains("\"type\":\"cancel_job\""));
        assert!(json.contains("\"job_id\":\"job-123\""));
        
        let deserialized: Query = serde_json::from_str(&json).unwrap();
        assert_eq!(query, deserialized);
    }
    
    #[test]
    fn test_job_submitted_response() {
        let response = JobSubmitResponse::new("job-1", "node-1")
            .with_queue_position(5);
        let wrapped = QueryResponse::JobSubmitted(response);
        let json = serde_json::to_string(&wrapped).unwrap();
        
        assert!(json.contains("\"type\":\"job_submitted\""));
        assert!(json.contains("\"job_id\":\"job-1\""));
        assert!(json.contains("\"queue_position\":5"));
        
        let deserialized: QueryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(wrapped, deserialized);
    }
    
    #[test]
    fn test_job_cancelled_response() {
        let response = QueryResponse::JobCancelled {
            job_id: "job-123".to_string(),
            success: true,
            message: Some("Cancelled successfully".to_string()),
        };
        let json = serde_json::to_string(&response).unwrap();
        
        assert!(json.contains("\"type\":\"job_cancelled\""));
        assert!(json.contains("\"success\":true"));
        
        let deserialized: QueryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(response, deserialized);
    }
    
    #[test]
    fn test_version_field() {
        let query = Query::get_node_info("node-1");
        assert_eq!(query.version(), QUERY_PROTOCOL_VERSION);
    }
    
    #[test]
    fn test_error_response_helpers() {
        let not_found = QueryResponse::not_found("Job", "job-123");
        assert!(not_found.is_error());
        
        if let QueryResponse::Error { code, .. } = not_found {
            assert_eq!(code, "NOT_FOUND");
        }
        
        let unsupported = QueryResponse::unsupported("Unknown query type");
        if let QueryResponse::Error { code, .. } = unsupported {
            assert_eq!(code, "UNSUPPORTED_QUERY");
        }
    }
    
    #[test]
    fn test_pong_response() {
        let pong = QueryResponse::pong();
        if let QueryResponse::Pong { timestamp } = pong {
            assert!(timestamp > 0);
        } else {
            panic!("Expected Pong response");
        }
    }
}

