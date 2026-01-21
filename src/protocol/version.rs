//! Protocol Version Handling and Compatibility
//!
//! This module provides:
//! - Protocol version management
//! - Backward compatibility checks
//! - Version negotiation for peer connections
//! - Message encoding compliance with protocol standards
//!
//! Implements Requirements 7.4, 7.5

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use thiserror::Error;

/// Current protocol version
pub const CURRENT_VERSION: ProtocolVersion = ProtocolVersion {
    major: 1,
    minor: 0,
    patch: 0,
};

/// Minimum supported version for backward compatibility
pub const MIN_SUPPORTED_VERSION: ProtocolVersion = ProtocolVersion {
    major: 1,
    minor: 0,
    patch: 0,
};

/// Protocol version following semantic versioning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolVersion {
    /// Create a new protocol version
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }

    /// Parse version from string (e.g., "1.0.0")
    pub fn parse(s: &str) -> Result<Self, VersionError> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::InvalidFormat(s.to_string()));
        }

        let major = parts[0].parse::<u16>()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let minor = parts[1].parse::<u16>()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;
        let patch = parts[2].parse::<u16>()
            .map_err(|_| VersionError::InvalidFormat(s.to_string()))?;

        Ok(Self { major, minor, patch })
    }

    /// Check if this version is compatible with another (backward compatibility)
    /// Compatible if same major version and other's minor is >= our minimum
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        // Same major version required
        if self.major != other.major {
            return false;
        }
        // For same major, any minor/patch is generally compatible
        true
    }

    /// Check if this version supports a feature from another version
    pub fn supports(&self, required: &Self) -> bool {
        // We support if we're >= the required version
        self >= required
    }

    /// Check if this version is the current version
    pub fn is_current(&self) -> bool {
        self == &CURRENT_VERSION
    }

    /// Check if this version is supported
    pub fn is_supported(&self) -> bool {
        self >= &MIN_SUPPORTED_VERSION && self.major == CURRENT_VERSION.major
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for ProtocolVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ProtocolVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => {
                match self.minor.cmp(&other.minor) {
                    Ordering::Equal => self.patch.cmp(&other.patch),
                    other => other,
                }
            }
            other => other,
        }
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        CURRENT_VERSION
    }
}

/// Version-related errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum VersionError {
    #[error("Invalid version format: {0}")]
    InvalidFormat(String),
    
    #[error("Unsupported version: {0}")]
    Unsupported(String),
    
    #[error("Version mismatch: expected {expected}, got {actual}")]
    Mismatch { expected: String, actual: String },
    
    #[error("Incompatible version: {0} is not compatible with {1}")]
    Incompatible(String, String),
}

/// Message encoding format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodingFormat {
    /// JSON encoding (default)
    Json,
    /// CBOR encoding (compact binary)
    Cbor,
    /// MessagePack encoding
    MessagePack,
}

impl Default for EncodingFormat {
    fn default() -> Self {
        Self::Json
    }
}

impl fmt::Display for EncodingFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json => write!(f, "json"),
            Self::Cbor => write!(f, "cbor"),
            Self::MessagePack => write!(f, "messagepack"),
        }
    }
}

/// Protocol capabilities that can be negotiated
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProtocolCapabilities {
    /// Supported encoding formats
    pub encodings: Vec<EncodingFormat>,
    /// Supported message types
    pub message_types: Vec<String>,
    /// Supports streaming
    pub streaming: bool,
    /// Supports compression
    pub compression: bool,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Extensions supported
    pub extensions: Vec<String>,
}

impl ProtocolCapabilities {
    /// Create default capabilities
    pub fn default_capabilities() -> Self {
        Self {
            encodings: vec![EncodingFormat::Json],
            message_types: vec![
                "announce".to_string(),
                "work_offer".to_string(),
                "job_claim".to_string(),
                "receipt".to_string(),
            ],
            streaming: false,
            compression: false,
            max_message_size: 10 * 1024 * 1024, // 10 MB
            extensions: vec![],
        }
    }

    /// Check if we support a specific encoding
    pub fn supports_encoding(&self, format: EncodingFormat) -> bool {
        self.encodings.contains(&format)
    }

    /// Check if we support a specific message type
    pub fn supports_message_type(&self, msg_type: &str) -> bool {
        self.message_types.iter().any(|t| t == msg_type)
    }
}

/// Version negotiation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionNegotiationRequest {
    /// Our protocol version
    pub version: ProtocolVersion,
    /// Minimum version we support
    pub min_version: ProtocolVersion,
    /// Our capabilities
    pub capabilities: ProtocolCapabilities,
    /// Node identifier
    pub node_id: String,
    /// Request timestamp
    pub timestamp: i64,
}

impl VersionNegotiationRequest {
    pub fn new(node_id: &str) -> Self {
        Self {
            version: CURRENT_VERSION,
            min_version: MIN_SUPPORTED_VERSION,
            capabilities: ProtocolCapabilities::default_capabilities(),
            node_id: node_id.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// Version negotiation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionNegotiationResponse {
    /// Whether negotiation was successful
    pub success: bool,
    /// Agreed version (if success)
    pub agreed_version: Option<ProtocolVersion>,
    /// Agreed encoding (if success)
    pub agreed_encoding: Option<EncodingFormat>,
    /// Error message (if failure)
    pub error: Option<String>,
    /// Responder's capabilities
    pub capabilities: ProtocolCapabilities,
    /// Response timestamp
    pub timestamp: i64,
}

impl VersionNegotiationResponse {
    /// Create a successful response
    pub fn success(version: ProtocolVersion, encoding: EncodingFormat) -> Self {
        Self {
            success: true,
            agreed_version: Some(version),
            agreed_encoding: Some(encoding),
            error: None,
            capabilities: ProtocolCapabilities::default_capabilities(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Create a failure response
    pub fn failure(error: &str) -> Self {
        Self {
            success: false,
            agreed_version: None,
            agreed_encoding: None,
            error: Some(error.to_string()),
            capabilities: ProtocolCapabilities::default_capabilities(),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

/// Version negotiator for handling peer connections
pub struct VersionNegotiator {
    /// Our version
    version: ProtocolVersion,
    /// Minimum supported version
    min_version: ProtocolVersion,
    /// Our capabilities
    capabilities: ProtocolCapabilities,
}

impl VersionNegotiator {
    /// Create a new negotiator with default settings
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            min_version: MIN_SUPPORTED_VERSION,
            capabilities: ProtocolCapabilities::default_capabilities(),
        }
    }

    /// Create with custom settings
    pub fn with_version(version: ProtocolVersion, min_version: ProtocolVersion) -> Self {
        Self {
            version,
            min_version,
            capabilities: ProtocolCapabilities::default_capabilities(),
        }
    }

    /// Negotiate with a peer's request
    pub fn negotiate(&self, request: &VersionNegotiationRequest) -> VersionNegotiationResponse {
        // Check if their version is in our supported range
        if request.version < self.min_version {
            return VersionNegotiationResponse::failure(&format!(
                "Version {} is below minimum supported {}",
                request.version, self.min_version
            ));
        }

        // Check if our version is in their supported range
        if self.version < request.min_version {
            return VersionNegotiationResponse::failure(&format!(
                "Our version {} is below their minimum {}",
                self.version, request.min_version
            ));
        }

        // Find common encoding
        let common_encoding = self.find_common_encoding(&request.capabilities);
        if common_encoding.is_none() {
            return VersionNegotiationResponse::failure("No common encoding format");
        }

        // Use lower version for compatibility
        let agreed_version = std::cmp::min(self.version, request.version);

        VersionNegotiationResponse::success(agreed_version, common_encoding.unwrap())
    }

    /// Find a common encoding format
    fn find_common_encoding(&self, other: &ProtocolCapabilities) -> Option<EncodingFormat> {
        // Prefer JSON as it's the standard
        if self.capabilities.supports_encoding(EncodingFormat::Json) 
            && other.supports_encoding(EncodingFormat::Json) {
            return Some(EncodingFormat::Json);
        }

        // Try other formats
        for encoding in &self.capabilities.encodings {
            if other.supports_encoding(*encoding) {
                return Some(*encoding);
            }
        }

        None
    }

    /// Get our version
    pub fn version(&self) -> ProtocolVersion {
        self.version
    }

    /// Get minimum supported version
    pub fn min_version(&self) -> ProtocolVersion {
        self.min_version
    }

    /// Get our capabilities
    pub fn capabilities(&self) -> &ProtocolCapabilities {
        &self.capabilities
    }
}

impl Default for VersionNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

/// Message wrapper with version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedMessage<T> {
    /// Protocol version
    pub version: ProtocolVersion,
    /// Message type tag
    pub msg_type: String,
    /// Message payload
    pub payload: T,
    /// Timestamp
    pub timestamp: i64,
}

impl<T: Serialize> VersionedMessage<T> {
    /// Create a new versioned message
    pub fn new(msg_type: &str, payload: T) -> Self {
        Self {
            version: CURRENT_VERSION,
            msg_type: msg_type.to_string(),
            payload,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Encode to JSON bytes
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

impl<T: for<'de> Deserialize<'de>> VersionedMessage<T> {
    /// Decode from JSON bytes
    pub fn from_json(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
}

/// Migration strategy for protocol updates
#[derive(Debug, Clone)]
pub struct MigrationStrategy {
    /// Source version
    pub from_version: ProtocolVersion,
    /// Target version
    pub to_version: ProtocolVersion,
    /// Migration steps
    pub steps: Vec<MigrationStep>,
}

/// Individual migration step
#[derive(Debug, Clone)]
pub struct MigrationStep {
    /// Step description
    pub description: String,
    /// Step order
    pub order: u32,
    /// Is this step reversible?
    pub reversible: bool,
}

impl MigrationStrategy {
    /// Create a new migration strategy
    pub fn new(from: ProtocolVersion, to: ProtocolVersion) -> Self {
        Self {
            from_version: from,
            to_version: to,
            steps: Vec::new(),
        }
    }

    /// Add a migration step
    pub fn add_step(&mut self, description: &str, reversible: bool) {
        let order = self.steps.len() as u32 + 1;
        self.steps.push(MigrationStep {
            description: description.to_string(),
            order,
            reversible,
        });
    }

    /// Check if migration is possible
    pub fn is_possible(&self) -> bool {
        // Migration is possible if going forward
        self.to_version > self.from_version
    }

    /// Check if migration can be rolled back
    pub fn is_reversible(&self) -> bool {
        self.steps.iter().all(|s| s.reversible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v = ProtocolVersion::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);

        assert!(ProtocolVersion::parse("invalid").is_err());
        assert!(ProtocolVersion::parse("1.2").is_err());
    }

    #[test]
    fn test_version_comparison() {
        let v1 = ProtocolVersion::new(1, 0, 0);
        let v2 = ProtocolVersion::new(1, 1, 0);
        let v3 = ProtocolVersion::new(2, 0, 0);

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v1 < v3);
    }

    #[test]
    fn test_version_compatibility() {
        let v1 = ProtocolVersion::new(1, 0, 0);
        let v2 = ProtocolVersion::new(1, 1, 0);
        let v3 = ProtocolVersion::new(2, 0, 0);

        assert!(v1.is_compatible_with(&v2));
        assert!(!v1.is_compatible_with(&v3));
    }

    #[test]
    fn test_version_display() {
        let v = ProtocolVersion::new(1, 2, 3);
        assert_eq!(format!("{}", v), "1.2.3");
    }

    #[test]
    fn test_negotiation_success() {
        let negotiator = VersionNegotiator::new();
        let request = VersionNegotiationRequest::new("test-node");

        let response = negotiator.negotiate(&request);
        assert!(response.success);
        assert!(response.agreed_version.is_some());
    }

    #[test]
    fn test_negotiation_failure_version_too_old() {
        let negotiator = VersionNegotiator::with_version(
            ProtocolVersion::new(2, 0, 0),
            ProtocolVersion::new(2, 0, 0),
        );

        let mut request = VersionNegotiationRequest::new("test-node");
        request.version = ProtocolVersion::new(1, 0, 0);

        let response = negotiator.negotiate(&request);
        assert!(!response.success);
        assert!(response.error.is_some());
    }

    #[test]
    fn test_migration_strategy() {
        let mut migration = MigrationStrategy::new(
            ProtocolVersion::new(1, 0, 0),
            ProtocolVersion::new(2, 0, 0),
        );

        migration.add_step("Update message format", true);
        migration.add_step("Add new fields", true);

        assert!(migration.is_possible());
        assert!(migration.is_reversible());
        assert_eq!(migration.steps.len(), 2);
    }
}











