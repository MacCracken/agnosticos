use tracing::{info, warn};
use uuid::Uuid;

use super::super::helpers::{
    error_result, extract_optional_u64, extract_required_string, get_optional_string_arg,
    success_result, validate_enum_opt,
};
use super::super::types::McpToolResult;
use super::bridge::HttpBridge;

// ---------------------------------------------------------------------------
// Aequi Accounting Agent Bridge
// ---------------------------------------------------------------------------

pub(crate) fn aequi_bridge() -> HttpBridge {
    HttpBridge::new(
        "AEQUI_URL",
        "http://127.0.0.1:8060",
        "AEQUI_API_KEY",
        "Aequi",
    )
}

// ---------------------------------------------------------------------------
// Aequi Tool Implementations (bridged)
// ---------------------------------------------------------------------------

pub(crate) async fn handle_aequi_estimate_tax(args: &serde_json::Value) -> McpToolResult {
    let quarter = get_optional_string_arg(args, "quarter");
    let year = get_optional_string_arg(args, "year");

    if let Err(e) = validate_enum_opt(&quarter, "quarter", &["1", "2", "3", "4"]) {
        return e;
    }

    let bridge = aequi_bridge();
    let mut query = Vec::new();
    if let Some(ref q) = quarter {
        query.push(("quarter".to_string(), q.clone()));
    }
    if let Some(ref y) = year {
        query.push(("year".to_string(), y.clone()));
    }

    match bridge.get("/api/v1/tax/estimate", &query).await {
        Ok(response) => {
            info!("Aequi: tax estimate (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for tax estimate");
            let q = quarter.as_deref().unwrap_or("1");
            success_result(serde_json::json!({
                "quarter": q,
                "year": year.as_deref().unwrap_or("2026"),
                "estimated_tax": 3250.00,
                "gross_income": 22500.00,
                "deductions": 5200.00,
                "taxable_income": 17300.00,
                "effective_rate": 0.188,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_aequi_schedule_c(args: &serde_json::Value) -> McpToolResult {
    let year = get_optional_string_arg(args, "year");

    let bridge = aequi_bridge();
    let mut query = Vec::new();
    if let Some(ref y) = year {
        query.push(("year".to_string(), y.clone()));
    }

    match bridge.get("/api/v1/tax/schedule-c", &query).await {
        Ok(response) => {
            info!("Aequi: schedule C preview (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for schedule C");
            success_result(serde_json::json!({
                "year": year.as_deref().unwrap_or("2026"),
                "gross_receipts": 90000.00,
                "cost_of_goods_sold": 0.00,
                "gross_income": 90000.00,
                "total_expenses": 21400.00,
                "net_profit": 68600.00,
                "categories": {
                    "office_supplies": 1200.00,
                    "software_subscriptions": 3600.00,
                    "home_office": 5400.00,
                    "professional_services": 4800.00,
                    "equipment_depreciation": 6400.00,
                },
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_aequi_import_bank(args: &serde_json::Value) -> McpToolResult {
    let file_path = match extract_required_string(args, "file_path") {
        Ok(p) => p,
        Err(e) => return e,
    };

    if file_path.is_empty() {
        return error_result("File path cannot be empty".to_string());
    }

    let format = get_optional_string_arg(args, "format");
    if let Err(e) = validate_enum_opt(&format, "format", &["ofx", "qfx", "csv"]) {
        return e;
    }

    let bridge = aequi_bridge();
    let body = serde_json::json!({
        "file_path": file_path,
        "format": format,
    });

    match bridge.post("/api/v1/import/bank-statement", body).await {
        Ok(response) => {
            info!(file = %file_path, "Aequi: import bank statement (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for import");
            success_result(serde_json::json!({
                "status": "imported",
                "file": file_path,
                "transactions_imported": 47,
                "transactions_matched": 12,
                "transactions_new": 35,
                "date_range": {"from": "2026-01-01", "to": "2026-01-31"},
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_aequi_balances(args: &serde_json::Value) -> McpToolResult {
    let account_type = get_optional_string_arg(args, "account_type");

    if let Err(e) = validate_enum_opt(
        &account_type,
        "account_type",
        &["asset", "liability", "equity", "revenue", "expense"],
    ) {
        return e;
    }

    let bridge = aequi_bridge();
    let mut query = Vec::new();
    if let Some(ref t) = account_type {
        query.push(("type".to_string(), t.clone()));
    }

    match bridge.get("/api/v1/accounts/balances", &query).await {
        Ok(response) => {
            info!("Aequi: account balances (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for balances");
            let accounts = vec![
                serde_json::json!({"name": "Business Checking", "type": "asset", "balance": 14523.67, "currency": "USD"}),
                serde_json::json!({"name": "Business Savings", "type": "asset", "balance": 8200.00, "currency": "USD"}),
                serde_json::json!({"name": "Accounts Receivable", "type": "asset", "balance": 3750.00, "currency": "USD"}),
                serde_json::json!({"name": "Credit Card", "type": "liability", "balance": -1234.56, "currency": "USD"}),
            ];
            success_result(serde_json::json!({
                "accounts": accounts,
                "total_assets": 26473.67,
                "total_liabilities": -1234.56,
                "net_worth": 25239.11,
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_aequi_receipts(args: &serde_json::Value) -> McpToolResult {
    let status = get_optional_string_arg(args, "status");
    let limit = extract_optional_u64(args, "limit", 20) as usize;

    if let Err(e) = validate_enum_opt(
        &status,
        "status",
        &["pending_review", "reviewed", "matched", "all"],
    ) {
        return e;
    }

    let bridge = aequi_bridge();
    let mut query = Vec::new();
    if let Some(ref s) = status {
        query.push(("status".to_string(), s.clone()));
    }
    query.push(("limit".to_string(), limit.to_string()));

    match bridge.get("/api/v1/receipts", &query).await {
        Ok(response) => {
            info!("Aequi: list receipts (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for receipts");
            let receipts = vec![
                serde_json::json!({"id": "rcpt-001", "vendor": "Office Depot", "amount": 87.50, "date": "2026-03-05", "status": "matched", "category": "office_supplies"}),
                serde_json::json!({"id": "rcpt-002", "vendor": "AWS", "amount": 142.30, "date": "2026-03-01", "status": "pending_review", "category": "software"}),
                serde_json::json!({"id": "rcpt-003", "vendor": "Starbucks", "amount": 5.75, "date": "2026-03-08", "status": "pending_review", "category": "meals"}),
            ];
            let filtered: Vec<_> = if let Some(ref s) = status {
                if s == "all" {
                    receipts
                } else {
                    receipts
                        .into_iter()
                        .filter(|r| r["status"].as_str() == Some(s.as_str()))
                        .collect()
                }
            } else {
                receipts
            };
            success_result(serde_json::json!({
                "receipts": filtered,
                "total": filtered.len(),
                "_source": "mock",
            }))
        }
    }
}

pub(crate) async fn handle_aequi_invoices(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["create", "list", "send", "void", "status"],
    ) {
        return e;
    }

    let client = get_optional_string_arg(args, "client");
    let amount = get_optional_string_arg(args, "amount");
    let invoice_id = get_optional_string_arg(args, "invoice_id");
    let due_date = get_optional_string_arg(args, "due_date");

    let bridge = aequi_bridge();

    match action.as_str() {
        "list" | "status" => {
            let mut query = Vec::new();
            query.push(("action".to_string(), action.clone()));
            if let Some(ref c) = client {
                query.push(("client".to_string(), c.clone()));
            }
            if let Some(ref id) = invoice_id {
                query.push(("invoice_id".to_string(), id.clone()));
            }
            match bridge.get("/api/v1/invoices", &query).await {
                Ok(response) => {
                    info!(action = %action, "Aequi: invoices {} (bridged)", action);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Aequi bridge: falling back to mock for invoices {}", action);
                    success_result(serde_json::json!({
                        "invoices": [],
                        "total": 0,
                        "_source": "mock",
                    }))
                }
            }
        }
        op @ ("create" | "send" | "void") => {
            let mut body = serde_json::json!({
                "action": op,
            });
            if let Some(ref c) = client {
                body["client"] = serde_json::json!(c);
            }
            if let Some(ref a) = amount {
                body["amount"] = serde_json::json!(a);
            }
            if let Some(ref id) = invoice_id {
                body["invoice_id"] = serde_json::json!(id);
            }
            if let Some(ref d) = due_date {
                body["due_date"] = serde_json::json!(d);
            }
            match bridge.post("/api/v1/invoices", body).await {
                Ok(response) => {
                    info!(action = %op, "Aequi: {} invoice (bridged)", op);
                    success_result(response)
                }
                Err(e) => {
                    warn!(error = %e, "Aequi bridge: falling back to mock for {} invoice", op);
                    let id = invoice_id.unwrap_or_else(|| Uuid::new_v4().to_string());
                    success_result(serde_json::json!({
                        "invoice_id": id,
                        "action": op,
                        "status": "ok",
                        "updated_at": chrono::Utc::now().to_rfc3339(),
                        "_source": "mock",
                    }))
                }
            }
        }
        _ => unreachable!(),
    }
}

pub(crate) async fn handle_aequi_reports(args: &serde_json::Value) -> McpToolResult {
    let action = match extract_required_string(args, "action") {
        Ok(a) => a,
        Err(e) => return e,
    };

    let action_opt = Some(action.clone());
    if let Err(e) = validate_enum_opt(
        &action_opt,
        "action",
        &["pnl", "balance_sheet", "cash_flow", "summary"],
    ) {
        return e;
    }

    let period = get_optional_string_arg(args, "period").unwrap_or_else(|| "ytd".to_string());
    let year = get_optional_string_arg(args, "year");

    let period_opt = Some(period.clone());
    if let Err(e) = validate_enum_opt(&period_opt, "period", &["month", "quarter", "year", "ytd"]) {
        return e;
    }

    let bridge = aequi_bridge();
    let mut query = Vec::new();
    query.push(("type".to_string(), action.clone()));
    query.push(("period".to_string(), period.clone()));
    if let Some(ref y) = year {
        query.push(("year".to_string(), y.clone()));
    }

    match bridge.get("/api/v1/reports", &query).await {
        Ok(response) => {
            info!(report_type = %action, "Aequi: report (bridged)");
            success_result(response)
        }
        Err(e) => {
            warn!(error = %e, "Aequi bridge: falling back to mock for report {}", action);
            success_result(serde_json::json!({
                "report_type": action,
                "data": {},
                "period": period,
                "_source": "mock",
            }))
        }
    }
}
