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

There's no frontend device store. The backend watcher auto-connects on USB hotplug and lists only connected devices'
storages as volumes (`{device}:{storage}` ids), so the volume list is the one source of which storages exist, and a
device-only volume id never reaches a pane. The events each have one consumer:

- `onMtpDeviceConnected`: the root layout's sticky connect toast.
- `onMtpDeviceDisconnected`: `pane/mtp-disconnect-watch.svelte.ts`, which moves a pane off a storage that left.
- `onMtpExclusiveAccessError` / `onMtpPermissionError`: the root layout, which opens the matching manual-fix dialog
  (ptpcamerad on macOS, udev rules on Linux) and retries the connect after it.

## ptpcamerad (macOS)

On macOS the `ptpcamerad` daemon auto-claims MTP/PTP devices. The backend handles this; when it can't get exclusive
access it emits an exclusive-access error carrying the blocking process. `PtpcameradDialog.svelte` is the manual
fallback: it shows a Terminal command the user can run to free the device.

## udev rules (Linux)

USB device files need udev rules for user access. On an `EACCES` open failure the backend emits a permission error, and
`MtpPermissionDialog.svelte` shows a copyable command to install the rules and reload them; the user replugs and
retries. The rules file ships at `src-tauri/resources/99-cmdr-mtp.rules` for deb/rpm packaging.

## Timeout

MTP operations use a 30s timeout (backend `crates/cmdr-mtp/src/connection/mod.rs`, `MTP_TIMEOUT_SECS`), longer than the
usual 10s, because some Android devices are slow (USB 2.0, old hardware).

## Volume trait integration

MTP volumes implement the `Volume` trait, so browsing, `create_directory`, `delete`, `rename`, copy (F5), and move (F6)
all route through the volume abstraction. System-clipboard operations (Cmd+C/X/V) stay blocked because the macOS
pasteboard needs local `public.file-url` paths, which MTP virtual paths can't provide.

## Cache invalidation

Directory cache invalidation is coarse: any object-change signal invalidates the whole directory cache for that device,
because pinpointing the changed directory would need extra MTP round-trips.

## i18n

MTP copy lives in the `mtp.*` catalog (`$lib/intl/messages/en/mtp.json`), resolved via `tString()` / `t()` / `<Trans>`;
`cmdr/no-raw-user-facing-string` is enforced on `lib/mtp/`. The two manual-fix dialogs use `<Trans>` for sentences with
inline components: `PtpcameradDialog` wraps the blocking-process name (`<process>`), the `ptpcamerad` token (`<code>`),
and a `<ShortcutChip key="Ctrl+C">` (`<key>`); the chip/`<code>` snippets are declared at markup top level (NOT inside
`<ModalDialog>`, or Svelte treats them as the dialog's named props). Runtime rules:
[`$lib/intl/CLAUDE.md`](../intl/CLAUDE.md).
