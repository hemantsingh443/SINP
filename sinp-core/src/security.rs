//! Security primitives for SINP.
//!
//! Implements:
//! - Semantic hashing (H_sem = SHA256(normalize(Ψ) || normalize(Γ)))
//! - JCS canonicalization (RFC 8785)
//! - Ed25519 signatures
//! - Replay protection

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use base64::Engine;

use crate::error::{SinpError, SinpResult};
use crate::message::{Context, Request};

/// Default replay window in milliseconds.
pub const DEFAULT_REPLAY_WINDOW_MS: i64 = 5000;

/// Normalize a string for hashing (lowercase, trim, collapse whitespace).
fn normalize(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compute semantic hash for caching.
///
/// H_sem = SHA256(normalize(Ψ) || normalize(Γ))
///
/// Note: Excludes timestamps to ensure identical intents hit the cache.
pub fn semantic_hash(intent: &str, context: &Context) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(intent).as_bytes());
    hasher.update(b"||");
    hasher.update(normalize(&context.content).as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Validate semantic hash matches expected.
pub fn validate_semantic_hash(intent: &str, context: &Context) -> bool {
    let computed = semantic_hash(intent, context);
    computed == context.semantic_hash
}

/// Check for replay attack.
///
/// Rejects messages where |T_now - T_sender| > window_ms (default 5000ms).
pub fn check_replay(
    message_timestamp: DateTime<Utc>,
    window_ms: Option<i64>,
) -> SinpResult<()> {
    let window = window_ms.unwrap_or(DEFAULT_REPLAY_WINDOW_MS);
    let now = Utc::now();
    let diff = now.signed_duration_since(message_timestamp);

    if diff.abs() > Duration::milliseconds(window) {
        return Err(SinpError::ReplayDetected {
            timestamp: message_timestamp.to_rfc3339(),
        });
    }

    Ok(())
}

/// JCS (RFC 8785) JSON Canonicalization.
///
/// For signature computation, we need deterministic JSON serialization:
/// 1. Object keys sorted lexicographically
/// 2. No whitespace
/// 3. Numbers in shortest form
/// 4. Strings escaped per RFC 8785
pub fn canonicalize_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            // RFC 8785: use shortest representation
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(f) = n.as_f64() {
                // Remove trailing zeros but keep at least one decimal
                let s = format!("{}", f);
                s
            } else {
                n.to_string()
            }
        }
        serde_json::Value::String(s) => {
            // Escape special characters per RFC 8785
            serde_json::to_string(s).unwrap()
        }
        serde_json::Value::Array(arr) => {
            let elements: Vec<String> = arr.iter().map(canonicalize_json).collect();
            format!("[{}]", elements.join(","))
        }
        serde_json::Value::Object(obj) => {
            // Sort keys lexicographically
            let mut keys: Vec<_> = obj.keys().collect();
            keys.sort();
            let pairs: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonicalize_json(&obj[*k])))
                .collect();
            format!("{{{}}}", pairs.join(","))
        }
    }
}

/// Sign a request message.
///
/// 1. Serialize request to JSON
/// 2. Remove signature field
/// 3. Canonicalize using JCS
/// 4. Sign with Ed25519
pub fn sign_message(request: &Request, signing_key: &SigningKey) -> SinpResult<String> {
    let mut value = serde_json::to_value(request)?;

    // Remove signature field before canonicalization
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature");
    }

    let canonical = canonicalize_json(&value);
    let signature: Signature = signing_key.sign(canonical.as_bytes());

    Ok(base64::engine::general_purpose::STANDARD.encode(signature.to_bytes()))
}

/// Verify a request signature.
pub fn verify_signature(
    request: &Request,
    verifying_key: &VerifyingKey,
) -> SinpResult<()> {
    let signature_b64 = request
        .signature
        .as_ref()
        .ok_or_else(|| SinpError::Crypto("No signature present".to_string()))?;

    let signature_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| SinpError::Crypto(format!("Invalid base64: {}", e)))?;

    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|e| SinpError::Crypto(format!("Invalid signature format: {}", e)))?;

    let mut value = serde_json::to_value(request)?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.remove("signature");
    }

    let canonical = canonicalize_json(&value);

    verifying_key
        .verify(canonical.as_bytes(), &signature)
        .map_err(|_| SinpError::SignatureInvalid)
}

// Re-export hex for convenience
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AuthMethod, ContextType, Sender};

    #[test]
    fn semantic_hash_deterministic() {
        let ctx = Context {
            context_type: ContextType::Transcript,
            content: "Hello world".to_string(),
            semantic_hash: String::new(),
        };

        let h1 = semantic_hash("Get weather", &ctx);
        let h2 = semantic_hash("get weather", &ctx);
        let h3 = semantic_hash("  GET   WEATHER  ", &ctx);

        // Normalized versions should match
        assert_eq!(h1, h2);
        assert_eq!(h2, h3);
    }

    #[test]
    fn replay_check_valid() {
        let now = Utc::now();
        assert!(check_replay(now, None).is_ok());
    }

    #[test]
    fn replay_check_expired() {
        let old = Utc::now() - Duration::seconds(10);
        assert!(check_replay(old, None).is_err());
    }

    #[test]
    fn jcs_canonicalization() {
        let json: serde_json::Value = serde_json::json!({
            "z": 1,
            "a": "hello",
            "m": [3, 1, 2]
        });

        let canonical = canonicalize_json(&json);
        // Keys should be sorted: a, m, z
        assert!(canonical.starts_with("{\"a\":"));
        assert!(canonical.contains("\"m\":[3,1,2]"));
        assert!(canonical.ends_with("\"z\":1}"));
    }

    #[test]
    fn sign_and_verify() {
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let ctx = Context {
            context_type: ContextType::Transcript,
            content: "test".to_string(),
            semantic_hash: "abc".to_string(),
        };
        let sender = Sender {
            id: "test".to_string(),
            auth_method: AuthMethod::Token,
        };
        let mut request = Request::new(sender, "Hello", 0.9, ctx);

        // Sign
        let sig = sign_message(&request, &signing_key).unwrap();
        request.signature = Some(sig);

        // Verify
        assert!(verify_signature(&request, &verifying_key).is_ok());
    }
}
