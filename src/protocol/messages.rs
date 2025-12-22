//! Protocol message types based on established schemas

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Job execution mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobMode {
    Batch,
    Session,
}

impl Default for JobMode {
    fn default() -> Self {
        JobMode::Batch
    }
}

/// Payment mode for sessions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMode {
    Stream,
    PerWindow,
    Postpaid,
}

/// Session configuration for streaming jobs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens_per_window: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_windows: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_mode: Option<PaymentMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval_sec: Option<i32>,
}

/// Job requirements (flexible object)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobRequirements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_vram_gb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_ram_gb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_vendor: Option<String>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Work offer message (job.schema.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkOffer {
    pub job_id: String,
    pub model: String,
    #[serde(default)]
    pub mode: JobMode,
    pub reward: f64,
    pub currency: String,
    #[serde(default)]
    pub requirements: JobRequirements,
    #[serde(default)]
    pub policy_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionConfig>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Execution manifest policies
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestPolicies {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redundancy_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redundancy_max: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_allowed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geos_allowed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_job_duration_sec: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_terms: Option<String>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Execution manifest capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ManifestCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_modes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_vram_gb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_tee: Option<bool>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Execution manifest (manifest.schema.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionManifest {
    pub manifest_id: String,
    pub version: String,
    pub issuer_pubkey: String,
    pub issued_at: i64, // Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policies: Option<ManifestPolicies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ManifestCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Job receipt (receipt.schema.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobReceipt {
    pub receipt_id: String,
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i32>, // For session receipts
    pub executor_pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_id: Option<String>,
    pub outputs_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy_kwh_estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>, // Unix timestamp
    pub completed_at: i64, // Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_proof: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_window_seconds: Option<i32>,
    pub signature: String,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

/// Node announcement message for P2P network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceMsg {
    pub manifest_id: String,
    pub version: String,
    pub manifest_hash: String,
    pub capabilities: NodeCapabilities,
    pub payment_preferences: Vec<String>,
    pub timestamp: i64,
    pub peer_id: String,
}

/// Node hardware capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_gb: Option<f64>,
}

/// Job claim message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobClaimMsg {
    pub job_id: String,
    pub agent_peer_id: String,
    pub manifest_id: String,
    pub timestamp: i64,
}

/// Job status for receipts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_processed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_memory_mb: Option<u64>,
}

/// Wrapper enum for all P2P messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum P2PMessage {
    Announce(AnnounceMsg),
    WorkOffer(WorkOffer),
    JobClaim(JobClaimMsg),
    Receipt(JobReceipt),
}

impl P2PMessage {
    /// Serialize message to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize message from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }

    /// Get message type as string
    pub fn message_type(&self) -> &'static str {
        match self {
            P2PMessage::Announce(_) => "announce",
            P2PMessage::WorkOffer(_) => "work_offer",
            P2PMessage::JobClaim(_) => "job_claim",
            P2PMessage::Receipt(_) => "receipt",
        }
    }
}