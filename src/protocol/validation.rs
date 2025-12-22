//! JSON schema validation for protocol messages

use anyhow::{Result, anyhow};
use serde_json::Value;
use crate::protocol::messages::*;

/// Schema validator for protocol messages
pub struct SchemaValidator {
    job_schema: Value,
    manifest_schema: Value,
    receipt_schema: Value,
}

impl SchemaValidator {
    /// Create a new schema validator with embedded schemas
    pub fn new() -> Result<Self> {
        // Use default schemas for now (could load from files in the future)
        Ok(Self::with_defaults())
    }

    /// Create a validator with default schemas (for when schema files are not available)
    pub fn with_defaults() -> Self {
        Self {
            job_schema: Self::default_job_schema(),
            manifest_schema: Self::default_manifest_schema(),
            receipt_schema: Self::default_receipt_schema(),
        }
    }

    /// Validate a work offer against the job schema
    pub fn validate_work_offer(&self, offer: &WorkOffer) -> Result<()> {
        let json_value = serde_json::to_value(offer)?;
        self.validate_against_schema(&json_value, &self.job_schema, "WorkOffer")
    }

    /// Validate an execution manifest against the manifest schema
    pub fn validate_execution_manifest(&self, manifest: &ExecutionManifest) -> Result<()> {
        let json_value = serde_json::to_value(manifest)?;
        self.validate_against_schema(&json_value, &self.manifest_schema, "ExecutionManifest")
    }

    /// Validate a job receipt against the receipt schema
    pub fn validate_job_receipt(&self, receipt: &JobReceipt) -> Result<()> {
        let json_value = serde_json::to_value(receipt)?;
        self.validate_against_schema(&json_value, &self.receipt_schema, "JobReceipt")
    }

    /// Validate any P2P message
    pub fn validate_p2p_message(&self, message: &P2PMessage) -> Result<()> {
        match message {
            P2PMessage::WorkOffer(offer) => self.validate_work_offer(offer),
            P2PMessage::Receipt(receipt) => self.validate_job_receipt(receipt),
            P2PMessage::Announce(_) => Ok(()), // Announce messages don't have a formal schema yet
            P2PMessage::JobClaim(_) => Ok(()), // Job claim messages don't have a formal schema yet
        }
    }

    /// Basic schema validation (simplified - in production would use jsonschema crate)
    fn validate_against_schema(&self, value: &Value, schema: &Value, schema_name: &str) -> Result<()> {
        // Basic validation - check required fields exist
        if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
            for req_field in required {
                if let Some(field_name) = req_field.as_str() {
                    if !value.get(field_name).is_some() {
                        return Err(anyhow!("Missing required field '{}' in {}", field_name, schema_name));
                    }
                }
            }
        }

        // Basic type validation for known fields
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (field_name, field_schema) in properties {
                if let Some(field_value) = value.get(field_name) {
                    self.validate_field_type(field_value, field_schema, field_name)?;
                }
            }
        }

        Ok(())
    }

    /// Validate field type against schema
    fn validate_field_type(&self, value: &Value, schema: &Value, field_name: &str) -> Result<()> {
        if let Some(expected_type) = schema.get("type").and_then(|t| t.as_str()) {
            let matches = match expected_type {
                "string" => value.is_string(),
                "number" => value.is_number(),
                "integer" => value.is_number() && value.as_f64().map_or(false, |n| n.fract() == 0.0),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => true, // Unknown type, assume valid
            };

            if !matches {
                return Err(anyhow!("Field '{}' has incorrect type, expected {}", field_name, expected_type));
            }
        }

        Ok(())
    }

    /// Default job schema (embedded)
    fn default_job_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["job_id", "model", "reward", "currency"],
            "properties": {
                "job_id": {"type": "string"},
                "model": {"type": "string"},
                "mode": {"type": "string", "enum": ["batch", "session"]},
                "reward": {"type": "number"},
                "currency": {"type": "string"},
                "requirements": {"type": "object"},
                "policy_refs": {"type": "array", "items": {"type": "string"}},
                "session": {"type": "object"}
            }
        })
    }

    /// Default manifest schema (embedded)
    fn default_manifest_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["manifest_id", "version", "issuer_pubkey", "issued_at"],
            "properties": {
                "manifest_id": {"type": "string"},
                "version": {"type": "string"},
                "issuer_pubkey": {"type": "string"},
                "issued_at": {"type": "integer"},
                "description": {"type": "string"},
                "policies": {"type": "object"},
                "capabilities": {"type": "object"},
                "contact": {"type": "string"},
                "signature": {"type": "string"}
            }
        })
    }

    /// Default receipt schema (embedded)
    fn default_receipt_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["receipt_id", "job_id", "executor_pubkey", "outputs_hash", "completed_at", "signature"],
            "properties": {
                "receipt_id": {"type": "string"},
                "job_id": {"type": "string"},
                "session_id": {"type": "string"},
                "sequence": {"type": "integer"},
                "executor_pubkey": {"type": "string"},
                "manifest_id": {"type": "string"},
                "outputs_hash": {"type": "string"},
                "tokens_processed": {"type": "number"},
                "energy_kwh_estimate": {"type": "number"},
                "started_at": {"type": "integer"},
                "completed_at": {"type": "integer"},
                "latency_ms": {"type": "number"},
                "payment_proof": {"type": "string"},
                "chunk_window_seconds": {"type": "integer"},
                "signature": {"type": "string"}
            }
        })
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Validate a work offer message
pub fn validate_work_offer(offer: &WorkOffer) -> Result<()> {
    let validator = SchemaValidator::with_defaults();
    validator.validate_work_offer(offer)
}

/// Validate an execution manifest
pub fn validate_execution_manifest(manifest: &ExecutionManifest) -> Result<()> {
    let validator = SchemaValidator::with_defaults();
    validator.validate_execution_manifest(manifest)
}

/// Validate a job receipt
pub fn validate_job_receipt(receipt: &JobReceipt) -> Result<()> {
    let validator = SchemaValidator::with_defaults();
    validator.validate_job_receipt(receipt)
}

/// Validate any P2P message
pub fn validate_p2p_message(message: &P2PMessage) -> Result<()> {
    let validator = SchemaValidator::with_defaults();
    validator.validate_p2p_message(message)
}