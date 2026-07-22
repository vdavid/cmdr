# Low disk space (frontend)

Frontend half of the low-disk-space warning. The backend `low-disk-space` event carries both hysteresis edges via
`isLow`: `true` (free space fell below the threshold) shows a persistent in-app WARN toast or a macOS native
notification; `false` (recovered above the re-arm margin) dismisses the in-app toast. Dispatch follows the
`behavior.fileSystemWatching.lowDiskSpaceNotifications` setting (`'in-app' | 'macos' | 'off'`).

Backend: the low-space section of `apps/desktop/src-tauri/src/space_poller.rs` (boot-volume watcher, hysteresis
detector, the `set_low_disk_space_config` live-apply command).

## Module map

- **`notifications-mode.ts`**: mode + threshold readers/writers, the Settings deep-link, and
  `pushLowDiskSpaceConfigToBackend()`.
- **`event-bridge.svelte.ts`**: one `low-disk-space` subscription; shows/dismisses per `isLow` and the settings enum.
- **`LowDiskSpaceToastContent.svelte`**: the persistent WARN toast (free space + percent, "Disable these
  notifications"); live-follows the boot volume's space.

## Must-knows

- **Live-apply re-reads both settings fresh, never cached.** `settings-applier.ts` wires both keys to
  `pushLowDiskSpaceConfigToBackend()`, which re-reads mode + threshold and calls
  `setLowDiskSpaceConfig(enabled, thresholdPercent)`. Don't thread cached values through. Startup needs no frontend
  push: `lib.rs` seeds the poller from `settings.json`.
- **The in-app toast auto-dismisses on the backend recovery edge, not on a timer.** It's `dismissal: 'persistent'`
  (never self-expires) with dedup id `low-disk-space:<volumeId>`; the bridge dismisses it by that id when `isLow: false`
  arrives. The macOS mode can't recall a delivered notification, so it no-ops on recovery. Keep dismiss keyed off the
  backend edge, not a frontend threshold compare, so it aligns exactly with the hysteresis re-arm.
- **The toast owns its own live-follow subscription** to `volume-space-changed` (filtered to its `volumeId`), seeded
  from the show-time snapshot. It computes the percent from `available / total`, mirroring the backend. Pass it
  `volumeId` + `totalBytes` (not a pre-baked `freePercent`), and don't drop the listener cleanup.
- **❌ No "both" mode, on purpose.** A persistent in-app toast can't be missed, so pairing it with a native notification
  is noise. Don't add a fourth enum value.
- **NOT FDA-gated** (unlike the Downloads siblings in the same Settings section): the backend poller reads `statfs`,
  which needs no TCC permission. Don't add a gate.
- **The bridge bails on `'off'` as defense in depth**: the backend removes its watcher when off, so no event should
  arrive, but the bridge re-checks the mode per event in case a settings flip races an in-flight emit.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
