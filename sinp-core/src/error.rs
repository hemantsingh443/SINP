//! Error types and refusal codes for SINP.

use thiserror::Error;

/// Refusal codes as defined in RFC 0.1 Appendix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalCode {
    /// Semantic hash mismatch or invalid structure.
    MalformedContext,
    /// Request requires PII but privacy constraints forbid.
    PrivacyViolation,
    /// No capability matches intent with Φ > 0.2.
    CapabilityMissing,
    /// Intent understood but forbidden by server rules.
    PolicyViolation,
}

impl std::fmt::Display for RefusalCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedContext => write!(f, "malformed_context"),
            Self::PrivacyViolation => write!(f, "privacy_violation"),
            Self::CapabilityMissing => write!(f, "capability_missing"),
            Self::PolicyViolation => write!(f, "policy_violation"),
        }
    }
}

/// SINP protocol errors.
#[derive(Debug, Error)]
pub enum SinpError {
    /// Protocol-level error (malformed message, invalid state transition).
    #[error("protocol error: {0}")]
    Protocol(String),

    /// Validation error (schema, constraints).
    #[error("validation error: {0}")]
    Validation(String),

    /// Cryptographic error (signature, hash).
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Transport error (connection, I/O).
    #[error("transport error: {0}")]
    Transport(String),

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Request was refused.
    #[error("refused: {code} - {reason}")]
    Refused {
        code: RefusalCode,
        reason: String,
    },

    /// Replay attack detected.
    #[error("replay attack detected: message timestamp {timestamp} outside acceptable window")]
    ReplayDetected { timestamp: String },

    /// Signature verification failed.
    #[error("signature verification failed")]
    SignatureInvalid,
}

/// Result type alias for SINP operations.
pub type SinpResult<T> = Result<T, SinpError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_code_display() {
        assert_eq!(RefusalCode::MalformedContext.to_string(), "malformed_context");
        assert_eq!(RefusalCode::PrivacyViolation.to_string(), "privacy_violation");
        assert_eq!(RefusalCode::CapabilityMissing.to_string(), "capability_missing");
        assert_eq!(RefusalCode::PolicyViolation.to_string(), "policy_violation");
    }

    #[test]
    fn refusal_code_serde() {
        let code = RefusalCode::PolicyViolation;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"policy_violation\"");

        let parsed: RefusalCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, code);
    }
}
