# Archive backend (zip)

Two layers in one directory: a **read-only zip core** (central directory → synthetic tree; streaming decompress) and
`ArchiveVolume`, the read-only `Volume` built on it. The core is **decoupled from the
`Volume` trait** — it deals in archive-native types (`ArchiveIndex`, `ArchiveNode`, `ArchiveError`); `volume.rs` is the
one file that maps them onto `FileEntry` / `VolumeError` / `VolumeReadStream`. Keep the core submodules `Volume`-free.

## Module map

- `volume.rs`: `ArchiveVolume` — the read-only `Volume` impl (browse + extract + `scan_for_copy`) over the core below.
- `source.rs`: `ArchiveByteSource` (byte-supply seam) + `LocalFileSource`, `BytesSource`.
- `index.rs`: `ArchiveIndex` (parsed tree + query surface), the central-directory parse driver, the pure tree builder.
- `name.rs`: `sanitize_entry_name` (Zip Slip defense). `read.rs`: `ArchiveEntryReader`. `cache.rs`: `ArchiveIndexCache`.
- `boundary.rs`: the SHARED `.zip`-boundary detector (both `VolumeManager::resolve` and `commands/volumes.rs` use it —
  two detectors would drift).

Depth, rationale, and the full test list: [DETAILS.md](DETAILS.md). Read it before non-trivial work here.

## Must-knows

- **Zip Slip is enforced at this layer. `sanitize_entry_name` is the single choke point every entry passes before it
  enters the tree; don't bypass it.** Guarantee: no `Accepted` inner path, joined under any root, escapes that root. `..`
  components are quarantined (rejected); absolute paths are clamped to root (leading `/` stripped); `\`→`/`. Pinned by
  the `name.rs` tests. Don't swap in rc-zip's `Entry::sanitized_name` — it's a coarser `contains("..")` substring test
  and skips `\` normalization.

- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`**: its only public entry reader borrows its
  `ArchiveHandle` (can't back an owned, cached stream), and it decompresses on the async executor (we need it off).
  Codec features live on the `rc-zip` dep. See [DETAILS.md](DETAILS.md) § Decision.

- **Decompression runs on `spawn_blocking`, never on the executor; reads are chunked, never whole-entry buffered.**
  `ArchiveEntryReader` is a bounded-channel producer/consumer (≤128 KiB/chunk, capacity 4 ⇒ ~512 KiB peak regardless of
  entry size). Dropping the reader cancels the producer. Don't add a whole-entry `Vec` anywhere in the read path.

- **The byte source is blocking and `pread`-shaped (`ArchiveByteSource`).** `LocalFileSource` backs it now; a future
  remote parent implements the same trait. Shared as `Arc` across concurrent reads — no shared cursor, so parallel reads
  are independent.

- **Encryption: browsing works, extraction doesn't.** Detected from GP flag bit 0 or the AE-x method (not in
  `rc_zip::Error`). `open_read` on an encrypted entry returns `Encrypted`; `has_encrypted_entries()` lets the volume
  layer gate up front.

- **Errors are typed (`no-string-matching`).** `matches!(err, ArchiveError::Corrupt(_))`, never a message substring.
  Magic-byte format detection (RAR/7z vs zip) is the routing layer's job, not ours.

- **Filename encoding is rc-zip's job** (CP437 vs the often-wrong UTF-8 flag); consume the decoded `entry.name`, don't
  re-decode.

- **The index cache key is `(path, size, mtime)`,** so an external edit auto-invalidates. `index_for_local` is blocking
  — call it from `spawn_blocking`. No eviction here; the volume layer owns archive lifetime and calls `clear()`.

## `ArchiveVolume` (the `Volume` layer)

- **Read-only until zip mutation lands.** Every mutation returns `NotSupported`, INCLUDING `create_directory_all`
  (overridden — the trait default no-ops to `Ok` on an existing dir, falsely claiming success).
- **`lane_key()` and `get_space_info()` delegate to the PARENT volume, never the archive** — the parent owns the
  serialization lane and the real disk cost; delegating also dodges `available = 0`, which reads as "disk full" and
  blocks paste. Capability flags + the typed `ArchiveError → VolumeError` backstop mapping: [DETAILS.md](DETAILS.md)
  § "The `ArchiveVolume` layer".

- **This layer is headless: it never registers itself.** `VolumeManager::resolve` routes a `.zip`-crossing path here
  (on-demand registration, archive LRU, backend-internal id). Full model + the routing-vs-display id split:
  [DETAILS.md](DETAILS.md) § "Routing and lifecycle".

Still ahead (see `/docs/specs/archive-browsing-plan.md`): the `'archive'` FE `VolumeKind`, live watching, mutation.
See [DETAILS.md](DETAILS.md) § Left for the follow-up milestones.
