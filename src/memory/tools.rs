use serde_json::{Value, json};

use crate::memory::store::SharedStore;

// ── Tool result helper ────────────────────────────────────────────────────────

pub struct ToolResult {
    pub content: Vec<Value>,
    pub is_error: bool,
}

impl ToolResult {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": s.into() })],
            is_error: false,
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": s.into() })],
            is_error: true,
        }
    }

    pub fn into_value(self) -> Value {
        json!({
            "content": self.content,
            "isError": self.is_error,
        })
    }

    pub fn into_tool_call_result(self) -> crate::mcp::protocol::ToolCallResult {
        let text = self
            .content
            .first()
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if self.is_error {
            crate::mcp::protocol::ToolCallResult::error(text)
        } else {
            crate::mcp::protocol::ToolCallResult::text(text)
        }
    }
}

// ── Tool definitions (for tools/list) ────────────────────────────────────────

pub fn all_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "get_ticket",
            "description": "Return the full cached record for a CNV Jira ticket. Includes summary, status, type, labels, components, fix versions, description excerpt, and the list of kubevirt-plugin commits that reference it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Jira ticket key (e.g. 'CNV-87321')"
                    }
                },
                "required": ["key"]
            }
        }),
        json!({
            "name": "search_tickets",
            "description": "Full-text search across all cached CNV tickets. Matches against summary, description excerpt, labels, and components. Returns matching tickets ordered by key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search term (e.g. 'snapshot', 'migration', 'RBAC')"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "list_tickets",
            "description": "Return a compact list of all cached CNV tickets with key, summary, status, type, and fix version. Useful for browsing the backlog.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "refresh_store",
            "description": "Re-fetch commit history from kubevirt-plugin and refresh stale Jira ticket data. Rebuilds and persists the store. May take up to a minute.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
    ]
}

// ── Tool implementations ──────────────────────────────────────────────────────

pub async fn get_ticket(store: &SharedStore, params: &Value) -> ToolResult {
    let key = match params.get("key").and_then(|v| v.as_str()) {
        Some(k) => k.to_uppercase(),
        None => return ToolResult::error("Missing required parameter: key"),
    };

    let store = store.read().await;
    match store.get(&key) {
        Some(record) => {
            let text = format!(
                "**{}** — {}\n\
                 Status: {} | Type: {} | Fix: {}\n\
                 Labels: {}\n\
                 Components: {}\n\
                 Description: {}\n\
                 Commits ({}): {}",
                record.key,
                record.summary,
                record.status,
                record.issue_type,
                record.fix_versions.join(", "),
                if record.labels.is_empty() { "(none)".into() } else { record.labels.join(", ") },
                if record.components.is_empty() { "(none)".into() } else { record.components.join(", ") },
                record.description_excerpt.trim(),
                record.commit_shas.len(),
                record.commit_shas.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
            );
            ToolResult::text(text)
        }
        None => ToolResult::error(format!("Ticket {} not found in store. Try refresh_store if it was recently added.", key)),
    }
}

pub async fn search_tickets(store: &SharedStore, params: &Value) -> ToolResult {
    let query = match params.get("query").and_then(|v| v.as_str()) {
        Some(q) => q.to_lowercase(),
        None => return ToolResult::error("Missing required parameter: query"),
    };

    let store = store.read().await;
    let mut matches: Vec<_> = store
        .tickets
        .values()
        .filter(|r| {
            r.summary.to_lowercase().contains(&query)
                || r.description_excerpt.to_lowercase().contains(&query)
                || r.labels.iter().any(|l| l.to_lowercase().contains(&query))
                || r.components.iter().any(|c| c.to_lowercase().contains(&query))
                || r.fix_versions.iter().any(|v| v.to_lowercase().contains(&query))
        })
        .collect();

    if matches.is_empty() {
        return ToolResult::text(format!("No tickets matched '{}'.", query));
    }

    matches.sort_by(|a, b| a.key.cmp(&b.key));

    let lines: Vec<String> = matches
        .iter()
        .map(|r| {
            format!(
                "**{}** [{}] {} — {}",
                r.key,
                r.issue_type,
                r.status,
                r.summary
            )
        })
        .collect();

    ToolResult::text(format!(
        "{} ticket(s) matched '{}':\n\n{}",
        lines.len(),
        query,
        lines.join("\n")
    ))
}

pub async fn list_tickets(store: &SharedStore) -> ToolResult {
    let store = store.read().await;
    if store.tickets.is_empty() {
        return ToolResult::text("Store is empty. Try refresh_store.");
    }

    let mut records: Vec<_> = store.tickets.values().collect();
    records.sort_by(|a, b| a.key.cmp(&b.key));

    let lines: Vec<String> = records
        .iter()
        .map(|r| {
            format!(
                "**{}** [{}] {} — {} ({})",
                r.key,
                r.issue_type,
                r.status,
                r.summary,
                r.fix_versions.first().map(|s| s.as_str()).unwrap_or("no version"),
            )
        })
        .collect();

    ToolResult::text(format!(
        "{} tickets in store:\n\n{}",
        lines.len(),
        lines.join("\n")
    ))
}
