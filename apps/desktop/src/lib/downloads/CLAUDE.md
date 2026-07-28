# Downloads (frontend)

Frontend half of the downloads watcher. Wires the backend `download-detected` event to the right surface (toast, macOS
notification, both, neither) and owns go-to-latest navigation. Backend counterpart:
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

`startDownloadsEventBridge` reads `getDownloadsNotificationsMode()` per event: `'in-app'` → toast, `'macos'` →
`sendNotification`, `'both'` → both, `'neither'` → no-op. The macOS path asks permission via
`$lib/notifications/macos-notification-permission.ts` (session-cached, one deduped INFO toast on denial, no retries, and
we DON'T flip the user's setting).

## Must-knows

- **Snapshot-at-creation**: both shortcut values are captured when the toast is created and passed as props; a remap
  while a toast is up does NOT change what it shows (a stale hint would mismatch what the user pressed). Hence literal
  chips, not `commandId` mode. The one deliberate live `$state` is the collapse toggle.
- **Skip-the-whole-toast edge case**: when NEITHER shortcut is teachable (in-app `⌘J` unbound AND global off/unbound),
  `dispatchToast` skips the toast even when the mode isn't `'neither'` — teaching them is its reason to exist. A
  `'both'`-mode macOS notification still fires (separate surface, never carried a hint).
- **Two shortcut hints**: in-app `⌘J` (`getEffectiveShortcuts('downloads.goToLatest')[0]`, `''` when unbound) and global
  `⌃⌥⌘J` (`''` unless the hotkey is BOTH enabled and bound). `GlobalShortcutAnimation` renders ONLY for the default
  global combo, because the SVG lights up literal keys.
- **FDA defense-in-depth**: the watcher won't emit when the FDA gate is closed (`runtime::refresh_runtime`), but the
  bridge re-checks `commands.downloadsWatcherStatus()` per event anyway, guarding a stale event during a gate flip.
  `goToLatestDownload` mirrors this.
- **One toast at a time**: the bridge passes `maxInGroup: 1`, so a new detection evicts the previous one; the visible
  toast is always the newest file. Don't raise it: a burst otherwise stacks five ~430px toasts.
- **The macOS banner coalesces instead** (`MACOS_COALESCE_MS`: a fixed 400ms window → one banner, count in the title):
  nothing we send reaches the OS as a replaceable identifier. The toast is NOT coalesced.
- **`goToLatestDownload` vs `goToDownload`**: latest consults the watcher ring (Downloads-scan fallback when empty); the
  per-toast jump reveals the file THAT toast advertised, which can differ from the ring's latest (a detection whose
  toast was skipped for having no shortcut to teach still updates the ring).
- **Pane reuse**: all jump entry points reveal through `revealFileInBestPane` / `navigateToDirInBestPane`
  (`file-explorer/navigation/navigate-and-select.ts`), NOT `navigateToFileInPane`, so an already-open Downloads view
  isn't duplicated. "Go to path" (⌘G) deliberately does NOT reuse panes.
- **Global hotkey binding mapping**: `global-shortcut-binding.ts` translates the stored macOS-symbol form (`'⌃⌥⌘J'`) to
  the plugin accelerator (`'Control+Alt+Super+J'`). ⌘ maps to `Super` (global-hotkey rejects `Meta`). Registration
  lifecycle is backend; the FE owns the trigger handler.
- **`setGlobalGoToLatestBinding` resets `acknowledged` to `false`** so a rebound combo gets its own first-trigger warn
  toast. It's the single chokepoint: don't write `binding` through plain `setSetting`, that bypasses the reset.
- **The global binding's persistent home is `settings.json`** (key
  `behavior.fileSystemWatching.globalGoToLatestShortcut.binding`, `hidden`), NOT `shortcuts.json`: the Rust startup/
  focus refresh reads it from disk before any window loads, and `shortcuts.json` isn't reachable from that path.
- **The toast body is mouse-only click-to-jump, NOT keyboard-focusable** (the two buttons own keyboard activation). Both
  buttons and both chevrons `stopPropagation()`, else "Stop showing these" would navigate before Settings opens.

Full details (per-file rundown, collapsible-toast states, first-trigger warn toast, deep-link target, settings-registry
note, and the smoke-test guide): `DETAILS.md`.
