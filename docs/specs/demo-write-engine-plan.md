# Demo write engine plan

Issues: [#929](https://github.com/getcmdr/cmdr/issues/929) (file operations in the browser demo through the real write
engine), part of [#302](https://github.com/getcmdr/cmdr/issues/302) round two, steps 3 and 4 (file operations, then the
transfer queue window). Plan written 2026-11-04 against `49aad7f793`.

**Goal.** In getcmdr.com/demo, F5 copy, F6 move, Shift+F6 rename, F7 new folder, F8 delete, and Cmd+Z undo run through
`crates/cmdr-operations` and `crates/cmdr-operation-log` compiled into the demo's WASM engine, on the same
`InMemoryVolume` the listing engine reads, and the transfer queue window opens on the same backend. The desktop build
behaves byte-identically throughout.

## Status and decisions for David

David's calls (2026-11-04): yes to the local-disk facade in `cmdr-operations`; undo keeps the real `cmdr-operation-log`,
with an LLVM clang in the toolchain (mise-managed, never brew, working locally, in CI, and in the website-demo image
build) so SQLite compiles for `wasm32-unknown-unknown`.

Decided (2026-11-04, after review round 1):

1. **Toolchain**: the Go build script pins `conda:clang@23.1.4` and `conda:llvm-tools@23.1.4` (Renovate tracks the pin),
   installs and resolves them with a PRIVATE `MISE_DATA_DIR` / `MISE_CACHE_DIR` under `~/.cache/cmdr/wasm-toolchain/`,
   and hands cargo only absolute `CC_wasm32_unknown_unknown` / `AR_wasm32_unknown_unknown`. CI caches that dir keyed on
   the pin. ❌ Never in `.mise.toml` or the default mise data dir: the conda env ships its own `cc`, `cpp`, `ar`, and
   `ld`, and its shims sit ahead of `/usr/bin`.
2. **Trash**: permanent delete with the app's own confirm, exactly as on an SMB mount. The `Volume` trait has only
   `has_trash()` (default `false`; only `LocalPosixVolume` overrides it), so the demo's `supportsTrash` comes from the
   engine's `has_trash()`, ❌ not the hardcoded `false` in `lib/demo/volumes.ts`.
3. **Throughput**: ❌ no change to the engine's `Pacing` seam or to E2E timing. The in-memory volume models device speed
   itself: a bandwidth property on its read stream, sleeping through `rt::time::sleep`, like a real slow disk. Verified:
   a demo copy streams (`InMemoryVolume` keeps `supports_copy_within() == false`, so `may_copy_on_server_from` says no
   and every file goes through `open_read_stream` → `write_from_stream`). A same-volume move is a rename (instant, as on
   a real disk), and so is delete. Milestone 2b makes that bandwidth meaningful: demo files carry synthetic content, and
   every in-memory call yields to the page like a device waiting on I/O.
4. **Archive edits** (copy into a zip, compress, move out of one) refuse cleanly with the routed read-only answer,
   before anything touches a scratch dir.
5. **"Which volume id is the local disk" is a HOST answer**, ❌ not `cfg`: natively `Some("root")`, the demo host
   `None`. The local engine bodies are still `cfg`-gated out of the wasm build so the browser can't reach them by
   mistake.
6. **Crate rename** (`cmdr-listing-wasm` → `cmdr-demo-wasm`): a separate commit after all three round-two efforts land,
   ❌ not in this plan's milestones.

## Spike result (the evidence the toolchain decision rests on)

Verified 2026-11-04 on macOS 27 arm64 (and Linux aarch64 in `ubuntu:24.04` Docker), rustc from `rust-toolchain.toml`,
`rusqlite` 0.41.1, `libsqlite3-sys` 0.39.0, `sqlite-wasm-rs` 0.5.5 (already in `Cargo.lock`):

- **No precompiled option exists**: `sqlite-wasm-rs` 0.5.5 has one feature (`sqlite3mc`) and always compiles `sqlite3.c`
  plus a small musl-derived libc shim with `cc`. Apple clang can't target wasm32.
- **`mise exec conda:clang@23.1.4 conda:llvm-tools@23.1.4`** installs in about 7 s (macOS), lists `wasm32` in
  `-print-targets`, compiles and archives a wasm object. Same on Linux (aarch64; conda-forge builds the same recipe for
  x86_64). Sizes: macOS 479 MB + 292 MB, Linux 1.2 GB + 332 MB. No clang config file applies to the wasm target (`-v`
  shows only the resource-dir include).
- **The real `cmdr-operation-log` builds for wasm32** once `cmdr_fs::sqlite_util` is ungated for wasm (and `rusqlite`
  added to `cmdr-fs`'s wasm deps): zero errors, `sqlite3_config` included.
- **It runs in Node** (a throwaway `wasm-bindgen --target web` crate, removed after): rusqlite in-memory with a custom
  collation (SQLite 3.53.0); `ensure_shared_page_cache` answers `TooLate` when a connection opened first (so it must
  stay first on wasm too); `OperationLogStore::open` runs the whole migration ladder on a PATH
  (`/demo/operation-log.db`, `sqlite-wasm-rs`'s default memory VFS); a second connection to the same path sees the
  tables and rows; `PRAGMA journal_mode=WAL` quietly stays `delete` (no shm in memvfs); `new_operation_id()` (UUIDv7
  through `uuid`'s `js` feature) returns a correct current-time id.
- **What traps**: `OperationLogWriter::spawn` returns `Err(Io: operation not supported on this platform)` (no threads),
  and `SystemTime::now()` (what `now_secs` reads) aborts with `RuntimeError: unreachable`.
- **Size**: the spike module (SQLite + op log + `cmdr-fs`) was 1.2 MB after `wasm-bindgen`; today's listing module is
  2.5 MB raw. Expect the combined engine around 5–6 MB raw; the host milestone measures it.
- ❗ **Why the private data dir**: installing the conda clang into the default mise data dir created shims named `cc`,
  `cpp`, `clang`, `clang-cpp`, and `llvm-ar` in `~/.local/share/mise/shims`, ahead of `/usr/bin` on David's PATH. They
  fell through to Apple's compiler while no config named the tool, one `mise use conda:clang` away from switching every
  native C build. The spike's install was removed from the default dir (`mise uninstall`, `mise reshim`; `which cc` is
  `/usr/bin/cc` again). Installing with `MISE_DATA_DIR=~/.cache/cmdr/wasm-toolchain/data` and
  `MISE_CACHE_DIR=~/.cache/cmdr/wasm-toolchain/cache` puts its shims in that private dir only, and `mise where` answers
  the absolute path (verified 2026-11-04).

## Parallel efforts

- **The viewer crate and demo viewer** (`docs/specs/viewer-crate-plan.md` on its own branch) consume this plan's
  `InMemoryVolume` content model (milestone 2b): stored real bytes per entry, synthetic bytes as a pure function of
  `(seed, offset)`, and unpaced positioned reads (`open_random_reader`) plus an unpaced media-serve path. This effort
  OWNS the content model and its pacing; the viewer effort owns WHICH demo files get real stored bytes and touches
  `crates/cmdr-listing-wasm/src/tree.rs` only after 2b has landed on its branch (through the lead). It also owns making
  demo events reach the frontend asynchronously (a `lib/demo` backend change with a Vitest pin); the lead says when
  that's done, and this effort's frontend milestone builds on it rather than duplicating it.
- **The demo menu bar** (round two's step 2) shares `lib/demo/`; coordinate edits there through the lead.

## Loud rules (every milestone)

- ❗ **Read first**: `crates/cmdr-operations/CLAUDE.md`, `src/host/CLAUDE.md` + `DETAILS.md`, `src/transfer/CLAUDE.md`,
  `src/transfer/volume/CLAUDE.md`, `crates/cmdr-operation-log/CLAUDE.md`, `crates/cmdr-fs/CLAUDE.md` (and its DETAILS §
  "The browser build"), `crates/cmdr-listing-wasm/CLAUDE.md` + `DETAILS.md`, `apps/desktop/src/lib/demo/CLAUDE.md`. Line
  refs drift: trust the code, keep the intent.
- ❗ **The desktop build is byte-identical in behavior.** Every wasm difference lives behind
  `cfg(target_family = "wasm")` or a seam the desktop answers exactly as today. Natively, a seam is a re-export or a
  one-line forward. ❌ No `cfg(test)`-only behavior; `any(test, feature = "testing")` as the crates already require.
- ❗ **Data safety outranks the demo.** No conversion may weaken a stage-and-land, a rollback recheck, a ledger record,
  or a conflict prompt on ANY target. A browser path that can't honor a guarantee refuses with a typed error.
- ❗ **Characterize, then convert.** Pins of CURRENT behavior land as their own commit before the rewrite they guard,
  and each timing-sensitive pin is seen RED once (break the code, watch it fail) before it counts.
- ❌ **Two clocks, never conflated.** `std::time::Instant::now` → `rt::now()`; `tokio::time::Instant::now` →
  `rt::time::now()`. Swapping them silently moves a site off (or onto) tokio's paused test clock: a `start_paused` cell
  then passes vacuously or hangs.
- ❌ **No ambient runtime on any target.** Spawns go through `host.runtime()` (already enforced); clocks and timers
  through `cmdr_fs::rt` (enforced by the clippy bans this plan adds, the same list `crates/cmdr-listing/clippy.toml`
  carries).
- ❌ **No string matching** on errors (`error-string-match`, `cmdr/no-error-string-match`); a "no local disk here"
  answer is a typed variant.
- ❌ **No logic in TS for the demo** (`apps/desktop/src/lib/demo/CLAUDE.md`): every write answer comes from the engine;
  no `if (demo)` in app code (`cmdr/demo-isolation`).
- ❌ **No conda clang on PATH or in the default mise data dir**, anywhere (decision 1): only the private
  `MISE_DATA_DIR`, and only absolute paths into `CC_wasm32_unknown_unknown` / `AR_wasm32_unknown_unknown`.
- ❌ **No change to `Pacing`, its answers, or E2E timing.** Demo throughput is the in-memory volume's own bandwidth.
- ❌ **No milestone tags** in code, comments, test names, `reason` strings, or docs: run
  `rg -n '\b(M[0-9]+[a-z]?|Milestone\s*[0-9]|Phase\s*[0-9])\b' <touched paths>` before reporting.
- ❌ **Never translate** new strings; English plus an `@key` description. Any new UI copy is a draft for David.
- **Gates per milestone**: `pnpm check --fast && pnpm check svelte-tests-related`, `pnpm check clippy`, the touched Rust
  tests via `cargo nextest run`, the wasm lane (`pnpm check desktop-rust-listing-wasm`), and from milestone 7b on the
  website's demo Playwright smoke. Foreground commands only.

## Survey of the current code (2026-11-04, `49aad7f793`)

Method: `rg` over `crates/cmdr-operations/src` excluding `*_test.rs`, `*_tests.rs`, `tests/`, `testing/`; a few inline
test modules still count (lower-bound noise of a few percent). Production: 250 files, about 89,800 lines (524 `.rs`
files with tests). `cmdr-operation-log`: 27 files, about 7,900 lines.

### What fails to COMPILE for wasm32 today

- **First wall**: `tokio`'s `fs` feature (only `archive_edit/remote.rs` uses `tokio::fs`): "Only features
  sync,macros,io-util,rt,time are supported on wasm".
- **With host deps target-gated** (a throwaway edit: `cmdr-local` and tokio `fs` moved under
  `cfg(not(target_family = "wasm"))`): 43 errors in 25 files. A floor: rustc stops at name resolution before type
  checking, so type errors (a tokio `JoinSet::spawn_on` handed an `rt::Runtime`, `VolumeHost::runtime()` already
  "expected `Handle`, found `Runtime`" in `host/mod.rs`) follow once those clear. Kinds: `std::os::unix` extensions
  (`MetadataExt::dev`/`ino`, `PermissionsExt::mode`, `DirBuilderExt`, `OpenOptionsExt`, `symlink`), `libc` (errnos,
  `statvfs`, `access`, `lstat`), and `cmdr_local` items. Heaviest: `durability.rs` (7),
  `transfer/copy/folder_metadata.rs` (6), then `mount_boundary.rs`, `scan_walker.rs`, `scan_dry_run.rs`,
  `move_op/source_sweep.rs`, `types/errors.rs`, `chunked_copy.rs` (2 each).
- `cmdr-operation-log`: compiles once `cmdr_fs::sqlite_util` is ungated (spike).

### Local-disk call sites (the facade's scope)

- `cmdr_local::`: 24 hits in 20 files (about 15 production sites: `rename_local_exclusive` ×3, `LocalPosixVolume` ×5,
  mount tables ×3, `filesystem_kind` ×2, `space`, `mutation::patch_listing_after_local_mutation`, `rename_volume_error`,
  `linux_mounts` ×2).
- `libc::`: 45 hits in 11 files: errno classification (`error_classification.rs`, `types/errors.rs`, `durability.rs`,
  `validation.rs`), syscalls (`statvfs`, `statfs`, `access`, `lstat`, `geteuid`, `copy_file_range`, `posix_fallocate`).
- `std::os::unix`: 34 hits in 19 files. `std::fs` on local paths: 410 hits in 68 files (most in the local engine:
  `macos_copy.rs`, `chunked_copy.rs`, `linux_copy.rs`, `copy_strategy.rs`, `overwrite.rs`, `validation.rs`, `move_op/`,
  `delete/walker.rs`, `cloud_trash.rs`).
- OS integrations: `objc2` (`delete/trash.rs`), `trash` crate (Linux), `xattr` (2 files), `dirs::home_dir` (2 files),
  `walkdir` (2 files), `tokio::fs` (1).
- **The `"root"` literal**: 54 production hits. Most fill a default id (`unwrap_or("root")`), which is harmless. About
  12 DECIDE "this is the local disk, take the `std::fs` engine": `rename.rs` (3), `rename/validity.rs`,
  `rename/bulk.rs`, `rename/bulk/by_move.rs`, `local_start.rs` (`delete_files_start`), `create.rs:355`, and the four
  archive-edit `settle_volume` sites. ❗ **The demo's volume id IS `"root"`** (`apps/desktop/src/lib/demo/demo-tree.ts`
  `DEMO_VOLUME_ID`), so on wasm every one of those must answer "not the local disk", or a demo delete walks `std::fs`.
  The desktop command layer decides the same thing (`write_ops.rs` `is_local`), but the demo host doesn't run it.
- **Keep `"root"` for the demo; renaming it would reroute the app's own UI.** The frontend reads `'root'` as the local
  disk in many places: `transfer-dispatch.ts` `isVolumeMove` is false for root to root (so a demo F6 calls `moveFiles`),
  `TransferDialog.svelte` `isSameVolumeMove` excludes root (so the deep scan preview keeps running and the move consumes
  its `preview_id`), `path-navigation.ts` (`~` is the landing folder), `listing-loader.ts` (falls back to `'root'`),
  `explorer-state.svelte.ts` (the initial tab). Another id would need `if (demo)` checks.
- Other local-only traps the facade's gating covers: `scratch_dir.rs` (`std::env::temp_dir()` panics on
  `wasm32-unknown-unknown`, so archive entry points must refuse BEFORE `ScratchDir::new`), `fresh_zip.rs`'s
  `blocking_send`, and `delete/real_image.rs`'s `process::id` (a test-image helper).
- What routes the demo onto the volume engine already: `copy_between_volumes` takes the volume path unless both volumes
  report `supports_local_fs_access()` (`InMemoryVolume` says no unless built `with_local_fs_access()`); rename and the
  rename pre-flight check the same predicate beside their `"root"` test.

### Clock and timer reads (what traps at RUNTIME in a browser)

- `Instant::now`: 107 hits in 42 files; `.elapsed()`: 46 in 25 files (`volume_outage.rs` 13, `transfer_probe.rs` 7,
  `progress.rs`, `retry.rs`, `scan_watchdog.rs`, `operation_intent.rs`, `human_wait.rs`, `volume_walker.rs`,
  `conflict_slot.rs` 6 each, …). Both clocks occur; see the loud rule.
- `tokio::time::`: 43 hits in 14 files (`copy_concurrent.rs` 6, `leftover_cleanup.rs` 6, `retry.rs` 5,
  `operation_intent.rs` 5, `volume_outage.rs` 4, …). With no tokio runtime in the browser these panic ("no reactor").
- `SystemTime::now`: `journal.rs`, `status_cache.rs`, `archive_edit/remote.rs`, `transfer/volume/cleanup.rs`,
  `archive_edit/fresh_zip.rs`, and the log's `now_secs` (`cmdr-operation-log/src/lib.rs`). Plus `SystemTime` as a value
  type in 13 files (fine).
- `cmdr_fs::deadline`: 15 tokio spawn/time sites; the engine reaches it from `conflict_preflight.rs`
  (`timeout_detached_within`, on the volume path) and `mutation_reply.rs` (app IPC helper; the demo host won't call it).
- `cmdr-operation-log`: 2 `tokio::time` (retention tick, rollback pacing), 1 `SystemTime::now`.
- `rt::` seam: 0 uses in either crate today.
- **`cmdr-fs` itself**: `InMemoryVolume`'s read stream calls `tokio::time::sleep` directly for its chunk delay
  (`in_memory/volume_impl.rs`, the `with_read_chunk_delay` test hook), which panics in the browser the moment a delay is
  set, and `cmdr-fs` has no `clippy.toml`, so nothing stops a new direct clock read there. (`InMemoryVolume::now_secs`
  already reads `web_time` in the browser.)
- **Checked and fine**: the pause gate's async half waits on tokio `Notify` (`operation_intent.rs`), which works in the
  browser, and `wake()`'s `Condvar::notify_all` is harmless there; `claim_ledger(None)` starts no writer or sweep
  (`in_flight_claim.rs`); `Uuid::new_v4` (`overwrite.rs`) gets `js` through `cmdr-fs`'s wasm deps by feature
  unification; `retry.rs`'s ambient spawns are test-only.

### Executors, threads, and blocking

- `spawn_blocking`: 16 code sites in 15 files, all on `host.runtime()` except `delete/real_image.rs` (test-image
  helper). In the browser `rt::Runtime::spawn_blocking` runs the closure INLINE. On the volume path only
  `locked_sources.rs:367` (a local `LockedScan`) and the archive ones appear; the rest are local-engine bodies
  (`local_start.rs`, `rename.rs` ×2, `bulk.rs`, `mount_boundary.rs`, `unblocked_removal.rs`) or the leftover sweep.
- `block_on`: 3 code sites, all `in_flight_sweep.rs` (the leftover sweep, local ledger records). No `block_on` exists in
  the browser runtime.
- Named `std::thread::Builder` threads: 7 in the engine (`cancellable.rs` ×2, `in_flight_claim.rs` sweep,
  `in_flight_log_writer.rs` ledger writer, `scan_preview.rs` local preview, `scan_preview/local.rs`, `fresh_zip.rs`)
  plus the op log's writer (`writer.rs`). `cancellable::spawn_named` and the local scan preview `.expect` a thread (a
  panic in the browser). `thread::sleep`: 9 in 7 files (local engine and archive pacing).
- Blocking waits: `blocking_recv` ×6 (`conflict.rs`, `refused_name::ask_blocking`, `locked_sources::plan_locked_local`,
  `queued_sources::kept_after_queue_local`, `archive_edit/conflicts.rs`, `fresh_zip.rs`), std `mpsc` (6 files),
  `Condvar` (the pause gate in `operation_intent.rs` + `sync_primitives`). Every one sits on a LOCAL or ARCHIVE path:
  the engine already keeps an `async` twin for the volume path (`ask_async`, `plan_locked` async, …).
- The op log: one writer thread on a bounded `sync_channel`; `record_items` BLOCKS under backpressure;
  `finalize_operation` and `flush_blocking` wait on a reply.
- tokio task types: `JoinSet` in `transfer/volume/copy_concurrent.rs` (`spawn_on(&runtime)`), taken for 3+ sources when
  `transfer_concurrency > 1`. `InMemoryVolume::max_concurrent_ops()` is 32, so a demo copy of three files takes it.
  `tokio::task::yield_now` in `checkpoint_stream.rs` (with no runtime it wakes at once, so a long in-memory copy would
  never yield to the page's event loop: no paint, no input until it ends).
- The scan preview: `start_scan_preview` spawns on the runtime for `Some(volume)` and on a named thread for `None`
  (local). The desktop's `scan_preview_source_volume` passes `None` for `"root"`; the demo host must pass the volume.

### Throughput and pacing

- `Pacing::copy_throttle_for` is read only in the LOCAL copy loop (`transfer/copy/mod.rs`, a `std::thread::sleep`).
  Nothing under `transfer/volume/` reads `pacing()`; on the volume path only delete batches (`delete/volume_walker.rs`),
  the scan preview (`scan_preview/volume.rs`), and rollback (`rollback.rs`, `state/wind_down.rs`) are paced. A new
  volume-copy pacing site would also slow the desktop's E2E volume copies (`operations_host::pacing` over `test_mode`),
  which is why decision 3 models speed on the volume instead.
- A demo copy streams: `InMemoryVolume` doesn't override `supports_copy_within()` (default `false`), so the trait's
  `may_copy_on_server_from` answers no and every file goes `open_read_stream` → `write_from_stream`, chunk by chunk,
  with progress per chunk. A bandwidth on the read stream therefore paces the bar honestly.

### The listing side

- `InMemoryVolume` keeps the trait's default no-op `notify_mutation`, so a write on it today patches no pane. Every
  mutable backend patches through `cmdr_fs::volume::patch_mutation` (a `PatchSource`) into `VolumeHost::listings()`; the
  demo needs the same, plus a `ListingHost` answered by the demo's `cmdr_listing::Listings` (the desktop's is
  `listing_host/backends.rs::AppListings`; the engine's test one is `host::testing::PaneListings`).

### The demo frontend

- `lib/demo/writes.ts` refuses 11 write commands; `volumes.ts` reports `mountIsReadOnly: true`; `windows.ts`
  `OPENABLE = {settings, shortcuts}`.
- The desktop's write-facing commands the frontend calls (camelCase on the wire): `copyBetweenVolumes`,
  `moveBetweenVolumes`, `moveFiles`, `deleteFiles`, `trashFiles`, `renameFile`, `checkRenamePermission`,
  `checkRenameValidity`, `createDirectory`, `createFile`, `moveToTrash`, `startScanPreview`, `cancelScanPreview`,
  `checkScanPreviewStatus`, `scanVolumeForConflicts`, `destinationWriteAccess`, `resolveWriteConflict`,
  `continueWithoutGoneSources`, `answerLockedSources`, `cancelWriteOperation`; queue: `listOperations`,
  `latestOperationProgress`, `getTransferTreeView`, `releaseTransferTree`, `cancelOperation(s)`,
  `pause/resumeOperation`, `pause/resumeAll`, `moveQueuedOperation`, `placeQueuedOperation`, `dismissFailedOperation`,
  `retryFailedOperation`, `dismissAllFailedOperations`; undo: `undoOperations`, `rollbackOperation`,
  `getRecentOperationLogEntries`, `getOperationLogDetail`. The host milestone re-derives the exact set by driving the
  demo and reading `window.__cmdrDemo.unanswered`, ❌ not from this list.

## Milestones

Sequential, one agent each, in this worktree. Sizes: S < half a day, M about a day, L more.

### 1. SQLite builds for the browser (S)

- **Scope**: the toolchain (decision 1), `cmdr_fs::sqlite_util` for wasm, and a wasm compile of `cmdr-operation-log` in
  the check lane.
- **Intentions**:
  - The Go build script (`scripts/build-listing-wasm/main.go`) pins `conda:clang` and `conda:llvm-tools`, runs
    `mise install` and `mise where` for exactly those with `MISE_DATA_DIR` / `MISE_CACHE_DIR` set to
    `~/.cache/cmdr/wasm-toolchain/{data,cache}`, and passes ONLY `CC_wasm32_unknown_unknown` /
    `AR_wasm32_unknown_unknown` (absolute) to cargo. Its tests pin that it never touches PATH and never runs mise
    against the default data dir. Renovate tracks the pin with a regex manager (`docs/tooling/` has the Renovate notes).
  - `cmdr-fs`: `rusqlite` joins the wasm deps with the same features; `sqlite_util` is ungated; its doc says the slab
    must still be installed before the first open (the spike's `TooLate`).
  - The check (`desktop-rust-listing-wasm`) also builds `cmdr-operation-log` for wasm32 (compile only for now).
  - CI: every job that runs the wasm build (Desktop Rust's wasm step, `.github/actions/build-website-demo`,
    `publish-website-demo`) gets the toolchain through the script itself; nothing adds it to `mise install`'s config.
    Cache `~/.cache/cmdr/wasm-toolchain/` keyed on the pin (about 1.5 GB on Linux), restored in BOTH places that build
    wasm: the `desktop-rust` job's wasm step and `.github/actions/build-website-demo` (which `publish-website-demo` runs
    only on its rebuild path). It counts against GitHub's 10 GB per-repo cache beside the Rust caches (`desktop-rust`
    alone is about 1.8 GB): check eviction pressure (`gh cache list`) before and after, and prefer caching only the
    `installs/` subdir if the download cache isn't needed for a restore.
  - The website-demo IMAGE (`apps/website/demo-image.Dockerfile`, `FROM scratch`) only copies the built bundle, so no
    Docker-side toolchain work exists; "the image build" means the CI jobs above.
  - Docs: `crates/cmdr-listing-wasm/DETAILS.md` § The build, `crates/cmdr-fs/DETAILS.md` § The browser build, and a
    one-line `AGENTS.md`-level pointer only if the toolchain needs one (prefer `scripts/` docs).
- **Landmines**: conda's `cc`/`ar`/`ld` on PATH or its shims in the default mise data dir (decision 1).
  `website-demo-key.sh` hashes `scripts/build-listing-wasm`, so the image key changes (expected). The listing module now
  links SQLite (+ about 1 MB) once the engines merge; this milestone doesn't link it yet.
- **Test plan**: the Go script's unit tests (env built, PATH untouched); the wasm lane green; a native
  `pnpm check clippy` and `cargo nextest run -p cmdr-fs -p cmdr-operation-log` unchanged; after a build, `which cc` is
  still `/usr/bin/cc` and `~/.local/share/mise/shims` holds no `cc`, `clang`, or `llvm-ar`. ❗ The spike proved Linux
  aarch64 only; CI's `ubuntu-latest` is x86_64, so the first CI run of this milestone is what proves the conda package
  has the wasm32 target there (`clang -print-targets` in the step log).
- **DONE**: `cmdr-operation-log` compiles for wasm32 in Linux CI (x86_64) and locally on macOS (no CI job runs the wasm
  lane on macOS; `macos-lane.yml` has no wasm step); the desktop build is untouched.

### 2. The runtime seam covers the write engine's needs (S–M)

- **Scope**: `crates/cmdr-fs/src/rt/`, `crates/cmdr-fs/src/deadline/`, the in-memory chunk delay, and a new
  `crates/cmdr-fs/clippy.toml`.
- **Intentions**: add what the survey found the write engine uses beyond the listing engine's ten sites:
  - `rt::wall_now()` (natively `SystemTime::now()`, in the browser `web_time::SystemTime::now()`).
  - `rt::JoinSet` (natively tokio's `JoinSet` re-exported with a `spawn_on(&Runtime)`; in the browser a small set over
    `spawn_local` with the same `join_next`/`len`/`is_empty`/task-id surface `copy_concurrent.rs` reads). Read that
    driver first: it relies on draining semantics (`transfer/volume/CLAUDE.md` "DRAINS, never aborts first").
  - `rt::yield_now()` (natively `tokio::task::yield_now`; in the browser a `setTimeout(0)` macrotask yield, so the page
    paints and takes input between chunks).
  - `deadline`: route its spawns and timers through `rt` (natively identical).
  - `InMemoryVolume`'s chunk delay (the `with_read_chunk_delay` test hook) sleeps through `rt::time::sleep`.
  - `crates/cmdr-fs/clippy.toml`: the root's general settings plus the clock bans from
    `crates/cmdr-listing/clippy.toml`; `rt/` keeps its module-level allow. Existing direct reads elsewhere in `cmdr-fs`
    either move to `rt` (if a browser path can reach them) or carry a reasoned allow (host-only modules).
- **Landmines**: dropping tokio's `JoinSet` ABORTS its tasks, while a dropped `spawn_local` task runs on. The browser
  set must abort on drop too (an abort flag each task checks at its next poll), or the plan must prove the driver never
  drops a non-empty set. `yield_now` natively must stay exactly tokio's (a `setTimeout`-shaped yield natively would slow
  every copy). `spawn_local` futures aren't `Send`-checked: keep the native signature's `Send` bounds so desktop code
  can't regress.
- **Test plan**: native unit tests for each wrapper (pure forwards); browser behavior through the Node smoke
  (`smoke.mjs` gains a `rt` self-test export: a three-task `JoinSet` drain, a `yield_now` that lets a `setTimeout` fire,
  `wall_now` within a second of `Date.now()`).
- **DONE**: the seam has every primitive the write engine needs, each natively a re-export or one-line forward.

### 2b. The in-memory volume models one device (M)

- **Scope**: `crates/cmdr-fs/src/volume/in_memory*` and the demo tree's builder (`cmdr-listing-wasm/src/tree.rs`). Every
  option here is off unless set, so every existing cell is unchanged; natively only tests and the demo turn them on.
- **Why**: today the demo's files are metadata only (`InMemoryVolume::with_entries` stores `content: None`, and
  `open_read_stream` streams `unwrap_or_default()`, so zero bytes). A copy of 1,200 photos would finish at once with
  0-byte files and size mismatches, and every in-memory call returns without ever waiting, so a 1,200-item delete, scan,
  move, or rollback runs as ONE JS task: the page freezes, Cancel can't be clicked, and every deferred event lands at
  the end. Real bytes are impossible (about 3.6 GB against wasm32's 4 GB address space). Free space is faked in TS
  (`lib/demo/answers.ts` answers `getVolumeSpace` with a constant 412 GB) while the engine's `get_space_info` says
  `NotSupported`.
- **Intentions**:
  - **Content model: real bytes OR a synthetic descriptor, per entry.** An entry holds stored real bytes (what a person
    writes, and the demo files the viewer effort gives real content) or "N generated bytes from a seed". Synthetic bytes
    are a PURE FUNCTION of `(seed, offset)` (a counter-mode generator, ❌ never a running state), so any positioned read
    works and every reader sees the same bytes; the read stream generates them chunk by chunk (cheap, deterministic, the
    size the listing says). `write_from_stream` compares each incoming chunk against what a descriptor would generate at
    that offset and, while every chunk matches, keeps the descriptor instead of the bytes (content-addressed, like a
    deduplicating filesystem, so a copy of a synthetic file stays a descriptor with no side channel between the read and
    write halves; one mismatched chunk and it falls back to real bytes from the start). The demo tree's builder gives
    every file a descriptor (seeded from its path) unless the tree supplies real content.
  - **Positioned reads stay unpaced**: `open_random_reader` (both content kinds) and the viewer's media serve path read
    at memory speed and never yield, because the viewer bridges sync-over-async on an in-memory volume by polling once.
    Bandwidth and yielding apply to the streaming path (`open_read_stream`, the copy) and to the walk, mutate, and stat
    calls only.
  - **Bandwidth**: `with_bandwidth(bytes_per_sec)` models ONE device's speed, for the demo (decision 3). **A volume-wide
    budget** (one token bucket the volume owns, which every read stream draws its chunks from), ❌ not a per-stream
    rate: `transfer_concurrency` takes `max_concurrent_ops()` (32 here), so a three-file copy runs three streams, and
    per-stream pacing would show three times the device's speed. A shared budget is how one disk behaves (several
    readers split its throughput), and it keeps the engine's real concurrent driver in play, so the demo runs the same
    path a desktop volume copy of three or more files does. (Chosen over `max_concurrent_ops = 1`, which would be
    simpler and drop the browser `JoinSet`, but would model the device by changing which driver runs.) Off unless set,
    so every existing cell is unchanged. Two copies at once can't double it either way: both sides share the volume's
    one lane, so the second queues.
  - **Device-modeled yielding**: `with_yielding(slice)`: every volume call (list, stat, create, delete, rename, each
    chunk) awaits `rt::yield_now()` once `slice` (about 8 ms of `rt::now()`) has passed since the volume last yielded,
    like a real device whose every call waits on I/O. This lets the page paint, take input, and deliver events during a
    long delete, scan, same-volume move, conflict scan, or rollback, all with zero engine change.
  - **Live free space**: `with_capacity(total_bytes)` answers `get_space_info` as `total` minus the stored plus
    synthetic bytes, computed live, so the copy's free-space pre-flight and a disk-full refusal (a Retry case for the
    queue window) are real. The demo answers `getVolumeSpace` from the engine (the host milestone), and the TS constant
    goes.
- **Landmines**: the bandwidth sleep and the yield must sit where cancel and pause still reach them (the engine's
  `CheckpointStream` wraps the source stream, so a pending sleep is dropped on cancel like any read); ❌ never a busy
  wait. The content-addressed compare must never accept a chunk that differs (compare bytes, not a hash of a prefix), or
  a person's edited file would silently become generated content. `InMemoryVolume` is the oracle for the `Volume`
  contracts (`crates/cmdr-fs/CLAUDE.md`): these options model a device, ❌ they never relax a contract.
- **Test plan**: paused-clock cells: a 1 MB file at 1 MB/s takes one virtual second; three 1 MB files read at once take
  three; a cancel mid-file stops it; a synthetic file reads back its stated size and the same bytes twice, and a
  positioned read at any offset equals the stream's bytes there (and is never delayed under bandwidth); a copy of one
  stays a descriptor (memory doesn't grow) and reads back identical; a modified chunk falls back to real bytes; free
  space drops by a copy's size and a copy larger than it refuses. The Node smoke: a `setTimeout` fires during a
  1,200-file delete, and a copied file's size equals the source's with a duration near size / bandwidth.
- **DONE**: the demo volume behaves like one real, finite, slow-enough device, with no engine change.

### 3. Characterize what the conversions touch (M)

- **Scope**: tests only, in `cmdr-operations` and `cmdr-operation-log`. No production change.
- **Intentions**: pin CURRENT behavior so milestones 4–6 are provably byte-identical:
  - **Golden traces**: one per op kind (copy, move, delete, rename, mkdir, a conflict answered Skip, cancel plus
    rollback), on both the local and the volume engine, recording the events in order with their payloads (minus ids and
    timestamps, normalized), the journal rows, and the typed outcome. Run before and after every later conversion
    milestone; a diff is a regression until explained. Format: JSON fixtures under
    `crates/cmdr-operations/tests/golden/` (one file per op kind and engine), compared by a small helper; a
    `CMDR_BLESS_GOLDEN=1` run rewrites them (no snapshot crate exists in the workspace, and none is added for this).
    7b's Node smoke reads the volume-engine fixtures to compare the browser run. This is what "byte-identical" means
    operationally, beyond routing and clocks.
  - **Routing table**: for each entry point (copy, move, delete, trash, rename, rename pre-flight, mkdir, mkfile, bulk
    rename, scan preview, and the four archive-edit `settle_volume` decisions: `copy_into.rs`, `driver.rs`,
    `fresh_compress.rs`, `move_out.rs`), which engine runs (local `std::fs` vs volume) for volume id `"root"` with a
    local root, `"root"` absent, and a non-local id. One table-driven cell file. Each cell asserts an OBSERVABLE that
    tells the engines apart (the bytes land in a tempdir versus inside the `InMemoryVolume` registered under that id),
    ❌ never just "it succeeded", which passes on both. The commit body shows each cell going red with the routing
    deliberately flipped. Milestone 5a extends the same table with the demo host's shape (no local-disk id), so both
    shapes run natively.
  - **Clock-sensitive cells**: list the existing `start_paused` and timing cells over the sites milestone 4 converts
    (outage waits, retry backoff, stall watchdog, scan watchdog, human-wait clock, leftover-cleanup dial, progress
    cadence). Add a pin where a converted site has none, and see each RED once.
  - **Op-log writer**: backpressure (`record_items` blocks at `CHANNEL_CAPACITY`), finalize outcome, a DB error dropping
    exactly one row, ordering across `open`/`record`/`finalize`. These pin what an inline browser writer must match.
  - **Pacing's answers**: a cell in `apps/desktop/src-tauri/src/operations_host/` pinning what `pacing.rs` answers for
    each `test_mode` setting (invariant 11), so nothing in this effort can move desktop or E2E timing unnoticed.
  - **Concurrent copy drain**: the first-failure drain and the stop-work drain on `copy_concurrent.rs` (likely already
    covered by `copy_concurrent_driver_tests.rs`; confirm and name them in the commit body).
- **Landmines**: pin reality, not this plan. A pin that passes before and after a deliberate break isn't a pin.
- **Test plan**: the new cells green natively; each timing pin shown red once (say how in the commit body).
- **DONE**: one commit of pins; the commit body lists which existing cells cover which converted site.

### 4. Every clock and timer goes through `cmdr_fs::rt` (M)

- **Scope**: `cmdr-operations` and `cmdr-operation-log` (production and the test helpers they share).
- **Intentions**: mechanical: `std` clock → `rt::now`/`rt::since`, tokio clock and timers → `rt::time::*`,
  `SystemTime::now` → `rt::wall_now`, `JoinSet` → `rt::JoinSet`, `yield_now` → `rt::yield_now`. Add the clock bans from
  `crates/cmdr-listing/clippy.toml` to both crates' existing `clippy.toml` (append; each already carries the root's
  settings and its spawn bans). Tests: `cmdr-operations/src/lib.rs`'s
  `cfg_attr(test, allow(clippy::disallowed_methods))` already lifts the whole lint for its tests (one lint can't be
  lifted per path, so the clock bans are lifted there too, which is fine); `cmdr-operation-log` gets the same
  `cfg(test)` lift, since it has none and its tests would hit the bans.
- **Landmines**: the two-clocks rule. `Instant` as a STORED type (`state.rs` `born`, outage records) changes nothing
  natively (re-export), but a field typed `tokio::time::Instant` must stay on tokio's clock. Shuttle's
  `sync_primitives.rs` swap is unaffected; don't touch it.
- **Test plan**: the golden traces unchanged, the full `cargo nextest run -p cmdr-operations -p cmdr-operation-log`, the
  Shuttle lane (`desktop-rust-shuttle`), clippy, and the Criterion bench's copy cases against the baseline
  (`docs/notes/transfer-extraction-baseline.md`'s method): no change beyond noise.
- **DONE**: zero direct clock or timer reads in either crate, enforced by clippy; natively identical.

### 5. The local disk as a typed capability (L, three commits, each green alone)

What "the facade" means here, so its invariant is checkable: **the local-engine module trees** (paths from
`crates/cmdr-operations/src/`, a starting list that 5b's classification finalizes: `transfer/copy/`,
`transfer/move_op/`, `transfer/chunked_copy.rs`, `transfer/macos_copy.rs`, `transfer/linux_copy.rs`,
`transfer/copy_strategy.rs`, `delete/walker.rs`, `delete/trash.rs`, `delete/cloud_trash.rs`, `scan.rs`,
`scan_dry_run.rs`, `scan_walker.rs`, `cancellable.rs`, `scan_preview/local.rs`, `in_flight_sweep.rs`,
`in_flight_sweep_surface.rs`, `unblocked_removal.rs`, `local_start.rs`, `scratch_dir.rs`, `overwrite.rs`,
`overwrite_aside_park.rs`, `durability.rs`, `mount_boundary.rs`, `free_space.rs`) **plus one primitives module**
(`local_disk/`, name it after what it is) holding what the shared code needs from the host OS: errno constants and
classification, the `(dev, ino)` identity, `statvfs`, mount tables, `rename_no_replace`. Outside those, no `cmdr_local`,
`libc`, `std::os::unix`, `objc2`, `xattr`, `trash`, or `dirs`. A Go check in the `index-crate-isolation` style enforces
it (`scripts/check/checks/`, an allowlist of the module paths above), ❌ not a review grep.

#### 5a. The capability, the typed answer, and the routing table in both shapes

- **Intentions**:
  - `OperationVolumes::local_disk(&self, volume_id) -> Option<LocalDisk<'_>>` (the per-concern seam split
    `crates/cmdr-listing/src/host/volumes.rs` models). `LocalDisk` is a small type you can only get from that call;
    every local-engine entry point takes `&LocalDisk`, so "run the local engine on a volume that isn't the local disk"
    is unrepresentable natively too (`AGENTS.md`: prefer a type to a ❌ rule). The default answer is today's
    `volume_id == "root"` (plus the existing `supports_local_fs_access` checks where they stand), so the desktop and
    every existing test change nothing; the demo host answers `None` (decision 5).
  - The ~12 deciding `"root"` sites become `match volumes.local_disk(id)`; the default-id fills stay literal.
  - **The refusal** when a local-only path is asked with no local disk (in practice only a local-only command reaching
    the demo, since the routing sends everything else to the volume engine): reuse an existing variant ONLY if the
    frontend's copy for it is honest in the demo. Candidates:
    `MutationError::Volume { error: VolumeError::NotSupported }` for instant ops; for transfers, check what
    `WriteOperationError` maps a volume `NotSupported` to and read its FE copy. If neither reads honestly ("this volume
    can't do that" is honest; "is read-only" is not), add ONE variant per family (`NoLocalDisk`), with English copy plus
    an `@key` description, its `bindings.ts` regen, and the FE mapping in the same commit; David reviews the copy later;
    ❌ never translate it. Archive edits keep the routed read-only answer (decision 4), refused BEFORE `ScratchDir::new`
    (whose `std::env::temp_dir()` panics in the browser).
- **Test plan**: milestone 3's routing table gains the no-local-disk shape: every entry point (archive edits included)
  on the volume engine or refused, each cell red once with the answer flipped. The golden traces unchanged.

#### 5b. Code motion behind the boundary (pure, natively identical)

- **Intentions**: first CLASSIFY every module in the starting list as gated (local-only), split, or shared, and write
  the table into the commit body. Several are MIXED today: shared and volume code imports from them, so gating them
  whole would break the wasm build. Known: `scan_preview/volume.rs` uses
  `scan::{SubtreeTotals, scan_subtree_with_oracle}`; `scope.rs` and `ledger.rs` use `overwrite::aside_park` and
  `overwrite::DisplacedEntry`; `conflict.rs` uses `durability::lookup_indexed_size` and
  `overwrite::ResolvedDestination`. Find the rest with `rg 'use (super|crate)::(…)::'` per module. Move each mixed
  module's SHARED half (types, the oracle scan, `aside_park`, the indexed-size lookup) out to a shared home, and the
  host-OS primitives the shared code uses into `local_disk/`, so the Go check passes and 5c can gate what's left whole.
  ❌ No logic change: moves and re-exports only, each module's tests travel with it.
- **Landmines**:
  - ❗ The boundary is only worth something if it's REAL: no `#[allow]`-sprinkled half-gate. Errno classification can
    stay where it is if it reads `io::Error::raw_os_error()` against constants `local_disk/` exports.
  - `EVERY local write lands via overwrite::stage_and_land_file` must stay the only door. `ledger.rs`'s local identity
    (`dev`, `ino`) comes from `local_disk/`; the volume identity (size) stays where it is.
  - `module-cycles` and `file-length` will notice; split by responsibility, not to satisfy the check. The public surface
    is capped (`DETAILS.md` § "The public surface is capped"); all of this is crate-private.
- **Test plan**: the full native suite, the golden traces, and the routing table unchanged; the new Go check green.

#### 5c. The wasm build

- **Intentions**: `cfg(not(target_family = "wasm"))` on the local-engine trees and `local_disk/`'s OS half (belt and
  braces: the browser can't reach a thread or a `blocking_recv` even if a host answer were wrong); `Cargo.toml` moves
  `cmdr-local`, `libc`, `dirs`, `walkdir`, `xattr`, and tokio's `fs` under that `cfg` (the `testing` feature's
  `cmdr-local/testing` with them). The wasm lane adds `cargo build` and
  `cargo clippy -p cmdr-operations --target wasm32-unknown-unknown`.
- **Landmines**: `LocalDisk` itself stays defined on wasm (so the seam's signature is target-independent); only its
  constructor's OS-backed half is gated. Expect the type errors the survey's floor hid (`JoinSet::spawn_on`,
  `VolumeHost::runtime()`); they're milestone 2's seam, wired here.
- **DONE**: `cmdr-operations` compiles and lints for wasm32; natively identical; the Go check enforces the boundary.

### 6. Nothing on the browser's volume path needs a thread (M)

- **Scope**: the remaining executor sites the browser can reach: the op log's writer and retention, the scan preview,
  the in-flight ledger, the conflict pre-flight's deadline, `spawn_blocking` on the volume path.
- **Intentions**:
  - **The op log's writer gets an inline mode in the browser**: the message handling the thread loop runs becomes one
    function both call; natively the thread and channel are untouched, in the browser each call runs it at once on the
    caller (no channel, no backpressure to block on; finalize answers directly). Milestone 3's writer pins hold for
    both.
  - Retention (`spawn_retention`) takes `rt::Runtime`; its tick through `rt::time`.
  - The ledger: the demo claims `claim_ledger(None)` (unpersisted). Confirm nothing in that path starts the ledger
    writer or sweep threads; the `LedgerUnpersisted` report is expected and the demo host answers it quietly.
  - Audit each remaining `spawn_blocking` the volume path reaches and document in `src/host/DETAILS.md` why running it
    inline in the browser is sound (in-memory, instant, takes no lock an awaiting task holds). Expect only
    `locked_sources.rs`'s scan; if it touches the local disk, it belongs behind the facade instead.
  - `conflict_preflight.rs`'s deadline runs on `rt` (milestone 2).
- **Landmines**: inline `spawn_blocking` plus a `std::sync::Mutex` held across the call site is a self-deadlock on one
  thread; check every inline closure for locks the caller holds. `record_items` callers natively RELY on the block for
  backpressure; inline in the browser there's no queue to bound, which is fine only because the writer is synchronous.
- **Test plan**: native suites unchanged, plus the wasm build and clippy green. The end-to-end browser run needs a host
  (`OperationVolumes`, `EngineEvents`, sinks, an op log), so it lands in 7b, ❌ not here; this milestone's audit is the
  written classification of every reachable executor site in `src/host/DETAILS.md`.
- **DONE**: nothing the browser can reach on the volume path starts a thread, calls `block_on`, or waits in
  `blocking_recv`, by the written audit and the `cfg` gating.

### 7a. An in-memory volume patches the panes (S–M)

- **Scope**: `cmdr-fs`'s `InMemoryVolume` and `cmdr-listing`; natively testable, landable alone.
- **Intentions**:
  - A builder option hands `InMemoryVolume` a `VolumeHost` (or `ListingHost`), and its `notify_mutation` then runs
    `patch_mutation` through a `PatchSource`, as every mutable backend does. Without the option it stays a no-op, so
    every existing cell is unchanged.
  - One `ListingHost` over `cmdr_listing::Listings`. If the engine's `host::testing::PaneListings` and the demo's would
    be the same code, put it once in `cmdr-listing` (a genuine boundary win); the desktop keeps `AppListings` (it
    composes the index and badges).
- **Test plan**: native cells: a create, delete, rename, and modify on a patched `InMemoryVolume` each reach a listing
  open on `Listings` as the matching `directory-diff` change; an unpatched volume stays silent.
- **DONE**: a write on an in-memory volume updates an open listing natively.

### 7b. The demo host: writes in the WASM engine (L)

- **Scope**: `cmdr-listing-wasm` (the rename is decision 6, later), its build and checks, and the demo image's inputs.
- **Intentions**:
  - **Its own module tree, mirroring `apps/desktop/src-tauri/src/operations_host/`** (`operations_host/` in the wasm
    crate: `volumes.rs`, `events.rs`, `sinks.rs`, `wire.rs`, `commands/` by domain), ❌ not a growing `commands.rs`. The
    listing half's `commands.rs` stays the listing commands.
  - The host: `OperationVolumes` over the one volume (id `"root"`, `local_disk` → `None`, decision 5), `EngineEvents`
    and the per-op sinks emitting the desktop's event names and payloads (`operations_host/wire.rs`, `sinks.rs`), the op
    log opened on `/demo/operation-log.db` (memvfs; gone on reload, as the demo's state is), `NameOrder` from the
    listing engine's collator, and `Pacing` left at its default `None`. The demo's `InMemoryVolume` is built
    `with_bandwidth(…)` (decision 3; pick a rate that makes a 10 MB copy take a couple of seconds, and say it in
    `DETAILS.md`).
  - Commands by their Tauri names with camelCase args, answers and typed rejections exactly as the desktop's
    (`commands/file_system/write_ops.rs`, `volume_copy.rs`, `operations.rs`, `commands/rename.rs`,
    `commands/operation_log.rs`). Where a desktop command is local-only (`moveFiles`, `trashFiles`, `moveToTrash`), the
    demo host answers it the way the desktop would for a non-local volume (the volume move; trash refused as a volume
    without one refuses, decision 2); say so in `DETAILS.md` § "Where the demo differs from the app". ❗ `moveFiles`
    must consume the dialog's `preview_id` (root to root keeps the deep scan preview running, per the survey), or the
    preview leaks and the move re-walks.
  - The demo volume is built `with_capacity(…)` and `with_yielding(…)` too (milestone 2b), and `getVolumeSpace` is
    answered by the engine (the desktop's command shape), so `lib/demo/answers.ts`'s constant goes in the frontend
    milestone.
  - The volume's `supportsTrash` (and every capability the frontend reads) comes from the engine's `Volume` answers
    (`has_trash()` false), ❌ not a TS constant.
  - `events_test.rs`-style pins for every new event's keys against `bindings.ts`.
  - The Node smoke grows: each command, a conflict round trip, a cancel, an undo, and a `directory-diff` seen for each
    mutation.
- **Landmines**:
  - ❗ Same wire or the frontend breaks silently (the listing crate's must-know). A drifted payload key shows up only in
    the browser.
  - The listing engine and the write engine must share ONE `InMemoryVolume` instance and ONE `Listings`; two would make
    a copy invisible to the panes.
  - `JsSink`'s `unsafe impl Send + Sync` soundness rests on no `atomics`; keep its `cfg` guard and reuse it.
  - The size budget: measure raw and gzipped `.wasm`; report it in `DETAILS.md`.
- ❗ **The demo image's inputs**: this is the milestone whose wasm module first links `cmdr-operations`,
  `cmdr-operation-log`, and `cmdr-archive` (plus any other path dep they newly pull in; list them from
  `cargo tree -p cmdr-listing-wasm --target wasm32-unknown-unknown -e normal --prefix none | rg '\(/'`). Add every one
  to all three lists in the same commit: `scripts/website-demo-key.sh`, `websiteDemoInputs` in
  `scripts/check/checks/inputs.go`, and ci.yml's `website` filter. Otherwise an engine-only change keeps the image key,
  `publish-website-demo` finds the old image "final", and getcmdr.com silently serves a stale write engine.
  `TestWebsiteDemoInputsMatchTheImageKey` keeps the first two aligned; the filter is by hand.
- **Test plan**: native tests of the host (commands, wire pins) with `cargo nextest`; the end-to-end run 6 deferred here
  (copy 3 files, move, delete, rename, mkdir, a conflict answered Skip, a cancel, journal rows read back); the wasm
  lane's Node smoke, plus milestone 3's volume-engine golden traces replayed as a wasm test build and diffed with the
  native ones (invariant 13); the smoke fails on any `RuntimeError` or unhandled rejection (invariant 6).
- **DONE**: from JS, every write command answers like the desktop's and the panes' listings change through
  `directory-diff`.

### 8. The demo frontend writes (M)

- **Scope**: `apps/desktop/src/lib/demo/` and the website's demo e2e.
- **Intentions**: delete `writes.ts` and its test; drop `answers.ts`'s constant `getVolumeSpace`; `volumes.ts` stops
  reporting `mountIsReadOnly` and stops hardcoding `supportsTrash` (the engine answers it); F8 then takes the app's own
  permanent-delete confirm; the backend routes the write commands to the engine (`DemoListing.handles()` or its renamed
  twin) and the write events through the existing fan-out. Re-derive the command set from `window.__cmdrDemo.unanswered`
  while driving F5–F8, rename, and the transfer dialog. Update `lib/demo/CLAUDE.md` + `DETAILS.md` (the "writes nothing,
  and says so" must-know becomes "writes through the real engine, in memory, gone on reload").
- **Landmines**: a TS answer for an engine command silently shadows the engine (the backend tries TS tables first).
  `answers.ts` may already answer some write-adjacent commands (`destinationWriteAccess`, space queries) with read-only
  values; each moves to the engine or to an honest TS fact. The demo can be framed: dialogs must work inside an iframe.
- **Test plan**: vitest for the routing; the website's demo Playwright smoke (`apps/website/e2e/demo.spec.ts`) gains F7
  new folder, F5 copy into the other pane, Shift+F6 rename, F6 move, a conflict prompt answered, F8 delete. A normal
  `pnpm build` still has no `__cmdrDemo` in `build/`.
- **DONE**: a visitor copies, moves, renames, creates, and deletes in the demo, and both panes update.

### 9. Undo in the demo (S–M)

- **Scope**: the host's undo commands and the frontend's Cmd+Z flow.
- **Intentions**: `undoOperations`, `rollbackOperation`, `getRecentOperationLogEntries`, `getOperationLogDetail` run on
  the real op log and rollback planner; the in-browser `RollbackRunner` is the engine's own executor. A copy, a move, a
  rename, and a new folder undo; a permanent delete says it can't (the planner's typed answer).
- **Landmines**: the planner FAILS SAFE (verify each item against its snapshot); an in-memory volume's identities must
  satisfy `verify_snapshot` honestly, or every undo skips. Rollback pacing stays `None`; an undo's copy-back streams
  through the same bandwidth. Multi-op undo reverses newest first.
- **Test plan**: Node smoke (each undoable op round-trips; a drifted item skips with its `SkipReason`), Playwright
  (Cmd+Z after a copy and after a rename).
- **DONE**: Cmd+Z undoes the last demo operation through the same planner the desktop uses.

### 10. The transfer queue window (S)

- **Scope**: `apps/desktop/src/lib/demo/windows.ts` and QA.
- **Intentions**: add `queue` to `OPENABLE`; it runs on the same backend and events. QA at the volume's bandwidth
  (decision 3; two copies at once queue behind each other's lane): progress, pause and resume (one and all), cancel,
  reorder, and the transfer tree view. Retained failures and Retry can't happen on a healthy in-memory volume; cover
  them in the Node smoke with a failing volume option rather than faking one in the demo.
- **Landmines**: the queue window may call commands no milestone answered yet (`unanswered` tells); it opens as a
  same-origin iframe of its route and must find the top frame's backend.
- **Test plan**: `windows.test.ts`; a Playwright cell that opens the queue during a paced copy and pauses it.
- **DONE**: the queue window opens in the demo and controls a running copy. Then the close-out: an adversarial
  conformance review against the invariants below and a docs-audit pass over every commit body and touched `CLAUDE.md`.

## Invariants (the conformance review checks these)

Each names the mechanism that enforces it; one without a mechanism is a review item, not an invariant.

1. Natively, every engine call takes the same path it took at `49aad7f793` (the routing table), and every timing cell
   that passed then passes now on the same clock.
2. No direct clock, timer, or ambient-runtime call in `cmdr-operations`, `cmdr-operation-log`, or `cmdr-fs` outside
   `cmdr_fs::rt`, enforced by clippy.
3. Outside the local-engine module trees and `local_disk/`, no `cmdr_local`, `libc`, `std::os::unix`, `objc2`, `xattr`,
   `trash`, or `dirs` in `cmdr-operations` production code, enforced by a Go check; and no local-engine entry point is
   callable without a `LocalDisk` (the type).
4. In the browser, no reachable path starts a thread, calls `block_on`, or waits in `blocking_recv`; the local engine
   isn't compiled in (the `cfg` gating, plus invariant 6's trap detection).
5. Every write in the browser still stages and lands (`resolve_staging` on the volume path), journals through its
   `OpScope` under the real volume id, and patches the panes through `notify_mutation` (invariant 13's browser traces).
6. A browser call the engine can't serve refuses with a typed error; nothing traps. Mechanism: the Node smoke and the
   Playwright smoke FAIL on any `RuntimeError`, `unhandledrejection`, or console error the run didn't expect.
7. The demo's wire (command names, args, answers, rejections, events) equals the desktop's, pinned against `bindings.ts`
   (`events_test.rs`-style pins) and by invariant 13.
8. No conda toolchain binary on PATH or in the default mise data dir in any script or CI job; only the private
   `MISE_DATA_DIR` and absolute paths in the wasm-target CC/AR variables (the build script's tests).
9. No demo code in a normal build, and no `if (demo)` in app code (`cmdr/demo-isolation`, the `build/` grep).
10. Undo in the demo is the real planner, failing safe; nothing in TS decides what an undo does. Mechanism:
    `cmdr/demo-isolation` plus a vitest that fails if any `lib/demo` TS table answers an undo or operation-log command
    (`undoOperations`, `rollbackOperation`, `getRecentOperationLogEntries`, `getOperationLogDetail`).
11. `Pacing`, its desktop answers, and E2E timing are unchanged; demo throughput is `InMemoryVolume`'s bandwidth alone.
    Mechanism: a test pinning `operations_host::pacing`'s answers per `test_mode` setting, landed with the
    characterization.
12. "Which id is the local disk" is the host's typed answer (`OperationVolumes::local_disk`, `"root"` by default), and
    the routing table pins both shapes.
13. The golden traces (events with payloads, journal rows, typed outcomes per op kind) are identical before and after
    every conversion milestone natively, AND the volume-engine traces run in the browser build and match the native ones
    (catching browser `JoinSet` ordering, the inline op-log writer, and inline `spawn_blocking` drift). **DONE**: the
    scenarios and normalizer are `cmdr_operations::golden` (a dev-only feature); `cmdr-listing-wasm`'s `golden_test.rs`
    replays the volume ones on a real `Demo`'s host natively and as a wasm test build under Node (`wasm-bindgen-test`),
    in the `listing-wasm` check. A test build so the shipped `.wasm` carries no scenarios;
    `crates/cmdr-listing-wasm/DETAILS.md` § "The build".
14. The demo image's content key, `websiteDemoInputs`, and ci.yml's `website` filter cover every crate the wasm module
    links (`TestWebsiteDemoInputsMatchTheImageKey` for the first two).
15. A copied file's size and bytes equal its source's, synthetic or real, and a synthetic copy stays a descriptor
    (`InMemoryVolume` cells; the Node smoke's size check).
16. No stretch of engine work holds the page thread longer than about 50 ms: every in-memory volume call yields once its
    slice is spent. Mechanism: the Node smoke runs a 1,200-file delete, scan, and rollback with a `setTimeout` probe and
    fails if the longest gap between probe ticks passes the bound.
