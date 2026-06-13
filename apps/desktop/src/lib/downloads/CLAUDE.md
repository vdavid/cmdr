# Downloads (frontend)

Frontend half of the downloads-watcher feature. Wires the backend `download-detected` Tauri event to the right user
surface (in-app toast, macOS native notification, both, or neither) and owns the "Go to latest download" / "Go to this
specific file" navigation helpers.

Backend counterpart: [`src-tauri/src/downloads/CLAUDE.md`](../../../src-tauri/src/downloads/CLAUDE.md).

## Architecture

- **`go-to-latest.ts`**: `goToLatestDownload(explorer)` consults ring + scan fallback.
  `goToDownload(explorer, dir, name)` jumps to a specific file.
- **`LatestDownloadEmptyToastContent.svelte`**: INFO toast: "Your Downloads folder is empty…" with a "Go to Downloads"
  action.
- **`LatestDownloadFdaToastContent.svelte`**: INFO toast: "Cmdr needs Full Disk Access…" with an "Open System Settings"
  action.
- **`go-to-latest-ids.ts`**: Dedup ids for the go-to-latest INFO toasts.
- **`event-bridge.svelte.ts`**: Listener bridge: one `download-detected` subscription, dispatches per the settings enum.
- **`DownloadToastContent.svelte`**: In-app toast: title with filename + size, optional subdir line, two snapshotted
  shortcut hints (in-app `⌘J` and global `⌃⌥⌘J`, each a literal `ShortcutChip`), `GlobalShortcutAnimation` for the
  default global combo, and a right-aligned `Button` row (secondary "Stop showing these" + primary "Jump to file" at the
  far right). Collapsible: a chevron toggles between the full teaching view and a compact one-line summary; the choice
  persists for the NEXT toast. Auto-hides (transient, 10s) and wider than default (`widthPx`) so the animation reads.
- **`downloads-toast-collapsed.ts`**: Getter/setter for the hidden `behavior.fileSystemWatching.downloadsToastCollapsed`
  setting. The bridge reads it to seed each new toast; the toast's chevron writes it. No Settings UI.
- **`download-toast-shortcuts.ts`**: Pure `buildShortcutSummary(shortcutHint, globalBinding)` → `{ inApp, global }`
  (nullable). Maps the two snapshotted bindings to the chips the collapsed summary line renders. Unit-tested in
  isolation.
- **`GlobalShortcutAnimation.svelte`**: Decorative looping keyboard SVG that shows ⌃⌥⌘J being pressed. Tokenized colors
  (reads on light + dark info toasts), accent-lit press, `aria-hidden`. Hard-coded to ⌃⌥⌘J: the toast renders it ONLY
  for the default global binding. Honors `prefers-reduced-motion` (static lit frame).
- **`notifications-mode.ts`**: Reader, writer, and deep-link helper for
  `behavior.fileSystemWatching.downloadsNotifications`.
- **`global-shortcut-bridge.svelte.ts`**: One `global-shortcut-fired` Tauri event subscription. Calls
  `goToLatestDownload` plus, on first un-acknowledged trigger, the warn toast.
- **`GlobalShortcutWarnToastContent.svelte`**: First-trigger persistent warn toast for ⌃⌥⌘J. "Keep it on" / "Turn it
  off" buttons. Snapshotted binding prop.
- **`global-shortcut-binding.ts`**: Translates the macOS-symbol binding (`'⌃⌥⌘J'`) into the accelerator string the
  plugin understands (`'Control+Alt+Super+J'`). ⌘ maps to `Super` (global-hotkey rejects `Meta`).
- **`global-shortcut-setting.ts`**: Narrow getters/setters for `behavior.fileSystemWatching.globalGoToLatestShortcut.*`.
  **`setGlobalGoToLatestBinding` resets `acknowledged` to `false`**: the new combo deserves the first-trigger warning
  again.
- **`global-shortcut-description.ts`**: Pure builder for the on/off toggle's helper text. Given the live binding,
  returns "Press ⌃⌥⌘J from any app to jump to your most recent download." so the description tracks rebinds.
- **`GlobalShortcutRow.svelte`**: The go-to-latest hotkey as a shortcut row in `Keyboard shortcuts`, marked `(global)`.
  Recorder pill + reset. Writes via `setGlobalGoToLatestBinding`, then `set_global_go_to_latest_shortcut` for
  live-apply.

## Settings-gated dispatch

`startDownloadsEventBridge` reads `getDownloadsNotificationsMode()` per event and fans out to:

- `'in-app'` → `addToast(DownloadToastContent, ...)` only.
- `'macos'` → `sendNotification(...)` from `@tauri-apps/plugin-notification` only.
- `'both'` → both.
- `'neither'` → no-op.

The macOS native path asks the OS for permission via the shared `$lib/notifications/macos-notification-permission.ts`
helper (also used by `lib/low-disk-space/`): session-cached answer, a single INFO toast with a stable dedup id on
denial, no retries, and we DON'T flip the user's setting. The user can re-enable in System Settings whenever; their
preference stays put.

## Two shortcut hints on the toast

The toast teaches BOTH go-to-latest shortcuts, each as a literal-mode `ShortcutChip` (non-clickable), so neither chip
re-renders live (see the snapshot rule below):

- **In-app `⌘J`** (`shortcutHint` prop): `getEffectiveShortcuts('downloads.goToLatest')[0]` at creation time. Hidden
  (`''`) only when the command is unbound.
- **Global `⌃⌥⌘J`** (`globalBinding` prop): the from-any-app hotkey. The bridge passes the binding only when the hotkey
  is BOTH enabled and bound; otherwise `''`, which hides the whole global line. When `globalBinding` still equals
  `DEFAULT_GLOBAL_GO_TO_LATEST_BINDING`, the toast also renders `GlobalShortcutAnimation`; a remapped combo keeps the
  text chip but drops the animation (the SVG lights up the literal default keys, so it would teach the wrong ones).

**Skip-the-whole-toast edge case.** When NEITHER shortcut is teachable (in-app unbound AND global off/unbound),
`dispatchToast` skips the in-app toast outright, even if downloads notifications aren't set to `'neither'` — the toast's
reason to exist is teaching these shortcuts. A `'both'`-mode macOS notification still fires (separate surface, never
carried a hint).

**Auto-hides + wide.** `dispatchToast` sends the toast `timeoutMs: 10_000` (a normal transient toast: auto-hides after
10s, pausing while hovered per the toast store's hover behavior) and `widthPx: 432` (wider than the 360 default so the
keyboard animation reads). At most 5 are visible at once via the toast store's global cap, automatic for transient
toasts (no `maxInGroup` is set). It still carries `toastGroup: 'downloads'` so a burst evicts its own oldest transient
first rather than pushing unrelated toasts off-screen.

## Collapsible toast

The toast can render in two states, toggled by a small chevron button:

- **Expanded** (default look): the full teaching view (intro line, both shortcut hints, and the
  `GlobalShortcutAnimation` for the default global combo), plus an up-chevron under the animation to collapse it.
- **Collapsed**: the same title line, a single compact summary (`Jump with ⌘J in-app, ⌃⌥⌘J globally.`, dynamic on which
  shortcuts are set, with the keys as literal `ShortcutChip`s), and a down-chevron to expand. The summary's nullable key
  map comes from the pure `buildShortcutSummary` in `download-toast-shortcuts.ts`.

The action button row ("Stop showing these" / "Jump to file") is identical in both states.

**State + persistence.** The bridge reads `getDownloadsToastCollapsed()` and passes it as the `initialCollapsed` prop,
so a NEW toast opens in the state the user last left. The component holds the live toggle in local `$state` (seeded from
`initialCollapsed`); the chevron's `onclick` flips it and calls `setDownloadsToastCollapsed(...)` to persist for the
next toast. The setting (`behavior.fileSystemWatching.downloadsToastCollapsed`, `hidden` in the registry) has no
Settings UI. Both chevrons `stopPropagation` so toggling doesn't trigger the whole-body click-to-jump.

## Snapshot-at-creation rule

Both shortcut values are snapshotted at toast-creation time and passed as props. A remap that happens between this toast
appearing and the user clicking does NOT change what's displayed — that would be confusing, because a hint would no
longer match what the user actually pressed when the toast first showed up. The next toast picks up the new binding
naturally. (The chips are literal, not `commandId` mode, precisely to preserve this snapshot semantic; a `commandId`
chip would re-render live.)

Mostly prop-driven: the toast component reads `event`, `shortcutHint`, `globalBinding`, `explorer`, `toastId`, and
`initialCollapsed` once on mount, with no live subscriptions to app state. The one piece of deliberate local `$state` is
the collapse toggle: `initialCollapsed` only SEEDS it, and the user flips it on this very toast (the snapshot semantic
would be wrong here, the toggle must respond live to the click). That choice is written back to the persisted setting so
the NEXT toast's `initialCollapsed` reflects it. The `ToastItem` host extends the toast store with a `props` field (see
`lib/ui/toast/`) which is forwarded only to component-content toasts that opt in; existing toasts that don't pass
`props` keep their zero-prop shape.

## Go-to-by-path vs go-to-latest

- **`goToLatestDownload(...)`**: The user pressed ⌘J (or another "jump to latest" affordance). Consults the watcher's
  ring; falls back to a Downloads scan when the ring is empty.
- **`goToDownload(explorer, ...)`**: The user clicked or pressed Jump on a SPECIFIC toast. Takes them to the file the
  toast was for, even if a newer download has landed and become "the latest."

The split matters when a burst of downloads arrives: each toast must take the user to the file IT advertised, not to
whichever file is most recent at click time.

**Pane reuse.** All jump entry points (`goToLatestDownload`, `goToDownload`, and the empty-toast "Go to Downloads"
action) reveal the target through the `revealFileInBestPane` / `navigateToDirInBestPane` helpers in
`file-explorer/navigation/navigate-and-select.ts`, NOT the plain `navigateToFileInPane`. If a pane already shows the
file's `parentDir`, those helpers move the cursor there (focused pane) or shift focus to the other pane — no
re-navigation, so an already-open Downloads view isn't duplicated. Only when neither pane shows the dir do they navigate
the focused pane. The match is the pane's ACTIVE tab only and volume-safe (a real local volume that contains the path;
an MTP or network pane at a same-looking path never counts). The empty-toast action snapshots the Downloads dir at
toast-add time but re-evaluates which pane shows it at CLICK time. "Go to path" (⌘G) deliberately keeps the
always-navigate behavior and does NOT reuse panes.

## FDA defense-in-depth

The watcher won't emit `download-detected` when the FDA gate is closed — that's the contract enforced in
`runtime::refresh_runtime`. The bridge re-checks the gate per event anyway (one `commands.downloadsWatcherStatus()`
call) before surfacing any toast or OS notification. This guards against a stale event slipping through during a gate
flip and mirrors the same defensive shape `goToLatestDownload` uses.

## Clickability shape

The downloads toast is whole-body clickable for mouse, but the clickable surface is NOT keyboard-focusable. The two
explicit buttons inside ("Jump to file" and "Stop showing these") own keyboard activation independently; the body click
is a mouse-only convenience.

Both buttons call `event.stopPropagation()` in their click handlers so the body-click jump doesn't also fire underneath
(otherwise "Stop showing these" would navigate to the file before the Settings window came up).

## Global go-to-latest hotkey

The default global hotkey is `⌃⌥⌘J`. Registration lifecycle lives in the backend (`refresh_global_go_to_latest_shortcut`
runs at startup, on focus, and when the Settings UI toggles); the FE bridge owns the trigger handler.

**Where the user controls it.** The on/off switch lives under `Behavior > File system watching > Go to latest download`
(a plain `Switch`; its description references the live binding via `global-shortcut-description.ts`). The combo itself
is edited under `Keyboard shortcuts`, rendered by `GlobalShortcutRow.svelte` as a row marked `(global)`. Both surfaces
call the `set_global_go_to_latest_shortcut(enabled, binding)` IPC on change for live-apply. The binding's persistent
home stays in `settings.json` (key `behavior.fileSystemWatching.globalGoToLatestShortcut.binding`, `hidden` in the
registry) because the Rust startup/focus refresh reads it from disk before any window loads — `shortcuts.json` isn't
reachable from that path.

**First-trigger warn toast.** Persistent, level `warn`. Fires only when the hotkey triggered AND
`acknowledged === false`. The bridge flips `acknowledged = true` BEFORE opening the toast so back-to-back presses don't
queue duplicates. The toast itself only carries the binding string snapshot and the two buttons — "Keep it on" (dismiss)
and "Turn it off" (flip `enabled = false` + call `setGlobalGoToLatestShortcut(false, ...)` IPC).

**Acknowledged reset.** When the user rebinds via `setGlobalGoToLatestBinding`, the helper resets `acknowledged = false`
so the new combo gets its own first-trigger warning. Single chokepoint — don't write `binding` through plain
`setSetting`; that bypasses the reset.

## Settings registry note

The `behavior.fileSystemWatching.downloadsNotifications` registry entry holds the canonical default `'in-app'`. The
reader (`getDownloadsNotificationsMode`) wraps `getSetting` in a try/catch as belt-and-braces against a corrupt stored
value (the registry guarantees the default, but a hand-edited `settings.json` could land here); the catch path falls
through to the same `'in-app'` default.

## Deep-link target

`openSettingsToDownloadsNotifications` calls
`openSettingsWindow(['Behavior', 'File system watching'], DOWNLOADS_NOTIFICATIONS_ANCHOR_ID)`. The settings page reads
the optional anchor from the URL on cold-open and from the `navigate-to-section` event on already-open windows, then
scrolls the matching DOM id into view. The anchor id is the source-of-truth `DOWNLOADS_NOTIFICATIONS_ANCHOR_ID` constant
exported from `notifications-mode.ts`; the section component imports the same constant for its `<div id={…}>` wrapper,
so renaming flows through one place.

## Smoke test guide

Run through this list after any change that touches the downloads watcher, the go-to-latest action, the global hotkey,
or the settings rows. Each step is independent; you can stop after the ones that cover your change.

1. Start dev: `pnpm dev` at repo root.
2. Wait for the FDA gate to open (existing onboarding). If FDA is already granted in System Settings, the gate clears
   automatically.
3. Drop a file via Terminal: `touch ~/Downloads/test1.txt` → expect a Downloads toast in Cmdr.
4. With neither pane on `~/Downloads`, click the toast body (anywhere outside the buttons) → the focused pane navigates
   to `~/Downloads` and selects `test1.txt`.
5. Pane reuse: open `~/Downloads` in the LEFT pane, focus the RIGHT pane, then press `⌘J` → focus shifts to the left
   pane and the cursor lands on `test1.txt`; the right pane is untouched (no duplicate Downloads view). Now with the
   FOCUSED pane already on `~/Downloads`, press `⌘J` again → just the cursor moves, no re-navigation and no focus
   change.
6. `Cmd-Tab` to Chrome, press `⌃⌥⌘J` → Cmdr foregrounds and reveals `test1.txt` (reusing a pane already on
   `~/Downloads`, else navigating the focused pane). The first trigger of this session shows the warn toast ("The ⌃⌥⌘J
   shortcut jumps to your latest download from anywhere. Keep it on?").
7. Click "Keep it on" on the warn toast → `acknowledged` flips to `true`; subsequent triggers don't show the toast.
8. Copy five files via Cmdr into `~/Downloads` (Cmd+C + Cmd+V or drag) → expect NO downloads toasts (Cmdr-own-write
   suppression).
9. In **Settings > Behavior > File system watching**, pick "macOS notifications" under "Downloads notifications". macOS
   asks for notification permission. Allow. Drop another file in Terminal → expect a macOS notification (no in-app toast
   for this event).
10. Pick "Both" → expect both surfaces. Pick "Neither" → expect neither.
11. Click "Stop showing these" on a Downloads toast → the setting flips to "Neither" and Settings opens scrolled to the
    right sub-group.
12. In **Settings > Behavior > File system watching**, toggle "Go to latest download" off → press `⌃⌥⌘J` from Chrome,
    expect nothing. Toggle on again → expect the jump to work. The toggle's description should read the live binding.
13. In **Settings > Keyboard shortcuts**, find "Go to latest download (global)", click its pill, press a new combo (for
    example `⌃⌥⌘K`) → the description in File system watching updates to the new combo, and the warn toast re-fires on
    the next trigger because `acknowledged` resets on rebind. The `↩` reset returns it to `⌃⌥⌘J`.
14. Revoke FDA in System Settings → return to Cmdr. The two sub-groups grey out with the shared FDA hint. The global
    hotkey unregisters. Pressing `⌘J` from Cmdr shows the FDA INFO toast (with a stable dedup id so spamming `⌘J`
    doesn't stack toasts).
