use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_required_string, get_optional_string_arg, success_result,
    validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// BullShift Trading Platform Agent Bridge
// ---------------------------------------------------------------------------

pub(crate) fn bullshift_bridge() -> HttpBridge {
    HttpBridge::new(
        "BULLSHIFT_URL",
        "http://127.0.0.1:8075",
        "BULLSHIFT_API_KEY",
        "BullShift",
    )
}

// ---------------------------------------------------------------------------
// BullShift Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_bullshift_portfolio(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["summary", "positions", "history", "pnl"],
    ) {
        return e;
    }

    let account = get_optional_string_arg(args, "account");
    let period = get_optional_string_arg(args, "period");

    if let Err(e) = validate_enum_opt(&period, "period", &["1d", "1w", "1m", "3m", "1y", "all"]) {
        return e;
    }

    let bridge = bullshift_bridge();
    let mut query = vec![("action".to_string(), action.clone())];
    if let Some(ref a) = account {
        query.push(("account".to_string(), a.clone()));
    }
    if let Some(ref p) = period {
        query.push(("period".to_string(), p.clone()));
    }

    match bridge.get("/api/v1/portfolio", &query).await {
        Ok(response) => {
            info!("BullShift: {} portfolio (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "BullShift bridge: falling back to mock for portfolio {}", action);
            success_result(serde_json::json!({
                "total_value": 0.0,
                "positions": 0,
                "day_pnl": 0.0,
                "currency": "USD",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_bullshift_orders(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["place", "cancel", "list", "status"],
    ) {
        return e;
    }

    let symbol = get_optional_string_arg(args, "symbol");
    let side = get_optional_string_arg(args, "side");
    let quantity = get_optional_string_arg(args, "quantity");
    let order_type = get_optional_string_arg(args, "order_type");
    let price = get_optional_string_arg(args, "price");
    let order_id = get_optional_string_arg(args, "order_id");

    if let Err(e) = validate_enum_opt(&side, "side", &["buy", "sell"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(&order_type, "order_type", &["market", "limit", "stop"]) {
        return e;
    }

    let bridge = bullshift_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = vec![("action".to_string(), action.clone())];
            if let Some(ref id) = order_id {
                query.push(("order_id".to_string(), id.clone()));
            }
            if let Some(ref s) = symbol {
                query.push(("symbol".to_string(), s.clone()));
            }
            match bridge.get("/api/v1/orders", &query).await {
                Ok(response) => {
                    info!("BullShift: {} orders (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "BullShift bridge: falling back to mock for orders {}", action);
                    success_result(serde_json::json!({
                        "orders": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("place" | "cancel") => {
            let body = serde_json::json!({
                "action": op,
                "symbol": symbol,
                "side": side,
                "quantity": quantity,
                "order_type": order_type,
                "price": price,
                "order_id": order_id,
            });
            match bridge.post("/api/v1/orders", body).await {
                Ok(response) => {
                    info!(action = %op, "BullShift: {} order (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "BullShift bridge: falling back to mock for {} order", op);
                    let id = order_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "order_id": id,
                        "action": op,
                        "symbol": symbol,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_bullshift_market(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["quote", "search", "watchlist", "history"],
    ) {
        return e;
    }

    let symbol = get_optional_string_arg(args, "symbol");
    let query_term = get_optional_string_arg(args, "query");
    let period = get_optional_string_arg(args, "period");

    if let Err(e) = validate_enum_opt(&period, "period", &["1d", "1w", "1m"]) {
        return e;
    }

    let bridge = bullshift_bridge();
    let mut query = vec![("action".to_string(), action.clone())];
    if let Some(ref s) = symbol {
        query.push(("symbol".to_string(), s.clone()));
    }
    if let Some(ref q) = query_term {
        query.push(("query".to_string(), q.clone()));
    }
    if let Some(ref p) = period {
        query.push(("period".to_string(), p.clone()));
    }

    match bridge.get("/api/v1/market", &query).await {
        Ok(response) => {
            info!("BullShift: {} market (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "BullShift bridge: falling back to mock for market {}", action);
            let sym = symbol.unwrap_or_else(|| "UNKNOWN".to_string());
            success_result(serde_json::json!({
                "symbol": sym,
                "price": 0.0,
                "change": 0.0,
                "message": "BullShift not reachable",
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_bullshift_alerts(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["set", "remove", "list", "triggered"],
    ) {
        return e;
    }

    let symbol = get_optional_string_arg(args, "symbol");
    let condition = get_optional_string_arg(args, "condition");
    let value = get_optional_string_arg(args, "value");
    let alert_id = get_optional_string_arg(args, "alert_id");

    if let Err(e) = validate_enum_opt(
        &condition,
        "condition",
        &["above", "below", "percent_change"],
    ) {
        return e;
    }

    let bridge = bullshift_bridge();

    match action.as_str() {
        "list" | "triggered" => {
            let mut query = vec![("action".to_string(), action.clone())];
            if let Some(ref s) = symbol {
                query.push(("symbol".to_string(), s.clone()));
            }
            match bridge.get("/api/v1/alerts", &query).await {
                Ok(response) => {
                    info!("BullShift: {} alerts (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "BullShift bridge: falling back to mock for alerts {}", action);
                    success_result(serde_json::json!({
                        "alerts": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("set" | "remove") => {
            let body = serde_json::json!({
                "action": op,
                "symbol": symbol,
                "condition": condition,
                "value": value,
                "alert_id": alert_id,
            });
            match bridge.post("/api/v1/alerts", body).await {
                Ok(response) => {
                    info!(action = %op, "BullShift: {} alert (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "BullShift bridge: falling back to mock for {} alert", op);
                    let id = alert_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "alert_id": id,
                        "action": op,
                        "symbol": symbol,
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_bullshift_strategy(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["list", "start", "stop", "backtest", "status"],
    ) {
        return e;
    }

    let name = get_optional_string_arg(args, "name");
    let params = get_optional_string_arg(args, "params");

    let bridge = bullshift_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = vec![("action".to_string(), action.clone())];
            if let Some(ref n) = name {
                query.push(("name".to_string(), n.clone()));
            }
            match bridge.get("/api/v1/strategies", &query).await {
                Ok(response) => {
                    info!("BullShift: {} strategies (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "BullShift bridge: falling back to mock for strategies {}", action);
                    success_result(serde_json::json!({
                        "strategies": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("start" | "stop" | "backtest") => {
            let params_json: Option<serde_json::Value> = match &params {
                Some(p) => match serde_json::from_str(p) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        warn!("BullShift: invalid JSON in strategy params");
                        return error_result("Invalid JSON in 'params' argument".to_string());
                    }
                },
                None => None,
            };
            let body = serde_json::json!({
                "action": op,
                "name": name,
                "params": params_json,
            });
            match bridge.post("/api/v1/strategies", body).await {
                Ok(response) => {
                    info!(action = %op, "BullShift: {} strategy (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "BullShift bridge: falling back to mock for {} strategy", op);
                    let strategy_id = Uuid::new_v4().to_string();
                    success_result(serde_json::json!({
                        "id": strategy_id,
                        "action": op,
                        "name": name.unwrap_or_else(|| "unnamed".to_string()),
                        "status": "ok",
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_bullshift_accounts(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(&action_opt, "action", &["list", "switch", "status", "info"])
    {
        return e;
    }

    let account_id = get_optional_string_arg(args, "account_id");
    let broker = get_optional_string_arg(args, "broker");

    let bridge = bullshift_bridge();
    let mut query = vec![("action".to_string(), action.clone())];
    if let Some(ref id) = account_id {
        query.push(("account_id".to_string(), id.clone()));
    }
    if let Some(ref b) = broker {
        query.push(("broker".to_string(), b.clone()));
    }

    match bridge.get("/api/v1/accounts", &query).await {
        Ok(response) => {
            info!("BullShift: {} accounts (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "BullShift bridge: falling back to mock for accounts {}", action);
            success_result(serde_json::json!({
                "accounts": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_bullshift_history(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["trades", "dividends", "tax_report", "export"],
    ) {
        return e;
    }

    let period = get_optional_string_arg(args, "period");
    let format = get_optional_string_arg(args, "format");

    if let Err(e) = validate_enum_opt(&period, "period", &["1d", "1w", "1m", "3m", "1y", "all"]) {
        return e;
    }
    if let Err(e) = validate_enum_opt(&format, "format", &["json", "csv"]) {
        return e;
    }

    let bridge = bullshift_bridge();
    let mut query = vec![("action".to_string(), action.clone())];
    if let Some(ref p) = period {
        query.push(("period".to_string(), p.clone()));
    }
    if let Some(ref f) = format {
        query.push(("format".to_string(), f.clone()));
    }

    match bridge.get("/api/v1/history", &query).await {
        Ok(response) => {
            info!("BullShift: {} history (bridged)", action);
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "BullShift bridge: falling back to mock for history {}", action);
            success_result(serde_json::json!({
                "trades": [],
                "total": 0,
                "_source": "mock",
            }))
        }
    }
}
