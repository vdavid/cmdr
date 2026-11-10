# Delete + trash

A local-FS walker, a volume walker (every backend), and OS-native trash.

`../../CLAUDE.md` holds the shared `WriteOperationState`, `OperationIntent`, cancel, ETA, and settle contracts;
`../transfer/CLAUDE.md` is the copy + move parallel. Frontend counterpart:
`apps/desktop/src/lib/file-operations/delete/CLAUDE.md`.

## Files

- **`walker.rs`** (local) and **`volume_walker.rs`** (any `Volume`, its scan in `volume_scan.rs`), routed by
  `../local_start.rs`. The volume scan asks `authoritative_listing` before every `list_directory`; deletes batch per
  `delete_batch_size`. DETAILS § "Volume-delete internals".
- **`outage.rs`**: waits out a dropped share. ❗ A re-run re-checks the entry (kind, size); a mismatch stops the delete
  (`ChangedWhileDisconnected`). ❌ No `through_outages` around a delete. DETAILS § "Waiting out a dropped share".
- **`trash.rs`**: `move_to_trash_sync()`, `trash_files_with_progress()` (batch, per-item progress, cancel), and
  `trash_dir_for_path()` (❌ keep its ancestor walk, DETAILS § Where a trash is). Each item it can't take emits a
  `Failed` source-item event.
- **`cloud_trash.rs`**: `routing_for_selection()` behind the `trash_routing_for_paths` command. DETAILS § "A trash of
  online-only cloud content becomes a delete".
- **`volume_start.rs`**: a volume delete's managed `async` lifecycle. DETAILS § "The volume delete's own lifecycle".
- **`already_gone.rs`**: an already-gone item is done, ❌ never `SourceNotFound`: top-level, past a preview
  (`MissingSource`), mid-walk (only a POSITIVE `NotFound`). DETAILS § "An item that's already gone is done".

## Must-knows

- **Files first, then directories deepest-first**, so `remove_dir` always finds an empty one. **Not rollbackable**:
  Cancel stops further deletes.
- **Both walkers reuse the scan-preview cache via `config.preview_id`** and emit identical events; on a hit an initial
  `phase: Deleting` event keeps the FE's denominator right.
- **A `preview_id` alone authorizes nothing.** The LOCAL walker deletes `scan_result.files` as is, so bind with
  `take_cached_delete_scan` (sources AND a delete-purpose walk); ❌ never skip it.
- **A symlink is a LEAF to the volume walker**: descend only an `EntryKind::Directory` (`EntryKind::of`,
  `Volume::entry_kind`), ❌ never `is_directory` or a preview's word (both follow links); ❌ nor a listing that hides
  links (`listings_mark_links`). DETAILS § "A link is a leaf".
- **A delete reaching a mount point refuses WHOLE, up front** (`MountedVolumeInside`), in both walkers and the delete
  preview, and again right before deleting. DETAILS § "A mount is a wall".
- **Never the person's home folder or a folder holding it**: delete, trash, and the delete preview refuse up front
  (`../home_folder.rs`). DETAILS § "The home folder stays".
- **❌ Never resolve a top-level source's type with `.unwrap_or(false)`.** Hand the `Option` to `scan_volume_recursive`,
  which propagates a failed probe: a guessed "file" books zero bytes for a whole tree.
- **Trash has no scan phase**: `trashItemAtURL` is atomic per top-level item, so progress tracks items. A PARTLY refused
  batch still COMPLETES, and its event carries `refused` (count + `strongest_refusal`): ❌ never `None`, or the ending
  reads as a clean success.
- **ONLINE-ONLY content (`SF_DATALESS`) in `~/Library/CloudStorage/<domain>/` trashes as a permanent delete**: trashing
  an evicted file DOWNLOADS it first. ❌ Never route on the folder alone (an ordinary Dropbox file trashes fine; routing
  it is data loss), ❌ never on the refusal; only a provider we know by name, all-or-nothing. A FOLDER is answered by
  the scan preview's walk (`OnlineOnlyWatch`), ❌ no second walk. DETAILS.
- **A refusal carries a typed `TrashRefusalKind`, read from the `NSError` DOMAIN + CODE**, ❌ never its localized words
  (`error-string-match` forbids it). A failed batch is `WriteOperationError::TrashRefused`, ❌ not an `IoError` (that
  leaves the dialog only "try again"), reporting `strongest_refusal`, ❌ not the most common one.
- **A test that really trashes holds `testing::TrashCleanup`** (`docs/testing.md`).
- **Delete and trash don't `fsync`, and ❌ never a global `sync(2)`** (it stalled every other app; `clippy.toml` bans
  `libc::sync`).
- **A scan bailing with `Err(Cancelled)` emits no `write-cancelled`; its top-level caller does**
  (`emit_cancelled_if_aborted`), or it fires once per frame.

Depth: `DETAILS.md`.
