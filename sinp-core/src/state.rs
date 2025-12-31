//! State machine definitions for SINP protocol.
//!
//! Defines the server and client state automata as per RFC Section 5.

use serde::{Deserialize, Serialize};

/// Server state automaton states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerState {
    /// Initial state - message received, pending validation.
    Received,
    /// Validating signature, schema, and replay protection.
    Validating,
    /// Running interpretation function f(Ψ, Γ).
    Interpreting,
    /// Applying decision logic δ(Φ_s, Φ_c).
    Deciding,
    /// Awaiting client response to CLARIFY or PROPOSE.
    Negotiating,
    /// Terminal state - action completed.
    Done,
    /// Error state - unrecoverable failure.
    Failed,
}

impl ServerState {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }

    /// Get valid transitions from current state.
    pub fn valid_transitions(&self) -> &'static [ServerState] {
        match self {
            Self::Received => &[Self::Validating, Self::Failed],
            Self::Validating => &[Self::Interpreting, Self::Failed],
            Self::Interpreting => &[Self::Deciding, Self::Failed],
            Self::Deciding => &[Self::Done, Self::Negotiating, Self::Failed],
            Self::Negotiating => &[Self::Received, Self::Done, Self::Failed],
            Self::Done => &[],
            Self::Failed => &[],
        }
    }

    /// Check if transition to target state is valid.
    pub fn can_transition_to(&self, target: ServerState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

/// Client state automaton states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientState {
    /// Initial state - preparing first request.
    Init,
    /// Request sent, awaiting server response.
    Pending,
    /// Processing server response (CLARIFY/PROPOSE).
    Refining,
    /// Terminal state - EXECUTE received, intent satisfied.
    Satisfied,
    /// Terminal state - conversation ended without satisfaction.
    Abandoned,
    /// Error state.
    Failed,
}

impl ClientState {
    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Satisfied | Self::Abandoned | Self::Failed)
    }

    /// Get valid transitions from current state.
    pub fn valid_transitions(&self) -> &'static [ClientState] {
        match self {
            Self::Init => &[Self::Pending, Self::Failed],
            Self::Pending => &[Self::Refining, Self::Satisfied, Self::Failed],
            Self::Refining => &[Self::Pending, Self::Abandoned, Self::Failed],
            Self::Satisfied => &[],
            Self::Abandoned => &[],
            Self::Failed => &[],
        }
    }

    /// Check if transition to target state is valid.
    pub fn can_transition_to(&self, target: ClientState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

/// Events that drive server state transitions.
#[derive(Debug, Clone)]
pub enum ServerEvent {
    /// New request received.
    RequestReceived,
    /// Validation passed.
    ValidationPassed,
    /// Validation failed.
    ValidationFailed(String),
    /// Interpretation complete.
    InterpretationComplete { confidence: f64 },
    /// Decision made: EXECUTE.
    DecisionExecute,
    /// Decision made: CLARIFY.
    DecisionClarify,
    /// Decision made: PROPOSE.
    DecisionPropose,
    /// Decision made: REFUSE.
    DecisionRefuse,
    /// Client responded to negotiation.
    ClientResponded,
    /// Action completed successfully.
    ActionCompleted,
    /// Error occurred.
    Error(String),
}

/// Events that drive client state transitions.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// User submitted intent.
    IntentSubmitted,
    /// Request sent to server.
    RequestSent,
    /// Server responded with EXECUTE.
    ResponseExecute,
    /// Server responded with CLARIFY.
    ResponseClarify,
    /// Server responded with PROPOSE.
    ResponsePropose,
    /// Server responded with REFUSE.
    ResponseRefuse,
    /// User provided clarification.
    ClarificationProvided,
    /// User accepted proposal.
    ProposalAccepted,
    /// User rejected proposal.
    ProposalRejected,
    /// User abandoned conversation.
    Abandoned,
    /// Error occurred.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_state_transitions() {
        let state = ServerState::Received;
        assert!(state.can_transition_to(ServerState::Validating));
        assert!(state.can_transition_to(ServerState::Failed));
        assert!(!state.can_transition_to(ServerState::Done));
    }

    #[test]
    fn server_terminal_states() {
        assert!(ServerState::Done.is_terminal());
        assert!(ServerState::Failed.is_terminal());
        assert!(!ServerState::Received.is_terminal());
    }

    #[test]
    fn client_state_transitions() {
        let state = ClientState::Init;
        assert!(state.can_transition_to(ClientState::Pending));
        assert!(!state.can_transition_to(ClientState::Satisfied));

        let refining = ClientState::Refining;
        assert!(refining.can_transition_to(ClientState::Pending));
        assert!(refining.can_transition_to(ClientState::Abandoned));
    }

    #[test]
    fn client_terminal_states() {
        assert!(ClientState::Satisfied.is_terminal());
        assert!(ClientState::Abandoned.is_terminal());
        assert!(!ClientState::Pending.is_terminal());
    }
}
