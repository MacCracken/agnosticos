//! Circuit breaker for agent failure management.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::debug;

/// State of the circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation — requests flow through.
    Closed,
    /// Failures exceeded threshold — requests are blocked.
    Open,
    /// Recovery window — limited requests allowed to test health.
    HalfOpen,
}

/// Configuration for a circuit breaker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before tripping to Open.
    pub failure_threshold: u32,
    /// How long to stay Open before transitioning to HalfOpen (milliseconds).
    pub recovery_timeout_ms: u64,
    /// Maximum requests allowed in HalfOpen state before deciding.
    pub half_open_max_attempts: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout_ms: 30_000,
            half_open_max_attempts: 3,
        }
    }
}

/// Circuit breaker that tracks agent failures and prevents cascading errors.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    success_count_half_open: u32,
    failure_threshold: u32,
    recovery_timeout: Duration,
    half_open_max: u32,
    last_failure_time: Option<Instant>,
    last_state_change: Instant,
}

impl CircuitBreaker {
    /// Create a new circuit breaker in the Closed state.
    pub fn new(failure_threshold: u32, recovery_timeout: Duration, half_open_max: u32) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count_half_open: 0,
            failure_threshold,
            recovery_timeout,
            half_open_max,
            last_failure_time: None,
            last_state_change: Instant::now(),
        }
    }

    /// Create from a config struct.
    pub fn from_config(config: &CircuitBreakerConfig) -> Self {
        Self::new(
            config.failure_threshold,
            Duration::from_millis(config.recovery_timeout_ms),
            config.half_open_max_attempts,
        )
    }

    /// Record a successful operation. Resets failure count; if HalfOpen, may transition to Closed.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count_half_open += 1;
                if self.success_count_half_open >= self.half_open_max {
                    self.transition_to(CircuitState::Closed);
                    self.failure_count = 0;
                }
            }
            CircuitState::Open => {
                // Shouldn't happen (requests are blocked), but handle gracefully
                self.failure_count = 0;
            }
        }
    }

    /// Record a failure. Increments count; trips to Open if threshold exceeded.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());

        match self.state {
            CircuitState::Closed => {
                if self.failure_count >= self.failure_threshold {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in HalfOpen trips back to Open
                self.transition_to(CircuitState::Open);
            }
            CircuitState::Open => {
                // Already open, just update the timestamp
            }
        }
    }

    /// Check whether a request should be allowed through.
    ///
    /// - Closed: always allows
    /// - Open: blocks unless recovery_timeout has elapsed (then transitions to HalfOpen)
    /// - HalfOpen: allows (limited attempts)
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if self.last_state_change.elapsed() >= self.recovery_timeout {
                    self.transition_to(CircuitState::HalfOpen);
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Get the current state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Get the current failure count.
    pub fn failure_count(&self) -> u32 {
        self.failure_count
    }

    /// Get the time of the last recorded failure.
    pub fn last_failure_time(&self) -> Option<Instant> {
        self.last_failure_time
    }

    /// Force the circuit breaker back to Closed state.
    pub fn reset(&mut self) {
        self.transition_to(CircuitState::Closed);
        self.failure_count = 0;
        self.success_count_half_open = 0;
        self.last_failure_time = None;
    }

    fn transition_to(&mut self, new_state: CircuitState) {
        debug!(
            from = ?self.state,
            to = ?new_state,
            failures = self.failure_count,
            "Circuit breaker state transition"
        );
        self.state = new_state;
        self.last_state_change = Instant::now();
        if new_state == CircuitState::HalfOpen {
            self.success_count_half_open = 0;
        }
    }
}
