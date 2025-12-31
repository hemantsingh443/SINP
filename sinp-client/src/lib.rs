//! SINP Client SDK - Semantic Intent Negotiation Protocol client library.
//!
//! # Example
//!
//! ```no_run
//! use sinp_client::SinpClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut client = SinpClient::connect("127.0.0.1:9000").await?;
//!     
//!     let response = client.send_intent("What's the weather?", 0.85).await?;
//!     println!("Response: {:?}", response);
//!     
//!     Ok(())
//! }
//! ```

mod connection;
mod state_machine;

pub use connection::{Connection, ConnectionConfig};
pub use state_machine::{ClientStateMachine, NextAction};

use std::net::SocketAddr;

use sinp_core::{
    message::{AuthMethod, Context, ContextType, Sender},
    security::semantic_hash,
    Action, Alternative, Request, SinpResult,
};

/// High-level SINP client.
pub struct SinpClient {
    connection: Connection,
    state_machine: ClientStateMachine,
    sender: Sender,
    context_history: Vec<String>,
}

impl SinpClient {
    /// Connect to a SINP server (plaintext).
    pub async fn connect(addr: impl AsRef<str>) -> SinpResult<Self> {
        let addr: SocketAddr = addr
            .as_ref()
            .parse()
            .map_err(|e| sinp_core::SinpError::Transport(format!("Invalid address: {}", e)))?;

        let config = ConnectionConfig::plaintext(addr);
        let connection = Connection::connect(&config).await?;

        Ok(Self {
            connection,
            state_machine: ClientStateMachine::new(),
            sender: Sender {
                id: format!("client_{}", uuid::Uuid::new_v4()),
                auth_method: AuthMethod::None,
            },
            context_history: Vec::new(),
        })
    }

    /// Connect to a SINP server with TLS.
    pub async fn connect_tls(
        addr: impl AsRef<str>,
        server_name: impl Into<String>,
    ) -> SinpResult<Self> {
        let addr: SocketAddr = addr
            .as_ref()
            .parse()
            .map_err(|e| sinp_core::SinpError::Transport(format!("Invalid address: {}", e)))?;

        let config = ConnectionConfig::tls(addr, server_name);
        let connection = Connection::connect(&config).await?;

        Ok(Self {
            connection,
            state_machine: ClientStateMachine::new(),
            sender: Sender {
                id: format!("client_{}", uuid::Uuid::new_v4()),
                auth_method: AuthMethod::Certificate,
            },
            context_history: Vec::new(),
        })
    }

    /// Set client identity.
    pub fn with_sender(mut self, sender: Sender) -> Self {
        self.sender = sender;
        self
    }

    /// Get current state.
    pub fn state(&self) -> sinp_core::ClientState {
        self.state_machine.state()
    }

    /// Send an intent to the server.
    pub async fn send_intent(
        &mut self,
        intent: impl Into<String>,
        confidence: f64,
    ) -> SinpResult<NextAction> {
        let intent = intent.into();
        self.context_history.push(format!("User: {}", intent));

        let context = self.build_context();
        let request = Request::new(self.sender.clone(), &intent, confidence, context);

        self.state_machine.on_request_sent(&request)?;
        let response = self.connection.send_request(&request).await?;

        self.context_history
            .push(format!("Server: {}", response.interpretation.text));

        self.state_machine.on_response_received(response)
    }

    /// Respond to a CLARIFY action with answers.
    pub async fn respond_to_clarify(
        &mut self,
        answers: impl Into<String>,
        confidence: f64,
    ) -> SinpResult<NextAction> {
        let answers = answers.into();
        self.context_history.push(format!("User: {}", answers));

        let context = self.build_context();
        let last_response = self
            .state_machine
            .last_response()
            .ok_or_else(|| sinp_core::SinpError::Protocol("No previous response".to_string()))?
            .clone();

        let request = Request::reply(&last_response, self.sender.clone(), &answers, confidence, context);

        self.state_machine.on_clarification_provided()?;
        self.state_machine.on_request_sent(&request)?;
        let response = self.connection.send_request(&request).await?;

        self.context_history
            .push(format!("Server: {}", response.interpretation.text));

        self.state_machine.on_response_received(response)
    }

    /// Accept a proposal.
    pub async fn accept_proposal(
        &mut self,
        alternative: &Alternative,
        confidence: f64,
    ) -> SinpResult<NextAction> {
        let intent = format!("Accept: {}", alternative.interpretation);
        self.context_history.push(format!("User: {}", intent));

        let context = self.build_context();
        let last_response = self
            .state_machine
            .last_response()
            .ok_or_else(|| sinp_core::SinpError::Protocol("No previous response".to_string()))?
            .clone();

        let request = Request::reply(&last_response, self.sender.clone(), &intent, confidence, context);

        self.state_machine.on_proposal_accepted()?;
        self.state_machine.on_request_sent(&request)?;
        let response = self.connection.send_request(&request).await?;

        self.context_history
            .push(format!("Server: {}", response.interpretation.text));

        self.state_machine.on_response_received(response)
    }

    /// Reject proposal and send new intent.
    pub async fn reject_proposal(
        &mut self,
        new_intent: impl Into<String>,
        confidence: f64,
    ) -> SinpResult<NextAction> {
        let new_intent = new_intent.into();
        self.context_history
            .push(format!("User (rejected proposal): {}", new_intent));

        let context = self.build_context();
        let last_response = self
            .state_machine
            .last_response()
            .ok_or_else(|| sinp_core::SinpError::Protocol("No previous response".to_string()))?
            .clone();

        let request = Request::reply(&last_response, self.sender.clone(), &new_intent, confidence, context);

        self.state_machine.on_proposal_rejected()?;
        self.state_machine.on_request_sent(&request)?;
        let response = self.connection.send_request(&request).await?;

        self.context_history
            .push(format!("Server: {}", response.interpretation.text));

        self.state_machine.on_response_received(response)
    }

    /// Get the result from an EXECUTE response.
    pub fn get_result(&self) -> Option<serde_json::Value> {
        self.state_machine
            .last_response()
            .filter(|r| r.action == Action::Execute)
            .and_then(|r| r.action_metadata.as_ref())
            .and_then(|m| m.result.clone())
    }

    /// Reset client for new conversation.
    pub fn reset(&mut self) {
        self.state_machine.reset();
        self.context_history.clear();
    }

    /// Build context from history.
    fn build_context(&self) -> Context {
        let content = self.context_history.join("\n");
        let hash = semantic_hash("", &Context {
            context_type: ContextType::Transcript,
            content: content.clone(),
            semantic_hash: String::new(),
        });

        Context {
            context_type: ContextType::Transcript,
            content,
            semantic_hash: hash,
        }
    }
}
