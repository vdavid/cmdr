//! What the official SDK calls for each MCP method: `crate::tools` and its `cmdr://` resources,
//! carried onto `rmcp`'s wire types.
//!
//! The one place every reply passes on its way out, so no tool or resource has to remember to take a
//! same-named twin's pick out of what it shows ([`show_strings`]).

use std::borrow::Cow;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ErrorCode, Implementation,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::tools::resources::{get_all_resources, read_resource};
use crate::tools::{ToolCtx, ToolError, ToolId, execute_tool, get_all_tools};

/// What a client reads at `initialize` to decide when to reach for this server.
const INSTRUCTIONS: &str = concat!(
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
);

/// One request's server. `rmcp` builds a fresh one per request, since the transport is stateless.
pub(super) struct CmdrMcpHandler<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> CmdrMcpHandler<R> {
    pub(super) fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> ServerHandler for CmdrMcpHandler<R> {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder().enable_tools().enable_resources().build();
        if let Some(tools) = capabilities.tools.as_mut() {
            tools.list_changed = Some(false);
        }
        if let Some(resources) = capabilities.resources.as_mut() {
            resources.list_changed = Some(false);
            resources.subscribe = Some(false);
        }
        ServerConfig::new(capabilities)
            // `rmcp` names itself otherwise: its default comes from its own build env.
            .with_server_info(Implementation::new("cmdr", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(
            get_all_tools().into_iter().map(wire_tool).collect(),
        ))
    }

    /// `rmcp` reads this to validate a 2026-07-28 request's `Mcp-Param-*` headers.
    fn get_tool(&self, name: &str) -> Option<Tool> {
        get_all_tools()
            .into_iter()
            .find(|tool| tool.name == name)
            .map(wire_tool)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name;
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let logged_name = logged_tool_name(&name);
        // allowed-log-private: a name from the tool table
        log::debug!("MCP: executing tool {logged_name}");

        #[cfg(test)]
        let _running = tests::ToolCallRunning::enter();
        let client_left = context.ct;
        // A client's disconnect is its cancel: it also takes down whatever the tool holds open.
        let ctx = ToolCtx::external(client_left.clone());
        // `rmcp` runs this in a task of its own, so nothing else stops an `await` or a `search`
        // once the client that asked has gone.
        let outcome = tokio::select! {
            outcome = execute_tool(&self.app, &ctx, &name, &arguments) => outcome,
            () = client_left.cancelled() => {
                // allowed-log-private: a name from the tool table
                log::debug!("MCP: tool {logged_name} stopped, the client went away");
                Err(ToolError::internal("The client went away before this finished."))
            }
        };

        match outcome {
            Ok(value) => {
                let text = rendered_for_a_client(value);
                // allowed-log-private: a name from the tool table
                log::debug!("MCP: tool {logged_name} succeeded, response length={}", text.len());
                Ok(CallToolResult::success(vec![ContentBlock::text(text)]).into())
            }
            Err(e) => {
                log_failed_call(&name, &e);
                Err(wire_error(e))
            }
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult::with_all_items(
            get_all_resources()
                .into_iter()
                .map(|resource| {
                    Resource::new(resource.uri, resource.name)
                        .with_description(resource.description)
                        .with_mime_type(resource.mime_type)
                })
                .collect(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let content = read_resource(&self.app, &request.uri)
            .await
            .map_err(|refused| wire_error(refused.into_tool_error()))?;
        let contents = ResourceContents::TextResourceContents {
            uri: shown(content.uri),
            mime_type: Some(content.mime_type),
            text: shown(content.text),
            meta: None,
        };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}

fn wire_tool(tool: crate::tools::Tool) -> Tool {
    let schema = match tool.input_schema {
        Value::Object(schema) => schema,
        // `test_tool_input_schemas_are_valid` pins every table entry to an object.
        _ => serde_json::Map::new(),
    };
    let hints = tool.annotations;
    let mut annotations = ToolAnnotations::new();
    annotations.read_only_hint = hints.read_only_hint;
    annotations.destructive_hint = hints.destructive_hint;
    Tool::new(tool.name, tool.description, schema).with_annotations(annotations)
}

/// A client-sent tool name as a log line may show it: `name` is any string a client sends, so one
/// outside the table doesn't get written.
fn logged_tool_name(name: &str) -> String {
    let id = ToolId::from_wire_name(name);
    match id.is_known() {
        true => id.as_wire_name().to_string(),
        false => "(not in the table)".to_string(),
    }
}

/// The failure's line: its code and typed token. ❌ Never the message: refusals echo arguments, the
/// log ships in error reports, and its redactor only knows paths.
fn log_failed_call(name: &str, error: &ToolError) {
    let verdict = if error.outcome_is_unknown() {
        "outcome unknown"
    } else {
        "failed"
    };
    let name = logged_tool_name(name);
    let token = error_token(error.data.as_ref());
    // allowed-log-private: a name from the tool table, a code, and a typed token
    log::warn!("MCP: tool {name} {verdict}, code={}, token={token}", error.code);
}

/// `data`'s typed token, the part a caller branches on, or `none`. Only an identifier gets through,
/// since `data` holds free values beside it.
fn error_token(data: Option<&Value>) -> &str {
    let is_token = |text: &&str| !text.is_empty() && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    ["refusal", "reason", "outcome"]
        .into_iter()
        .find_map(|field| data?.get(field)?.as_str().filter(is_token))
        .unwrap_or("none")
}

/// A tool's error as the client gets it: still a JSON-RPC error, with the typed `data` a caller
/// branches on.
fn wire_error(error: ToolError) -> ErrorData {
    let mut data = error.data;
    if let Some(data) = data.as_mut() {
        show_strings(data);
    }
    ErrorData::new(ErrorCode(error.code), shown(error.message), data)
}

/// A tool's result as the text of its one content block. The pick comes out BEFORE this renders:
/// rendering writes a NUL as the six characters `\u0000`, which `shown` can't recognize afterward.
fn rendered_for_a_client(mut value: Value) -> String {
    show_strings(&mut value);
    match value {
        Value::String(text) => text,
        value => serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
    }
}

fn shown(text: String) -> String {
    match cmdr_fs::twin_path::shown(&text) {
        Cow::Owned(shown) => shown,
        Cow::Borrowed(_) => text,
    }
}

/// Every string in `value`, nested ones too, with a same-named twin's pick taken out
/// (`cmdr_fs::twin_path::shown`). A twin is then addressed by its plain path, which a path-only call
/// refuses as ambiguous.
fn show_strings(value: &mut Value) {
    match value {
        Value::String(text) => {
            if let Cow::Owned(shown) = cmdr_fs::twin_path::shown(text) {
                *text = shown;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(show_strings),
        Value::Object(fields) => fields.values_mut().for_each(show_strings),
        _ => {}
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TOOL_CALLS_RUNNING: AtomicUsize = AtomicUsize::new(0);

    /// How many `tools/call`s are still running, so a test can see one stop from out here.
    pub(in crate::mcp) fn tool_calls_running() -> usize {
        TOOL_CALLS_RUNNING.load(Ordering::SeqCst)
    }

    pub(super) struct ToolCallRunning;

    impl ToolCallRunning {
        pub(super) fn enter() -> Self {
            TOOL_CALLS_RUNNING.fetch_add(1, Ordering::SeqCst);
            Self
        }
    }

    impl Drop for ToolCallRunning {
        fn drop(&mut self) {
            TOOL_CALLS_RUNNING.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn a_string_result_is_its_own_text() {
        assert_eq!(rendered_for_a_client(json!("simple string")), "simple string");
    }

    #[test]
    fn any_other_result_renders_as_pretty_json() {
        assert_eq!(
            rendered_for_a_client(json!({"key": "value"})),
            "{\n  \"key\": \"value\"\n}"
        );
    }

    /// ❗ A same-named twin's row path carries a NUL pick (`cmdr_fs::twin_path`): no reply shows it,
    /// in a result's text, nested JSON, or an error.
    #[test]
    fn a_reply_shows_a_twin_row_path_as_its_plain_path() {
        let twin = format!("/DCIM/{}/IMG_1.JPG", cmdr_fs::twin_path::pick_segment(4242));

        let result = rendered_for_a_client(json!({"cursor": format!("cursor: {twin}"), "paths": [twin]}));
        assert_eq!(
            serde_json::from_str::<Value>(&result).expect("JSON text"),
            json!({"cursor": "cursor: /DCIM/IMG_1.JPG", "paths": ["/DCIM/IMG_1.JPG"]})
        );

        let refusal =
            wire_error(ToolError::invalid_params(format!("can't open {twin}")).with_data(json!({"path": twin})));
        assert_eq!(refusal.message, "can't open /DCIM/IMG_1.JPG");
        assert_eq!(refusal.data, Some(json!({"path": "/DCIM/IMG_1.JPG"})));
    }

    /// `cmdr.log` ships in error reports and its redactor is path-shaped, so free text a client put
    /// in an argument would travel intact. A refusal's sentence echoes arguments.
    #[test]
    fn a_failed_call_logs_its_code_and_token_and_no_words_from_the_call() {
        let refused = ToolError::invalid_params("unknown value 'my-private-words'")
            .with_data(json!({"refusal": "invalidValue", "path": "/my-private-words"}));
        let ((), records) = crate::test_support::capture_logs(|| {
            log_failed_call("no_such_my-private-words", &refused);
            log_failed_call("search", &ToolError::outcome_unknown("my-private-words"));
            log_failed_call("search", &ToolError::invalid_params("my-private-words"));
        });

        let lines: Vec<&str> = records.iter().map(|record| record.message.as_str()).collect();
        assert!(lines.iter().all(|line| !line.contains("my-private-words")), "{lines:?}");
        assert!(lines[0].contains("code=-32602") && lines[0].contains("token=invalidValue"));
        assert!(lines[1].contains("search") && lines[1].contains("token=unknown"));
        assert!(lines[2].contains("token=none"));
    }

    /// A token comes from our own code today, but `data` carries free values too, so only a
    /// token-shaped one gets written.
    #[test]
    fn a_data_value_that_isnt_a_token_isnt_logged() {
        assert_eq!(error_token(Some(&json!({"reason": "folderStalled"}))), "folderStalled");
        assert_eq!(error_token(Some(&json!({"refusal": "two words"}))), "none");
        assert_eq!(error_token(Some(&json!({"outcome": 5}))), "none");
        assert_eq!(error_token(Some(&json!({"path": "/a"}))), "none");
        assert_eq!(error_token(None), "none");
    }

    #[test]
    fn a_tool_errors_code_and_data_pass_through_untouched() {
        let error = wire_error(ToolError::outcome_unknown("Check the queue."));
        assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
        assert_eq!(error.data, Some(json!({"outcome": "unknown"})));
    }

    /// `ToolError.code` is a plain `i32`, and these two carry every tool refusal. The 2026-07-28
    /// renumbering left them where JSON-RPC puts them, and `rmcp` agrees.
    #[test]
    fn the_codes_a_tool_answers_with_are_rmcps_too() {
        use crate::tools::error_codes::{INTERNAL_ERROR, INVALID_PARAMS};
        assert_eq!(ErrorCode::INVALID_PARAMS, ErrorCode(INVALID_PARAMS));
        assert_eq!(ErrorCode::INTERNAL_ERROR, ErrorCode(INTERNAL_ERROR));
    }
}
