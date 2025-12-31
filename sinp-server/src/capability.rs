//! Capability registry for SINP server.

use std::collections::HashMap;
use sinp_core::{
    Capability, Context, Request, SinpResult,
    interpreter::{InterpretationResult, Interpreter, KeywordInterpreter},
};

/// Handler function type for capability execution.
pub type CapabilityHandler = Box<dyn Fn(&Request) -> SinpResult<serde_json::Value> + Send + Sync>;

/// Registry of server capabilities.
pub struct CapabilityRegistry {
    capabilities: HashMap<String, RegisteredCapability>,
    interpreter: Box<dyn Interpreter>,
}

struct RegisteredCapability {
    capability: Capability,
    handler: CapabilityHandler,
    reliability: f64,
}

impl CapabilityRegistry {
    /// Create a new empty registry with keyword interpreter.
    pub fn new() -> Self {
        Self {
            capabilities: HashMap::new(),
            interpreter: Box::new(KeywordInterpreter::default()),
        }
    }

    /// Create with custom interpreter.
    pub fn with_interpreter(interpreter: Box<dyn Interpreter>) -> Self {
        Self {
            capabilities: HashMap::new(),
            interpreter,
        }
    }

    /// Register a capability with handler.
    pub fn register<F>(&mut self, capability: Capability, handler: F, reliability: f64)
    where
        F: Fn(&Request) -> SinpResult<serde_json::Value> + Send + Sync + 'static,
    {
        self.capabilities.insert(
            capability.id.clone(),
            RegisteredCapability {
                capability,
                handler: Box::new(handler),
                reliability: reliability.clamp(0.0, 1.0),
            },
        );
    }

    /// Get all capability IDs.
    pub fn capability_ids(&self) -> Vec<String> {
        self.capabilities.keys().cloned().collect()
    }

    /// Get all capabilities.
    pub fn capabilities(&self) -> Vec<&Capability> {
        self.capabilities.values().map(|r| &r.capability).collect()
    }

    /// Get reliability for a capability.
    pub fn get_reliability(&self, id: &str) -> f64 {
        self.capabilities
            .get(id)
            .map(|r| r.reliability)
            .unwrap_or(0.0)
    }

    /// Check policy for request (stub - always returns true).
    pub fn check_policy(&self, _request: &Request) -> bool {
        // TODO: Implement policy checks
        true
    }

    /// Interpret intent using registered capabilities.
    pub fn interpret(&self, intent: &str, context: &Context) -> InterpretationResult {
        let caps: Vec<Capability> = self
            .capabilities
            .values()
            .map(|r| r.capability.clone())
            .collect();
        self.interpreter.interpret(intent, context, &caps)
    }

    /// Execute a capability.
    pub fn execute(&self, id: &str, request: &Request) -> SinpResult<serde_json::Value> {
        let registered = self
            .capabilities
            .get(id)
            .ok_or_else(|| sinp_core::SinpError::Protocol(format!("Capability not found: {}", id)))?;
        (registered.handler)(request)
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sinp_core::message::{AuthMethod, ContextType, Sender};

    fn sample_capability() -> Capability {
        Capability {
            id: "test:v1".to_string(),
            description: "Test capability".to_string(),
            inputs: vec!["input1".to_string()],
            privacy_level: "public".to_string(),
            cost_units: 1.0,
        }
    }

    #[test]
    fn register_and_execute() {
        let mut registry = CapabilityRegistry::new();
        registry.register(
            sample_capability(),
            |_req| Ok(serde_json::json!({"status": "ok"})),
            0.9,
        );

        assert_eq!(registry.capability_ids(), vec!["test:v1"]);
        assert_eq!(registry.get_reliability("test:v1"), 0.9);

        let ctx = Context {
            context_type: ContextType::Transcript,
            content: "test".to_string(),
            semantic_hash: "hash".to_string(),
        };
        let sender = Sender {
            id: "test".to_string(),
            auth_method: AuthMethod::Token,
        };
        let request = Request::new(sender, "test", 0.9, ctx);

        let result = registry.execute("test:v1", &request).unwrap();
        assert_eq!(result["status"], "ok");
    }
}
