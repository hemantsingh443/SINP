//! Confidence scoring and decision logic for SINP.
//!
//! Implements the mathematical model from RFC Section 4:
//! - Confidence derivation: Φ_s = min(1, ρ · R(c) · A(res)) · P(pol)
//! - Decision boundary: δ(Φ_s, Φ_c) → Action

use crate::message::Action;

/// Decision thresholds as defined in RFC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Threshold for EXECUTE action (default 0.85).
    pub tau_exec: f64,
    /// Threshold for CLARIFY action (default 0.50).
    pub tau_clarify: f64,
    /// Minimum client confidence to accept (default 0.50).
    pub tau_accept: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            tau_exec: 0.85,
            tau_clarify: 0.50,
            tau_accept: 0.50,
        }
    }
}

impl Thresholds {
    /// Create custom thresholds.
    pub fn new(tau_exec: f64, tau_clarify: f64, tau_accept: f64) -> Self {
        Self {
            tau_exec,
            tau_clarify,
            tau_accept,
        }
    }
}

/// Compute server confidence score.
///
/// Φ_s = min(1, ρ · R(c) · A(res)) · P(pol)
///
/// # Arguments
/// * `rho` - Raw model probability from interpretation function
/// * `reliability` - R(c), reliability factor of capability c, in [0, 1]
/// * `availability` - A(res), resource availability factor, in [0, 1]
/// * `policy_passed` - P(pol), whether policy check passed (true = 1, false = 0)
///
/// # Returns
/// Server confidence score Φ_s in [0, 1]
pub fn compute_server_confidence(
    rho: f64,
    reliability: f64,
    availability: f64,
    policy_passed: bool,
) -> f64 {
    if !policy_passed {
        return 0.0;
    }
    (rho * reliability * availability).min(1.0).max(0.0)
}

/// Decide action based on confidence scores.
///
/// Implements the decision boundary δ(Φ_s, Φ_c) from RFC Section 4.3.
///
/// # Arguments
/// * `phi_s` - Server confidence
/// * `phi_c` - Client confidence
/// * `thresholds` - Decision thresholds
/// * `has_better_alternative` - Whether a better capability exists
/// * `policy_violated` - Whether the request violates policy
/// * `malformed` - Whether the request is malformed
///
/// # Returns
/// The action to take
pub fn decide_action(
    phi_s: f64,
    phi_c: f64,
    thresholds: &Thresholds,
    has_better_alternative: bool,
    policy_violated: bool,
    malformed: bool,
) -> Action {
    // REFUSE takes precedence
    if policy_violated || malformed {
        return Action::Refuse;
    }

    // EXECUTE if both confidences meet thresholds
    if phi_s >= thresholds.tau_exec && phi_c >= thresholds.tau_accept {
        return Action::Execute;
    }

    // PROPOSE if a better alternative exists
    if has_better_alternative {
        return Action::Propose;
    }

    // CLARIFY if confidence is below execution threshold
    if phi_s < thresholds.tau_exec {
        return Action::Clarify;
    }

    // Default to CLARIFY for edge cases
    Action::Clarify
}

/// Simplified decision function for common cases.
pub fn decide_action_simple(phi_s: f64, phi_c: f64) -> Action {
    decide_action(phi_s, phi_c, &Thresholds::default(), false, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_computation() {
        // Normal case
        let phi = compute_server_confidence(0.9, 0.95, 1.0, true);
        assert!((phi - 0.855).abs() < 0.001);

        // Policy failed
        let phi = compute_server_confidence(0.9, 0.95, 1.0, false);
        assert_eq!(phi, 0.0);

        // Clamping to 1.0
        let phi = compute_server_confidence(1.0, 1.0, 1.0, true);
        assert_eq!(phi, 1.0);

        // Low availability
        let phi = compute_server_confidence(0.9, 1.0, 0.5, true);
        assert!((phi - 0.45).abs() < 0.001);
    }

    #[test]
    fn decision_execute() {
        let thresholds = Thresholds::default();
        let action = decide_action(0.90, 0.85, &thresholds, false, false, false);
        assert_eq!(action, Action::Execute);
    }

    #[test]
    fn decision_clarify_low_server() {
        let thresholds = Thresholds::default();
        let action = decide_action(0.60, 0.85, &thresholds, false, false, false);
        assert_eq!(action, Action::Clarify);
    }

    #[test]
    fn decision_refuse_policy() {
        let thresholds = Thresholds::default();
        let action = decide_action(0.95, 0.95, &thresholds, false, true, false);
        assert_eq!(action, Action::Refuse);
    }

    #[test]
    fn decision_refuse_malformed() {
        let thresholds = Thresholds::default();
        let action = decide_action(0.95, 0.95, &thresholds, false, false, true);
        assert_eq!(action, Action::Refuse);
    }

    #[test]
    fn decision_propose() {
        let thresholds = Thresholds::default();
        let action = decide_action(0.70, 0.85, &thresholds, true, false, false);
        assert_eq!(action, Action::Propose);
    }

    #[test]
    fn custom_thresholds() {
        let thresholds = Thresholds::new(0.70, 0.40, 0.40);
        // With lower exec threshold, 0.75 should trigger EXECUTE
        let action = decide_action(0.75, 0.50, &thresholds, false, false, false);
        assert_eq!(action, Action::Execute);
    }
}
