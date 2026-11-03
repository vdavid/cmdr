# zip 9.2.0, patched

The crates.io release of `zip` 9.2.0 (upstream `zip-rs/zip2` at tag `v9.2.0`), swapped in through the root
`Cargo.toml`'s `[patch.crates-io]`. Everything here is the published package byte for byte, apart from the change below
and the files left out:

- `Cargo.lock`: a path dependency resolves through the workspace lockfile, so it was never read, and `osv` would scan
  its stale pins as if they shipped.
- `.cargo-ok`, `.config/` (upstream's `nextest.toml`), `.devcontainer/`, `.pre-commit-config.yaml`, `.whitesource`:
  registry and upstream dev tooling, nothing the build reads.

Upstream covers what earlier versions of this patch carried (verified by diffing the 9.0.5 and 9.2.0 packages,
2026-11-03): a raw copy keeps an entry's name bytes and language encoding flag as stored (9.1.0, `zip-rs/zip2` #1124,
Cmdr's #514; tests `crates/cmdr-archive/src/mutation/mutator_raw_names_test.rs`), and `ZipArchive::has_duplicate_names()`
is public (9.1.0, #1109), which `refuse_a_folded_duplicate` in `crates/cmdr-archive/src/mutation/plan.rs` calls.

Upstream's fix for zip cloaking (GHSA-m665-v7pm-7grj: opt-in `ZipReadOptions::enforce_header_consistency` and
`ZipArchive::validate_header_consistency`) is opt-in. Cmdr doesn't opt in, because it never reads by local-header name:
`crates/cmdr-archive/src/read/DETAILS.md` § "Zip cloaking".

`tauri-plugin-updater` 2.14.1 depends on `zip` ^9 too, so this vendored copy is the only `zip` in the graph.

## The change

Encrypted entries (#592), in `src/write.rs` and `src/types.rs`:

- A raw copy of an encrypted entry keeps its encrypted flag (bit 0) and its data descriptor flag (bit 3), carried by
  `ZipRawValues::encrypted_with_descriptor` into `start_entry`, which applies them after its own data descriptor
  default.
- `raw_copy_file_rename_internal` writes a data descriptor after the copied bytes when bit 3 is set (`finish_file` skips
  it for raw entries, as `add_prepared_file` already works around).
- Upstream drops both flags, because `ZipFile::options()` carries no ZipCrypto state, so a kept ZipCrypto entry reads
  back as ciphertext (verified by reading `raw_copy_file_rename_internal` and `start_entry` in the 9.2.0 package,
  2026-11-03). Bit 3 matters on its own: it picks ZipCrypto's password check byte (the DOS time's high byte under it,
  which is how Info-ZIP's `zip -e` writes, else the CRC's).
- An AES entry gets its extra field (`0x9901`) once: upstream's `raw_copy_options` skips the stored one, as `options()`
  already carries it.
- Known limit: `raw_copy_file_touch` on a bit-3 ZipCrypto entry changes the time its check byte encrypts, which no copy
  can fix without the password; Cmdr never touches.
- Tests: `raw_copy_file_keeps_a_zipcrypto_entry_decryptable` and `raw_copy_file_writes_an_aes_entrys_extra_field_once`
  at the end of `src/write.rs`'s `tests` module, and Cmdr's `crates/cmdr-archive/src/mutation/mutator_encrypted_test.rs`.
  The package leaves out upstream's `tests/data/` fixtures, so the vendored crate's own tests don't build here: clone
  `zip-rs/zip2` at the tag (its `src/` equals the package's), copy this `src/` over it, and run `cargo test --tests`
  (passed on 9.2.0 at `4100a7d1`, 196 lib tests plus the integration suites, 2026-11-03).

Known limit, upstream too: a raw copy re-encodes a non-ASCII entry COMMENT as UTF-8 (the reader keeps no raw comment
bytes), so a CP437 comment under a clear flag reads back garbled. Entry comments are rare; names were the data loss.

## Why

Cmdr's zip edit rewrites every kept entry through a raw copy. Without this change, keeping a ZipCrypto entry through any
edit (deleting or renaming a sibling) left it undecryptable. The change isn't drafted for upstream yet.

## Dropping it

When upstream ships a raw copy that keeps an encrypted entry's flags (and its data descriptor), delete this folder, its
`[patch.crates-io]` lines (root `Cargo.toml` and `fuzz/Cargo.toml`), and its `exclude` entry, and keep the tests.
`desktop-rust-vendor-patch-applied` fails as soon as a bump past 9.2.0 makes cargo resolve crates.io again.
