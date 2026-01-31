# MTP (Android device) support

Browse and manage files on Android devices connected via USB.

## Overview

Android phones in "File transfer / Android Auto" mode use MTP (Media Transfer Protocol). Unlike USB mass storage,
MTP doesn't mount as a filesystem — it requires a specialized client. macOS has no native MTP support, so we need to
implement it ourselves.

We'll use [mtp-rs](https://github.com/vdavid/mtp-rs), a pure Rust MTP implementation (local path: `../mtp-rs`).

## Goals

1. Detect connected MTP devices and show them in the volume picker
2. Browse files and folders on the device
3. Full file operations: copy, move, delete, rename, create folder
4. Handle macOS `ptpcamerad` interference gracefully
5. Support multiple devices simultaneously

## Non-goals (for now)

- Thumbnail/preview generation for device files
- Syncing or backup features
- iOS device support (uses a different protocol)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Frontend (Svelte)                                                  │
│  - VolumeBreadcrumb shows MTP devices with new "Mobile" category    │
│  - File list renders MTP entries like local files                   │
│  - ptpcamerad help dialog when exclusive access error               │
├─────────────────────────────────────────────────────────────────────┤
│  Tauri Commands                                                     │
│  - list_mtp_devices() → Vec<MtpDeviceInfo>                          │
│  - connect_mtp_device(device_id) → Result<()>                       │
│  - disconnect_mtp_device(device_id) → Result<()>                    │
├─────────────────────────────────────────────────────────────────────┤
│  MTP Service (src-tauri/src/mtp/)                                   │
│  - Device discovery and connection management                       │
│  - MtpVolume implementing Volume trait                              │
│  - macOS-specific ptpcamerad detection                              │
├─────────────────────────────────────────────────────────────────────┤
│  mtp-rs (../mtp-rs)                                                 │
│  - Pure Rust MTP protocol implementation                            │
│  - Async API, uses nusb for USB transport                           │
└─────────────────────────────────────────────────────────────────────┘
```

## Key design decisions

### 1. New location category: `MobileDevice`

Add to `LocationCategory` enum:

```rust
pub enum LocationCategory {
    Favorite,
    MainVolume,
    AttachedVolume,
    CloudDrive,
    Network,
    MobileDevice,  // NEW
}
```

MTP devices appear in their own section in the volume picker, separate from attached volumes (USB drives). This makes
it clear these are different — MTP has different capabilities and performance characteristics.

### 2. MtpVolume implements Volume trait

Create `src-tauri/src/mtp/mtp_volume.rs`:

```rust
pub struct MtpVolume {
    device: Arc<Mutex<MtpDevice>>,
    storage_id: u32,
    name: String,
}

impl Volume for MtpVolume {
    fn name(&self) -> &str { &self.name }
    fn root(&self) -> &Path { Path::new("/") }  // Virtual root

    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        // Translate path to MTP object handle, call device.get_object_handles()
    }

    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        // Call device.send_object_info_and_data()
    }

    // ... etc
}
```

### 3. Handle ptpcamerad on macOS

On macOS, the system daemon `ptpcamerad` automatically claims MTP devices. When we get an exclusive access error:

1. Query IORegistry to find the blocking process
2. Show a dialog explaining the issue
3. Provide a copyable Terminal command to work around it:
   ```bash
   while true; do pkill -9 ptpcamerad 2>/dev/null; sleep 1; done
   ```

See `docs/specs/mtp-library-info.md` for implementation details.

### 4. Device lifecycle

```
USB plugged in → list_devices() detects it → user clicks in UI → connect_mtp_device()
                                                                         ↓
                                              ← MtpVolume registered ← open device
                                                                         ↓
                                                           emit "mtp-device-connected"
                                                                         ↓
USB unplugged → device.get_*() returns Disconnected → emit "mtp-device-disconnected"
                                                         ↓
                                                  MtpVolume unregistered
```

### 5. File operations routing

Currently, all operations go through `operations.rs` with hardcoded "root" volume. With Phase 4 Volume integration:

```rust
// Current (hardcoded):
ops_list_directory_start_with_volume("root", &path, ...)

// Future (dynamic):
let volume = volume_manager.get(volume_id)?;
volume.list_directory(&path)
```

For MTP, we can either:
- **Option A**: Integrate with Phase 4 Volume refactor (cleaner, but depends on Phase 4)
- **Option B**: Add parallel MTP-specific commands now (faster to ship, more code to maintain)

Recommendation: **Option B first, refactor to A later**. This unblocks MTP without waiting for Phase 4.

## File structure

```
src-tauri/src/
├── mtp/
│   ├── mod.rs              # Module exports, MTP service initialization
│   ├── discovery.rs        # Device detection, USB hotplug
│   ├── connection.rs       # Device connection/disconnection, session management
│   ├── mtp_volume.rs       # Volume trait implementation
│   ├── macos_workaround.rs # ptpcamerad detection and dialog
│   └── types.rs            # MtpDeviceInfo, MtpStorageInfo, etc.
├── commands/
│   └── mtp.rs              # Tauri commands for MTP operations
└── lib.rs                  # Add mtp module, init in setup()
```

Frontend:

```
src/lib/
├── file-explorer/
│   └── VolumeBreadcrumb.svelte  # Add "Mobile" category section
├── mtp/
│   ├── mtp-store.ts             # Reactive state for connected devices
│   ├── mtp-commands.ts          # Tauri command wrappers
│   └── PtpcameradDialog.svelte  # Help dialog for macOS workaround
└── tauri-commands.ts            # Add MTP command types
```

## Events

| Event | Payload | When |
|-------|---------|------|
| `mtp-device-detected` | `{ deviceId, name, vendorId, productId }` | USB device with MTP detected |
| `mtp-device-removed` | `{ deviceId }` | USB device unplugged |
| `mtp-device-connected` | `{ deviceId, storages: [...] }` | Successfully opened MTP session |
| `mtp-device-disconnected` | `{ deviceId, reason }` | Session closed (user or error) |
| `mtp-exclusive-access-error` | `{ deviceId, blockingProcess? }` | Another process has the device |

## Performance considerations

- **Listing is slow**: MTP lists one folder at a time, no recursive listing. Consider caching folder contents.
- **Transfers are serial**: MTP can only do one transfer at a time per device. Queue operations.
- **No random access**: Can't seek in files. For previews, download entire file.
- **Timeouts**: Set generous timeouts (30s+) — some devices are slow.

## Testing strategy

1. **Unit tests**: Mock `MtpDevice` to test `MtpVolume` logic
2. **Integration tests**: Use real device (manual, not CI)
3. **E2E tests**: Mock at Tauri command level, test UI flows

## Open questions

1. **Multi-storage**: Android devices often have multiple storages (Internal, SD Card). Show as:
   - One volume per storage? (e.g., "Pixel 8 - Internal", "Pixel 8 - SD Card")
   - Or one volume with storage picker inside?

   Recommendation: One volume per storage — simpler, matches how Finder shows disk partitions.

2. **Reconnection**: If device disconnects mid-operation, should we auto-reconnect?

   Recommendation: No auto-reconnect. Show error, let user manually reconnect. Simpler and more predictable.

3. **Progress reporting**: MTP transfers can be large. How to show progress?

   Use existing progress infrastructure from copy/move operations. mtp-rs supports progress callbacks.

## Dependencies

Add to `apps/desktop/src-tauri/Cargo.toml`:

```toml
[dependencies]
mtp-rs = { path = "../../../mtp-rs" }  # Or git URL once published
```

No other new dependencies needed — mtp-rs bundles nusb internally.

## References

- [mtp-rs repository](https://github.com/vdavid/mtp-rs)
- [MTP library integration guide](mtp-library-info.md) — API examples and macOS workaround details
- [USB.org MTP spec](https://www.usb.org/document-library/media-transfer-protocol-v11-spec-and-mtp-media-format-specs-702)
