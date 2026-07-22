# Low disk space details

Depth for the frontend low-disk-space warning. `CLAUDE.md` holds the must-knows; the dispatch detail and Settings wiring
live here.

## The two hysteresis edges

The backend `low-disk-space` event fires on both edges of the detector, distinguished by `isLow`:

- `isLow: true`: free space crossed below the threshold. Show the warning.
- `isLow: false`: free space recovered above threshold + the backend's 1% re-arm margin. Dismiss the warning.

The backend owns the boundary (the margin is business logic), so the frontend never re-derives "recovered" from a raw
percent compare; it just acts on the edge. This keeps auto-dismiss aligned exactly with the hysteresis re-arm: the same
condition that re-arms the detector emits the dismiss.

## Settings-gated dispatch

`startLowDiskSpaceEventBridge` (mounted from `routes/(main)/+page.svelte` next to the downloads bridge) reads
`getLowDiskSpaceNotificationsMode()` per event and fans out to:

- `'in-app'`: `isLow: true` → `addToast(LowDiskSpaceToastContent, ...)`, level `warn`, `dismissal: 'persistent'`, dedup
  id `low-disk-space:<volumeId>`. `isLow: false` → `dismissToast(low-disk-space:<volumeId>)` (a no-op if the user
  already closed it).
- `'macos'`: `isLow: true` → `sendNotification(...)` via the shared permission flow in
  `$lib/notifications/macos-notification-permission.ts` (one INFO toast on denial, no retries, setting stays put). The
  native notification is text-only: the plugin can't carry custom action buttons on desktop, so the disable affordance
  lives only on the in-app toast and in Settings. `isLow: false` → no-op: a delivered notification can't be recalled or
  live-updated, which is why live-follow and auto-dismiss are in-app-only.
- `'off'`: no-op.

## The two settings

Both live under **Behavior > File system watching > Low disk space** (`FileSystemWatchingSection.svelte`, anchor id
`LOW_DISK_SPACE_ANCHOR_ID` so the toast's "Disable these notifications" deep-link lands on the sub-group):

- `behavior.fileSystemWatching.lowDiskSpaceNotifications` (default `'in-app'`): the three-option ToggleGroup.
- `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent` (default 5, UI range 1–50): the percent number input,
  greyed out while the mode is `'off'`.

`pushLowDiskSpaceConfigToBackend()` calls `setLowDiskSpaceConfig(mode !== 'off', threshold)`. On the Rust side the
command updates the poller's atomics, re-arms the hysteresis, and registers or removes the boot-volume watcher.

## Toast shape and live-follow

Props: `toastId`, `volumeId`, `availableBytes`, `totalBytes`. The bytes seed the readout at show time; the component
then subscribes to `onVolumeSpaceChanged` (filtered to its `volumeId`) and updates the numbers live as the disk fills or
drains. That stream already flows for the boot volume: the poller's permanent `low-space:boot` watcher keeps
`volume-space-changed` emitting every tick while the warning is on, independent of what the panes show. The percent is
computed on the frontend from `available / total` (mirroring the backend's `free_percent`, including the
`total == 0 → 100` guard), so no pre-baked percent crosses the IPC boundary. The listener is unsubscribed on destroy,
with a `disposed` guard for the case where the toast is dismissed before the async `listen` resolves.

"Disable these notifications" flips the mode to `'off'` (the applier pushes the disable to the backend), dismisses the
toast, and deep-links to the Settings sub-group so the user sees where to re-enable it.

## i18n

All user-facing copy here lives in `$lib/intl/messages/en/lowDiskSpace.json` (prefix `lowDiskSpace.*`), resolved via
`tString()` from `$lib/intl`; `cmdr/no-raw-user-facing-string` is enforced on `lib/low-disk-space/`. Don't hardcode
copy. Base-en output is parity-pinned by `low-disk-space-i18n-parity.test.ts`.
