use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_knowledge(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::KnowledgeSearch { query, source } => {
            let _source_flag = source
                .as_ref()
                .map(|s| format!(" --source {}", s))
                .unwrap_or_default();
            Ok(Translation {
                command: "curl".to_string(),
                args: vec![
                    "-s".to_string(),
                    "-X".to_string(),
                    "POST".to_string(),
                    "http://127.0.0.1:8090/v1/knowledge/search".to_string(),
                    "-H".to_string(),
                    "Content-Type: application/json".to_string(),
                    "-d".to_string(),
                    format!(r#"{{"query":"{}","limit":10}}"#, query),
                ],
                description: format!("Search knowledge base for: {}", query),
                permission: PermissionLevel::Safe,
                explanation: "Searches the local knowledge base index".to_string(),
            })
        }

        Intent::RagQuery { query } => Ok(Translation {
            command: "curl".to_string(),
            args: vec![
                "-s".to_string(),
                "-X".to_string(),
                "POST".to_string(),
                "http://127.0.0.1:8090/v1/rag/query".to_string(),
                "-H".to_string(),
                "Content-Type: application/json".to_string(),
                "-d".to_string(),
                format!(r#"{{"query":"{}","top_k":5}}"#, query),
            ],
            description: format!("RAG query: {}", query),
            permission: PermissionLevel::Safe,
            explanation: "Retrieves context-augmented results from the RAG pipeline"
                .to_string(),
        }),

        _ => unreachable!("translate_knowledge called with non-knowledge intent"),
    }
}
