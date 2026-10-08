# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, with **no `tauri` in its dependency tree**,
so the index reaches `Volume` and `FileEntry` without reaching the app. App code uses the re-exports at their original
paths (`crate::file_system::volume::VolumeError`); other crates use `cmdr_fs::…`.

## Module map

- `volume/`: the `Volume` trait and its types, the ID funnel, the copy-scan seam, the bodies a stat-and-listing backend
  gets for free, `InMemoryVolume`, `host/` (read `src/volume/host/CLAUDE.md` before writing a backend), and
  `connection.rs`'s four remote types, whose header says which answers what; ❗ confusing any two is a bug.
- Around it: `entry.rs` + `icons/`, `sqlite_util.rs`, `staging.rs`, `archive_format.rs`, `firmlinks.rs`, and a dozen
  small leaves. Per-module responsibilities: DETAILS § Module map.

## Must-knows

- **`#![deny(missing_docs)]` holds here**, and several types reach `bindings.ts` through `specta::Type`, which is why
  `Cargo.toml` pins `specta` to the app's exact version.
- **`Volume::capabilities()` is a PURE FOLD of the trait's predicates, published over IPC.** ❌ Never override it: add a
  predicate instead (`src/volume/capabilities.rs`).
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy. DETAILS § What the app kept.
- **❌ Never gate BEHAVIOR on `cfg(test)` here; use `any(test, feature = "testing")`.** `cfg(test)` is off in a
  consumer's test build, so production behavior runs inside their suite and surfaces as someone else's flake.
- **`InMemoryVolume` is the oracle for the `Volume` contracts data safety leans on**, and LIES on request
  (`set_stat_failing`, `with_delete_failing`, …) so a defense against a hostile backend is testable. ❌ Never relax a
  contract to green a test. Cross-backend promises live in `volume::conformance`.
- **Stream length is explicit**: real files use `StreamLength::Known`; generated output may use `Unknown`. A backend
  that has not opted into unknown writes must reject before opening the destination or polling the source.
- **Atomic replacement is explicit**: `supports_atomic_replace_rename` means force-rename is one replacement step.
  Delete-then-rename backends keep the conservative default and need tracked aside/restore publication.
- **❌ Never build a volume ID by hand, or by stripping characters.** `volume::ids` is the one funnel; an ID keys the
  index DB, `lastUsedPaths`, tab state, and routing, so a lossy one hands two disks one identity and sends deletes to
  the wrong one.
- **❌ Never open SQLite outside `sqlite_util`'s factories.** They install the process-wide page-cache slab, installable
  only before the process's first connection (`desktop-rust-sqlite-open-direct` enforces it). ❌ Nor unlink a database
  by hand: `delete_database` retires threads' cached connections.
- **Nothing here produces user-facing prose**: errors carry typed reasons and structured params, the frontend renders
  every word, and `FileEntry.git_meta` states a FACT, ❌ never a sentence. DETAILS § The one place prose is produced
  here.
- **A test needing a real removable volume uses `testing::disk_images` (macOS), ❌ never its own `hdiutil` or `diskutil`
  call.** Its runner holds the machine-wide lock, SIGKILLs a stuck tool, and proves a disk is the test's own before
  touching it. ❌ Never a physical disk or a new FAT/exFAT image. DETAILS § "`testing::disk_images`".
- **The global allocator is decided here** (`process_memory/allocator.rs`: system on macOS, mimalloc on Linux or with
  the `mimalloc` feature). Gate allocator code on the `cmdr_mimalloc` cfg, ❌ never `feature = "mimalloc"` (false on
  Linux). DETAILS § Which global allocator.
- **A stat-and-listing backend implements three small traits, ❌ never its own copy of the walk**: `ScanSource`,
  `MakesDirectories`, `PatchSource` (DETAILS § Bodies a backend gets for free). `secret_store.rs` is a backend's only
  door to the credential store.

Composition rationale, the boundary decisions, and what deliberately stayed in the app: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
