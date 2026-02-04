# Chunked copy improvements

## Summary

Three improvements to the chunked copy implementation for network filesystems:
1. Add progress reporting during large file copies
2. Make `sync_all()` optional/cancellable
3. Add ACL support

Plus clarification on why iSCSI doesn't need special handling.

## Background

The chunked copy implementation works well for cancellation but has gaps:
- **No intra-file progress**: For a 5GB file, the UI shows 0% until the file completes
- **Blocking sync_all()**: `sync_all()` on line 180 can block for seconds on network drives
- **Missing ACL support**: Only copies xattrs, timestamps, permissions - not ACLs

## Design

### 1. Progress callback for intra-file updates

Add an optional progress callback to `chunked_copy_with_metadata`:

```rust
pub type ChunkedCopyProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

pub fn chunked_copy_with_metadata(
    source: &Path,
    dest: &Path,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<&ChunkedCopyProgressCallback>,  // (bytes_done, bytes_total)
) -> Result<u64, WriteOperationError>
```

In `copy_data_chunked`, call the callback after each chunk write:

```rust
total_bytes += bytes_read as u64;
if let Some(cb) = progress_callback {
    cb(total_bytes, source_size);
}
```

The callback in copy.rs will:
- Update `bytes_done` with the current file's progress
- Emit progress events if the interval has elapsed

### 2. Remove sync_all()

After analysis, `sync_all()` is unnecessary for our use case:

- **Network writes are synchronous**: SMB/NFS `write()` calls push data to the server
- **fsync doesn't guarantee durability on network**: The server may still buffer
- **We already have async sync**: `spawn_async_sync()` runs at operation completion
- **Blocking sync_all() defeats cancellation**: User can't cancel during sync

**Decision**: Remove `sync_all()` entirely. The existing async sync at operation completion is sufficient.

### 3. ACL support via exacl crate

Add `exacl` crate for cross-platform ACL support:

```toml
exacl = "0.12"
```

Add `copy_acls()` function:

```rust
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "freebsd"))]
fn copy_acls(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    use exacl::{getfacl, setfacl, AclOption};

    match getfacl(source, AclOption::empty()) {
        Ok(acl) => {
            if let Err(e) = setfacl(&[dest], &acl, AclOption::empty()) {
                log::debug!("copy_acls: failed to set ACL on {}: {}", dest.display(), e);
            }
        }
        Err(e) => {
            log::debug!("copy_acls: failed to get ACL from {}: {}", source.display(), e);
        }
    }
    Ok(())
}
```

### 4. iSCSI and similar: no special handling needed

iSCSI, Fibre Channel, and similar storage appear as **local block devices** to macOS:
- `f_fstypename` shows "apfs" or "hfs", not a network protocol
- The block device layer responds properly to `copyfile()` cancellation
- Performance is similar to local SSD

The chunked copy is specifically for **network filesystem protocols** (SMB, NFS, AFP, WebDAV) where:
- The VFS layer buffers data before sending over the network
- `copyfile()` ignores `COPYFILE_QUIT` because the syscall is "complete" from its perspective

**No changes needed** - current detection is correct.

## Implementation

### Files to modify

1. `Cargo.toml` - Add exacl dependency
2. `chunked_copy.rs` - Add progress callback, remove sync_all, add copy_acls
3. `copy.rs` - Wire up progress callback in both copy functions

### Changes to chunked_copy.rs

```rust
// At top of file
pub type ChunkedCopyProgressCallback<'a> = &'a dyn Fn(u64, u64);

// Update signature
pub fn chunked_copy_with_metadata(
    source: &Path,
    dest: &Path,
    cancelled: &Arc<AtomicBool>,
    progress_callback: Option<ChunkedCopyProgressCallback>,
) -> Result<u64, WriteOperationError> {
    let source_size = std::fs::metadata(source)
        .map(|m| m.len())
        .unwrap_or(0);

    let bytes = copy_data_chunked(source, dest, cancelled, source_size, progress_callback)?;
    // ... metadata copy
}

fn copy_data_chunked(
    source: &Path,
    dest: &Path,
    cancelled: &Arc<AtomicBool>,
    source_size: u64,
    progress_callback: Option<ChunkedCopyProgressCallback>,
) -> Result<u64, WriteOperationError> {
    // ... existing code ...

    loop {
        // ... read and write ...

        total_bytes += bytes_read as u64;

        // Report progress after each chunk
        if let Some(cb) = progress_callback {
            cb(total_bytes, source_size);
        }
    }

    // REMOVED: dst_file.sync_all() - unnecessary, see design notes
    Ok(total_bytes)
}

// Add copy_acls to copy_metadata
fn copy_metadata(source: &Path, dest: &Path) -> Result<(), WriteOperationError> {
    copy_xattrs(source, dest)?;
    copy_acls(source, dest)?;  // NEW
    copy_timestamps(source, dest)?;
    copy_permissions(source, dest)?;
    Ok(())
}
```

### Changes to copy.rs

Wire up the callback to update progress:

```rust
// In copy_single_file_sorted, around line 507:
let bytes = if is_network_filesystem(&actual_dest) {
    log::debug!("copy: using chunked copy for network destination {}", actual_dest.display());

    // Create progress callback that updates bytes_done
    let progress_cb = |chunk_bytes: u64, _total: u64| {
        // Update current file's byte progress (not cumulative)
        // This will be used for progress reporting
    };

    chunked_copy_with_metadata(source, &actual_dest, &state.cancelled, Some(&progress_cb))?
}
```

The tricky part: the progress callback needs to emit events. We have two options:

**Option A**: Pass references to the progress state into the callback
- Complex lifetimes, might need RefCell or similar

**Option B**: Have the callback emit events directly
- Needs app handle and operation_id in callback closure
- More straightforward

Going with **Option B** - the callback will emit progress events directly if the interval has elapsed.

## Testing

```bash
# Run existing tests (should still pass)
cd apps/desktop/src-tauri
cargo nextest run chunked_copy
cargo nextest run copy

# Manual test: Copy large file to network drive
# - Should see progress updates during the file (not just after)
# - Cancellation should still be instant

# Lint checks
./scripts/check.sh --check clippy
./scripts/check.sh --check rust-tests
```

## Task list

- [x] Add exacl dependency to Cargo.toml
- [x] Add progress callback type and parameter to chunked_copy_with_metadata
- [x] Update copy_data_chunked to report progress after each chunk
- [x] Remove sync_all() call
- [x] Add copy_acls function
- [x] Update copy_metadata to call copy_acls
- [x] Update copy.rs copy_single_file_sorted to wire up callback
- [x] Update copy.rs copy_path_recursive to wire up callback
- [x] Update existing tests for new signature
- [x] Add test for progress callback
- [x] Run cargo nextest and clippy
