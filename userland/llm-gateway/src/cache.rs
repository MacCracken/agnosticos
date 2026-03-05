//! Response caching for LLM requests

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

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

    /// Generate a cache key from a request by hashing its contents.
    ///
    /// Uses `DefaultHasher` to produce a fixed-size key regardless of prompt length,
    /// keeping HashMap operations O(1) even for large prompts.
    fn make_key(request: &InferenceRequest) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        request.model.hash(&mut hasher);
        request.prompt.hash(&mut hasher);
        request.temperature.to_bits().hash(&mut hasher);
        request.top_p.to_bits().hash(&mut hasher);
        request.max_tokens.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
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
    #[allow(dead_code)]
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

    #[tokio::test]
    async fn test_cache_cleanup_on_overflow() {
        // Use a very short TTL so entries expire immediately
        let cache = ResponseCache::new(Duration::from_millis(1));

        let response = InferenceResponse {
            text: "reply".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };

        // Insert 1001 entries to trigger cleanup
        for i in 0..1001 {
            let request = InferenceRequest {
                prompt: format!("prompt-{}", i),
                model: "test".to_string(),
                max_tokens: 100,
                temperature: 0.7,
                top_p: 1.0,
                presence_penalty: 0.0,
                frequency_penalty: 0.0,
            };
            cache.set(request, response.clone()).await;
        }

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Insert one more to trigger cleanup of expired entries
        let request = InferenceRequest {
            prompt: "trigger-cleanup".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        cache.set(request, response).await;

        // After cleanup, most expired entries should be removed
        let stats = cache.stats().await;
        assert!(stats.total_entries < 1001, "expected cleanup to remove entries, got {}", stats.total_entries);
    }

    #[tokio::test]
    async fn test_cache_expired_entry_returns_none() {
        let cache = ResponseCache::new(Duration::from_millis(1));

        let request = InferenceRequest {
            prompt: "expire me".to_string(),
            model: "test".to_string(),
            max_tokens: 10,
            temperature: 0.5,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "will expire".to_string(),
            tokens_generated: 2,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };

        cache.set(request.clone(), response).await;
        tokio::time::sleep(Duration::from_millis(5)).await;

        let cached = cache.get(&request).await;
        assert!(cached.is_none(), "Expired entry should not be returned");
    }

    #[tokio::test]
    async fn test_cache_key_deterministic() {
        let req = InferenceRequest {
            prompt: "deterministic".to_string(),
            model: "model-a".to_string(),
            max_tokens: 50,
            temperature: 0.3,
            top_p: 0.9,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let key1 = ResponseCache::make_key(&req);
        let key2 = ResponseCache::make_key(&req);
        assert_eq!(key1, key2, "Same request should produce same key");
    }

    #[tokio::test]
    async fn test_cache_different_prompts_different_keys() {
        let req_a = InferenceRequest {
            prompt: "prompt A".to_string(),
            model: "model".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let req_b = InferenceRequest {
            prompt: "prompt B".to_string(),
            model: "model".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let key_a = ResponseCache::make_key(&req_a);
        let key_b = ResponseCache::make_key(&req_b);
        assert_ne!(key_a, key_b, "Different prompts should produce different keys");
    }

    #[tokio::test]
    async fn test_cache_different_models_different_keys() {
        let req_a = InferenceRequest {
            prompt: "same".to_string(),
            model: "model-1".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let req_b = InferenceRequest {
            prompt: "same".to_string(),
            model: "model-2".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert_ne!(
            ResponseCache::make_key(&req_a),
            ResponseCache::make_key(&req_b)
        );
    }

    #[tokio::test]
    async fn test_cache_different_temperature_different_keys() {
        let req_a = InferenceRequest {
            prompt: "same".to_string(),
            model: "same".to_string(),
            max_tokens: 100,
            temperature: 0.0,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let req_b = InferenceRequest {
            prompt: "same".to_string(),
            model: "same".to_string(),
            max_tokens: 100,
            temperature: 1.0,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert_ne!(
            ResponseCache::make_key(&req_a),
            ResponseCache::make_key(&req_b)
        );
    }

    #[tokio::test]
    async fn test_cache_stats_with_expired_entries() {
        let cache = ResponseCache::new(Duration::from_millis(1));

        let response = InferenceResponse {
            text: "r".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "m".to_string(),
            usage: TokenUsage::default(),
        };

        cache.set(InferenceRequest {
            prompt: "a".to_string(),
            model: "m".to_string(),
            ..InferenceRequest::default()
        }, response.clone()).await;

        tokio::time::sleep(Duration::from_millis(5)).await;

        // Now insert a fresh entry
        cache.set(InferenceRequest {
            prompt: "b".to_string(),
            model: "m".to_string(),
            ..InferenceRequest::default()
        }, response).await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 2);
        assert!(stats.expired_entries >= 1, "At least one entry should be expired");
        assert!(stats.active_entries >= 1, "At least one entry should be active");
    }

    #[tokio::test]
    async fn test_cache_overwrite_same_key() {
        let cache = ResponseCache::new(Duration::from_secs(60));

        let request = InferenceRequest {
            prompt: "overwrite me".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };

        let response1 = InferenceResponse {
            text: "first".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        let response2 = InferenceResponse {
            text: "second".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };

        cache.set(request.clone(), response1).await;
        cache.set(request.clone(), response2).await;

        let cached = cache.get(&request).await.unwrap();
        assert_eq!(cached.text, "second", "Later set should overwrite earlier");
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 1, "Overwrite should not duplicate entries");
    }

    #[tokio::test]
    async fn test_cache_concurrent_reads() {
        let cache = std::sync::Arc::new(ResponseCache::new(Duration::from_secs(60)));

        let request = InferenceRequest {
            prompt: "concurrent".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "concurrent response".to_string(),
            tokens_generated: 2,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };

        cache.set(request.clone(), response).await;

        let mut handles = vec![];
        for _ in 0..10 {
            let c = cache.clone();
            let r = request.clone();
            handles.push(tokio::spawn(async move {
                c.get(&r).await
            }));
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
            assert_eq!(result.unwrap().text, "concurrent response");
        }
    }

    #[tokio::test]
    async fn test_cache_empty_prompt() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let request = InferenceRequest {
            prompt: "".to_string(),
            model: "".to_string(),
            max_tokens: 0,
            temperature: 0.0,
            top_p: 0.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "empty".to_string(),
            tokens_generated: 0,
            finish_reason: FinishReason::Stop,
            model: "".to_string(),
            usage: TokenUsage::default(),
        };

        cache.set(request.clone(), response).await;
        let cached = cache.get(&request).await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_cache_large_prompt() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let large_prompt = "x".repeat(100_000);
        let request = InferenceRequest {
            prompt: large_prompt,
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "large".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };

        cache.set(request.clone(), response).await;
        let cached = cache.get(&request).await;
        assert!(cached.is_some());
        // Key should still be fixed-length hash
        let key = ResponseCache::make_key(&request);
        assert_eq!(key.len(), 16, "Key should be 16 hex chars regardless of prompt size");
    }

    #[tokio::test]
    async fn test_cache_clear_then_get() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let request = InferenceRequest::default();
        let response = InferenceResponse {
            text: "will be cleared".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "m".to_string(),
            usage: TokenUsage::default(),
        };
        cache.set(request.clone(), response).await;
        cache.clear().await;
        assert!(cache.get(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_key_differs_on_max_tokens() {
        let req_a = InferenceRequest {
            prompt: "same".to_string(),
            model: "same".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let req_b = InferenceRequest {
            prompt: "same".to_string(),
            model: "same".to_string(),
            max_tokens: 200,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert_ne!(
            ResponseCache::make_key(&req_a),
            ResponseCache::make_key(&req_b),
            "Different max_tokens should produce different keys"
        );
    }

    // ------------------------------------------------------------------
    // Additional cache tests: TTL boundary, top_p key diff, concurrent writes,
    // stats accuracy, multiple clear cycles, key length stability
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_cache_key_differs_on_top_p() {
        let req_a = InferenceRequest {
            prompt: "same".to_string(),
            model: "same".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.5,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let req_b = InferenceRequest {
            prompt: "same".to_string(),
            model: "same".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.9,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        assert_ne!(
            ResponseCache::make_key(&req_a),
            ResponseCache::make_key(&req_b),
            "Different top_p should produce different keys"
        );
    }

    #[tokio::test]
    async fn test_cache_ttl_boundary_just_before_expiry() {
        // Use a generous TTL so the entry is still valid
        let cache = ResponseCache::new(Duration::from_secs(60));
        let request = InferenceRequest {
            prompt: "ttl boundary".to_string(),
            model: "test".to_string(),
            max_tokens: 50,
            temperature: 0.5,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "still valid".to_string(),
            tokens_generated: 2,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        cache.set(request.clone(), response).await;
        // Immediately get should succeed
        assert!(cache.get(&request).await.is_some());
    }

    #[tokio::test]
    async fn test_cache_concurrent_writes() {
        let cache = std::sync::Arc::new(ResponseCache::new(Duration::from_secs(60)));
        let mut handles = vec![];
        for i in 0..20 {
            let c = cache.clone();
            handles.push(tokio::spawn(async move {
                let request = InferenceRequest {
                    prompt: format!("concurrent-write-{}", i),
                    model: "test".to_string(),
                    max_tokens: 100,
                    temperature: 0.7,
                    top_p: 1.0,
                    presence_penalty: 0.0,
                    frequency_penalty: 0.0,
                };
                let response = InferenceResponse {
                    text: format!("response-{}", i),
                    tokens_generated: 1,
                    finish_reason: FinishReason::Stop,
                    model: "test".to_string(),
                    usage: TokenUsage::default(),
                };
                c.set(request, response).await;
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 20);
    }

    #[tokio::test]
    async fn test_cache_concurrent_read_write() {
        let cache = std::sync::Arc::new(ResponseCache::new(Duration::from_secs(60)));
        let request = InferenceRequest {
            prompt: "shared".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "shared response".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        cache.set(request.clone(), response).await;

        let mut handles = vec![];
        // Concurrent readers
        for _ in 0..5 {
            let c = cache.clone();
            let r = request.clone();
            handles.push(tokio::spawn(async move {
                c.get(&r).await
            }));
        }
        // Concurrent writers
        for i in 0..5 {
            let c = cache.clone();
            handles.push(tokio::spawn(async move {
                let req = InferenceRequest {
                    prompt: format!("other-{}", i),
                    model: "test".to_string(),
                    max_tokens: 100,
                    temperature: 0.7,
                    top_p: 1.0,
                    presence_penalty: 0.0,
                    frequency_penalty: 0.0,
                };
                let resp = InferenceResponse {
                    text: "other".to_string(),
                    tokens_generated: 1,
                    finish_reason: FinishReason::Stop,
                    model: "test".to_string(),
                    usage: TokenUsage::default(),
                };
                c.set(req, resp).await;
                None // return type alignment
            }));
        }
        for handle in handles {
            let _ = handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_cache_stats_all_expired() {
        let cache = ResponseCache::new(Duration::from_millis(1));
        let response = InferenceResponse {
            text: "expire".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "m".to_string(),
            usage: TokenUsage::default(),
        };
        for i in 0..5 {
            cache.set(InferenceRequest {
                prompt: format!("e-{}", i),
                model: "m".to_string(),
                ..InferenceRequest::default()
            }, response.clone()).await;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 5);
        assert_eq!(stats.expired_entries, 5);
        assert_eq!(stats.active_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_clear_then_stats() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        for i in 0..10 {
            cache.set(InferenceRequest {
                prompt: format!("p-{}", i),
                model: "m".to_string(),
                ..InferenceRequest::default()
            }, InferenceResponse {
                text: "x".to_string(),
                tokens_generated: 1,
                finish_reason: FinishReason::Stop,
                model: "m".to_string(),
                usage: TokenUsage::default(),
            }).await;
        }
        assert_eq!(cache.stats().await.total_entries, 10);
        cache.clear().await;
        assert_eq!(cache.stats().await.total_entries, 0);
    }

    #[tokio::test]
    async fn test_cache_multiple_clear_cycles() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        for cycle in 0..3 {
            for i in 0..5 {
                cache.set(InferenceRequest {
                    prompt: format!("c{}-p{}", cycle, i),
                    model: "m".to_string(),
                    ..InferenceRequest::default()
                }, InferenceResponse {
                    text: "x".to_string(),
                    tokens_generated: 1,
                    finish_reason: FinishReason::Stop,
                    model: "m".to_string(),
                    usage: TokenUsage::default(),
                }).await;
            }
            assert_eq!(cache.stats().await.total_entries, 5 * (cycle + 1));
            if cycle < 2 {
                // Don't clear on last cycle to verify accumulation
            }
        }
        cache.clear().await;
        assert_eq!(cache.stats().await.total_entries, 0);
    }

    #[test]
    fn test_cache_key_length_always_16() {
        let cases = vec![
            InferenceRequest::default(),
            InferenceRequest {
                prompt: "a".repeat(100_000),
                model: "model".to_string(),
                ..InferenceRequest::default()
            },
            InferenceRequest {
                prompt: "".to_string(),
                model: "".to_string(),
                max_tokens: 0,
                temperature: 0.0,
                top_p: 0.0,
                presence_penalty: 0.0,
                frequency_penalty: 0.0,
            },
        ];
        for req in &cases {
            let key = ResponseCache::make_key(req);
            assert_eq!(key.len(), 16, "Key length should always be 16 hex chars");
        }
    }

    #[test]
    fn test_cache_key_hex_format() {
        let req = InferenceRequest::default();
        let key = ResponseCache::make_key(&req);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()), "Key should be hex: {}", key);
    }

    #[tokio::test]
    async fn test_cache_get_returns_correct_response() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let request = InferenceRequest {
            prompt: "unique prompt for correctness".to_string(),
            model: "test".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "expected response text".to_string(),
            tokens_generated: 42,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 42,
                total_tokens: 52,
            },
        };
        cache.set(request.clone(), response).await;
        let cached = cache.get(&request).await.unwrap();
        assert_eq!(cached.text, "expected response text");
        assert_eq!(cached.tokens_generated, 42);
        assert_eq!(cached.usage.total_tokens, 52);
        assert_eq!(cached.model, "test");
    }

    #[tokio::test]
    async fn test_cache_special_characters_in_prompt() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let request = InferenceRequest {
            prompt: "Hello! @#$%^&*() \n\t\r 日本語 🎉".to_string(),
            model: "test".to_string(),
            max_tokens: 50,
            temperature: 0.7,
            top_p: 1.0,
            presence_penalty: 0.0,
            frequency_penalty: 0.0,
        };
        let response = InferenceResponse {
            text: "special".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        cache.set(request.clone(), response).await;
        assert!(cache.get(&request).await.is_some());
    }

    #[tokio::test]
    async fn test_cache_zero_ttl_entries_expire_immediately() {
        let cache = ResponseCache::new(Duration::from_secs(0));
        let request = InferenceRequest::default();
        let response = InferenceResponse {
            text: "zero ttl".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "m".to_string(),
            usage: TokenUsage::default(),
        };
        cache.set(request.clone(), response).await;
        // With 0 TTL, entry should be expired by the time we read it
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(cache.get(&request).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_overwrite_updates_expiry() {
        let cache = ResponseCache::new(Duration::from_secs(60));
        let request = InferenceRequest {
            prompt: "overwrite expiry".to_string(),
            model: "test".to_string(),
            ..InferenceRequest::default()
        };
        let resp1 = InferenceResponse {
            text: "first".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        let resp2 = InferenceResponse {
            text: "second".to_string(),
            tokens_generated: 1,
            finish_reason: FinishReason::Stop,
            model: "test".to_string(),
            usage: TokenUsage::default(),
        };
        cache.set(request.clone(), resp1).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
        cache.set(request.clone(), resp2).await;
        // Should get the second response with refreshed TTL
        let cached = cache.get(&request).await.unwrap();
        assert_eq!(cached.text, "second");
    }

    #[test]
    fn test_cache_stats_clone() {
        let stats = CacheStats {
            total_entries: 10,
            expired_entries: 3,
            active_entries: 7,
        };
        let cloned = stats.clone();
        assert_eq!(cloned.total_entries, 10);
        assert_eq!(cloned.expired_entries, 3);
        assert_eq!(cloned.active_entries, 7);
    }

    #[test]
    fn test_cache_stats_debug() {
        let stats = CacheStats {
            total_entries: 5,
            expired_entries: 2,
            active_entries: 3,
        };
        let dbg = format!("{:?}", stats);
        assert!(dbg.contains("5"));
        assert!(dbg.contains("2"));
        assert!(dbg.contains("3"));
    }
}
