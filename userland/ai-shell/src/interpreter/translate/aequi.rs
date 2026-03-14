use anyhow::Result;

use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_aequi(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::AequiTaxEstimate { quarter } => {
            let mut args_json = serde_json::Map::new();
            if let Some(q) = quarter {
                args_json.insert("quarter".to_string(), serde_json::Value::String(q.clone()));
            }
            let body =
                serde_json::json!({"name": "aequi_estimate_quarterly_tax", "arguments": args_json});
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
                    "Quarterly tax estimate{}",
                    quarter
                        .as_ref()
                        .map_or(String::new(), |q| format!(" for Q{}", q))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Calculates quarterly tax estimate via Aequi MCP bridge".to_string(),
            })
        }

        Intent::AequiScheduleC { year } => {
            let mut args_json = serde_json::Map::new();
            if let Some(y) = year {
                args_json.insert("year".to_string(), serde_json::Value::String(y.clone()));
            }
            let body =
                serde_json::json!({"name": "aequi_schedule_c_preview", "arguments": args_json});
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
                    "Schedule C preview{}",
                    year.as_ref()
                        .map_or(String::new(), |y| format!(" for {}", y))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Retrieves Schedule C preview from Aequi via MCP bridge".to_string(),
            })
        }

        Intent::AequiImportBank { file_path } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "file_path".to_string(),
                serde_json::Value::String(file_path.clone()),
            );
            let body =
                serde_json::json!({"name": "aequi_import_bank_statement", "arguments": args_json});
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
                description: format!("Import bank statement: {}", file_path),
                permission: PermissionLevel::SystemWrite,
                explanation: "Imports a bank statement file into Aequi via MCP bridge".to_string(),
            })
        }

        Intent::AequiBalance => {
            let body = serde_json::json!({"name": "aequi_account_balances", "arguments": {}});
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
                description: "Show account balances".to_string(),
                permission: PermissionLevel::Safe,
                explanation: "Retrieves account balances from Aequi via MCP bridge".to_string(),
            })
        }

        Intent::AequiReceipts { status } => {
            let mut args_json = serde_json::Map::new();
            if let Some(s) = status {
                args_json.insert("status".to_string(), serde_json::Value::String(s.clone()));
            }
            let body = serde_json::json!({"name": "aequi_list_receipts", "arguments": args_json});
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
                    "List receipts{}",
                    status
                        .as_ref()
                        .map_or(String::new(), |s| format!(" ({})", s))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Lists receipts from Aequi via MCP bridge".to_string(),
            })
        }

        Intent::AequiInvoices { action, client } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(c) = client {
                args_json.insert("client".to_string(), serde_json::Value::String(c.clone()));
            }
            let body = serde_json::json!({"name": "aequi_invoices", "arguments": args_json});
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
                    "Aequi invoices: {}{}",
                    action,
                    client
                        .as_ref()
                        .map_or(String::new(), |c| format!(" for '{}'", c))
                ),
                permission: match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                explanation: "Manages invoices via Aequi".to_string(),
            })
        }

        Intent::AequiReports { action, period } => {
            let mut args_json = serde_json::Map::new();
            args_json.insert(
                "action".to_string(),
                serde_json::Value::String(action.clone()),
            );
            if let Some(p) = period {
                args_json.insert("period".to_string(), serde_json::Value::String(p.clone()));
            }
            let body = serde_json::json!({"name": "aequi_reports", "arguments": args_json});
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
                    "Aequi report: {}{}",
                    action,
                    period
                        .as_ref()
                        .map_or(String::new(), |p| format!(" ({})", p))
                ),
                permission: PermissionLevel::Safe,
                explanation: "Generates financial report via Aequi".to_string(),
            })
        }

        _ => unreachable!("translate_aequi called with non-aequi intent"),
    }
}
