# Bugfix: MTP volumes missing from copy/move dialog

## Context

When copying/moving files TO an MTP device, the transfer dialog's volume selector dropdown is empty -- MTP volumes don't
appear. This causes the path to show as a raw `mtp://` URI, path validation fails ("must start with /"), and after a
successful transfer the user gets thrown to the main volume's root.

Root cause: `list_volumes()` only returns filesystem volumes. MTP volumes live in a separate frontend store
(`getMtpVolumes()`) that's never passed to the transfer dialog.

Goal: Unify MTP volumes into the main volume list so the dialog is transport-agnostic. This establishes the pattern for
future S3/FTP/etc. backends.

### Boundary: what stays separate

The backend owns the volume _list_ (what's browsable). The frontend MTP store (`mtp-store.svelte.ts`) still owns the
connection _lifecycle_ (initialize, scanDevices, connect, disconnect, error handling). The store's `getMtpVolumes()`
function stays for any code that genuinely needs connection state, but volume display/selection code should use the
unified `VolumeInfo[]` from `list_volumes()`.

## Plan

### Step 1: Add `MobileDevice` category and `is_read_only` to backend `LocationInfo`

All three platform files that define `LocationCategory` and `LocationInfo`/`VolumeInfo`:

- Add `MobileDevice` variant to `LocationCategory` enum (auto-serializes as `mobile_device` via `rename_all`)
- Add `pub is_read_only: bool` field to `LocationInfo` / `VolumeInfo`
- Set `is_read_only: false` at all existing construction sites

The stubs file (`stubs/volumes.rs`) is also missing `fs_type` and `supports_trash` fields that exist on macOS/Linux. Fix
this pre-existing drift while we're here — add all three fields (`fs_type`, `supports_trash`, `is_read_only`) to the
stub `VolumeInfo` struct and set them at all construction sites.

The frontend types already have `isReadOnly?: boolean` and `'mobile_device'` in `LocationCategory`
(`src/lib/file-explorer/types.ts:134,153`). No frontend type changes needed.

Files:

- `src-tauri/src/volumes/mod.rs` (macOS)
- `src-tauri/src/volumes_linux/mod.rs` (Linux)
- `src-tauri/src/stubs/volumes.rs` (other platforms — fix full struct drift while adding `is_read_only`)

### Step 2: `list_volumes()` includes connected MTP devices

Both `commands/volumes.rs` (macOS) and `commands/volumes_linux.rs` (Linux).

**Threading approach**: The MTP query (`get_all_connected_devices()`) is async but the macOS `list_volumes` uses
`blocking_with_timeout_flag` (a sync closure). Solution: get filesystem volumes from the blocking closure first, then
append MTP volumes via an async call after:

```rust
// macOS
pub async fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    let mut result = blocking_with_timeout_flag(VOLUME_TIMEOUT, vec![], volumes::list_mounted_volumes).await;
    append_mtp_volumes(&mut result.data).await;
    result
}
```

Linux `list_volumes` is currently sync — make it `async` (Tauri commands support this) and append after:

```rust
// Linux
pub async fn list_volumes() -> TimedOut<Vec<VolumeInfo>> {
    let mut data = volumes_linux::list_mounted_volumes();
    append_mtp_volumes(&mut data).await;
    TimedOut { data, timed_out: false }
}
```

Extract a shared helper (or duplicate in each file since they import different `VolumeInfo` types):

```rust
async fn append_mtp_volumes(volumes: &mut Vec<VolumeInfo>) {
    let devices = crate::mtp::connection_manager().get_all_connected_devices().await;
    for device in devices {
        let multi = device.storages.len() > 1;
        let device_name = device.device.product.as_deref()
            .or(device.device.manufacturer.as_deref())
            .unwrap_or("Mobile device");
        for storage in &device.storages {
            let name = if multi {
                format!("{} - {}", device_name, storage.name)
            } else {
                device_name.to_string()
            };
            volumes.push(VolumeInfo {
                id: format!("{}:{}", device.device.id, storage.id),
                name,
                path: format!("mtp://{}/{}", device.device.id, storage.id),
                category: LocationCategory::MobileDevice,
                icon: None,
                is_ejectable: true,
                is_read_only: storage.is_read_only,
                fs_type: Some("mtp".to_string()),
                supports_trash: false,
            });
        }
    }
}
```

The MTP query reads an in-memory `HashMap` — sub-millisecond, no timeout needed. The `mtp-device-connected` event is
emitted AFTER volumes are registered in the connection manager (`connection/mod.rs:329-348`), so no race condition.

**Cache note**: The macOS `LOCATIONS_CACHE` (5-second TTL) only caches the `list_locations()` result. MTP volumes are
appended after the cache lookup, so they're always fresh.

**Stubs**: `stubs/volumes.rs` does not need MTP logic (MTP is not available on stub platforms). Its `list_volumes` stays
as-is — it just needs the struct field changes from Step 1.

Also apply the same MTP append to `find_containing_volume` in both files. Currently it calls `list_locations()`
(filesystem-only), so `findContainingVolume("mtp://dev/65537/DCIM")` returns null — the frontend falls back to the root
volume ID, which is wrong. Same pattern: get filesystem results, then append MTP volumes, then do the longest-prefix
match. This fixes a pre-existing bug where tab restore and breadcrumb display break for MTP paths.

Files:

- `src-tauri/src/commands/volumes.rs`
- `src-tauri/src/commands/volumes_linux.rs`

### Step 3: Make `dest_path` consistently volume-relative in `copy_between_volumes`

The local-to-local optimization in `volume_copy.rs:92` does `dest_root.join(&dest_path)`. `PathBuf::join` with an
absolute path (starting with `/`) **replaces** the base, so currently this only works because `dest_path` is a full
absolute path. With volume-relative paths, `/Documents` would replace instead of appending.

Fix: use `resolve()` from `LocalPosixVolume` (which already handles this correctly for `scan_for_conflicts` and all
other Volume trait methods), or simply strip the leading `/` before joining:

```rust
// volume_copy.rs, local-to-local optimization
let absolute_dest = dest_root.join(dest_path.strip_prefix("/").unwrap_or(&dest_path));
```

This makes `dest_path` consistently volume-relative throughout the pipeline. `LocalPosixVolume::resolve` (line 67-71)
already does the same strip-and-join for non-root volumes, so this aligns the optimization path with the trait path.

Source paths are NOT changed — they're full absolute paths from the file listing and `src_root.join` with an absolute
path replaces the base (correct behavior, acts as a no-op).

File: `src-tauri/src/file_system/write_operations/volume_copy.rs` (line 92)

### Step 4: Fix `destVolumeId` being ignored after user changes volume

In `dialog-state.svelte.ts`, `handleTransferConfirm` ignores the `volumeId` parameter (named `_volumeId`) and always
uses `transferDialogProps.destVolumeId`. If the user changes the volume in the dropdown, the wrong volume ID is used.

- Rename `_volumeId` to `volumeId`
- Use it for `destVolumeId` in `transferProgressProps`

File: `src/lib/file-explorer/pane/dialog-state.svelte.ts` (line 257, 276)

### Step 5: TransferDialog works with volume-relative paths end-to-end

The dialog displays and passes paths relative to the selected volume. No reconstruction needed — the backend now handles
volume-relative `dest_path` correctly (Step 3), and all Volume trait methods (`list_directory`, `scan_for_conflicts`,
etc.) already resolve paths through `LocalPosixVolume::resolve` or `MtpVolume::to_mtp_path`.

In `TransferDialog.svelte`:

**On init**: Derive volume-relative path from `destinationPath` by stripping the selected volume's `path` prefix.
Extract a pure helper function `toVolumeRelativePath(fullPath, volumePath)`:

```typescript
/** Strips the volume prefix to get a volume-relative path. Always returns a `/`-prefixed string. */
function toVolumeRelativePath(fullPath: string, volumePath: string): string {
  if (volumePath === '/') return fullPath
  if (fullPath.startsWith(volumePath)) {
    return fullPath.slice(volumePath.length) || '/'
  }
  return '/'
}
```

For root volume (`/`): full path = relative path (identity). For MTP (`mtp://dev/65537`): strips to `/DCIM`. For USB
(`/Volumes/USB`): strips to `/Documents`.

**On volume change** (`handleVolumeChange`): Reset `editedPath` to `/` (the new volume's root). The current path is
meaningless on a different volume.

**On confirm**: Pass `editedPath` directly to `onConfirm` — it's already volume-relative, and the whole pipeline handles
it: `copy_between_volumes` uses `(destVolumeId, destPath)` where `destPath` is volume-relative.

**`validateDirectoryPath`**: No changes needed -- volume-relative paths always start with `/`.

**Known limitation**: If the user manually types a nonsensical path like `/Users/david/Documents` while an MTP volume is
selected, the backend MTP volume will fail to resolve it and return an error. The error handling is graceful
(TransferErrorDialog shows the error). Acceptable edge case for v1.

File: `src/lib/file-operations/transfer/TransferDialog.svelte`

### Step 6: Re-fetch volumes on MTP connect/disconnect

In `DualPaneExplorer.svelte`:

- Listen for `mtp-device-connected` and `mtp-device-disconnected` Tauri events (using the existing
  `onMtpDeviceConnected` / `onMtpDeviceDisconnected` wrappers from `$lib/tauri-commands`)
- On either event, re-fetch `volumes = (await listVolumes()).data`
- Clean up listeners in `onDestroy`
- This keeps the `volumes` state fresh as devices are plugged/unplugged

File: `src/lib/file-explorer/pane/DualPaneExplorer.svelte`

### Step 7: Remove `getMtpVolumes()` from volume info lookups

Now that MTP is in the unified `volumes` list:

**`transfer-operations.ts`**: Simplify `getDestinationVolumeInfo(volumeId, volumes, mtpVolumes)` to
`getDestinationVolumeInfo(volumeId, volumes)`. Remove the `MtpVolumeInfo` interface. Just do
`volumes.find(v => v.id === volumeId)`.

**`DualPaneExplorer.svelte`**: Remove `import { getMtpVolumes }`. Update callers:

- Line 1305 (`startRename`): `getDestinationVolumeInfo(volId, volumes)` (drop 3rd arg)
- Line 1464 (`openTransferDialog`): same
- Line 2076 (`selectVolumeByName`): replace `getMtpVolumes().find(v => v.name === name)` with
  `volumes.find(v => v.category === 'mobile_device' && v.name === name)`

**`volume-grouping.ts`**: Remove `mtpVols` parameter from `groupByCategory()`. The `mobile_device` case just filters
from the unified list: `vols.filter(v => v.category === 'mobile_device')`.

**`VolumeBreadcrumb.svelte`**: Remove `getMtpVolumes()` dependency and the `$derived(getMtpVolumes())` line. Use unified
`volumes` for current volume lookup (use `category === 'mobile_device'` instead of `volumeId.startsWith('mtp-')`).
Update `groupByCategory(volumes)` call (drop mtpVolumes arg). Keep `initialize()` and `scanDevices()` calls -- those are
for MTP connection lifecycle, not volume listing.

Files:

- `src/lib/file-explorer/pane/transfer-operations.ts`
- `src/lib/file-explorer/pane/DualPaneExplorer.svelte`
- `src/lib/file-explorer/navigation/volume-grouping.ts`
- `src/lib/file-explorer/navigation/VolumeBreadcrumb.svelte`

### Step 8: Update tests

- `transfer-operations.test.ts`: Update `getDestinationVolumeInfo` tests for new signature (remove mtpVolumes param)
- `volume-grouping` tests (if any): Update for new `groupByCategory` signature
- Add unit tests for `toVolumeRelativePath` covering: root volume, MTP volume root, MTP subdirectory, USB volume, volume
  root path, empty-result fallback
- `mtp-store.test.ts`: `getMtpVolumes` tests stay (the function still exists for connection management)
- Existing E2E tests should still pass

## Verification

1. `cargo build` / `cargo check` -- backend compiles (check macOS, Linux, and stub targets)
2. `pnpm vitest` -- unit tests pass (especially transfer-operations, volume-grouping, path helpers)
3. Manual test with real or virtual MTP device:
   - Connect device, verify it appears in transfer dialog volume dropdown
   - Copy file from Desktop to MTP device -- dialog shows `/` as path, copy succeeds
   - Copy to MTP subfolder -- dialog shows `/DCIM`, copy goes to correct location
   - After copy, pane stays at the MTP location (not thrown to root)
   - Change volume in dropdown mid-dialog -- path resets to `/`, correct volume ID used
   - Switch from MTP to local volume and back — paths reset correctly each time
4. Verify local-to-local copy/move still works (regression)
5. Volume breadcrumb still shows MTP volumes correctly
6. Disconnect MTP device while transfer dialog is open — verify graceful error on confirm

## Why this approach is elegant

- **Backend owns the truth**: `list_volumes()` returns ALL browsable volumes. Frontend never assembles its own list.
- **Dialog is transport-agnostic**: It renders `VolumeInfo[]` and works with `/`-prefixed relative paths. No `mtp://`
  leaks into the UI. Adding S3 later = register a volume on the backend, done.
- **No path format hacks**: `validateDirectoryPath` stays unchanged. Volume-relative paths always start with `/`.
- **Volume-relative paths flow end-to-end**: Dialog shows `/DCIM`, passes `/DCIM` to the backend. No strip → reconstruct
  → strip round-trip. The one-line fix in `volume_copy.rs` aligns the local-to-local optimization with what
  `LocalPosixVolume::resolve` and `MtpVolume::to_mtp_path` already do.
- **No threading hacks**: MTP volumes are appended after the blocking filesystem query, not inside it.
