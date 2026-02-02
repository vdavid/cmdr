# Volume trait streaming redesign for MTP-to-MTP copy

## Executive summary

**Scope:** Add streaming copy support to the Volume trait to enable MTP-to-MTP copy without temp files.

**Effort estimate:** ~4-6 hours (smaller than expected!)

**Key insight:** The change is mostly **additive**. We don't need to change the existing `export_to_local`/`import_from_local` methods. We add new streaming methods alongside them.

---

## Current architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  Volume trait (mod.rs)                                              │
│  ├── export_to_local(source, local_dest) → Result<u64>              │
│  └── import_from_local(local_source, dest) → Result<u64>            │
└─────────────────────────────────────────────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ LocalPosixVolume│  │   MtpVolume     │  │ InMemoryVolume  │
│ (std::fs copy)  │  │ (download/upload│  │ (for tests)     │
│                 │  │  to local temp) │  │                 │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

**Problem:** `copy_single_path()` in `volume_copy.rs` returns `VolumeError::NotSupported` when both volumes are MTP because there's no way to stream data between them.

---

## New mtp-rs streaming API

```rust
// Download: returns size + stream of chunks
pub async fn download_streaming(&self, handle: ObjectHandle)
    -> Result<(u64, ReceiveStream), Error>

// Upload: accepts size upfront + stream of chunks
pub async fn upload_streaming<S>(&self, parent: Option<ObjectHandle>, info: NewObjectInfo, data: S)
    -> Result<ObjectHandle, Error>
where S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin
```

**Key constraint:** MTP protocol requires knowing file size before upload starts.

---

## Proposed design

### Option A: Add streaming methods to Volume trait (recommended)

Add new **optional** streaming methods that work alongside existing ones:

```rust
// In volume/mod.rs

/// A stream of file data chunks.
pub type DataStream = Pin<Box<dyn Stream<Item = Result<Bytes, VolumeError>> + Send>>;

pub trait Volume: Send + Sync {
    // ... existing methods ...

    // ========================================
    // Streaming: Optional, for MTP-to-MTP
    // ========================================

    /// Returns whether this volume supports streaming export.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Exports a file as a stream of chunks.
    /// Returns (file_size, data_stream).
    ///
    /// The caller MUST consume the entire stream before calling other methods.
    fn export_streaming(&self, path: &Path) -> Result<(u64, DataStream), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Imports a file from a stream of chunks.
    /// The size MUST be known upfront (MTP protocol requirement).
    fn import_streaming(&self, dest: &Path, size: u64, data: DataStream) -> Result<u64, VolumeError> {
        let _ = (dest, size, data);
        Err(VolumeError::NotSupported)
    }
}
```

### Changes needed

#### 1. `volume/mod.rs` - Add streaming methods to trait (~30 lines)

```rust
use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;

pub type DataStream = Pin<Box<dyn Stream<Item = Result<Bytes, VolumeError>> + Send>>;

// Add to Volume trait:
fn supports_streaming(&self) -> bool { false }
fn export_streaming(&self, path: &Path) -> Result<(u64, DataStream), VolumeError> { ... }
fn import_streaming(&self, dest: &Path, size: u64, data: DataStream) -> Result<u64, VolumeError> { ... }
```

#### 2. `volume/mtp.rs` - Implement streaming for MtpVolume (~80 lines)

```rust
impl Volume for MtpVolume {
    fn supports_streaming(&self) -> bool {
        true
    }

    fn export_streaming(&self, path: &Path) -> Result<(u64, DataStream), VolumeError> {
        let mtp_path = self.to_mtp_path(path);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        let handle = tokio::runtime::Handle::current();

        handle.block_on(async move {
            let manager = connection_manager();

            // Resolve path to handle
            let object_handle = manager.resolve_path_to_handle(&device_id, storage_id, &mtp_path).await?;

            // Get streaming download
            let (size, receive_stream) = manager.download_streaming(&device_id, storage_id, object_handle).await?;

            // Wrap ReceiveStream as DataStream
            let data_stream: DataStream = Box::pin(receive_stream.map(|result| {
                result.map_err(|e| VolumeError::IoError(e.to_string()))
            }));

            Ok((size, data_stream))
        })
    }

    fn import_streaming(&self, dest: &Path, size: u64, data: DataStream) -> Result<u64, VolumeError> {
        let mtp_path = self.to_mtp_path(dest);
        let device_id = self.device_id.clone();
        let storage_id = self.storage_id;

        let handle = tokio::runtime::Handle::current();

        handle.block_on(async move {
            let manager = connection_manager();

            // Get parent handle and filename
            let (parent_handle, filename) = manager.resolve_parent_and_name(&device_id, storage_id, &mtp_path).await?;

            // Create object info with known size
            let info = NewObjectInfo::file(&filename, size);

            // Upload from stream
            manager.upload_streaming(&device_id, storage_id, parent_handle, info, data).await?;

            Ok(size)
        })
    }
}
```

#### 3. `mtp/connection.rs` - Add streaming wrappers (~60 lines)

```rust
impl MtpConnectionManager {
    /// Downloads a file as a stream (no temp file).
    pub async fn download_streaming(
        &self,
        device_id: &str,
        storage_id: u32,
        handle: ObjectHandle,
    ) -> Result<(u64, impl Stream<Item = Result<Bytes, MtpConnectionError>>), MtpConnectionError> {
        let device = self.get_device(device_id).await?;
        let storage = device.storage(StorageId(storage_id)).ok_or_else(/* ... */)?;

        let (size, stream) = storage.download_streaming(handle).await
            .map_err(|e| map_mtp_error(device_id, e))?;

        // Wrap the stream to map errors
        let mapped_stream = stream.map(move |result| {
            result.map_err(|e| MtpConnectionError::Other {
                device_id: device_id.to_string(),
                message: e.to_string(),
            })
        });

        Ok((size, mapped_stream))
    }

    /// Uploads a file from a stream (no temp file).
    pub async fn upload_streaming<S>(
        &self,
        device_id: &str,
        storage_id: u32,
        parent_handle: ObjectHandle,
        info: NewObjectInfo,
        data: S,
    ) -> Result<ObjectHandle, MtpConnectionError>
    where
        S: Stream<Item = Result<Bytes, VolumeError>> + Unpin,
    {
        let device = self.get_device(device_id).await?;
        let storage = device.storage(StorageId(storage_id)).ok_or_else(/* ... */)?;

        // Map VolumeError to std::io::Error for mtp-rs
        let mapped_data = data.map(|result| {
            result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        });

        storage.upload_streaming(Some(parent_handle), info, mapped_data).await
            .map_err(|e| map_mtp_error(device_id, e))
    }
}
```

#### 4. `write_operations/volume_copy.rs` - Use streaming for MTP→MTP (~40 lines)

```rust
fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<u64, VolumeError> {
    // ... existing checks ...

    if source_is_local && !dest_is_local {
        // Local → MTP: use import_from_local (existing)
        dest_volume.import_from_local(&local_source, dest_path)
    } else if !source_is_local && dest_is_local {
        // MTP → Local: use export_to_local (existing)
        source_volume.export_to_local(source_path, &local_dest)
    } else if source_is_local && dest_is_local {
        // Local → Local: use export_to_local (existing)
        source_volume.export_to_local(source_path, &local_dest)
    } else {
        // MTP → MTP: use streaming if supported
        if source_volume.supports_streaming() && dest_volume.supports_streaming() {
            copy_via_streaming(source_volume, source_path, dest_volume, dest_path, state)
        } else {
            Err(VolumeError::NotSupported)
        }
    }
}

fn copy_via_streaming(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<u64, VolumeError> {
    // Check cancellation
    if state.cancelled.load(Ordering::Relaxed) {
        return Err(VolumeError::IoError("Operation cancelled".to_string()));
    }

    // 1. Start streaming download from source
    let (size, data_stream) = source_volume.export_streaming(source_path)?;

    // 2. Stream directly to destination
    dest_volume.import_streaming(dest_path, size, data_stream)
}
```

#### 5. Frontend - Remove MTP-to-MTP block (~5 lines)

In `DualPaneExplorer.svelte`, remove lines 1003-1013:

```diff
- // MTP to MTP copy is not supported
- if (sourceIsMtp && destIsMtp) {
-     log.warn('MTP to MTP copy is not supported')
-     alertDialogProps = {
-         title: 'Not supported',
-         message: "Copying between two mobile devices isn't supported yet...",
-     }
-     showAlertDialog = true
-     return
- }
```

---

## Summary of changes

| File | Lines changed | Complexity |
|------|---------------|------------|
| `volume/mod.rs` | +30 | Easy |
| `volume/mtp.rs` | +80 | Medium |
| `volume/local_posix.rs` | +0 | None (optional) |
| `volume/in_memory.rs` | +0 | None (optional) |
| `mtp/connection.rs` | +60 | Medium |
| `write_operations/volume_copy.rs` | +40 | Easy |
| `DualPaneExplorer.svelte` | -10 | Trivial |
| **Total** | ~220 lines | **4-6 hours** |

---

## Why this is simpler than expected

1. **Additive, not replacement** - We're adding new methods, not changing existing ones. Local-to-MTP and MTP-to-local still use the battle-tested temp file approach.

2. **Optional trait methods** - Default implementations return `NotSupported`, so LocalPosixVolume and InMemoryVolume don't need changes.

3. **mtp-rs already does the hard work** - The new `download_streaming()` and `upload_streaming()` handle all the USB-level complexity.

4. **Single code path** - The streaming flow is simple: get (size, stream) from source, pass to destination. No branching.

5. **Synchronous trait with async runtime** - MtpVolume already handles the sync→async bridge with `block_on`, so we just reuse that pattern.

---

## Testing plan

1. **Unit tests** - Mock streams for `copy_via_streaming()`
2. **Integration test** - Two real MTP devices (or same device, two storages)
3. **Edge cases:**
   - Empty file (0 bytes)
   - Large file (>1GB) - verify no memory spike
   - Cancellation mid-stream
   - Device disconnect mid-stream

---

## Alternative: Option B - Keep temp file, add memory buffer option

If streaming proves tricky, we could instead:

```rust
fn copy_via_temp_or_memory(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    max_memory_buffer: usize, // e.g., 100MB
) -> Result<u64, VolumeError> {
    // Get source size
    let metadata = source_volume.get_metadata(source_path)?;
    let size = metadata.size.unwrap_or(0);

    if size <= max_memory_buffer as u64 {
        // Small file: buffer in memory
        let mut buffer = Vec::with_capacity(size as usize);
        source_volume.export_to_memory(source_path, &mut buffer)?;
        dest_volume.import_from_memory(&buffer, dest_path)
    } else {
        // Large file: use temp file
        let temp_dir = std::env::temp_dir().join("cmdr-mtp-staging");
        // ... existing temp file logic ...
    }
}
```

This is simpler (~100 lines) but uses memory for small files and disk for large files.

---

## Recommendation

**Go with Option A (streaming)**. The mtp-rs library already has the hard parts done, and the Volume trait changes are minimal. The result is cleaner architecture and no temp files or memory buffers needed.
