use uuid::Uuid;

use super::types::{McpContentBlock, McpToolResult};

pub(crate) fn success_result(value: serde_json::Value) -> McpToolResult {
    McpToolResult {
        content: vec![McpContentBlock {
            content_type: "text".to_string(),
            text: serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
        }],
        is_error: false,
    }
}

pub(crate) fn error_result(message: String) -> McpToolResult {
    McpToolResult {
        content: vec![McpContentBlock {
            content_type: "text".to_string(),
            text: serde_json::json!({"error": message}).to_string(),
        }],
        is_error: true,
    }
}

/// Maximum length for MCP tool string arguments (10 KB).
/// Prevents forwarding multi-megabyte payloads to consumer bridges.
const MAX_ARG_STRING_LEN: usize = 10_240;

pub(crate) fn get_string_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s[..s.len().min(MAX_ARG_STRING_LEN)].to_string())
}

pub(crate) fn get_optional_string_arg(args: &serde_json::Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| if v.is_null() { None } else { v.as_str() })
        .map(|s| s[..s.len().min(MAX_ARG_STRING_LEN)].to_string())
}

// ---------------------------------------------------------------------------
// H24: Consolidated JSON validation helpers (replacing 35+ duplicated patterns)
// ---------------------------------------------------------------------------

/// Extract a required string field from MCP tool arguments, returning an
/// `McpToolResult` error if the field is missing or not a string.
pub(crate) fn extract_required_string(
    args: &serde_json::Value,
    field: &str,
) -> Result<String, McpToolResult> {
    get_string_arg(args, field)
        .ok_or_else(|| error_result(format!("Missing required argument: {}", field)))
}

/// Extract a required string field and parse it as a UUID, returning an
/// `McpToolResult` error for missing fields or invalid UUIDs.
pub(crate) fn extract_required_uuid(
    args: &serde_json::Value,
    field: &str,
) -> Result<Uuid, McpToolResult> {
    let raw = extract_required_string(args, field)?;
    Uuid::parse_str(&raw)
        .map_err(|_| error_result(format!("Invalid UUID for '{}': {}", field, raw)))
}

/// Extract an optional unsigned integer field from MCP tool arguments.
pub(crate) fn extract_optional_u64(args: &serde_json::Value, field: &str, default: u64) -> u64 {
    args.get(field).and_then(|v| v.as_u64()).unwrap_or(default)
}

/// Validate that an optional string value belongs to an allowed set.
pub(crate) fn validate_enum_opt(
    value: &Option<String>,
    field: &str,
    allowed: &[&str],
) -> Result<(), McpToolResult> {
    if let Some(ref v) = value {
        if !allowed.contains(&v.as_str()) {
            return Err(error_result(format!(
                "Invalid {} '{}': must be {}",
                field,
                v,
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}
