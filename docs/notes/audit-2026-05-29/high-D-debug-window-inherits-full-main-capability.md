# Debug window inherits the main window's full IPC and FS capability surface

**Severity:** high
**Lens:** D — IPC boundary
**Confidence:** high

## Location

- `apps/desktop/src-tauri/capabilities/default.json` line 5-8 (`"windows": ["main", "debug"]`)
- `apps/desktop/src-tauri/capabilities/desktop.json` line 8-11 (`"windows": ["main", "debug"]`)
- Debug capability of record: `apps/desktop/src-tauri/capabilities/debug.json`
- Debug window construction: `apps/desktop/src/lib/debug/debug-window.ts:51`
- Debug-window open gate: `apps/desktop/src/routes/(main)/+page.svelte:114`
  (`import.meta.env.DEV && e.metaKey && !e.shiftKey && !e.altKey && e.key === 'd'`)

## What

The capability `identifier: "default"` lists `"debug"` alongside `"main"` in its `windows` array, so the debug window
receives the full main-window permission set: `core:default`, `core:window:allow-start-dragging`, `core:window:allow-
close`, `core:window:allow-theme`, `core:window:allow-set-effects`, `core:webview:allow-webview-close`, `core:webview:
allow-create-webview-window`, `core:app:allow-set-app-theme`, `core:event:default`, `opener:default`, `opener:allow-
open-path` (with `**/*` + `**/.*` globs), `store:default`, `clipboard-manager:default`, `mcp-bridge:default`,
`fs:allow-temp-write`, `fs:allow-remove`, `updater:default`, `process:allow-restart`, and `dialog:allow-ask`. The
narrower `debug.json` only adds a few items on top — it doesn't subtract anything, and Tauri merges capability files
union-wise per window label.

The debug window is gated dev-only at construction time (`openDebugWindow` is only invoked under `import.meta.env.DEV`)
and the `mcp-bridge` plugin is similarly gated to `#[cfg(debug_assertions)]`. But the capability config itself
is part of every build's `tauri.conf.json`-generated manifest. In release builds the debug window is unreachable, so
the runtime risk is zero today. The structural issue is that the principle documented in
`capabilities/CLAUDE.md` — "Splitting by window prevents privilege escalation: a compromised viewer webview can't
invoke filesystem operations" — is not honoured for debug.

## Why it matters

1. **Principle erosion.** `capabilities/CLAUDE.md` § "Decision: One capability file per window type" frames the
   per-window split as the security boundary. The debug window's own `debug.json` already exists; the inclusion of
   `"debug"` in `default.json::windows` undoes the split for the most powerful capability set in the app. A future
   maintainer reading just `debug.json` will believe the debug window is locked down. It isn't.
2. **Release-build foot-gun waiting to happen.** Today the gate is `import.meta.env.DEV` in the FE plus
   `#[cfg(debug_assertions)]` on the MCP-bridge plugin. The webview label `"debug"` is matched at the capability layer
   regardless of build mode. If anyone ever (a) drops the FE DEV gate by accident, (b) ships a release with
   `debug_assertions` enabled for any reason, or (c) adds a non-debug feature that happens to register a webview
   labeled `"debug"`, that webview inherits the full main-window IPC and FS surface — including `process:allow-restart`,
   `fs:allow-remove`, `updater:default`, `clipboard-manager:default`, and `mcp-bridge:default`.
3. **Reduces value of the debug.json file.** As written, `debug.json` is purely additive over `default.json`. The
   per-window discipline only works if `default.json` doesn't claim the same window.

## Evidence

`default.json` lines 4-7:

```json
"identifier": "default",
"description": "Capability for the main window",
"windows": [
    "main",
    "debug"
],
```

Compared to `desktop.json`, which is also shared (its share is intentional and documented in
`capabilities/CLAUDE.md`: it's the platform-specific `window-state:default` perm), so the actual privileged sharing is
`default.json`'s union.

Debug window construction with label `"debug"`:

```ts
// debug-window.ts:51
const win = new WebviewWindow('debug', { url: '/debug', … })
```

Doc claim being violated:

> Splitting by window prevents privilege escalation — a compromised viewer webview can't invoke filesystem operations.

(`capabilities/CLAUDE.md` § "Decision: One capability file per window type")

## Suggested fix

1. Remove `"debug"` from `default.json::windows` so the main-window capability applies only to `main`.
2. Move into `debug.json` only the permissions the debug page actually needs (`core:default`, `core:event:default`,
   `core:app:allow-set-app-theme`, `store:default`, and whatever the debug panels call: from `routes/debug/`
   inspection that includes `clipboard-manager:default` if they show "Copy" buttons, and probably
   `dialog:allow-ask`). Audit each `Debug*Panel.svelte` and add only what's verified by `await`-with-`try/catch`.
3. Confirm by running `pnpm dev`, opening the debug window with `⌘D`, and exercising each panel. Any permission gap
   surfaces immediately as a "not allowed" rejection in the panel's log (per the `capabilities/CLAUDE.md` Mitigation
   pattern).
4. Leave `desktop.json` sharing `main` + `debug` as-is — `window-state:default` is genuinely shared between the two.

## Notes

This finding stacks with `high-D-fs-plugin-unscoped-allow-write-and-remove.md` — fixing only the fs-plugin scope but
leaving debug as a co-owner of `default.json` means the debug page (if reachable) still gets the new scoped FS write
permission. Tightening the debug-window membership is what makes the split honest.
