# MTP upload leaves a partial/corrupt file on the device after mid-stream failure

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/mtp/connection/file_ops.rs:490-581` (`upload_from_stream`); the chunk adapter that injects cancel is at `apps/desktop/src-tauri/src/file_system/volume/backends/mtp.rs:803-871`.

## What
When an MTP upload is interrupted mid-transfer — user cancel (the chunk adapter injects an `Interrupted` io::Error), device disconnect, or the `MTP_TIMEOUT_SECS * 10` timeout — `storage.upload(...)` returns `Err` and `upload_from_stream` propagates it via `?` with **no cleanup of the partially-written object** on the device. Contrast the download path (`file_ops.rs:178-186`), which explicitly `remove_file`s the partial local file on cancel. The upload path has no equivalent, so a truncated object can be left in the destination MTP folder.

## Why it matters
Copying photos onto an Android device over USB; the cable is bumped mid-file. The destination gets a half-written file that shows up in the listing but is corrupt, and the user isn't told to discard it. On a cross-volume **move** this isn't loss of the source (delete only happens after `Ok`), but the device is left with a silent corrupt artifact at the target name.

## Evidence
```rust
// file_ops.rs ~544
let new_handle = tokio::time::timeout(
    Duration::from_secs(MTP_TIMEOUT_SECS * 10),
    storage.upload(parent_opt, object_info, data_stream),
)
.await
.map_err(|_| MtpConnectionError::Timeout { device_id: device_id.to_string() })?  // ← no delete of partial object
.map_err(|e| map_mtp_error(e, device_id))?;                                       // ← no delete of partial object
```

## Suggested fix
On the `Err`/timeout branch of `storage.upload`, attempt a best-effort delete of the just-created object before returning the error (resolve the destination handle/path and call the device delete), mirroring the download path's `remove_file` cleanup. First confirm with mtp-rs whether a failed `SendObject` already removes the partial object on the device; if it does, document that and drop the concern. Add a virtual-MTP test that cancels mid-upload and asserts the destination object is absent afterward.

## Notes
Confidence is medium because whether mtp-rs / the device itself discards a partial `SendObject` is unverified — if it does, this is a non-issue. The asymmetry with the download path (which does clean up) is the signal that the upload path was not given the same treatment. Sibling to the SMB partial-write finding.
