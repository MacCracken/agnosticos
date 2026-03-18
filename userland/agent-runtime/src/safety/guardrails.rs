//! Safety — Circuit breaker guardrail for per-agent safety enforcement.

use std::time::Instant;

use serde::{Deserialize, Serialize};
use tracing::info;
use tracing::warn;

/// State of a per-agent circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation — all actions allowed.
    Closed,
    /// Agent is blocked due to too many violations.
    Open,
    /// Cooling down — one action allowed as a test.
    HalfOpen,
}

/// Per-agent circuit breaker that flips open after repeated safety violations
/// and auto-recovers after a cooldown period.
pub struct SafetyCircuitBreaker {
    pub state: CircuitState,
    /// Number of violations required to trip the breaker.
    pub threshold: usize,
    /// Cooldown before transitioning from Open to HalfOpen.
    pub cooldown_secs: u64,
    failure_count: usize,
    last_failure: Option<Instant>,
    window_secs: u64,
    /// Timestamps of recent failures for sliding-window counting.
    failure_timestamps: Vec<Instant>,
}

impl SafetyCircuitBreaker {
    /// Create a breaker that opens after `threshold` violations within
    /// `window_secs` seconds, and cools down after `cooldown_secs`.
    pub fn new(threshold: usize, window_secs: u64, cooldown_secs: u64) -> Self {
        Self {
            state: CircuitState::Closed,
            threshold,
            cooldown_secs,
            failure_count: 0,
            last_failure: None,
            window_secs,
            failure_timestamps: Vec::new(),
        }
    }

    /// Record a safety violation. May transition Closed -> Open.
    pub fn record_violation(&mut self) {
        let now = Instant::now();
        self.failure_timestamps.push(now);
        self.last_failure = Some(now);
        self.failure_count += 1;

        // Count failures within the window
        let cutoff = now - std::time::Duration::from_secs(self.window_secs);
        self.failure_timestamps.retain(|t| *t >= cutoff);

        if self.failure_timestamps.len() >= self.threshold {
            if self.state != CircuitState::Open {
                warn!(
                    failure_count = self.failure_timestamps.len(),
                    threshold = self.threshold,
                    "Circuit breaker tripped to Open"
                );
            }
            self.state = CircuitState::Open;
        }
    }

    /// Check whether the agent is allowed to proceed.
    ///
    /// - **Closed**: always allowed.
    /// - **Open**: blocked, but auto-transitions to HalfOpen after cooldown.
    /// - **HalfOpen**: allowed once (transitions back to Closed on success,
    ///   or Open on the next violation via `record_violation`).
    pub fn check_allowed(&mut self) -> bool {
        // If Open and cooldown has elapsed, transition to HalfOpen first.
        if self.state == CircuitState::Open {
            if let Some(last) = self.last_failure {
                if last.elapsed() >= std::time::Duration::from_secs(self.cooldown_secs) {
                    info!("Circuit breaker transitioning to HalfOpen");
                    self.state = CircuitState::HalfOpen;
                }
            }
        }

        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                // Allow one action, then close
                info!("Circuit breaker test action allowed, transitioning to Closed");
                self.state = CircuitState::Closed;
                self.failure_timestamps.clear();
                self.failure_count = 0;
                true
            }
        }
    }

    /// Force-reset the breaker to Closed.
    pub fn reset(&mut self) {
        info!("Circuit breaker force-reset to Closed");
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.failure_timestamps.clear();
        self.last_failure = None;
    }
}
