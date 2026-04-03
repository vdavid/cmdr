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

### Linux USB permission handling

On Linux, USB device files need udev rules to grant user access. When `open_device()` fails with EACCES:

1. Backend detects "permission denied" in the USB error string (`#[cfg(target_os = "linux")]`)
2. Emits `mtp-permission-error` event
3. Frontend shows `MtpPermissionDialog` with a copyable command to install udev rules and reload them
4. User runs command, replugs device, clicks "Retry connection"

The udev rules file is at `src-tauri/resources/99-cmdr-mtp.rules` (for deb/rpm packaging).

### Volume trait integration

MTP volumes implement the `Volume` trait. Browsing, `create_directory`, `delete`, `rename`, copy (F5), move (F6), and
delete (F8) all route through the Volume abstraction. Clipboard operations (Cmd+C/V/X) remain blocked because the system
clipboard requires local file paths, which MTP virtual paths can't provide — the UI suggests F5/F6 instead.

## Gotchas

- **Backend auto-connects**: The backend watcher auto-connects MTP devices on USB hotplug. The frontend MTP store is a
  passive consumer that tracks connection state via `mtp-device-connected`/`mtp-device-disconnected` events. It never
  orchestrates connections.
- **Storage ID is hex string in Tauri, number in mtp-rs**: Backend converts `u32` to hex string for frontend. Parse with
  `parseInt(storageId, 16)` if needed.
- **Directory cache invalidation is coarse**: Any `ObjectAdded` event invalidates entire directory cache for that
  device. Not granular (don't know which directory changed without extra MTP calls).
- **30-second timeout is intentional**: Some Android devices are slow (USB 2.0, old hardware). MTP operations have 30s
  timeout, not the usual 10s.
- **`resetForTesting()` must stay in sync with module state**: When adding new module-level state to
  `mtp-store.svelte.ts`, update `resetForTesting()` to clear it. Tests use this instead of `vi.resetModules()` to avoid
  ~8s module re-parse penalty per test.
