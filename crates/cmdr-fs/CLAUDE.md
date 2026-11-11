# `cmdr-fs`

The filesystem vocabulary and host primitives every layer of Cmdr speaks in, with **no `tauri` in its dependency tree**,
so the index reaches `Volume` and `FileEntry` without reaching the app. App code uses the re-exports at their original
paths (`crate::file_system::volume::VolumeError`); other crates use `cmdr_fs::…`.

## Module map

- `volume/`: the `Volume` trait and its types, the ID funnel, the copy-scan seam, the free backend bodies,
  `InMemoryVolume`, `host/` (read `src/volume/host/CLAUDE.md` before writing a backend), and `connection.rs`'s four
  remote types, whose header says which answers what; ❗ confusing any two is a bug.
- Around it: `entry.rs` + `icons/`, `sqlite_util.rs`, `staging.rs`, `metadata/`, `archive_format.rs`, `firmlinks.rs`,
  `attr_buf.rs` (macOS: THE `getattrlistbulk` record decoder), and a dozen small leaves. Per-module responsibilities:
  DETAILS § Module map.

## Must-knows

- **`#![deny(missing_docs)]` holds here**; `specta` is pinned to the app's exact version for `bindings.ts`.
- **It builds for `wasm32` too**: host-only code is gated `cmdr_host`, POSIX calls `cfg(unix)`, ❌ never
  `not(target_family = "wasm")`. DETAILS § The browser build.
- **`Volume::capabilities()` is a PURE FOLD of the trait's predicates, published over IPC.** ❌ Never override it: add a
  predicate instead (`src/volume/capabilities.rs`).
- **`Volume::notify_mutation` defaults to a no-op.** A new mutable backend must override it or its destination pane goes
  stale after a copy. DETAILS § What the app kept.
- **❌ Never gate BEHAVIOR on `cfg(test)` here; use `any(test, feature = "testing")`.** `cfg(test)` is off in a
  consumer's test build, so production behavior leaks into their suite.
- **`InMemoryVolume` is the oracle for the `Volume` contracts data safety leans on**, and LIES on request so a defense
  against a hostile backend is testable. ❌ Never relax a contract to green a test. Cross-backend promises live in
  `volume::conformance`.
- **Stream length is explicit**: real files use `StreamLength::Known`; generated output may use `Unknown`. A backend
  that has not opted into unknown writes must reject before opening the destination or polling the source.
- **Atomic replacement is explicit**: `supports_atomic_replace_rename` means force-rename is one replacement step.
  Delete-then-rename backends keep the conservative default and need tracked aside/restore publication.
- **❌ Never build a volume ID by hand, or by stripping characters.** `volume::ids` is the one funnel: a lossy ID hands
  two disks one identity and sends deletes to the wrong one.
- **❌ Never open SQLite outside `sqlite_util`'s factories.** They install the process-wide page-cache slab, installable
  only before the process's first connection (`desktop-rust-sqlite-open-direct` enforces it). ❌ Nor unlink a database
  by hand: `delete_database` retires threads' cached connections.
- **Nothing here produces user-facing prose**: errors carry typed reasons and structured params, the frontend renders
  every word, and `FileEntry.git_meta` / `restic_meta` state FACTS, ❌ never a sentence. DETAILS § The one place prose
  is produced here.
- **A test needing a real removable volume uses `testing::disk_images` (macOS), ❌ never its own `hdiutil` or `diskutil`
  call**, a physical disk, or a new image. DETAILS § "`testing::disk_images`".
- **Log a user value as `LogPrivate`, a path-carrying error as `LogError(&e)`.** ❌ Never a `String` of either, and ❌
  no marking `Display`. DETAILS § "Marked log values".
- **The global allocator is decided here** (`process_memory/allocator.rs`: system on macOS, mimalloc on Linux or with
  the `mimalloc` feature). Gate allocator code on the `cmdr_mimalloc` cfg, ❌ never `feature = "mimalloc"` (false on
  Linux). DETAILS § Which global allocator.
- **A stat-and-listing backend implements three small traits, ❌ never its own copy of the walk**: `ScanSource`,
  `MakesDirectories`, `PatchSource` (DETAILS § Bodies a backend gets for free). `secret_store.rs` is a backend's only
  door to the credential store.

Composition rationale, the boundary decisions, and what deliberately stayed in the app: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
