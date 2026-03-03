//! LLM integration for AI assistance
//!
//! Connects to the AGNOS LLM Gateway (port 8088) for natural language
//! understanding and command generation.

use anyhow::{Context, Result};
use tracing::{debug, warn};

/// Default LLM Gateway endpoint
const DEFAULT_GATEWAY_URL: &str = "http://127.0.0.1:8088";

/// LLM client for AI shell assistance
pub struct LlmClient {
    endpoint: String,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(endpoint: Option<String>) -> Self {
        let endpoint = endpoint.unwrap_or_else(|| DEFAULT_GATEWAY_URL.to_string());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { endpoint, client }
    }

    /// Send a chat completion request to the LLM gateway
    async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String> {
        let url = format!("{}/v1/chat/completions", self.endpoint);

        let body = serde_json::json!({
            "model": "default",
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": user_message}
            ],
            "max_tokens": 1024,
            "temperature": 0.3
        });

        let resp = self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to connect to LLM Gateway")?;

        let status = resp.status();
        let result: serde_json::Value = resp.json().await
            .context("Failed to parse LLM response")?;

        if status.is_success() {
            let text = result["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string();
            debug!("LLM response ({} chars)", text.len());
            Ok(text)
        } else {
            let msg = result["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            anyhow::bail!("LLM Gateway error ({}): {}", status, msg)
        }
    }

    /// Generate command suggestion from natural language
    pub async fn suggest_command(&self, request: &str) -> Result<String> {
        let system = "You are an expert Linux shell assistant in the AGNOS operating system. \
            Given a natural language request, output ONLY the shell command(s) that would \
            accomplish the task. No explanation, no markdown — just the command(s), one per line.";

        match self.chat(system, request).await {
            Ok(cmd) => Ok(cmd),
            Err(e) => {
                warn!("LLM suggestion failed, falling back to hint: {}", e);
                Ok(format!("# Suggested command for: {}\n# (LLM Gateway unavailable: {})", request, e))
            }
        }
    }

    /// Explain what a command does
    pub async fn explain_command(&self, command: &str) -> Result<String> {
        let system = "You are an expert Linux shell teacher in the AGNOS operating system. \
            Explain the given command clearly and concisely. Break down each flag and argument. \
            Keep it under 10 lines.";

        let prompt = format!("Explain this command: {}", command);

        match self.chat(system, &prompt).await {
            Ok(explanation) => Ok(explanation),
            Err(e) => {
                warn!("LLM explanation failed, falling back: {}", e);
                Ok(format!("Command: {}\n(Explanation unavailable — LLM Gateway error: {})", command, e))
            }
        }
    }

    /// Answer general question
    pub async fn answer_question(&self, question: &str) -> Result<String> {
        let system = "You are a helpful assistant in the AGNOS AI-native operating system. \
            Answer the user's question concisely and accurately. If the question is about \
            AGNOS, explain its features. Keep responses under 20 lines.";

        match self.chat(system, question).await {
            Ok(answer) => Ok(answer),
            Err(e) => {
                warn!("LLM answer failed, falling back: {}", e);
                Ok(format!("Question: {}\n(Answer unavailable — LLM Gateway error: {})", question, e))
            }
        }
    }
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_client_new() {
        let client = LlmClient::new(None);
        assert_eq!(client.endpoint, DEFAULT_GATEWAY_URL);
    }

    #[test]
    fn test_llm_client_new_with_endpoint() {
        let client = LlmClient::new(Some("http://localhost:11434".to_string()));
        assert_eq!(client.endpoint, "http://localhost:11434");
    }

    #[test]
    fn test_llm_client_default() {
        let client = LlmClient::default();
        assert_eq!(client.endpoint, DEFAULT_GATEWAY_URL);
    }

    #[tokio::test]
    async fn test_suggest_command() {
        let client = LlmClient::default();
        let result = client.suggest_command("list files").await;
        assert!(result.is_ok());
        // Falls back gracefully when gateway is not running
        let suggestion = result.unwrap();
        assert!(!suggestion.is_empty());
    }

    #[tokio::test]
    async fn test_explain_command() {
        let client = LlmClient::default();
        let result = client.explain_command("ls -la").await;
        assert!(result.is_ok());
        let explanation = result.unwrap();
        assert!(!explanation.is_empty());
    }

    #[tokio::test]
    async fn test_answer_question() {
        let client = LlmClient::default();
        let result = client.answer_question("What is AGNOS?").await;
        assert!(result.is_ok());
        let answer = result.unwrap();
        assert!(!answer.is_empty());
    }
}
