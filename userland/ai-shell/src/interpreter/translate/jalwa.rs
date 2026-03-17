use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_jalwa(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::JalwaPlay { path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "path", path);
            Ok(mcp_call(
                "jalwa_play",
                a,
                format!("Play: {path}"),
                PermissionLevel::SystemWrite,
                "Plays a media file via Jalwa media player".to_string(),
            ))
        }
        Intent::JalwaPause => {
            let a = serde_json::Map::new();
            Ok(mcp_call(
                "jalwa_pause",
                a,
                "Pause playback".to_string(),
                PermissionLevel::SystemWrite,
                "Pauses current playback in Jalwa".to_string(),
            ))
        }
        Intent::JalwaStatus => {
            let a = serde_json::Map::new();
            Ok(mcp_call(
                "jalwa_status",
                a,
                "Playback status".to_string(),
                PermissionLevel::Safe,
                "Gets current playback status from Jalwa".to_string(),
            ))
        }
        Intent::JalwaSearch { query } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "query", query);
            Ok(mcp_call(
                "jalwa_search",
                a,
                format!("Search library: {query}"),
                PermissionLevel::Safe,
                "Searches the Jalwa media library".to_string(),
            ))
        }
        Intent::JalwaRecommend { item_id, max } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "item_id", item_id);
            if let Some(m) = max {
                a.insert("max".to_string(), serde_json::Value::Number((*m).into()));
            }
            Ok(mcp_call(
                "jalwa_recommend",
                a,
                "Get recommendations".to_string(),
                PermissionLevel::Safe,
                "Gets AI-powered media recommendations from Jalwa".to_string(),
            ))
        }
        Intent::JalwaQueue { action, item_id } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "item_id", item_id);
            Ok(mcp_call(
                "jalwa_queue",
                a,
                format!("Queue: {action}"),
                PermissionLevel::SystemWrite,
                "Manages the Jalwa play queue".to_string(),
            ))
        }
        Intent::JalwaLibrary { action, path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "path", path);
            Ok(mcp_call(
                "jalwa_library",
                a,
                format!("Library: {action}"),
                PermissionLevel::Safe,
                "Manages the Jalwa media library".to_string(),
            ))
        }
        Intent::JalwaPlaylist {
            action,
            name,
            item_id,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            insert_opt(&mut a, "item_id", item_id);
            Ok(mcp_call(
                "jalwa_playlist",
                a,
                format!("Playlist: {action}"),
                PermissionLevel::SystemWrite,
                "Manages Jalwa playlists".to_string(),
            ))
        }
        _ => unreachable!("translate_jalwa called with non-jalwa intent"),
    }
}
