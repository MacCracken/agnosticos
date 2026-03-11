use serde::{Deserialize, Serialize};

/// Description of a single MCP tool parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// Description of a single MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDescription {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// The full tool manifest returned by `GET /v1/mcp/tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolManifest {
    pub tools: Vec<McpToolDescription>,
}

/// Incoming MCP tool call request body for `POST /v1/mcp/tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// A single content block in an MCP tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// MCP tool call response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContentBlock>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

/// An externally registered MCP tool with a callback URL for dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMcpTool {
    /// Tool definition (name, description, input_schema).
    pub tool: McpToolDescription,
    /// HTTP endpoint to POST tool calls to.
    pub callback_url: String,
    /// Source service that registered this tool.
    pub source: String,
}

/// Request body for POST /v1/mcp/tools (register external tool).
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterMcpToolRequest {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    pub callback_url: String,
    #[serde(default)]
    pub source: Option<String>,
}
