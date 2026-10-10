# Archive edits

The driver for writes inside a `.zip`: mkdir, mkfile, rename, delete, transfer, and compress. Existing-archive writes
are O(archive) temp+rename rewrites. Up: `../CLAUDE.md`; mutation: `crates/cmdr-archive/src/mutation/DETAILS.md`.

## Module map

- `routing.rs`: inner-path helpers, the tar/7z write guard, duplicate pre-check, and instant-op sink builder.
- `driver.rs`: managed lifecycle and delete routing. `engine.rs`: local/remote dispatch; `remote.rs`: pull, apply,
  upload, swap. `edit_error.rs`: shared error leaf. `conflicts.rs`: archive-index resolution.
- Per-shape routes: `copy_into.rs` (transfer routing, plus the remote-source pull), `move_out.rs`, and
  `compress.rs`. Fresh creation uses `fresh_plan.rs` (sources/identity), `fresh_compress.rs` (managed driver),
  `fresh_zip.rs` (bounded producer), and `fresh_validate.rs` (pre-publish checks). Create and rename routes live with
  their instant ops in `../create.rs` and `../rename.rs`.

## Must-knows

- **An archive edit is MANAGED, never instant**: it goes through `spawn_managed`, takes the PARENT drive's lane, and
  marks that drive busy. A `create` / `rename` returns an operation id, ❌ not a path.
- **Every existing-archive apply site runs through `engine::run_managed_edit`**, ❌ never a bare
  `spawn_blocking(mutator::apply(...))`. Fresh compression never enters the mutator.
- **❌ No in-place remote edit.** A remote parent (direct SMB / MTP) goes pull → apply locally → upload to a temp name →
  swap, and the remote ORIGINAL keeps its bytes until that final swap. Keep the four steps in that order and keep the
  cleanup on every early exit; the swap's shape depends on whether the backend allows same-name siblings. DETAILS §
  "Remote edit: the data-safety contract".
- **Routing detection must be PARENT-AWARE**: the seams call the async `VolumeManager::path_is_inside_archive` /
  `path_crosses_archive_boundary`, ❌ never the sync `std::fs`-only predicates, which answer FALSE for an `smb://` /
  `mtp://` path and drop the write onto the parent volume.
- **Fresh compression is seedless and publish-last**: reserve source + destination lanes, stream into a tracked stage,
  reconcile writer/producer/stat counts, parse it through `ArchiveVolume`, then publish. Local, SMB, SFTP, and ADB
  generate direct; WebDAV and MTP spool locally. DETAILS § Compress.
- **Fresh-source name collisions honor the requested policy inside the registered op**: Stop prompts through its
  conflict slot; Skip, Rename, Overwrite, and conditional variants retain their ordinary meanings.
- **`fresh_zip`: EOF is never success** on either queue (`Complete` in, `End` out); join off the async runtime. ONE
  cancellation source (a child of `backend_cancel`) reaches producer, feed, stream, and write callback (`Break`). ❌
  Never race a write against it.
- **Compress progress has two different byte axes**: `Compressing` is uncompressed source bytes; remote
  `Transferring` is completed-ZIP bytes. Both finishing phases clear BOTH totals and ETA. Ordinary archive mutation
  stays `ArchiveEdit` + `Copying`.
- **Move OUT deletes only what durably landed**: extract first, then ONE batch `{ delete }` rewrite over the sources
  that extracted with ZERO deep skips (a hard error deletes the durable prefix; cancel and rollback delete nothing).
  The copy engine's deep `skipped_file_count` fold is what makes that count honest.
- **Unrepresentable entries (symlinks, fifos, devices, broken links) are SKIPPED, never lost**, and any skip suppresses
  a move's source deletion. Every skip increments `skipped_count` and surfaces as `files_skipped`.
- **Conflicts are planned INSIDE the op**, against the working copy `run_managed_edit` hands the closure — planning up
  front would break a remote edit. Stop-mode prompts per FILE (dirs merge silently), storing the sender BEFORE the emit.
- **The terminal `files_processed` is `MutationProgress::entries_changed`**, ❌ not `entries_total`: deleting one file
  from a 3-entry zip reports 1.
- **Compression level comes from `behavior.archiveCompressionLevel`**: fresh creation passes it to `new_stream`;
  existing mutation carries it on the `Changeset`. It applies only to newly written entries; `None` is level six.

Routing, remote-edit safety, changesets, compress, move-out, conflicts, and test coverage: `DETAILS.md`. Read it before
non-trivial work here.
