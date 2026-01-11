//! MCP protocol message types and handling.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP JSON-RPC request format.
#[derive(Debug, Clone, Deserialize)]
pub struct McpRequest {
    #[allow(dead_code)] // Required by JSON-RPC spec, validated implicitly
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// MCP JSON-RPC response format.
#[derive(Debug, Clone, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// MCP error format.
#[derive(Debug, Clone, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// MCP error codes (JSON-RPC standard + MCP specific)
// Some are reserved for future use
#[allow(dead_code)]
pub const PARSE_ERROR: i32 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

impl McpResponse {
    /// Create a successful response.
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    /// Create an error response with additional data.
    #[allow(dead_code)] // Reserved for future use
    pub fn error_with_data(id: Option<Value>, code: i32, message: impl Into<String>, data: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
                data: Some(data),
            }),
        }
    }
}

/// Server capabilities returned during initialization.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub protocol_version: String,
    pub capabilities: Capabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub tools: ToolCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilities {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

impl Default for ServerCapabilities {
    fn default() -> Self {
        Self {
            protocol_version: "2024-11-05".to_string(),
            capabilities: Capabilities {
                tools: ToolCapabilities { list_changed: false },
            },
            server_info: ServerInfo {
                name: "cmdr".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_request() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        }"#;

        let request: McpRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.method, "tools/list");
        assert_eq!(request.id, Some(json!(1)));
    }

    #[test]
    fn test_parse_request_without_params() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize"
        }"#;

        let request: McpRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.method, "initialize");
        assert_eq!(request.params, Value::Null);
    }

    #[test]
    fn test_success_response() {
        let response = McpResponse::success(Some(json!(1)), json!({"status": "ok"}));
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["result"]["status"], "ok");
        assert!(json.get("error").is_none());
    }

    #[test]
    fn test_error_response() {
        let response = McpResponse::error(Some(json!(1)), METHOD_NOT_FOUND, "Method not found");
        let json = serde_json::to_value(&response).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert!(json.get("result").is_none());
        assert_eq!(json["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(json["error"]["message"], "Method not found");
    }

    #[test]
    fn test_server_capabilities() {
        let caps = ServerCapabilities::default();
        assert_eq!(caps.server_info.name, "cmdr");
        assert!(!caps.capabilities.tools.list_changed);
    }
}
