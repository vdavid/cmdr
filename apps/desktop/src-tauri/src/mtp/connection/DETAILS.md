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
