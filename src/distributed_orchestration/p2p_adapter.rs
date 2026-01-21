//! P2P Adapter for Distributed Orchestration
//!
//! Bridges the distributed orchestration system with the network layer.

use super::protocols::{ConsensusMessage, GroupMessage, HeartbeatMessage, CoordinationMessage};
use super::types::NodeId;
use super::{DistributedOrchestrator, P2POutboundMessage, P2PSender};
use crate::network::topics;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn, error};

/// P2P Adapter for distributed orchestration
pub struct P2PAdapter {
    /// Node ID
    node_id: NodeId,
    
    /// Sender for outbound messages
    outbound_tx: mpsc::Sender<OrchestrationNetworkMessage>,
    
    /// Receiver for inbound messages
    inbound_rx: Arc<RwLock<mpsc::Receiver<OrchestrationNetworkMessage>>>,
    
    /// Sender for inbound messages (for network layer to send to us)
    inbound_tx: mpsc::Sender<OrchestrationNetworkMessage>,
    
    /// Connected peers
    peers: Arc<RwLock<Vec<NodeId>>>,
}

/// Messages that flow through the P2P adapter
#[derive(Debug, Clone)]
pub enum OrchestrationNetworkMessage {
    /// Consensus-related message
    Consensus(ConsensusMessage),
    
    /// Group management message
    Group(GroupMessage),
    
    /// Coordination message
    Coordination(CoordinationMessage),
    
    /// Heartbeat message
    Heartbeat(HeartbeatMessage),
}

impl OrchestrationNetworkMessage {
    /// Get the gossipsub topic for this message
    pub fn topic(&self) -> &'static str {
        match self {
            Self::Consensus(_) => topics::ORCHESTRATION_CONSENSUS,
            Self::Group(_) => topics::ORCHESTRATION_GROUPS,
            Self::Coordination(_) => topics::ORCHESTRATION_COORDINATION,
            Self::Heartbeat(_) => topics::ORCHESTRATION_HEARTBEAT,
        }
    }
    
    /// Serialize to JSON bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let json = match self {
            Self::Consensus(msg) => serde_json::to_vec(msg)?,
            Self::Group(msg) => serde_json::to_vec(msg)?,
            Self::Coordination(msg) => serde_json::to_vec(msg)?,
            Self::Heartbeat(msg) => serde_json::to_vec(msg)?,
        };
        Ok(json)
    }
    
    /// Deserialize from topic and bytes
    pub fn from_topic_and_bytes(topic: &str, bytes: &[u8]) -> Result<Self> {
        match topic {
            topics::ORCHESTRATION_CONSENSUS => {
                Ok(Self::Consensus(serde_json::from_slice(bytes)?))
            }
            topics::ORCHESTRATION_GROUPS => {
                Ok(Self::Group(serde_json::from_slice(bytes)?))
            }
            topics::ORCHESTRATION_COORDINATION => {
                Ok(Self::Coordination(serde_json::from_slice(bytes)?))
            }
            topics::ORCHESTRATION_HEARTBEAT => {
                Ok(Self::Heartbeat(serde_json::from_slice(bytes)?))
            }
            _ => anyhow::bail!("Unknown orchestration topic: {}", topic),
        }
    }
}

impl P2PAdapter {
    /// Create a new P2P adapter
    pub fn new(node_id: NodeId) -> (Self, mpsc::Receiver<OrchestrationNetworkMessage>) {
        let (outbound_tx, outbound_rx) = mpsc::channel(100);
        let (inbound_tx, inbound_rx) = mpsc::channel(100);
        
        (
            Self {
                node_id,
                outbound_tx,
                inbound_rx: Arc::new(RwLock::new(inbound_rx)),
                inbound_tx,
                peers: Arc::new(RwLock::new(Vec::new())),
            },
            outbound_rx,
        )
    }
    
    /// Get a sender for delivering inbound messages
    pub fn get_inbound_sender(&self) -> mpsc::Sender<OrchestrationNetworkMessage> {
        self.inbound_tx.clone()
    }
    
    /// Register a peer
    pub async fn register_peer(&self, peer_id: NodeId) {
        let mut peers = self.peers.write().await;
        if !peers.contains(&peer_id) {
            peers.push(peer_id.clone());
            info!("Orchestration peer registered: {}", peer_id);
        }
    }
    
    /// Unregister a peer
    pub async fn unregister_peer(&self, peer_id: &NodeId) {
        let mut peers = self.peers.write().await;
        peers.retain(|p| p != peer_id);
        info!("Orchestration peer unregistered: {}", peer_id);
    }
    
    /// Get connected peer count
    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }
    
    /// Get list of connected peers
    pub async fn get_peers(&self) -> Vec<NodeId> {
        self.peers.read().await.clone()
    }
    
    /// Broadcast a consensus message
    pub async fn broadcast_consensus(&self, msg: ConsensusMessage) -> Result<()> {
        debug!("Broadcasting consensus message: {:?}", msg);
        self.outbound_tx
            .send(OrchestrationNetworkMessage::Consensus(msg))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send consensus message: {}", e))
    }
    
    /// Broadcast a group message
    pub async fn broadcast_group(&self, msg: GroupMessage) -> Result<()> {
        debug!("Broadcasting group message: {:?}", msg);
        self.outbound_tx
            .send(OrchestrationNetworkMessage::Group(msg))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send group message: {}", e))
    }
    
    /// Broadcast a coordination message
    pub async fn broadcast_coordination(&self, msg: CoordinationMessage) -> Result<()> {
        debug!("Broadcasting coordination message: {:?}", msg);
        self.outbound_tx
            .send(OrchestrationNetworkMessage::Coordination(msg))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send coordination message: {}", e))
    }
    
    /// Send a heartbeat
    pub async fn send_heartbeat(&self, msg: HeartbeatMessage) -> Result<()> {
        self.outbound_tx
            .send(OrchestrationNetworkMessage::Heartbeat(msg))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send heartbeat: {}", e))
    }
    
    /// Try to receive an inbound message (non-blocking)
    pub async fn try_receive(&self) -> Option<OrchestrationNetworkMessage> {
        let mut rx = self.inbound_rx.write().await;
        rx.try_recv().ok()
    }
    
    /// Receive an inbound message (blocking)
    pub async fn receive(&self) -> Option<OrchestrationNetworkMessage> {
        let mut rx = self.inbound_rx.write().await;
        rx.recv().await
    }
}

/// Creates a P2P sender from the adapter's outbound channel
pub fn create_p2p_sender(
    adapter_outbound_rx: mpsc::Receiver<OrchestrationNetworkMessage>,
) -> (P2PSender, tokio::task::JoinHandle<()>) {
    let (sender, mut internal_rx) = mpsc::channel::<P2POutboundMessage>(100);
    
    // Spawn a task that converts internal messages to network messages
    let handle = tokio::spawn(async move {
        // This would be connected to the actual network layer
        // For now, just log the messages
        loop {
            tokio::select! {
                Some(msg) = internal_rx.recv() => {
                    match msg {
                        P2POutboundMessage::BroadcastProposal(proposal) => {
                            debug!("Would broadcast proposal: {:?}", proposal);
                        }
                        P2POutboundMessage::DirectMessage(peer, msg) => {
                            debug!("Would send direct message to {}: {:?}", peer, msg);
                        }
                        P2POutboundMessage::BroadcastGroupMessage(msg) => {
                            debug!("Would broadcast group message: {:?}", msg);
                        }
                        P2POutboundMessage::Heartbeat(msg) => {
                            debug!("Would send heartbeat: {:?}", msg);
                        }
                    }
                }
                else => break,
            }
        }
    });
    
    (sender, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    
    #[tokio::test]
    async fn test_adapter_creation() {
        let (adapter, _rx) = P2PAdapter::new("test-node".to_string());
        assert_eq!(adapter.peer_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_peer_registration() {
        let (adapter, _rx) = P2PAdapter::new("test-node".to_string());
        
        adapter.register_peer("peer-1".to_string()).await;
        adapter.register_peer("peer-2".to_string()).await;
        
        assert_eq!(adapter.peer_count().await, 2);
        
        adapter.unregister_peer(&"peer-1".to_string()).await;
        assert_eq!(adapter.peer_count().await, 1);
    }
    
    #[tokio::test]
    async fn test_message_serialization() {
        let msg = OrchestrationNetworkMessage::Heartbeat(HeartbeatMessage::new(
            "node-1".to_string(),
            true,
            vec!["translation".to_string()],
            0.5,
        ));
        
        let bytes = msg.to_bytes().unwrap();
        let parsed = OrchestrationNetworkMessage::from_topic_and_bytes(
            topics::ORCHESTRATION_HEARTBEAT,
            &bytes,
        ).unwrap();
        
        if let OrchestrationNetworkMessage::Heartbeat(hb) = parsed {
            assert_eq!(hb.node_id, "node-1");
            assert!(hb.is_coordinator);
        } else {
            panic!("Expected Heartbeat message");
        }
    }
    
    #[tokio::test]
    async fn test_broadcast_consensus() {
        let (adapter, mut rx) = P2PAdapter::new("test-node".to_string());
        
        let msg = ConsensusMessage::proposal(
            "node-1".to_string(),
            "accept".to_string(),
            0.9,
            "High confidence".to_string(),
            super::super::types::DecisionContext::default(),
            "llama".to_string(),
        );
        
        adapter.broadcast_consensus(msg).await.unwrap();
        
        let received = rx.recv().await.unwrap();
        assert!(matches!(received, OrchestrationNetworkMessage::Consensus(_)));
    }
}

