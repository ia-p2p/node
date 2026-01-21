//! Property-based tests for error handling and recovery
//!
//! Validates:
//! - Property 13: Topology update handling
//! - Property 15: Connection cleanup
//! - Requirements 3.3, 3.5, 5.3

use mvp_node::error::{
    MvpNodeError, ErrorCategory, ErrorSeverity, RecoveryStrategy,
    ConnectionCleanup, TopologyHandler, TopologyChange, ErrorContext,
    CleanupStats, MvpResult,
};
use proptest::prelude::*;

// ============================================================================
// Property 13: Topology update handling tests
// ============================================================================

/// Property: Peer join events should add peer to list
#[test]
fn test_topology_peer_join() {
    let mut handler = TopologyHandler::new(10);
    
    handler.handle_change(TopologyChange::PeerJoined {
        peer_id: "peer1".to_string(),
        addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
    });
    
    assert_eq!(handler.peer_count(), 1);
    assert!(handler.has_peer("peer1"));
}

/// Property: Peer leave events should remove peer from list
#[test]
fn test_topology_peer_leave() {
    let mut handler = TopologyHandler::new(10);
    
    handler.handle_change(TopologyChange::PeerJoined {
        peer_id: "peer1".to_string(),
        addresses: vec![],
    });
    assert!(handler.has_peer("peer1"));
    
    handler.handle_change(TopologyChange::PeerLeft {
        peer_id: "peer1".to_string(),
        reason: Some("disconnect".to_string()),
    });
    
    assert!(!handler.has_peer("peer1"));
    assert_eq!(handler.peer_count(), 0);
}

/// Property: Address changes should not affect peer membership
#[test]
fn test_topology_address_change() {
    let mut handler = TopologyHandler::new(10);
    
    handler.handle_change(TopologyChange::PeerJoined {
        peer_id: "peer1".to_string(),
        addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
    });
    
    handler.handle_change(TopologyChange::PeerAddressChanged {
        peer_id: "peer1".to_string(),
        old_addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
        new_addresses: vec!["/ip4/192.168.1.1/tcp/9000".to_string()],
    });
    
    // Peer should still be present
    assert!(handler.has_peer("peer1"));
    assert_eq!(handler.peer_count(), 1);
}

/// Property: Topology handler should limit history size
#[test]
fn test_topology_history_limit() {
    let mut handler = TopologyHandler::new(5);
    
    // Add more changes than history limit
    for i in 0..10 {
        handler.handle_change(TopologyChange::PeerJoined {
            peer_id: format!("peer{}", i),
            addresses: vec![],
        });
    }
    
    // Should only keep last 5 changes
    let recent = handler.recent_changes(100);
    assert!(recent.len() <= 5);
}

/// Property: Recent changes should be in reverse order
#[test]
fn test_topology_recent_changes_order() {
    let mut handler = TopologyHandler::new(100);
    
    handler.handle_change(TopologyChange::PeerJoined {
        peer_id: "first".to_string(),
        addresses: vec![],
    });
    handler.handle_change(TopologyChange::PeerJoined {
        peer_id: "second".to_string(),
        addresses: vec![],
    });
    handler.handle_change(TopologyChange::PeerJoined {
        peer_id: "third".to_string(),
        addresses: vec![],
    });
    
    let recent = handler.recent_changes(2);
    assert_eq!(recent.len(), 2);
    
    // Most recent should be first
    if let TopologyChange::PeerJoined { peer_id, .. } = recent[0] {
        assert_eq!(peer_id, "third");
    }
}

// ============================================================================
// Property 15: Connection cleanup tests
// ============================================================================

/// Property: Cleanup should schedule peers for cleanup
#[test]
fn test_connection_cleanup_schedule() {
    let mut cleanup = ConnectionCleanup::new();
    
    cleanup.schedule("peer1".to_string());
    cleanup.schedule("peer2".to_string());
    
    let stats = cleanup.stats();
    assert_eq!(stats.pending, 2);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.failed, 0);
}

/// Property: Cleanup should not duplicate scheduled peers
#[test]
fn test_connection_cleanup_no_duplicates() {
    let mut cleanup = ConnectionCleanup::new();
    
    cleanup.schedule("peer1".to_string());
    cleanup.schedule("peer1".to_string()); // Duplicate
    cleanup.schedule("peer1".to_string()); // Duplicate
    
    assert_eq!(cleanup.stats().pending, 1);
}

/// Property: Cleanup next() should return scheduled peers
#[test]
fn test_connection_cleanup_next() {
    let mut cleanup = ConnectionCleanup::new();
    
    cleanup.schedule("peer1".to_string());
    cleanup.schedule("peer2".to_string());
    
    let peer1 = cleanup.next();
    assert!(peer1.is_some());
    
    let peer2 = cleanup.next();
    assert!(peer2.is_some());
    
    let peer3 = cleanup.next();
    assert!(peer3.is_none());
}

/// Property: Completed cleanups should be tracked
#[test]
fn test_connection_cleanup_completed() {
    let mut cleanup = ConnectionCleanup::new();
    
    cleanup.schedule("peer1".to_string());
    let peer = cleanup.next().unwrap();
    cleanup.mark_completed(&peer);
    
    let stats = cleanup.stats();
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.completed, 1);
}

/// Property: Failed cleanups should be tracked
#[test]
fn test_connection_cleanup_failed() {
    let mut cleanup = ConnectionCleanup::new();
    
    cleanup.schedule("peer1".to_string());
    let peer = cleanup.next().unwrap();
    cleanup.mark_failed(&peer, "connection reset");
    
    let stats = cleanup.stats();
    assert_eq!(stats.pending, 0);
    assert_eq!(stats.failed, 1);
}

/// Property: Clear history should reset completed/failed
#[test]
fn test_connection_cleanup_clear_history() {
    let mut cleanup = ConnectionCleanup::new();
    
    cleanup.schedule("peer1".to_string());
    let peer = cleanup.next().unwrap();
    cleanup.mark_completed(&peer);
    
    cleanup.schedule("peer2".to_string());
    let peer = cleanup.next().unwrap();
    cleanup.mark_failed(&peer, "error");
    
    assert_eq!(cleanup.stats().completed, 1);
    assert_eq!(cleanup.stats().failed, 1);
    
    cleanup.clear_history();
    
    assert_eq!(cleanup.stats().completed, 0);
    assert_eq!(cleanup.stats().failed, 0);
}

// ============================================================================
// Error type tests
// ============================================================================

/// Property: Error categories should be correctly assigned
#[test]
fn test_error_categories() {
    let network_err = MvpNodeError::ConnectionError {
        message: "test".to_string(),
        peer_id: None,
        recoverable: true,
    };
    assert_eq!(network_err.category(), ErrorCategory::Network);
    
    let job_err = MvpNodeError::JobExecutionError {
        job_id: "job1".to_string(),
        message: "failed".to_string(),
        duration_ms: 100,
    };
    assert_eq!(job_err.category(), ErrorCategory::Job);
    
    let inference_err = MvpNodeError::ModelNotLoaded;
    assert_eq!(inference_err.category(), ErrorCategory::Inference);
    
    let config_err = MvpNodeError::PortConflict { port: 9000 };
    assert_eq!(config_err.category(), ErrorCategory::Configuration);
    
    let resource_err = MvpNodeError::MemoryPressure { usage_percent: 90.0 };
    assert_eq!(resource_err.category(), ErrorCategory::Resource);
    
    let protocol_err = MvpNodeError::SignatureError;
    assert_eq!(protocol_err.category(), ErrorCategory::Protocol);
}

/// Property: Recoverable errors should be correctly identified
#[test]
fn test_error_recoverability() {
    // Recoverable errors
    let conn_err = MvpNodeError::ConnectionError {
        message: "test".to_string(),
        peer_id: None,
        recoverable: true,
    };
    assert!(conn_err.is_recoverable());
    
    let disconnect_err = MvpNodeError::PeerDisconnected {
        peer_id: "peer1".to_string(),
        reason: None,
    };
    assert!(disconnect_err.is_recoverable());
    
    let queue_err = MvpNodeError::QueueFullError {
        max_size: 10,
        current_size: 10,
    };
    assert!(queue_err.is_recoverable());
    
    // Non-recoverable errors
    let shutdown_err = MvpNodeError::ShuttingDown;
    assert!(!shutdown_err.is_recoverable());
    
    let internal_err = MvpNodeError::InternalError {
        message: "test".to_string(),
    };
    assert!(!internal_err.is_recoverable());
}

/// Property: Severity levels should be correctly assigned
#[test]
fn test_error_severity() {
    let info_err = MvpNodeError::ShuttingDown;
    assert_eq!(info_err.severity(), ErrorSeverity::Info);
    
    let warn_err = MvpNodeError::QueueFullError {
        max_size: 10,
        current_size: 10,
    };
    assert_eq!(warn_err.severity(), ErrorSeverity::Warning);
    
    let err = MvpNodeError::JobExecutionError {
        job_id: "job1".to_string(),
        message: "failed".to_string(),
        duration_ms: 100,
    };
    assert_eq!(err.severity(), ErrorSeverity::Error);
    
    let critical_err = MvpNodeError::SignatureError;
    assert_eq!(critical_err.severity(), ErrorSeverity::Critical);
}

/// Property: Recovery strategies should be appropriate
#[test]
fn test_recovery_strategies() {
    let conn_err = MvpNodeError::ConnectionError {
        message: "test".to_string(),
        peer_id: None,
        recoverable: true,
    };
    assert!(matches!(conn_err.recovery_strategy(), RecoveryStrategy::Retry { .. }));
    
    let disconnect_err = MvpNodeError::PeerDisconnected {
        peer_id: "peer1".to_string(),
        reason: None,
    };
    assert!(matches!(disconnect_err.recovery_strategy(), RecoveryStrategy::Reconnect { .. }));
    
    let topology_err = MvpNodeError::TopologyError {
        message: "test".to_string(),
        affected_peers: vec![],
    };
    assert_eq!(topology_err.recovery_strategy(), RecoveryStrategy::UpdatePeerList);
    
    let queue_err = MvpNodeError::QueueFullError {
        max_size: 10,
        current_size: 10,
    };
    assert!(matches!(queue_err.recovery_strategy(), RecoveryStrategy::Throttle { .. }));
    
    let memory_err = MvpNodeError::MemoryPressure { usage_percent: 90.0 };
    assert_eq!(memory_err.recovery_strategy(), RecoveryStrategy::Cleanup);
    
    let shutdown_err = MvpNodeError::ShuttingDown;
    assert_eq!(shutdown_err.recovery_strategy(), RecoveryStrategy::Shutdown);
}

/// Property: Error context should store data correctly
#[test]
fn test_error_context() {
    let context = ErrorContext::new("node-1", "network", "connect")
        .with_data("peer", "peer-1")
        .with_data("port", "9000");
    
    assert_eq!(context.node_id, "node-1");
    assert_eq!(context.component, "network");
    assert_eq!(context.operation, "connect");
    assert_eq!(context.data.len(), 2);
    assert_eq!(context.data.get("peer"), Some(&"peer-1".to_string()));
}

// ============================================================================
// Property tests with proptest
// ============================================================================

proptest! {
    /// Property: All errors should have a valid category
    #[test]
    fn all_errors_have_category(port in 1024u16..65535u16) {
        let errors = vec![
            MvpNodeError::ConnectionError {
                message: "test".to_string(),
                peer_id: None,
                recoverable: true,
            },
            MvpNodeError::PortConflict { port },
            MvpNodeError::ModelNotLoaded,
            MvpNodeError::ShuttingDown,
        ];
        
        for err in errors {
            let _category = err.category(); // Should not panic
            let _severity = err.severity();
            let _recoverable = err.is_recoverable();
            let _strategy = err.recovery_strategy();
        }
    }
    
    /// Property: Topology handler maintains accurate peer count
    #[test]
    fn topology_maintains_peer_count(peer_count in 1usize..20usize) {
        let mut handler = TopologyHandler::new(100);
        
        for i in 0..peer_count {
            handler.handle_change(TopologyChange::PeerJoined {
                peer_id: format!("peer{}", i),
                addresses: vec![],
            });
        }
        
        prop_assert_eq!(handler.peer_count(), peer_count);
        
        // Remove half
        let to_remove = peer_count / 2;
        for i in 0..to_remove {
            handler.handle_change(TopologyChange::PeerLeft {
                peer_id: format!("peer{}", i),
                reason: None,
            });
        }
        
        prop_assert_eq!(handler.peer_count(), peer_count - to_remove);
    }
    
    /// Property: Cleanup stats are always consistent
    #[test]
    fn cleanup_stats_consistent(schedule_count in 0usize..10usize) {
        let mut cleanup = ConnectionCleanup::new();
        
        for i in 0..schedule_count {
            cleanup.schedule(format!("peer{}", i));
        }
        
        let stats = cleanup.stats();
        prop_assert_eq!(stats.pending, schedule_count);
        prop_assert_eq!(stats.completed, 0);
        prop_assert_eq!(stats.failed, 0);
    }
}

/// Property: Error display messages should not be empty
#[test]
fn test_error_display_not_empty() {
    let errors: Vec<MvpNodeError> = vec![
        MvpNodeError::ConnectionError {
            message: "test".to_string(),
            peer_id: None,
            recoverable: true,
        },
        MvpNodeError::PeerDisconnected {
            peer_id: "peer1".to_string(),
            reason: None,
        },
        MvpNodeError::ModelNotLoaded,
        MvpNodeError::ShuttingDown,
        MvpNodeError::SignatureError,
    ];
    
    for err in errors {
        let display = format!("{}", err);
        assert!(!display.is_empty());
    }
}

/// Property: MvpResult should work with standard Result operations
#[test]
fn test_mvp_result_type() {
    fn operation_that_succeeds() -> MvpResult<i32> {
        Ok(42)
    }
    
    fn operation_that_fails() -> MvpResult<i32> {
        Err(MvpNodeError::ModelNotLoaded)
    }
    
    assert!(operation_that_succeeds().is_ok());
    assert!(operation_that_fails().is_err());
    
    let result = operation_that_succeeds().map(|x| x * 2);
    assert_eq!(result.unwrap(), 84);
}











