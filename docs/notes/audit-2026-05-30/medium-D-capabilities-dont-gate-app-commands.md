# Per-window capabilities don't gate app commands — viewer/settings webviews can reach secrets and destructive file ops

**Severity:** medium
**Lens:** D — IPC boundary (security)
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/ipc.rs` (single global `tauri::generate_handler![]` registering all commands, e.g. `:296` `get_smb_credentials`, `:455` `get_ai_api_key`, `:467` `get_mcp_token`); the over-claim is in `apps/desktop/src-tauri/src/commands/capabilities/CLAUDE.md` (and `src-tauri/capabilities/*.json`).

## What
The per-window capability files (`viewer.json`, `settings.json`, `default.json`) only restrict Tauri **plugin/core** APIs (`core:`, `fs:`, `opener:`, `clipboard-manager:`, …). They do **not** restrict which `#[tauri::command]` a window can invoke. Every command is registered once in the global handler in `ipc.rs::builder()` and is reachable from *any* window's webview via raw `invoke()` — including `delete_files`, `move_to_trash`, `get_smb_credentials`, `get_ai_api_key`, and `get_mcp_token`. The capabilities CLAUDE.md describes the split as "the security boundary between webview code and native APIs … a compromised viewer webview can't invoke filesystem operations," which is false for app commands.

## Why it matters
The viewer window is intended as a low-trust, read-only surface that renders arbitrary, possibly attacker-controlled file contents. If its webview were ever compromised (a renderer exploit, a content-injection bug), it can delete files and exfiltrate stored SMB credentials / AI API keys / the MCP bearer token — none of which a file *viewer* has any business touching. The documented boundary that a future maintainer would rely on doesn't exist for the most sensitive surface, so someone could add a new secrets command assuming the viewer is sandboxed from it when it isn't.

## Evidence
- `viewer.json` grants only window/clipboard-write/dialog-save permissions, yet `commands/file_viewer.rs` and every other command share one global handler in `ipc.rs`.
- `ipc.rs:296` `crate::commands::network::get_smb_credentials`, `:455` `crate::ai::api_keys::get_ai_api_key`, `:467` `crate::commands::mcp::get_mcp_token` — all globally registered, reachable from any window.

## Suggested fix
Either (a) gate sensitive commands on the calling window label — reject `get_*_credentials` / `get_mcp_token` / the destructive `delete_files` / `move_to_trash` when `window.label()` isn't `"main"` (Tauri's `Window`/`WebviewWindow` is available in the command via `tauri::Window` arg) — or (b) correct `capabilities/CLAUDE.md` to state plainly that app commands are NOT window-scoped and the capability files only gate plugin/core APIs, so the doc stops promising a boundary that isn't enforced. Option (a) is the real defense-in-depth fix.

## Notes
Mitigating factors keep this at medium, not high: `withGlobalTauri: false` in prod and a tight CSP (`script-src 'self'; frame-src 'none'; object-src 'none'`) make webview compromise hard, and the viewer loads only local content. This is defense-in-depth + doc accuracy, not a live exploit today. The rest of the IPC boundary is in good shape: no raw-`invoke` survivors without an opt-out, `serde_json::Value` at the boundary limited to the documented `record_breadcrumb` case, secrets keyed internally (no frontend-supplied paths), and all shell-outs use argv (no shell injection).
