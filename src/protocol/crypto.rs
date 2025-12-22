//! Cryptographic utilities for message signing and verification

use anyhow::{Result, anyhow};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use hex;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Cryptographic utilities for protocol messages
pub struct CryptoUtils {
    signing_key: Option<SigningKey>,
}

impl CryptoUtils {
    /// Create new crypto utils without a signing key (verification only)
    pub fn new() -> Self {
        Self { signing_key: None }
    }

    /// Create crypto utils with a signing key for signing
    pub fn with_signing_key(signing_key: SigningKey) -> Self {
        Self {
            signing_key: Some(signing_key),
        }
    }

    /// Generate a new Ed25519 signing key
    pub fn generate_signing_key() -> SigningKey {
        SigningKey::generate(&mut rand::rngs::OsRng)
    }

    /// Get public key as hex string
    pub fn public_key_hex(&self) -> Result<String> {
        match &self.signing_key {
            Some(sk) => {
                let verifying_key = sk.verifying_key();
                Ok(hex::encode(verifying_key.as_bytes()))
            }
            None => Err(anyhow!("No signing key available")),
        }
    }

    /// Sign a message with the loaded signing key
    pub fn sign_message(&self, message: &[u8]) -> Result<String> {
        match &self.signing_key {
            Some(sk) => {
                let signature = sk.sign(message);
                Ok(hex::encode(signature.to_bytes()))
            }
            None => Err(anyhow!("No signing key available for signing")),
        }
    }

    /// Sign a JSON value by serializing it first
    pub fn sign_json(&self, value: &Value) -> Result<String> {
        let message = serde_json::to_vec(value)?;
        self.sign_message(&message)
    }

    /// Verify a message signature
    pub fn verify_signature(&self, message: &[u8], signature_hex: &str, public_key_hex: &str) -> Result<bool> {
        // Decode hex strings
        let signature_bytes = hex::decode(signature_hex)
            .map_err(|e| anyhow!("Invalid signature hex: {}", e))?;
        let public_key_bytes = hex::decode(public_key_hex)
            .map_err(|e| anyhow!("Invalid public key hex: {}", e))?;

        // Convert to fixed-size arrays
        let signature_array: [u8; 64] = signature_bytes.try_into()
            .map_err(|_| anyhow!("Signature must be exactly 64 bytes"))?;
        let public_key_array: [u8; 32] = public_key_bytes.try_into()
            .map_err(|_| anyhow!("Public key must be exactly 32 bytes"))?;

        // Create signature and verifying key objects
        let signature = Signature::from_bytes(&signature_array);
        let verifying_key = VerifyingKey::from_bytes(&public_key_array)
            .map_err(|e| anyhow!("Invalid public key format: {}", e))?;

        // Verify signature
        match verifying_key.verify(message, &signature) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify a JSON signature
    pub fn verify_json_signature(&self, value: &Value, signature_hex: &str, public_key_hex: &str) -> Result<bool> {
        let message = serde_json::to_vec(value)?;
        self.verify_signature(&message, signature_hex, public_key_hex)
    }

    /// Create a signed execution manifest
    pub fn create_signed_manifest(
        &self,
        manifest_id: String,
        version: String,
        description: Option<String>,
        policies: Option<crate::protocol::messages::ManifestPolicies>,
        capabilities: Option<crate::protocol::messages::ManifestCapabilities>,
        contact: Option<String>,
    ) -> Result<crate::protocol::messages::ExecutionManifest> {
        let public_key = self.public_key_hex()?;
        let issued_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        // Create manifest without signature first
        let mut manifest = crate::protocol::messages::ExecutionManifest {
            manifest_id,
            version,
            issuer_pubkey: public_key,
            issued_at,
            description,
            policies,
            capabilities,
            contact,
            signature: None,
            additional: std::collections::HashMap::new(),
        };

        // Sign the manifest
        let manifest_json = serde_json::to_value(&manifest)?;
        let signature = self.sign_json(&manifest_json)?;
        manifest.signature = Some(signature);

        Ok(manifest)
    }

    /// Create a signed job receipt
    pub fn create_signed_receipt(
        &self,
        receipt_id: String,
        job_id: String,
        session_id: Option<String>,
        sequence: Option<i32>,
        manifest_id: Option<String>,
        outputs_hash: String,
        tokens_processed: Option<f64>,
        energy_kwh_estimate: Option<f64>,
        started_at: Option<i64>,
        latency_ms: Option<f64>,
        payment_proof: Option<String>,
        chunk_window_seconds: Option<i32>,
    ) -> Result<crate::protocol::messages::JobReceipt> {
        let executor_pubkey = self.public_key_hex()?;
        let completed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        // Create receipt without signature first
        let mut receipt = crate::protocol::messages::JobReceipt {
            receipt_id,
            job_id,
            session_id,
            sequence,
            executor_pubkey,
            manifest_id,
            outputs_hash,
            tokens_processed,
            energy_kwh_estimate,
            started_at,
            completed_at,
            latency_ms,
            payment_proof,
            chunk_window_seconds,
            signature: String::new(), // Temporary
            additional: std::collections::HashMap::new(),
        };

        // Sign the receipt
        let receipt_json = serde_json::to_value(&receipt)?;
        let signature = self.sign_json(&receipt_json)?;
        receipt.signature = signature;

        Ok(receipt)
    }

    /// Verify an execution manifest signature
    pub fn verify_manifest(&self, manifest: &crate::protocol::messages::ExecutionManifest) -> Result<bool> {
        match &manifest.signature {
            Some(signature) => {
                // Create a copy without signature for verification
                let mut manifest_copy = manifest.clone();
                manifest_copy.signature = None;
                let manifest_json = serde_json::to_value(&manifest_copy)?;
                
                self.verify_json_signature(&manifest_json, signature, &manifest.issuer_pubkey)
            }
            None => Ok(false), // No signature to verify
        }
    }

    /// Verify a job receipt signature
    pub fn verify_receipt(&self, receipt: &crate::protocol::messages::JobReceipt) -> Result<bool> {
        // Create a copy without signature for verification
        let mut receipt_copy = receipt.clone();
        receipt_copy.signature = String::new();
        let receipt_json = serde_json::to_value(&receipt_copy)?;
        
        self.verify_json_signature(&receipt_json, &receipt.signature, &receipt.executor_pubkey)
    }

    /// Hash data using SHA-256
    pub fn hash_data(data: &[u8]) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Hash JSON data
    pub fn hash_json(value: &Value) -> Result<String> {
        let data = serde_json::to_vec(value)?;
        Ok(Self::hash_data(&data))
    }

    /// Generate a unique ID based on timestamp and random data
    pub fn generate_id(prefix: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let random: u32 = rand::random();
        format!("{}-{}-{:x}", prefix, timestamp, random)
    }
}

impl Default for CryptoUtils {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience functions for common operations

/// Generate a new signing key
pub fn generate_signing_key() -> SigningKey {
    CryptoUtils::generate_signing_key()
}

/// Sign a message with a signing key
pub fn sign_message(signing_key: &SigningKey, message: &[u8]) -> String {
    let utils = CryptoUtils::with_signing_key(signing_key.clone());
    utils.sign_message(message).unwrap_or_default()
}

/// Verify a message signature
pub fn verify_signature(message: &[u8], signature_hex: &str, public_key_hex: &str) -> Result<bool> {
    let utils = CryptoUtils::new();
    utils.verify_signature(message, signature_hex, public_key_hex)
}

/// Hash data using SHA-256
pub fn hash_data(data: &[u8]) -> String {
    CryptoUtils::hash_data(data)
}

/// Generate a unique ID
pub fn generate_id(prefix: &str) -> String {
    CryptoUtils::generate_id(prefix)
}