use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::Result;
use crate::{ConfigManager, NodeIdentity};
use libp2p::{identity::Keypair, PeerId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub node_id: String,
    pub listen_port: u16,
    pub bootstrap_peers: Vec<String>,
    pub model_path: String,
    pub max_queue_size: usize,
    pub log_level: String,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: "auto".to_string(),
            listen_port: 0, // Auto-assign
            bootstrap_peers: Vec::new(),
            model_path: "./models/micromodel.gguf".to_string(),
            max_queue_size: 100,
            log_level: "info".to_string(),
        }
    }
}

/// Default configuration manager implementation
pub struct DefaultConfigManager;

impl ConfigManager for DefaultConfigManager {
    fn load_config(path: &str) -> Result<NodeConfig> {
        if Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            let config: NodeConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config file
            let default_config = NodeConfig::default();
            let content = toml::to_string_pretty(&default_config)?;
            std::fs::write(path, content)?;
            Ok(default_config)
        }
    }
    
    fn validate_config(config: &NodeConfig) -> Result<()> {
        if config.max_queue_size == 0 {
            anyhow::bail!("max_queue_size must be greater than 0");
        }
        
        if !Path::new(&config.model_path).exists() {
            anyhow::bail!("Model path does not exist: {}", config.model_path);
        }
        
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