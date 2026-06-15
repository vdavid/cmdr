# Low disk space details

Depth for the frontend low-disk-space warning. `CLAUDE.md` holds the must-knows; the dispatch detail and Settings wiring
live here.

## Settings-gated dispatch

`startLowDiskSpaceEventBridge` (mounted from `routes/(main)/+page.svelte` next to the downloads bridge) reads
`getLowDiskSpaceNotificationsMode()` per event and fans out to:

- `'in-app'`: `addToast(LowDiskSpaceToastContent, ...)`, level `warn`, `dismissal: 'persistent'`, dedup id
  `low-disk-space:<volumeId>`.
- `'macos'`: `sendNotification(...)` via the shared permission flow in
  `$lib/notifications/macos-notification-permission.ts` (one INFO toast on denial, no retries, setting stays put). The
  native notification is text-only: the plugin can't carry custom action buttons on desktop, so the disable affordance
  lives only on the in-app toast and in Settings.
- `'off'`: no-op.

## The two settings

Both live under **Behavior > File system watching > Low disk space** (`FileSystemWatchingSection.svelte`, anchor id
`LOW_DISK_SPACE_ANCHOR_ID` so the toast's "Disable these notifications" deep-link lands on the sub-group):

- `behavior.fileSystemWatching.lowDiskSpaceNotifications` (default `'in-app'`): the three-option ToggleGroup.
- `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent` (default 5, UI range 1–50): the percent number input,
  greyed out while the mode is `'off'`.

`pushLowDiskSpaceConfigToBackend()` calls `setLowDiskSpaceConfig(mode !== 'off', threshold)`. On the Rust side the
command updates the poller's atomics, re-arms the hysteresis, and registers or removes the boot-volume watcher.

## Toast shape

Props-only (`toastId`, `availableBytes`, `freePercent`), snapshotted at event arrival. "Disable these notifications"
flips the mode to `'off'` (the applier pushes the disable to the backend), dismisses the toast, and deep-links to the
Settings sub-group so the user sees where to re-enable it.
