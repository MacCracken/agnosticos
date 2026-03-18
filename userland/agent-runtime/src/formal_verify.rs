//! Formal Verification of Security-Critical Components
//!
//! A practical property-based verification framework for expressing, checking,
//! and tracking formal properties and invariants about AGNOS security
//! subsystems. Covers sandboxing, trust verification, privilege escalation,
//! audit chains, and more.

use std::collections::HashMap;
use std::fmt;
use std::time::Instant;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// ComponentId
// ---------------------------------------------------------------------------

/// Identifies a security-critical AGNOS component whose properties we verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComponentId {
    Sandbox,
    TrustVerifier,
    AuditChain,
    AccessControl,
    CryptoOperations,
    StateTransitions,
    ResourceLimits,
    IpcProtocol,
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sandbox => write!(f, "Sandbox"),
            Self::TrustVerifier => write!(f, "TrustVerifier"),
            Self::AuditChain => write!(f, "AuditChain"),
            Self::AccessControl => write!(f, "AccessControl"),
            Self::CryptoOperations => write!(f, "CryptoOperations"),
            Self::StateTransitions => write!(f, "StateTransitions"),
            Self::ResourceLimits => write!(f, "ResourceLimits"),
            Self::IpcProtocol => write!(f, "IpcProtocol"),
        }
    }
}

// ---------------------------------------------------------------------------
// PropertyType
// ---------------------------------------------------------------------------

/// Classification of a formal property.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PropertyType {
    /// Must always hold (e.g., "audit chain hash is never broken").
    Invariant { condition: String },
    /// Must hold before an operation begins.
    Precondition {
        operation: String,
        condition: String,
    },
    /// Must hold after an operation completes.
    Postcondition {
        operation: String,
        condition: String,
    },
    /// Something bad never happens.
    SafetyProperty { description: String },
    /// Something good eventually happens.
    LivenessProperty { description: String },
    /// A concrete implementation refines an abstract specification.
    Refinement {
        abstract_spec: String,
        concrete_impl: String,
    },
}

// ---------------------------------------------------------------------------
// ProofMethod
// ---------------------------------------------------------------------------

/// The method used (or to be used) to verify a property.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProofMethod {
    ModelChecking,
    PropertyTesting { num_cases: u64 },
    StaticAnalysis,
    TypeSystemProof,
    RuntimeMonitor,
    ManualReview,
}

// ---------------------------------------------------------------------------
// VerificationStatus
// ---------------------------------------------------------------------------

/// Current verification status of a property.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VerificationStatus {
    Unverified,
    InProgress,
    Verified { confidence: f64 },
    Failed { counterexample: String },
    Skipped { reason: String },
}

// ---------------------------------------------------------------------------
// Property
// ---------------------------------------------------------------------------

/// A formal property about a security-critical component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub property_id: String,
    pub name: String,
    pub description: String,
    pub component: ComponentId,
    pub property_type: PropertyType,
    pub status: VerificationStatus,
    pub last_verified: Option<DateTime<Utc>>,
    pub proof_method: ProofMethod,
}

// ---------------------------------------------------------------------------
// StateMachineProperty
// ---------------------------------------------------------------------------

/// Properties that can be checked against a state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StateMachineProperty {
    /// Can the target state be reached from the initial state?
    Reachability { target_state: String },
    /// Are there non-terminal states with no outgoing transitions?
    Deadlock,
    /// No state has duplicate outgoing transitions (simplified: no duplicate
    /// source in the transition list).
    Determinism,
    /// All declared states are reachable from the initial state.
    NoUnreachableStates,
}

// ---------------------------------------------------------------------------
// VerificationResult
// ---------------------------------------------------------------------------

/// Outcome of running a verification check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub property_id: String,
    pub passed: bool,
    pub method: ProofMethod,
    pub iterations_run: u64,
    pub counterexample: Option<String>,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ComponentCoverage
// ---------------------------------------------------------------------------

/// Verification coverage summary for a single component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentCoverage {
    pub component: ComponentId,
    pub total_properties: usize,
    pub verified: usize,
    pub failed: usize,
    pub coverage_percent: f64,
}

// ---------------------------------------------------------------------------
// VerificationReport
// ---------------------------------------------------------------------------

/// Aggregate report across all tracked properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub total_properties: usize,
    pub verified_count: usize,
    pub failed_count: usize,
    pub unverified_count: usize,
    pub skipped_count: usize,
    pub results: Vec<VerificationResult>,
    pub component_coverage: HashMap<ComponentId, ComponentCoverage>,
}

// ---------------------------------------------------------------------------
// InvariantCheckResult
// ---------------------------------------------------------------------------

/// Result of a single runtime invariant check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvariantCheckResult {
    pub name: String,
    pub passed: bool,
    pub checked_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// InvariantMonitor
// ---------------------------------------------------------------------------

/// Runtime monitor that tracks invariant descriptions and checks them on
/// demand with caller-supplied functions.
#[derive(Debug, Default)]
pub struct InvariantMonitor {
    invariant_names: Vec<String>,
}

impl InvariantMonitor {
    pub fn new() -> Self {
        Self {
            invariant_names: Vec::new(),
        }
    }

    /// Register an invariant by name. The actual check function is supplied
    /// when `check_all` is called.
    pub fn add_invariant(&mut self, name: &str) {
        self.invariant_names.push(name.to_string());
    }

    /// Return the list of registered invariant names.
    pub fn invariants(&self) -> &[String] {
        &self.invariant_names
    }

    /// Run all registered invariants against the supplied check functions.
    ///
    /// `checks` maps invariant name to a callable that returns `true` when the
    /// invariant holds. Any invariant without a matching entry is reported as
    /// failed.
    pub fn check_all(
        &self,
        checks: &HashMap<String, Box<dyn Fn() -> bool>>,
    ) -> Vec<InvariantCheckResult> {
        let now = Utc::now();
        self.invariant_names
            .iter()
            .map(|name| {
                let passed = checks.get(name).map(|f| f()).unwrap_or(false);
                if !passed {
                    warn!(invariant = %name, "invariant check failed");
                }
                InvariantCheckResult {
                    name: name.clone(),
                    passed,
                    checked_at: now,
                }
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PropertyChecker
// ---------------------------------------------------------------------------

/// Core verification engine: registers properties, runs checks, and produces
/// reports.
pub struct PropertyChecker {
    properties: HashMap<String, Property>,
    results: Vec<VerificationResult>,
}

impl PropertyChecker {
    pub fn new() -> Self {
        Self {
            properties: HashMap::new(),
            results: Vec::new(),
        }
    }

    /// Register a property for tracking. Validates that the property_id is
    /// non-empty and not already registered.
    pub fn register_property(&mut self, property: Property) -> Result<()> {
        if property.property_id.is_empty() {
            bail!("property_id must not be empty");
        }
        if property.name.is_empty() {
            bail!("property name must not be empty");
        }
        if self.properties.contains_key(&property.property_id) {
            bail!("property '{}' is already registered", property.property_id);
        }
        debug!(id = %property.property_id, name = %property.name, "registered property");
        self.properties
            .insert(property.property_id.clone(), property);
        Ok(())
    }

    /// Look up a property by id.
    pub fn get_property(&self, property_id: &str) -> Option<&Property> {
        self.properties.get(property_id)
    }

    /// Return all properties belonging to a component.
    pub fn properties_for_component(&self, component: ComponentId) -> Vec<&Property> {
        self.properties
            .values()
            .filter(|p| p.component == component)
            .collect()
    }

    /// Run an invariant check `num_iterations` times. Reports the first
    /// failing iteration as a counterexample.
    pub fn check_invariant(
        &mut self,
        property_id: &str,
        test_fn: impl Fn() -> bool,
        num_iterations: u64,
    ) -> VerificationResult {
        let start = Instant::now();
        let mut counterexample: Option<String> = None;
        let mut passed = true;
        let mut iterations_run: u64 = 0;

        for i in 0..num_iterations {
            iterations_run = i + 1;
            if !test_fn() {
                passed = false;
                counterexample = Some(format!("failed at iteration {}", i));
                warn!(
                    property = %property_id,
                    iteration = i,
                    "invariant check failed"
                );
                break;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = VerificationResult {
            property_id: property_id.to_string(),
            passed,
            method: ProofMethod::PropertyTesting {
                num_cases: num_iterations,
            },
            iterations_run,
            counterexample,
            duration_ms,
            timestamp: Utc::now(),
        };

        // Update property status if registered.
        if let Some(prop) = self.properties.get_mut(property_id) {
            prop.status = if result.passed {
                VerificationStatus::Verified { confidence: 1.0 }
            } else {
                VerificationStatus::Failed {
                    counterexample: result.counterexample.clone().unwrap_or_default(),
                }
            };
            prop.last_verified = Some(Utc::now());
        }

        self.results.push(result.clone());
        info!(
            property = %property_id,
            passed = result.passed,
            iterations = iterations_run,
            "invariant check complete"
        );
        result
    }

    /// Verify a property of a state machine defined by `states`, `transitions`
    /// (source, target pairs), and an `initial` state.
    pub fn check_state_machine(
        &mut self,
        states: &[String],
        transitions: &[(String, String)],
        initial: &str,
        property: StateMachineProperty,
    ) -> VerificationResult {
        let start = Instant::now();

        let (passed, counterexample) = match &property {
            StateMachineProperty::Reachability { target_state } => {
                let reachable = self.reachable_states(states, transitions, initial);
                if reachable.contains(target_state.as_str()) {
                    (true, None)
                } else {
                    (
                        false,
                        Some(format!(
                            "state '{}' is not reachable from '{}'",
                            target_state, initial
                        )),
                    )
                }
            }
            StateMachineProperty::Deadlock => {
                // Find non-terminal states with no outgoing transitions.
                // A state is considered a potential deadlock if it has no
                // outgoing transitions AND it is not the only state (single-
                // state machines are trivially deadlock-free if intended).
                let sources: std::collections::HashSet<&str> =
                    transitions.iter().map(|(s, _)| s.as_str()).collect();
                let deadlocked: Vec<&str> = states
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|s| !sources.contains(s))
                    .collect();
                if deadlocked.is_empty() {
                    (true, None)
                } else {
                    (
                        false,
                        Some(format!("deadlocked states: {}", deadlocked.join(", "))),
                    )
                }
            }
            StateMachineProperty::Determinism => {
                // Simplified: check for duplicate (source) entries in
                // transitions, meaning the same source goes to two different
                // targets.
                let mut seen: HashMap<&str, &str> = HashMap::new();
                let mut non_det: Option<String> = None;
                for (src, tgt) in transitions {
                    if let Some(prev_tgt) = seen.get(src.as_str()) {
                        if *prev_tgt != tgt.as_str() {
                            non_det = Some(format!(
                                "state '{}' has transitions to '{}' and '{}'",
                                src, prev_tgt, tgt
                            ));
                            break;
                        }
                    } else {
                        seen.insert(src.as_str(), tgt.as_str());
                    }
                }
                match non_det {
                    None => (true, None),
                    Some(msg) => (false, Some(msg)),
                }
            }
            StateMachineProperty::NoUnreachableStates => {
                let reachable = self.reachable_states(states, transitions, initial);
                let unreachable: Vec<&str> = states
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|s| !reachable.contains(s))
                    .collect();
                if unreachable.is_empty() {
                    (true, None)
                } else {
                    (
                        false,
                        Some(format!("unreachable states: {}", unreachable.join(", "))),
                    )
                }
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = VerificationResult {
            property_id: format!("sm_{}", Uuid::new_v4()),
            passed,
            method: ProofMethod::ModelChecking,
            iterations_run: 1,
            counterexample,
            duration_ms,
            timestamp: Utc::now(),
        };

        self.results.push(result.clone());
        result
    }

    /// Check trace refinement: every concrete trace must be a prefix/subset of
    /// some abstract trace.
    pub fn verify_refinement(
        &mut self,
        abstract_traces: &[Vec<String>],
        concrete_traces: &[Vec<String>],
    ) -> VerificationResult {
        let start = Instant::now();
        let mut passed = true;
        let mut counterexample: Option<String> = None;

        for (i, concrete) in concrete_traces.iter().enumerate() {
            let matches_any = abstract_traces.iter().any(|abs| {
                // Concrete trace must be a prefix of an abstract trace.
                if concrete.len() > abs.len() {
                    return false;
                }
                concrete.iter().zip(abs.iter()).all(|(c, a)| c == a)
            });
            if !matches_any {
                passed = false;
                counterexample = Some(format!(
                    "concrete trace {} ({:?}) is not a prefix of any abstract trace",
                    i, concrete
                ));
                break;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = VerificationResult {
            property_id: format!("refinement_{}", Uuid::new_v4()),
            passed,
            method: ProofMethod::ModelChecking,
            iterations_run: concrete_traces.len() as u64,
            counterexample,
            duration_ms,
            timestamp: Utc::now(),
        };

        self.results.push(result.clone());
        result
    }

    /// Update the status of a registered property.
    pub fn update_status(&mut self, property_id: &str, status: VerificationStatus) -> Result<()> {
        match self.properties.get_mut(property_id) {
            Some(prop) => {
                debug!(id = %property_id, "updating property status");
                prop.status = status;
                prop.last_verified = Some(Utc::now());
                Ok(())
            }
            None => bail!("property '{}' not found", property_id),
        }
    }

    /// Generate an aggregate verification report.
    pub fn verification_report(&self) -> VerificationReport {
        let mut verified_count = 0usize;
        let mut failed_count = 0usize;
        let mut unverified_count = 0usize;
        let mut skipped_count = 0usize;

        let mut component_map: HashMap<ComponentId, (usize, usize, usize)> = HashMap::new();

        for prop in self.properties.values() {
            let entry = component_map.entry(prop.component).or_insert((0, 0, 0));
            entry.0 += 1; // total

            match &prop.status {
                VerificationStatus::Verified { .. } => {
                    verified_count += 1;
                    entry.1 += 1;
                }
                VerificationStatus::Failed { .. } => {
                    failed_count += 1;
                    entry.2 += 1;
                }
                VerificationStatus::Skipped { .. } => {
                    skipped_count += 1;
                }
                VerificationStatus::Unverified => {
                    unverified_count += 1;
                }
                VerificationStatus::InProgress => {
                    unverified_count += 1;
                }
            }
        }

        let component_coverage: HashMap<ComponentId, ComponentCoverage> = component_map
            .into_iter()
            .map(|(comp, (total, verified, failed))| {
                let coverage_percent = if total > 0 {
                    (verified as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                (
                    comp,
                    ComponentCoverage {
                        component: comp,
                        total_properties: total,
                        verified,
                        failed,
                        coverage_percent,
                    },
                )
            })
            .collect();

        VerificationReport {
            total_properties: self.properties.len(),
            verified_count,
            failed_count,
            unverified_count,
            skipped_count,
            results: self.results.clone(),
            component_coverage,
        }
    }

    // -- helpers --

    /// BFS to find all states reachable from `initial`.
    fn reachable_states<'a>(
        &self,
        states: &'a [String],
        transitions: &[(String, String)],
        initial: &str,
    ) -> std::collections::HashSet<&'a str> {
        let mut reachable = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        // Find the initial state in the states slice so we borrow from it.
        if let Some(init) = states.iter().find(|s| s.as_str() == initial) {
            reachable.insert(init.as_str());
            queue.push_back(init.as_str());
        }

        while let Some(current) = queue.pop_front() {
            for (src, tgt) in transitions {
                if src.as_str() == current {
                    if let Some(target) = states.iter().find(|s| *s == tgt) {
                        if reachable.insert(target.as_str()) {
                            queue.push_back(target.as_str());
                        }
                    }
                }
            }
        }

        reachable
    }
}

impl Default for PropertyChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in AGNOS security properties
// ---------------------------------------------------------------------------

/// Returns a set of built-in formal properties covering AGNOS security
/// subsystems.
pub fn agnos_security_properties() -> Vec<Property> {
    vec![
        Property {
            property_id: "agnos.audit.chain_integrity".to_string(),
            name: "Audit chain integrity".to_string(),
            description: "Audit chain hash is never broken — each entry's hash includes the previous entry's hash".to_string(),
            component: ComponentId::AuditChain,
            property_type: PropertyType::Invariant {
                condition: "hash(entry_n) == sha256(entry_n.data || entry_{n-1}.hash)".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::RuntimeMonitor,
        },
        Property {
            property_id: "agnos.sandbox.isolation".to_string(),
            name: "Sandbox isolation".to_string(),
            description: "Agent cannot access files outside its allowed paths".to_string(),
            component: ComponentId::Sandbox,
            property_type: PropertyType::SafetyProperty {
                description: "No file access outside the sandbox boundary".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::PropertyTesting { num_cases: 1000 },
        },
        Property {
            property_id: "agnos.trust.hierarchy".to_string(),
            name: "Trust hierarchy ordering".to_string(),
            description: "Trust levels are strictly ordered: SystemCore > Verified > Community > Unverified > Revoked".to_string(),
            component: ComponentId::TrustVerifier,
            property_type: PropertyType::Invariant {
                condition: "SystemCore.rank > Verified.rank > Community.rank > Unverified.rank > Revoked.rank".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::TypeSystemProof,
        },
        Property {
            property_id: "agnos.state.valid_transitions".to_string(),
            name: "State machine validity".to_string(),
            description: "No invalid transitions in service lifecycle state machine".to_string(),
            component: ComponentId::StateTransitions,
            property_type: PropertyType::SafetyProperty {
                description: "Only declared transitions are allowed".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::ModelChecking,
        },
        Property {
            property_id: "agnos.access.privilege_escalation".to_string(),
            name: "Privilege escalation requires approval".to_string(),
            description: "Privilege escalation always requires explicit user approval".to_string(),
            component: ComponentId::AccessControl,
            property_type: PropertyType::Precondition {
                operation: "escalate_privilege".to_string(),
                condition: "user_approval == true".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::StaticAnalysis,
        },
        Property {
            property_id: "agnos.crypto.secret_zeroing".to_string(),
            name: "Secret zeroing on drop".to_string(),
            description: "Secrets are zeroed from memory when dropped".to_string(),
            component: ComponentId::CryptoOperations,
            property_type: PropertyType::Postcondition {
                operation: "drop_secret".to_string(),
                condition: "memory_region == all_zeros".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::PropertyTesting { num_cases: 500 },
        },
        Property {
            property_id: "agnos.rate_limiter.atomicity".to_string(),
            name: "Rate limiter atomicity".to_string(),
            description: "Check-and-increment operation on the rate limiter is atomic".to_string(),
            component: ComponentId::ResourceLimits,
            property_type: PropertyType::SafetyProperty {
                description: "No TOCTOU between rate check and increment".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::ModelChecking,
        },
        Property {
            property_id: "agnos.ipc.fifo_ordering".to_string(),
            name: "IPC message ordering".to_string(),
            description: "Messages are delivered in FIFO order per channel".to_string(),
            component: ComponentId::IpcProtocol,
            property_type: PropertyType::SafetyProperty {
                description: "For any channel, if msg A is sent before msg B, A is delivered before B".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::PropertyTesting { num_cases: 10000 },
        },
        Property {
            property_id: "agnos.sandbox.apply_order".to_string(),
            name: "Sandbox apply order".to_string(),
            description: "Sandbox layers applied in correct order: encrypted storage, MAC, Landlock, seccomp, network, audit".to_string(),
            component: ComponentId::Sandbox,
            property_type: PropertyType::Invariant {
                condition: "apply_order == [EncryptedStorage, MAC, Landlock, Seccomp, Network, Audit]".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::StaticAnalysis,
        },
        Property {
            property_id: "agnos.access.no_default_root".to_string(),
            name: "No default root access".to_string(),
            description: "Agents never receive root privileges by default".to_string(),
            component: ComponentId::AccessControl,
            property_type: PropertyType::SafetyProperty {
                description: "Default agent capability set excludes CAP_SYS_ADMIN".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::StaticAnalysis,
        },
        Property {
            property_id: "agnos.crypto.key_rotation".to_string(),
            name: "Key rotation liveness".to_string(),
            description: "Signing keys are rotated within the configured interval".to_string(),
            component: ComponentId::CryptoOperations,
            property_type: PropertyType::LivenessProperty {
                description: "Key rotation eventually completes within max_key_age".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::RuntimeMonitor,
        },
        Property {
            property_id: "agnos.state.service_liveness".to_string(),
            name: "Service startup liveness".to_string(),
            description: "A started service eventually reaches Running or Failed state".to_string(),
            component: ComponentId::StateTransitions,
            property_type: PropertyType::LivenessProperty {
                description: "Starting -> Running | Failed within timeout".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::ModelChecking,
        },
        Property {
            property_id: "agnos.audit.completeness".to_string(),
            name: "Audit completeness".to_string(),
            description: "Every security-sensitive operation produces an audit entry".to_string(),
            component: ComponentId::AuditChain,
            property_type: PropertyType::Invariant {
                condition: "forall op in sensitive_ops: exists entry in audit_log where entry.op == op".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::StaticAnalysis,
        },
        Property {
            property_id: "agnos.resource.memory_bound".to_string(),
            name: "Agent memory bounded".to_string(),
            description: "Agent memory usage never exceeds its declared limit".to_string(),
            component: ComponentId::ResourceLimits,
            property_type: PropertyType::SafetyProperty {
                description: "agent.rss <= agent.memory_limit at all times".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::RuntimeMonitor,
        },
        Property {
            property_id: "agnos.trust.refinement".to_string(),
            name: "Trust verification refinement".to_string(),
            description: "Concrete trust verification refines the abstract trust model".to_string(),
            component: ComponentId::TrustVerifier,
            property_type: PropertyType::Refinement {
                abstract_spec: "TrustModel { verify(artifact) -> TrustLevel }".to_string(),
                concrete_impl: "sigil::verify_artifact() -> TrustLevel".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::ManualReview,
        },
    ]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    fn make_property(id: &str, component: ComponentId) -> Property {
        Property {
            property_id: id.to_string(),
            name: format!("Test property {}", id),
            description: "A test property".to_string(),
            component,
            property_type: PropertyType::Invariant {
                condition: "true".to_string(),
            },
            status: VerificationStatus::Unverified,
            last_verified: None,
            proof_method: ProofMethod::PropertyTesting { num_cases: 100 },
        }
    }

    fn s(v: &str) -> String {
        v.to_string()
    }

    // -- Property registration and retrieval --

    #[test]
    fn test_register_property() {
        let mut checker = PropertyChecker::new();
        let prop = make_property("p1", ComponentId::Sandbox);
        assert!(checker.register_property(prop).is_ok());
        assert!(checker.get_property("p1").is_some());
    }

    #[test]
    fn test_register_duplicate_fails() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("p1", ComponentId::Sandbox))
            .unwrap();
        let result = checker.register_property(make_property("p1", ComponentId::Sandbox));
        assert!(result.is_err());
    }

    #[test]
    fn test_register_empty_id_fails() {
        let mut checker = PropertyChecker::new();
        let result = checker.register_property(make_property("", ComponentId::Sandbox));
        assert!(result.is_err());
    }

    #[test]
    fn test_register_empty_name_fails() {
        let mut checker = PropertyChecker::new();
        let mut prop = make_property("p1", ComponentId::Sandbox);
        prop.name = String::new();
        assert!(checker.register_property(prop).is_err());
    }

    #[test]
    fn test_get_nonexistent_property() {
        let checker = PropertyChecker::new();
        assert!(checker.get_property("nope").is_none());
    }

    #[test]
    fn test_register_multiple_properties() {
        let mut checker = PropertyChecker::new();
        for i in 0..10 {
            checker
                .register_property(make_property(&format!("p{}", i), ComponentId::Sandbox))
                .unwrap();
        }
        assert_eq!(checker.properties.len(), 10);
    }

    // -- Component filtering --

    #[test]
    fn test_properties_for_component() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("s1", ComponentId::Sandbox))
            .unwrap();
        checker
            .register_property(make_property("s2", ComponentId::Sandbox))
            .unwrap();
        checker
            .register_property(make_property("a1", ComponentId::AuditChain))
            .unwrap();
        assert_eq!(
            checker.properties_for_component(ComponentId::Sandbox).len(),
            2
        );
        assert_eq!(
            checker
                .properties_for_component(ComponentId::AuditChain)
                .len(),
            1
        );
    }

    #[test]
    fn test_properties_for_component_empty() {
        let checker = PropertyChecker::new();
        assert!(checker
            .properties_for_component(ComponentId::IpcProtocol)
            .is_empty());
    }

    #[test]
    fn test_all_component_ids() {
        // Ensure all component variants are distinct.
        let components = [
            ComponentId::Sandbox,
            ComponentId::TrustVerifier,
            ComponentId::AuditChain,
            ComponentId::AccessControl,
            ComponentId::CryptoOperations,
            ComponentId::StateTransitions,
            ComponentId::ResourceLimits,
            ComponentId::IpcProtocol,
        ];
        let set: std::collections::HashSet<ComponentId> = components.iter().copied().collect();
        assert_eq!(set.len(), 8);
    }

    // -- Invariant checking --

    #[test]
    fn test_check_invariant_passing() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("inv1", ComponentId::AuditChain))
            .unwrap();
        let result = checker.check_invariant("inv1", || true, 100);
        assert!(result.passed);
        assert_eq!(result.iterations_run, 100);
        assert!(result.counterexample.is_none());
    }

    #[test]
    fn test_check_invariant_failing() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("inv2", ComponentId::AuditChain))
            .unwrap();
        let result = checker.check_invariant("inv2", || false, 100);
        assert!(!result.passed);
        assert_eq!(result.iterations_run, 1);
        assert!(result.counterexample.is_some());
        assert!(result.counterexample.unwrap().contains("iteration 0"));
    }

    #[test]
    fn test_check_invariant_fails_midway() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("inv3", ComponentId::Sandbox))
            .unwrap();
        let counter = std::cell::Cell::new(0u64);
        let result = checker.check_invariant(
            "inv3",
            || {
                let v = counter.get();
                counter.set(v + 1);
                v < 5
            },
            100,
        );
        assert!(!result.passed);
        assert_eq!(result.iterations_run, 6); // 0..5 pass, 5 fails
        assert!(result.counterexample.unwrap().contains("iteration 5"));
    }

    #[test]
    fn test_check_invariant_zero_iterations() {
        let mut checker = PropertyChecker::new();
        let result = checker.check_invariant("ghost", || false, 0);
        assert!(result.passed); // vacuously true
        assert_eq!(result.iterations_run, 0);
    }

    #[test]
    fn test_check_invariant_updates_status_verified() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("inv4", ComponentId::Sandbox))
            .unwrap();
        checker.check_invariant("inv4", || true, 10);
        let prop = checker.get_property("inv4").unwrap();
        assert!(matches!(prop.status, VerificationStatus::Verified { .. }));
        assert!(prop.last_verified.is_some());
    }

    #[test]
    fn test_check_invariant_updates_status_failed() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("inv5", ComponentId::Sandbox))
            .unwrap();
        checker.check_invariant("inv5", || false, 10);
        let prop = checker.get_property("inv5").unwrap();
        assert!(matches!(prop.status, VerificationStatus::Failed { .. }));
    }

    #[test]
    fn test_check_invariant_unregistered_property() {
        let mut checker = PropertyChecker::new();
        // Should still produce a result even if property isn't registered.
        let result = checker.check_invariant("unknown", || true, 5);
        assert!(result.passed);
    }

    // -- State machine: reachability --

    #[test]
    fn test_sm_reachability_pass() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C")];
        let transitions = vec![(s("A"), s("B")), (s("B"), s("C"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::Reachability {
                target_state: s("C"),
            },
        );
        assert!(result.passed);
    }

    #[test]
    fn test_sm_reachability_fail() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C")];
        let transitions = vec![(s("A"), s("B"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::Reachability {
                target_state: s("C"),
            },
        );
        assert!(!result.passed);
        assert!(result.counterexample.unwrap().contains("not reachable"));
    }

    #[test]
    fn test_sm_reachability_initial_is_target() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A")];
        let transitions = vec![];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::Reachability {
                target_state: s("A"),
            },
        );
        assert!(result.passed);
    }

    // -- State machine: deadlock --

    #[test]
    fn test_sm_deadlock_detected() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C")];
        let transitions = vec![(s("A"), s("B"))];
        // B and C have no outgoing transitions -> deadlock
        let result =
            checker.check_state_machine(&states, &transitions, "A", StateMachineProperty::Deadlock);
        assert!(!result.passed);
        assert!(result.counterexample.as_ref().unwrap().contains("B"));
        assert!(result.counterexample.as_ref().unwrap().contains("C"));
    }

    #[test]
    fn test_sm_no_deadlock() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B")];
        let transitions = vec![(s("A"), s("B")), (s("B"), s("A"))];
        let result =
            checker.check_state_machine(&states, &transitions, "A", StateMachineProperty::Deadlock);
        assert!(result.passed);
    }

    #[test]
    fn test_sm_deadlock_self_loop() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B")];
        let transitions = vec![(s("A"), s("B")), (s("B"), s("B"))];
        let result =
            checker.check_state_machine(&states, &transitions, "A", StateMachineProperty::Deadlock);
        assert!(result.passed);
    }

    // -- State machine: determinism --

    #[test]
    fn test_sm_deterministic() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C")];
        let transitions = vec![(s("A"), s("B")), (s("B"), s("C"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::Determinism,
        );
        assert!(result.passed);
    }

    #[test]
    fn test_sm_nondeterministic() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C")];
        let transitions = vec![(s("A"), s("B")), (s("A"), s("C"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::Determinism,
        );
        assert!(!result.passed);
        assert!(result.counterexample.unwrap().contains("A"));
    }

    #[test]
    fn test_sm_deterministic_same_target() {
        // Duplicate transition to same target is fine.
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B")];
        let transitions = vec![(s("A"), s("B")), (s("A"), s("B"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::Determinism,
        );
        assert!(result.passed);
    }

    // -- State machine: unreachable states --

    #[test]
    fn test_sm_no_unreachable() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C")];
        let transitions = vec![(s("A"), s("B")), (s("B"), s("C"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::NoUnreachableStates,
        );
        assert!(result.passed);
    }

    #[test]
    fn test_sm_unreachable_detected() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C"), s("D")];
        let transitions = vec![(s("A"), s("B"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::NoUnreachableStates,
        );
        assert!(!result.passed);
        let ce = result.counterexample.unwrap();
        assert!(ce.contains("C"));
        assert!(ce.contains("D"));
    }

    #[test]
    fn test_sm_single_state_no_unreachable() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A")];
        let transitions = vec![];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::NoUnreachableStates,
        );
        assert!(result.passed);
    }

    // -- State machine: edge cases --

    #[test]
    fn test_sm_empty_states() {
        let mut checker = PropertyChecker::new();
        let states: Vec<String> = vec![];
        let transitions: Vec<(String, String)> = vec![];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::NoUnreachableStates,
        );
        // No states => none unreachable
        assert!(result.passed);
    }

    #[test]
    fn test_sm_disconnected_graph() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A"), s("B"), s("C"), s("D")];
        let transitions = vec![(s("A"), s("B")), (s("C"), s("D"))];
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "A",
            StateMachineProperty::NoUnreachableStates,
        );
        assert!(!result.passed);
        assert!(result.counterexample.unwrap().contains("C"));
    }

    #[test]
    fn test_sm_self_loop_only() {
        let mut checker = PropertyChecker::new();
        let states = vec![s("A")];
        let transitions = vec![(s("A"), s("A"))];
        let result =
            checker.check_state_machine(&states, &transitions, "A", StateMachineProperty::Deadlock);
        assert!(result.passed);
    }

    // -- Refinement checking --

    #[test]
    fn test_refinement_pass() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("init"), s("auth"), s("run")]];
        let conc = vec![vec![s("init"), s("auth")]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(result.passed);
    }

    #[test]
    fn test_refinement_exact_match() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("A"), s("B"), s("C")]];
        let conc = vec![vec![s("A"), s("B"), s("C")]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(result.passed);
    }

    #[test]
    fn test_refinement_fail() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("init"), s("auth"), s("run")]];
        let conc = vec![vec![s("init"), s("WRONG")]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(!result.passed);
        assert!(result.counterexample.unwrap().contains("concrete trace 0"));
    }

    #[test]
    fn test_refinement_concrete_longer_than_abstract() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("A")]];
        let conc = vec![vec![s("A"), s("B")]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(!result.passed);
    }

    #[test]
    fn test_refinement_empty_concrete() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("A"), s("B")]];
        let conc: Vec<Vec<String>> = vec![];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(result.passed); // vacuously true
    }

    #[test]
    fn test_refinement_empty_abstract() {
        let mut checker = PropertyChecker::new();
        let abs: Vec<Vec<String>> = vec![];
        let conc = vec![vec![s("A")]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(!result.passed);
    }

    #[test]
    fn test_refinement_multiple_abstract_traces() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("A"), s("B"), s("C")], vec![s("A"), s("D"), s("E")]];
        let conc = vec![vec![s("A"), s("D")]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(result.passed);
    }

    #[test]
    fn test_refinement_empty_concrete_trace() {
        let mut checker = PropertyChecker::new();
        let abs = vec![vec![s("A")]];
        let conc = vec![vec![]];
        let result = checker.verify_refinement(&abs, &conc);
        assert!(result.passed); // empty is prefix of anything
    }

    // -- Status updates --

    #[test]
    fn test_update_status() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("p1", ComponentId::Sandbox))
            .unwrap();
        checker
            .update_status("p1", VerificationStatus::Verified { confidence: 0.95 })
            .unwrap();
        let prop = checker.get_property("p1").unwrap();
        assert!(matches!(
            prop.status,
            VerificationStatus::Verified { confidence } if (confidence - 0.95).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn test_update_status_nonexistent() {
        let mut checker = PropertyChecker::new();
        let result = checker.update_status("nope", VerificationStatus::Unverified);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_status_to_skipped() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("p1", ComponentId::Sandbox))
            .unwrap();
        checker
            .update_status(
                "p1",
                VerificationStatus::Skipped {
                    reason: "not applicable".to_string(),
                },
            )
            .unwrap();
        assert!(matches!(
            checker.get_property("p1").unwrap().status,
            VerificationStatus::Skipped { .. }
        ));
    }

    #[test]
    fn test_update_status_to_in_progress() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("p1", ComponentId::Sandbox))
            .unwrap();
        checker
            .update_status("p1", VerificationStatus::InProgress)
            .unwrap();
        assert!(matches!(
            checker.get_property("p1").unwrap().status,
            VerificationStatus::InProgress
        ));
    }

    // -- Verification report --

    #[test]
    fn test_verification_report_empty() {
        let checker = PropertyChecker::new();
        let report = checker.verification_report();
        assert_eq!(report.total_properties, 0);
        assert_eq!(report.verified_count, 0);
        assert_eq!(report.failed_count, 0);
    }

    #[test]
    fn test_verification_report_counts() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("p1", ComponentId::Sandbox))
            .unwrap();
        checker
            .register_property(make_property("p2", ComponentId::Sandbox))
            .unwrap();
        checker
            .register_property(make_property("p3", ComponentId::AuditChain))
            .unwrap();

        checker
            .update_status("p1", VerificationStatus::Verified { confidence: 1.0 })
            .unwrap();
        checker
            .update_status(
                "p2",
                VerificationStatus::Failed {
                    counterexample: "bad".to_string(),
                },
            )
            .unwrap();
        // p3 stays Unverified

        let report = checker.verification_report();
        assert_eq!(report.total_properties, 3);
        assert_eq!(report.verified_count, 1);
        assert_eq!(report.failed_count, 1);
        assert_eq!(report.unverified_count, 1);
    }

    #[test]
    fn test_verification_report_component_coverage() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("s1", ComponentId::Sandbox))
            .unwrap();
        checker
            .register_property(make_property("s2", ComponentId::Sandbox))
            .unwrap();
        checker
            .update_status("s1", VerificationStatus::Verified { confidence: 1.0 })
            .unwrap();

        let report = checker.verification_report();
        let cov = report
            .component_coverage
            .get(&ComponentId::Sandbox)
            .unwrap();
        assert_eq!(cov.total_properties, 2);
        assert_eq!(cov.verified, 1);
        assert!((cov.coverage_percent - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_verification_report_results_accumulated() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("inv1", ComponentId::Sandbox))
            .unwrap();
        checker.check_invariant("inv1", || true, 5);
        checker.check_invariant("inv1", || true, 10);
        let report = checker.verification_report();
        assert_eq!(report.results.len(), 2);
    }

    #[test]
    fn test_verification_report_skipped() {
        let mut checker = PropertyChecker::new();
        checker
            .register_property(make_property("p1", ComponentId::Sandbox))
            .unwrap();
        checker
            .update_status(
                "p1",
                VerificationStatus::Skipped {
                    reason: "n/a".to_string(),
                },
            )
            .unwrap();
        let report = checker.verification_report();
        assert_eq!(report.skipped_count, 1);
    }

    // -- Built-in properties --

    #[test]
    fn test_agnos_security_properties_nonempty() {
        let props = agnos_security_properties();
        assert!(props.len() >= 10);
    }

    #[test]
    fn test_agnos_security_properties_unique_ids() {
        let props = agnos_security_properties();
        let ids: std::collections::HashSet<&str> =
            props.iter().map(|p| p.property_id.as_str()).collect();
        assert_eq!(ids.len(), props.len());
    }

    #[test]
    fn test_agnos_security_properties_registerable() {
        let mut checker = PropertyChecker::new();
        for prop in agnos_security_properties() {
            checker.register_property(prop).unwrap();
        }
        assert!(checker.properties.len() >= 10);
    }

    #[test]
    fn test_agnos_security_properties_all_unverified() {
        let props = agnos_security_properties();
        for prop in &props {
            assert!(matches!(prop.status, VerificationStatus::Unverified));
            assert!(prop.last_verified.is_none());
        }
    }

    #[test]
    fn test_agnos_properties_cover_multiple_components() {
        let props = agnos_security_properties();
        let components: std::collections::HashSet<ComponentId> =
            props.iter().map(|p| p.component).collect();
        assert!(components.len() >= 5);
    }

    // -- InvariantMonitor --

    #[test]
    fn test_invariant_monitor_add_and_list() {
        let mut monitor = InvariantMonitor::new();
        monitor.add_invariant("inv1");
        monitor.add_invariant("inv2");
        assert_eq!(monitor.invariants().len(), 2);
    }

    #[test]
    fn test_invariant_monitor_check_all_pass() {
        let mut monitor = InvariantMonitor::new();
        monitor.add_invariant("always_true");
        let mut checks: HashMap<String, Box<dyn Fn() -> bool>> = HashMap::new();
        checks.insert("always_true".to_string(), Box::new(|| true));
        let results = monitor.check_all(&checks);
        assert_eq!(results.len(), 1);
        assert!(results[0].passed);
    }

    #[test]
    fn test_invariant_monitor_check_all_fail() {
        let mut monitor = InvariantMonitor::new();
        monitor.add_invariant("always_false");
        let mut checks: HashMap<String, Box<dyn Fn() -> bool>> = HashMap::new();
        checks.insert("always_false".to_string(), Box::new(|| false));
        let results = monitor.check_all(&checks);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
    }

    #[test]
    fn test_invariant_monitor_missing_check() {
        let mut monitor = InvariantMonitor::new();
        monitor.add_invariant("no_check_provided");
        let checks: HashMap<String, Box<dyn Fn() -> bool>> = HashMap::new();
        let results = monitor.check_all(&checks);
        assert!(!results[0].passed); // Missing check -> failed
    }

    #[test]
    fn test_invariant_monitor_empty() {
        let monitor = InvariantMonitor::new();
        let checks: HashMap<String, Box<dyn Fn() -> bool>> = HashMap::new();
        let results = monitor.check_all(&checks);
        assert!(results.is_empty());
    }

    #[test]
    fn test_invariant_monitor_mixed() {
        let mut monitor = InvariantMonitor::new();
        monitor.add_invariant("good");
        monitor.add_invariant("bad");
        let mut checks: HashMap<String, Box<dyn Fn() -> bool>> = HashMap::new();
        checks.insert("good".to_string(), Box::new(|| true));
        checks.insert("bad".to_string(), Box::new(|| false));
        let results = monitor.check_all(&checks);
        let good = results.iter().find(|r| r.name == "good").unwrap();
        let bad = results.iter().find(|r| r.name == "bad").unwrap();
        assert!(good.passed);
        assert!(!bad.passed);
    }

    // -- PropertyType variants --

    #[test]
    fn test_property_type_invariant() {
        let pt = PropertyType::Invariant {
            condition: "x > 0".to_string(),
        };
        assert!(matches!(pt, PropertyType::Invariant { .. }));
    }

    #[test]
    fn test_property_type_precondition() {
        let pt = PropertyType::Precondition {
            operation: "op".to_string(),
            condition: "x > 0".to_string(),
        };
        assert!(matches!(pt, PropertyType::Precondition { .. }));
    }

    #[test]
    fn test_property_type_postcondition() {
        let pt = PropertyType::Postcondition {
            operation: "op".to_string(),
            condition: "result.is_ok()".to_string(),
        };
        assert!(matches!(pt, PropertyType::Postcondition { .. }));
    }

    #[test]
    fn test_property_type_safety() {
        let pt = PropertyType::SafetyProperty {
            description: "no crash".to_string(),
        };
        assert!(matches!(pt, PropertyType::SafetyProperty { .. }));
    }

    #[test]
    fn test_property_type_liveness() {
        let pt = PropertyType::LivenessProperty {
            description: "eventually terminates".to_string(),
        };
        assert!(matches!(pt, PropertyType::LivenessProperty { .. }));
    }

    #[test]
    fn test_property_type_refinement() {
        let pt = PropertyType::Refinement {
            abstract_spec: "spec".to_string(),
            concrete_impl: "impl".to_string(),
        };
        assert!(matches!(pt, PropertyType::Refinement { .. }));
    }

    // -- ProofMethod variants --

    #[test]
    fn test_proof_method_serialization() {
        let methods = vec![
            ProofMethod::ModelChecking,
            ProofMethod::PropertyTesting { num_cases: 42 },
            ProofMethod::StaticAnalysis,
            ProofMethod::TypeSystemProof,
            ProofMethod::RuntimeMonitor,
            ProofMethod::ManualReview,
        ];
        for method in &methods {
            let json = serde_json::to_string(method).unwrap();
            let _: ProofMethod = serde_json::from_str(&json).unwrap();
        }
    }

    // -- VerificationStatus --

    #[test]
    fn test_verification_status_variants() {
        let statuses = vec![
            VerificationStatus::Unverified,
            VerificationStatus::InProgress,
            VerificationStatus::Verified { confidence: 0.99 },
            VerificationStatus::Failed {
                counterexample: "x=0".to_string(),
            },
            VerificationStatus::Skipped {
                reason: "n/a".to_string(),
            },
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let deserialized: VerificationStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, deserialized);
        }
    }

    // -- ComponentId Display --

    #[test]
    fn test_component_id_display() {
        assert_eq!(ComponentId::Sandbox.to_string(), "Sandbox");
        assert_eq!(ComponentId::TrustVerifier.to_string(), "TrustVerifier");
        assert_eq!(ComponentId::AuditChain.to_string(), "AuditChain");
        assert_eq!(ComponentId::IpcProtocol.to_string(), "IpcProtocol");
    }

    // -- Default impls --

    #[test]
    fn test_property_checker_default() {
        let checker = PropertyChecker::default();
        assert_eq!(checker.properties.len(), 0);
        assert_eq!(checker.results.len(), 0);
    }

    #[test]
    fn test_invariant_monitor_default() {
        let monitor = InvariantMonitor::default();
        assert!(monitor.invariants().is_empty());
    }

    // -- Serialization round-trips --

    #[test]
    fn test_property_serialization() {
        let prop = make_property("ser1", ComponentId::CryptoOperations);
        let json = serde_json::to_string(&prop).unwrap();
        let deserialized: Property = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.property_id, "ser1");
        assert_eq!(deserialized.component, ComponentId::CryptoOperations);
    }

    #[test]
    fn test_verification_result_serialization() {
        let result = VerificationResult {
            property_id: "vr1".to_string(),
            passed: true,
            method: ProofMethod::ModelChecking,
            iterations_run: 42,
            counterexample: None,
            duration_ms: 100,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let _: VerificationResult = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_component_coverage_serialization() {
        let cov = ComponentCoverage {
            component: ComponentId::Sandbox,
            total_properties: 10,
            verified: 7,
            failed: 1,
            coverage_percent: 70.0,
        };
        let json = serde_json::to_string(&cov).unwrap();
        let deserialized: ComponentCoverage = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.total_properties, 10);
    }

    // -- Complex scenarios --

    #[test]
    fn test_full_workflow() {
        let mut checker = PropertyChecker::new();

        // Register built-in properties.
        for prop in agnos_security_properties() {
            checker.register_property(prop).unwrap();
        }

        // Verify one invariant.
        let result = checker.check_invariant("agnos.audit.chain_integrity", || true, 50);
        assert!(result.passed);

        // Mark another as skipped.
        checker
            .update_status(
                "agnos.crypto.key_rotation",
                VerificationStatus::Skipped {
                    reason: "not yet implemented".to_string(),
                },
            )
            .unwrap();

        // Generate report.
        let report = checker.verification_report();
        assert!(report.total_properties >= 10);
        assert_eq!(report.verified_count, 1);
        assert_eq!(report.skipped_count, 1);
    }

    #[test]
    fn test_state_machine_complex_graph() {
        let mut checker = PropertyChecker::new();
        let states = vec![
            s("Init"),
            s("Starting"),
            s("Running"),
            s("Stopping"),
            s("Stopped"),
            s("Failed"),
        ];
        let transitions = vec![
            (s("Init"), s("Starting")),
            (s("Starting"), s("Running")),
            (s("Starting"), s("Failed")),
            (s("Running"), s("Stopping")),
            (s("Stopping"), s("Stopped")),
            (s("Running"), s("Failed")),
        ];

        // All states reachable
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "Init",
            StateMachineProperty::NoUnreachableStates,
        );
        assert!(result.passed);

        // Failed and Stopped are deadlocked (terminal)
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "Init",
            StateMachineProperty::Deadlock,
        );
        assert!(!result.passed);

        // Non-deterministic (Starting -> Running and Starting -> Failed)
        let result = checker.check_state_machine(
            &states,
            &transitions,
            "Init",
            StateMachineProperty::Determinism,
        );
        assert!(!result.passed);
    }

    #[test]
    fn test_invariant_check_result_has_timestamp() {
        let mut monitor = InvariantMonitor::new();
        monitor.add_invariant("ts_test");
        let mut checks: HashMap<String, Box<dyn Fn() -> bool>> = HashMap::new();
        checks.insert("ts_test".to_string(), Box::new(|| true));
        let results = monitor.check_all(&checks);
        // Timestamp should be recent (within last minute).
        let now = Utc::now();
        let diff = now
            .signed_duration_since(results[0].checked_at)
            .num_seconds();
        assert!(diff.abs() < 60);
    }
}
