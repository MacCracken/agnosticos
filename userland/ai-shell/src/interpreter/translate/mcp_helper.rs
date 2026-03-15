use crate::interpreter::intent::Translation;
use crate::security::PermissionLevel;

/// Build a Translation that calls an MCP tool via curl to the daimon API.
///
/// This eliminates ~30 lines of boilerplate per MCP tool call across all
/// consumer project translator files. Every MCP call follows the same
/// pattern: POST JSON to `http://127.0.0.1:8090/v1/mcp/tools/call`.
pub(crate) fn mcp_call(
    tool_name: &str,
    args: serde_json::Map<String, serde_json::Value>,
    description: String,
    permission: PermissionLevel,
    explanation: String,
) -> Translation {
    let body = serde_json::json!({"name": tool_name, "arguments": args});
    Translation {
        command: "curl".to_string(),
        args: vec![
            "-s".to_string(),
            "-X".to_string(),
            "POST".to_string(),
            "http://127.0.0.1:8090/v1/mcp/tools/call".to_string(),
            "-H".to_string(),
            "Content-Type: application/json".to_string(),
            "-d".to_string(),
            serde_json::to_string(&body).unwrap(),
        ],
        description,
        permission,
        explanation,
    }
}

/// Convenience: insert an optional string into an args map.
pub(crate) fn insert_opt(
    args: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &Option<String>,
) {
    if let Some(v) = value {
        args.insert(key.to_string(), serde_json::Value::String(v.clone()));
    }
}

/// Convenience: insert a required string into an args map.
pub(crate) fn insert_str(
    args: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: &str,
) {
    args.insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
}
