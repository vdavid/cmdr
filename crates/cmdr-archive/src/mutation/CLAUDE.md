# Zip mutation (write side)

`ArchiveMutator::apply(archive_path, changeset, hooks)` — the zip-only write side, safe-overwrite by temp+rename.
`Volume`-free and manager-free like the [read core](../read/CLAUDE.md): the write-ops `ArchiveEditOperation` driver
wraps it with the real event sink, pause gate, and cancel intent. `mutator.rs` is the write; `changeset.rs` the request
and error types both speak; `plan.rs` decides which original entries an edit keeps, and under what name.

## Must-knows

- **Edits go through `mutator.rs` + the write-ops `ArchiveEditOperation` driver, NOT `ArchiveVolume`'s mutation
  methods** — routing is path-based and backend-side, so those stay `NotSupported` and nothing calls them.
- **Only zip is WRITABLE.** The write chokepoint is the host's `ensure_zip_writable` chokepoint (non-zip → typed
  `ReadOnlyDevice`, untouched). Don't route a non-zip archive here.
- **Temp+rename is the ONLY strategy; never `ZipWriter::new_append`** (it corrupts the archive on cancel). The original
  is byte-for-byte intact until the final atomic rename.
- **Deletes and renames match the READER's name for an entry (rc-zip, by header offset); an unrenamed entry goes through
  `raw_copy_file`, which keeps its stored name bytes and UTF-8 flag** (vendored `zip` patch). ❌ Never write a kept
  entry under `name()`: it re-encodes every Shift-JIS or GBK name as mojibake. DETAILS § "Entry names".
- **A rewrite keeps the comment, any bytes before the zip data (an SFX stub), and a symlink's target as the file it
  replaces** (`apply` resolves a linked archive first). An archive with a duplicate entry name is refused: the `zip`
  crate folds the twins into one. DETAILS § "The rewrite keeps the archive's own shape". So is an edit that would write
  an entry under a FILE entry, which every reader hides (`EntryUnderFile`).
- **A kept ZipCrypto or WinZip AES entry raw-copies byte-identical and still decrypts**: the vendored `zip` patch keeps
  its encrypted and data-descriptor flags (bit 3 picks ZipCrypto's check byte). A scheme it can't carry (Strong
  Encryption, masked headers, bit 3 with an unparseable DOS time) refuses the edit (`carries_its_encryption`). DETAILS §
  "Encrypted entries".
- **`Changeset::compression_level` applies to ADDED entries only, clamped 1..=9 in `add_entry_options`.** The `zip`
  crate HARD-ERRORS on an out-of-range Deflated level (it doesn't clamp), failing the whole edit at the first entry — so
  keep the clamp; don't set a raw level on `FileOptions` elsewhere. `None` = crate default (level 6). See `DETAILS.md` §
  "Compression level applies to ADDED entries only".
- **A new entry's options go through `with_entry_mtime`** (local DOS time + the exact UTC second in the `UT` extra
  field), the host's fresh-ZIP producer included. ❌ Never bare `SimpleFileOptions::default()`: it dates the entry
  1980-01-01.
- **An item's `__MACOSX` sidecar moves with it**: a delete or rename takes it, and an add replaces any at its name (two
  entries at one name fail the edit). Compress writes one only for xattrs worth keeping. DETAILS § "Entries' xattrs".

Depth, rationale, and the data-safety test list: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
