//! Message types for SINP protocol.
//!
//! Defines the core message tuple M = (ID, CID, T, Sender, Ψ, Γ, Φ, Σ)
//! as well as Request and Response schemas per RFC 0.1.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::RefusalCode;

/// Authentication method for sender identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Token,
    Certificate,
    ApiKey,
    None,
}

/// Sender identity object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sender {
    pub id: String,
    pub auth_method: AuthMethod,
}

/// Context type (Γ).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    Transcript,
    Summary,
    Structured,
}

/// Context object containing conversation history/state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    #[serde(rename = "type")]
    pub context_type: ContextType,
    pub content: String,
    pub semantic_hash: String,
}

/// Client-specified constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Constraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// Server capability definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub description: String,
    pub inputs: Vec<String>,
    pub privacy_level: String,
    pub cost_units: f64,
}

/// Server's interpretation of client intent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interpretation {
    pub text: String,
    pub confidence: f64,
}

/// Action types the server can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Action {
    Execute,
    Clarify,
    Propose,
    Refuse,
}

/// Metadata for action responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ActionMetadata {
    /// Result data if action is EXECUTE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,

    /// Clarifying questions if action is CLARIFY.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub questions: Option<Vec<String>>,

    /// Reason code if action is REFUSE.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<RefusalCode>,

    /// Human-readable reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Alternative action proposal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alternative {
    pub interpretation: String,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    pub capability_id: String,
}

/// Responder identity (server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Responder {
    pub id: String,
    pub capabilities: Vec<String>,
}

/// Base message fields common to requests and responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub message_id: Uuid,
    pub conversation_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

/// Client request message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol_version: String,
    pub message_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_response_to: Option<Uuid>,
    pub conversation_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sender: Sender,
    pub intent: String,
    pub confidence: f64,
    pub context: Context,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<Constraints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl Request {
    /// Create a new initial request (no in_response_to).
    pub fn new(
        sender: Sender,
        intent: impl Into<String>,
        confidence: f64,
        context: Context,
    ) -> Self {
        Self {
            protocol_version: crate::PROTOCOL_VERSION.to_string(),
            message_id: Uuid::new_v4(),
            in_response_to: None,
            conversation_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender,
            intent: intent.into(),
            confidence,
            context,
            constraints: None,
            signature: None,
        }
    }

    /// Create a follow-up request responding to a previous message.
    pub fn reply(
        previous: &Response,
        sender: Sender,
        intent: impl Into<String>,
        confidence: f64,
        context: Context,
    ) -> Self {
        Self {
            protocol_version: crate::PROTOCOL_VERSION.to_string(),
            message_id: Uuid::new_v4(),
            in_response_to: Some(previous.message_id),
            conversation_id: previous.conversation_id,
            timestamp: Utc::now(),
            sender,
            intent: intent.into(),
            confidence,
            context,
            constraints: None,
            signature: None,
        }
    }
}

/// Server response message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub message_id: Uuid,
    pub in_response_to: Uuid,
    pub conversation_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub responder: Responder,
    pub interpretation: Interpretation,
    pub action: Action,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_metadata: Option<ActionMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<Vec<Alternative>>,
    pub confidence: f64,
}

impl Response {
    /// Create a response to a request.
    pub fn to_request(
        request: &Request,
        responder: Responder,
        interpretation: Interpretation,
        action: Action,
        confidence: f64,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            in_response_to: request.message_id,
            conversation_id: request.conversation_id,
            timestamp: Utc::now(),
            responder,
            interpretation,
            action,
            action_metadata: None,
            alternatives: None,
            confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sender() -> Sender {
        Sender {
            id: "client_1".to_string(),
            auth_method: AuthMethod::Token,
        }
    }

    fn sample_context() -> Context {
        Context {
            context_type: ContextType::Transcript,
            content: "User asked for weather".to_string(),
            semantic_hash: "abc123".to_string(),
        }
    }

    #[test]
    fn request_serialization() {
        let req = Request::new(sample_sender(), "Get the weather", 0.85, sample_context());

        let json = serde_json::to_string_pretty(&req).unwrap();
        assert!(json.contains("\"protocol_version\": \"0.1\""));
        assert!(json.contains("\"intent\": \"Get the weather\""));

        let parsed: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.intent, req.intent);
        assert_eq!(parsed.confidence, req.confidence);
    }

    #[test]
    fn action_serialization() {
        assert_eq!(serde_json::to_string(&Action::Execute).unwrap(), "\"EXECUTE\"");
        assert_eq!(serde_json::to_string(&Action::Clarify).unwrap(), "\"CLARIFY\"");
        assert_eq!(serde_json::to_string(&Action::Propose).unwrap(), "\"PROPOSE\"");
        assert_eq!(serde_json::to_string(&Action::Refuse).unwrap(), "\"REFUSE\"");
    }

    #[test]
    fn response_creation() {
        let req = Request::new(sample_sender(), "Book a flight", 0.9, sample_context());
        let responder = Responder {
            id: "srv_1".to_string(),
            capabilities: vec!["flight_booking:v1".to_string()],
        };
        let interpretation = Interpretation {
            text: "Booking a flight to destination".to_string(),
            confidence: 0.92,
        };

        let resp = Response::to_request(&req, responder, interpretation, Action::Execute, 0.90);

        assert_eq!(resp.in_response_to, req.message_id);
        assert_eq!(resp.conversation_id, req.conversation_id);
        assert_eq!(resp.action, Action::Execute);
    }
}
