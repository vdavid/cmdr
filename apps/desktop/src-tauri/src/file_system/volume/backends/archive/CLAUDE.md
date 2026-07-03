# Archive reading core (zip)

Read-only zip core the `ArchiveVolume` backend is built on: parse a central directory into a synthetic directory tree,
and stream-decompress entries. **Decoupled from the `Volume` trait** — it deals in archive-native types (`ArchiveIndex`,
`ArchiveNode`, `ArchiveError`); the volume layer maps them onto `FileEntry` / `VolumeError` / `VolumeReadStream`.

## Module map

- `source.rs`: `ArchiveByteSource` (byte-supply seam) + `LocalFileSource`, `BytesSource`.
- `index.rs`: `ArchiveIndex` (parsed tree + query surface), the central-directory parse driver, the pure tree builder.
- `name.rs`: `sanitize_entry_name` (Zip Slip defense). `read.rs`: `ArchiveEntryReader`. `cache.rs`: `ArchiveIndexCache`.

Depth, rationale, and the full test list: [DETAILS.md](DETAILS.md). Read it before non-trivial work here.

## Must-knows

- **Zip Slip is enforced at this layer. `sanitize_entry_name` is the single choke point every entry passes before it
  enters the tree; don't bypass it.** Guarantee: no `Accepted` inner path, joined under any root, escapes that root. `..`
  components are quarantined (rejected); absolute paths are clamped to root (leading `/` stripped); `\`→`/`. Pinned by
  `name.rs` tests and `zip_slip_traversal_entry_is_quarantined_not_browsable`. Don't swap in rc-zip's
  `Entry::sanitized_name` — it's a coarser `contains("..")` substring test and skips `\` normalization.

- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`.** Two reasons: its only public entry reader borrows its
  `ArchiveHandle` (can't back an owned, cached stream), and it decompresses on the async executor (we need it off).
  Codec features (deflate/bzip2/lzma/zstd) live on the `rc-zip` dep. See [DETAILS.md](DETAILS.md) § Decision.

- **Decompression runs on `spawn_blocking`, never on the executor; reads are chunked, never whole-entry buffered.**
  `ArchiveEntryReader` is a bounded-channel producer/consumer (≤128 KiB/chunk, capacity 4 ⇒ ~512 KiB peak regardless of
  entry size). Dropping the reader cancels the producer. Don't add a whole-entry `Vec` anywhere in the read path.

- **The byte source is blocking and `pread`-shaped (`ArchiveByteSource`).** `LocalFileSource` backs it now; remote (M5)
  implements the same trait. Shared as `Arc` across concurrent reads — no shared cursor, so parallel reads are
  independent.

- **Encryption: browsing works, extraction doesn't.** Detected from general-purpose flag bit 0 or the AE-x method (NOT
  in `rc_zip::Error`). `open_read` on an encrypted entry returns `ArchiveError::Encrypted`; `has_encrypted_entries()`
  lets the volume layer gate up front.

- **Errors are typed (`no-string-matching`).** `matches!(err, ArchiveError::Corrupt(_))`, never a message substring. Not
  a zip → `NotAnArchive`; truncated/broken CD → `Corrupt`; undecodable method → `Unsupported`. Magic-byte format
  detection (RAR/7z vs zip) is the routing layer's job, not ours.

- **Filename encoding (CP437 vs the often-wrong UTF-8 flag) is rc-zip's job.** We consume the already-decoded UTF-8
  `entry.name`; don't re-decode.

- **The index cache key is `(path, size, mtime)`,** so an external edit auto-invalidates. `index_for_local` is blocking
  — call it from `spawn_blocking`. No eviction here; the volume layer owns archive lifetime and calls `clear()`.

Not here (the `ArchiveVolume` layer's job): the `Volume` impl, capability flags, `scan_for_copy`, registration /
refcount / LRU, the capability-matrix column, and the `docs/architecture.md` line. See [DETAILS.md](DETAILS.md) § Left
for the volume layer.
