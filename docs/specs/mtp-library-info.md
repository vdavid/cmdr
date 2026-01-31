# MTP implementation guide for Cmdr

## Overview

Cmdr needs to support browsing and managing files on Android devices connected via USB using the MTP (Media Transfer
Protocol). This guide covers the Rust implementation using the `mtp-rs` crate, including handling macOS-specific quirks.

## Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
mtp-rs = { path = "../mtp-rs" }  # Or git/crates.io once published
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Basic usage of mtp-rs

### Listing connected MTP devices

```rust
use mtp_rs::{MtpDevice, MtpDeviceBuilder};

// List all connected MTP devices
let devices = MtpDevice::list_devices() ?;
for device_info in & devices {
println!("Found: {:04x}:{:04x}",
         device_info.vendor_id(),
         device_info.product_id());
}
```

### Opening a device and browsing files

```rust
// Open the first available device
let device = MtpDeviceBuilder::new()
.timeout(std::time::Duration::from_secs(30))
.open_first()
.await?;

// Get device info
let info = device.device_info();
println!("Connected to: {} {}", info.manufacturer, info.model);

// List storage areas (e.g., "Internal Storage", "SD Card")
let storages = device.storage_ids().await?;

// Get root folder contents of first storage
let root_handle = mtp_rs::ObjectHandle::ROOT;
let entries = device.get_object_handles(storages[0], None, Some(root_handle)).await?;

for handle in entries {
let info = device.get_object_info(handle).await ?;
println ! ("{} - {} bytes", info.filename, info.object_compressed_size);
}
```

### File Operations

```rust
// Download a file
let data = device.get_object(file_handle).await?;
std::fs::write("local_copy.jpg", & data) ?;

// Upload a file
let data = std::fs::read("photo.jpg") ?;
let new_handle = device.send_object_info_and_data(
storage_id,
parent_folder_handle,
& object_info,
& data,
).await?;

// Delete a file
device.delete_object(file_handle).await?;

// Create a folder
let folder_handle = device.create_folder(storage_id, parent_handle, "New Folder").await?;

// Move/rename (via MTP MoveObject or SetObjectPropValue)
device.move_object(handle, new_parent_handle, storage_id).await?;
```

### Closing the Device

```rust
// Graceful close (sends CloseSession to device)
device.close().await?;

// Or just drop - will attempt graceful close
drop(device);
```

## Error Handling

mtp-rs provides typed errors:

```rust
use mtp_rs::Error;

match device.get_object(handle).await {
Ok(data) => { /* success */ }
Err(Error::Disconnected) => { /* device unplugged */ }
Err(Error::Timeout) => { /* operation timed out, may be retryable */ }
Err(Error::Protocol { code, operation }) => {
/* device returned error code */
}
Err(e) if e.is_retryable() => { /* can retry */ }
Err(e) if e.is_exclusive_access() => {
/* IMPORTANT: another process has the device - see macOS section below */
}
Err(e) => { /* other error */ }
}
```

## macOS-Specific: Handling ptpcamerad Interference

### The Problem

On macOS, a system daemon called `ptpcamerad` automatically claims MTP/PTP devices when connected. This causes
`is_exclusive_access()` errors when Cmdr tries to connect.

### Detection

When opening a device fails, check for exclusive access:

```rust
match MtpDeviceBuilder::new().open_first().await {
Ok(device) => { /* success */ }
Err(e) if e.is_exclusive_access() => {
# [cfg(target_os = "macos")]
{
// Query IORegistry to find out WHO has the device
if let Some((pid, process_name)) = get_usb_exclusive_owner() {
// Show user-friendly message with specific process info
show_exclusive_access_dialog(pid, & process_name);
} else {
// Generic message
show_exclusive_access_dialog_generic();
}
}
# [cfg(not(target_os = "macos"))]
{
// On other platforms, just show generic error
show_error("Device is in use by another application");
}
}
Err(e) => { /* handle other errors */ }
}
```

### Querying IORegistry for Device Owner (macOS only)

The USB device owner info is available in IORegistry under the `UsbExclusiveOwner` property. Here's how to query it:

```rust
#[cfg(target_os = "macos")]
mod macos_usb {
    use std::process::Command;

    /// Query IORegistry for the process holding exclusive access to MTP devices.
    /// Returns (pid, process_name) if found.
    pub fn get_usb_exclusive_owner() -> Option<(u32, String)> {
        // Use ioreg to query USB device ownership
        // Looking for: "UsbExclusiveOwner" = "pid 45145, ptpcamerad"
        let output = Command::new("ioreg")
            .args(["-l", "-w", "0"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if line.contains("UsbExclusiveOwner") && line.contains("ptpcamera") {
                // Parse: "UsbExclusiveOwner" = "pid 45145, ptpcamerad"
                if let Some(value) = line.split('=').nth(1) {
                    let value = value.trim().trim_matches('"');
                    // Parse "pid 45145, ptpcamerad"
                    if value.starts_with("pid ") {
                        let parts: Vec<&str> = value[4..].splitn(2, ", ").collect();
                        if parts.len() == 2 {
                            if let Ok(pid) = parts[0].parse::<u32>() {
                                return Some((pid, parts[1].to_string()));
                            }
                        }
                    }
                }
            }
        }
        None
    }
}
```

**Note:** For a production implementation, consider using IOKit Rust bindings directly instead of shelling out to
`ioreg`. The `io-kit-sys` crate provides the necessary FFI bindings.

### User-Facing Dialog

When exclusive access is detected on macOS, show a dialog like Commander One does:

```
┌─────────────────────────────────────────────────┐
│  Could not connect to MTP device.               │
│                                                 │
│  Most probably it is being in use by            │
│  "pid 45145, ptpcamerad" software.              │
│                                                 │
│  To fix this, run the following command in      │
│  Terminal (keep it running while using Cmdr):   │
│                                                 │
│  ┌─────────────────────────────────────────┐    │
│  │ while true; do pkill -9 ptpcamerad; \   │    │
│  │ sleep 1; done                           │    │
│  └─────────────────────────────────────────┘    │
│                                                 │
│  [Copy Command]              [Learn More]       │
│                                                 │
│  [ ] Don't show again                           │
│                                                 │
│              [OK]        [Retry]                │
└─────────────────────────────────────────────────┘
```

The command to copy:

```bash
while true; do pkill -9 ptpcamerad 2>/dev/null; sleep 1; done
```

### App Store Considerations

If Cmdr is distributed via the App Store (sandboxed):

- You CAN detect exclusive access errors
- You CAN show dialogs and copy text to clipboard
- You CANNOT run `pkill` or `ioreg` directly from the app
- The `ioreg` query might need to use IOKit APIs directly with proper entitlements

If Cmdr is distributed outside the App Store (notarized):

- You CAN do everything above
- You COULD potentially auto-kill ptpcamerad (but recommended to let user do it)

## Architecture Recommendation

```
┌─────────────────────────────────────────────────────────────┐
│  Cmdr UI Layer (Tauri/Swift)                                │
│  - File browser views                                       │
│  - Dialogs (including ptpcamerad help dialog)               │
│  - Clipboard operations                                     │
├─────────────────────────────────────────────────────────────┤
│  Cmdr MTP Service (Rust)                                    │
│  - Wraps mtp-rs with app-specific logic                     │
│  - Handles reconnection attempts                            │
│  - macOS: IORegistry queries for diagnostics                │
│  - Translates errors to user-friendly messages              │
├─────────────────────────────────────────────────────────────┤
│  mtp-rs (Library)                                           │
│  - Platform-independent MTP protocol                        │
│  - Provides Error::is_exclusive_access() for detection      │
│  - No macOS-specific code                                   │
├─────────────────────────────────────────────────────────────┤
│  nusb (USB transport)                                       │
│  - Platform abstraction for USB                             │
└─────────────────────────────────────────────────────────────┘
```

## Key Points Summary

1. **Use `mtp-rs`** for all MTP operations - it's async and platform-independent

2. **Check `is_exclusive_access()`** on connection errors to detect the macOS ptpcamerad issue

3. **Query IORegistry** at the app level (not in mtp-rs) to get the specific PID and process name for user-friendly
   error messages

4. **Provide the Terminal workaround** to users via a copyable command - this is what Commander One and other apps do

5. **Don't auto-kill ptpcamerad** from the app - let users opt-in by running the command themselves (safer, App Store
   compatible)

6. **The workaround is temporary** - users need to run the kill loop only while using Cmdr's MTP features

## Reference: MTP Operations Supported by mtp-rs

| Operation     | Method                                  | Notes                             |
|---------------|-----------------------------------------|-----------------------------------|
| List devices  | `MtpDevice::list_devices()`             | Sync, no device connection needed |
| Open device   | `MtpDeviceBuilder::open_first()`        | Async                             |
| Device info   | `device.device_info()`                  | Cached from open                  |
| List storages | `device.storage_ids()`                  | Async                             |
| Storage info  | `device.storage_info(id)`               | Async                             |
| List objects  | `device.get_object_handles(...)`        | Async                             |
| Object info   | `device.get_object_info(handle)`        | Async                             |
| Download      | `device.get_object(handle)`             | Async, returns Vec<u8>            |
| Upload        | `device.send_object_info_and_data(...)` | Async                             |
| Delete        | `device.delete_object(handle)`          | Async                             |
| Create folder | `device.create_folder(...)`             | Async                             |
| Move object   | `device.move_object(...)`               | Async                             |
| Rename        | `device.set_object_prop_value(...)`     | Async, sets filename prop         |
| Close         | `device.close()`                        | Async, graceful session close     |
