//! Multi-Modal Agent Support
//!
//! Enables agents to declare and use multiple input/output modalities:
//! - Text (NLP, code generation, analysis)
//! - Vision (image analysis, screenshot understanding)
//! - Audio (speech recognition, audio analysis)
//! - Tool use (function calling, API invocation)
//! - Structured data (JSON, tables, graphs)

use std::collections::HashMap;

use agnos_common::AgentId;

/// A modality that an agent can process or produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modality {
    Text,
    Vision,
    Audio,
    ToolUse,
    StructuredData,
    Code,
}

impl std::fmt::Display for Modality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Modality::Text => write!(f, "text"),
            Modality::Vision => write!(f, "vision"),
            Modality::Audio => write!(f, "audio"),
            Modality::ToolUse => write!(f, "tool_use"),
            Modality::StructuredData => write!(f, "structured_data"),
            Modality::Code => write!(f, "code"),
        }
    }
}

/// Describes an agent's multi-modal capabilities.
#[derive(Debug, Clone)]
pub struct ModalityProfile {
    pub agent_id: AgentId,
    /// Modalities this agent can accept as input.
    pub input_modalities: Vec<Modality>,
    /// Modalities this agent can produce as output.
    pub output_modalities: Vec<Modality>,
    /// Maximum input sizes per modality (bytes).
    pub max_input_sizes: HashMap<Modality, u64>,
    /// Supported MIME types per modality.
    pub supported_formats: HashMap<Modality, Vec<String>>,
    /// Model backing this agent (for LLM-based agents).
    pub model: Option<String>,
}

impl ModalityProfile {
    pub fn text_only(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            input_modalities: vec![Modality::Text],
            output_modalities: vec![Modality::Text],
            max_input_sizes: HashMap::from([(Modality::Text, 256 * 1024)]),
            supported_formats: HashMap::from([(Modality::Text, vec!["text/plain".into()])]),
            model: None,
        }
    }

    pub fn vision_capable(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            input_modalities: vec![Modality::Text, Modality::Vision],
            output_modalities: vec![Modality::Text, Modality::StructuredData],
            max_input_sizes: HashMap::from([
                (Modality::Text, 256 * 1024),
                (Modality::Vision, 20 * 1024 * 1024),
            ]),
            supported_formats: HashMap::from([
                (Modality::Text, vec!["text/plain".into()]),
                (Modality::Vision, vec!["image/png".into(), "image/jpeg".into(), "image/webp".into()]),
            ]),
            model: None,
        }
    }

    pub fn full_multimodal(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            input_modalities: vec![
                Modality::Text, Modality::Vision, Modality::Audio,
                Modality::ToolUse, Modality::StructuredData, Modality::Code,
            ],
            output_modalities: vec![
                Modality::Text, Modality::ToolUse,
                Modality::StructuredData, Modality::Code,
            ],
            max_input_sizes: HashMap::from([
                (Modality::Text, 256 * 1024),
                (Modality::Vision, 20 * 1024 * 1024),
                (Modality::Audio, 50 * 1024 * 1024),
                (Modality::StructuredData, 10 * 1024 * 1024),
                (Modality::Code, 1024 * 1024),
            ]),
            supported_formats: HashMap::from([
                (Modality::Text, vec!["text/plain".into()]),
                (Modality::Vision, vec!["image/png".into(), "image/jpeg".into(), "image/webp".into()]),
                (Modality::Audio, vec!["audio/wav".into(), "audio/mp3".into(), "audio/ogg".into()]),
                (Modality::StructuredData, vec!["application/json".into(), "text/csv".into()]),
                (Modality::Code, vec!["text/x-rust".into(), "text/x-python".into(), "text/javascript".into()]),
            ]),
            model: None,
        }
    }

    /// Whether this agent can accept a given input modality.
    pub fn accepts(&self, modality: Modality) -> bool {
        self.input_modalities.contains(&modality)
    }

    /// Whether this agent can produce a given output modality.
    pub fn produces(&self, modality: Modality) -> bool {
        self.output_modalities.contains(&modality)
    }

    /// Check if an input is within size limits.
    pub fn validate_input_size(&self, modality: Modality, size: u64) -> bool {
        self.max_input_sizes
            .get(&modality)
            .map(|max| size <= *max)
            .unwrap_or(false)
    }

    /// Check if a MIME type is supported for a modality.
    pub fn supports_format(&self, modality: Modality, mime_type: &str) -> bool {
        self.supported_formats
            .get(&modality)
            .map(|formats| formats.iter().any(|f| f == mime_type))
            .unwrap_or(false)
    }
}

/// A multi-modal input content block.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        data: Vec<u8>,
        mime_type: String,
        width: Option<u32>,
        height: Option<u32>,
    },
    Audio {
        data: Vec<u8>,
        mime_type: String,
        duration_ms: Option<u64>,
    },
    ToolCall {
        tool_name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        tool_name: String,
        result: serde_json::Value,
        is_error: bool,
    },
    StructuredData {
        data: serde_json::Value,
        schema: Option<String>,
    },
    Code {
        language: String,
        source: String,
    },
}

impl ContentBlock {
    pub fn modality(&self) -> Modality {
        match self {
            ContentBlock::Text { .. } => Modality::Text,
            ContentBlock::Image { .. } => Modality::Vision,
            ContentBlock::Audio { .. } => Modality::Audio,
            ContentBlock::ToolCall { .. } | ContentBlock::ToolResult { .. } => Modality::ToolUse,
            ContentBlock::StructuredData { .. } => Modality::StructuredData,
            ContentBlock::Code { .. } => Modality::Code,
        }
    }

    pub fn size_bytes(&self) -> u64 {
        match self {
            ContentBlock::Text { text } => text.len() as u64,
            ContentBlock::Image { data, .. } => data.len() as u64,
            ContentBlock::Audio { data, .. } => data.len() as u64,
            ContentBlock::ToolCall { arguments, .. } => arguments.to_string().len() as u64,
            ContentBlock::ToolResult { result, .. } => result.to_string().len() as u64,
            ContentBlock::StructuredData { data, .. } => data.to_string().len() as u64,
            ContentBlock::Code { source, .. } => source.len() as u64,
        }
    }
}

/// A multi-modal message with mixed content blocks.
#[derive(Debug, Clone)]
pub struct MultiModalMessage {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    pub agent_id: Option<AgentId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MultiModalMessage {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            agent_id: None,
        }
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: vec![ContentBlock::Text { text: text.into() }],
            agent_id: None,
        }
    }

    /// Get all modalities present in this message.
    pub fn modalities(&self) -> Vec<Modality> {
        let mut mods: Vec<Modality> = self.content.iter().map(|c| c.modality()).collect();
        mods.sort_by_key(|m| *m as u8);
        mods.dedup();
        mods
    }

    /// Total size of all content blocks.
    pub fn total_size(&self) -> u64 {
        self.content.iter().map(|c| c.size_bytes()).sum()
    }

    /// Validate message against an agent's modality profile.
    pub fn validate_for(&self, profile: &ModalityProfile) -> Result<(), String> {
        for block in &self.content {
            let modality = block.modality();
            if !profile.accepts(modality) {
                return Err(format!("Agent does not accept {} input", modality));
            }
            if !profile.validate_input_size(modality, block.size_bytes()) {
                return Err(format!(
                    "{} input too large ({} bytes)",
                    modality,
                    block.size_bytes()
                ));
            }
        }
        Ok(())
    }
}

/// Registry of agent multi-modal capabilities.
pub struct ModalityRegistry {
    profiles: HashMap<AgentId, ModalityProfile>,
}

impl ModalityRegistry {
    pub fn new() -> Self {
        Self {
            profiles: HashMap::new(),
        }
    }

    pub fn register(&mut self, profile: ModalityProfile) {
        self.profiles.insert(profile.agent_id, profile);
    }

    pub fn unregister(&mut self, agent_id: &AgentId) {
        self.profiles.remove(agent_id);
    }

    pub fn get(&self, agent_id: &AgentId) -> Option<&ModalityProfile> {
        self.profiles.get(agent_id)
    }

    /// Find agents that can handle a specific set of input modalities.
    pub fn find_capable(&self, required: &[Modality]) -> Vec<AgentId> {
        self.profiles
            .iter()
            .filter(|(_, profile)| required.iter().all(|m| profile.accepts(*m)))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Find agents that can produce a specific output modality.
    pub fn find_producers(&self, modality: Modality) -> Vec<AgentId> {
        self.profiles
            .iter()
            .filter(|(_, profile)| profile.produces(modality))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Find the best agent for a multi-modal message (accepts all modalities).
    pub fn find_best_for(&self, message: &MultiModalMessage) -> Vec<AgentId> {
        let required = message.modalities();
        self.find_capable(&required)
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

impl Default for ModalityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn agent(n: u8) -> AgentId {
        AgentId(Uuid::from_bytes([n; 16]))
    }

    #[test]
    fn test_text_only_profile() {
        let profile = ModalityProfile::text_only(agent(1));
        assert!(profile.accepts(Modality::Text));
        assert!(!profile.accepts(Modality::Vision));
        assert!(profile.produces(Modality::Text));
    }

    #[test]
    fn test_vision_profile() {
        let profile = ModalityProfile::vision_capable(agent(1));
        assert!(profile.accepts(Modality::Text));
        assert!(profile.accepts(Modality::Vision));
        assert!(!profile.accepts(Modality::Audio));
    }

    #[test]
    fn test_full_multimodal() {
        let profile = ModalityProfile::full_multimodal(agent(1));
        assert!(profile.accepts(Modality::Text));
        assert!(profile.accepts(Modality::Vision));
        assert!(profile.accepts(Modality::Audio));
        assert!(profile.accepts(Modality::ToolUse));
        assert!(profile.accepts(Modality::Code));
    }

    #[test]
    fn test_input_size_validation() {
        let profile = ModalityProfile::text_only(agent(1));
        assert!(profile.validate_input_size(Modality::Text, 1000));
        assert!(!profile.validate_input_size(Modality::Text, 1024 * 1024));
        assert!(!profile.validate_input_size(Modality::Vision, 100)); // not supported
    }

    #[test]
    fn test_format_support() {
        let profile = ModalityProfile::vision_capable(agent(1));
        assert!(profile.supports_format(Modality::Vision, "image/png"));
        assert!(profile.supports_format(Modality::Vision, "image/jpeg"));
        assert!(!profile.supports_format(Modality::Vision, "image/bmp"));
    }

    #[test]
    fn test_content_block_modality() {
        assert_eq!(ContentBlock::Text { text: "hi".into() }.modality(), Modality::Text);
        assert_eq!(ContentBlock::Image { data: vec![], mime_type: "image/png".into(), width: None, height: None }.modality(), Modality::Vision);
        assert_eq!(ContentBlock::Audio { data: vec![], mime_type: "audio/wav".into(), duration_ms: None }.modality(), Modality::Audio);
        assert_eq!(ContentBlock::Code { language: "rust".into(), source: "fn main(){}".into() }.modality(), Modality::Code);
    }

    #[test]
    fn test_message_modalities() {
        let msg = MultiModalMessage {
            role: MessageRole::User,
            content: vec![
                ContentBlock::Text { text: "Describe this image".into() },
                ContentBlock::Image { data: vec![0; 100], mime_type: "image/png".into(), width: Some(100), height: Some(100) },
            ],
            agent_id: None,
        };
        let mods = msg.modalities();
        assert!(mods.contains(&Modality::Text));
        assert!(mods.contains(&Modality::Vision));
        assert_eq!(mods.len(), 2);
    }

    #[test]
    fn test_validate_message_for_profile() {
        let profile = ModalityProfile::text_only(agent(1));
        let text_msg = MultiModalMessage::user_text("Hello");
        assert!(text_msg.validate_for(&profile).is_ok());

        let vision_msg = MultiModalMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Image { data: vec![], mime_type: "image/png".into(), width: None, height: None }],
            agent_id: None,
        };
        assert!(vision_msg.validate_for(&profile).is_err());
    }

    #[test]
    fn test_registry_find_capable() {
        let mut registry = ModalityRegistry::new();
        registry.register(ModalityProfile::text_only(agent(1)));
        registry.register(ModalityProfile::vision_capable(agent(2)));
        registry.register(ModalityProfile::full_multimodal(agent(3)));

        let text_agents = registry.find_capable(&[Modality::Text]);
        assert_eq!(text_agents.len(), 3);

        let vision_agents = registry.find_capable(&[Modality::Text, Modality::Vision]);
        assert_eq!(vision_agents.len(), 2);

        let audio_agents = registry.find_capable(&[Modality::Audio]);
        assert_eq!(audio_agents.len(), 1);
    }

    #[test]
    fn test_registry_find_producers() {
        let mut registry = ModalityRegistry::new();
        registry.register(ModalityProfile::text_only(agent(1)));
        registry.register(ModalityProfile::vision_capable(agent(2)));

        let text_producers = registry.find_producers(Modality::Text);
        assert_eq!(text_producers.len(), 2);

        let data_producers = registry.find_producers(Modality::StructuredData);
        assert_eq!(data_producers.len(), 1); // vision_capable outputs StructuredData
    }

    #[test]
    fn test_registry_find_best_for() {
        let mut registry = ModalityRegistry::new();
        registry.register(ModalityProfile::text_only(agent(1)));
        registry.register(ModalityProfile::vision_capable(agent(2)));

        let msg = MultiModalMessage {
            role: MessageRole::User,
            content: vec![
                ContentBlock::Text { text: "What's in this?".into() },
                ContentBlock::Image { data: vec![], mime_type: "image/png".into(), width: None, height: None },
            ],
            agent_id: None,
        };

        let candidates = registry.find_best_for(&msg);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], agent(2));
    }

    #[test]
    fn test_tool_content_blocks() {
        let call = ContentBlock::ToolCall {
            tool_name: "port_scan".into(),
            arguments: serde_json::json!({"target": "10.0.0.1"}),
        };
        assert_eq!(call.modality(), Modality::ToolUse);

        let result = ContentBlock::ToolResult {
            tool_name: "port_scan".into(),
            result: serde_json::json!({"ports": [80, 443]}),
            is_error: false,
        };
        assert_eq!(result.modality(), Modality::ToolUse);
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = ModalityRegistry::new();
        registry.register(ModalityProfile::text_only(agent(1)));
        assert_eq!(registry.len(), 1);
        registry.unregister(&agent(1));
        assert!(registry.is_empty());
    }

    #[test]
    fn test_message_total_size() {
        let msg = MultiModalMessage {
            role: MessageRole::User,
            content: vec![
                ContentBlock::Text { text: "hello".into() }, // 5 bytes
                ContentBlock::Image { data: vec![0; 1000], mime_type: "image/png".into(), width: None, height: None },
            ],
            agent_id: None,
        };
        assert_eq!(msg.total_size(), 1005);
    }
}
