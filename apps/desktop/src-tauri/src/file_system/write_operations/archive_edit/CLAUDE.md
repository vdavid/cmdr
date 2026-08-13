# Archive edits

The driver for writing INSIDE a `.zip`: mkdir, mkfile, rename, delete, copy/move into, move out, and compress. Every
one is an O(archive) temp+rename rewrite of the whole file. Up: `../CLAUDE.md`. The mutation mechanism itself
(`ArchiveMutator`, safe-overwrite) is the backend crate's: `crates/cmdr-archive/src/mutation/DETAILS.md`.

## Module map

- `routing.rs`: the shared primitives every route builds on — inner-path helpers, `ensure_zip_writable` (the one
  write-side chokepoint refusing tar/7z), `archive_inner_exists` (the duplicate pre-check), the instant-op sink builder.
- `driver.rs`: `archive_edit_start` (the managed op's whole lifecycle) plus `route_archive_delete`. `engine.rs`:
  `run_managed_edit`, the local-vs-remote dispatcher. `conflicts.rs`: resolution against the archive index.
- Per-shape routes: `copy_into.rs` (`route_archive_copy_into`, plus the remote-source pull), `move_out.rs`,
  `compress.rs` (`seed_empty_zip` + `compress_start`). The create and rename routes live with their instant ops in
  `../create.rs` and `../rename.rs`, and call in here.

## Must-knows

- **An archive edit is MANAGED, never instant**: it goes through `spawn_managed`, takes the PARENT drive's lane, and
  marks that drive busy. A `create` / `rename` returns an operation id, ❌ not a path.
- **Every apply site runs through `engine::run_managed_edit`**, ❌ never a bare `spawn_blocking(mutator::apply(...))` —
  that dispatcher is what makes one closure work for both a local and a remote parent.
- **❌ No in-place remote edit.** A remote parent (direct SMB / MTP) goes pull → apply locally → upload to a temp name →
  swap, and the remote ORIGINAL keeps its bytes until that final swap. Keep the four steps in that order and keep the
  cleanup on every early exit; the swap's shape depends on whether the backend allows same-name siblings. DETAILS §
  "Remote edit: the data-safety contract".
- **Routing detection must be PARENT-AWARE**: the seams call the async `VolumeManager::path_is_inside_archive` /
  `path_crosses_archive_boundary`, ❌ never the sync `std::fs`-only predicates, which answer FALSE for an `smb://` /
  `mtp://` path and drop the write onto the parent volume.
- **The empty-zip seed is LOAD-BEARING for compress**: `ZipArchive::new` rejects a 0-byte file, so a brand-new target
  gets a valid 22-byte archive first, placed the same way its parent is reached (`seed_empty_zip` local,
  `seed_empty_zip_remote` through the volume). ❌ Don't "optimize" it away.
- **Move OUT deletes only what durably landed**: extract first, then ONE batch `{ delete }` rewrite over the sources
  that extracted with ZERO deep skips (a hard error deletes the durable prefix; cancel and rollback delete nothing).
  The copy engine's deep `skipped_file_count` fold is what makes that count honest.
- **Unrepresentable entries (symlinks, fifos, devices, broken links) are SKIPPED, never lost**, and any skip suppresses
  a move's source deletion. Every skip increments `skipped_count` and surfaces as `files_skipped`.
- **Conflicts are planned INSIDE the op**, against the working copy `run_managed_edit` hands the closure — planning up
  front would break a remote edit. Stop-mode prompts per FILE (dirs merge silently), storing the sender BEFORE the emit.
- **The terminal `files_processed` is `MutationProgress::entries_changed`**, ❌ not `entries_total`: deleting one file
  from a 3-entry zip reports 1.
- **Compression level rides on the `Changeset`** (from the `behavior.archiveCompressionLevel` setting) and applies to
  newly ADDED entries only; `None` means the crate default.

Routing detail, the remote-edit contract and its stale-temp reap, the per-op changesets, compress, move-out, conflicts,
and the mutation-test coverage: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
