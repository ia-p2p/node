//! Comprehensive error handling and recovery for MVP Node
//!
//! This module provides:
//! - Typed error categories for different failure modes
//! - Recovery strategies for common error scenarios
//! - Error logging with detailed debugging information
//!
//! Implements Requirements 3.3, 3.5, 5.3

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use tracing::{debug, error, info, warn};

/// Main error type for MVP Node operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum MvpNodeError {
    // ========================================================================
    // Network Errors (Requirements 3.x)
    // ========================================================================
    
    /// Peer connection failed
    #[error("Connection error: {message}")]
    ConnectionError {
        message: String,
        peer_id: Option<String>,
        #[serde(skip)]
        recoverable: bool,
    },
    
    /// Peer disconnected unexpectedly
    #[error("Peer disconnected: {peer_id}")]
    PeerDisconnected {
        peer_id: String,
        reason: Option<String>,
    },
    
    /// Network topology change error
    #[error("Topology error: {message}")]
    TopologyError {
        message: String,
        affected_peers: Vec<String>,
    },
    
    /// Message routing error
    #[error("Routing error: {message}")]
    RoutingError {
        message: String,
        topic: Option<String>,
    },
    
    /// Network timeout
    #[error("Network timeout: {operation}")]
    NetworkTimeout {
        operation: String,
        timeout_secs: u64,
    },
    
    // ========================================================================
    // Job Processing Errors (Requirements 2.x)
    // ========================================================================
    
    /// Job submission error
    #[error("Job submission failed: {message}")]
    JobSubmissionError {
        job_id: Option<String>,
        message: String,
    },
    
    /// Job execution error
    #[error("Job execution failed: {message}")]
    JobExecutionError {
        job_id: String,
        message: String,
        duration_ms: u64,
    },
    
    /// Queue full error
    #[error("Job queue full: max size {max_size}, current size {current_size}")]
    QueueFullError {
        max_size: usize,
        current_size: usize,
    },
    
    /// Invalid job input
    #[error("Invalid job input: {message}")]
    InvalidJobInput {
        job_id: String,
        message: String,
    },
    
    // ========================================================================
    // Inference Errors (Requirements 4.x)
    // ========================================================================
    
    /// Model loading error
    #[error("Model load error: {message}")]
    ModelLoadError {
        model_name: String,
        message: String,
    },
    
    /// Inference error
    #[error("Inference error: {message}")]
    InferenceError {
        job_id: Option<String>,
        message: String,
    },
    
    /// Model not loaded
    #[error("No model loaded")]
    ModelNotLoaded,
    
    /// Inference timeout
    #[error("Inference timeout: exceeded {timeout_secs}s")]
    InferenceTimeout {
        job_id: String,
        timeout_secs: u64,
    },
    
    // ========================================================================
    // Configuration Errors (Requirements 8.x)
    // ========================================================================
    
    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigurationError {
        message: String,
        field: Option<String>,
    },
    
    /// Port conflict
    #[error("Port conflict: port {port} is not available")]
    PortConflict {
        port: u16,
    },
    
    // ========================================================================
    // Resource Errors (Requirements 4.5, 6.5)
    // ========================================================================
    
    /// Resource exhausted
    #[error("Resource exhausted: {resource}")]
    ResourceExhausted {
        resource: String,
        current_usage: String,
        limit: String,
    },
    
    /// Memory pressure
    #[error("Memory pressure: {usage_percent}% usage")]
    MemoryPressure {
        usage_percent: f64,
    },
    
    // ========================================================================
    // Protocol Errors (Requirements 7.x)
    // ========================================================================
    
    /// Protocol version mismatch
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    ProtocolVersionMismatch {
        expected: String,
        actual: String,
    },
    
    /// Invalid message format
    #[error("Invalid message format: {message}")]
    InvalidMessageFormat {
        message: String,
    },
    
    /// Signature verification failed
    #[error("Signature verification failed")]
    SignatureError,
    
    // ========================================================================
    // Internal Errors
    // ========================================================================
    
    /// Internal error
    #[error("Internal error: {message}")]
    InternalError {
        message: String,
    },
    
    /// Shutdown in progress
    #[error("Node is shutting down")]
    ShuttingDown,
}

impl MvpNodeError {
    /// Check if the error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ConnectionError { recoverable, .. } => *recoverable,
            Self::PeerDisconnected { .. } => true,
            Self::NetworkTimeout { .. } => true,
            Self::QueueFullError { .. } => true,
            Self::MemoryPressure { .. } => true,
            Self::ModelNotLoaded => true,
            Self::ShuttingDown => false,
            Self::InternalError { .. } => false,
            _ => false,
        }
    }
    
    /// Get error category for logging
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::ConnectionError { .. } |
            Self::PeerDisconnected { .. } |
            Self::TopologyError { .. } |
            Self::RoutingError { .. } |
            Self::NetworkTimeout { .. } => ErrorCategory::Network,
            
            Self::JobSubmissionError { .. } |
            Self::JobExecutionError { .. } |
            Self::QueueFullError { .. } |
            Self::InvalidJobInput { .. } => ErrorCategory::Job,
            
            Self::ModelLoadError { .. } |
            Self::InferenceError { .. } |
            Self::ModelNotLoaded |
            Self::InferenceTimeout { .. } => ErrorCategory::Inference,
            
            Self::ConfigurationError { .. } |
            Self::PortConflict { .. } => ErrorCategory::Configuration,
            
            Self::ResourceExhausted { .. } |
            Self::MemoryPressure { .. } => ErrorCategory::Resource,
            
            Self::ProtocolVersionMismatch { .. } |
            Self::InvalidMessageFormat { .. } |
            Self::SignatureError => ErrorCategory::Protocol,
            
            Self::InternalError { .. } |
            Self::ShuttingDown => ErrorCategory::Internal,
        }
    }
    
    /// Get severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::ShuttingDown => ErrorSeverity::Info,
            Self::QueueFullError { .. } |
            Self::MemoryPressure { .. } |
            Self::NetworkTimeout { .. } => ErrorSeverity::Warning,
            Self::InternalError { .. } |
            Self::SignatureError => ErrorSeverity::Critical,
            _ => ErrorSeverity::Error,
        }
    }
    
    /// Log the error with appropriate level
    pub fn log(&self) {
        let category = self.category();
        let severity = self.severity();
        
        match severity {
            ErrorSeverity::Info => info!(
                category = %category,
                recoverable = %self.is_recoverable(),
                "{}",
                self
            ),
            ErrorSeverity::Warning => warn!(
                category = %category,
                recoverable = %self.is_recoverable(),
                "{}",
                self
            ),
            ErrorSeverity::Error => error!(
                category = %category,
                recoverable = %self.is_recoverable(),
                "{}",
                self
            ),
            ErrorSeverity::Critical => error!(
                category = %category,
                recoverable = %self.is_recoverable(),
                critical = true,
                "CRITICAL: {}",
                self
            ),
        }
    }
}

/// Error category for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    Network,
    Job,
    Inference,
    Configuration,
    Resource,
    Protocol,
    Internal,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network => write!(f, "network"),
            Self::Job => write!(f, "job"),
            Self::Inference => write!(f, "inference"),
            Self::Configuration => write!(f, "configuration"),
            Self::Resource => write!(f, "resource"),
            Self::Protocol => write!(f, "protocol"),
            Self::Internal => write!(f, "internal"),
        }
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Recovery strategy for errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// No action needed
    None,
    /// Retry the operation
    Retry { max_attempts: u32, delay_ms: u64 },
    /// Reconnect to peer
    Reconnect { peer_id: String },
    /// Update peer list
    UpdatePeerList,
    /// Cleanup and continue
    Cleanup,
    /// Throttle operations
    Throttle { delay_ms: u64 },
    /// Graceful shutdown
    Shutdown,
}

impl MvpNodeError {
    /// Get recommended recovery strategy
    pub fn recovery_strategy(&self) -> RecoveryStrategy {
        match self {
            Self::ConnectionError { recoverable: true, .. } => {
                RecoveryStrategy::Retry { max_attempts: 3, delay_ms: 1000 }
            }
            Self::PeerDisconnected { peer_id, .. } => {
                RecoveryStrategy::Reconnect { peer_id: peer_id.clone() }
            }
            Self::TopologyError { .. } => RecoveryStrategy::UpdatePeerList,
            Self::NetworkTimeout { .. } => {
                RecoveryStrategy::Retry { max_attempts: 2, delay_ms: 2000 }
            }
            Self::QueueFullError { .. } => {
                RecoveryStrategy::Throttle { delay_ms: 500 }
            }
            Self::MemoryPressure { .. } => RecoveryStrategy::Cleanup,
            Self::ShuttingDown => RecoveryStrategy::Shutdown,
            _ => RecoveryStrategy::None,
        }
    }
}

/// Error context for detailed logging
#[derive(Debug, Clone, Serialize)]
pub struct ErrorContext {
    /// Node ID where error occurred
    pub node_id: String,
    /// Component where error occurred
    pub component: String,
    /// Operation being performed
    pub operation: String,
    /// Additional context data
    pub data: std::collections::HashMap<String, String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ErrorContext {
    /// Create new error context
    pub fn new(node_id: &str, component: &str, operation: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            component: component.to_string(),
            operation: operation.to_string(),
            data: std::collections::HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }
    
    /// Add context data
    pub fn with_data(mut self, key: &str, value: &str) -> Self {
        self.data.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Log error with context
    pub fn log_error(&self, error: &MvpNodeError) {
        error!(
            node_id = %self.node_id,
            component = %self.component,
            operation = %self.operation,
            category = %error.category(),
            severity = %error.severity(),
            recoverable = %error.is_recoverable(),
            context = ?self.data,
            "Error occurred: {}",
            error
        );
    }
}

/// Topology change event for handling network updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TopologyChange {
    /// New peer joined
    PeerJoined {
        peer_id: String,
        addresses: Vec<String>,
    },
    /// Peer left
    PeerLeft {
        peer_id: String,
        reason: Option<String>,
    },
    /// Peer address updated
    PeerAddressChanged {
        peer_id: String,
        old_addresses: Vec<String>,
        new_addresses: Vec<String>,
    },
}

/// Connection cleanup handler
pub struct ConnectionCleanup {
    /// Pending cleanups
    pending: Vec<String>,
    /// Completed cleanups
    completed: Vec<String>,
    /// Failed cleanups
    failed: Vec<(String, String)>, // (peer_id, error)
}

impl ConnectionCleanup {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            completed: Vec::new(),
            failed: Vec::new(),
        }
    }
    
    /// Schedule cleanup for a peer
    pub fn schedule(&mut self, peer_id: String) {
        if !self.pending.contains(&peer_id) {
            debug!("Scheduling cleanup for peer: {}", peer_id);
            self.pending.push(peer_id);
        }
    }
    
    /// Get next peer to cleanup
    pub fn next(&mut self) -> Option<String> {
        self.pending.pop()
    }
    
    /// Mark cleanup as completed
    pub fn mark_completed(&mut self, peer_id: &str) {
        info!("Cleanup completed for peer: {}", peer_id);
        self.completed.push(peer_id.to_string());
    }
    
    /// Mark cleanup as failed
    pub fn mark_failed(&mut self, peer_id: &str, error: &str) {
        warn!("Cleanup failed for peer {}: {}", peer_id, error);
        self.failed.push((peer_id.to_string(), error.to_string()));
    }
    
    /// Get cleanup statistics
    pub fn stats(&self) -> CleanupStats {
        CleanupStats {
            pending: self.pending.len(),
            completed: self.completed.len(),
            failed: self.failed.len(),
        }
    }
    
    /// Check if there are pending cleanups
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    
    /// Clear completed and failed lists
    pub fn clear_history(&mut self) {
        self.completed.clear();
        self.failed.clear();
    }
}

impl Default for ConnectionCleanup {
    fn default() -> Self {
        Self::new()
    }
}

/// Cleanup statistics
#[derive(Debug, Clone, Serialize)]
pub struct CleanupStats {
    pub pending: usize,
    pub completed: usize,
    pub failed: usize,
}

/// Topology update handler
pub struct TopologyHandler {
    /// Recent topology changes
    changes: Vec<TopologyChange>,
    /// Current peer list
    peers: std::collections::HashSet<String>,
    /// Maximum changes to keep in history
    max_history: usize,
}

impl TopologyHandler {
    pub fn new(max_history: usize) -> Self {
        Self {
            changes: Vec::new(),
            peers: std::collections::HashSet::new(),
            max_history,
        }
    }
    
    /// Handle a topology change
    pub fn handle_change(&mut self, change: TopologyChange) {
        info!("Handling topology change: {:?}", change);
        
        match &change {
            TopologyChange::PeerJoined { peer_id, .. } => {
                self.peers.insert(peer_id.clone());
            }
            TopologyChange::PeerLeft { peer_id, .. } => {
                self.peers.remove(peer_id);
            }
            TopologyChange::PeerAddressChanged { .. } => {
                // Address changes don't affect peer membership
            }
        }
        
        self.changes.push(change);
        
        // Trim history
        if self.changes.len() > self.max_history {
            self.changes.remove(0);
        }
    }
    
    /// Get current peer count
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }
    
    /// Get current peers
    pub fn get_peers(&self) -> Vec<String> {
        self.peers.iter().cloned().collect()
    }
    
    /// Get recent changes
    pub fn recent_changes(&self, limit: usize) -> Vec<&TopologyChange> {
        self.changes.iter().rev().take(limit).collect()
    }
    
    /// Check if peer is known
    pub fn has_peer(&self, peer_id: &str) -> bool {
        self.peers.contains(peer_id)
    }
}

impl Default for TopologyHandler {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Result type for MVP Node operations
pub type MvpResult<T> = Result<T, MvpNodeError>;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_categories() {
        let err = MvpNodeError::ConnectionError {
            message: "test".to_string(),
            peer_id: None,
            recoverable: true,
        };
        assert_eq!(err.category(), ErrorCategory::Network);
        assert!(err.is_recoverable());
        
        let err = MvpNodeError::JobExecutionError {
            job_id: "job1".to_string(),
            message: "failed".to_string(),
            duration_ms: 100,
        };
        assert_eq!(err.category(), ErrorCategory::Job);
        assert!(!err.is_recoverable());
    }
    
    #[test]
    fn test_error_severity() {
        let err = MvpNodeError::ShuttingDown;
        assert_eq!(err.severity(), ErrorSeverity::Info);
        
        let err = MvpNodeError::QueueFullError {
            max_size: 10,
            current_size: 10,
        };
        assert_eq!(err.severity(), ErrorSeverity::Warning);
        
        let err = MvpNodeError::SignatureError;
        assert_eq!(err.severity(), ErrorSeverity::Critical);
    }
    
    #[test]
    fn test_recovery_strategies() {
        let err = MvpNodeError::PeerDisconnected {
            peer_id: "peer1".to_string(),
            reason: None,
        };
        assert!(matches!(err.recovery_strategy(), RecoveryStrategy::Reconnect { .. }));
        
        let err = MvpNodeError::TopologyError {
            message: "test".to_string(),
            affected_peers: vec![],
        };
        assert_eq!(err.recovery_strategy(), RecoveryStrategy::UpdatePeerList);
    }
    
    #[test]
    fn test_connection_cleanup() {
        let mut cleanup = ConnectionCleanup::new();
        
        cleanup.schedule("peer1".to_string());
        cleanup.schedule("peer2".to_string());
        
        assert!(cleanup.has_pending());
        assert_eq!(cleanup.stats().pending, 2);
        
        let peer = cleanup.next().unwrap();
        cleanup.mark_completed(&peer);
        
        assert_eq!(cleanup.stats().pending, 1);
        assert_eq!(cleanup.stats().completed, 1);
    }
    
    #[test]
    fn test_topology_handler() {
        let mut handler = TopologyHandler::new(10);
        
        handler.handle_change(TopologyChange::PeerJoined {
            peer_id: "peer1".to_string(),
            addresses: vec![],
        });
        
        assert_eq!(handler.peer_count(), 1);
        assert!(handler.has_peer("peer1"));
        
        handler.handle_change(TopologyChange::PeerLeft {
            peer_id: "peer1".to_string(),
            reason: Some("disconnect".to_string()),
        });
        
        assert_eq!(handler.peer_count(), 0);
        assert!(!handler.has_peer("peer1"));
    }
}











