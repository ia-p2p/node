//! Protocol types and message definitions
//! 
//! This module will contain the protocol message types and validation logic.
//! To be fully implemented in task 2.

use serde::{Deserialize, Serialize};

/// Placeholder for protocol message types
/// These will be fully implemented in task 2 based on the existing schemas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMessage {
    pub message_type: String,
    pub payload: serde_json::Value,
}

// TODO: Implement full protocol types in task 2:
// - AnnounceMsg
// - JobOfferMsg  
// - JobClaimMsg
// - ReceiptMsg
// - Schema validation
// - Cryptographic utilities