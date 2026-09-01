# Downloads (frontend) details

Depth for the downloads frontend. `CLAUDE.md` holds the must-knows; this file holds the per-file rundown, toast
mechanics, deep-link wiring, and the smoke-test guide.

## Per-file rundown

- **`go-to-latest.ts`**: `goToLatestDownload(explorer)` consults the watcher ring with a Downloads-scan fallback;
  `goToDownload(explorer, dir, name)` jumps to a specific file.
- **`go-to-latest-ids.ts`**: dedup ids for the go-to-latest INFO toasts.
- **`event-bridge.svelte.ts`**: one `download-detected` subscription (`startDownloadsEventBridge`), dispatches per the
  settings enum, re-checks the FDA gate per event, and coalesces the macOS banner (see § One toast at a time).
- **`DownloadToastContent.svelte`**: in-app toast: title with filename + size, optional subdir line, two snapshotted
  shortcut hints (in-app `⌘J` and global `⌃⌥⌘J`, each a literal `ShortcutChip`), `GlobalShortcutAnimation` for the
  default global combo, and a button row (secondary "Stop showing these" + primary "Jump to file"). Collapsible;
  auto-hides at 10s; `widthPx: 432` (wider than the 360 default so the animation reads). Carries
  `toastGroup: 'downloads'` with `maxInGroup: 1` (see § One toast at a time).
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
  pill (`$lib/settings/sections/ShortcutPill.svelte`, shared with the section's rows) + reset; writes via `setGlobalGoToLatestBinding` then `set_global_go_to_latest_shortcut` for live-apply.
- **`LatestDownloadEmptyToastContent.svelte`**: INFO toast "Your Downloads folder is empty…" with "Go to Downloads".
- **`LatestDownloadFdaToastContent.svelte`**: INFO toast "Cmdr needs Full Disk Access…" with "Open System Settings".

## Collapsible toast

Two states, toggled by a chevron button:

- **Expanded** (default): full teaching view (intro line, both shortcut hints, `GlobalShortcutAnimation` for the default
  combo) plus an up-chevron under the animation.
- **Collapsed**: same title, one compact summary (`Jump with ⌘J in-app, ⌃⌥⌘J globally.`, dynamic on which shortcuts are
  set, keys as literal `ShortcutChip`s, from the pure `buildShortcutSummary`), and a down-chevron.

`GlobalShortcutAnimation` renders ONLY when `globalBinding === DEFAULT_GLOBAL_GO_TO_LATEST_BINDING`: the SVG lights up
the literal default key caps, so a remapped combo would animate the wrong keys. A remapped combo therefore keeps the
text chip and drops the animation.

The action button row is identical in both states. The bridge passes `getDownloadsToastCollapsed()` as
`initialCollapsed`; the component holds the live toggle in local `$state` (seeded from it), and the chevron's `onclick`
calls `setDownloadsToastCollapsed(...)` to persist for the next toast. The `ToastItem` host forwards a `props` field
only to component-content toasts that opt in.

## One toast at a time

`dispatchToast` passes `maxInGroup: 1` alongside `toastGroup: 'downloads'`, so the toast store evicts the previous
downloads toast when a new detection arrives (`ui/DETAILS.md` § Toast system covers the group-cap mechanics). The
visible toast is therefore always the newest file, with a fresh 10s timer.

**Decision/Why**: the default group cap is 5, and a browser saving several files at once produced a stack of five
near-identical ~430px teaching toasts covering the pane. Only the newest matters: the toast teaches a shortcut, and the
teaching text is identical across a burst.

What the eviction costs, and why it's acceptable:

- **The evicted toast's jump target is lost.** Each toast jumps to the file IT advertised (`goToDownload`, not
  `goToLatestDownload`), so an evicted toast's file is no longer one click away. `⌘J` still reaches the newest file, and
  during a burst the user hadn't acted on the older toast anyway.
- **A hovered toast can be replaced.** The store's hover-pause keeps a toast alive past its timer, but a new detection
  still evicts it. Deliberate: "always the newest" wins over "never interrupt reading".
- **The collapse state survives.** It lives in the `behavior.fileSystemWatching.downloadsToastCollapsed` setting, not in
  the toast, so the replacement re-opens in the same state.
- **Rapid bursts mount and unmount intermediate toasts.** `ToastContainer` has no enter/leave transitions, so it's a
  frame-level swap, not visible churn. If that ever reads as flicker, coalesce in `dispatchToast` (hold the newest event
  for a few hundred ms) rather than raising the cap.

The macOS surface reaches the same "one at a time" outcome a different way, because it can't replace a delivered banner
(see § macOS notifications can't be deduped). It coalesces instead: `queueMacosNotification` folds detections into a
burst and `flushMacosBurst` sends ONE banner when the window closes.

- **`MACOS_COALESCE_MS` is 400.** Long enough for a browser saving several files in one go, short enough that a lone
  download doesn't feel delayed.
- **It's a fixed window opened by the first event, not a debounce that restarts on each one.** A restarting debounce
  would let a sustained stream (a torrent client unpacking) push the deadline out forever and never notify. A stream
  instead gets one banner per window: a bound rather than silence.
- **The permission prompt moved behind the window.** A burst the user gets no banner for shouldn't cost them a prompt.
- **The toast is NOT coalesced.** It dispatches per event and relies on the store's group cap, so it still appears
  instantly. Only the OS banner waits.
- **Teardown cancels a pending burst** (the unlisten returned by `startDownloadsEventBridge` wraps
  `cancelPendingMacosBurst`), so a window that outlives the layout can't fire.

Wording, in `describeMacosBurst`: one file keeps the original phrasing (name in the title, folder in the body); a
coalesced burst uses `downloads.notification.titleMultiple` ("Downloaded 3 files") with
`downloads.notification.mostRecent` naming the newest. The subdir line is dropped for a burst on purpose: its files can
come from different folders, so naming one would be a claim about the others that isn't true.

## macOS notifications can't be deduped

There is no way to make a new banner REPLACE a delivered one through our current stack, so don't go looking for a flag.
(This is why the macOS path coalesces; see § One toast at a time.)

(Verified 2026-07-28 by reading the pinned crate sources in `~/.cargo/registry` and the upstream repos, plus a bundled
Swift spike for the OS behavior.)

The blocker is the plumbing between us and the OS, NOT macOS:

- `@tauri-apps/plugin-notification`'s `Options` has `id?: number` ("the notification identifier to reference this object
  later") and `group?: string` (documented against Apple's `threadIdentifier`), which reads like it should work.
- It doesn't: those fields are MOBILE-only. `tauri-plugin-notification` 2.3.3's desktop `NotificationBuilder::show`
  forwards exactly four fields to `notify_rust`: title, body, icon, sound. `id`, `group`, `extra`, and the rest are
  dropped before they reach the OS.
- `notify-rust` 4.18.0's legacy macOS backend ignores `notification.id` too (its doc comment claims "XDG, Windows, and
  legacy macOS"; only XDG actually reads it, as the D-Bus `replaces_id`).
- `mac-notification-sys` 0.6.15 DOES set `NSUserNotification.identifier` — from a random UUID it generates per send for
  response correlation. Right mechanism, not caller-controllable.

macOS itself is fine with this: delivering an `NSUserNotification` whose `identifier` matches an already-delivered one
REPLACES it (`deliveredNotifications` stays at one entry, content updates), while a different identifier stacks.
Verified on Darwin 25.5.0 with a minimal bundled Swift app, 2026-07-28. So the deprecated API Tauri already uses would
carry this if the three layers passed an identifier through; the `preview-macos-un` / `UNUserNotificationCenter` route
isn't needed. Fixing it means PRs to three repos, which we decided against.

Related dev-mode quirk: the plugin calls `notify_rust::set_application("com.apple.Terminal")` under `tauri::is_dev()`,
so notifications from a dev build are attributed to Terminal, not Cmdr. Test this surface in a release build.

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
`openSettingsWindow('downloads-toast', ['Behavior', 'Notifications'], DOWNLOADS_NOTIFICATIONS_ANCHOR_ID)`. The settings
page reads the optional anchor from the URL on cold-open and from the `navigate-to-section` event on already-open
windows, then scrolls the matching DOM id into view. The anchor id is the source-of-truth
`DOWNLOADS_NOTIFICATIONS_ANCHOR_ID` from `notifications-mode.ts`; the section component imports the same constant for
its `<div id={…}>`.

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
