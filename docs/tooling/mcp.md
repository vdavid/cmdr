# MCP servers

Two MCP servers are available when the app is running via `pnpm dev`.

## Servers

- **cmdr** (port 9224) — High-level app control: navigation, file operations, search, dialogs, state inspection. This is
  the primary way to test and interact with the running app. Architecture docs: `src-tauri/src/mcp/CLAUDE.md`.
- **tauri** (port 9223) — Low-level Tauri access: screenshots, DOM inspection, JS execution, IPC calls. Use for visual
  verification and UI automation.

## How to use

**Prefer the Cmdr MCP over the Tauri MCP.** The Cmdr MCP has purpose-built tools for app interaction (navigation,
search, file ops, state inspection). The Tauri MCP is generic low-level access. For example, read `cmdr://state` to
understand what the app is showing instead of taking a Tauri screenshot. Only fall back to the Tauri MCP for things the
Cmdr MCP can't do (window management, DOM inspection, arbitrary JS execution).

**Prefer the wired-up MCP tools** (e.g. `mcp__cmdr__search`, `mcp__cmdr__nav_to_path`). These are available when Claude
Code's MCP integration is connected. Always call `tools/list` first if you're unsure about parameter names.

**Fallback: `./scripts/mcp-call.sh`** — a curl wrapper for Cmdr's MCP server:

```bash
# Search for files
./scripts/mcp-call.sh search '{"pattern":"*.pdf","limit":5}'
./scripts/mcp-call.sh ai_search '{"query":"recent invoices"}'

# Navigate and inspect
./scripts/mcp-call.sh nav_to_path '{"pane":"left","path":"/Users"}'
./scripts/mcp-call.sh --read-resource 'cmdr://state'

# Discover available tools and their parameter schemas
./scripts/mcp-call.sh --list-tools
```

## Connection resilience

The MCP server goes down during hot reloads (up to 15s for Rust changes, up to 3s for frontend changes). Multiple agents
working simultaneously can trigger frequent reloads. Follow this escalation:

1. **Try the wired-up MCP tools** (`mcp__cmdr__*`). If they work, use them.
2. **If disconnected, try `./scripts/mcp-call.sh`** — it connects independently and may work when the MCP integration is
   temporarily down.
3. **If both fail, wait ~15 seconds and retry** — the app is probably mid-reload from a Rust change.
4. **If still failing, ask the user** to stop other agents that may be triggering hot reloads, and report back when it's
   clear.

Do NOT retry in a tight loop. One retry after 15 seconds is enough before escalating.

## Tauri MCP pitfalls

### Window management: use `manage_window`, not JS APIs

The Tauri webview's `window.__TAURI__` JS APIs (e.g. `getCurrentWindow().setSize()`) are gated by per-window capability
permissions in `src-tauri/capabilities/`. Calling them via `webview_execute_js` will fail with "not allowed" errors
unless the specific permission is granted.

Instead, use the Tauri MCP's dedicated `manage_window` tool:

```
manage_window action: "resize", width: 1142, height: 705, logical: true
manage_window action: "info"
manage_window action: "list"
```

These operate through the MCP bridge and bypass capability restrictions.

**General rule**: before reaching for `webview_execute_js` with `window.__TAURI__` APIs, check if there's a dedicated
Tauri MCP tool for the job. `webview_execute_js` is best for DOM manipulation and reading app state, not for Tauri
platform APIs.
