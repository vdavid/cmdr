# Low disk space (frontend)

Frontend half of the low-disk-space warning. Wires the backend `low-disk-space` Tauri event (emitted by
`space_poller.rs` when the boot volume's free space crosses below the configured percent threshold) to the right user
surface: a persistent in-app WARN toast or a macOS native notification, per the
`behavior.fileSystemWatching.lowDiskSpaceNotifications` setting (`'in-app' | 'macos' | 'off'`).

Backend counterpart: the low-space section of [`src-tauri/src/space_poller.rs`](../../../src-tauri/src/space_poller.rs)
(permanent boot-volume watcher + hysteresis detector + the `set_low_disk_space_config` live-apply command).

## Architecture

| File                              | Purpose                                                                                                                              |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `notifications-mode.ts`           | Mode + threshold readers/writers, the Settings deep-link, and `pushLowDiskSpaceConfigToBackend()` (the applier's live-apply helper). |
| `event-bridge.svelte.ts`          | Listener bridge: one `low-disk-space` subscription, dispatches per the settings enum.                                                |
| `LowDiskSpaceToastContent.svelte` | Persistent WARN toast: snapshotted free space + percent, "Disable these notifications" action.                                       |

## Settings-gated dispatch

`startLowDiskSpaceEventBridge` (mounted from `routes/(main)/+page.svelte` next to the downloads bridge) reads
`getLowDiskSpaceNotificationsMode()` per event and fans out to:

- `'in-app'` → `addToast(LowDiskSpaceToastContent, ...)`: level `warn`, `dismissal: 'persistent'` (low disk space stays
  true until the user acts, so no auto-dismiss), dedup id `low-disk-space:<volumeId>` so a re-fire after the backend's
  hysteresis re-arms replaces the visible toast instead of stacking.
- `'macos'` → `sendNotification(...)` via the shared permission flow in
  `$lib/notifications/macos-notification-permission.ts` (one INFO toast on denial, no retries, setting stays put).
- `'off'` → no-op. Defense in depth: the backend removes its boot-volume watcher when the warning is off, so no event
  should arrive; the bridge bails anyway in case a settings flip races an in-flight emit.

There's no "Both" mode on purpose: a persistent in-app toast can't be missed, so pairing it with a native notification
adds noise without information.

## The two settings

Both live under **Behavior > File system watching > Low disk space** (`FileSystemWatchingSection.svelte`, anchor id
`LOW_DISK_SPACE_ANCHOR_ID` so the toast's "Disable these notifications" deep-link lands on the sub-group):

- `behavior.fileSystemWatching.lowDiskSpaceNotifications` (`'in-app'` default): the 3-option ToggleGroup.
- `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent` (default 5, range 1–50): the percent number input, greyed
  out while the mode is `'off'`.

The sub-group is NOT FDA-gated, unlike its Downloads siblings: the backend's space poller reads `statfs`, which needs no
TCC permission.

**Live-apply**: `settings-applier.ts` wires BOTH keys to `pushLowDiskSpaceConfigToBackend()`, which re-reads both
settings fresh and calls the `set_low_disk_space_config(enabled, thresholdPercent)` IPC (the AI-triplet shape: never
pass cached values). On the Rust side the command updates the poller's atomics, re-arms the hysteresis, and registers or
removes the permanent boot-volume watcher. Startup needs no frontend push: `lib.rs` seeds the poller from
`settings.json` via `loader.rs`.

## Toast shape

Props-only (`toastId`, `availableBytes`, `freePercent`), snapshotted at event arrival. "Disable these notifications"
flips the mode to `'off'` (the applier pushes the disable to the backend), dismisses the toast, and deep-links to the
Settings sub-group so the user sees where to re-enable it. The macOS native notification is text-only — the plugin can't
carry custom action buttons on desktop, so the disable affordance lives on the in-app toast and in Settings.
