use clap::Parser;
use tracing::info;

// Import core interfaces and types
use mvp_node::ConfigManager;
use mvp_node::config::DefaultConfigManager;

mod node;

#[derive(Parser)]
#[command(name = "mvp-node")]
#[command(about = "MVP Node for decentralized AI network")]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    config: String,
    
    /// Override listen port
    #[arg(short, long)]
    port: Option<u16>,
    
    /// Bootstrap peer addresses
    #[arg(short, long)]
    bootstrap: Vec<String>,
    
    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(&cli.log_level)
        .json()
        .init();
    
    info!("Starting MVP Node...");
    
    // Load configuration
    let mut config = DefaultConfigManager::load_config(&cli.config)?;
    
    // Apply CLI overrides
    if let Some(port) = cli.port {
        config.listen_port = port;
    }
    if !cli.bootstrap.is_empty() {
        config.bootstrap_peers = cli.bootstrap;
    }
    
    // Validate configuration
    DefaultConfigManager::validate_config(&config)?;
    
    info!("Configuration loaded: {:?}", config);
    
    // TODO: Initialize and start node
    // This will be implemented in task 1
    
    Ok(())
}