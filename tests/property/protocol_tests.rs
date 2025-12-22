use proptest::prelude::*;
use mvp_node::protocol::*;
use std::collections::HashMap;

/// **Feature: mvp-node, Property 31: Job schema compliance**
/// 
/// For any job announcement sent by the MVP_Node, the message should conform 
/// to the standard job schema format defined in job.schema.json.
/// 
/// **Validates: Requirements 7.1**
#[cfg(test)]
mod protocol_property_tests {
    use super::*;

    // Generator for valid WorkOffer instances
    fn arb_work_offer() -> impl Strategy<Value = WorkOffer> {
        (
            "[a-zA-Z0-9_-]{8,32}",  // job_id
            "[a-zA-Z0-9_/-]{1,50}",  // model
            prop::option::of(prop_oneof![
                Just(JobMode::Batch),
                Just(JobMode::Session)
            ]),
            1.0f64..1000.0f64,       // reward
            "[A-Z]{3}",              // currency (e.g., USD, BTC)
            prop::collection::hash_map("[a-z_]+", any::<f64>(), 0..5), // requirements
            prop::collection::vec("[a-zA-Z0-9_-]+", 0..3), // policy_refs
        ).prop_map(|(job_id, model, mode_opt, reward, currency, req_map, policy_refs)| {
            let mut requirements = JobRequirements::default();
            let mut additional = HashMap::new();
            
            for (key, value) in req_map {
                additional.insert(key, serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()));
            }
            requirements.additional = additional;

            WorkOffer {
                job_id,
                model,
                mode: mode_opt.unwrap_or_default(),
                reward,
                currency,
                requirements,
                policy_refs,
                session: None, // Simplified for now
                additional: HashMap::new(),
            }
        })
    }

    // Generator for valid ExecutionManifest instances
    fn arb_execution_manifest() -> impl Strategy<Value = ExecutionManifest> {
        (
            "[a-zA-Z0-9_-]{8,32}",  // manifest_id
            "[0-9]+\\.[0-9]+\\.[0-9]+", // version (semver)
            "[a-fA-F0-9]{64}",       // issuer_pubkey (hex)
            1640000000i64..2000000000i64, // issued_at (reasonable timestamp)
            prop::option::of("[a-zA-Z0-9 .,!?-]{10,100}"), // description
            prop::option::of("[a-zA-Z0-9@.-]+"), // contact
        ).prop_map(|(manifest_id, version, issuer_pubkey, issued_at, description, contact)| {
            ExecutionManifest {
                manifest_id,
                version,
                issuer_pubkey,
                issued_at,
                description,
                policies: None, // Simplified for now
                capabilities: None, // Simplified for now
                contact,
                signature: None, // Will be added by crypto utils
                additional: HashMap::new(),
            }
        })
    }

    // Generator for valid JobReceipt instances
    fn arb_job_receipt() -> impl Strategy<Value = JobReceipt> {
        (
            "[a-zA-Z0-9_-]{8,32}",  // receipt_id
            "[a-zA-Z0-9_-]{8,32}",  // job_id
            "[a-fA-F0-9]{64}",       // executor_pubkey
            "[a-fA-F0-9]{64}",       // outputs_hash
            1640000000i64..2000000000i64, // completed_at
            "[a-fA-F0-9]{128}",      // signature (placeholder)
            prop::option::of(1.0f64..10000.0f64), // tokens_processed
            prop::option::of(0.001f64..1.0f64),   // energy_kwh_estimate
            prop::option::of(1.0f64..30000.0f64), // latency_ms
        ).prop_map(|(receipt_id, job_id, executor_pubkey, outputs_hash, completed_at, signature, tokens_processed, energy_kwh_estimate, latency_ms)| {
            JobReceipt {
                receipt_id,
                job_id,
                session_id: None,
                sequence: None,
                executor_pubkey,
                manifest_id: None,
                outputs_hash,
                tokens_processed,
                energy_kwh_estimate,
                started_at: None,
                completed_at,
                latency_ms,
                payment_proof: None,
                chunk_window_seconds: None,
                signature,
                additional: HashMap::new(),
            }
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// Property 31: Job schema compliance
        /// For any WorkOffer, serialization and deserialization should preserve all fields
        #[test]
        fn work_offer_serialization_roundtrip(offer in arb_work_offer()) {
            // Serialize to JSON
            let json_str = serde_json::to_string(&offer).unwrap();
            
            // Deserialize back
            let deserialized: WorkOffer = serde_json::from_str(&json_str).unwrap();
            
            // Verify all required fields are preserved
            prop_assert_eq!(deserialized.job_id, offer.job_id);
            prop_assert_eq!(deserialized.model, offer.model);
            prop_assert_eq!(deserialized.mode, offer.mode);
            // Use approximate equality for floating point
            prop_assert!((deserialized.reward - offer.reward).abs() < 1e-10, 
                        "Reward should be approximately equal: {} vs {}", deserialized.reward, offer.reward);
            prop_assert_eq!(deserialized.currency, offer.currency);
            prop_assert_eq!(deserialized.policy_refs, offer.policy_refs);
        }
        
        /// Property 31: Job schema validation
        /// For any valid WorkOffer, schema validation should succeed
        #[test]
        fn work_offer_schema_validation(offer in arb_work_offer()) {
            let result = validate_work_offer(&offer);
            prop_assert!(result.is_ok(), "WorkOffer should pass schema validation: {:?}", result);
        }
        
        /// Property 32: Receipt format and signature compliance
        /// For any JobReceipt, serialization should preserve required fields
        #[test]
        fn job_receipt_serialization_roundtrip(receipt in arb_job_receipt()) {
            // Serialize to JSON
            let json_str = serde_json::to_string(&receipt).unwrap();
            
            // Deserialize back
            let deserialized: JobReceipt = serde_json::from_str(&json_str).unwrap();
            
            // Verify required fields are preserved
            prop_assert_eq!(deserialized.receipt_id, receipt.receipt_id);
            prop_assert_eq!(deserialized.job_id, receipt.job_id);
            prop_assert_eq!(deserialized.executor_pubkey, receipt.executor_pubkey);
            prop_assert_eq!(deserialized.outputs_hash, receipt.outputs_hash);
            prop_assert_eq!(deserialized.completed_at, receipt.completed_at);
            prop_assert_eq!(deserialized.signature, receipt.signature);
        }
        
        /// Property 32: Receipt schema validation
        /// For any valid JobReceipt, schema validation should succeed
        #[test]
        fn job_receipt_schema_validation(receipt in arb_job_receipt()) {
            let result = validate_job_receipt(&receipt);
            prop_assert!(result.is_ok(), "JobReceipt should pass schema validation: {:?}", result);
        }
        
        /// Property 33: Manifest schema compliance
        /// For any ExecutionManifest, serialization should preserve required fields
        #[test]
        fn execution_manifest_serialization_roundtrip(manifest in arb_execution_manifest()) {
            // Serialize to JSON
            let json_str = serde_json::to_string(&manifest).unwrap();
            
            // Deserialize back
            let deserialized: ExecutionManifest = serde_json::from_str(&json_str).unwrap();
            
            // Verify required fields are preserved
            prop_assert_eq!(deserialized.manifest_id, manifest.manifest_id);
            prop_assert_eq!(deserialized.version, manifest.version);
            prop_assert_eq!(deserialized.issuer_pubkey, manifest.issuer_pubkey);
            prop_assert_eq!(deserialized.issued_at, manifest.issued_at);
        }
        
        /// Property 33: Manifest schema validation
        /// For any valid ExecutionManifest, schema validation should succeed
        #[test]
        fn execution_manifest_schema_validation(manifest in arb_execution_manifest()) {
            let result = validate_execution_manifest(&manifest);
            prop_assert!(result.is_ok(), "ExecutionManifest should pass schema validation: {:?}", result);
        }
        
        /// Property: P2P message serialization consistency
        /// For any P2P message, serialization and deserialization should be consistent
        #[test]
        fn p2p_message_serialization_roundtrip(offer in arb_work_offer()) {
            let message = P2PMessage::WorkOffer(offer);
            
            // Serialize to bytes
            let bytes = message.to_bytes().unwrap();
            
            // Deserialize back
            let deserialized = P2PMessage::from_bytes(&bytes).unwrap();
            
            // Verify message type is preserved
            prop_assert_eq!(message.message_type(), deserialized.message_type());
            
            // Verify content is preserved (for WorkOffer)
            if let (P2PMessage::WorkOffer(orig), P2PMessage::WorkOffer(deser)) = (&message, &deserialized) {
                prop_assert_eq!(&orig.job_id, &deser.job_id);
                prop_assert_eq!(&orig.model, &deser.model);
                // Use approximate equality for floating point
                prop_assert!((orig.reward - deser.reward).abs() < 1e-10, 
                            "Reward should be approximately equal: {} vs {}", orig.reward, deser.reward);
            }
        }
        
        /// Property: Cryptographic signature verification
        /// For any message signed with a key, verification with the same key should succeed
        #[test]
        fn cryptographic_signature_verification(message_data in prop::collection::vec(any::<u8>(), 1..1000)) {
            let signing_key = generate_signing_key();
            let crypto_utils = CryptoUtils::with_signing_key(signing_key.clone());
            
            // Sign the message
            let signature = crypto_utils.sign_message(&message_data).unwrap();
            let public_key = crypto_utils.public_key_hex().unwrap();
            
            // Verify the signature
            let is_valid = crypto_utils.verify_signature(&message_data, &signature, &public_key).unwrap();
            prop_assert!(is_valid, "Signature verification should succeed for valid signature");
        }
        
        /// Property: Hash consistency
        /// For any data, hashing should be deterministic
        #[test]
        fn hash_consistency(data in prop::collection::vec(any::<u8>(), 1..1000)) {
            let hash1 = hash_data(&data);
            let hash2 = hash_data(&data);
            
            prop_assert_eq!(&hash1, &hash2, "Hash should be deterministic");
            prop_assert_eq!(hash1.len(), 64, "SHA-256 hash should be 64 hex characters");
        }
        
        /// Property: ID generation uniqueness
        /// For any prefix, generated IDs should be unique
        #[test]
        fn id_generation_uniqueness(prefix in "[a-zA-Z]{1,10}") {
            let id1 = generate_id(&prefix);
            let id2 = generate_id(&prefix);
            
            prop_assert_ne!(&id1, &id2, "Generated IDs should be unique");
            prop_assert!(id1.starts_with(&prefix), "ID should start with prefix");
            prop_assert!(id2.starts_with(&prefix), "ID should start with prefix");
        }
    }

    #[test]
    fn test_invalid_work_offer_missing_required_fields() {
        // Test with missing required field (job_id)
        let invalid_json = r#"{"model": "test-model", "reward": 100.0, "currency": "USD"}"#;
        let result: Result<WorkOffer, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err(), "Should fail to deserialize WorkOffer missing job_id");
    }

    #[test]
    fn test_invalid_signature_verification() {
        let signing_key = generate_signing_key();
        let crypto_utils = CryptoUtils::with_signing_key(signing_key);
        
        let message = b"test message";
        let public_key = crypto_utils.public_key_hex().unwrap();
        let invalid_signature = "invalid_signature_hex";
        
        let result = crypto_utils.verify_signature(message, invalid_signature, &public_key);
        assert!(result.is_err(), "Should fail to verify invalid signature");
    }
}