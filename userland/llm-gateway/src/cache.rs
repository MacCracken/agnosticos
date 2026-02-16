//! Response caching for LLM requests

use std::collections::HashMap;

use agnos_common::{InferenceRequest, InferenceResponse};
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// Cached response entry
#[derive(Clone)]
struct CacheEntry {
    response: InferenceResponse,
    expires_at: Instant,
}

/// Simple LRU cache for LLM responses
pub struct ResponseCache {
    cache: RwLock<HashMap<String, CacheEntry>>,
    ttl: Duration,
}

impl ResponseCache {
    /// Create a new cache with the specified TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Generate a cache key from a request
    fn make_key(request: &InferenceRequest) -> String {
        // Create a deterministic key from request parameters
        format!(
            "{}:{}:{:.2}:{:.2}:{}",
            request.model,
            request.prompt,
            request.temperature,
            request.top_p,
            request.max_tokens
        )
    }

    /// Get a cached response if available and not expired
    pub async fn get(&self, request: &InferenceRequest) -> Option<InferenceResponse> {
        let key = Self::make_key(request);
        let cache = self.cache.read().await;
        
        if let Some(entry) = cache.get(&key) {
            if entry.expires_at > Instant::now() {
                return Some(entry.response.clone());
            }
        }
        
        None
    }

    /// Store a response in the cache
    pub async fn set(&self, request: InferenceRequest, response: InferenceResponse) {
        let key = Self::make_key(&request);
        let entry = CacheEntry {
            response,
            expires_at: Instant::now() + self.ttl,
        };
        
        let mut cache = self.cache.write().await;
        cache.insert(key, entry);
        
        // Simple cleanup: if cache gets too large, clear old entries
        if cache.len() > 1000 {
            self.cleanup_expired(&mut cache).await;
        }
    }

    /// Clean up expired entries
    async fn cleanup_expired(&self, cache: &mut HashMap<String, CacheEntry>) {
        let now = Instant::now();
        cache.retain(|_, entry| entry.expires_at > now);
    }

    /// Clear all cached entries
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total = cache.len();
        let expired = cache.values().filter(|e| e.expires_at <= Instant::now()).count();
        
        CacheStats {
            total_entries: total,
            expired_entries: expired,
            active_entries: total - expired,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub expired_entries: usize,
    pub active_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use agnos_common::{FinishReason, InferenceResponse, TokenUsage};

    #[tokio::test]
    async fn test_cache_new() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_set_and_get() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        
        let request = InferenceRequest {
            prompt: "Hello".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        
        let response = InferenceResponse {
            text: "Hi there!".to_string(),
            tokens_generated: 5,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage {
                prompt_tokens: 2,
                completion_tokens: 5,
                total_tokens: 7,
            },
        };
        
        cache.set(request.clone(), response.clone()).await;
        
        let cached = cache.get(&request).await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        
        let request = InferenceRequest {
            prompt: "Different prompt".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        
        let cached = cache.get(&request).await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        
        let request = InferenceRequest::default();
        let response = InferenceResponse {
            text: "Test".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        
        cache.set(request, response).await;
        cache.clear().await;
        
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        
        let stats = cache.stats().await;
        assert_eq!(stats.active_entries, 0);
    }
}
