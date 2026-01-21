//! Configuration management for MVP Node
//!
//! This module provides configuration loading, validation, and management
//! for multi-instance support.
//!
//! Implements Requirements 8.1, 8.2, 8.3, 8.4, 8.5, 10.1

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::net::TcpListener;
use anyhow::{Result, anyhow};
use crate::{ConfigManager, NodeIdentity};
use crate::orchestration::{
    OrchestratorConfig as OrchestratorConfigInternal,
    GenerationConfig as GenerationConfigInternal,
    CacheConfig as CacheConfigInternal,
    TrainingConfig as TrainingConfigInternal,
    SafetyConfig as SafetyConfigInternal,
};
use libp2p::{identity::Keypair, PeerId};
use std::sync::atomic::{AtomicU16, Ordering};

/// Base port for automatic assignment when port=0
const BASE_PORT: u16 = 9000;

/// Port range for automatic assignment
const PORT_RANGE: u16 = 1000;

/// Atomic counter for port assignment across instances
static PORT_COUNTER: AtomicU16 = AtomicU16::new(0);

/// Node configuration with multi-instance support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Unique node identifier (auto-generated if "auto")
    pub node_id: String,
    
    /// Network listen port (0 for auto-assignment)
    pub listen_port: u16,
    
    /// Bootstrap peer addresses for network discovery
    pub bootstrap_peers: Vec<String>,
    
    /// Path to the AI model file
    pub model_path: String,
    
    /// Maximum job queue size
    pub max_queue_size: usize,
    
    /// Logging level (trace, debug, info, warn, error)
    pub log_level: String,
    
    /// Instance-specific data directory (optional)
    #[serde(default)]
    pub data_dir: Option<String>,
    
    /// Enable mDNS for local discovery (default: true)
    #[serde(default = "default_mdns")]
    pub enable_mdns: bool,
    
    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,
    
    /// Maximum number of peers
    #[serde(default = "default_max_peers")]
    pub max_peers: usize,
    
    /// LLM Orchestrator configuration (optional)
    #[serde(default)]
    pub orchestrator: Option<OrchestratorFileConfig>,
    
    /// Distributed Orchestration configuration (optional)
    #[serde(default)]
    pub distributed_orchestration: Option<DistributedOrchestrationFileConfig>,
}

/// Orchestrator configuration as read from TOML file
/// This maps to the internal OrchestratorConfig
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrchestratorFileConfig {
    /// Whether the orchestrator is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    /// Path to the model files
    #[serde(default = "default_model_path")]
    pub model_path: String,
    
    /// Type of model to use
    #[serde(default = "default_model_type")]
    pub model_type: String,
    
    /// Device to use for inference
    #[serde(default = "default_device")]
    pub device: String,
    
    /// Generation parameters
    #[serde(default)]
    pub generation: GenerationFileConfig,
    
    /// Cache configuration
    #[serde(default)]
    pub cache: CacheFileConfig,
    
    /// Training data collection configuration
    #[serde(default)]
    pub training: TrainingFileConfig,
    
    /// Safety configuration
    #[serde(default)]
    pub safety: SafetyFileConfig,
}

fn default_enabled() -> bool { true }
fn default_model_path() -> String { "./models/llama3-2-3b-instruct".to_string() }
fn default_model_type() -> String { "llama3_2_3b".to_string() }
fn default_device() -> String { "auto".to_string() }

/// Generation parameters from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationFileConfig {
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_top_p")]
    pub top_p: f64,
    #[serde(default = "default_repetition_penalty")]
    pub repetition_penalty: f64,
}

fn default_max_tokens() -> usize { 512 }
fn default_temperature() -> f64 { 0.7 }
fn default_top_p() -> f64 { 0.9 }
fn default_repetition_penalty() -> f64 { 1.1 }

impl Default for GenerationFileConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            repetition_penalty: default_repetition_penalty(),
        }
    }
}

/// Cache configuration from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheFileConfig {
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_ttl")]
    pub ttl_seconds: u64,
}

fn default_cache_enabled() -> bool { true }
fn default_max_entries() -> usize { 1000 }
fn default_ttl() -> u64 { 300 }

impl Default for CacheFileConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
            max_entries: default_max_entries(),
            ttl_seconds: default_ttl(),
        }
    }
}

/// Training configuration from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingFileConfig {
    #[serde(default = "default_collect_data")]
    pub collect_data: bool,
    #[serde(default = "default_export_path")]
    pub export_path: String,
    #[serde(default = "default_auto_export_threshold")]
    pub auto_export_threshold: usize,
}

fn default_collect_data() -> bool { true }
fn default_export_path() -> String { "./training_data".to_string() }
fn default_auto_export_threshold() -> usize { 1000 }

impl Default for TrainingFileConfig {
    fn default() -> Self {
        Self {
            collect_data: default_collect_data(),
            export_path: default_export_path(),
            auto_export_threshold: default_auto_export_threshold(),
        }
    }
}

/// Safety configuration from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyFileConfig {
    #[serde(default = "default_allowed_actions")]
    pub allowed_actions: Vec<String>,
    #[serde(default = "default_max_confidence")]
    pub max_confidence_threshold: f64,
    #[serde(default = "default_require_confirmation")]
    pub require_confirmation_above: f64,
}

fn default_allowed_actions() -> Vec<String> {
    vec![
        "start_election".to_string(),
        "migrate_context".to_string(),
        "adjust_gossip".to_string(),
        "scale_queue".to_string(),
        "wait".to_string(),
        "restart_component".to_string(),
    ]
}
fn default_max_confidence() -> f64 { 0.95 }
fn default_require_confirmation() -> f64 { 0.8 }

impl Default for SafetyFileConfig {
    fn default() -> Self {
        Self {
            allowed_actions: default_allowed_actions(),
            max_confidence_threshold: default_max_confidence(),
            require_confirmation_above: default_require_confirmation(),
        }
    }
}

// ============================================================================
// Distributed Orchestration Configuration
// ============================================================================

/// Distributed Orchestration configuration from TOML file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedOrchestrationFileConfig {
    /// Whether distributed orchestration is enabled
    #[serde(default = "default_distributed_enabled")]
    pub enabled: bool,
    
    /// Orchestration mode: "emergent", "permanent", or "fully_decentralized"
    #[serde(default = "default_distributed_mode")]
    pub mode: String,
    
    /// Rotation trigger: "periodic", "event_driven", or "adaptive"
    #[serde(default = "default_rotation_trigger")]
    pub rotation_trigger: String,
    
    /// Jobs before rotation (for periodic trigger)
    #[serde(default = "default_rotation_interval")]
    pub rotation_interval_jobs: u64,
    
    /// High value threshold for LLM strategy
    #[serde(default = "default_high_value_threshold")]
    pub high_value_threshold: f64,
    
    /// Complexity threshold for hybrid strategy
    #[serde(default = "default_complexity_threshold")]
    pub complexity_threshold: f64,
    
    /// Minimum nodes per context group
    #[serde(default = "default_min_nodes_per_group")]
    pub min_nodes_per_group: usize,
    
    /// Maximum groups a node can join
    #[serde(default = "default_max_groups_per_node")]
    pub max_groups_per_node: usize,
    
    /// Heartbeat timeout for partition detection (seconds)
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u64,
    
    /// Consensus timeout (milliseconds)
    #[serde(default = "default_consensus_timeout")]
    pub consensus_timeout_ms: u64,
    
    /// Maximum consensus rounds
    #[serde(default = "default_max_consensus_rounds")]
    pub max_consensus_rounds: usize,
    
    /// Base threshold for adaptive consensus
    #[serde(default = "default_base_threshold")]
    pub base_consensus_threshold: f64,
}

fn default_distributed_enabled() -> bool { false }
fn default_distributed_mode() -> String { "emergent".to_string() }
fn default_rotation_trigger() -> String { "adaptive".to_string() }
fn default_rotation_interval() -> u64 { 100 }
fn default_high_value_threshold() -> f64 { 10.0 }
fn default_complexity_threshold() -> f64 { 0.7 }
fn default_min_nodes_per_group() -> usize { 2 }
fn default_max_groups_per_node() -> usize { 3 }
fn default_heartbeat_timeout() -> u64 { 15 }
fn default_consensus_timeout() -> u64 { 5000 }
fn default_max_consensus_rounds() -> usize { 3 }
fn default_base_threshold() -> f64 { 0.6 }

impl Default for DistributedOrchestrationFileConfig {
    fn default() -> Self {
        Self {
            enabled: default_distributed_enabled(),
            mode: default_distributed_mode(),
            rotation_trigger: default_rotation_trigger(),
            rotation_interval_jobs: default_rotation_interval(),
            high_value_threshold: default_high_value_threshold(),
            complexity_threshold: default_complexity_threshold(),
            min_nodes_per_group: default_min_nodes_per_group(),
            max_groups_per_node: default_max_groups_per_node(),
            heartbeat_timeout_secs: default_heartbeat_timeout(),
            consensus_timeout_ms: default_consensus_timeout(),
            max_consensus_rounds: default_max_consensus_rounds(),
            base_consensus_threshold: default_base_threshold(),
        }
    }
}

impl DistributedOrchestrationFileConfig {
    /// Convert to internal DistributedOrchestratorConfig
    pub fn to_internal(&self) -> crate::distributed_orchestration::DistributedOrchestratorConfig {
        use crate::distributed_orchestration::{
            DistributedOrchestratorConfig,
            types::{OrchestrationMode, RotationTrigger, ConsensusCriteria, AffinityWeights},
            config::{DecisionRoutingConfig, ContextGroupConfig, PartitionHandlingConfig},
        };
        
        let mode = match self.mode.as_str() {
            "permanent" => OrchestrationMode::PermanentCoordinator {
                coordinator_id: String::new(), // Will be set at runtime
            },
            "fully_decentralized" => OrchestrationMode::FullyDecentralized {
                consensus_for_every_decision: true,
            },
            _ => OrchestrationMode::EmergentCoordinator {
                rotation_trigger: match self.rotation_trigger.as_str() {
                    "periodic" => RotationTrigger::Periodic {
                        interval_jobs: self.rotation_interval_jobs,
                    },
                    "event_driven" => RotationTrigger::EventDriven,
                    _ => RotationTrigger::Adaptive,
                },
                affinity_recalc_interval_secs: 60,
            },
        };
        
        DistributedOrchestratorConfig {
            mode,
            affinity_weights: AffinityWeights::default(),
            consensus_criteria: ConsensusCriteria::AdaptiveThreshold {
                base_threshold: self.base_consensus_threshold,
                stimulus_weight: 0.6,
                demand_weight: 0.4,
                min_participants: 2,
            },
            decision_routing: DecisionRoutingConfig {
                high_value_threshold: self.high_value_threshold,
                complexity_threshold: self.complexity_threshold,
                ..Default::default()
            },
            context_groups: ContextGroupConfig {
                min_nodes_per_group: self.min_nodes_per_group,
                max_groups_per_node: self.max_groups_per_node,
                ..Default::default()
            },
            partition_handling: PartitionHandlingConfig {
                heartbeat_timeout_secs: self.heartbeat_timeout_secs,
                ..Default::default()
            },
            max_consensus_rounds: self.max_consensus_rounds,
            consensus_timeout_ms: self.consensus_timeout_ms,
            enabled: self.enabled,
            simulation_mode: false, // Can be enabled via environment variable
        }
    }
}

impl OrchestratorFileConfig {
    /// Convert to internal OrchestratorConfig
    pub fn to_internal(&self) -> OrchestratorConfigInternal {
        OrchestratorConfigInternal {
            enabled: self.enabled,
            model_path: self.model_path.clone(),
            model_type: self.model_type.clone(),
            device: self.device.clone(),
            generation: GenerationConfigInternal {
                max_tokens: self.generation.max_tokens,
                temperature: self.generation.temperature,
                top_p: self.generation.top_p,
                repetition_penalty: self.generation.repetition_penalty,
            },
            cache: CacheConfigInternal {
                enabled: self.cache.enabled,
                max_entries: self.cache.max_entries,
                ttl_seconds: self.cache.ttl_seconds,
            },
            training: TrainingConfigInternal {
                collect_data: self.training.collect_data,
                export_path: self.training.export_path.clone(),
                auto_export_threshold: self.training.auto_export_threshold,
            },
            safety: SafetyConfigInternal {
                allowed_actions: self.safety.allowed_actions.clone(),
                max_confidence_threshold: self.safety.max_confidence_threshold,
                require_confirmation_above: self.safety.require_confirmation_above,
            },
        }
    }
}

fn default_mdns() -> bool { true }
fn default_connection_timeout() -> u64 { 30 }
fn default_max_peers() -> usize { 50 }

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: "auto".to_string(),
            listen_port: 0, // Auto-assign
            bootstrap_peers: Vec::new(),
            model_path: "./models/micromodel.gguf".to_string(),
            max_queue_size: 100,
            log_level: "info".to_string(),
            data_dir: None,
            enable_mdns: true,
            connection_timeout_secs: 30,
            max_peers: 50,
            orchestrator: None,
            distributed_orchestration: None,
        }
    }
}

impl NodeConfig {
    /// Get the orchestrator configuration, converting from file format if present
    pub fn get_orchestrator_config(&self) -> OrchestratorConfigInternal {
        self.orchestrator
            .as_ref()
            .map(|c| c.to_internal())
            .unwrap_or_default()
    }
    
    /// Check if orchestrator is enabled in config
    pub fn is_orchestrator_enabled(&self) -> bool {
        self.orchestrator
            .as_ref()
            .map(|c| c.enabled)
            .unwrap_or(false)
    }
    
    /// Get the distributed orchestration configuration, converting from file format if present
    pub fn get_distributed_orchestration_config(&self) -> crate::distributed_orchestration::DistributedOrchestratorConfig {
        self.distributed_orchestration
            .as_ref()
            .map(|c| c.to_internal())
            .unwrap_or_default()
    }
    
    /// Check if distributed orchestration is enabled in config
    pub fn is_distributed_orchestration_enabled(&self) -> bool {
        self.distributed_orchestration
            .as_ref()
            .map(|c| c.enabled)
            .unwrap_or(false)
    }
}

impl NodeConfig {
    /// Create a new NodeConfig with auto-generated unique settings
    pub fn new_instance() -> Self {
        let mut config = Self::default();
        config.node_id = generate_unique_node_id();
        config.listen_port = find_available_port().unwrap_or(0);
        config
    }
    
    /// Create a config for a specific instance number
    pub fn for_instance(instance_num: u16) -> Self {
        let mut config = Self::default();
        config.node_id = format!("node-{}", instance_num);
        config.listen_port = BASE_PORT + instance_num;
        config.data_dir = Some(format!("./data/instance-{}", instance_num));
        config
    }
    
    /// Assign an available port if current port is 0 or in use
    pub fn ensure_available_port(&mut self) -> Result<u16> {
        if self.listen_port == 0 {
            self.listen_port = find_available_port()?;
        } else if !is_port_available(self.listen_port) {
            self.listen_port = find_available_port()?;
        }
        Ok(self.listen_port)
    }
    
    /// Get the multiaddr for this node's listen address
    pub fn get_listen_multiaddr(&self) -> String {
        format!("/ip4/127.0.0.1/tcp/{}", self.listen_port)
    }
    
    /// Add a bootstrap peer
    pub fn add_bootstrap_peer(&mut self, addr: &str) {
        if !self.bootstrap_peers.contains(&addr.to_string()) {
            self.bootstrap_peers.push(addr.to_string());
        }
    }
    
    /// Create a multiaddr pointing to another instance
    pub fn peer_multiaddr(port: u16) -> String {
        format!("/ip4/127.0.0.1/tcp/{}", port)
    }
}

/// Find an available port for binding
pub fn find_available_port() -> Result<u16> {
    // Try sequential ports from base
    let counter = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let start_port = BASE_PORT + (counter % PORT_RANGE);
    
    for offset in 0..PORT_RANGE {
        let port = start_port + offset;
        if port > u16::MAX - 1 {
            continue;
        }
        if is_port_available(port) {
            return Ok(port);
        }
    }
    
    // Fallback: let OS assign a port
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Check if a port is available for binding
pub fn is_port_available(port: u16) -> bool {
    TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
}

/// Generate a unique node ID based on timestamp and random bytes
pub fn generate_unique_node_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    
    let random_bytes: [u8; 4] = rand::random();
    format!("node-{:x}-{}", timestamp % 0xFFFF, hex::encode(random_bytes))
}

/// Default configuration manager implementation
pub struct DefaultConfigManager;

impl ConfigManager for DefaultConfigManager {
    fn load_config(path: &str) -> Result<NodeConfig> {
        if Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            let mut config: NodeConfig = toml::from_str(&content)?;
            
            // Auto-generate node_id if set to "auto"
            if config.node_id == "auto" {
                config.node_id = generate_unique_node_id();
            }
            
            Ok(config)
        } else {
            // Create default config file
            let mut default_config = NodeConfig::default();
            default_config.node_id = generate_unique_node_id();
            
            // Try to save the config file (don't fail if we can't)
            if let Ok(content) = toml::to_string_pretty(&default_config) {
                let _ = std::fs::write(path, content);
            }
            
            Ok(default_config)
        }
    }
    
    fn validate_config(config: &NodeConfig) -> Result<()> {
        if config.max_queue_size == 0 {
            return Err(anyhow!("max_queue_size must be greater than 0"));
        }
        
        if config.node_id.is_empty() {
            return Err(anyhow!("node_id cannot be empty"));
        }
        
        // Validate bootstrap peer format
        for peer in &config.bootstrap_peers {
            if !peer.starts_with("/ip4/") && !peer.starts_with("/ip6/") && !peer.starts_with("/dns") {
                return Err(anyhow!("Invalid bootstrap peer format: {}", peer));
            }
        }
        
        // Don't validate model path here - it might be a model name, not a file path
        
        Ok(())
    }
    
    fn generate_identity() -> Result<NodeIdentity> {
        let keypair = Keypair::generate_ed25519();
        let peer_id = PeerId::from(keypair.public());
        let manifest_id = format!("manifest-{}", hex::encode(&peer_id.to_bytes()[..8]));
        
        Ok(NodeIdentity {
            peer_id,
            manifest_id,
        })
    }
    
    fn save_config(config: &NodeConfig, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Multi-instance configuration builder
pub struct MultiInstanceConfig {
    /// Base configuration template
    base_config: NodeConfig,
    
    /// Number of instances
    instance_count: usize,
    
    /// Generated configurations
    configs: Vec<NodeConfig>,
}

impl MultiInstanceConfig {
    /// Create a new multi-instance configuration
    pub fn new(instance_count: usize) -> Self {
        Self {
            base_config: NodeConfig::default(),
            instance_count,
            configs: Vec::new(),
        }
    }
    
    /// Set the base configuration template
    pub fn with_base_config(mut self, config: NodeConfig) -> Self {
        self.base_config = config;
        self
    }
    
    /// Build configurations for all instances
    pub fn build(mut self) -> Result<Vec<NodeConfig>> {
        let mut configs = Vec::new();
        let mut used_ports = Vec::new();
        
        for i in 0..self.instance_count {
            let mut config = self.base_config.clone();
            
            // Assign unique node ID
            config.node_id = format!("{}-{}", self.base_config.node_id, i);
            
            // Find available port
            let port = find_available_port()?;
            config.listen_port = port;
            used_ports.push(port);
            
            // Set instance-specific data directory
            config.data_dir = Some(format!("./data/instance-{}", i));
            
            // Add previous instances as bootstrap peers
            for &prev_port in &used_ports[..used_ports.len().saturating_sub(1)] {
                config.add_bootstrap_peer(&NodeConfig::peer_multiaddr(prev_port));
            }
            
            configs.push(config);
        }
        
        self.configs = configs.clone();
        Ok(configs)
    }
    
    /// Get the generated configurations
    pub fn get_configs(&self) -> &[NodeConfig] {
        &self.configs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = NodeConfig::default();
        assert_eq!(config.listen_port, 0);
        assert!(config.bootstrap_peers.is_empty());
        assert_eq!(config.max_queue_size, 100);
        assert!(config.enable_mdns);
    }
    
    #[test]
    fn test_new_instance() {
        let config = NodeConfig::new_instance();
        assert!(config.listen_port > 0 || config.listen_port == 0);
        assert!(config.node_id.starts_with("node-"));
    }
    
    #[test]
    fn test_for_instance() {
        let config = NodeConfig::for_instance(5);
        assert_eq!(config.node_id, "node-5");
        assert_eq!(config.listen_port, BASE_PORT + 5);
        assert!(config.data_dir.is_some());
    }
    
    #[test]
    fn test_find_available_port() {
        let port1 = find_available_port().unwrap();
        assert!(port1 > 0);
        
        // Bind to port1 to make it unavailable
        let _listener = TcpListener::bind(format!("127.0.0.1:{}", port1));
        
        let port2 = find_available_port().unwrap();
        assert!(port2 > 0);
        // Ports might or might not be different depending on timing
    }
    
    #[test]
    fn test_is_port_available() {
        // Find an available port
        let port = find_available_port().unwrap();
        assert!(is_port_available(port));
        
        // Bind to it
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).unwrap();
        
        // Now it should not be available
        assert!(!is_port_available(port));
        
        drop(listener);
    }
    
    #[test]
    fn test_generate_unique_node_id() {
        let id1 = generate_unique_node_id();
        let id2 = generate_unique_node_id();
        
        assert!(id1.starts_with("node-"));
        assert!(id2.starts_with("node-"));
        assert_ne!(id1, id2);
    }
    
    #[test]
    fn test_add_bootstrap_peer() {
        let mut config = NodeConfig::default();
        config.add_bootstrap_peer("/ip4/127.0.0.1/tcp/9000");
        config.add_bootstrap_peer("/ip4/127.0.0.1/tcp/9001");
        config.add_bootstrap_peer("/ip4/127.0.0.1/tcp/9000"); // Duplicate
        
        assert_eq!(config.bootstrap_peers.len(), 2);
    }
    
    #[test]
    fn test_multi_instance_config() {
        let configs = MultiInstanceConfig::new(3)
            .with_base_config(NodeConfig {
                node_id: "test".to_string(),
                model_path: "tinyllama-1.1b".to_string(),
                ..Default::default()
            })
            .build()
            .unwrap();
        
        assert_eq!(configs.len(), 3);
        
        // Check unique ports
        let ports: Vec<_> = configs.iter().map(|c| c.listen_port).collect();
        let unique_ports: std::collections::HashSet<_> = ports.iter().collect();
        assert_eq!(ports.len(), unique_ports.len());
        
        // Check unique node IDs
        let ids: Vec<_> = configs.iter().map(|c| &c.node_id).collect();
        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len());
        
        // Check bootstrap peers (second and third should have bootstrap peers)
        assert!(configs[0].bootstrap_peers.is_empty());
        assert!(!configs[1].bootstrap_peers.is_empty());
        assert!(!configs[2].bootstrap_peers.is_empty());
    }
    
    #[test]
    fn test_validate_config() {
        // Valid config
        let config = NodeConfig {
            node_id: "test-node".to_string(),
            listen_port: 9000,
            bootstrap_peers: vec!["/ip4/127.0.0.1/tcp/9001".to_string()],
            model_path: "tinyllama".to_string(),
            max_queue_size: 10,
            log_level: "info".to_string(),
            ..Default::default()
        };
        assert!(DefaultConfigManager::validate_config(&config).is_ok());
        
        // Invalid: empty node_id
        let mut bad_config = config.clone();
        bad_config.node_id = "".to_string();
        assert!(DefaultConfigManager::validate_config(&bad_config).is_err());
        
        // Invalid: zero queue size
        let mut bad_config = config.clone();
        bad_config.max_queue_size = 0;
        assert!(DefaultConfigManager::validate_config(&bad_config).is_err());
        
        // Invalid: bad bootstrap peer format
        let mut bad_config = config.clone();
        bad_config.bootstrap_peers = vec!["invalid-peer".to_string()];
        assert!(DefaultConfigManager::validate_config(&bad_config).is_err());
    }
    
    #[test]
    fn test_get_listen_multiaddr() {
        let mut config = NodeConfig::default();
        config.listen_port = 9000;
        
        let addr = config.get_listen_multiaddr();
        assert_eq!(addr, "/ip4/127.0.0.1/tcp/9000");
    }
}
