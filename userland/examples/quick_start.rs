//! AGNOS API Quick Start Example
//!
//! This example demonstrates the basic usage of AGNOS APIs.

use agnostik::{
    AgentConfig, AgentId, AgentType, FilesystemRule, FsAccess, InferenceRequest, LlmProvider,
    NetworkAccess, Permission, ResourceLimits, SandboxConfig,
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
            ..Default::default()
        },
        sandbox: SandboxConfig {
            filesystem_rules: vec![FilesystemRule {
                path: PathBuf::from("/tmp"),
                access: FsAccess::ReadWrite,
                readonly: false,
                noexec: false,
                nosuid: false,
                nodev: false,
                propagation: Default::default(),
            }],
            network_access: NetworkAccess::Restricted,
            ..Default::default()
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
        ..Default::default()
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
        model: "llama2".to_string(),
        prompt: "Explain what AGNOS is in one sentence.".to_string(),
        system: None,
        messages: vec![],
        max_tokens: Some(100),
        sampling: Default::default(),
        stream: false,
        tools: vec![],
        tool_choice: None,
        response_format: None,
        logprobs: false,
        top_logprobs: None,
    };
    println!("   Model: {}", request.model);
    println!("   Prompt: {}", request.prompt);
    println!("   Max tokens: {:?}", request.max_tokens);
    println!();

    // 4. Provider Info
    println!("4. LLM Providers...");
    let provider = LlmProvider::Ollama;
    println!("   Provider: {:?}", provider);
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
