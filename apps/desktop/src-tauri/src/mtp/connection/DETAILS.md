# MTP connection details

Depth and rationale. `CLAUDE.md` holds the must-knows; the depth lives here.

## Upload partial cleanup (two-phase PTP uploads)

PTP uploads are two-phase: `SendObjectInfo` creates the object on the device, then `SendObject` streams the bytes. If the
data phase fails (a genuine error or a user cancel), mtp-rs's `Storage::upload` returns `Err(mtp_rs::UploadError)` whose
`partial: Some(handle)` carries the created-but-incomplete object. The library deliberately does not auto-delete it; the
caller owns the cleanup-or-resume decision.

Per cmdr's no-corrupt-artifact policy (AGENTS.md principle #4), both upload call sites in `file_ops.rs` (`upload_file`,
`upload_from_stream`) best-effort delete the partial via `storage.delete(handle)` before returning, then surface the
mapped `upload_err.source` error. This holds for cancel too: on cancel, `source` is `Error::Cancelled` and `partial` is
`Some`, so a cancelled upload also deletes the half-file (the user cancelled, don't leave it on their phone), and
`map_mtp_error(Error::Cancelled)` still yields `MtpConnectionError::Cancelled`, preserving cancel classification for the
write-op layer.

The delete needs a live device/session. If the device just disconnected, the delete fails: we log under
`target: "mtp_upload"` and move on (the partial lingers, recognizable; nothing we can do with a dead device). A failed
cleanup never masks the original upload error. Pinned by `upload_failure_deletes_partial_object_on_device` and
`upload_cancel_deletes_partial_and_surfaces_cancelled` (virtual-mtp tests in `volume/backends/mtp.rs`).

## Stale parent handle on upload (self-heal + one-shot retry)

`resolve_path_to_handle` is cache-only: the parent-folder handle an upload uses comes from whenever the user last listed
that folder. Android routes MTP through MediaProvider, whose object handles are NOT stable across a media rescan, so a
handle can go stale between the listing and a later upload into the folder. The device then rejects `SendObjectInfo`
(phase 1, before any source byte is read) with `InvalidParentObject` (or `InvalidObjectHandle`). Field report: a 307 MB
upload into a Pixel's `/Documents` failed this way, surfaced to the user as a "Path not found" on the intact *source*
file (`map_volume_error` funneled `VolumeError::NotFound` into `SourceNotFound`), with no log and no retry.

The recovery, split across two layers because the data stream is single-use:

- **Connection layer (`upload_from_stream`)**: `is_stale_handle_rejection(&upload_err.source)` classifies the rejection.
  Then — crucially — it DROPS the device lock before healing: `refresh_dir_handle` re-acquires the lock through
  `list_directory`, and the per-device lock is a non-reentrant `tokio::sync::Mutex`, so refreshing while still holding it
  deadlocks. `refresh_dir_handle` re-lists the folder's ancestors root-first (invalidating each listing cache first so
  the 5 s TTL can't serve a stale listing); listing `parent(dir)` repopulates the fresh handle for `dir`. Root is a
  constant, so a top-level folder like `/Documents` heals with a single re-list of `/`. The method then returns
  `MtpConnectionError::StaleParentHandle { dest_folder }` (→ `VolumeError::StaleDestinationHandle`). It does NOT retry
  the upload itself — the `data_stream` was moved into `Storage::upload` and consumed.
- **Transfer engine (`write_operations/transfer/volume_strategy.rs::stream_pipe_file`)**: owns the retry because it can
  re-open the source. On `VolumeError::StaleDestinationHandle` it re-opens the source stream and re-runs
  `write_from_stream` once (`retried` budget of 1). Safe to restart the whole file: the rejection lands before any
  source byte is read or destination byte written, so no progress double-counts and no partial lingers (on
  `InvalidParentObject` mtp-rs creates nothing, so `UploadError.partial` is `None`).

If the retry also fails, `map_volume_error` maps `StaleDestinationHandle` → `WriteOperationError::WriteError { path:
dest_folder }` ("Couldn't write to the destination…"), a destination-correct message — never `SourceNotFound`. All
upload failures now also `log::warn!` under `target: "mtp_upload"`, so a bare protocol rejection leaves a breadcrumb.
Pinned by `upload_into_stale_parent_handle_heals_and_retry_succeeds` (connection layer) and
`stream_pipe_file_retries_once_on_stale_destination_handle` (engine).
