# Fuzz targets: details

## Why this crate looks the way it does

- **One shared crate, not a `fuzz/` per crate.** Ten targets across five crates plus three app decoders share one
  instrumented build (one `target/`, one lock, one CI cache). Per-crate dirs would build the common dependency graph
  (tokio, quick-xml, the codecs) once per crate.
- **Outside the workspace** (`[workspace]` in its own `Cargo.toml`): the sanitizer build needs nightly flags no other
  lane should see, and cargo-fuzz wants its own lock and target dir. The lane holds no lock on the shared `target/`.
- **`libfuzzer-sys` only, no `arbitrary`**: every target reads raw bytes, which is what the attacker sends. A target
  that needs structure takes it from leading bytes (`adb_sync`'s feature byte, `archive_index`'s format byte).
- **Seams behind a `fuzzing` feature** where the parser is `pub(crate)`. The seam adds no logic: it calls the parser the
  way the transport does (lossy UTF-8 for XML bodies, a scripted `AdbConnection` for the ADB wire).
- **Versions** (verified on crates.io and GitHub, 2026-10-05): `libfuzzer-sys` 0.4.13 (2026-06-04), cargo-fuzz 0.13.2
  (2026-06-09, pinned in `desktop-rust-fuzz.go`), the nightly shared with cargo-udeps.

## Targets

- `adb_sync`: `SyncSession` stat, listing, and pull against device-chosen bytes; first byte picks v1/v2 verbs and the
  compressed pull.
- `adb_lz4`: the frame reader behind a compressed pull, on its own so a mutation doesn't have to survive a stat and a
  listing first; the first byte picks the packet size, and a chunk past `MAX_DATA_CHUNK` panics. Seeds: `liblz4`'s
  linked frame (`crates/cmdr-adb/src/testing/liblz4_frame.bin`), plus stored, independent, checksummed, and 4 MiB-block
  ones.
- `adb_shell`: shell v2 frames, bounded and unbounded, then `parse_df_k` on stdout.
- `webdav_propfind`: `parse_multistatus` (namespaces, entities, hrefs, RFC 3339 and HTTP dates).
- `s3_xml`: every S3 response parser over one body (`xml::parse_tree` plus the per-document readers).
- `nfs_reply`: one reply record through `decode_reply`, then its results through every portmap, MOUNT, and NFSv3 decoder
  (the record doesn't say which call it answers, so all of them); asserts a READ never yields more than `MAX_PAYLOAD`.
  Seeds: the knfsd captures (`crates/cmdr-nfs/src/proto/testdata/knfsd/`, hex to binary, no record mark) plus hand-built
  WRITE and COMMIT answers, which no capture has.
- `archive_index`: `ArchiveIndex::parse` over in-memory zip, tar (plain, gzip, bzip2, xz, zstd), and 7z; walks the tree
  asserting every listed path is safe and consistent, then reads up to 256 KiB from each of the first three files.
- `archive_entry_name`: `sanitize_entry_name` never accepts a name that escapes its root, and accepting is idempotent.
- `pdf`: the app's `pdf-extract` calls (load, page tree, encryption, Info strings, the first three pages) inside
  `catch_unwind`, behind the app's `parent_chain_ends` guard.
- `image_headers`: the `image` dimensions read and EXIF shaping, which the app does NOT contain, so a panic counts.

## Running and reproducing

- Everything: `pnpm check fuzz` (`CMDR_FUZZ_SECONDS=600` for a real hunt). Per target, from `fuzz/`:
  `cargo +<nightly> fuzz run -O -a <target> corpus/<target> seeds/<target> -- -max_total_time=120 -timeout=20`.
  `./scripts/check/check --print-nightly` prints the nightly.
- A finding lands in `artifacts/<target>/` (CI uploads it as `fuzz-artifacts`). Reproduce with
  `cargo +<nightly> fuzz run -O -a <target> artifacts/<target>/<file>`; minimize with `fuzz tmin`.
- Then: a regression test in the owning crate, red first, then the fix. Copy the input into `seeds/<target>/` once it
  passes, so every run replays it.
- New target: a `fuzzing` seam if the parser is private, a file in `fuzz_targets/`, a `[[bin]]` in `Cargo.toml`, a seed
  dir, and a mutant record per assertion. The lane discovers targets through `cargo fuzz list`.

## Mutant records

Each assertion has a one-line change to the code it guards that makes it fail, recorded in
`fuzz_targets/<target>.mutants.json` with the input that catches it; `desktop-rust-property-mutants` applies each in a
mirror of the tree and runs the input through the target once. Format, lanes, and how to add one live in
`scripts/check/checks/DETAILS.md` § "Property mutants". Here:

- **Recorded**: `archive_entry_name` (all five assertions), `archive_index` (both), `nfs_reply`, `adb_lz4`. The six
  crash-only targets sit in `scripts/check/checks/property-mutants-allowlist.json` until they assert something.
- **Catching inputs are seeds where they fit**, so every fuzz run replays them. The `nfs_reply` one is a READ of 1 MiB
  plus one byte: as a seed it would raise the target's inferred `-max_len` to over a megabyte, so it lives gzipped in
  `mutant-inputs/`.
- **In `archive_entry_name`, the root-escape check runs before the per-component one**: the component check also fails
  on every `..` and leading `/`, so run second, the escape check could never fire and no mutant could reach it. The
  component check owns the `.` and empty components `Path` normalizes away.

## Findings

- **ADB, ours, fixed**: a sync payload or shell frame length straight off the wire sized a `Vec` (4 GiB per packet).
  Capped before allocating; `crates/cmdr-adb/DETAILS.md` § The wire contract.
- **PDF, `pdf-extract` 0.12.0, guarded in our wrapper**: a page that is its own `Parent` with no `Resources` recursed
  forever in `get_inherited` (stack overflow, uncontainable). `parent_chain_ends` in `inspect/pdf.rs` skips such pages.
  Worth reporting upstream.
- **xz dictionary, `lzma-rust2`, bounded in our wrapper**: a block header names its LZMA2 dictionary and the decoder
  allocated and zeroed it up front (4 GiB from 40 bytes). `lzma-rust2` 0.21.0 added `XzReader::new_mem_limit`;
  `format::XZ_MEMORY_LIMIT_KIB` sets it (`crates/cmdr-archive/src/read/DETAILS.md` § Resource caps).
- **xz index, fixed upstream in `lzma-rust2` 0.22.0**: the index's record count is a varint the reader passed straight
  to `try_reserve_exact`, so 23 bytes reserved ~3 GiB. David's hasenbanck/lzma-rust2#129 caps the reservation. The input
  is the committed seed `seeds/archive_index/xz-index-huge-count.bin`, and `archive_index` runs under the default
  per-allocation cap, so a regression fails the lane.

## Deliberately not fuzzed here

- **SMB**: the protocol layer is the `smb2` crate, which has its own 14 targets and a weekly fuzz workflow.
- **MTP**: `mtp-rs` parses PTP; it has proptest but no fuzz targets yet. Belongs in that repo.
- **SFTP**: `russh` / `russh-sftp` parse the wire; Cmdr's SFTP code maps typed results. `known_hosts.rs` reads the
  user's own file, not an attacker's.
- **Git**: `gix` parses repos and fuzzes upstream.
- **The ADB host messages** (`host:devices-l`, features): split-and-trim over a 16-bit-length message; nothing to find.
- **Thumbnails**: the native ImageIO / Vision backend in `cmdr-index`, not Rust code.
