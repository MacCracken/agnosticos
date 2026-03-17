//! AGNOS LLM Gateway — library target for benchmarks and integration tests.

pub mod acceleration;
pub mod accounting;
pub mod cache;
pub mod mcp_proxy;
pub mod providers;
pub mod rate_limiter;

pub use acceleration::AcceleratorRegistry;

// Re-export key types from main.rs that live in the binary.
// The gateway struct itself lives in main.rs; for benchmarks we use
// the individual modules directly (cache, accounting, providers).
