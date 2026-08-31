# Tauri capabilities

Each window has its own capability file controlling which PLUGIN and `core:` APIs it can call. One file per window type
(not one global file): each window has a different trust level, so splitting by window prevents privilege escalation.

❌ **Capabilities do NOT gate Cmdr's own commands**, so everything in `generate_handler![]` is reachable from every
window. Having no caller-window guard is deliberate: `docs/security.md` § "Why there's no caller-window authorization
guard". Don't claim a window "can't invoke" an app command.

- `default.json`: main window (the most powerful capability: `updater`, `process:allow-restart`, `mcp-bridge`,
  `create-webview-window`, `global-shortcut`, scoped `fs` write/remove, …). Lists only `"main"`.
- `settings.json`: settings window. `viewer.json`: file viewer windows (wildcard `"viewer-*"` covers `viewer-0`, …).
- `shortcuts.json`: read-only Keyboard shortcuts help window. `debug.json`: dev-only debug window, self-contained with
  `core:default`. E2E's capability sits in `../capabilities-e2e/playwright.json`.
- `queue.json`: operation-queue window — settings perms minus `store:default` + `dialog:allow-ask`.

[Tauri permissions reference](https://tauri.app/security/permissions/). **Adding a whole new window?** See
`docs/guides/adding-a-window.md` for the route + opener + capability recipe, and the gotcha that
window-creation perms are checked against the *calling* window.

## Must-knows (invariants and guardrails)

- **Missing permissions fail silently at runtime**: the call rejects with a generic `... not allowed ...` and the
  feature just looks broken. When you add any new Tauri API call from a window (`setFocus`, `setTitle`, `setMinSize`,
  plugin commands), add the matching permission to that window's capability file, and `await` it in `try/catch` with a
  `log.warn` (never `void` it) so failures surface in development.
- **The viewer window must never get store access (`store:default` OR granular `store:allow-*`).** The viewer is the
  highest-risk webview: it renders arbitrary, possibly-hostile file content. The `tauri-plugin-store` permissions gate
  *commands* (`load`, `get`, `set`, `save`), not *filenames*, so any of them lets the webview `load('license.json')` and
  read or tamper with any store in the data dir. Viewer settings persist instead through the typed restricted-window
  command pair in `commands/settings.rs` (`get_restricted_window_settings` + `persist_restricted_window_setting`, whose
  enum allowlist is the boundary). Extend the enum + the `restricted-settings-bridge.ts` allowlist for new persistable
  viewer settings; never re-add store access and never widen to a free-form id string.
- **A `store:default` change means updating `WINDOW_SETTINGS_ACCESS`**
  (`apps/desktop/src/lib/settings/window-settings.ts`), the frontend's store-backed vs restricted-snapshot map;
  `window-settings.test.ts` fails on drift.
- **A new debug-panel API goes in `debug.json`, never `default.json`** (the app's most privileged capability), so a
  future gate slip can't expose the full surface. Perms fail silently, so smoke-test with `pnpm dev` + ⌘D after.
- **The `fs` plugin write/remove are scoped to exactly the two drag temp files** (`$TEMP/drag-icon.png`,
  `$TEMP/drag-image.png`), not granted broadly. A broad `fs:allow-remove` is "remove any path the process can," which a
  compromised main webview could exploit via raw `invoke('plugin:fs|remove', …)`, bypassing every typed-IPC guardrail.
  The only consumer is `file-explorer/drag/drag-drop.ts`. If the drag temp filenames change, update the scope here too
  (perms fail silently).
- **Every window with a custom/overlay title bar needs `core:window:allow-start-dragging`** (`data-tauri-drag-region`
  calls `window.startDragging()`); without it the drag rejects silently and the title bar feels like a dead zone, while
  markup and "has the attribute" unit tests still pass. Applies to `default.json`, `settings.json`, `viewer.json`.
- **`opener:allow-open-path` needs an explicit `"**/.*"` glob for hidden files.** The default `"**/*"` excludes
  dotfiles, so opening hidden files silently fails without the separate pattern.
- **`playwright:default` can't go in any file here.** Its permission schemas exist only under the `playwright-e2e`
  feature, so every other build rejects it as unknown. It sits in the sibling `capabilities-e2e/`, globbed only by
  feature builds (`DETAILS.md` § The E2E capability).

See the `tauri-apis` rule in `.claude/rules/` for the higher-level callout. Architecture, flows, and decisions:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
