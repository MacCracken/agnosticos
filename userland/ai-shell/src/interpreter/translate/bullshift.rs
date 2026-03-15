use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_bullshift(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::BullShiftPortfolio { action, period } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "period", period);
            Ok(mcp_call(
                "bullshift_portfolio",
                a,
                format!("BullShift portfolio: {}", action),
                PermissionLevel::Safe,
                format!("Views portfolio {} via BullShift MCP bridge", action),
            ))
        }

        Intent::BullShiftOrders {
            action,
            symbol,
            side,
        } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "symbol", symbol);
            insert_opt(&mut a, "side", side);
            Ok(mcp_call(
                "bullshift_orders",
                a,
                format!("BullShift order: {}", action),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} orders via BullShift MCP bridge",
                    match action.as_str() {
                        "place" => "Places",
                        "cancel" => "Cancels",
                        "list" => "Lists",
                        "status" => "Checks",
                        _ => "Manages",
                    }
                ),
            ))
        }

        Intent::BullShiftMarket { action, symbol } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "symbol", symbol);
            Ok(mcp_call(
                "bullshift_market",
                a,
                format!(
                    "BullShift market: {}{}",
                    action,
                    symbol
                        .as_ref()
                        .map_or(String::new(), |s| format!(" for {}", s))
                ),
                PermissionLevel::Safe,
                "Queries market data via BullShift MCP bridge".to_string(),
            ))
        }

        Intent::BullShiftAlerts { action, symbol } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "symbol", symbol);
            Ok(mcp_call(
                "bullshift_alerts",
                a,
                format!("BullShift alert: {}", action),
                match action.as_str() {
                    "list" | "triggered" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
                    "{} price alerts via BullShift MCP bridge",
                    match action.as_str() {
                        "set" => "Sets",
                        "remove" => "Removes",
                        "list" => "Lists",
                        "triggered" => "Lists triggered",
                        _ => "Manages",
                    }
                ),
            ))
        }

        Intent::BullShiftStrategy { action, name } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "name", name);
            Ok(mcp_call(
                "bullshift_strategy",
                a,
                format!("BullShift strategy: {}", action),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                format!(
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
            ))
        }

        Intent::BullShiftAccounts { action, broker } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "broker", broker);
            Ok(mcp_call(
                "bullshift_accounts",
                a,
                format!(
                    "BullShift accounts: {}{}",
                    action,
                    broker
                        .as_ref()
                        .map_or(String::new(), |b| format!(" '{}'", b))
                ),
                PermissionLevel::Safe,
                "Views broker accounts via BullShift".to_string(),
            ))
        }

        Intent::BullShiftHistory { action, period } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "period", period);
            Ok(mcp_call(
                "bullshift_history",
                a,
                format!(
                    "BullShift history: {}{}",
                    action,
                    period
                        .as_ref()
                        .map_or(String::new(), |p| format!(" ({})", p))
                ),
                PermissionLevel::Safe,
                "Views trade history via BullShift".to_string(),
            ))
        }

        _ => unreachable!("translate_bullshift called with non-bullshift intent"),
    }
}
