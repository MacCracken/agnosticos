use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_bullshift(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::BullShiftPortfolio { action, period } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(p) = period {
                args_json.insert("period".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "bullshift_portfolio", "arguments": args_json});
            Ok(Translation {
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
                description: format!("BullShift portfolio: {}", action),
                permission: PermissionLevel::Safe,
                explanation: format!(
                    "Views portfolio {} via BullShift MCP bridge",
                    action
                ),
            })
        }

        Intent::BullShiftOrders {
            action,
            symbol,
            side,
        } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(s) = symbol {
                args_json.insert("symbol".to_string(), serde_json::Value::String(s.clone()));
            }
            if let Some(sd) = side {
                args_json.insert("side".to_string(), serde_json::Value::String(sd.clone()));
            }
            let body = serde_json::json!({"name": "bullshift_orders", "arguments": args_json});
            Ok(Translation {
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
                description: format!("BullShift order: {}", action),
                permission: match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} orders via BullShift MCP bridge",
                    match action.as_str() {
                        "place" => "Places",
                        "cancel" => "Cancels",
                        "list" => "Lists",
                        "status" => "Checks",
                        _ => "Manages",
                    }
                ),
            })
        }

        Intent::BullShiftMarket { action, symbol } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(s) = symbol {
                args_json.insert("symbol".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "bullshift_market", "arguments": args_json});
            Ok(Translation {
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
                description: format!(
                    "BullShift market: {}{}",
                    action,
                    symbol
                        .as_ref()
                        .map_or(String::new(), |s| format!(" for {}", s))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Queries market data via BullShift MCP bridge".to_string(),
            })
        }

        Intent::BullShiftAlerts { action, symbol } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(s) = symbol {
                args_json.insert("symbol".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "bullshift_alerts", "arguments": args_json});
            Ok(Translation {
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
                description: format!("BullShift alert: {}", action),
                permission: match action.as_str() {
                    "list" | "triggered" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} price alerts via BullShift MCP bridge",
                    match action.as_str() {
                        "set" => "Sets",
                        "remove" => "Removes",
                        "list" => "Lists",
                        "triggered" => "Lists triggered",
                        _ => "Manages",
                    }
                ),
            })
        }

        Intent::BullShiftStrategy { action, name } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(n) = name {
                args_json.insert("name".to_string(), serde_json::Value::String(n.clone()));
            }
            let body = serde_json::json!({"name": "bullshift_strategy", "arguments": args_json});
            Ok(Translation {
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
                description: format!("BullShift strategy: {}", action),
                permission: match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: format!(
                    "{} trading strategy via BullShift MCP bridge",
                    match action.as_str() {
                        "list" => "Lists",
                        "start" => "Starts",
                        "stop" => "Stops",
                        "backtest" => "Backtests",
                        "status" => "Checks status of",
                        _ => "Manages",
                    }
                ),
            })
        }

        _ => unreachable!("translate_bullshift called with non-bullshift intent"),
    }
}
