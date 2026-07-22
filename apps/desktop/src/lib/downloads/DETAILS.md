# Downloads (frontend) details

Depth for the downloads frontend. `CLAUDE.md` holds the must-knows; this file holds the per-file rundown, toast
mechanics, deep-link wiring, and the smoke-test guide.

## Per-file rundown

- **`go-to-latest.ts`**: `goToLatestDownload(explorer)` consults the watcher ring with a Downloads-scan fallback;
  `goToDownload(explorer, dir, name)` jumps to a specific file.
- **`go-to-latest-ids.ts`**: dedup ids for the go-to-latest INFO toasts.
- **`event-bridge.svelte.ts`**: one `download-detected` subscription (`startDownloadsEventBridge`), dispatches per the
  settings enum, re-checks the FDA gate per event.
- **`DownloadToastContent.svelte`**: in-app toast: title with filename + size, optional subdir line, two snapshotted
  shortcut hints (in-app `⌘J` and global `⌃⌥⌘J`, each a literal `ShortcutChip`), `GlobalShortcutAnimation` for the
  default global combo, and a button row (secondary "Stop showing these" + primary "Jump to file"). Collapsible;
  auto-hides at 10s; `widthPx: 432` (wider than the 360 default so the animation reads). Carries
  `toastGroup: 'downloads'` so a burst evicts its own oldest transient first; the toast store's global cap shows at
  most 5.
- **`downloads-toast-collapsed.ts`**: getter/setter for the hidden `behavior.fileSystemWatching.downloadsToastCollapsed`
  setting. No Settings UI.
- **`download-toast-shortcuts.ts`**: pure `buildShortcutSummary(shortcutHint, globalBinding)` → `{ inApp, global }`
  (nullable) for the collapsed summary line. Unit-tested in isolation.
- **`GlobalShortcutAnimation.svelte`**: decorative looping keyboard SVG showing ⌃⌥⌘J pressed. Tokenized colors,
  `aria-hidden`, honors `prefers-reduced-motion` (static lit frame). Hard-coded to ⌃⌥⌘J.
- **`notifications-mode.ts`**: reader, writer, and deep-link helper for
  `behavior.fileSystemWatching.downloadsNotifications`. Exports `DOWNLOADS_NOTIFICATIONS_ANCHOR_ID`.
- **`global-shortcut-bridge.svelte.ts`**: one `global-shortcut-fired` subscription; calls `goToLatestDownload` plus, on
  the first un-acknowledged trigger, the warn toast.
- **`GlobalShortcutWarnToastContent.svelte`**: first-trigger persistent warn toast for ⌃⌥⌘J.
- **`global-shortcut-binding.ts`**: macOS-symbol binding → plugin accelerator (`⌘` → `Super`).
- **`global-shortcut-setting.ts`**: getters/setters for `behavior.fileSystemWatching.globalGoToLatestShortcut.*`.
  `setGlobalGoToLatestBinding` resets `acknowledged` to `false`.
- **`global-shortcut-description.ts`**: pure builder for the on/off toggle's helper text, tracking the live binding.
- **`GlobalShortcutRow.svelte`**: the go-to-latest hotkey as a `(global)`-marked row in Keyboard shortcuts. Recorder
  pill + reset; writes via `setGlobalGoToLatestBinding` then `set_global_go_to_latest_shortcut` for live-apply.
- **`LatestDownloadEmptyToastContent.svelte`**: INFO toast "Your Downloads folder is empty…" with "Go to Downloads".
- **`LatestDownloadFdaToastContent.svelte`**: INFO toast "Cmdr needs Full Disk Access…" with "Open System Settings".

## Collapsible toast

Two states, toggled by a chevron button:

- **Expanded** (default): full teaching view (intro line, both shortcut hints, `GlobalShortcutAnimation` for the default
  combo) plus an up-chevron under the animation.
- **Collapsed**: same title, one compact summary (`Jump with ⌘J in-app, ⌃⌥⌘J globally.`, dynamic on which shortcuts are
  set, keys as literal `ShortcutChip`s, from the pure `buildShortcutSummary`), and a down-chevron.

The action button row is identical in both states. The bridge passes `getDownloadsToastCollapsed()` as
`initialCollapsed`; the component holds the live toggle in local `$state` (seeded from it), and the chevron's `onclick`
calls `setDownloadsToastCollapsed(...)` to persist for the next toast. The `ToastItem` host forwards a `props` field
only to component-content toasts that opt in.

## Global go-to-latest hotkey

Default `⌃⌥⌘J`. The on/off switch lives under Behavior > Notifications > Go to latest download (a plain `Switch`, its
description references the live binding via `global-shortcut-description.ts`); the combo is edited under Keyboard
shortcuts via `GlobalShortcutRow.svelte`. Both surfaces call `set_global_go_to_latest_shortcut(enabled, binding)` for
live-apply.

**First-trigger warn toast**: persistent, level `warn`. Fires only when the hotkey triggered AND
`acknowledged === false`. The bridge flips `acknowledged = true` BEFORE opening the toast so back-to-back presses don't
queue duplicates. Buttons: "Keep it on" (dismiss) and "Turn it off" (`enabled = false` +
`setGlobalGoToLatestShortcut(false, ...)`).

## Deep-link target

`openSettingsToDownloadsNotifications` calls
`openSettingsWindow(['Behavior', 'Notifications'], DOWNLOADS_NOTIFICATIONS_ANCHOR_ID)`. The settings page reads the
optional anchor from the URL on cold-open and from the `navigate-to-section` event on already-open windows, then scrolls
the matching DOM id into view. The anchor id is the source-of-truth `DOWNLOADS_NOTIFICATIONS_ANCHOR_ID` from
`notifications-mode.ts`; the section component imports the same constant for its `<div id={…}>`.

## Settings registry note

The `behavior.fileSystemWatching.downloadsNotifications` registry entry holds the canonical default `'in-app'`. The
reader (`getDownloadsNotificationsMode`) wraps `getSetting` in a try/catch as belt-and-braces against a hand-edited
corrupt value; the catch path falls through to the same `'in-app'` default.

## Smoke-test guide

Run after any change touching the watcher, the go-to-latest action, the global hotkey, or the settings rows. Each step
is independent; stop after the ones that cover your change.

1. Start dev: `pnpm dev` at repo root.
2. Wait for the FDA gate to open (or it clears automatically if FDA is already granted).
3. `touch ~/Downloads/test1.txt` → expect a Downloads toast.
4. With neither pane on `~/Downloads`, click the toast body (outside the buttons) → the focused pane navigates to
   `~/Downloads` and selects `test1.txt`.
5. Pane reuse: open `~/Downloads` in the LEFT pane, focus the RIGHT pane, press `⌘J` → focus shifts left, cursor lands
   on `test1.txt`, right pane untouched. With the FOCUSED pane already on `~/Downloads`, press `⌘J` again → only the
   cursor moves, no re-navigation, no focus change.
6. Cmd-Tab to Chrome, press `⌃⌥⌘J` → Cmdr foregrounds and reveals `test1.txt` (reusing a pane on `~/Downloads`, else
   navigating the focused pane). The first trigger of the session shows the warn toast.
7. Click "Keep it on" → `acknowledged` flips to `true`; later triggers don't show the toast.
8. Copy five files via Cmdr into `~/Downloads` → expect NO toasts (Cmdr-own-write suppression).
9. In Settings > Behavior > Notifications, pick "macOS notifications". Allow the permission prompt. Drop a file in
   Terminal → expect a macOS notification (no in-app toast).
10. Pick "Both" → both surfaces. Pick "Neither" → neither.
11. Click "Stop showing these" on a toast → the setting flips to "Neither" and Settings opens scrolled to the sub-group.
12. Toggle "Go to latest download" off → press `⌃⌥⌘J` from Chrome, expect nothing. Toggle on → the jump works. The
    toggle's description reads the live binding.
13. In Settings > Keyboard shortcuts, find "Go to latest download (global)", set a new combo (for example `⌃⌥⌘K`) → the
    File-system-watching description updates and the warn toast re-fires on the next trigger (`acknowledged` resets on
    rebind). The `↩` reset returns it to `⌃⌥⌘J`.
14. Revoke FDA in System Settings → both sub-groups grey out with the shared FDA hint, the global hotkey unregisters,
    and pressing `⌘J` shows the FDA INFO toast (stable dedup id so spamming `⌘J` doesn't stack toasts).

## i18n

All user-facing copy in this area lives in `$lib/intl/messages/en/downloads.json` (prefix `downloads.*`), resolved via
`tString()` / `<Trans>` from `$lib/intl`; `cmdr/no-raw-user-facing-string` is enforced on `lib/downloads/`. Don't
hardcode copy. The download-toast sentences with inline `ShortcutChip`s / `<code>` / `<em>` use `<Trans>` (snippet per
tag; the chip snippets discard the tag's inner text and render a literal chip from the snapshotted binding). The
keyboard-animation SVG's key-cap labels are NOT copy (the lint skips SVG `<text>`). `GlobalShortcutRow`'s status line
carries a typed `statusIsWarn` flag for the warn styling (not a substring match on the localized status text). Base-en
output is parity-pinned by `downloads-i18n-parity.test.ts`.
