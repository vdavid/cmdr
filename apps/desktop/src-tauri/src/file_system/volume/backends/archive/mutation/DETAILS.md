# Zip mutation — details

Pull-tier docs for the zip write side. Must-know invariants live in [CLAUDE.md](CLAUDE.md). Read this before any
non-trivial work here: editing, planning, reorganizing, or advising.

The write side is `Volume`-free and manager-free like the [read core](../read/DETAILS.md). Full driver wiring (event
sink, pause gate, cancel intent via the `MutationHooks` seam, and the remote pull→edit→upload→swap flow):
[`../../../../write_operations/DETAILS.md`](../../../../write_operations/DETAILS.md) § "Archive edits".

## Temp+rename safe-overwrite (`mutator.rs`)

`ArchiveMutator::apply(archive_path, changeset, hooks)` applies a batched `Changeset` (`add` / `mkdir` / `delete` /
`rename`; a mkfile is an add of empty bytes) by building the FULL new archive into a same-directory sibling temp
(`foo.zip.cmdr-tmp-<uuid>`), then atomically renaming it over the original.

- **Temp+rename, not append-in-place.** `zip`'s `ZipWriter::new_append` overwrites the old central directory, so a
  cancel mid-edit corrupts the archive (verified: truncating before the new EOCD yields "Could not find EOCD"; the
  original does NOT survive). Building fresh into a temp and renaming is the app's mandated safe-overwrite and the only
  shape where cancel is genuinely free (abandon the temp, no rollback ledger). The original is byte-for-byte untouched
  until the final rename; a cancel or crash at any earlier point leaves it fully readable.
- **Retained entries copy verbatim** via `raw_copy_file_rename` (no decompress/recompress); only added files
  stream-compress (chunked, never whole-buffered — the add chunk is also the pause/cancel granularity mid-file). An
  added file carries its SOURCE's modification time into the entry (`add_entry_options`), not the write time — zip stores
  it as MS-DOS date/time (2-second granularity, 1980–2107 range; an mtime outside that range keeps the default). The
  decompose is done in UTC because `rc-zip` reads the DOS fields back as UTC, so the mtime round-trips through the index
  parse.
- **Metadata preservation (the archive FILE, not the entries).** A rewrite yields a fresh inode, so the original `.zip`'s
  mode, timestamps, and xattrs are carried onto the temp before the swap: macOS `copyfile` with
  `COPYFILE_STAT | COPYFILE_ACL | COPYFILE_XATTR`. `COPYFILE_STAT` carries mode and all timestamps INCLUDING the
  creation/birth date (`st_birthtime` lives in the inode, not an xattr); `COPYFILE_XATTR` carries Finder tags,
  quarantine, and `com.apple.FinderInfo` VERBATIM — a faithful copy that keeps the custom-icon flag, so the `tags.rs`
  FinderInfo gotcha doesn't apply here. mode+mtime+xattr elsewhere. Best-effort: metadata loss never fails a data-safe
  edit.
- **External replacement of the `.zip` between planning and the final rename is last-writer-wins.** The changeset is
  planned against the archive as parsed at plan time; if an outside process rewrites the same `.zip` before the mutator's
  atomic rename, that outside write is simply overwritten by our temp (and vice-versa — a rename that lands after ours
  wins). This is acceptable for the single-user local model Cmdr targets; there's no cross-process lock. It's stated here
  so a future multi-writer scenario revisits it deliberately rather than assuming a guard exists.
- **Decision — refuse to retain an encrypted entry (data-safety, deviates from the plan).** `zip`'s raw copy
  reconstructs an entry's options from `ZipFile::options()`, which does NOT carry the traditional-PKWARE encryption GP
  flag. So a retained encrypted entry would keep its ciphertext bytes but lose the "encrypted" header bit — semantically
  corrupt (a reader hands back ciphertext as plaintext). `apply` therefore returns `EncryptedEntryRetained` for any edit
  that would KEEP an encrypted entry, leaving the original untouched. Deleting an encrypted entry is allowed (it isn't
  retained). Editing encrypted archives is out of scope in v1. The plan's "raw_copy retains encrypted entries
  byte-for-byte" claim is false against `zip` 8.6 (verified by `mutator_test.rs`); this refusal is the resolution.
- **Leftover policy — no startup reaper.** A leftover `foo.zip.cmdr-tmp-*` is always an ABANDONED build (the original is
  intact), so it's harmless. `apply` reaps siblings of the target at the START of the next edit of that archive (before
  building its own fresh-uuid temp), which is sufficient; a cancel/error removes its own temp immediately via an RAII
  guard. Caveat for a REMOTE archive: the sibling-reap runs on the LOCAL scratch copy, so a leftover
  `foo.zip.cmdr-tmp-*` on the REMOTE share (a crash after upload, before swap) is NOT reaped by the next edit — it stays
  until the user removes it. Still harmless: the original is intact, and the leftover holds the fully-uploaded NEW bytes
  (see `write_operations/archive_remote_edit.rs`).
- **Deletes/renames reshape the retained set.** A delete drops a file or a whole subtree (component-wise match, so
  `foo` never catches `foobar`); a rename rewrites a subtree prefix. Both are computed per original entry in one pass
  (`plan_new_name`); deletes win over renames.

## Testing

`mutator_test.rs` is the red-first TDD anchor for every data-safety property: round-trips (add/delete/rename/mkdir/mkfile
verified via our reader AND external `unzip -t`), cancel-midway-leaves-the-original-intact-with-no-temp, a leftover temp
reaped on the next edit, the merge invariant (a delete keeps siblings' raw compressed bytes byte-identical), metadata
(mode/mtime/xattr) survival, pause-parks-mid-add-then-completes-on-resume, and the encrypted-entry refusal.
`write_operations/archive_edit_tests.rs::ensure_zip_writable_allows_zip_and_refuses_read_only_formats` pins the
non-zip-refuses-typed matrix.
