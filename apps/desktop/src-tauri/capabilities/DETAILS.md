# Tauri capabilities: details

Depth and rationale. `CLAUDE.md` holds the must-knows that prevent silent breakage; this file holds the per-file design
rationale.

## Why one file per window

Each window type has a different trust level, and the capability system is the only security boundary between webview
code and native APIs. The main window needs filesystem access, drag-and-drop, clipboard, and the updater. The settings
window only needs event dispatch and theme control. The viewer only needs window management. Splitting by window means a
compromised viewer webview can't invoke filesystem operations.

## `desktop.json` separate from `default.json`

`desktop.json` holds desktop-platform-specific permissions (`window-state:default` for remembering window
size/position), scoped to `["macOS", "windows", "linux"]`. Keeping it separate from `default.json` keeps the
platform-specific vs. universal split clear and avoids accidentally applying desktop-only permissions to future
non-desktop targets. It legitimately shares `main` + `debug`.

## Debug window draws solely from `debug.json`

`default.json` is the most powerful capability in the app. The debug window must not inherit it: listing `"debug"` in
`default.json` would silently undo the per-window split for the most privileged capability. The debug window is dev-only
(frontend gates open on `import.meta.env.DEV`; the `mcp-bridge` plugin is `#[cfg(debug_assertions)]`), so the runtime
risk is low, but the structure is the foot-gun, any future gate slip would expose the full surface. The debug panels
only need core window/webview/event/app-theme ops, devtools, and `store:default` (they reach the backend through typed
app commands, which aren't ACL-gated, and through events), so `debug.json` carries `core:default` and is self-contained.

## Viewer settings persistence path

Because the viewer has no store access (see `CLAUDE.md`), viewer settings persist through the typed restricted-window
command pair in `commands/settings.rs`:

- `get_restricted_window_settings`: read allowlist (word wrap, binary-warning suppression, text size, app color).
- `persist_restricted_window_setting`: write allowlist, a typed enum covering only `viewer.wordWrap` and
  `fileViewer.suppressBinaryWarning`, forwarded to the main window's `restricted-settings-bridge.ts`, which re-checks
  the allowlist before persisting through the normal store pipeline.

The enum is the boundary: a compromised viewer can flip those two booleans and nothing else. Viewer tail mode stays
deliberately unpersisted (defaults off per session, see `routes/viewer/CLAUDE.md` § Tail mode).
