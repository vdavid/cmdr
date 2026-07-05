# Archive backend (zip, tar, 7z)

Two layers: a **read core** (parse → synthetic tree; streaming decompress) and `ArchiveVolume`, the `Volume` built on
it. The core is **decoupled from the `Volume` trait** (archive-native types: `ArchiveIndex`, `ArchiveNode`,
`ArchiveError`); `volume.rs` alone maps them onto `FileEntry` / `VolumeError` / `VolumeReadStream`. Keep the core
submodules `Volume`-free.

**Formats: zip (browse + extract + WRITE), tar / tar.gz / tar.xz / tar.bz2 / tar.zst / 7z (browse + extract, READ-ONLY).**
The tree, Zip Slip sanitizer, DoS caps, cache, and byte-source seam are format-agnostic; only parsing and the per-entry
read handle differ per format. Extraction is all pure-Rust and streams in bounded chunks (never whole-buffered).

## Module map

- `volume.rs`: `ArchiveVolume` — the read-only `Volume` impl (browse + extract + `scan_for_copy`) over the core below.
- `format.rs`: `ArchiveFormat` / `TarCodec`, suffix→format detection (`format_for_name`), and the pure-Rust tar codec
  decoder factory. `is_sequential` = the access class.
- `index.rs`: `ArchiveIndex` (parsed tree + query surface + the `EntryStore` dispatch), the generic tree builder.
- `zip.rs` / `tar.rs` / `sevenz.rs`: the per-format parse (→ `RawEntry` list) and per-entry streaming producer.
- `source.rs`: `ArchiveByteSource` (byte-supply seam) + `LocalFileSource`, `BytesSource`, `SourceReader` (Read+Seek
  adapter for the tar/7z decoders), `TailCachedSource`.
- `name.rs`: `sanitize_entry_name` (Zip Slip defense). `read.rs`: `ArchiveEntryReader`. `cache.rs`: `ArchiveIndexCache`.
- `boundary.rs`: the SHARED archive-boundary detector + per-format magic (`VolumeManager::resolve` and
  `commands/volumes.rs` both use it; two would drift).
- `mutator.rs`: the WRITE side (temp+rename safe-overwrite, ZIP ONLY). `Volume`-free; the write-ops
  `ArchiveEditOperation` driver wraps it. See [DETAILS.md](DETAILS.md) § "Zip mutation".

Depth, rationale, and full test list: [DETAILS.md](DETAILS.md); read before non-trivial work here.

## Must-knows

- **Zip Slip is enforced at this layer. `sanitize_entry_name` is the single choke point every entry passes before it
  enters the tree; don't bypass it.** Guarantee: no `Accepted` inner path, joined under any root, escapes that root. `..`
  components are quarantined (rejected); absolute paths are clamped to root (leading `/` stripped); `\`→`/`. Pinned by
  the `name.rs` tests. Don't swap in rc-zip's `Entry::sanitized_name` (coarser `contains("..")`, skips `\`).

- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`**: its only public entry reader borrows its
  `ArchiveHandle` (can't back an owned, cached stream), and it decompresses on the async executor (we need it off).
  Codec features live on the `rc-zip` dep. See [DETAILS.md](DETAILS.md) § Decision.

- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format magic
  at navigation.** Longest-suffix wins, so `.tar.gz` is a gzip tar and a bare `.gz` is NOT an archive. A plain `.tar`'s
  ustar magic is at offset 257, so the confirm reads 512 bytes. Details: [DETAILS.md](DETAILS.md) § "tar and 7z".

- **Only zip is WRITABLE; tar/7z are read-only.** The write-side chokepoint is
  `write_operations::archive_edit::ensure_zip_writable`; a non-zip target returns typed `ReadOnlyDevice`, untouched.
  Don't route a non-zip archive to the mutator (it parses-as-zip and fails).

- **Compressed tar and 7z are SEQUENTIAL-access (`Volume::extraction_is_sequential`).** A single extract prefix-decodes
  to the target (O(prefix)); 7z solid blocks consume earlier entries to advance the stream. Plain `.tar` and zip are
  random-access. The whole-subtree copy is O(n²) (one-pass planner deferred): [DETAILS.md](DETAILS.md) § "tar and 7z".

- **Decompression runs on `spawn_blocking`, never on the executor; reads are chunked, never whole-entry buffered.**
  `ArchiveEntryReader` is a bounded-channel producer/consumer (≤128 KiB/chunk, capacity 4 ⇒ ~512 KiB peak). Dropping
  the reader cancels the producer. Don't add a whole-entry `Vec` anywhere in the read path.

- **The byte source is blocking and `pread`-shaped (`ArchiveByteSource`).** A LOCAL archive uses `LocalFileSource`; a
  REMOTE one (direct SMB / MTP) uses `VolumeByteSource`, which bridges to the parent volume's async `read_range`.
  `ArchiveVolume` picks local vs remote by `parent.supports_local_fs_access()`, NOT by whether the path opens locally
  (a direct-SMB parent must read through the parent, never its possibly-hung OS mount). `SmbVolume::read_range` is
  implemented via `smb2::FileReader`, which currently rides a TEMPORARY workspace-root `[patch.crates-io]` override until
  the primitive is published — don't land that patch on `main`; merge checklist in DETAILS. Full model (the `block_on`
  bridge, the tail cache, the primitive): [DETAILS.md](DETAILS.md) § "Remote-backed archives (read path)". Shared as
  `Arc` across concurrent reads (no shared cursor, so parallel reads are independent).

- **Encryption: browsing works, extraction doesn't.** Detected from GP flag bit 0 or the AE-x method (not in
  `rc_zip::Error`). `open_read` on an encrypted entry returns `Encrypted`; `has_encrypted_entries()` gates up front.

- **Errors are typed (`no-string-matching`).** `matches!(err, ArchiveError::Corrupt(_))`, never a message substring.
  Magic-byte format detection (RAR/7z vs zip) is the routing layer's job, not ours.

- **Filename encoding is rc-zip's job** (CP437 vs the often-wrong UTF-8 flag); consume the decoded `entry.name`, don't
  re-decode.

- **The index cache key is `(path, size, mtime)`,** so an external edit auto-invalidates. `index_for_local` is blocking
  — call it from `spawn_blocking`. No eviction here; the volume layer owns archive lifetime and calls `clear()`.

## Zip mutation (`mutator.rs`)

- **Editing goes through `mutator.rs` + the write-ops driver, NOT `ArchiveVolume`'s mutation methods.** Routing is
  path-based and backend-side: `create`/`rename`/`delete` inside and copy/move INTO a zip route to the
  `ArchiveEditOperation` driver, which drives `ArchiveMutator::apply` (temp+rename) directly from the archive path. So
  `ArchiveVolume`'s `create_file`/`delete`/`rename` stay `NotSupported` on purpose — nothing calls them.
- **Temp+rename is the ONLY strategy; never `ZipWriter::new_append`** (it overwrites the old central directory and
  corrupts the archive on cancel — the original does not survive). The original is byte-for-byte intact until the final
  atomic rename.
- **An edit that would RETAIN an encrypted entry is refused** (`zip`'s raw copy drops the PKWARE encryption flag, which
  would silently corrupt the entry). Deleting an encrypted entry is fine. See [DETAILS.md](DETAILS.md) § "Zip mutation".

## `ArchiveVolume` (the `Volume` layer)

- **`ArchiveVolume` itself is read-only.** Every mutation method returns `NotSupported`, incl. `create_directory_all`
  (overridden — the trait default falsely returns `Ok` on an existing dir). Writes happen via `mutator.rs` (above), not
  here.
- **`lane_key()` and `get_space_info()` delegate to the PARENT volume, never the archive** — the parent owns the
  serialization lane and the real disk cost; delegating also dodges `available = 0` (reads as "disk full", blocks
  paste). Capability flags + typed `ArchiveError → VolumeError` mapping: [DETAILS.md](DETAILS.md) § "The `ArchiveVolume`
  layer".

- **This layer is headless: it never registers itself.** `VolumeManager::resolve` (async — a REMOTE `.zip` is confirmed
  through the parent's `get_metadata` + a four-byte `read_range`, not `std::fs`) routes a `.zip`-crossing path here
  (on-demand, archive LRU, backend-internal id). A backend that can't do positioned reads yet routes anyway and refuses
  typed. The sync `resolve_local_only` is for the write-op oracle alone. Full model + routing-vs-display id split:
  [DETAILS.md](DETAILS.md) § "Routing and lifecycle".

- **Live watch (`watch.rs`): refresh via `refresh_archive_listings` (PARENT DRIVE id + full `/…/foo.zip/inner` path),
  never the archive id or `notify_directory_changed`** — the listing cache keys on the parent and re-resolves. Watches
  the `.zip`'s parent DIRECTORY (survives temp+rename inode swaps); `listing_is_watched` is live-only. Details:
  [DETAILS.md](DETAILS.md) § "Live content watch".

Local zip mutation has landed (`mutator.rs` + the write-ops driver). Still ahead: remote-backed archives, in-place
append (`/docs/specs/archive-browsing-plan.md`; [DETAILS.md](DETAILS.md) § follow-up milestones).
