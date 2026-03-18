//! AI Safety Mechanisms for AGNOS
//!
//! Enforces safety constraints on agent behavior to prevent harm — even when
//! instructed by malicious prompts or compromised models. Includes policy-based
//! action filtering, prompt injection detection, output validation, rate
//! limiting, and a per-agent circuit breaker.

pub mod guardrails;
pub mod injection;
pub mod policy;
pub mod types;

// Re-export the full public API so existing consumers are unaffected.
pub use guardrails::{CircuitState, SafetyCircuitBreaker};
pub use injection::{InjectionResult, PromptInjectionDetector};
pub use policy::{default_policies, SafetyEngine};
pub use types::{
    ActionType, SafetyAction, SafetyEnforcement, SafetyPolicy, SafetyRule, SafetyRuleType,
    SafetySeverity, SafetyVerdict, SafetyViolation,
};

#[cfg(test)]
mod tests;
