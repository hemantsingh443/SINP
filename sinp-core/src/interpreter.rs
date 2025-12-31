//! Interpretation function traits and baseline implementations.
//!
//! The interpretation function f: (Ψ_req, Γ) → (Ψ̂, c, ρ) maps client intent
//! to a server action with confidence.



use crate::message::{Capability, Context};

/// Result of an interpretation function.
#[derive(Debug, Clone)]
pub struct InterpretationResult {
    /// Server's interpretation of the intent (Ψ̂).
    pub interpretation: String,
    /// Matched capability.
    pub capability: Option<Capability>,
    /// Raw model probability (ρ).
    pub raw_confidence: f64,
    /// Alternative interpretations.
    pub alternatives: Vec<AlternativeInterpretation>,
}

/// An alternative interpretation with different capability.
#[derive(Debug, Clone)]
pub struct AlternativeInterpretation {
    pub interpretation: String,
    pub capability: Capability,
    pub confidence: f64,
}

/// Trait for interpretation functions.
///
/// Implementations can range from simple keyword matching to LLM-based
/// semantic understanding.
pub trait Interpreter: Send + Sync {
    /// Interpret client intent given context.
    ///
    /// # Arguments
    /// * `intent` - Client's intent string (Ψ)
    /// * `context` - Conversation context (Γ)
    /// * `capabilities` - Available server capabilities
    ///
    /// # Returns
    /// Interpretation result with matched capability and confidence.
    fn interpret(
        &self,
        intent: &str,
        context: &Context,
        capabilities: &[Capability],
    ) -> InterpretationResult;
}

/// Baseline deterministic interpreter using keyword matching.
///
/// Implements the scoring function from RFC Section 6.1:
/// Score(c) = (Σ I(k ∈ V_int) for k in K_c) / |K_c| × w_match
#[derive(Debug, Clone)]
pub struct KeywordInterpreter {
    /// Keyword weights (default 1.0 for each).
    pub match_weight: f64,
    /// Minimum score to consider a match.
    pub min_score: f64,
}

impl Default for KeywordInterpreter {
    fn default() -> Self {
        Self {
            match_weight: 1.0,
            min_score: 0.2,
        }
    }
}

impl KeywordInterpreter {
    /// Create a new keyword interpreter with custom parameters.
    pub fn new(match_weight: f64, min_score: f64) -> Self {
        Self {
            match_weight,
            min_score,
        }
    }

    /// Tokenize text into words.
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }

    /// Extract keywords from capability description and inputs.
    fn capability_keywords(cap: &Capability) -> Vec<String> {
        let mut keywords = Self::tokenize(&cap.description);
        for input in &cap.inputs {
            keywords.extend(Self::tokenize(input));
        }
        // Also include capability ID parts
        keywords.extend(Self::tokenize(&cap.id));
        keywords
    }

    /// Score a capability against intent.
    fn score(&self, intent_tokens: &[String], cap: &Capability) -> f64 {
        let cap_keywords = Self::capability_keywords(cap);
        if cap_keywords.is_empty() {
            return 0.0;
        }

        let matches = cap_keywords
            .iter()
            .filter(|k| intent_tokens.contains(k))
            .count();

        (matches as f64 / cap_keywords.len() as f64) * self.match_weight
    }
}

impl Interpreter for KeywordInterpreter {
    fn interpret(
        &self,
        intent: &str,
        _context: &Context,
        capabilities: &[Capability],
    ) -> InterpretationResult {
        let intent_tokens = Self::tokenize(intent);

        // Score all capabilities
        let mut scores: Vec<(f64, &Capability)> = capabilities
            .iter()
            .map(|cap| (self.score(&intent_tokens, cap), cap))
            .collect();

        // Sort by score descending
        scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Best match
        let (best_score, capability) = if let Some((score, cap)) = scores.first().copied() {
            if score >= self.min_score {
                (score, Some(cap.clone()))
            } else {
                (score, None)
            }
        } else {
            (0.0, None)
        };

        // Build alternatives (next best matches above threshold)
        let alternatives: Vec<AlternativeInterpretation> = scores
            .iter()
            .skip(1)
            .take(3)
            .filter(|(score, _)| *score >= self.min_score)
            .map(|(score, cap)| AlternativeInterpretation {
                interpretation: format!("Use {} capability", cap.id),
                capability: (*cap).clone(),
                confidence: *score,
            })
            .collect();

        InterpretationResult {
            interpretation: capability
                .as_ref()
                .map(|c| format!("Execute {} for: {}", c.id, intent))
                .unwrap_or_else(|| "No matching capability found".to_string()),
            capability,
            raw_confidence: best_score,
            alternatives,
        }
    }
}

/// Calibration function for LLM confidence scores.
///
/// Uses Platt scaling: P(y=1|x) = 1 / (1 + exp(Ax + B))
pub fn platt_scale(raw_confidence: f64, a: f64, b: f64) -> f64 {
    1.0 / (1.0 + (-a * raw_confidence - b).exp())
}

/// Compute Brier score for evaluating calibration.
///
/// BS = (1/N) Σ (f_t - o_t)²
/// where f_t is forecast probability and o_t is outcome (0 or 1).
pub fn brier_score(predictions: &[(f64, bool)]) -> f64 {
    if predictions.is_empty() {
        return 0.0;
    }

    let sum: f64 = predictions
        .iter()
        .map(|(forecast, outcome)| {
            let o = if *outcome { 1.0 } else { 0.0 };
            (forecast - o).powi(2)
        })
        .sum();

    sum / predictions.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ContextType;

    fn sample_capabilities() -> Vec<Capability> {
        vec![
            Capability {
                id: "fetch_weather:v1".to_string(),
                description: "Get current weather for a location".to_string(),
                inputs: vec!["location".to_string()],
                privacy_level: "public".to_string(),
                cost_units: 0.5,
            },
            Capability {
                id: "book_flight:v1".to_string(),
                description: "Book a flight reservation".to_string(),
                inputs: vec!["origin".to_string(), "destination".to_string(), "date".to_string()],
                privacy_level: "pii_sensitive".to_string(),
                cost_units: 5.0,
            },
            Capability {
                id: "send_email:v1".to_string(),
                description: "Send an email message".to_string(),
                inputs: vec!["recipient".to_string(), "subject".to_string(), "body".to_string()],
                privacy_level: "private".to_string(),
                cost_units: 1.0,
            },
        ]
    }

    fn sample_context() -> Context {
        Context {
            context_type: ContextType::Transcript,
            content: "User session".to_string(),
            semantic_hash: "test".to_string(),
        }
    }

    #[test]
    fn keyword_interpreter_weather() {
        let interpreter = KeywordInterpreter::default();
        let caps = sample_capabilities();
        let ctx = sample_context();

        let result = interpreter.interpret("What's the weather in London?", &ctx, &caps);

        assert!(result.capability.is_some());
        assert!(result.capability.as_ref().unwrap().id.contains("weather"));
        assert!(result.raw_confidence > 0.0);
    }

    #[test]
    fn keyword_interpreter_flight() {
        let interpreter = KeywordInterpreter::default();
        let caps = sample_capabilities();
        let ctx = sample_context();

        let result = interpreter.interpret("I need to book a flight to NYC", &ctx, &caps);

        assert!(result.capability.is_some());
        assert!(result.capability.as_ref().unwrap().id.contains("flight"));
    }

    #[test]
    fn keyword_interpreter_no_match() {
        let interpreter = KeywordInterpreter::new(1.0, 0.5);
        let caps = sample_capabilities();
        let ctx = sample_context();

        let result = interpreter.interpret("Play some music", &ctx, &caps);

        // With high min_score, unrelated intent should not match
        assert!(result.capability.is_none() || result.raw_confidence < 0.5);
    }

    #[test]
    fn platt_scaling() {
        // Test that platt scaling produces values in [0, 1]
        let calibrated = platt_scale(0.5, 1.0, 0.0);
        assert!(calibrated > 0.0 && calibrated < 1.0);

        // Higher raw confidence should give higher calibrated
        let low = platt_scale(0.3, 1.0, 0.0);
        let high = platt_scale(0.9, 1.0, 0.0);
        assert!(high > low);
    }

    #[test]
    fn brier_score_perfect() {
        // Perfect predictions
        let predictions = vec![(1.0, true), (0.0, false), (1.0, true)];
        let bs = brier_score(&predictions);
        assert!(bs < 0.001);
    }

    #[test]
    fn brier_score_worst() {
        // Worst predictions
        let predictions = vec![(0.0, true), (1.0, false)];
        let bs = brier_score(&predictions);
        assert!((bs - 1.0).abs() < 0.001);
    }
}
