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
- [ ] Create `src-tauri/src/mtp/mtp_volume.rs`
- [ ] Implement `Volume` trait for `MtpVolume`
- [ ] Implement `list_directory()` — translate path to object handles
- [ ] Implement `get_metadata()` — get object info
- [ ] Implement `exists()` — check if object handle exists
- [ ] Handle path-to-handle mapping (cache object handles by path)

### 3.2 Directory listing commands
- [ ] Add `list_mtp_directory` command (returns `Vec<FileEntry>`)
- [ ] Convert MTP object info to `FileEntry` format
- [ ] Handle MTP-specific fields (no permissions, different timestamps)
- [ ] Add TypeScript wrapper

### 3.3 Frontend integration
- [ ] Add `LocationCategory.MobileDevice` to TypeScript types
- [ ] Update `VolumeBreadcrumb.svelte` to show "Mobile" section
- [ ] Create `src/lib/mtp/mtp-store.ts` for device state
- [ ] Wire up device list in sidebar/breadcrumb

**Checkpoint**: Can browse folders on Android device, see files in file list.

---

## Phase 4: File operations

### 4.1 Download (device → Mac)
- [ ] Implement `download_mtp_file(device_id, object_path, local_dest)`
- [ ] Add progress callback support (emit events for progress bar)
- [ ] Handle large files (streaming, not loading entire file in memory)
- [ ] Add to copy operation flow when source is MTP

### 4.2 Upload (Mac → device)
- [ ] Implement `upload_to_mtp(device_id, local_path, dest_folder)`
- [ ] Create object info from local file metadata
- [ ] Add progress callback support
- [ ] Add to copy operation flow when destination is MTP

### 4.3 Delete
- [ ] Implement `delete_mtp_object(device_id, object_path)`
- [ ] Handle folder deletion (MTP requires empty folder)
- [ ] Add confirmation dialog (same as local delete)

### 4.4 Create folder
- [ ] Implement `create_mtp_folder(device_id, parent_path, name)`
- [ ] Wire up to "New folder" action

### 4.5 Rename/Move
- [ ] Implement `rename_mtp_object(device_id, path, new_name)`
- [ ] Implement `move_mtp_object(device_id, path, new_parent)` if MTP supports it
- [ ] Fall back to copy+delete if device doesn't support MoveObject

**Checkpoint**: Full CRUD operations on MTP device.

---

## Phase 5: Polish

### 5.1 USB hotplug detection
- [ ] Add USB device watcher (using nusb or IOKit)
- [ ] Emit `mtp-device-detected` when Android connected
- [ ] Emit `mtp-device-removed` when unplugged
- [ ] Auto-refresh device list in UI

### 5.2 Multi-storage support
- [ ] Show each storage as separate volume ("Pixel 8 - Internal", "Pixel 8 - SD Card")
- [ ] Handle storage IDs in paths (prefix or separate volume)

### 5.3 Error handling
- [ ] Map all `mtp_rs::Error` variants to user-friendly messages
- [ ] Handle timeout errors with retry option
- [ ] Handle "device busy" gracefully

### 5.4 Performance
- [ ] Cache folder listings (invalidate on operations)
- [ ] Queue operations to avoid concurrent MTP calls
- [ ] Consider background prefetch for folder contents

### 5.5 Icons
- [ ] Add device icon (phone) to volume list
- [ ] Use generic icons for MTP files (no macOS icon extraction possible)

---

## Phase 6: Testing

### 6.1 Unit tests
- [ ] Test `MtpVolume` with mocked device
- [ ] Test path-to-handle mapping
- [ ] Test error handling (disconnected, timeout, etc.)

### 6.2 Integration tests
- [ ] Document manual test procedure with real device
- [ ] Test with multiple device models (Samsung, Pixel, etc.)
- [ ] Test with devices that have multiple storages

### 6.3 E2E tests
- [ ] Mock MTP commands at Tauri level
- [ ] Test UI flows (connect, browse, copy file, disconnect)

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
