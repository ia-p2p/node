//! Property-based tests for protocol version compatibility
//!
//! Validates:
//! - Property 34: Message encoding compliance
//! - Property 35: Protocol version compatibility

use mvp_node::protocol::{
    ProtocolVersion, VersionError, EncodingFormat, ProtocolCapabilities,
    VersionNegotiationRequest, VersionNegotiationResponse, VersionNegotiator,
    VersionedMessage, MigrationStrategy, CURRENT_VERSION, MIN_SUPPORTED_VERSION,
    P2PMessage, AnnounceMsg, WorkOffer, JobClaimMsg, JobReceipt,
    NodeCapabilities, JobMode, JobRequirements,
};
use proptest::prelude::*;
use serde::{Serialize, Deserialize};

// ============================================================================
// Property 34: Message encoding compliance tests
// ============================================================================

/// Property: All P2P messages should serialize to valid JSON
#[test]
fn test_message_encoding_to_json() {
    let announce = P2PMessage::Announce(AnnounceMsg {
        manifest_id: "test-manifest".to_string(),
        version: "1.0.0".to_string(),
        manifest_hash: "abc123".to_string(),
        capabilities: NodeCapabilities {
            hostname: "test-node".to_string(),
            total_memory_gb: 16.0,
            cpus: vec!["Intel Core i7".to_string()],
            gpus: vec![],
        },
        payment_preferences: vec!["USD".to_string()],
        timestamp: 1234567890,
        peer_id: "peer123".to_string(),
    });

    let bytes = announce.to_bytes();
    assert!(bytes.is_ok(), "Message should serialize to bytes");
    
    let json_str = String::from_utf8(bytes.unwrap());
    assert!(json_str.is_ok(), "Bytes should be valid UTF-8");
    
    let json = json_str.unwrap();
    assert!(json.contains("announce"), "JSON should contain message type");
}

/// Property: Serialized messages should contain type tag
#[test]
fn test_message_encoding_has_type_tag() {
    let messages = vec![
        P2PMessage::Announce(AnnounceMsg {
            manifest_id: "test".to_string(),
            version: "1.0.0".to_string(),
            manifest_hash: "hash".to_string(),
            capabilities: NodeCapabilities {
                hostname: "node".to_string(),
                total_memory_gb: 8.0,
                cpus: vec![],
                gpus: vec![],
            },
            payment_preferences: vec![],
            timestamp: 0,
            peer_id: "peer".to_string(),
        }),
        P2PMessage::WorkOffer(WorkOffer {
            job_id: "job1".to_string(),
            model: "model1".to_string(),
            mode: JobMode::Batch,
            reward: 10.0,
            currency: "USD".to_string(),
            requirements: JobRequirements::default(),
            policy_refs: vec![],
            session: None,
            additional: Default::default(),
        }),
    ];

    for msg in messages {
        let json = String::from_utf8(msg.to_bytes().unwrap()).unwrap();
        assert!(json.contains("\"type\""), "JSON should contain type field");
    }
}

/// Property: Messages should round-trip through serialization
#[test]
fn test_message_encoding_roundtrip() {
    let original = P2PMessage::Announce(AnnounceMsg {
        manifest_id: "test-manifest".to_string(),
        version: "1.0.0".to_string(),
        manifest_hash: "abc123".to_string(),
        capabilities: NodeCapabilities {
            hostname: "test-node".to_string(),
            total_memory_gb: 16.0,
            cpus: vec!["CPU1".to_string()],
            gpus: vec![],
        },
        payment_preferences: vec!["BTC".to_string()],
        timestamp: 1234567890,
        peer_id: "peer123".to_string(),
    });

    let bytes = original.to_bytes().unwrap();
    let decoded = P2PMessage::from_bytes(&bytes).unwrap();
    
    // Re-serialize and compare
    let bytes2 = decoded.to_bytes().unwrap();
    assert_eq!(bytes, bytes2, "Round-trip should produce identical bytes");
}

/// Property: Versioned messages should include version field
#[test]
fn test_versioned_message_includes_version() {
    let msg: VersionedMessage<String> = VersionedMessage::new("test", "Hello".to_string());
    
    let json = msg.to_json().unwrap();
    let json_str = String::from_utf8(json).unwrap();
    
    assert!(json_str.contains("version"), "Should contain version field");
    // Check version components are present (format is serialized as object with major/minor/patch)
    assert!(json_str.contains("major"), "Should contain major field");
    assert!(json_str.contains("minor"), "Should contain minor field");
    assert!(json_str.contains("patch"), "Should contain patch field");
}

/// Property: Versioned messages should round-trip
#[test]
fn test_versioned_message_roundtrip() {
    let original: VersionedMessage<String> = VersionedMessage::new("test", "Hello World".to_string());
    
    let bytes = original.to_json().unwrap();
    let decoded: VersionedMessage<String> = VersionedMessage::from_json(&bytes).unwrap();
    
    assert_eq!(decoded.version, original.version);
    assert_eq!(decoded.msg_type, original.msg_type);
    assert_eq!(decoded.payload, original.payload);
}

// ============================================================================
// Property 35: Protocol version compatibility tests
// ============================================================================

/// Property: Version parsing should work for valid formats
#[test]
fn test_version_parsing_valid() {
    let versions = ["1.0.0", "2.5.10", "0.1.0", "10.20.30"];
    
    for v in versions {
        let parsed = ProtocolVersion::parse(v);
        assert!(parsed.is_ok(), "Should parse: {}", v);
        assert_eq!(parsed.unwrap().to_string(), v);
    }
}

/// Property: Version parsing should fail for invalid formats
#[test]
fn test_version_parsing_invalid() {
    let invalid = ["1.0", "1.0.0.0", "a.b.c", "1.0.x", ""];
    
    for v in invalid {
        let parsed = ProtocolVersion::parse(v);
        assert!(parsed.is_err(), "Should fail: {}", v);
    }
}

/// Property: Same major version should be compatible
#[test]
fn test_version_compatibility_same_major() {
    let v1_0_0 = ProtocolVersion::new(1, 0, 0);
    let v1_5_0 = ProtocolVersion::new(1, 5, 0);
    let v1_9_9 = ProtocolVersion::new(1, 9, 9);
    
    assert!(v1_0_0.is_compatible_with(&v1_5_0));
    assert!(v1_5_0.is_compatible_with(&v1_0_0));
    assert!(v1_0_0.is_compatible_with(&v1_9_9));
}

/// Property: Different major version should be incompatible
#[test]
fn test_version_compatibility_different_major() {
    let v1 = ProtocolVersion::new(1, 0, 0);
    let v2 = ProtocolVersion::new(2, 0, 0);
    let v3 = ProtocolVersion::new(3, 5, 2);
    
    assert!(!v1.is_compatible_with(&v2));
    assert!(!v2.is_compatible_with(&v3));
    assert!(!v1.is_compatible_with(&v3));
}

/// Property: Version comparison should follow semver ordering
#[test]
fn test_version_ordering() {
    let versions = [
        ProtocolVersion::new(1, 0, 0),
        ProtocolVersion::new(1, 0, 1),
        ProtocolVersion::new(1, 1, 0),
        ProtocolVersion::new(2, 0, 0),
    ];
    
    for i in 0..versions.len() - 1 {
        assert!(versions[i] < versions[i + 1]);
    }
}

/// Property: Current version should be supported
#[test]
fn test_current_version_supported() {
    assert!(CURRENT_VERSION.is_current());
    assert!(CURRENT_VERSION.is_supported());
}

/// Property: Negotiation should succeed with compatible versions
#[test]
fn test_negotiation_compatible_versions() {
    let negotiator = VersionNegotiator::new();
    let request = VersionNegotiationRequest::new("test-node");
    
    let response = negotiator.negotiate(&request);
    
    assert!(response.success);
    assert!(response.agreed_version.is_some());
    assert!(response.agreed_encoding.is_some());
}

/// Property: Negotiation should fail with incompatible versions
#[test]
fn test_negotiation_incompatible_versions() {
    // Negotiator expects v2.x
    let negotiator = VersionNegotiator::with_version(
        ProtocolVersion::new(2, 0, 0),
        ProtocolVersion::new(2, 0, 0),
    );
    
    // Request with v1.x
    let mut request = VersionNegotiationRequest::new("test-node");
    request.version = ProtocolVersion::new(1, 0, 0);
    request.min_version = ProtocolVersion::new(1, 0, 0);
    
    let response = negotiator.negotiate(&request);
    
    assert!(!response.success);
    assert!(response.error.is_some());
}

/// Property: Negotiation should choose lower version for compatibility
#[test]
fn test_negotiation_chooses_lower_version() {
    let negotiator = VersionNegotiator::with_version(
        ProtocolVersion::new(1, 5, 0),
        ProtocolVersion::new(1, 0, 0),
    );
    
    let mut request = VersionNegotiationRequest::new("test-node");
    request.version = ProtocolVersion::new(1, 2, 0);
    request.min_version = ProtocolVersion::new(1, 0, 0);
    
    let response = negotiator.negotiate(&request);
    
    assert!(response.success);
    assert_eq!(response.agreed_version, Some(ProtocolVersion::new(1, 2, 0)));
}

/// Property: Capabilities should list supported encodings
#[test]
fn test_capabilities_encodings() {
    let caps = ProtocolCapabilities::default_capabilities();
    
    assert!(caps.supports_encoding(EncodingFormat::Json));
    assert!(!caps.encodings.is_empty());
}

/// Property: Capabilities should list supported message types
#[test]
fn test_capabilities_message_types() {
    let caps = ProtocolCapabilities::default_capabilities();
    
    assert!(caps.supports_message_type("announce"));
    assert!(caps.supports_message_type("work_offer"));
    assert!(caps.supports_message_type("job_claim"));
    assert!(caps.supports_message_type("receipt"));
    
    assert!(!caps.supports_message_type("unknown_type"));
}

/// Property: Migration strategy should be reversible if all steps are
#[test]
fn test_migration_reversibility() {
    let mut migration = MigrationStrategy::new(
        ProtocolVersion::new(1, 0, 0),
        ProtocolVersion::new(2, 0, 0),
    );
    
    migration.add_step("Step 1", true);
    migration.add_step("Step 2", true);
    
    assert!(migration.is_reversible());
    
    migration.add_step("Step 3", false);
    
    assert!(!migration.is_reversible());
}

/// Property: Migration should be possible going forward
#[test]
fn test_migration_direction() {
    let forward = MigrationStrategy::new(
        ProtocolVersion::new(1, 0, 0),
        ProtocolVersion::new(2, 0, 0),
    );
    assert!(forward.is_possible());
    
    let backward = MigrationStrategy::new(
        ProtocolVersion::new(2, 0, 0),
        ProtocolVersion::new(1, 0, 0),
    );
    assert!(!backward.is_possible());
}

// ============================================================================
// Property tests with proptest
// ============================================================================

proptest! {
    /// Property: Version display and parse should round-trip
    #[test]
    fn version_display_parse_roundtrip(
        major in 0u16..100u16,
        minor in 0u16..100u16,
        patch in 0u16..100u16
    ) {
        let v = ProtocolVersion::new(major, minor, patch);
        let s = v.to_string();
        let parsed = ProtocolVersion::parse(&s).unwrap();
        
        prop_assert_eq!(v, parsed);
    }
    
    /// Property: Version ordering should be consistent
    #[test]
    fn version_ordering_consistent(
        a_major in 0u16..10u16,
        a_minor in 0u16..10u16,
        a_patch in 0u16..10u16,
        b_major in 0u16..10u16,
        b_minor in 0u16..10u16,
        b_patch in 0u16..10u16
    ) {
        let a = ProtocolVersion::new(a_major, a_minor, a_patch);
        let b = ProtocolVersion::new(b_major, b_minor, b_patch);
        
        // Antisymmetry: if a < b and b < a, then a == b
        if a < b {
            prop_assert!(!(b < a));
        }
        
        // Reflexivity: a <= a
        prop_assert!(a <= a);
    }
    
    /// Property: Same major version is always compatible
    #[test]
    fn same_major_always_compatible(
        major in 1u16..10u16,
        minor1 in 0u16..20u16,
        patch1 in 0u16..20u16,
        minor2 in 0u16..20u16,
        patch2 in 0u16..20u16
    ) {
        let v1 = ProtocolVersion::new(major, minor1, patch1);
        let v2 = ProtocolVersion::new(major, minor2, patch2);
        
        prop_assert!(v1.is_compatible_with(&v2));
    }
    
    /// Property: JSON encoding should produce valid UTF-8
    #[test]
    fn json_encoding_valid_utf8(
        job_id in "[a-z0-9]{1,20}",
        model in "[a-z0-9]{1,20}",
        reward in 0.0f64..1000.0f64
    ) {
        let msg = P2PMessage::WorkOffer(WorkOffer {
            job_id,
            model,
            mode: JobMode::Batch,
            reward,
            currency: "USD".to_string(),
            requirements: JobRequirements::default(),
            policy_refs: vec![],
            session: None,
            additional: Default::default(),
        });
        
        let bytes = msg.to_bytes().unwrap();
        let utf8_result = String::from_utf8(bytes);
        prop_assert!(utf8_result.is_ok());
    }
}

/// Property: Encoding format display should match name
#[test]
fn test_encoding_format_display() {
    assert_eq!(format!("{}", EncodingFormat::Json), "json");
    assert_eq!(format!("{}", EncodingFormat::Cbor), "cbor");
    assert_eq!(format!("{}", EncodingFormat::MessagePack), "messagepack");
}

/// Property: Default encoding should be JSON
#[test]
fn test_default_encoding_is_json() {
    let encoding = EncodingFormat::default();
    assert_eq!(encoding, EncodingFormat::Json);
}

/// Property: Default version should be current version
#[test]
fn test_default_version_is_current() {
    let version = ProtocolVersion::default();
    assert_eq!(version, CURRENT_VERSION);
}

