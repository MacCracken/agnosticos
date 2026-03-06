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

// ---------------------------------------------------------------------------
// Prometheus-style Metrics Collection
// ---------------------------------------------------------------------------

/// Per-model request and token counters.
#[derive(Debug, Clone, Default)]
struct ModelMetrics {
    success_count: u64,
    error_count: u64,
    tokens_prompt: u64,
    tokens_completion: u64,
    latency_sum_ms: u64,
    request_count: u64,
    cache_hits: u64,
    cache_misses: u64,
}

/// Thread-safe Prometheus-style metrics collector for the LLM Gateway.
///
/// Tracks per-model request counts, token usage, latency, cache hit/miss,
/// and per-agent rate-limit events.
#[derive(Debug)]
pub struct GatewayMetrics {
    models: std::sync::Mutex<HashMap<String, ModelMetrics>>,
    rate_limits: std::sync::Mutex<HashMap<String, u64>>,
}

impl GatewayMetrics {
    /// Create a new metrics collector.
    pub fn new() -> Self {
        Self {
            models: std::sync::Mutex::new(HashMap::new()),
            rate_limits: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Record a completed LLM request.
    pub fn record_request(
        &self,
        model: &str,
        tokens_prompt: u64,
        tokens_completion: u64,
        latency_ms: u64,
        success: bool,
    ) {
        let mut models = self.models.lock().expect("metrics lock poisoned");
        let m = models.entry(model.to_string()).or_default();
        if success {
            m.success_count += 1;
        } else {
            m.error_count += 1;
        }
        m.tokens_prompt += tokens_prompt;
        m.tokens_completion += tokens_completion;
        m.latency_sum_ms += latency_ms;
        m.request_count += 1;
    }

    /// Record a cache hit for a model.
    pub fn record_cache_hit(&self, model: &str) {
        let mut models = self.models.lock().expect("metrics lock poisoned");
        models.entry(model.to_string()).or_default().cache_hits += 1;
    }

    /// Record a cache miss for a model.
    pub fn record_cache_miss(&self, model: &str) {
        let mut models = self.models.lock().expect("metrics lock poisoned");
        models.entry(model.to_string()).or_default().cache_misses += 1;
    }

    /// Record a rate-limit event for an agent.
    pub fn record_rate_limit(&self, agent_id: &str) {
        let mut rl = self.rate_limits.lock().expect("metrics lock poisoned");
        *rl.entry(agent_id.to_string()).or_insert(0) += 1;
    }

    /// Export all metrics in Prometheus exposition format.
    pub fn export_prometheus(&self) -> String {
        let models = self.models.lock().expect("metrics lock poisoned");
        let rate_limits = self.rate_limits.lock().expect("metrics lock poisoned");

        let mut out = String::new();

        // --- requests total ---
        out.push_str("# HELP llm_requests_total Total LLM requests\n");
        out.push_str("# TYPE llm_requests_total counter\n");
        let mut model_names: Vec<&String> = models.keys().collect();
        model_names.sort();
        for model in &model_names {
            let m = &models[*model];
            out.push_str(&format!(
                "llm_requests_total{{model=\"{}\",status=\"success\"}} {}\n",
                model, m.success_count
            ));
            out.push_str(&format!(
                "llm_requests_total{{model=\"{}\",status=\"error\"}} {}\n",
                model, m.error_count
            ));
        }

        // --- tokens total ---
        out.push_str("# HELP llm_tokens_total Total tokens consumed\n");
        out.push_str("# TYPE llm_tokens_total counter\n");
        for model in &model_names {
            let m = &models[*model];
            out.push_str(&format!(
                "llm_tokens_total{{model=\"{}\",type=\"prompt\"}} {}\n",
                model, m.tokens_prompt
            ));
            out.push_str(&format!(
                "llm_tokens_total{{model=\"{}\",type=\"completion\"}} {}\n",
                model, m.tokens_completion
            ));
        }

        // --- request duration ---
        out.push_str("# HELP llm_request_duration_ms LLM request latency\n");
        out.push_str("# TYPE llm_request_duration_ms histogram\n");
        for model in &model_names {
            let m = &models[*model];
            out.push_str(&format!(
                "llm_request_duration_ms_sum{{model=\"{}\"}} {}\n",
                model, m.latency_sum_ms
            ));
            out.push_str(&format!(
                "llm_request_duration_ms_count{{model=\"{}\"}} {}\n",
                model, m.request_count
            ));
        }

        // --- cache hits ---
        out.push_str("# HELP llm_cache_hits_total Cache hit count\n");
        out.push_str("# TYPE llm_cache_hits_total counter\n");
        for model in &model_names {
            let m = &models[*model];
            out.push_str(&format!(
                "llm_cache_hits_total{{model=\"{}\"}} {}\n",
                model, m.cache_hits
            ));
        }

        // --- rate limits ---
        out.push_str("# HELP llm_rate_limits_total Rate limit events\n");
        out.push_str("# TYPE llm_rate_limits_total counter\n");
        let mut agent_ids: Vec<&String> = rate_limits.keys().collect();
        agent_ids.sort();
        for agent_id in &agent_ids {
            out.push_str(&format!(
                "llm_rate_limits_total{{agent=\"{}\"}} {}\n",
                agent_id, rate_limits[*agent_id]
            ));
        }

        out
    }

    /// Reset all collected metrics.
    pub fn reset(&self) {
        self.models.lock().expect("metrics lock poisoned").clear();
        self.rate_limits
            .lock()
            .expect("metrics lock poisoned")
            .clear();
    }

    /// Total request count for a model (success + error).
    pub fn request_count(&self, model: &str) -> u64 {
        let models = self.models.lock().expect("metrics lock poisoned");
        models
            .get(model)
            .map(|m| m.success_count + m.error_count)
            .unwrap_or(0)
    }

    /// Total tokens (prompt, completion) for a model.
    pub fn total_tokens(&self, model: &str) -> (u64, u64) {
        let models = self.models.lock().expect("metrics lock poisoned");
        models
            .get(model)
            .map(|m| (m.tokens_prompt, m.tokens_completion))
            .unwrap_or((0, 0))
    }

    /// Cache hit rate for a model (0.0–1.0). Returns 0.0 if no cache events recorded.
    pub fn cache_hit_rate(&self, model: &str) -> f64 {
        let models = self.models.lock().expect("metrics lock poisoned");
        match models.get(model) {
            Some(m) => {
                let total = m.cache_hits + m.cache_misses;
                if total == 0 {
                    0.0
                } else {
                    m.cache_hits as f64 / total as f64
                }
            }
            None => 0.0,
        }
    }
}

impl Default for GatewayMetrics {
    fn default() -> Self {
        Self::new()
    }
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

    // ------------------------------------------------------------------
    // GatewayMetrics Tests
    // ------------------------------------------------------------------

    #[test]
    fn test_gateway_metrics_new() {
        let metrics = GatewayMetrics::new();
        assert_eq!(metrics.request_count("gpt-4"), 0);
        assert_eq!(metrics.total_tokens("gpt-4"), (0, 0));
    }

    #[test]
    fn test_gateway_metrics_record_request_success() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 100, 50, 200, true);
        assert_eq!(metrics.request_count("gpt-4"), 1);
        assert_eq!(metrics.total_tokens("gpt-4"), (100, 50));
    }

    #[test]
    fn test_gateway_metrics_record_request_error() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 100, 0, 500, false);
        assert_eq!(metrics.request_count("gpt-4"), 1);
        let export = metrics.export_prometheus();
        assert!(export.contains("status=\"error\"} 1"));
    }

    #[test]
    fn test_gateway_metrics_multiple_requests() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 100, 50, 200, true);
        metrics.record_request("gpt-4", 200, 100, 300, true);
        metrics.record_request("gpt-4", 50, 25, 100, false);
        assert_eq!(metrics.request_count("gpt-4"), 3);
        assert_eq!(metrics.total_tokens("gpt-4"), (350, 175));
    }

    #[test]
    fn test_gateway_metrics_per_model_isolation() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 100, 50, 200, true);
        metrics.record_request("claude-3", 200, 100, 300, true);
        assert_eq!(metrics.request_count("gpt-4"), 1);
        assert_eq!(metrics.request_count("claude-3"), 1);
        assert_eq!(metrics.total_tokens("gpt-4"), (100, 50));
        assert_eq!(metrics.total_tokens("claude-3"), (200, 100));
    }

    #[test]
    fn test_gateway_metrics_cache_hit() {
        let metrics = GatewayMetrics::new();
        metrics.record_cache_hit("gpt-4");
        metrics.record_cache_hit("gpt-4");
        metrics.record_cache_miss("gpt-4");
        let rate = metrics.cache_hit_rate("gpt-4");
        assert!((rate - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_gateway_metrics_cache_hit_rate_no_events() {
        let metrics = GatewayMetrics::new();
        assert_eq!(metrics.cache_hit_rate("gpt-4"), 0.0);
    }

    #[test]
    fn test_gateway_metrics_rate_limit() {
        let metrics = GatewayMetrics::new();
        metrics.record_rate_limit("agent-001");
        metrics.record_rate_limit("agent-001");
        metrics.record_rate_limit("agent-002");
        let export = metrics.export_prometheus();
        assert!(export.contains("llm_rate_limits_total{agent=\"agent-001\"} 2"));
        assert!(export.contains("llm_rate_limits_total{agent=\"agent-002\"} 1"));
    }

    #[test]
    fn test_gateway_metrics_reset() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 100, 50, 200, true);
        metrics.record_cache_hit("gpt-4");
        metrics.record_rate_limit("agent-001");
        metrics.reset();
        assert_eq!(metrics.request_count("gpt-4"), 0);
        assert_eq!(metrics.total_tokens("gpt-4"), (0, 0));
        assert_eq!(metrics.cache_hit_rate("gpt-4"), 0.0);
        let export = metrics.export_prometheus();
        assert!(!export.contains("agent-001"));
    }

    #[test]
    fn test_gateway_metrics_export_format_headers() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 100, 50, 200, true);
        let export = metrics.export_prometheus();
        assert!(export.contains("# HELP llm_requests_total Total LLM requests"));
        assert!(export.contains("# TYPE llm_requests_total counter"));
        assert!(export.contains("# HELP llm_tokens_total Total tokens consumed"));
        assert!(export.contains("# TYPE llm_tokens_total counter"));
        assert!(export.contains("# HELP llm_request_duration_ms LLM request latency"));
        assert!(export.contains("# TYPE llm_request_duration_ms histogram"));
        assert!(export.contains("# HELP llm_cache_hits_total Cache hit count"));
        assert!(export.contains("# TYPE llm_cache_hits_total counter"));
        assert!(export.contains("# HELP llm_rate_limits_total Rate limit events"));
        assert!(export.contains("# TYPE llm_rate_limits_total counter"));
    }

    #[test]
    fn test_gateway_metrics_export_values() {
        let metrics = GatewayMetrics::new();
        metrics.record_request("gpt-4", 500, 200, 300, true);
        metrics.record_request("gpt-4", 100, 50, 150, false);
        metrics.record_cache_hit("gpt-4");

        let export = metrics.export_prometheus();
        assert!(export.contains("llm_requests_total{model=\"gpt-4\",status=\"success\"} 1"));
        assert!(export.contains("llm_requests_total{model=\"gpt-4\",status=\"error\"} 1"));
        assert!(export.contains("llm_tokens_total{model=\"gpt-4\",type=\"prompt\"} 600"));
        assert!(export.contains("llm_tokens_total{model=\"gpt-4\",type=\"completion\"} 250"));
        assert!(export.contains("llm_request_duration_ms_sum{model=\"gpt-4\"} 450"));
        assert!(export.contains("llm_request_duration_ms_count{model=\"gpt-4\"} 2"));
        assert!(export.contains("llm_cache_hits_total{model=\"gpt-4\"} 1"));
    }

    #[test]
    fn test_gateway_metrics_export_empty() {
        let metrics = GatewayMetrics::new();
        let export = metrics.export_prometheus();
        // Should still have section headers but no data lines with models
        assert!(export.contains("# HELP"));
        assert!(!export.contains("gpt-4"));
    }

    #[test]
    fn test_gateway_metrics_default() {
        let metrics = GatewayMetrics::default();
        assert_eq!(metrics.request_count("any"), 0);
    }
}
