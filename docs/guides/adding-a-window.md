# Adding a window

How to add a new top-level window (like Settings, the File viewer, or the Keyboard shortcuts help window). Read this
before creating a `WebviewWindow`, especially the capabilities section: missing perms fail **silently**.

A window is four pieces plus one decision:

- **A route**: `apps/desktop/src/routes/<name>/+page.svelte`. The app is a SPA (`ssr = false` in the root layout), so a
  new route dir just works, no config. This is the window's UI shell.
- **An opener**: `<feature>-window.ts` exporting `openXxxWindow()`, which constructs the `WebviewWindow`. Mirror an
  existing one: `lib/settings/settings-window.ts` (singleton, glass, text-size-scaled), `lib/file-viewer/open-viewer.ts`
  (multi-instance, cascading), or `lib/shortcuts/shortcuts-window.ts` (singleton, read-only). Reuse the shared helpers:
  `resolveChildPosition` (from `$lib/window-positioning`, places + clamps to the monitor), `decorateChildWindowTitle`
  (dev/worktree title suffix), `getEffectiveScale` (size the window to the app-wide text size), and the `focus-self`
  event so a singleton re-focuses instead of spawning a duplicate.
- **A capability file**: `src-tauri/capabilities/<name>.json`, listing the window label and the minimal Tauri perms the
  window's own code calls. See `src-tauri/capabilities/CLAUDE.md`. Grant only what it uses.
- **The route shell wiring** (in `+page.svelte`): init the stores it reads (`initializeSettings`, plus feature stores),
  `initAccentColor`, `initTextSize` (so it tracks the font-size setting), `trackOwnRect('<name>')` to remember
  position/size in-session, and an Escape handler that closes via `setTimeout(0)` (NOT `requestAnimationFrame`, which is
  throttled in unfocused windows on macOS). Hide the `loading-screen` element on mount.

## The capability gotcha: perms are checked against the _calling_ window

`new WebviewWindow(...)` and the calls around it are gated against **the window whose JS runs them**, and they reject
**silently**. So the window that _opens_ another window needs, in its own capability file:

- **`core:webview:allow-create-webview-window`**: construct the window.
- **`core:window:allow-get-all-windows`**: `WebviewWindow.getByLabel(...)` (the singleton check) calls this.
- **`core:window:allow-available-monitors`**: `readMonitors()` for positioning.
- **`core:window:allow-set-effects`**: only if you apply an `NSVisualEffectView` material (the Settings glass).

The main window has all of these. A focused child window usually should not.

## Least privilege: route the open through the main window

A read-only or hostile-content window (the viewer renders arbitrary files; the Keyboard shortcuts window is read-only)
should **not** get window-creation perms just to open Settings. Instead, emit an event the main window already listens
for, and let it open the target on your behalf. Example: the Keyboard shortcuts window's "Edit shortcuts" link calls
`requestOpenSettings('Keyboard shortcuts')` (in `$lib/tauri-commands`), which emits the shared `open-settings` event;
the main window's `onOpenSettings` handler calls `openSettingsWindow([...])`. The child window needs only
`core:event:default`. This keeps the capability split honest (see `capabilities/CLAUDE.md` for why that boundary
matters).

## Checklist

- **Route**: `routes/<name>/+page.svelte` with the shell wiring above.
- **Opener**: `<feature>-window.ts`, mirroring the closest existing window.
- **Capability**: `capabilities/<name>.json` with minimal perms (or just `core:event:default` if it routes opens through
  main).
- **Menu / command** (if user-launched): a `COMMAND_IDS` entry + registry entry + handler, and a menu item if it belongs
  in the menu bar. See `lib/commands/CLAUDE.md` § "Adding a command".
- **`await` window calls in try/catch with a `log.warn`** so a missing perm surfaces as a log line, not a dead feature.
