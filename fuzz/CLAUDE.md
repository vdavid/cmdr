# Fuzz targets

libFuzzer targets (cargo-fuzz) for the code that parses bytes someone else chose: a phone over ADB, a WebDAV or S3
server, an archive, a PDF, or an image a user opens. Run them with `pnpm check fuzz` (`CMDR_FUZZ_SECONDS`, default 60
per target); CI runs the same lane in `slow-checks.yml`.

## Module map

- `fuzz_targets/`: one file per target, each a thin call into a `fuzzing` module or a public API.
- `seeds/<target>/`: the committed seed corpus. `corpus/` (grown), `artifacts/` (findings), `target/`: gitignored.
- The seams live in the crates: `cmdr_adb::fuzzing`, `cmdr_webdav::fuzzing`, `cmdr_s3::fuzzing`, each behind that
  crate's `fuzzing` feature (on in its dev builds, so clippy lints it).

## Must-knows

- **Outside the workspace, on the pinned nightly** (`nightlyToolchain` in
  `scripts/check/checks/desktop-rust-cargo-udeps.go`). `Cargo.lock` here is its own; it started as a copy of the root
  one, so keep the shared crates' versions in step.
- **Every command passes `-O -a`** (optimized, debug assertions on). Mixing flags rebuilds everything.
- **`pdf` and `image_headers` MIRROR app code** (`agent/tools/read/inspect/pdf.rs`, `exif.rs`, `file_viewer/media.rs`):
  instrumenting the Tauri app isn't worth it. Change those calls, change the target.
- **In `pdf`, a panic is not a finding**: the app contains them. Aborts, OOMs, and hangs are.
- **A finding in OUR code gets a red regression test in the crate, then the fix.** One in a third-party crate gets
  reported upstream, plus a guard in our wrapper when we can bound it.
- **Not under `cargo deny`**: `libfuzzer-sys` is NCSA-licensed and never ships, so it stays out of the root graph.

Targets, seeds, the reproduce-and-fix loop, findings so far, and what's deliberately not fuzzed: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
