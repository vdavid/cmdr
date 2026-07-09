# Zip mutation (write side)

`ArchiveMutator::apply(archive_path, changeset, hooks)` — the zip-only write side, safe-overwrite by temp+rename.
`Volume`-free and manager-free like the [read core](../read/CLAUDE.md): the write-ops `ArchiveEditOperation` driver
wraps it with the real event sink, pause gate, and cancel intent.

Depth, rationale, and the data-safety test list: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.

## Must-knows

- **Edits go through `mutator.rs` + the write-ops `ArchiveEditOperation` driver, NOT `ArchiveVolume`'s mutation
  methods** — routing is path-based and backend-side, so those stay `NotSupported` and nothing calls them.
- **Only zip is WRITABLE.** The write chokepoint is `write_operations::archive_edit::ensure_zip_writable` (non-zip →
  typed `ReadOnlyDevice`, untouched). Don't route a non-zip archive here.
- **Temp+rename is the ONLY strategy; never `ZipWriter::new_append`** (it corrupts the archive on cancel). The original
  is byte-for-byte intact until the final atomic rename.
- **An edit that would RETAIN an encrypted entry is refused** (`zip`'s raw copy drops the PKWARE flag → silent
  corruption). Deleting an encrypted entry is fine.
- **`Changeset::compression_level` applies to ADDED entries only, clamped 1..=9 in `add_entry_options`.** The `zip`
  crate HARD-ERRORS on an out-of-range Deflated level (it doesn't clamp), failing the whole edit at the first entry —
  so keep the clamp; don't set a raw level on `FileOptions` elsewhere. `None` = crate default (level 6). See
  [DETAILS.md](DETAILS.md) § "Compression level applies to ADDED entries only".
