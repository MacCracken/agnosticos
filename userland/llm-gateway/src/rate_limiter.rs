//! Per-agent rate limiting for LLM Gateway.
//!
//! Enforces configurable limits on tokens-per-hour, requests-per-minute,
//! and concurrent requests per agent to prevent runaway costs.

use std::collections::HashMap;
use std::time::Instant;

use agnos_common::{AgentId, AgentRateLimit};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Tracks per-agent request/token usage for rate limiting enforcement.
pub struct AgentRateLimiter {
    /// Configured limits per agent.
    limits: RwLock<HashMap<AgentId, AgentRateLimit>>,
    /// Rolling window of request timestamps per agent (for requests/minute).
    request_windows: RwLock<HashMap<AgentId, Vec<Instant>>>,
    /// Token usage in the current hour per agent.
    hourly_tokens: RwLock<HashMap<AgentId, HourlyTokenBucket>>,
    /// Currently active (in-flight) requests per agent.
    active_requests: RwLock<HashMap<AgentId, u32>>,
    /// Global default rate limit (applied when agent has no specific limit).
    default_limit: RwLock<Option<AgentRateLimit>>,
}

/// Tracks token usage within a rolling 1-hour window.
#[derive(Debug, Clone)]
struct HourlyTokenBucket {
    tokens_used: u64,
    window_start: Instant,
}

impl HourlyTokenBucket {
    fn new() -> Self {
        Self {
            tokens_used: 0,
            window_start: Instant::now(),
        }
    }

    /// Reset the bucket if the window has expired.
    fn maybe_reset(&mut self) {
        if self.window_start.elapsed().as_secs() >= 3600 {
            self.tokens_used = 0;
            self.window_start = Instant::now();
        }
    }
}

/// Reason a request was rate-limited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitReason {
    TokensPerHourExceeded { used: u64, limit: u64 },
    RequestsPerMinuteExceeded { count: u32, limit: u32 },
    ConcurrentRequestsExceeded { active: u32, limit: u32 },
}

impl std::fmt::Display for RateLimitReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TokensPerHourExceeded { used, limit } => {
                write!(f, "token limit exceeded: {}/{} tokens/hour", used, limit)
            }
            Self::RequestsPerMinuteExceeded { count, limit } => {
                write!(f, "request limit exceeded: {}/{} requests/minute", count, limit)
            }
            Self::ConcurrentRequestsExceeded { active, limit } => {
                write!(f, "concurrent limit exceeded: {}/{} active requests", active, limit)
            }
        }
    }
}

impl AgentRateLimiter {
    pub fn new() -> Self {
        Self {
            limits: RwLock::new(HashMap::new()),
            request_windows: RwLock::new(HashMap::new()),
            hourly_tokens: RwLock::new(HashMap::new()),
            active_requests: RwLock::new(HashMap::new()),
            default_limit: RwLock::new(None),
        }
    }

    /// Set the rate limit for a specific agent.
    pub async fn set_limit(&self, agent_id: AgentId, limit: AgentRateLimit) {
        debug!(agent_id = ?agent_id, "Setting rate limit: {:?}", limit);
        self.limits.write().await.insert(agent_id, limit);
    }

    /// Remove the rate limit for an agent.
    pub async fn remove_limit(&self, agent_id: AgentId) {
        self.limits.write().await.remove(&agent_id);
        self.request_windows.write().await.remove(&agent_id);
        self.hourly_tokens.write().await.remove(&agent_id);
        self.active_requests.write().await.remove(&agent_id);
    }

    /// Set a global default rate limit for agents without a specific one.
    pub async fn set_default_limit(&self, limit: AgentRateLimit) {
        *self.default_limit.write().await = Some(limit);
    }

    /// Get the effective rate limit for an agent (specific or default).
    pub async fn get_limit(&self, agent_id: AgentId) -> Option<AgentRateLimit> {
        let limits = self.limits.read().await;
        if let Some(limit) = limits.get(&agent_id) {
            return Some(limit.clone());
        }
        drop(limits);
        self.default_limit.read().await.clone()
    }

    /// Atomically check rate limits and record request start if allowed.
    /// Returns `Ok(())` if the request was admitted, `Err(reason)` if rate-limited.
    /// This combines check + record into a single operation to avoid race conditions
    /// where concurrent requests could slip through between a separate check and record.
    pub async fn check_and_record(&self, agent_id: AgentId) -> Result<(), RateLimitReason> {
        let limit = match self.get_limit(agent_id).await {
            Some(l) => l,
            None => return Ok(()), // No limit configured → allow
        };

        // Acquire write locks to make check-and-increment atomic
        let mut active = self.active_requests.write().await;
        let mut windows = self.request_windows.write().await;
        let mut buckets = self.hourly_tokens.write().await;

        // Check concurrent requests
        if limit.max_concurrent_requests > 0 {
            let count = active.get(&agent_id).copied().unwrap_or(0);
            if count >= limit.max_concurrent_requests {
                warn!(
                    agent_id = ?agent_id,
                    active = count,
                    limit = limit.max_concurrent_requests,
                    "Rate limited: concurrent requests exceeded"
                );
                return Err(RateLimitReason::ConcurrentRequestsExceeded {
                    active: count,
                    limit: limit.max_concurrent_requests,
                });
            }
        }

        // Check requests per minute
        if limit.max_requests_per_minute > 0 {
            if let Some(timestamps) = windows.get(&agent_id) {
                let one_minute_ago = Instant::now() - std::time::Duration::from_secs(60);
                let recent_count = timestamps.iter().filter(|t| **t > one_minute_ago).count() as u32;
                if recent_count >= limit.max_requests_per_minute {
                    warn!(
                        agent_id = ?agent_id,
                        count = recent_count,
                        limit = limit.max_requests_per_minute,
                        "Rate limited: requests per minute exceeded"
                    );
                    return Err(RateLimitReason::RequestsPerMinuteExceeded {
                        count: recent_count,
                        limit: limit.max_requests_per_minute,
                    });
                }
            }
        }

        // Check tokens per hour
        if limit.max_tokens_per_hour > 0 {
            let bucket = buckets
                .entry(agent_id)
                .or_insert_with(HourlyTokenBucket::new);
            bucket.maybe_reset();
            if bucket.tokens_used >= limit.max_tokens_per_hour {
                warn!(
                    agent_id = ?agent_id,
                    used = bucket.tokens_used,
                    limit = limit.max_tokens_per_hour,
                    "Rate limited: tokens per hour exceeded"
                );
                return Err(RateLimitReason::TokensPerHourExceeded {
                    used: bucket.tokens_used,
                    limit: limit.max_tokens_per_hour,
                });
            }
        }

        // All checks passed — atomically record the request start
        *active.entry(agent_id).or_insert(0) += 1;

        let timestamps = windows.entry(agent_id).or_default();
        timestamps.push(Instant::now());
        // Prune old timestamps (keep only last 5 minutes for memory efficiency)
        let cutoff = Instant::now() - std::time::Duration::from_secs(300);
        timestamps.retain(|t| *t > cutoff);

        Ok(())
    }

    /// Check if a request from this agent should be allowed (without recording).
    /// Prefer `check_and_record()` for production use to avoid race conditions.
    pub async fn check_request(&self, agent_id: AgentId) -> Result<(), RateLimitReason> {
        let limit = match self.get_limit(agent_id).await {
            Some(l) => l,
            None => return Ok(()), // No limit configured → allow
        };

        // Check concurrent requests
        if limit.max_concurrent_requests > 0 {
            let active = self.active_requests.read().await;
            let count = active.get(&agent_id).copied().unwrap_or(0);
            if count >= limit.max_concurrent_requests {
                return Err(RateLimitReason::ConcurrentRequestsExceeded {
                    active: count,
                    limit: limit.max_concurrent_requests,
                });
            }
        }

        // Check requests per minute
        if limit.max_requests_per_minute > 0 {
            let windows = self.request_windows.read().await;
            if let Some(timestamps) = windows.get(&agent_id) {
                let one_minute_ago = Instant::now() - std::time::Duration::from_secs(60);
                let recent_count = timestamps.iter().filter(|t| **t > one_minute_ago).count() as u32;
                if recent_count >= limit.max_requests_per_minute {
                    return Err(RateLimitReason::RequestsPerMinuteExceeded {
                        count: recent_count,
                        limit: limit.max_requests_per_minute,
                    });
                }
            }
        }

        // Check tokens per hour
        if limit.max_tokens_per_hour > 0 {
            let mut buckets = self.hourly_tokens.write().await;
            let bucket = buckets
                .entry(agent_id)
                .or_insert_with(HourlyTokenBucket::new);
            bucket.maybe_reset();
            if bucket.tokens_used >= limit.max_tokens_per_hour {
                return Err(RateLimitReason::TokensPerHourExceeded {
                    used: bucket.tokens_used,
                    limit: limit.max_tokens_per_hour,
                });
            }
        }

        Ok(())
    }

    /// Record a request starting (increments active count and timestamps).
    /// Note: prefer `check_and_record()` which atomically checks and records.
    pub async fn record_request_start(&self, agent_id: AgentId) {
        // Increment active requests
        let mut active = self.active_requests.write().await;
        *active.entry(agent_id).or_insert(0) += 1;

        // Record timestamp
        let mut windows = self.request_windows.write().await;
        let timestamps = windows.entry(agent_id).or_default();
        timestamps.push(Instant::now());

        // Prune old timestamps (keep only last 5 minutes for memory efficiency)
        let cutoff = Instant::now() - std::time::Duration::from_secs(300);
        timestamps.retain(|t| *t > cutoff);
    }

    /// Record a request completing (decrements active count, adds token usage).
    pub async fn record_request_end(&self, agent_id: AgentId, tokens_used: u64) {
        // Decrement active requests
        let mut active = self.active_requests.write().await;
        if let Some(count) = active.get_mut(&agent_id) {
            *count = count.saturating_sub(1);
        }

        // Record token usage
        if tokens_used > 0 {
            let mut buckets = self.hourly_tokens.write().await;
            let bucket = buckets
                .entry(agent_id)
                .or_insert_with(HourlyTokenBucket::new);
            bucket.maybe_reset();
            bucket.tokens_used += tokens_used;
        }
    }

    /// Get current usage stats for an agent.
    pub async fn get_usage(&self, agent_id: AgentId) -> AgentRateLimitUsage {
        let active = self
            .active_requests
            .read()
            .await
            .get(&agent_id)
            .copied()
            .unwrap_or(0);

        let requests_last_minute = {
            let windows = self.request_windows.read().await;
            windows
                .get(&agent_id)
                .map(|ts| {
                    let cutoff = Instant::now() - std::time::Duration::from_secs(60);
                    ts.iter().filter(|t| **t > cutoff).count() as u32
                })
                .unwrap_or(0)
        };

        let tokens_this_hour = {
            let mut buckets = self.hourly_tokens.write().await;
            buckets
                .get_mut(&agent_id)
                .map(|b| {
                    b.maybe_reset();
                    b.tokens_used
                })
                .unwrap_or(0)
        };

        AgentRateLimitUsage {
            active_requests: active,
            requests_last_minute,
            tokens_this_hour,
        }
    }

    /// List all agents with rate limits configured.
    pub async fn list_limited_agents(&self) -> Vec<(AgentId, AgentRateLimit)> {
        self.limits
            .read()
            .await
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }
}

impl Default for AgentRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Current rate limit usage for an agent.
#[derive(Debug, Clone)]
pub struct AgentRateLimitUsage {
    pub active_requests: u32,
    pub requests_last_minute: u32,
    pub tokens_this_hour: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_agent() -> AgentId {
        AgentId::new()
    }

    fn test_limit() -> AgentRateLimit {
        AgentRateLimit {
            max_tokens_per_hour: 10_000,
            max_requests_per_minute: 10,
            max_concurrent_requests: 3,
        }
    }

    #[tokio::test]
    async fn test_new_limiter() {
        let limiter = AgentRateLimiter::new();
        let agents = limiter.list_limited_agents().await;
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_set_and_get_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();
        let limit = test_limit();

        limiter.set_limit(agent, limit.clone()).await;
        let got = limiter.get_limit(agent).await.unwrap();
        assert_eq!(got.max_tokens_per_hour, 10_000);
        assert_eq!(got.max_requests_per_minute, 10);
    }

    #[tokio::test]
    async fn test_no_limit_allows_all() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();
        assert!(limiter.check_request(agent).await.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_request_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();
        limiter
            .set_limit(
                agent,
                AgentRateLimit {
                    max_concurrent_requests: 2,
                    ..AgentRateLimit::default()
                },
            )
            .await;

        // First two should be allowed
        assert!(limiter.check_request(agent).await.is_ok());
        limiter.record_request_start(agent).await;
        assert!(limiter.check_request(agent).await.is_ok());
        limiter.record_request_start(agent).await;

        // Third should be blocked
        let result = limiter.check_request(agent).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RateLimitReason::ConcurrentRequestsExceeded { active: 2, limit: 2 }
        ));

        // After one finishes, should be allowed again
        limiter.record_request_end(agent, 0).await;
        assert!(limiter.check_request(agent).await.is_ok());
    }

    #[tokio::test]
    async fn test_requests_per_minute_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();
        limiter
            .set_limit(
                agent,
                AgentRateLimit {
                    max_requests_per_minute: 3,
                    ..AgentRateLimit::default()
                },
            )
            .await;

        for _ in 0..3 {
            assert!(limiter.check_request(agent).await.is_ok());
            limiter.record_request_start(agent).await;
            limiter.record_request_end(agent, 0).await;
        }

        // Fourth should be blocked
        let result = limiter.check_request(agent).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RateLimitReason::RequestsPerMinuteExceeded { count: 3, limit: 3 }
        ));
    }

    #[tokio::test]
    async fn test_token_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();
        limiter
            .set_limit(
                agent,
                AgentRateLimit {
                    max_tokens_per_hour: 100,
                    ..AgentRateLimit::default()
                },
            )
            .await;

        // Use up tokens
        limiter.record_request_start(agent).await;
        limiter.record_request_end(agent, 100).await;

        // Should now be blocked
        let result = limiter.check_request(agent).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RateLimitReason::TokensPerHourExceeded { used: 100, limit: 100 }
        ));
    }

    #[tokio::test]
    async fn test_remove_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();
        limiter.set_limit(agent, test_limit()).await;

        // Fill up requests
        for _ in 0..3 {
            limiter.record_request_start(agent).await;
        }
        assert!(limiter.check_request(agent).await.is_err());

        // Remove limit
        limiter.remove_limit(agent).await;
        assert!(limiter.check_request(agent).await.is_ok());
    }

    #[tokio::test]
    async fn test_default_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();

        // No limit yet
        assert!(limiter.get_limit(agent).await.is_none());

        // Set default
        limiter
            .set_default_limit(AgentRateLimit {
                max_concurrent_requests: 1,
                ..AgentRateLimit::default()
            })
            .await;

        // Default should apply
        let limit = limiter.get_limit(agent).await.unwrap();
        assert_eq!(limit.max_concurrent_requests, 1);
    }

    #[tokio::test]
    async fn test_specific_limit_overrides_default() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();

        limiter
            .set_default_limit(AgentRateLimit {
                max_concurrent_requests: 1,
                ..AgentRateLimit::default()
            })
            .await;

        limiter
            .set_limit(
                agent,
                AgentRateLimit {
                    max_concurrent_requests: 10,
                    ..AgentRateLimit::default()
                },
            )
            .await;

        let limit = limiter.get_limit(agent).await.unwrap();
        assert_eq!(limit.max_concurrent_requests, 10);
    }

    #[tokio::test]
    async fn test_get_usage() {
        let limiter = AgentRateLimiter::new();
        let agent = test_agent();

        limiter.record_request_start(agent).await;
        limiter.record_request_end(agent, 500).await;
        limiter.record_request_start(agent).await;

        let usage = limiter.get_usage(agent).await;
        assert_eq!(usage.active_requests, 1);
        assert_eq!(usage.requests_last_minute, 2);
        assert_eq!(usage.tokens_this_hour, 500);
    }

    #[tokio::test]
    async fn test_rate_limit_reason_display() {
        let r1 = RateLimitReason::TokensPerHourExceeded {
            used: 10000,
            limit: 10000,
        };
        assert!(r1.to_string().contains("10000/10000 tokens/hour"));

        let r2 = RateLimitReason::RequestsPerMinuteExceeded {
            count: 60,
            limit: 60,
        };
        assert!(r2.to_string().contains("60/60 requests/minute"));

        let r3 = RateLimitReason::ConcurrentRequestsExceeded {
            active: 5,
            limit: 5,
        };
        assert!(r3.to_string().contains("5/5 active requests"));
    }

    #[tokio::test]
    async fn test_multiple_agents_independent() {
        let limiter = AgentRateLimiter::new();
        let agent1 = test_agent();
        let agent2 = test_agent();

        limiter
            .set_limit(
                agent1,
                AgentRateLimit {
                    max_concurrent_requests: 1,
                    ..AgentRateLimit::default()
                },
            )
            .await;

        limiter.record_request_start(agent1).await;
        // agent1 should be blocked
        assert!(limiter.check_request(agent1).await.is_err());
        // agent2 should not be affected (no limit)
        assert!(limiter.check_request(agent2).await.is_ok());
    }

    #[tokio::test]
    async fn test_list_limited_agents() {
        let limiter = AgentRateLimiter::new();
        let a1 = test_agent();
        let a2 = test_agent();

        limiter.set_limit(a1, test_limit()).await;
        limiter.set_limit(a2, test_limit()).await;

        let agents = limiter.list_limited_agents().await;
        assert_eq!(agents.len(), 2);
    }
}
