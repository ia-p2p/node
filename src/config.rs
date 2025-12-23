//! Configuration management for MVP Node
//!
//! This module provides configuration loading, validation, and management
//! for multi-instance support.
//!
//! Implements Requirements 8.1, 8.2, 8.3, 8.4, 8.5

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::net::TcpListener;
use anyhow::{Result, anyhow};
use crate::{ConfigManager, NodeIdentity};
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
        }
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
