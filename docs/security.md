# Security

## withGlobalTauri

The app uses [MCP Server Tauri](https://github.com/hypothesi/mcp-server-tauri) to let AI assistants (Claude Code,
Cursor) control this app: take screenshots, click buttons, and read front-end logs.

The MCP bridge requires `withGlobalTauri: true` which exposes `window.__TAURI__` to the frontend. This would be a huge
security risk in production (untrusted JS could access system APIs, not good), so we enable it **only in development**:

1. **Compile-time exclusion**: The MCP plugin is only registered via `#[cfg(debug_assertions)]` in `lib.rs`
2. **Config separation**: `"withGlobalTauri": false` in `tauri.conf.json` (production), only overridden via
   `tauri.dev.json` during dev
3. **Wrapper script**: `apps/desktop/scripts/tauri-wrapper.js` injects `-c src-tauri/tauri.dev.json` only for `dev` commands.
   (`pnpm tauri dev` calls the wrapper which adds `-c src-tauri/tauri.dev.json`, then Tauri merges this
   with `tauri.conf.json` via [JSON Merge Patch (RFC 7396)](https://datatracker.ietf.org/doc/html/rfc7396).))

To avoid security issues in dev mode, always add a condition to **disable** that functionality in dev mode. This way,
malicious websites can't access the system APIs even on your machine.
