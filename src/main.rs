//! MVP Node - Main entry point
//!
//! This binary provides the command-line interface for running
//! MVP nodes in the decentralized AI network.
//!
//! ## Usage
//!
//! ```bash
//! # Start with default configuration
//! mvp-node
//!
//! # Start with custom config file
//! mvp-node --config my-config.toml
//!
//! # Start with CLI overrides
//! mvp-node --port 9001 --bootstrap /ip4/127.0.0.1/tcp/9000
//!
//! # Start with environment variables
//! MVP_NODE_PORT=9001 MVP_NODE_LOG_LEVEL=debug mvp-node
//!
//! # Show help
//! mvp-node --help
//! ```

use clap::{Parser, Subcommand};
use std::env;
use tracing::{info, warn, error};

// Import core interfaces and types
use mvp_node::ConfigManager;
use mvp_node::config::{DefaultConfigManager, NodeConfig};

mod node;

/// Environment variable prefix for configuration
const ENV_PREFIX: &str = "MVP_NODE";

/// MVP Node - Decentralized AI Network Node
///
/// Run inference workloads on a peer-to-peer network with support
/// for job queuing, model management, and distributed processing.
#[derive(Parser)]
#[command(name = "mvp-node")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "RAIS Network")]
#[command(about = "MVP Node for decentralized AI network")]
#[command(long_about = r#"
MVP Node for the RAIS decentralized AI network.

This node can:
- Load and serve AI models for inference
- Connect to other nodes via P2P networking
- Process job requests from the network
- Maintain a job queue with configurable limits

Configuration can be provided via:
1. Configuration file (TOML format)
2. Command-line arguments (override config file)
3. Environment variables (MVP_NODE_* prefix)

Environment Variables:
  MVP_NODE_PORT          Override listen port
  MVP_NODE_LOG_LEVEL     Set log level (trace, debug, info, warn, error)
  MVP_NODE_MODEL_PATH    Path to AI model
  MVP_NODE_QUEUE_SIZE    Maximum job queue size
  MVP_NODE_NODE_ID       Custom node identifier
"#)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml", env = "MVP_NODE_CONFIG")]
    config: String,
    
    /// Override listen port (0 for auto-assignment)
    #[arg(short, long, env = "MVP_NODE_PORT")]
    port: Option<u16>,
    
    /// Bootstrap peer addresses (multiaddr format)
    /// Example: /ip4/127.0.0.1/tcp/9000
    #[arg(short, long, value_name = "MULTIADDR")]
    bootstrap: Vec<String>,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", env = "MVP_NODE_LOG_LEVEL")]
    log_level: String,
    
    /// Path to AI model file or model name
    #[arg(short, long, env = "MVP_NODE_MODEL_PATH")]
    model: Option<String>,
    
    /// Maximum job queue size
    #[arg(short = 'q', long, env = "MVP_NODE_QUEUE_SIZE")]
    queue_size: Option<usize>,
    
    /// Custom node identifier (auto-generated if not provided)
    #[arg(long, env = "MVP_NODE_NODE_ID")]
    node_id: Option<String>,
    
    /// Enable mDNS for local peer discovery
    #[arg(long, default_value = "true")]
    mdns: bool,
    
    /// Maximum number of peer connections
    #[arg(long, env = "MVP_NODE_MAX_PEERS")]
    max_peers: Option<usize>,
    
    /// Data directory for node state
    #[arg(long, env = "MVP_NODE_DATA_DIR")]
    data_dir: Option<String>,
    
    /// Run in foreground (don't daemonize)
    #[arg(long, default_value = "true")]
    foreground: bool,
    
    /// Subcommand to execute
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the node (default)
    Start,
    
    /// Generate a default configuration file
    Init {
        /// Output file path
        #[arg(short, long, default_value = "config.toml")]
        output: String,
    },
    
    /// Validate a configuration file
    Validate {
        /// Configuration file to validate
        #[arg(short, long, default_value = "config.toml")]
        config: String,
    },
    
    /// Show node information
    Info,
}

/// Load configuration with environment variable overrides
fn load_config_with_env(cli: &Cli) -> anyhow::Result<NodeConfig> {
    // Load base configuration from file
    let mut config = DefaultConfigManager::load_config(&cli.config)?;
    
    // Apply CLI argument overrides (highest priority)
    if let Some(port) = cli.port {
        config.listen_port = port;
    }
    
    if !cli.bootstrap.is_empty() {
        config.bootstrap_peers = cli.bootstrap.clone();
    }
    
    if let Some(ref model) = cli.model {
        config.model_path = model.clone();
    }
    
    if let Some(queue_size) = cli.queue_size {
        config.max_queue_size = queue_size;
    }
    
    if let Some(ref node_id) = cli.node_id {
        config.node_id = node_id.clone();
    }
    
    if let Some(max_peers) = cli.max_peers {
        config.max_peers = max_peers;
    }
    
    if let Some(ref data_dir) = cli.data_dir {
        config.data_dir = Some(data_dir.clone());
    }
    
    config.enable_mdns = cli.mdns;
    config.log_level = cli.log_level.clone();
    
    Ok(config)
}

/// Initialize logging with the specified level
fn init_logging(log_level: &str) {
    use tracing_subscriber::EnvFilter;
    
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

/// Generate a default configuration file
fn generate_config(output: &str) -> anyhow::Result<()> {
    let config = NodeConfig::new_instance();
    DefaultConfigManager::save_config(&config, output)?;
    println!("Generated configuration file: {}", output);
    println!("Node ID: {}", config.node_id);
    println!("Listen port: {}", config.listen_port);
    Ok(())
}

/// Validate a configuration file
fn validate_config(config_path: &str) -> anyhow::Result<()> {
    let config = DefaultConfigManager::load_config(config_path)?;
    DefaultConfigManager::validate_config(&config)?;
    println!("Configuration is valid!");
    println!("  Node ID: {}", config.node_id);
    println!("  Listen port: {}", config.listen_port);
    println!("  Model path: {}", config.model_path);
    println!("  Queue size: {}", config.max_queue_size);
    println!("  Bootstrap peers: {}", config.bootstrap_peers.len());
    Ok(())
}

/// Show node information
fn show_info() {
    println!("MVP Node v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Build Information:");
    println!("  Package: {}", env!("CARGO_PKG_NAME"));
    println!("  Version: {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Environment Variables:");
    println!("  MVP_NODE_CONFIG     - Configuration file path");
    println!("  MVP_NODE_PORT       - Listen port");
    println!("  MVP_NODE_LOG_LEVEL  - Log level");
    println!("  MVP_NODE_MODEL_PATH - Model path");
    println!("  MVP_NODE_QUEUE_SIZE - Queue size");
    println!("  MVP_NODE_NODE_ID    - Node identifier");
    println!("  MVP_NODE_DATA_DIR   - Data directory");
    println!("  MVP_NODE_MAX_PEERS  - Maximum peers");
}

/// Run the node with the given configuration
async fn run_node(config: NodeConfig) -> anyhow::Result<()> {
    info!("Starting MVP Node...");
    info!("Node ID: {}", config.node_id);
    info!("Listen port: {}", config.listen_port);
    info!("Model: {}", config.model_path);
    info!("Queue size: {}", config.max_queue_size);
    
    if !config.bootstrap_peers.is_empty() {
        info!("Bootstrap peers: {:?}", config.bootstrap_peers);
    } else {
        info!("No bootstrap peers - running as bootstrap node");
    }
    
    // Create and start the node
    let mut node = node::MvpNode::new(config).await?;
    
    info!("Node peer ID: {}", node.get_peer_id());
    
    // Start the node
    node.start().await?;
    
    info!("MVP Node is running. Press Ctrl+C to stop.");
    
    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }
    
    // Graceful shutdown
    info!("Initiating graceful shutdown...");
    node.shutdown().await?;
    
    info!("MVP Node stopped.");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Handle subcommands
    match &cli.command {
        Some(Commands::Init { output }) => {
            return generate_config(output);
        }
        Some(Commands::Validate { config }) => {
            return validate_config(config);
        }
        Some(Commands::Info) => {
            show_info();
            return Ok(());
        }
        Some(Commands::Start) | None => {
            // Continue to start the node
        }
    }
    
    // Initialize logging
    init_logging(&cli.log_level);
    
    info!("Loading configuration...");
    
    // Load configuration with CLI and environment overrides
    let config = match load_config_with_env(&cli) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            eprintln!("Error: {}", e);
            eprintln!("Use 'mvp-node init' to generate a default configuration file.");
            return Err(e);
        }
    };
    
    // Validate configuration
    if let Err(e) = DefaultConfigManager::validate_config(&config) {
        error!("Configuration validation failed: {}", e);
        eprintln!("Configuration error: {}", e);
        return Err(e);
    }
    
    info!("Configuration validated successfully");
    
    // Run the node
    run_node(config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cli_parsing() {
        // Test default parsing
        let cli = Cli::try_parse_from(&["mvp-node"]).unwrap();
        assert_eq!(cli.config, "config.toml");
        assert_eq!(cli.log_level, "info");
        assert!(cli.port.is_none());
    }
    
    #[test]
    fn test_cli_with_port() {
        let cli = Cli::try_parse_from(&["mvp-node", "--port", "9001"]).unwrap();
        assert_eq!(cli.port, Some(9001));
    }
    
    #[test]
    fn test_cli_with_bootstrap() {
        let cli = Cli::try_parse_from(&[
            "mvp-node",
            "--bootstrap", "/ip4/127.0.0.1/tcp/9000",
            "--bootstrap", "/ip4/127.0.0.1/tcp/9001"
        ]).unwrap();
        assert_eq!(cli.bootstrap.len(), 2);
    }
    
    #[test]
    fn test_cli_with_model() {
        let cli = Cli::try_parse_from(&["mvp-node", "--model", "tinyllama-1.1b"]).unwrap();
        assert_eq!(cli.model, Some("tinyllama-1.1b".to_string()));
    }
    
    #[test]
    fn test_cli_init_subcommand() {
        let cli = Cli::try_parse_from(&["mvp-node", "init", "--output", "test.toml"]).unwrap();
        match cli.command {
            Some(Commands::Init { output }) => assert_eq!(output, "test.toml"),
            _ => panic!("Expected Init subcommand"),
        }
    }
    
    #[test]
    fn test_cli_validate_subcommand() {
        let cli = Cli::try_parse_from(&["mvp-node", "validate", "--config", "test.toml"]).unwrap();
        match cli.command {
            Some(Commands::Validate { config }) => assert_eq!(config, "test.toml"),
            _ => panic!("Expected Validate subcommand"),
        }
    }
    
    #[test]
    fn test_cli_info_subcommand() {
        let cli = Cli::try_parse_from(&["mvp-node", "info"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Info)));
    }
}
