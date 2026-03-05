//! AGNOS LLM Gateway — library target for benchmarks and integration tests.

pub mod accounting;
pub mod cache;
pub mod providers;

// Re-export key types from main.rs that live in the binary.
// The gateway struct itself lives in main.rs; for benchmarks we use
// the individual modules directly (cache, accounting, providers).
