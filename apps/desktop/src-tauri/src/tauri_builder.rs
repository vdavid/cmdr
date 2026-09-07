//! Everything handed to `tauri::Builder` before `setup` runs: the
//! `cmdr-media://` URI scheme, and every plugin including the three registered
//! conditionally.
//!
//! This is the half of launch that is genuinely INDEPENDENT: no registration
//! here cares which one ran before it, and none of them observes app state.
//! The ORDERED spine of launch stays in `lib.rs::run`, where the sequence is
//! the point and a helper would hide it. That's the line to keep when adding
//! something: order-free registration here, anything that has to happen after
//! (or before) another step in `run`.

use crate::file_viewer;

/// Registers the media protocol and every plugin, in the one order that doesn't
/// matter.
pub fn configure(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let builder = register_media_protocol(builder);
    let builder = register_mcp_bridge(builder);
    let builder = register_playwright(builder);
    let builder = register_updater(builder);

    builder
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(crate::downloads::global_shortcut::plugin_builder())
}

/// The `cmdr-media://` async scheme the file viewer serves images and PDFs
/// through.
///
/// Registered before any window exists, which is correct: `viewer-*` windows are
/// created lazily and inherit the app-wide scheme. The handler is a thin shell
/// over `file_viewer::media_protocol`; access is gated by an unguessable
/// per-open token, not the path. See `file_viewer/media_protocol.rs`.
fn register_media_protocol(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.register_asynchronous_uri_scheme_protocol(
        file_viewer::media_protocol::SCHEME,
        |_ctx, request, responder| {
            file_viewer::media_protocol::handle_request(request, responder);
        },
    )
}

/// The MCP bridge, debug builds only, for security.
///
/// Two non-obvious things to keep in mind here:
///   1. The plugin's `Config::default()` is `bind_address: "0.0.0.0"`, which exposes
///      the WebSocket bridge (DOM inspection, JS execution, IPC monitoring) to anyone
///      on the LAN. We always force `127.0.0.1` so the bridge is localhost-only. This
///      is a security fix; do NOT remove it even when adding remote-device support.
///   2. The plugin has no public method to query the bound port, and its internal
///      `find_available_port` silently returns `base_port` on exhaustion (no error).
///      We therefore let `tauri-wrapper.js` allocate an ephemeral port up front via
///      `net.createServer().listen(0)`, pass it as `CMDR_MCP_BRIDGE_PORT`, AND have
///      the wrapper write `<data_dir>/tauri-mcp.port` BEFORE Tauri launches. After
///      plugin setup we run a 500 ms post-bind `TcpStream::connect` probe and
///      warn-log on mismatch so a silent fallback is visible in the logs.
///
/// See `docs/tooling/instance-isolation.md` § "Per-resource breakdown" (Tauri MCP
/// bridge port row) for the wrapper-writes-port-file contract.
#[cfg(debug_assertions)]
fn register_mcp_bridge(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let mut bridge_builder = tauri_plugin_mcp_bridge::Builder::new().bind_address("127.0.0.1");
    let expected_bridge_port: Option<u16> = std::env::var("CMDR_MCP_BRIDGE_PORT").ok().and_then(|v| v.parse().ok());
    if let Some(port) = expected_bridge_port {
        bridge_builder = bridge_builder.base_port(port);
    }
    let plugin = bridge_builder.build::<tauri::Wry>();

    if let Some(port) = expected_bridge_port {
        spawn_bridge_port_probe(port);
    }

    builder.plugin(plugin)
}

#[cfg(not(debug_assertions))]
fn register_mcp_bridge(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
}

/// 500 ms after registration, try to connect on the expected port.
///
/// On success: log info. On failure: warn that the wrapper-written port file may
/// be stale (the plugin silently fell back to a different port; readers will
/// discover it on first request via `ECONNREFUSED`).
#[cfg(debug_assertions)]
fn spawn_bridge_port_probe(port: u16) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::time::timeout(
            std::time::Duration::from_millis(500),
            tokio::net::TcpStream::connect(addr),
        )
        .await
        {
            Ok(Ok(_)) => log::info!(
                target: "mcp::bridge",
                "tauri-MCP bridge bound to 127.0.0.1:{port}",
            ),
            Ok(Err(err)) => log::warn!(
                target: "mcp::bridge",
                "tauri-MCP bridge did not bind 127.0.0.1:{port} within 500 ms ({err}); the port file at <data_dir>/tauri-mcp.port may be stale",
            ),
            Err(_) => log::warn!(
                target: "mcp::bridge",
                "tauri-MCP bridge probe to 127.0.0.1:{port} timed out after 500 ms; the port file at <data_dir>/tauri-mcp.port may be stale",
            ),
        }
    });
}

/// The Playwright E2E socket bridge for direct webview injection.
///
/// The socket path is overridable via `CMDR_PLAYWRIGHT_SOCKET` so parallel E2E
/// shards can each spawn their own Tauri instance bound to a distinct socket.
#[cfg(feature = "playwright-e2e")]
fn register_playwright(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    let mut pw_config = tauri_plugin_playwright::PluginConfig::new();
    if let Ok(socket_path) = std::env::var("CMDR_PLAYWRIGHT_SOCKET") {
        pw_config = pw_config.socket_path(socket_path);
    }
    builder.plugin(tauri_plugin_playwright::init_with_config(pw_config))
}

#[cfg(not(feature = "playwright-e2e"))]
fn register_playwright(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
}

/// Tauri's updater plugin, skipped on macOS (the custom updater preserves TCC
/// permissions) and in CI (avoids a network dependency and latency during E2E).
#[cfg(not(target_os = "macos"))]
fn register_updater(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    if std::env::var("CI").is_ok() {
        builder
    } else {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    }
}

#[cfg(target_os = "macos")]
fn register_updater(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
}
