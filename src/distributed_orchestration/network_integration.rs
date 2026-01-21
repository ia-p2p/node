//! Network Integration for Distributed Orchestration
//!
//! Connects the P2PAdapter to the P2PNetworkLayer event loop.

use super::p2p_adapter::{OrchestrationNetworkMessage, P2PAdapter};
use super::protocols::{ConsensusMessage, GroupMessage, CoordinationMessage, HeartbeatMessage};
use crate::network::{NetworkEvent, P2PNetworkLayer, topics};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn, error};

/// Orchestration Network Bridge
/// 
/// Bridges the P2PAdapter with the P2PNetworkLayer, routing messages
/// between the distributed orchestration system and the P2P network.
pub struct OrchestrationNetworkBridge {
    /// P2P Adapter for orchestration
    adapter: Arc<P2PAdapter>,
    
    /// Sender for outbound messages to publish
    outbound_tx: mpsc::Sender<OrchestrationNetworkMessage>,
    
    /// Receiver for outbound messages
    outbound_rx: mpsc::Receiver<OrchestrationNetworkMessage>,
    
    /// Network layer reference (for publishing)
    network_layer: Arc<RwLock<Option<NetworkLayerHandle>>>,
}

/// Handle to interact with network layer
pub struct NetworkLayerHandle {
    /// Sender for raw publish commands
    publish_tx: mpsc::Sender<(String, Vec<u8>)>,
}

impl NetworkLayerHandle {
    /// Publish raw bytes to a topic
    pub async fn publish(&self, topic: &str, data: Vec<u8>) -> Result<()> {
        self.publish_tx
            .send((topic.to_string(), data))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send publish command: {}", e))
    }
}

impl OrchestrationNetworkBridge {
    /// Create a new bridge
    pub fn new(node_id: String) -> (Self, Arc<P2PAdapter>) {
        let (adapter, outbound_rx) = P2PAdapter::new(node_id);
        let adapter = Arc::new(adapter);
        
        // Create outbound channel for the adapter to use
        let (outbound_tx, _) = mpsc::channel(100);
        
        (
            Self {
                adapter: adapter.clone(),
                outbound_tx,
                outbound_rx,
                network_layer: Arc::new(RwLock::new(None)),
            },
            adapter,
        )
    }
    
    /// Set the network layer handle
    pub async fn set_network_handle(&self, handle: NetworkLayerHandle) {
        let mut nl = self.network_layer.write().await;
        *nl = Some(handle);
    }
    
    /// Get the adapter
    pub fn adapter(&self) -> Arc<P2PAdapter> {
        self.adapter.clone()
    }
    
    /// Process a network event from the P2P layer
    pub async fn process_network_event(&self, event: NetworkEvent) -> Result<()> {
        match event {
            NetworkEvent::OrchestrationMessageReceived { topic, data, source } => {
                debug!("Received orchestration message on topic: {} from {:?}", topic, source);
                
                // Parse the message based on topic
                let msg = OrchestrationNetworkMessage::from_topic_and_bytes(&topic, &data)?;
                
                // Forward to adapter
                let sender = self.adapter.get_inbound_sender();
                sender.send(msg).await.map_err(|e| {
                    anyhow::anyhow!("Failed to forward orchestration message: {}", e)
                })?;
                
                // Register peer if we have a source
                if let Some(peer_id) = source {
                    self.adapter.register_peer(peer_id.to_string()).await;
                }
            }
            NetworkEvent::PeerDiscovered(peer_id) => {
                self.adapter.register_peer(peer_id.to_string()).await;
            }
            NetworkEvent::PeerExpired(peer_id) => {
                self.adapter.unregister_peer(&peer_id.to_string()).await;
            }
            _ => {
                // Other events are not relevant for orchestration
            }
        }
        
        Ok(())
    }
    
    /// Run the outbound message loop
    /// 
    /// This task takes messages from the adapter's outbound queue and publishes
    /// them to the P2P network.
    pub async fn run_outbound_loop(&mut self) {
        info!("Starting orchestration outbound message loop");
        
        while let Some(msg) = self.outbound_rx.recv().await {
            let topic = msg.topic().to_string();
            
            match msg.to_bytes() {
                Ok(data) => {
                    let nl = self.network_layer.read().await;
                    if let Some(ref handle) = *nl {
                        if let Err(e) = handle.publish(&topic, data).await {
                            warn!("Failed to publish orchestration message: {}", e);
                        } else {
                            debug!("Published orchestration message to {}", topic);
                        }
                    } else {
                        warn!("Network layer not connected, dropping message");
                    }
                }
                Err(e) => {
                    error!("Failed to serialize orchestration message: {}", e);
                }
            }
        }
        
        info!("Orchestration outbound message loop stopped");
    }
}

/// Creates the integration between P2PNetworkLayer and P2PAdapter
/// 
/// Returns a task handle that should be spawned to process events.
pub fn create_orchestration_network_integration(
    node_id: String,
    mut event_rx: mpsc::Receiver<NetworkEvent>,
) -> (
    Arc<P2PAdapter>,
    tokio::task::JoinHandle<()>,
    mpsc::Sender<(String, Vec<u8>)>,
) {
    let (bridge, adapter) = OrchestrationNetworkBridge::new(node_id);
    let bridge = Arc::new(RwLock::new(bridge));
    
    // Create publish channel
    let (publish_tx, mut publish_rx) = mpsc::channel::<(String, Vec<u8>)>(100);
    
    // Create handle for the bridge
    let handle = NetworkLayerHandle {
        publish_tx: publish_tx.clone(),
    };
    
    // Spawn event processing task
    let bridge_clone = bridge.clone();
    let event_task = tokio::spawn(async move {
        // Set network handle
        {
            let b = bridge_clone.read().await;
            b.set_network_handle(handle).await;
        }
        
        info!("Starting orchestration network integration event loop");
        
        while let Some(event) = event_rx.recv().await {
            let b = bridge_clone.read().await;
            if let Err(e) = b.process_network_event(event).await {
                warn!("Error processing network event: {}", e);
            }
        }
        
        info!("Orchestration network integration event loop stopped");
    });
    
    (adapter, event_task, publish_tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;
    
    #[tokio::test]
    async fn test_bridge_creation() {
        let (bridge, adapter) = OrchestrationNetworkBridge::new("test-node".to_string());
        assert_eq!(adapter.peer_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_process_peer_discovered() {
        let (bridge, adapter) = OrchestrationNetworkBridge::new("test-node".to_string());
        
        let peer_id = PeerId::random();
        let event = NetworkEvent::PeerDiscovered(peer_id);
        
        bridge.process_network_event(event).await.unwrap();
        
        assert_eq!(adapter.peer_count().await, 1);
    }
    
    #[tokio::test]
    async fn test_process_peer_expired() {
        let (bridge, adapter) = OrchestrationNetworkBridge::new("test-node".to_string());
        
        let peer_id = PeerId::random();
        
        // First discover
        bridge.process_network_event(NetworkEvent::PeerDiscovered(peer_id)).await.unwrap();
        assert_eq!(adapter.peer_count().await, 1);
        
        // Then expire
        bridge.process_network_event(NetworkEvent::PeerExpired(peer_id)).await.unwrap();
        assert_eq!(adapter.peer_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_process_orchestration_message() {
        let (bridge, adapter) = OrchestrationNetworkBridge::new("test-node".to_string());
        
        // Create a heartbeat message
        let heartbeat = HeartbeatMessage::new(
            "node-1".to_string(),
            false,
            vec![],
            0.5,
        );
        let data = serde_json::to_vec(&heartbeat).unwrap();
        
        let event = NetworkEvent::OrchestrationMessageReceived {
            topic: topics::ORCHESTRATION_HEARTBEAT.to_string(),
            data,
            source: None,
        };
        
        bridge.process_network_event(event).await.unwrap();
        
        // The message should be in the adapter's inbound queue
        // (we'd need to check this via receive, but that would consume it)
    }
    
    #[tokio::test]
    async fn test_integration_creation() {
        let (event_tx, event_rx) = mpsc::channel(100);
        
        let (adapter, task, publish_tx) = create_orchestration_network_integration(
            "test-node".to_string(),
            event_rx,
        );
        
        // Send a peer discovered event
        let peer_id = PeerId::random();
        event_tx.send(NetworkEvent::PeerDiscovered(peer_id)).await.unwrap();
        
        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        assert_eq!(adapter.peer_count().await, 1);
        
        // Clean up
        drop(event_tx);
        task.abort();
    }
}

