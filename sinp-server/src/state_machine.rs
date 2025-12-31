//! Server state machine implementation.

use sinp_core::{
    check_replay, compute_server_confidence, decide_action,
    Action, ActionMetadata, Interpretation, RefusalCode, Request, Responder, Response,
    ServerEvent, ServerState, SinpError, SinpResult,
};

use crate::config::ServerConfig;
use crate::capability::CapabilityRegistry;

/// Server state machine managing a single conversation.
pub struct ServerStateMachine {
    state: ServerState,
    config: ServerConfig,
    conversation_id: Option<uuid::Uuid>,
    last_message_id: Option<uuid::Uuid>,
}

impl ServerStateMachine {
    /// Create a new state machine.
    pub fn new(config: ServerConfig) -> Self {
        Self {
            state: ServerState::Received,
            config,
            conversation_id: None,
            last_message_id: None,
        }
    }

    /// Get current state.
    pub fn state(&self) -> ServerState {
        self.state
    }

    /// Process an incoming request.
    pub fn process_request(
        &mut self,
        request: &Request,
        registry: &CapabilityRegistry,
    ) -> SinpResult<Response> {
        // Transition: Received -> Validating
        self.transition(ServerEvent::RequestReceived)?;

        // Validate replay protection
        if let Err(e) = check_replay(request.timestamp, Some(self.config.replay_window_ms)) {
            self.transition(ServerEvent::ValidationFailed(e.to_string()))?;
            return Err(e);
        }

        // Validate conversation continuity
        if let Some(cid) = self.conversation_id {
            if request.conversation_id != cid {
                let err = SinpError::Validation("Conversation ID mismatch".to_string());
                self.transition(ServerEvent::ValidationFailed(err.to_string()))?;
                return Err(err);
            }
        } else {
            self.conversation_id = Some(request.conversation_id);
        }

        // Validate in_response_to for follow-up messages
        if self.last_message_id.is_some() && request.in_response_to.is_none() {
            let err = SinpError::Validation("Missing in_response_to for follow-up".to_string());
            self.transition(ServerEvent::ValidationFailed(err.to_string()))?;
            return Err(err);
        }

        // Transition: Validating -> Interpreting
        self.transition(ServerEvent::ValidationPassed)?;

        // Interpret the request
        let interpretation_result = registry.interpret(&request.intent, &request.context);

        // Transition: Interpreting -> Deciding
        self.transition(ServerEvent::InterpretationComplete {
            confidence: interpretation_result.raw_confidence,
        })?;

        // Compute server confidence
        let (phi_s, policy_passed) = if let Some(ref cap) = interpretation_result.capability {
            let reliability = registry.get_reliability(&cap.id);
            let availability = 1.0; // TODO: Resource availability check
            let policy = registry.check_policy(&request);
            let conf = compute_server_confidence(
                interpretation_result.raw_confidence,
                reliability,
                availability,
                policy,
            );
            (conf, policy)
        } else {
            (0.0, true)
        };

        // Decide action
        let has_alternatives = !interpretation_result.alternatives.is_empty();
        let action = decide_action(
            phi_s,
            request.confidence,
            &self.config.thresholds,
            has_alternatives && phi_s < self.config.thresholds.tau_exec,
            !policy_passed,
            false,
        );

        // Build response
        let responder = Responder {
            id: "sinp-server".to_string(),
            capabilities: registry.capability_ids(),
        };

        let interpretation = Interpretation {
            text: interpretation_result.interpretation.clone(),
            confidence: phi_s,
        };

        let mut response = Response::to_request(request, responder, interpretation, action, phi_s);

        // Add action metadata
        response.action_metadata = Some(match action {
            Action::Execute => {
                // Execute the capability
                self.transition(ServerEvent::DecisionExecute)?;
                let result = if let Some(ref cap) = interpretation_result.capability {
                    registry.execute(&cap.id, request)?
                } else {
                    serde_json::Value::Null
                };
                // State is already Done after DecisionExecute
                ActionMetadata {
                    result: Some(result),
                    ..Default::default()
                }
            }
            Action::Clarify => {
                self.transition(ServerEvent::DecisionClarify)?;
                ActionMetadata {
                    questions: Some(vec![
                        "Could you provide more details?".to_string(),
                        "What specific action would you like?".to_string(),
                    ]),
                    ..Default::default()
                }
            }
            Action::Propose => {
                self.transition(ServerEvent::DecisionPropose)?;
                ActionMetadata::default()
            }
            Action::Refuse => {
                self.transition(ServerEvent::DecisionRefuse)?;
                let code = if !policy_passed {
                    RefusalCode::PolicyViolation
                } else if interpretation_result.capability.is_none() {
                    RefusalCode::CapabilityMissing
                } else {
                    RefusalCode::MalformedContext
                };
                ActionMetadata {
                    reason_code: Some(code),
                    reason: Some(format!("Request refused: {}", code)),
                    ..Default::default()
                }
            }
        });

        // Add alternatives for PROPOSE
        if action == Action::Propose {
            response.alternatives = Some(
                interpretation_result
                    .alternatives
                    .into_iter()
                    .map(|alt| sinp_core::Alternative {
                        interpretation: alt.interpretation,
                        confidence: alt.confidence,
                        estimated_cost: Some(alt.capability.cost_units),
                        capability_id: alt.capability.id,
                    })
                    .collect(),
            );
        }

        self.last_message_id = Some(response.message_id);
        Ok(response)
    }

    /// Transition to a new state based on event.
    fn transition(&mut self, event: ServerEvent) -> SinpResult<()> {
        let new_state = match (&self.state, &event) {
            (ServerState::Received, ServerEvent::RequestReceived) => ServerState::Validating,
            (ServerState::Validating, ServerEvent::ValidationPassed) => ServerState::Interpreting,
            (ServerState::Validating, ServerEvent::ValidationFailed(_)) => ServerState::Failed,
            (ServerState::Interpreting, ServerEvent::InterpretationComplete { .. }) => {
                ServerState::Deciding
            }
            (ServerState::Deciding, ServerEvent::DecisionExecute) => ServerState::Done,
            (ServerState::Deciding, ServerEvent::DecisionClarify) => ServerState::Negotiating,
            (ServerState::Deciding, ServerEvent::DecisionPropose) => ServerState::Negotiating,
            (ServerState::Deciding, ServerEvent::DecisionRefuse) => ServerState::Done,
            (ServerState::Done, ServerEvent::ActionCompleted) => ServerState::Done,
            (ServerState::Negotiating, ServerEvent::ClientResponded) => ServerState::Received,
            (_, ServerEvent::Error(msg)) => {
                tracing::error!("State machine error: {}", msg);
                ServerState::Failed
            }
            _ => {
                return Err(SinpError::Protocol(format!(
                    "Invalid transition from {:?} on {:?}",
                    self.state, event
                )));
            }
        };

        if self.state.can_transition_to(new_state) {
            tracing::debug!("State transition: {:?} -> {:?}", self.state, new_state);
            self.state = new_state;
            Ok(())
        } else {
            Err(SinpError::Protocol(format!(
                "Invalid state transition: {:?} -> {:?}",
                self.state, new_state
            )))
        }
    }

    /// Reset state machine for new conversation.
    pub fn reset(&mut self) {
        self.state = ServerState::Received;
        self.conversation_id = None;
        self.last_message_id = None;
    }
}
