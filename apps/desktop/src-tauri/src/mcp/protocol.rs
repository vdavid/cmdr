//! MCP protocol message types and handling (spec version 2025-11-25).
//!
//! Implements JSON-RPC 2.0 message format as required by MCP.
//! See: <https://modelcontextprotocol.io/specification/2025-11-25>

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP JSON-RPC request format.
#[derive(Debug, Clone, Deserialize)]
pub struct McpRequest {
    #[allow(dead_code, reason = "Required by JSON-RPC spec, validated implicitly")]
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

#[cfg(test)]
pub use crate::tools::error_codes::{INTERNAL_ERROR, PARSE_ERROR};
pub use crate::tools::error_codes::{INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND};

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
        Self::error_with_data(id, code, message, None)
    }

    /// Create an error response carrying structured detail in the JSON-RPC `data`
    /// member. Used where the caller has to ACT on the reason rather than only read
    /// it (which dialog is blocking, for instance), so it stays a typed field
    /// instead of something an agent has to parse back out of the sentence.
    pub fn error_with_data(id: Option<Value>, code: i32, message: impl Into<String>, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
                data,
            }),
        }
    }

    /// This response as it leaves for the client: every string in it, a path
    /// or text holding one, reads with a same-named twin's pick taken out
    /// (`cmdr_fs::twin_path::shown`). The one place every reply passes, so no
    /// resource or tool has to remember it; a twin is then addressed by its
    /// plain path, which a path-only call refuses as ambiguous.
    pub fn into_shown(mut self) -> Self {
        if let Some(result) = self.result.as_mut() {
            show_strings(result);
        }
        if let Some(error) = self.error.as_mut() {
            if let std::borrow::Cow::Owned(message) = cmdr_fs::twin_path::shown(&error.message) {
                error.message = message;
            }
            if let Some(data) = error.data.as_mut() {
                show_strings(data);
            }
        }
        self
    }
}

/// Every string in `value`, nested ones too, with a twin's pick taken out.
fn show_strings(value: &mut Value) {
    match value {
        Value::String(text) => {
            if let std::borrow::Cow::Owned(shown) = cmdr_fs::twin_path::shown(text) {
                *text = shown;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(show_strings),
        Value::Object(fields) => fields.values_mut().for_each(show_strings),
        _ => {}
    }
}

/// Server capabilities returned during initialization.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub protocol_version: String,
    pub capabilities: Capabilities,
    pub server_info: ServerInfo,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capabilities {
    pub tools: ToolCapabilities,
    pub resources: ResourceCapabilities,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCapabilities {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceCapabilities {
    #[serde(rename = "listChanged")]
    pub list_changed: bool,
    pub subscribe: bool,
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
                resources: ResourceCapabilities {
                    list_changed: false,
                    subscribe: false,
                },
            },
            server_info: ServerInfo {
                name: "cmdr".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: concat!(
                "Controls a running Cmdr file manager instance, a keyboard-driven two-pane file ",
                "explorer for macOS. Use these tools when the user asks to navigate directories, browse ",
                "files, manage selections, copy files, change view settings, or interact with dialogs in ",
                "Cmdr.\n\n",
                "Start by reading the cmdr://state resource to understand current pane paths, cursor ",
                "positions, and selections before taking actions. Most tools target a specific pane (left ",
                "or right); the state resource tells you which pane is focused. Navigation tools ",
                "(nav_to_path, nav_to_parent, nav_back, nav_forward) and view tools (sort, set_view_mode, ",
                "toggle_hidden) affect one pane at a time. File operations like copy work on the current ",
                "selection and target the opposite pane. The dialog tool manages modal dialogs (settings, ",
                "file viewer, about); only one instance of each type can be open.",
            )
            .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// ❗ A same-named twin's row path carries a NUL pick (`cmdr_fs::twin_path`):
    /// no reply shows it, in a result's text, nested JSON, or an error.
    #[test]
    fn a_reply_shows_a_twin_row_path_as_its_plain_path() {
        let twin = format!("/DCIM/{}/IMG_1.JPG", cmdr_fs::twin_path::pick_segment(4242));
        let reply = McpResponse::success(
            Some(json!(1)),
            json!({"content": [{"type": "text", "text": format!("cursor: {twin}")}], "paths": [twin]}),
        )
        .into_shown();
        let refusal = McpResponse::error(Some(json!(2)), INVALID_PARAMS, format!("can't open {twin}")).into_shown();

        assert_eq!(
            serde_json::to_value(&reply).expect("serializes")["result"],
            json!({"content": [{"type": "text", "text": "cursor: /DCIM/IMG_1.JPG"}], "paths": ["/DCIM/IMG_1.JPG"]})
        );
        assert_eq!(refusal.error.expect("an error").message, "can't open /DCIM/IMG_1.JPG");
    }

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
