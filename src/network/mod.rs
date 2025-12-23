//! P2P network layer implementation using libp2p
//! 
//! This module implements the NetworkLayer trait with gossipsub, mDNS, and identify protocols.

use crate::protocol::P2PMessage;
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use libp2p::{
    gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode},
    identify,
    identity::Keypair,
    mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm,
};
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Network configuration for P2P layer
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub listen_addr: Multiaddr,
    pub bootstrap_peers: Vec<Multiaddr>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            bootstrap_peers: Vec::new(),
        }
    }
}

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

/// Topics for gossipsub communication
pub mod topics {
    pub const ANNOUNCE: &str = "p2p-ai/announce/1.0";
    pub const JOB_OFFER: &str = "p2p-ai/job-offer/1.0";
    pub const JOB_CLAIM: &str = "p2p-ai/job-claim/1.0";
    pub const RECEIPT: &str = "p2p-ai/receipt/1.0";
}

/// Events emitted by the network layer
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerDiscovered(PeerId),
    PeerExpired(PeerId),
    MessageReceived {
        topic: String,
        message: P2PMessage,
        source: Option<PeerId>,
    },
    Published {
        topic: String,
    },
    ConnectionEstablished(PeerId),
    ConnectionClosed(PeerId),
}

/// Combined network behaviour for libp2p
#[derive(NetworkBehaviour)]
pub struct P2PBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
}

/// Bootstrap node mode for network initialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapMode {
    /// This node is a bootstrap node (no bootstrap peers provided)
    Bootstrap,
    /// This node connects to existing bootstrap peers
    Client,
}

impl std::fmt::Display for BootstrapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootstrapMode::Bootstrap => write!(f, "Bootstrap"),
            BootstrapMode::Client => write!(f, "Client"),
        }
    }
}

/// Configuration for peer acceptance and topology management
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    /// Maximum number of peers to accept
    pub max_peers: usize,
    /// Enable automatic peer acceptance
    pub auto_accept_peers: bool,
    /// Preferred peers to maintain connection with
    pub preferred_peers: Vec<PeerId>,
    /// Enable peer rotation for load balancing
    pub enable_peer_rotation: bool,
}

impl Default for TopologyConfig {
    fn default() -> Self {
        Self {
            max_peers: 50,
            auto_accept_peers: true,
            preferred_peers: Vec::new(),
            enable_peer_rotation: true,
        }
    }
}

/// P2P Network implementation
pub struct P2PNetworkLayer {
    swarm: Swarm<P2PBehaviour>,
    event_tx: mpsc::Sender<NetworkEvent>,
    event_rx: Option<mpsc::Receiver<NetworkEvent>>,
    peers: HashMap<PeerId, PeerInfo>,
    subscribed_topics: Vec<String>,
    is_running: bool,
    /// Mode of operation (bootstrap or client)
    bootstrap_mode: BootstrapMode,
    /// Topology configuration
    topology_config: TopologyConfig,
    /// List of known bootstrap peers for network recovery
    known_bootstrap_peers: Vec<Multiaddr>,
    /// Timestamp when node started
    started_at: Option<std::time::Instant>,
}

/// Information about a connected peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    pub addresses: Vec<Multiaddr>,
    pub protocol_version: Option<String>,
    pub connected_at: std::time::Instant,
}

impl P2PNetworkLayer {
    /// Create a new P2P network layer
    pub async fn new(keypair: Keypair) -> Result<Self> {
        let peer_id = PeerId::from(keypair.public());
        info!("Initializing P2P network layer with peer ID: {}", peer_id);

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel(1000);

        // Gossipsub configuration
        let message_id_fn = |message: &gossipsub::Message| {
            let mut hasher = DefaultHasher::new();
            message.data.hash(&mut hasher);
            if let Some(source) = &message.source {
                source.hash(&mut hasher);
            }
            gossipsub::MessageId::from(hasher.finish().to_string())
        };

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Permissive) // Changed from Strict to allow single-node testing
            .message_id_fn(message_id_fn)
            .flood_publish(true) // Allow publishing without peers for testing
            .build()
            .map_err(|e| anyhow::anyhow!("Gossipsub config error: {}", e))?;

        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .map_err(|e| anyhow::anyhow!("Gossipsub error: {}", e))?;

        // mDNS for local discovery
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;

        // Identify protocol
        let identify = identify::Behaviour::new(identify::Config::new(
            "/mvp-node/1.0.0".to_string(),
            keypair.public(),
        ));

        let behaviour = P2PBehaviour {
            gossipsub,
            mdns,
            identify,
        };

        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|_| behaviour)?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        Ok(Self {
            swarm,
            event_tx,
            event_rx: Some(event_rx),
            peers: HashMap::new(),
            subscribed_topics: Vec::new(),
            is_running: false,
            bootstrap_mode: BootstrapMode::Client, // Default, updated on start
            topology_config: TopologyConfig::default(),
            known_bootstrap_peers: Vec::new(),
            started_at: None,
        })
    }

    /// Create a new P2P network layer configured as a bootstrap node
    pub async fn new_bootstrap(keypair: Keypair, topology_config: TopologyConfig) -> Result<Self> {
        let mut layer = Self::new(keypair).await?;
        layer.bootstrap_mode = BootstrapMode::Bootstrap;
        layer.topology_config = topology_config;
        Ok(layer)
    }

    /// Check if this node is operating as a bootstrap node
    pub fn is_bootstrap_node(&self) -> bool {
        self.bootstrap_mode == BootstrapMode::Bootstrap
    }

    /// Get the current bootstrap mode
    pub fn get_bootstrap_mode(&self) -> BootstrapMode {
        self.bootstrap_mode
    }

    /// Get the topology configuration
    pub fn get_topology_config(&self) -> &TopologyConfig {
        &self.topology_config
    }

    /// Set the topology configuration
    pub fn set_topology_config(&mut self, config: TopologyConfig) {
        self.topology_config = config;
    }

    /// Get known bootstrap peers
    pub fn get_known_bootstrap_peers(&self) -> &[Multiaddr] {
        &self.known_bootstrap_peers
    }

    /// Add a known bootstrap peer
    pub fn add_known_bootstrap_peer(&mut self, addr: Multiaddr) {
        if !self.known_bootstrap_peers.contains(&addr) {
            self.known_bootstrap_peers.push(addr);
        }
    }

    /// Check if we can accept more peers based on topology configuration
    pub fn can_accept_peer(&self) -> bool {
        if !self.topology_config.auto_accept_peers {
            return false;
        }
        self.peers.len() < self.topology_config.max_peers
    }

    /// Get node uptime if started
    pub fn get_uptime(&self) -> Option<Duration> {
        self.started_at.map(|t| t.elapsed())
    }

    /// Subscribe to all standard topics
    pub fn subscribe_all_topics(&mut self) -> Result<()> {
        let topics = [topics::ANNOUNCE, topics::JOB_OFFER, topics::JOB_CLAIM, topics::RECEIPT];
        for topic_str in topics {
            self.subscribe_topic_internal(topic_str)?;
        }
        Ok(())
    }

    /// Internal topic subscription
    fn subscribe_topic_internal(&mut self, topic_str: &str) -> Result<()> {
        let topic = IdentTopic::new(topic_str);
        self.swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
        if !self.subscribed_topics.contains(&topic_str.to_string()) {
            self.subscribed_topics.push(topic_str.to_string());
        }
        info!("Subscribed to topic: {}", topic_str);
        Ok(())
    }

    /// Publish a message to a topic
    pub fn publish_message_internal(&mut self, topic_str: &str, message: &P2PMessage) -> Result<()> {
        let topic = IdentTopic::new(topic_str);
        let data = message.to_bytes()?;
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)?;
        debug!("Published message to topic: {}", topic_str);
        Ok(())
    }

    /// Get peer information
    pub fn get_peer_info(&self, peer_id: &PeerId) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    /// Get all connected peers
    pub fn get_connected_peers(&self) -> Vec<PeerId> {
        self.peers.keys().cloned().collect()
    }

    /// Take the event receiver (can only be called once)
    pub fn take_event_receiver(&mut self) -> Option<mpsc::Receiver<NetworkEvent>> {
        self.event_rx.take()
    }

    /// Run the network event loop
    pub async fn run_event_loop(&mut self) {
        self.is_running = true;
        info!("Starting P2P network event loop");

        while self.is_running {
            let event = self.swarm.select_next_some().await;
            self.handle_swarm_event(event).await;
        }

        info!("P2P network event loop stopped");
    }

    /// Stop the event loop
    pub fn stop(&mut self) {
        self.is_running = false;
        info!("Stopping P2P network layer");
    }

    /// Handle swarm events
    async fn handle_swarm_event(&mut self, event: SwarmEvent<P2PBehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(P2PBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source,
                message,
                ..
            })) => {
                let topic = message.topic.to_string();
                match P2PMessage::from_bytes(&message.data) {
                    Ok(msg) => {
                        let event = NetworkEvent::MessageReceived {
                            topic,
                            message: msg,
                            source: Some(propagation_source),
                        };
                        if let Err(e) = self.event_tx.send(event).await {
                            warn!("Failed to send network event: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse P2P message: {}", e);
                    }
                }
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
                for (peer_id, addr) in peers {
                    // Check if we can accept more peers based on topology configuration
                    if !self.can_accept_peer() {
                        debug!("Ignoring discovered peer {} - max peers reached ({}/{})", 
                               peer_id, self.peers.len(), self.topology_config.max_peers);
                        continue;
                    }

                    info!("mDNS discovered peer: {} at {}", peer_id, addr);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .add_explicit_peer(&peer_id);
                    
                    // Add to peer list
                    self.peers.insert(peer_id, PeerInfo {
                        peer_id,
                        addresses: vec![addr],
                        protocol_version: None,
                        connected_at: std::time::Instant::now(),
                    });

                    if self.is_bootstrap_node() {
                        info!("Bootstrap node accepted peer {} ({}/{})", 
                              peer_id, self.peers.len(), self.topology_config.max_peers);
                    }

                    if let Err(e) = self.event_tx.send(NetworkEvent::PeerDiscovered(peer_id)).await {
                        warn!("Failed to send peer discovered event: {}", e);
                    }
                }
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
                for (peer_id, _) in peers {
                    info!("mDNS peer expired: {}", peer_id);
                    self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .remove_explicit_peer(&peer_id);
                    
                    // Remove from peer list
                    self.peers.remove(&peer_id);

                    if let Err(e) = self.event_tx.send(NetworkEvent::PeerExpired(peer_id)).await {
                        warn!("Failed to send peer expired event: {}", e);
                    }
                }
            }
            SwarmEvent::Behaviour(P2PBehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
            })) => {
                debug!(
                    "Identified peer {}: {} with {} addresses",
                    peer_id,
                    info.protocol_version,
                    info.listen_addrs.len()
                );

                // Update peer info
                if let Some(peer_info) = self.peers.get_mut(&peer_id) {
                    peer_info.protocol_version = Some(info.protocol_version);
                    peer_info.addresses = info.listen_addrs;
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Now listening on {}", address);
            }
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                // Check peer limits for incoming connections
                if endpoint.is_listener() && !self.can_accept_peer() && !self.is_preferred_peer(&peer_id) {
                    info!("Rejecting connection from {} - max peers reached ({}/{})", 
                          peer_id, self.peers.len(), self.topology_config.max_peers);
                    let _ = self.swarm.disconnect_peer_id(peer_id);
                    return;
                }

                info!("Connection established with peer: {} ({})", 
                      peer_id, if endpoint.is_listener() { "incoming" } else { "outgoing" });
                
                if self.is_bootstrap_node() && endpoint.is_listener() {
                    info!("Bootstrap node: accepted incoming connection from {}", peer_id);
                }

                if let Err(e) = self.event_tx.send(NetworkEvent::ConnectionEstablished(peer_id)).await {
                    warn!("Failed to send connection established event: {}", e);
                }
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                info!("Connection closed with peer: {}", peer_id);
                // Remove from peer list
                self.peers.remove(&peer_id);
                if let Err(e) = self.event_tx.send(NetworkEvent::ConnectionClosed(peer_id)).await {
                    warn!("Failed to send connection closed event: {}", e);
                }
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    warn!("Outgoing connection error to {}: {}", peer_id, error);
                } else {
                    warn!("Outgoing connection error: {}", error);
                }
            }
            SwarmEvent::IncomingConnectionError { error, .. } => {
                warn!("Incoming connection error: {}", error);
            }
            _ => {}
        }
    }
}

#[async_trait]
impl NetworkLayer for P2PNetworkLayer {
    async fn start(&mut self, config: NetworkConfig) -> Result<()> {
        info!("Starting P2P network layer with config: {:?}", config);

        // Detect bootstrap mode based on whether bootstrap peers are provided
        if config.bootstrap_peers.is_empty() {
            self.bootstrap_mode = BootstrapMode::Bootstrap;
            info!("No bootstrap peers provided - operating as BOOTSTRAP NODE");
        } else {
            self.bootstrap_mode = BootstrapMode::Client;
            info!("Bootstrap peers provided - operating as CLIENT NODE");
            // Store known bootstrap peers for network recovery
            self.known_bootstrap_peers = config.bootstrap_peers.clone();
        }

        // Record start time
        self.started_at = Some(std::time::Instant::now());

        // Listen on configured address
        self.swarm.listen_on(config.listen_addr.clone())?;

        // Poll the swarm until we get the first listener address
        // This ensures the network is actually listening before we return
        let mut listener_established = false;
        let timeout = tokio::time::Duration::from_secs(5);
        let start_time = tokio::time::Instant::now();

        while !listener_established && start_time.elapsed() < timeout {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("Now listening on {}", address);
                            listener_established = true;
                        }
                        _ => {
                            // Handle other events during startup
                            self.handle_swarm_event(event).await;
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {
                    // Continue polling
                }
            }
        }

        if !listener_established {
            anyhow::bail!("Failed to establish listener within timeout");
        }

        // Bootstrap node specific initialization
        if self.is_bootstrap_node() {
            info!("Bootstrap node initialized - ready to accept incoming connections");
            info!("Max peers configured: {}", self.topology_config.max_peers);
            info!("Auto-accept peers: {}", self.topology_config.auto_accept_peers);
        }

        // Connect to bootstrap peers (only for client nodes)
        for addr in &config.bootstrap_peers {
            info!("Dialing bootstrap peer: {}", addr);
            if let Err(e) = self.swarm.dial(addr.clone()) {
                warn!("Failed to dial bootstrap peer {}: {}", addr, e);
            }
        }

        // Subscribe to all standard topics
        self.subscribe_all_topics()?;

        info!("P2P network layer started successfully in {} mode", self.bootstrap_mode);
        Ok(())
    }

    async fn publish_message(&mut self, topic: &str, message: &[u8]) -> Result<()> {
        // Publish raw bytes directly
        let topic = IdentTopic::new(topic);
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, message.to_vec())?;
        debug!("Published {} bytes to topic", message.len());
        Ok(())
    }

    async fn subscribe_topic(&mut self, topic: &str) -> Result<()> {
        self.subscribe_topic_internal(topic)
    }

    fn get_peer_count(&self) -> usize {
        self.peers.len()
    }

    fn get_local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    fn get_listeners(&self) -> Vec<Multiaddr> {
        self.swarm.listeners().cloned().collect()
    }

    async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down P2P network layer");
        self.stop();
        
        // Close all connections gracefully
        for peer_id in self.get_connected_peers() {
            let _ = self.swarm.disconnect_peer_id(peer_id);
        }

        // Clear peer list
        self.peers.clear();
        self.subscribed_topics.clear();
        self.started_at = None;

        info!("P2P network layer shutdown complete");
        Ok(())
    }
}

/// Bootstrap node status information
#[derive(Debug, Clone)]
pub struct BootstrapNodeStatus {
    /// Whether this node is operating as a bootstrap node
    pub is_bootstrap: bool,
    /// Bootstrap mode
    pub mode: BootstrapMode,
    /// Current peer count
    pub peer_count: usize,
    /// Maximum allowed peers
    pub max_peers: usize,
    /// Whether auto-accept is enabled
    pub auto_accept_peers: bool,
    /// Node uptime in seconds
    pub uptime_secs: u64,
    /// Listening addresses
    pub listen_addresses: Vec<String>,
    /// Number of known bootstrap peers (for client nodes)
    pub known_bootstrap_peers_count: usize,
}

impl P2PNetworkLayer {
    /// Get bootstrap node status for monitoring
    pub fn get_bootstrap_status(&self) -> BootstrapNodeStatus {
        BootstrapNodeStatus {
            is_bootstrap: self.is_bootstrap_node(),
            mode: self.bootstrap_mode,
            peer_count: self.peers.len(),
            max_peers: self.topology_config.max_peers,
            auto_accept_peers: self.topology_config.auto_accept_peers,
            uptime_secs: self.get_uptime().map(|d| d.as_secs()).unwrap_or(0),
            listen_addresses: self.get_listeners().iter().map(|a| a.to_string()).collect(),
            known_bootstrap_peers_count: self.known_bootstrap_peers.len(),
        }
    }

    /// Dial a specific peer address
    pub fn dial_peer(&mut self, addr: &Multiaddr) -> Result<()> {
        self.swarm.dial(addr.clone())?;
        info!("Dialing peer at {}", addr);
        Ok(())
    }

    /// Disconnect from a specific peer
    pub fn disconnect_peer(&mut self, peer_id: &PeerId) -> Result<()> {
        let _ = self.swarm.disconnect_peer_id(*peer_id);
        self.peers.remove(peer_id);
        info!("Disconnected from peer {}", peer_id);
        Ok(())
    }

    /// Add a peer to the preferred peers list for priority connections
    pub fn add_preferred_peer(&mut self, peer_id: PeerId) {
        if !self.topology_config.preferred_peers.contains(&peer_id) {
            self.topology_config.preferred_peers.push(peer_id);
            info!("Added {} to preferred peers", peer_id);
        }
    }

    /// Remove a peer from the preferred peers list
    pub fn remove_preferred_peer(&mut self, peer_id: &PeerId) {
        self.topology_config.preferred_peers.retain(|p| p != peer_id);
        info!("Removed {} from preferred peers", peer_id);
    }

    /// Check if a peer is in the preferred list
    pub fn is_preferred_peer(&self, peer_id: &PeerId) -> bool {
        self.topology_config.preferred_peers.contains(peer_id)
    }

    /// Get network statistics for the bootstrap node
    pub fn get_network_stats(&self) -> NetworkStats {
        NetworkStats {
            connected_peers: self.peers.len(),
            subscribed_topics: self.subscribed_topics.len(),
            is_bootstrap: self.is_bootstrap_node(),
            uptime: self.get_uptime(),
        }
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub subscribed_topics: usize,
    pub is_bootstrap: bool,
    pub uptime: Option<Duration>,
}

/// Load or generate Ed25519 keypair from file
pub fn load_or_generate_keypair(path: &std::path::Path) -> Result<Keypair> {
    if path.exists() {
        let data = std::fs::read(path)?;
        let keys: serde_json::Value = serde_json::from_slice(&data)?;
        if let Some(secret) = keys.get("secret_key").and_then(|v| v.as_str()) {
            // Parse hex-encoded secret key
            let bytes = hex::decode(secret).or_else(|_| {
                // Try base64 or raw
                Ok::<_, anyhow::Error>(secret.as_bytes().to_vec())
            })?;
            if bytes.len() >= 32 {
                let keypair = Keypair::ed25519_from_bytes(bytes[..32].to_vec())?;
                return Ok(keypair);
            }
        }
    }

    // Generate new keypair
    let keypair = Keypair::generate_ed25519();
    Ok(keypair)
}

/// Convert libp2p Keypair to hex-encoded strings for storage
pub fn keypair_to_hex(keypair: &Keypair) -> Result<(String, String)> {
    match keypair.clone().try_into_ed25519() {
        Ok(ed_keypair) => {
            let secret = hex::encode(ed_keypair.secret().as_ref());
            let public = hex::encode(ed_keypair.public().to_bytes());
            Ok((public, secret))
        }
        Err(_) => anyhow::bail!("Only Ed25519 keys are supported"),
    }
}

/// Save keypair to file
pub fn save_keypair(keypair: &Keypair, path: &std::path::Path) -> Result<()> {
    let (public, secret) = keypair_to_hex(keypair)?;
    let keys = serde_json::json!({
        "public_key": public,
        "secret_key": secret
    });
    
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    std::fs::write(path, serde_json::to_string_pretty(&keys)?)?;
    info!("Keypair saved to: {}", path.display());
    Ok(())
}