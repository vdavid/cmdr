# Downloads (frontend)

Frontend half of the downloads-watcher feature. Wires the backend `download-detected` Tauri event to the right surface
(in-app toast, macOS notification, both, or neither) and owns the go-to-latest navigation. Backend counterpart:
`apps/desktop/src-tauri/src/downloads/CLAUDE.md`.

## Module map

- **`event-bridge.svelte.ts`**: one `download-detected` subscription (`startDownloadsEventBridge`), fans out per the
  settings enum.
- **`global-shortcut-bridge.svelte.ts`**: one `global-shortcut-fired` subscription; calls `goToLatestDownload` plus the
  first-trigger warn toast.
- **`go-to-latest.ts`**: `goToLatestDownload(explorer)` (ring + scan fallback) and `goToDownload(explorer, dir, name)`
  (a specific file).
- **`DownloadToastContent.svelte`**: the teaching toast (collapsible; two shortcut hints; jump/stop buttons).
- Helpers and the `GlobalShortcut*` / `LatestDownload*` components: see the DETAILS.md per-file rundown.

## Settings-gated dispatch

`startDownloadsEventBridge` reads `getDownloadsNotificationsMode()` per event: `'in-app'` → toast only, `'macos'` →
`sendNotification` only, `'both'` → both, `'neither'` → no-op. The macOS path asks permission via the shared
`$lib/notifications/macos-notification-permission.ts` (session-cached, one INFO toast with a stable dedup id on denial,
no retries, and we DON'T flip the user's setting).

## Must-knows

- **Snapshot-at-creation**: both shortcut values are captured when the toast is created and passed as props; a remap
  between a toast appearing and the user clicking does NOT change what's shown (a stale hint would mismatch what the
  user pressed). The chips are literal mode, not `commandId` mode, precisely to preserve this. The one deliberate live
  `$state` is the collapse toggle (`initialCollapsed` only seeds it).
- **Skip-the-whole-toast edge case**: when NEITHER shortcut is teachable (in-app `⌘J` unbound AND global off/unbound),
  `dispatchToast` skips the in-app toast even when the mode isn't `'neither'`. The toast's reason to exist is teaching
  these shortcuts. A `'both'`-mode macOS notification still fires (separate surface, never carried a hint).
- **Two shortcut hints**: in-app `⌘J` (`getEffectiveShortcuts('downloads.goToLatest')[0]`, `''` when unbound) and global
  `⌃⌥⌘J` (passed only when the hotkey is BOTH enabled and bound, else `''`). `GlobalShortcutAnimation` renders ONLY when
  `globalBinding === DEFAULT_GLOBAL_GO_TO_LATEST_BINDING` (the SVG lights up the literal default keys, so a remapped
  combo would teach the wrong ones); a remapped combo keeps the text chip but drops the animation.
- **FDA defense-in-depth**: the watcher won't emit `download-detected` when the FDA gate is closed
  (`runtime::refresh_runtime`), but the bridge re-checks `commands.downloadsWatcherStatus()` per event before surfacing
  anything, guarding against a stale event during a gate flip. `goToLatestDownload` mirrors this.
- **`goToLatestDownload` vs `goToDownload`**: latest consults the watcher ring (Downloads-scan fallback when empty); the
  per-toast jump reveals the file THAT toast advertised, even after a newer download lands. The split matters for
  download bursts: each toast must reveal its own file.
- **Pane reuse**: all jump entry points reveal through `revealFileInBestPane` / `navigateToDirInBestPane`
  (`file-explorer/navigation/navigate-and-select.ts`), NOT `navigateToFileInPane`, so an already-open Downloads view
  isn't duplicated (the helpers move the cursor or shift focus instead). "Go to path" (⌘G) deliberately keeps
  always-navigate and does NOT reuse panes.
- **Global hotkey binding mapping**: `global-shortcut-binding.ts` translates the stored macOS-symbol form (`'⌃⌥⌘J'`) to
  the plugin accelerator (`'Control+Alt+Super+J'`). ⌘ maps to `Super` (global-hotkey rejects `Meta`). Registration
  lifecycle is backend; the FE owns the trigger handler.
- **`setGlobalGoToLatestBinding` resets `acknowledged` to `false`** so a rebound combo gets its own first-trigger warn
  toast. It's the single chokepoint: don't write `binding` through plain `setSetting`, that bypasses the reset.
- **The global binding's persistent home is `settings.json`** (key
  `behavior.fileSystemWatching.globalGoToLatestShortcut.binding`, `hidden`), NOT `shortcuts.json`: the Rust startup/
  focus refresh reads it from disk before any window loads, and `shortcuts.json` isn't reachable from that path.
- **The toast body is mouse-only click-to-jump, NOT keyboard-focusable** (the two buttons own keyboard activation). Both
  buttons and both collapse chevrons call `event.stopPropagation()` so the body jump doesn't also fire underneath (else
  "Stop showing these" would navigate before Settings opens).

Full details (per-file rundown, collapsible-toast states, first-trigger warn toast, deep-link target, settings-registry
note, and the smoke-test guide): `DETAILS.md`.
