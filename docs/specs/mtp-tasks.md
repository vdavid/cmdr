# MTP implementation tasks

Task breakdown for adding Android device (MTP) support to Cmdr. See [mtp.md](mtp.md) for full spec.

## Phase 1: Foundation

### 1.1 Add mtp-rs dependency
- [x] Add `mtp-rs = { path = "../../../mtp-rs" }` to Cargo.toml
- [x] Verify it compiles on macOS
- [x] Add `mtp-rs` to the "silence unused crate" section in lib.rs (temporary, until used)

### 1.2 Create mtp module structure
- [x] Create `src-tauri/src/mtp/mod.rs` with submodule declarations
- [x] Create `src-tauri/src/mtp/types.rs` with `MtpDeviceInfo`, `MtpStorageInfo` structs
- [x] Add `mod mtp;` to lib.rs (behind `#[cfg(target_os = "macos")]`)

### 1.3 Device discovery
- [x] Create `src-tauri/src/mtp/discovery.rs`
- [x] Implement `list_mtp_devices()` using `MtpDevice::list_devices()`
- [x] Add basic Tauri command `list_mtp_devices` in `src-tauri/src/commands/mtp.rs`
- [x] Register command in lib.rs
- [x] Add TypeScript wrapper in `tauri-commands.ts`

**Checkpoint**: Can list connected Android devices from the frontend.

---

## Phase 2: Device connection

### 2.1 Connection management
- [x] Create `src-tauri/src/mtp/connection.rs`
- [x] Implement `MtpConnectionManager` with device registry (HashMap<device_id, MtpDevice>)
- [x] Implement `connect_device(device_id)` → opens MTP session
- [x] Implement `disconnect_device(device_id)` → closes session gracefully
- [x] Handle `Error::Disconnected` when device is unplugged

### 2.2 Tauri commands for connection
- [x] Add `connect_mtp_device` command
- [x] Add `disconnect_mtp_device` command
- [x] Add `get_mtp_device_info` command (returns storages, device name, etc.)
- [x] Add TypeScript wrappers

### 2.3 macOS ptpcamerad handling
- [x] Create `src-tauri/src/mtp/macos_workaround.rs`
- [x] Implement `get_usb_exclusive_owner()` using ioreg or IOKit
- [x] When `is_exclusive_access()` error, emit `mtp-exclusive-access-error` event
- [x] Create `src/lib/mtp/PtpcameradDialog.svelte` with Terminal command + copy button

**Checkpoint**: Can connect to a device, see its storages, handle ptpcamerad error gracefully.

---

## Phase 3: File browsing

### 3.1 MtpVolume implementation
- [x] ~~Create `src-tauri/src/mtp/mtp_volume.rs`~~ (Option B approach: MTP-specific commands instead)
- [x] ~~Implement `Volume` trait for `MtpVolume`~~ (Deferred to later refactor)
- [x] Implement `list_directory()` — translate path to object handles (in connection.rs)
- [x] ~~Implement `get_metadata()` — get object info~~ (Handled in list_directory)
- [x] ~~Implement `exists()` — check if object handle exists~~ (Not needed for MVP)
- [x] Handle path-to-handle mapping (cache object handles by path)

### 3.2 Directory listing commands
- [x] Add `list_mtp_directory` command (returns `Vec<FileEntry>`)
- [x] Convert MTP object info to `FileEntry` format
- [x] Handle MTP-specific fields (no permissions, different timestamps)
- [x] Add TypeScript wrapper

### 3.3 Frontend integration
- [x] Add `LocationCategory.MobileDevice` to TypeScript types
- [x] Update `VolumeBreadcrumb.svelte` to show "Mobile" section
- [x] Create `src/lib/mtp/mtp-store.svelte.ts` for device state
- [x] Wire up device list in sidebar/breadcrumb

**Checkpoint**: Can browse folders on Android device, see files in file list.

---

## Phase 4: File operations

### 4.1 Download (device → Mac)
- [x] Implement `download_mtp_file(device_id, object_path, local_dest)`
- [x] Add progress callback support (emit events for progress bar)
- [x] Handle large files (streaming via DownloadStream chunks)
- [ ] Add to copy operation flow when source is MTP

### 4.2 Upload (Mac → device)
- [x] Implement `upload_to_mtp(device_id, local_path, dest_folder)`
- [x] Create object info from local file metadata (via NewObjectInfo::file())
- [x] Add progress callback support
- [ ] Add to copy operation flow when destination is MTP

### 4.3 Delete
- [x] Implement `delete_mtp_object(device_id, object_path)`
- [x] Handle folder deletion (recursive deletion of contents first)
- [ ] Add confirmation dialog (same as local delete)

### 4.4 Create folder
- [x] Implement `create_mtp_folder(device_id, parent_path, name)`
- [ ] Wire up to "New folder" action

### 4.5 Rename/Move
- [x] Implement `rename_mtp_object(device_id, path, new_name)`
- [x] Implement `move_mtp_object(device_id, path, new_parent)` using MTP MoveObject
- [ ] Fall back to copy+delete if device doesn't support MoveObject (returns error instead)

**Checkpoint**: Backend CRUD operations implemented. UI integration pending.

---

## Phase 5: Polish

### 5.1 USB hotplug detection
- [x] Add USB device watcher (using nusb's `watch_devices()`)
- [x] Emit `mtp-device-detected` when Android connected
- [x] Emit `mtp-device-removed` when unplugged
- [x] Auto-refresh device list in UI

### 5.2 Multi-storage support
- [x] Show each storage as separate volume ("Pixel 8 - Internal", "Pixel 8 - SD Card")
- [x] Handle storage IDs in paths (MtpVolume with deviceId:storageId format)

### 5.3 Error handling
- [x] Map all `mtp_rs::Error` variants to user-friendly messages
- [x] Handle timeout errors with `is_retryable()` method
- [x] Handle "device busy" with `DeviceBusy` error type

### 5.4 Performance
- [x] Cache folder listings (invalidate on operations)
- [ ] Queue operations to avoid concurrent MTP calls (not implemented - low priority)
- [ ] Consider background prefetch for folder contents (deferred)

### 5.5 Icons
- [x] Add device icon (phone SVG) to volume list
- [x] Use generic icons for MTP files (no macOS icon extraction possible)

**Checkpoint**: USB hotplug works, multi-storage volumes shown, errors are user-friendly, folder listings cached.

---

## Phase 6: Testing

### 6.1 Unit tests (Rust)
- [x] Test `MtpConnectionManager` helper functions (normalize_mtp_path, get_mtp_icon_id)
- [x] Test path-to-handle parsing (parse_device_id edge cases)
- [x] Test error handling (MtpConnectionError variants, is_retryable, user_message)
- [x] Test directory listing cache behavior
- [x] Test error message formatting and serialization
- [x] Test transfer types and operation result serialization

### 6.2 Unit tests (TypeScript/Svelte)
- [x] Test mtp-store.svelte.ts reactive behavior
- [x] Test device state management (getDevices, connect, disconnect)
- [x] Test MTP volume generation (getMtpVolumes)
- [x] Test event handling (connected, disconnected, exclusive access, hotplug)
- [x] Mock Tauri commands appropriately

### 6.3 Integration test documentation
See [Manual testing procedure](#manual-testing-procedure) below.

### 6.4 E2E test considerations
- [x] MTP commands gracefully return empty arrays when no device is connected
- [x] Existing E2E tests continue to pass with MTP code present
- [ ] Add E2E test mocks when UI integration is complete (Phase 4 pending items)

---

## Manual testing procedure

This section documents how to manually test MTP functionality with a real Android device.

### Prerequisites

1. An Android device with USB debugging enabled or in "File transfer" mode
2. A USB cable to connect the device to your Mac
3. Cmdr running in dev mode (`pnpm dev`)

### Test scenarios

#### 1. Device discovery
1. Connect Android device via USB
2. Set device to "File transfer / Android Auto" mode
3. In Cmdr, check the volume picker - device should appear under "Mobile" section
4. Device name should show (for example, "Pixel 8" or "Samsung Galaxy")

#### 2. Connection
1. Click on the device in the volume picker
2. **If ptpcamerad blocks**: a dialog should appear with Terminal command
   - Copy the command and run in Terminal
   - Click "Retry connection"
3. After successful connection, storages should appear (for example, "Internal shared storage")

#### 3. Browse files
1. Navigate into a storage
2. Browse folders (DCIM, Download, etc.)
3. Verify file names, sizes, and dates display correctly
4. Navigate up with ".." entry

#### 4. File operations (when UI integration complete)
1. **Download**: Select a file, copy to local folder
2. **Upload**: Copy a local file to device folder
3. **Delete**: Delete a file on device
4. **Create folder**: Create new folder on device
5. **Rename**: Rename a file on device

#### 5. USB hotplug
1. Disconnect device while browsing
2. Verify graceful error handling (no crash)
3. Reconnect device
4. Verify device reappears in volume picker

#### 6. Multi-storage devices
1. If device has SD card, verify both storages appear
2. Verify each storage is independently navigable

### Tested devices

| Device | Android version | Notes |
|--------|-----------------|-------|
| (Add tested devices here) | | |

### Known issues

- ptpcamerad on macOS requires workaround command
- Some devices may be slow to respond (30s timeout)
- MTP protocol only allows one operation at a time

---

## Future enhancements (not in initial scope)

- [ ] Thumbnail generation for images on device
- [ ] Drag and drop from device to local
- [ ] Quick Look preview for device files
- [ ] Remember last-used device and auto-connect
- [ ] Device battery level display
- [ ] Wireless MTP (if devices support it)

---

## Dependencies between phases

```
Phase 1 (Foundation)
    ↓
Phase 2 (Connection)
    ↓
Phase 3 (Browsing) ←──────────────┐
    ↓                              │
Phase 4 (Operations)               │ Can parallelize UI work
    ↓                              │ with backend work
Phase 5 (Polish) ─────────────────┘
    ↓
Phase 6 (Testing)
```

Phases 1–3 are the MVP. Phase 4 makes it useful. Phase 5 makes it polished.
