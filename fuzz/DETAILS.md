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
  compressed pull. Asserts every listed name is one path segment (not empty, `.`, `..`, or holding a `/`) and every
  pulled chunk is at most `MAX_DATA_CHUNK`.
- `adb_lz4`: the frame reader behind a compressed pull, on its own so a mutation doesn't have to survive a stat and a
  listing first; the first byte picks the packet size, and a chunk past `MAX_DATA_CHUNK` panics. Seeds: `liblz4`'s
  linked frame (`crates/cmdr-adb/src/testing/liblz4_frame.bin`), plus stored, independent, checksummed, and 4 MiB-block
  ones. The same bytes then round-trip as plaintext: our `Lz4Writer` encodes them, our `Lz4Reader` decodes them back
  unchanged in packets of the chosen size.
- `adb_shell`: shell v2 frames, bounded and unbounded, then `parse_df_k` on stdout. Asserts a bounded read never returns
  more stdout than its bound, and that a `df -k` answer written from the first 24 bytes (toybox or busybox layout) reads
  back as its own figures.
- `webdav_propfind`: `parse_multistatus` and `every_propstat_succeeded` on the raw body (namespaces, entities, hrefs,
  RFC 3339 and HTTP dates). Then a round trip: a one-file answer written from the input (`join_remote`'s URL echoed
  absolute or path-only, our encoding or a looser one carried by XML entities, a 404 `propstat` with a wrong size before
  the 200 one, dates in any zone) must read back with the same path, kind, size, and dates.
- `s3_xml`: every S3 response parser over one body (`xml::parse_tree` plus the per-document readers). Then two round
  trips: a key sent in `delete_objects_body`, echoed back as S3's `DeleteResult` does, must name the same key, and a
  one-object listing, URL-encoded (as we ask for) and literal, must read back with its key, prefix, and size untouched.
- `nfs_reply`: one reply record through `decode_reply`, then its results through every portmap, MOUNT, and NFSv3 decoder
  (the record doesn't say which call it answers, so all of them); asserts a READ never yields more than `MAX_PAYLOAD`.
  Seeds: the knfsd captures (`crates/cmdr-nfs/src/proto/testdata/knfsd/`, hex to binary, no record mark) plus hand-built
  WRITE and COMMIT answers, which no capture has.
- `archive_index`: `ArchiveIndex::parse` over in-memory zip, tar (plain, gzip, bzip2, xz, zstd), and 7z; walks the tree
  asserting every listed path is safe and consistent, then reads up to 256 KiB from each of the first three files.
- `archive_entry_name`: `sanitize_entry_name` never accepts a name that escapes its root, and accepting is idempotent.
- `pdf`: the app's `pdf-extract` calls (load, page tree, encryption, Info strings, the first three pages) inside
  `catch_unwind`, behind the app's `parent_chain_ends` guard. Crash-only by design: only third-party code runs, and the
  app contains its panics.
- `image_headers`: the `image` dimensions read and EXIF shaping, which the app does NOT contain, so a panic counts.
  Crash-only: only third-party decoders run, with no Cmdr line for a property's mutant to change.

## Running and reproducing

- Everything: `pnpm check fuzz` (`CMDR_FUZZ_SECONDS=600` for a real hunt). Per target, from `fuzz/`:
  `cargo +<nightly> fuzz run -O -a <target> corpus/<target> seeds/<target> -- -max_total_time=120 -timeout=20`, plus
  `target/large-seeds/<target>` and the target's `-max_len` where it has them (the lane unpacks the first and passes the
  second). `./scripts/check/check --print-nightly` prints the nightly.
- **CI's long run** (`slow-checks.yml`, every six days): 20 minutes per target, about 200 minutes in all, under a
  300-minute job timeout. The grown corpus is cached run to run; a target's corpus past 32 MiB (`fuzzCorpusBudget`) is
  minimized with `cargo fuzz cmin` after its run, so the cache stays far below GitHub's 10 GB ceiling.
- A finding lands in `artifacts/<target>/` (CI uploads it as `fuzz-artifacts`). Reproduce with
  `cargo +<nightly> fuzz run -O -a <target> artifacts/<target>/<file>`; minimize with `fuzz tmin`.
- Then: a regression test in the owning crate, red first, then the fix. Copy the input into `seeds/<target>/` once it
  passes, so every run replays it.
- New target: a `fuzzing` seam if the parser is private, a file in `fuzz_targets/`, a `[[bin]]` in `Cargo.toml`, a seed
  dir, and a mutant record per assertion. The lane discovers targets through `cargo fuzz list`.

## Large inputs

libFuzzer caps an input at `-max_len`, which defaults to its largest starting input (4 KiB at least), so with small
seeds no run reaches a bug that needs a large one. Targets whose deep code sits behind size get an explicit cap in
`fuzzTargetMaxLen` (`scripts/check/checks/desktop-rust-fuzz.go`) and large seeds shaped to reach it, gzipped in
`large-seeds/<target>/` (the lane unpacks them to `target/large-seeds/<target>/` and adds that dir to the run):

- `adb_lz4`, 4 MiB + 64 KiB: a stored block at the format's 4 MiB maximum with a content checksum, and 192 KiB of
  plaintext (16 KiB of noise, 12 times) whose round trip writes linked blocks that reach into the block before.
- `archive_index`, 4 MiB + 64 KiB: an uncompressed tar and a stored zip, each a 1 MiB file (past the target's 256 KiB
  read) plus 300 entries over 30 folders.
- `nfs_reply`, 1 MiB + 4 KiB: a READ one byte past `MAX_PAYLOAD`.

libFuzzer still starts short and lengthens inputs as coverage pays (`-len_control`), so a big cap costs little until the
corpus earns it. The 20–64 MB inputs some published finds needed stay out of reach on purpose: at that size a target
runs a handful of inputs a second.

## Mutant records

Each assertion has a one-line change to the code it guards that makes it fail, recorded in
`fuzz_targets/<target>.mutants.json` with the input that catches it; `desktop-rust-property-mutants` applies each in a
mirror of the tree and runs the input through the target once. Format, lanes, and how to add one live in
`scripts/check/checks/DETAILS.md` § "Property mutants". Here:

- **Recorded**: every assertion in every target but `pdf` and `image_headers`, which run only third-party code and sit
  in `scripts/check/checks/property-mutants-allowlist.json` with that reason.
- **Catching inputs are part of the target's corpus**: a seed, or a large seed (`mutant-records` refuses any other
  path). So the mutant run is also the coverage witness: it proves an input every fuzz run starts from reaches the
  property, and that the property bites there, which a line-coverage report alone can't.
- **Each record aims at a bug class the property exists for**: a 4 GiB size read as 32 bits, a lost `&`, a trimmed key,
  an off-by-one February, a `propstat` status read too loosely, a window dropped between LZ4 blocks.
- **In `archive_entry_name`, the root-escape check runs before the per-component one**: the component check also fails
  on every `..` and leading `/`, so run second, the escape check could never fire and no mutant could reach it. The
  component check owns the `.` and empty components `Path` normalizes away.

## Findings

- **ADB listing names, ours, fixed**: `push_entry` dropped only `.` and `..`, so a device could list an empty name or
  one holding a `/`, which the volume would join into a path naming another file. Now dropped like NFS does
  (`a_listed_name_that_isnt_one_path_segment_is_dropped`). Found while writing `adb_sync`'s listing property.
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

- **SMB**: the protocol layer is the `smb2` crate, which has its own 18 targets and a weekly fuzz workflow.
- **MTP**: `mtp-rs` parses PTP and fuzzes every device-facing parser in its own `crates/mtp-rs/fuzz/` (10 targets,
  weekly workflow). Both repos keep their targets single-threaded and input-only so they qualify for OSS Verify's
  proofs.
- **SFTP**: `russh` / `russh-sftp` parse the wire; Cmdr's SFTP code maps typed results. `known_hosts.rs` reads the
  user's own file, not an attacker's.
- **Git**: `gix` parses repos and fuzzes upstream.
- **The ADB host messages** (`host:devices-l`, features): split-and-trim over a 16-bit-length message; nothing to find.
- **Thumbnails**: the native ImageIO / Vision backend in `cmdr-index`, not Rust code.
