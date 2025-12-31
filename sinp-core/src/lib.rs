//! # sinp-core
//!
//! Core library for the Semantic Intent Negotiation Protocol (SINP).
//!
//! This crate provides the fundamental types, confidence computation,
//! decision logic, security primitives, and state machine definitions
//! for implementing SINP clients and servers.

pub mod confidence;
pub mod error;
pub mod interpreter;
pub mod message;
pub mod security;
pub mod state;

pub use confidence::{compute_server_confidence, decide_action, Thresholds};
pub use error::{RefusalCode, SinpError, SinpResult};
pub use message::{
    Action, ActionMetadata, Alternative, Capability, Constraints, Context, ContextType,
    Interpretation, Message, Request, Responder, Response, Sender,
};
pub use security::{check_replay, semantic_hash, sign_message, verify_signature};
pub use state::{ClientEvent, ClientState, ServerEvent, ServerState};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "0.1";
