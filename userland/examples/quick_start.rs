//! AGNOS API Quick Start Example
//!
//! This example demonstrates the basic usage of AGNOS APIs.

use agnos_common::llm::{ModelCapability, ModelInfo, Provider};
use agnos_common::{
    AgentConfig, AgentId, AgentType, FilesystemRule, FsAccess, InferenceRequest, NetworkAccess,
    Permission, ResourceLimits, SandboxConfig,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AGNOS API Quick Start ===\n");

    // 1. Agent Configuration
    println!("1. Creating Agent Configuration...");
    let config = AgentConfig {
        name: "example-agent".to_string(),
        agent_type: AgentType::Service,
        resource_limits: ResourceLimits {
            max_memory: 512 * 1024 * 1024,
            max_cpu_time: 3600 * 1000, // 1 hour
            max_file_descriptors: 100,
            max_processes: 5,
        },
        sandbox: SandboxConfig {
            filesystem_rules: vec![FilesystemRule {
                path: PathBuf::from("/tmp"),
                access: FsAccess::ReadWrite,
            }],
            network_access: NetworkAccess::Restricted,
            seccomp_rules: vec![],
            isolate_network: true,
        },
        permissions: vec![
            Permission::FileRead,
            Permission::FileWrite,
            Permission::NetworkAccess,
        ],
        metadata: serde_json::json!({
            "description": "Example agent",
            "version": "1.0.0"
        }),
    };
    println!("   Agent name: {}", config.name);
    println!("   Agent type: {:?}", config.agent_type);
    println!("   Max memory: {} bytes", config.resource_limits.max_memory);
    println!("   Permissions: {:?}", config.permissions);
    println!();

    // 2. Generate Agent ID
    println!("2. Generating Agent ID...");
    let agent_id = AgentId::new();
    println!("   Agent ID: {}", agent_id);
    println!();

    // 3. LLM Request
    println!("3. Creating LLM Inference Request...");
    let request = InferenceRequest {
        prompt: "Explain what AGNOS is in one sentence.".to_string(),
        model: "llama2".to_string(),
        max_tokens: 100,
        temperature: 0.7,
        top_p: 0.9,
        presence_penalty: 0.0,
        frequency_penalty: 0.0,
    };
    println!("   Model: {}", request.model);
    println!("   Prompt: {}", request.prompt);
    println!("   Max tokens: {}", request.max_tokens);
    println!("   Temperature: {}", request.temperature);
    println!();

    // 4. Model Info
    println!("4. Creating Model Info...");
    let model = ModelInfo {
        id: "llama2-7b".to_string(),
        name: "Llama 2 7B".to_string(),
        provider: Provider::Local,
        capabilities: vec![
            ModelCapability::TextGeneration,
            ModelCapability::CodeGeneration,
        ],
        max_tokens: 4096,
        size_bytes: 3_800_000_000,
        loaded: false,
    };
    println!("   Model ID: {}", model.id);
    println!("   Provider: {:?}", model.provider);
    println!("   Max tokens: {}", model.max_tokens);
    println!();

    // 5. Serialization
    println!("5. Serializing to JSON...");
    let config_json = serde_json::to_string_pretty(&config)?;
    println!("   Config JSON length: {} bytes", config_json.len());

    let request_json = serde_json::to_string_pretty(&request)?;
    println!("   Request JSON length: {} bytes", request_json.len());
    println!();

    println!("=== Quick Start Complete ===");
    println!("\nNext steps:");
    println!("  1. Check out examples/file-manager-agent.rs");
    println!("  2. Read docs/development/agent-development.md");
    println!("  3. Run: cargo run --example quick_start");

    Ok(())
}
