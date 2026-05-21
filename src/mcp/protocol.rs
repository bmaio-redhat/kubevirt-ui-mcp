use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 wire types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl Response {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
    }

    pub fn err(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError { code, message: message.into() }),
        }
    }
}

// ── Tool definition (for tools/list) ─────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

// ── MCP-level types ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub tools: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: Capabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
pub struct ContentItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub content: Vec<ContentItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

impl ToolCallResult {
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            content: vec![ContentItem { kind: "text".into(), text: s.into() }],
            is_error: None,
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        Self {
            content: vec![ContentItem { kind: "text".into(), text: s.into() }],
            is_error: Some(true),
        }
    }

    pub fn into_value(self) -> Value {
        serde_json::to_value(self).unwrap_or(serde_json::json!({"content":[]}))
    }
}
