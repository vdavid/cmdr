# MTP frontend details

Depth and rationale. `CLAUDE.md` holds the must-knows; this is everything else.

## Path format

`mtp://{deviceId}/{storageId}/{path}`, all slashes:

- `deviceId`: backend device identifier (for example `0-5`)
- `storageId`: MTP storage ID as a decimal number (for example `65537`)
- `path`: virtual path within the storage, empty for the storage root

Examples: `mtp://0-5/65537` (storage root), `mtp://0-5/65537/DCIM/Camera` (subfolder). A device with multiple storages
(Internal + SD card) surfaces as separate volumes, each with a distinct volume ID. See `mtp-path-utils.ts` for
parse/construct.

## Storage ID across the IPC boundary

The backend (`mtp-rs`) holds storage IDs as `u32`; Tauri may surface them as a hex string. Convert with
`parseInt(storageId, 16)` if you receive a hex form. Internally the frontend stores them as numbers.

## Event-driven state

`mtp-store.svelte.ts::initialize()` registers four listeners (and stores their unlisten handles):

- `onMtpDeviceConnected`: creates/updates the device entry, marks it `connected`, records its storages.
- `onMtpDeviceDisconnected`: marks the device `disconnected`, clears storages.
- `onMtpExclusiveAccessError`: marks `error`, records the blocking process if known (the ptpcamerad case on macOS).
- `onMtpPermissionError`: marks `error` with a "USB permission denied, install udev rules" message (the Linux case).

The store never initiates a connection; the backend watcher auto-connects on USB hotplug and emits these events.

## ptpcamerad (macOS)

On macOS the `ptpcamerad` daemon auto-claims MTP/PTP devices. The backend handles this; when it can't get exclusive
access it emits an exclusive-access error carrying the blocking process. `PtpcameradDialog.svelte` is the manual
fallback: it shows a Terminal command the user can run to free the device.

## udev rules (Linux)

USB device files need udev rules for user access. On an `EACCES` open failure the backend emits a permission error, and
`MtpPermissionDialog.svelte` shows a copyable command to install the rules and reload them; the user replugs and
retries. The rules file ships at `src-tauri/resources/99-cmdr-mtp.rules` for deb/rpm packaging.

## Timeout

MTP operations use a 30s timeout (backend `mtp/connection/mod.rs`, `MTP_TIMEOUT_SECS`), longer than the usual 10s,
because some Android devices are slow (USB 2.0, old hardware).

## Volume trait integration

MTP volumes implement the `Volume` trait, so browsing, `create_directory`, `delete`, `rename`, copy (F5), and move (F6)
all route through the volume abstraction. System-clipboard operations (Cmd+C/X/V) stay blocked because the macOS
pasteboard needs local `public.file-url` paths, which MTP virtual paths can't provide.

## Cache invalidation

Directory cache invalidation is coarse: any object-change signal invalidates the whole directory cache for that device,
because pinpointing the changed directory would need extra MTP round-trips.
