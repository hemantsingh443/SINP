//! Client state machine for SINP protocol.

use sinp_core::{
    Action, ClientEvent, ClientState, Request, Response, SinpError, SinpResult,
};

/// Client state machine managing conversation flow.
pub struct ClientStateMachine {
    state: ClientState,
    conversation_id: Option<uuid::Uuid>,
    last_response: Option<Response>,
}

impl ClientStateMachine {
    /// Create a new client state machine.
    pub fn new() -> Self {
        Self {
            state: ClientState::Init,
            conversation_id: None,
            last_response: None,
        }
    }

    /// Get current state.
    pub fn state(&self) -> ClientState {
        self.state
    }

    /// Get conversation ID.
    pub fn conversation_id(&self) -> Option<uuid::Uuid> {
        self.conversation_id
    }

    /// Get last response.
    pub fn last_response(&self) -> Option<&Response> {
        self.last_response.as_ref()
    }

    /// Handle sending a request.
    pub fn on_request_sent(&mut self, request: &Request) -> SinpResult<()> {
        if self.state == ClientState::Init {
            self.conversation_id = Some(request.conversation_id);
        }
        self.transition(ClientEvent::RequestSent)
    }

    /// Handle receiving a response.
    pub fn on_response_received(&mut self, response: Response) -> SinpResult<NextAction> {
        self.last_response = Some(response.clone());

        let next = match response.action {
            Action::Execute => {
                self.transition(ClientEvent::ResponseExecute)?;
                NextAction::Done(response)
            }
            Action::Clarify => {
                self.transition(ClientEvent::ResponseClarify)?;
                let questions = response
                    .action_metadata
                    .as_ref()
                    .and_then(|m| m.questions.clone())
                    .unwrap_or_default();
                NextAction::Clarify { questions, response }
            }
            Action::Propose => {
                self.transition(ClientEvent::ResponsePropose)?;
                let alternatives = response.alternatives.clone().unwrap_or_default();
                NextAction::Propose {
                    alternatives,
                    response,
                }
            }
            Action::Refuse => {
                self.transition(ClientEvent::ResponseRefuse)?;
                let reason = response
                    .action_metadata
                    .as_ref()
                    .and_then(|m| m.reason.clone())
                    .unwrap_or_else(|| "Request refused".to_string());
                NextAction::Refused { reason, response }
            }
        };

        Ok(next)
    }

    /// User provided clarification.
    pub fn on_clarification_provided(&mut self) -> SinpResult<()> {
        self.transition(ClientEvent::ClarificationProvided)
    }

    /// User accepted proposal.
    pub fn on_proposal_accepted(&mut self) -> SinpResult<()> {
        self.transition(ClientEvent::ProposalAccepted)
    }

    /// User rejected proposal.
    pub fn on_proposal_rejected(&mut self) -> SinpResult<()> {
        self.transition(ClientEvent::ProposalRejected)
    }

    /// User abandoned conversation.
    pub fn abandon(&mut self) -> SinpResult<()> {
        self.transition(ClientEvent::Abandoned)
    }

    /// Reset for new conversation.
    pub fn reset(&mut self) {
        self.state = ClientState::Init;
        self.conversation_id = None;
        self.last_response = None;
    }

    /// Transition to new state.
    fn transition(&mut self, event: ClientEvent) -> SinpResult<()> {
        let new_state = match (&self.state, &event) {
            (ClientState::Init, ClientEvent::RequestSent) => ClientState::Pending,
            (ClientState::Pending, ClientEvent::ResponseExecute) => ClientState::Satisfied,
            (ClientState::Pending, ClientEvent::ResponseClarify) => ClientState::Refining,
            (ClientState::Pending, ClientEvent::ResponsePropose) => ClientState::Refining,
            (ClientState::Pending, ClientEvent::ResponseRefuse) => ClientState::Failed,
            (ClientState::Refining, ClientEvent::ClarificationProvided) => ClientState::Pending,
            (ClientState::Refining, ClientEvent::ProposalAccepted) => ClientState::Pending,
            (ClientState::Refining, ClientEvent::ProposalRejected) => ClientState::Pending,
            (ClientState::Refining, ClientEvent::Abandoned) => ClientState::Abandoned,
            (ClientState::Refining, ClientEvent::RequestSent) => ClientState::Pending,
            (_, ClientEvent::Error(_)) => ClientState::Failed,
            _ => {
                return Err(SinpError::Protocol(format!(
                    "Invalid transition from {:?} on {:?}",
                    self.state, event
                )));
            }
        };

        if self.state.can_transition_to(new_state) {
            tracing::debug!("Client state: {:?} -> {:?}", self.state, new_state);
            self.state = new_state;
            Ok(())
        } else {
            Err(SinpError::Protocol(format!(
                "Invalid state transition: {:?} -> {:?}",
                self.state, new_state
            )))
        }
    }
}

impl Default for ClientStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// What the client should do next after receiving a response.
#[derive(Debug)]
pub enum NextAction {
    /// Intent satisfied, contains result.
    Done(Response),
    /// Server needs clarification.
    Clarify {
        questions: Vec<String>,
        response: Response,
    },
    /// Server proposes alternatives.
    Propose {
        alternatives: Vec<sinp_core::Alternative>,
        response: Response,
    },
    /// Request was refused.
    Refused { reason: String, response: Response },
}

#[cfg(test)]
mod tests {
    use super::*;
    use sinp_core::{
        message::{AuthMethod, ContextType, Interpretation, Responder, Sender},
        Context,
    };

    fn sample_request() -> Request {
        Request::new(
            Sender {
                id: "test".to_string(),
                auth_method: AuthMethod::Token,
            },
            "test intent",
            0.9,
            Context {
                context_type: ContextType::Transcript,
                content: "test".to_string(),
                semantic_hash: "hash".to_string(),
            },
        )
    }

    fn sample_response(action: Action) -> Response {
        Response {
            message_id: uuid::Uuid::new_v4(),
            in_response_to: uuid::Uuid::new_v4(),
            conversation_id: uuid::Uuid::new_v4(),
            timestamp: chrono::Utc::now(),
            responder: Responder {
                id: "srv".to_string(),
                capabilities: vec![],
            },
            interpretation: Interpretation {
                text: "test".to_string(),
                confidence: 0.9,
            },
            action,
            action_metadata: None,
            alternatives: None,
            confidence: 0.9,
        }
    }

    #[test]
    fn execute_flow() {
        let mut sm = ClientStateMachine::new();
        assert_eq!(sm.state(), ClientState::Init);

        let req = sample_request();
        sm.on_request_sent(&req).unwrap();
        assert_eq!(sm.state(), ClientState::Pending);

        let resp = sample_response(Action::Execute);
        let next = sm.on_response_received(resp).unwrap();
        assert!(matches!(next, NextAction::Done(_)));
        assert_eq!(sm.state(), ClientState::Satisfied);
    }

    #[test]
    fn clarify_flow() {
        let mut sm = ClientStateMachine::new();
        let req = sample_request();
        sm.on_request_sent(&req).unwrap();

        let resp = sample_response(Action::Clarify);
        let next = sm.on_response_received(resp).unwrap();
        assert!(matches!(next, NextAction::Clarify { .. }));
        assert_eq!(sm.state(), ClientState::Refining);

        sm.on_clarification_provided().unwrap();
        assert_eq!(sm.state(), ClientState::Pending);
    }
}
