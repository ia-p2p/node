//! MVP Node - Core interfaces and types for decentralized AI network
//!
//! This library defines the core traits and interfaces that components
//! must implement to participate in the decentralized AI network.

use anyhow::Result;
use async_trait::async_trait;
use libp2p::{Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export modules
pub mod config;
pub mod protocol;
pub mod network;

// Core data types
pub use config::NodeConfig;

/// Network configuration for P2P layer
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub listen_addr: Multiaddr,
    pub bootstrap_peers: Vec<Multiaddr>,
}

/// Job processing status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub pending_jobs: usize,
    pub processing_job: Option<String>,
    pub completed_jobs: usize,
    pub failed_jobs: usize,
}

/// Information about loaded AI model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub parameters: u64,
    pub memory_usage_mb: u64,
    pub supported_tasks: Vec<String>,
}

/// Result of inference execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResult {
    pub output: String,
    pub tokens_processed: u64,
    pub duration_ms: u64,
    pub memory_used_mb: u64,
}

/// Job offer from network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOffer {
    pub job_id: String,
    pub model: String,
    pub mode: JobMode,
    pub reward: f64,
    pub currency: String,
    pub requirements: Requirements,
    pub input_data: String,
}

/// Job execution mode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobMode {
    Batch,
    Session,
}

/// Job requirements
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Requirements {
    pub min_vram_gb: Option<f64>,
    pub min_ram_gb: Option<f64>,
    pub gpu_vendor: Option<String>,
}

/// Job execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: String,
    pub status: JobStatus,
    pub output: Option<String>,
    pub metrics: ExecutionMetrics,
    pub error: Option<String>,
}

/// Job execution status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Completed,
    Failed,
    Timeout,
}

/// Execution metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionMetrics {
    pub duration_ms: u64,
    pub tokens_processed: Option<u64>,
    pub peak_memory_mb: Option<u64>,
}

/// Node identity and capabilities
#[derive(Debug, Clone)]
pub struct NodeIdentity {
    pub peer_id: PeerId,
    pub manifest_id: String,
}

/// Hardware capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub hostname: String,
    pub total_memory_gb: f64,
    pub cpus: Vec<String>,
    pub gpus: Vec<GpuInfo>,
}

/// GPU information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub driver_version: Option<String>,
    pub memory_gb: Option<f64>,
}

// Core trait interfaces

/// Network layer interface for P2P communication
#[async_trait]
pub trait NetworkLayer: Send {
    /// Start the network layer with given configuration
    async fn start(&mut self, config: NetworkConfig) -> Result<()>;
    
    /// Publish a message to a specific topic
    async fn publish_message(&mut self, topic: &str, message: &[u8]) -> Result<()>;
    
    /// Subscribe to a topic for receiving messages
    async fn subscribe_topic(&mut self, topic: &str) -> Result<()>;
    
    /// Get current number of connected peers
    fn get_peer_count(&self) -> usize;
    
    /// Get local peer ID
    fn get_local_peer_id(&self) -> PeerId;
    
    /// Get listening addresses
    fn get_listeners(&self) -> Vec<Multiaddr>;
    
    /// Shutdown the network layer gracefully
    async fn shutdown(&mut self) -> Result<()>;
}

/// Job executor interface for managing job lifecycle
#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// Submit a new job for processing
    async fn submit_job(&mut self, job: JobOffer) -> Result<String>;
    
    /// Process the next job in queue
    async fn process_next_job(&mut self) -> Result<Option<JobResult>>;
    
    /// Get current queue status
    fn get_queue_status(&self) -> QueueStatus;
    
    /// Cancel a specific job
    async fn cancel_job(&mut self, job_id: &str) -> Result<()>;
    
    /// Get maximum queue size
    fn get_max_queue_size(&self) -> usize;
    
    /// Check if queue is full
    fn is_queue_full(&self) -> bool;
}

/// Inference engine interface for AI model execution
#[async_trait]
pub trait InferenceEngine: Send + Sync {
    /// Load a model from the specified path
    async fn load_model(&mut self, model_path: &str) -> Result<()>;
    
    /// Execute inference on the given input
    async fn infer(&self, input: &str) -> Result<InferenceResult>;
    
    /// Get information about the loaded model
    fn get_model_info(&self) -> Option<ModelInfo>;
    
    /// Check if a model is currently loaded
    fn is_model_loaded(&self) -> bool;
    
    /// Unload the current model to free memory
    async fn unload_model(&mut self) -> Result<()>;
    
    /// Validate input against model requirements
    fn validate_input(&self, input: &str) -> Result<()>;
}

/// Protocol handler interface for message validation and compliance
#[async_trait]
pub trait ProtocolHandler: Send + Sync {
    /// Create a manifest describing node capabilities
    fn create_manifest(&self, capabilities: &Capabilities) -> Result<serde_json::Value>;
    
    /// Create a receipt for completed job
    fn create_receipt(&self, job_result: &JobResult) -> Result<serde_json::Value>;
    
    /// Validate a job offer against schema
    fn validate_job_offer(&self, offer: &JobOffer) -> Result<()>;
    
    /// Sign a message with node's private key
    fn sign_message(&self, message: &[u8]) -> Result<String>;
    
    /// Verify a message signature
    fn verify_signature(&self, message: &[u8], signature: &str, public_key: &str) -> Result<bool>;
    
    /// Validate message against protocol schema
    fn validate_message_schema(&self, message: &serde_json::Value, schema_type: &str) -> Result<()>;
}

/// Health and monitoring interface
pub trait HealthMonitor: Send + Sync {
    /// Get current health status
    fn get_health_status(&self) -> HashMap<String, serde_json::Value>;
    
    /// Get performance metrics
    fn get_metrics(&self) -> HashMap<String, f64>;
    
    /// Record a metric value
    fn record_metric(&mut self, name: &str, value: f64);
    
    /// Check if node is healthy
    fn is_healthy(&self) -> bool;
}

/// Configuration management interface
pub trait ConfigManager: Send + Sync {
    /// Load configuration from file
    fn load_config(path: &str) -> Result<NodeConfig>;
    
    /// Validate configuration parameters
    fn validate_config(config: &NodeConfig) -> Result<()>;
    
    /// Generate unique node identity
    fn generate_identity() -> Result<NodeIdentity>;
    
    /// Save configuration to file
    fn save_config(config: &NodeConfig, path: &str) -> Result<()>;
}