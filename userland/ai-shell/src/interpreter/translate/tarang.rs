use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_tarang(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::TarangProbe { path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "path", path);
            Ok(mcp_call("tarang_probe", a, format!("Probe media: {path}"), PermissionLevel::Safe, "Probes a media file for format, codec, and stream info via Tarang".to_string()))
        }
        Intent::TarangAnalyze { path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "path", path);
            Ok(mcp_call("tarang_analyze", a, format!("Analyze media: {path}"), PermissionLevel::Safe, "AI-powered media content analysis via Tarang".to_string()))
        }
        Intent::TarangCodecs => {
            let a = serde_json::Map::new();
            Ok(mcp_call("tarang_codecs", a, "List supported codecs".to_string(), PermissionLevel::Safe, "Lists all audio and video codecs supported by Tarang".to_string()))
        }
        Intent::TarangTranscribe { path, language } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "path", path);
            insert_opt(&mut a, "language", language);
            Ok(mcp_call("tarang_transcribe", a, format!("Transcribe: {path}"), PermissionLevel::SystemWrite, "Prepares audio transcription request via Tarang (routes to hoosh)".to_string()))
        }
        Intent::TarangFormats { path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "path", path);
            Ok(mcp_call("tarang_formats", a, format!("Detect format: {path}"), PermissionLevel::Safe, "Detects media container format from file header via Tarang".to_string()))
        }
        _ => unreachable!("translate_tarang called with non-tarang intent"),
    }
}
