//! MCP HTTP server implementation.

use axum::{
    Json, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
use tower_http::cors::{Any, CorsLayer};

use super::config::McpConfig;
use super::executor::execute_tool;
use super::protocol::{INVALID_PARAMS, METHOD_NOT_FOUND, McpRequest, McpResponse, ServerCapabilities};
use super::tools::get_all_tools;

/// Shared state for the MCP server.
pub struct McpState<R: Runtime> {
    pub app: AppHandle<R>,
}

/// Start the MCP server.
pub fn start_mcp_server<R: Runtime + 'static>(app: AppHandle<R>, config: McpConfig) {
    if !config.enabled {
        log::info!("MCP server is disabled");
        return;
    }

    let port = config.port;
    let state = Arc::new(McpState { app: app.clone() });

    // Spawn the server in a separate tokio task
    tauri::async_runtime::spawn(async move {
        let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

        let app = Router::new()
            .route("/mcp", post(handle_mcp_request::<R>))
            .route("/mcp/health", get(health_check))
            .layer(cors)
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        log::info!("MCP server listening on http://{}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await;
        match listener {
            Ok(listener) => {
                if let Err(e) = axum::serve(listener, app).await {
                    log::error!("MCP server error: {}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to bind MCP server to {}: {}", addr, e);
            }
        }
    });
}

/// Health check endpoint.
async fn health_check() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

/// Handle MCP JSON-RPC requests.
async fn handle_mcp_request<R: Runtime>(
    State(state): State<Arc<McpState<R>>>,
    Json(request): Json<McpRequest>,
) -> impl IntoResponse {
    let response = process_request(&state, request).await;
    Json(response)
}

/// Process an MCP request and return a response.
async fn process_request<R: Runtime>(state: &McpState<R>, request: McpRequest) -> McpResponse {
    match request.method.as_str() {
        "initialize" => {
            let caps = ServerCapabilities::default();
            McpResponse::success(request.id, serde_json::to_value(caps).unwrap())
        }

        "tools/list" => {
            let tools = get_all_tools();
            McpResponse::success(request.id, json!({"tools": tools}))
        }

        "tools/call" => {
            let name = match request.params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => {
                    return McpResponse::error(request.id, INVALID_PARAMS, "Missing 'name' parameter");
                }
            };

            let arguments = request.params.get("arguments").cloned().unwrap_or(json!({}));

            let result = execute_tool(&state.app, name, &arguments);

            match result {
                Ok(value) => McpResponse::success(
                    request.id,
                    json!({"content": [{"type": "text", "text": value.to_string()}]}),
                ),
                Err(e) => McpResponse::error(request.id, e.code, e.message),
            }
        }

        _ => McpResponse::error(
            request.id,
            METHOD_NOT_FOUND,
            format!("Unknown method: {}", request.method),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_response() {
        let response = json!({"status": "ok"});
        assert_eq!(response["status"], "ok");
    }
}
