# Low disk space (frontend)

Frontend half of the low-disk-space warning. Wires the backend `low-disk-space` Tauri event to either a persistent
in-app WARN toast or a macOS native notification, per the `behavior.fileSystemWatching.lowDiskSpaceNotifications`
setting (`'in-app' | 'macos' | 'off'`).

Backend: the low-space section of [`src-tauri/src/space_poller.rs`](../../../src-tauri/src/space_poller.rs) (boot-volume
watcher, hysteresis detector, the `set_low_disk_space_config` live-apply command).

## Module map

- **`notifications-mode.ts`**: mode + threshold readers/writers, the Settings deep-link, and
  `pushLowDiskSpaceConfigToBackend()`.
- **`event-bridge.svelte.ts`**: one `low-disk-space` subscription; dispatches per the settings enum.
- **`LowDiskSpaceToastContent.svelte`**: the persistent WARN toast (free space + percent, "Disable these
  notifications").

## Must-knows

- **Live-apply re-reads both settings fresh, never cached.** `settings-applier.ts` wires both keys to
  `pushLowDiskSpaceConfigToBackend()`, which re-reads mode + threshold and calls
  `setLowDiskSpaceConfig(enabled, thresholdPercent)`. Don't thread cached values through. Startup needs no frontend
  push: `lib.rs` seeds the poller from `settings.json`.
- **The in-app toast is `dismissal: 'persistent'` with dedup id `low-disk-space:<volumeId>`.** Low disk space stays true
  until the user acts, so no auto-dismiss; the dedup id makes a re-fire (after hysteresis re-arms) replace the toast
  instead of stacking. Don't change either without rethinking both.
- **❌ No "both" mode, on purpose.** A persistent in-app toast can't be missed, so pairing it with a native notification
  is noise. Don't add a fourth enum value.
- **NOT FDA-gated** (unlike the Downloads siblings in the same Settings section): the backend poller reads `statfs`,
  which needs no TCC permission. Don't add a gate.
- **The bridge bails on `'off'` as defense in depth**: the backend removes its watcher when off, so no event should
  arrive, but the bridge re-checks the mode per event in case a settings flip races an in-flight emit.

Full details: [DETAILS.md](DETAILS.md).
