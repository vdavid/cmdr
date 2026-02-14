# MTP frontend integration

UI and state management for Android device file browsing via MTP (Media Transfer Protocol).

## Architecture

- **State**: `mtp-store.svelte.ts` — Reactive device list, connection status, Tauri event listeners
- **Path utils**: `mtp-path-utils.ts` — Parse/construct MTP paths (`mtp://{deviceId}:{storageId}/path`)
- **Dialog**: `PtpcameradDialog.svelte` — macOS-specific helper for `ptpcamerad` workaround (shows Terminal command)
- **Backend**: See `src-tauri/src/mtp/` for device management, file operations, event loop

## Key decisions

### Path format: `mtp://{deviceId}:{storageId}/path`

- `deviceId` — Unique USB device identifier (vendor:product)
- `storageId` — MTP storage ID (hex, like `0x00010001` for Internal Storage)
- `path` — Virtual filesystem path on device (e.g., `/DCIM/Camera`)

Multiple storages (Internal + SD Card) become separate volumes in UI. Each has distinct volume ID.

### Event-driven refresh

Listen to `mtp-directory-changed` events from backend. When device emits MTP `ObjectAdded/Removed/Changed`, backend
sends event with `deviceId`. Frontend re-fetches current directory if viewing that device.

### Graceful ptpcamerad handling

On macOS, `ptpcamerad` daemon auto-claims devices. When exclusive access error:

1. Backend queries IORegistry (`ioreg`) for blocking process name
2. Emits `mtp-exclusive-access-error` event with process info
3. Frontend shows `PtpcameradDialog` with copyable Terminal command:
    ```bash
    while true; do pkill -9 ptpcamerad 2>/dev/null; sleep 1; done
    ```
4. User runs command, clicks "Retry connection"

### No Volume trait integration (yet)

MTP operations use dedicated Tauri commands (`listMtpDirectory`, `uploadToMtp`, `downloadFromMtp`, etc.) instead of
unified Volume API. This was intentional (spec "Option B") — unblocks MTP without waiting for Volume refactor. Future:
integrate with Volume trait.

## Gotchas

- **Device list is NOT reactive to USB hotplug in store**: `mtp-store.svelte.ts` updates devices on
  `mtp-device-detected/removed` events, but initial list requires calling `listMtpDevices()` on mount. Hotplug works
  after first fetch.
- **Storage ID is hex string in Tauri, number in mtp-rs**: Backend converts `u32` to hex string for frontend. Parse with
  `parseInt(storageId, 16)` if needed.
- **Directory cache invalidation is coarse**: Any `ObjectAdded` event invalidates entire directory cache for that
  device. Not granular (don't know which directory changed without extra MTP calls).
- **30-second timeout is intentional**: Some Android devices are slow (USB 2.0, old hardware). MTP operations have 30s
  timeout, not the usual 10s.
