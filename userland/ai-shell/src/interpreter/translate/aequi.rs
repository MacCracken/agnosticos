use anyhow::Result;

use super::mcp_helper::{insert_opt, insert_str, mcp_call};
use crate::interpreter::intent::{Intent, Translation};
use crate::security::PermissionLevel;

pub(crate) fn translate_aequi(intent: &Intent) -> Result<Translation> {
    match intent {
        Intent::AequiTaxEstimate { quarter } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "quarter", quarter);
            Ok(mcp_call(
                "aequi_estimate_quarterly_tax",
                a,
                format!(
                    "Quarterly tax estimate{}",
                    quarter
                        .as_ref()
                        .map_or(String::new(), |q| format!(" for Q{}", q))
                ),
                PermissionLevel::Safe,
                "Calculates quarterly tax estimate via Aequi MCP bridge".to_string(),
            ))
        }

        Intent::AequiScheduleC { year } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "year", year);
            Ok(mcp_call(
                "aequi_schedule_c_preview",
                a,
                format!(
                    "Schedule C preview{}",
                    year.as_ref()
                        .map_or(String::new(), |y| format!(" for {}", y))
                ),
                PermissionLevel::Safe,
                "Retrieves Schedule C preview from Aequi via MCP bridge".to_string(),
            ))
        }

        Intent::AequiImportBank { file_path } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "file_path", file_path);
            Ok(mcp_call(
                "aequi_import_bank_statement",
                a,
                format!("Import bank statement: {}", file_path),
                PermissionLevel::SystemWrite,
                "Imports a bank statement file into Aequi via MCP bridge".to_string(),
            ))
        }

        Intent::AequiBalance => Ok(mcp_call(
            "aequi_account_balances",
            serde_json::Map::new(),
            "Show account balances".to_string(),
            PermissionLevel::Safe,
            "Retrieves account balances from Aequi via MCP bridge".to_string(),
        )),

        Intent::AequiReceipts { status } => {
            let mut a = serde_json::Map::new();
            insert_opt(&mut a, "status", status);
            Ok(mcp_call(
                "aequi_list_receipts",
                a,
                format!(
                    "List receipts{}",
                    status
                        .as_ref()
                        .map_or(String::new(), |s| format!(" ({})", s))
                ),
                PermissionLevel::Safe,
                "Lists receipts from Aequi via MCP bridge".to_string(),
            ))
        }

        Intent::AequiInvoices { action, client } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "client", client);
            Ok(mcp_call(
                "aequi_invoices",
                a,
                format!(
                    "Aequi invoices: {}{}",
                    action,
                    client
                        .as_ref()
                        .map_or(String::new(), |c| format!(" for '{}'", c))
                ),
                match action.as_str() {
                    "list" | "status" => PermissionLevel::Safe,
                    _ => PermissionLevel::SystemWrite,
                },
                "Manages invoices via Aequi".to_string(),
            ))
        }

        Intent::AequiReports { action, period } => {
            let mut a = serde_json::Map::new();
            insert_str(&mut a, "action", action);
            insert_opt(&mut a, "period", period);
            Ok(mcp_call(
                "aequi_reports",
                a,
                format!(
                    "Aequi report: {}{}",
                    action,
                    period
                        .as_ref()
                        .map_or(String::new(), |p| format!(" ({})", p))
                ),
                PermissionLevel::Safe,
                "Generates financial report via Aequi".to_string(),
            ))
        }

        _ => unreachable!("translate_aequi called with non-aequi intent"),
    }
}
