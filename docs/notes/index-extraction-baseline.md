# Index-extraction baseline

The measured "before" for `docs/specs/index-crate-extraction-plan.md`, which moves `indexing/`, `media_index/`, and
`importance/` (93,256 of 332,264 `src-tauri/src` lines, 28.1%) into a standalone `cmdr-index` crate. Every number here
gets re-measured at the end of that plan, and the comparison is only meaningful if the method matches, so each section
carries the exact command.

Measured 2026-07-30 on an Apple M3 Max (16 cores, 64 GB), macOS 26.5.2, rustc 1.97.1 (`8bab26f4f`, 2026-07-14), machine
otherwise idle. Tree: the `david-index-crate-extraction` worktree with thin LTO landed, parent commit `905935df5`.

## Read this before comparing

- **Thin LTO is already on** (`[profile.release] lto = "thin"` at the workspace root). It landed before any code moved,
  deliberately, so a post-extraction delta is attributable to the extraction and not to a profile change. `bench`
  inherits `release`, so every Criterion number below is a thin-LTO number.
- **The Rust memory numbers this repo already has were taken under a different allocator.** `test_support.rs` installs
  `COUNTING_ALLOCATOR` as `#[global_allocator]` in test builds, which REPLACES mimalloc (`main.rs`). So
  `count_allocations` / `heap_bytes_held` figures from the test suite describe the system allocator, not the shipped
  one. Nothing in this note is allocator-sensitive (Criterion measures wall time under the `bench` profile, which uses
  mimalloc), but the moment a memory figure joins the comparison, it has to compare test-allocator against
  test-allocator. This is Decision 18's consequence, and it's the easiest apples-to-oranges mistake available here.
- Every number is a median of 100 samples with the machine idle. Reproduce on an idle machine or don't compare.

## Criterion benches

`apps/desktop/src-tauri/benches/index_benchmarks.rs`, built for this baseline; there was no index benchmark before it.
The fixture is a synthetic index DB built through the public `store` API (no files on disk, no scan, no lifecycle), so
it's deterministic and machine-independent in shape. The harness asserts every fixture directory actually resolves
before it times anything, because all three paths have cheap early returns that would otherwise produce a green run
measuring nothing.

```
cd apps/desktop/src-tauri
cargo bench --bench index_benchmarks -- --save-baseline m0-run1   # then compare with: -- --baseline m0-run1
```

Medians, two consecutive runs:

- `enrich_entries_with_index/50` — 66.36 µs, 66.04 µs
- `enrich_entries_with_index/500` — 570.13 µs, 568.64 µs
- `enrich_entries_with_index/2000` — 2.3395 ms, 2.3513 ms
- `get_dir_stats_batch/50` — 342.73 µs, 342.31 µs
- `get_dir_stats_batch/500` — 3.4059 ms, 3.4452 ms
- `get_dir_stats_batch/2000` — 13.672 ms, 13.756 ms
- `compute_all_aggregates_reported/500` — 2.3682 ms, 2.3665 ms
- `compute_all_aggregates_reported/5000` — 26.844 ms, 27.006 ms

**Reproducibility: every case landed within ±1.1% across the two runs**, and Criterion classified all eight as "no
change" or "within noise threshold". So a post-extraction move outside roughly ±2% is signal, and anything smaller
isn't. `enrich_entries_with_index` is the one that matters: it's the sub-millisecond path every directory listing pays
for its recursive sizes, and it holds ~850 K directories/s at every listing size, so the per-directory cost is flat.

## Scan throughput isn't in this set, on purpose

The plan asked for a scan-throughput bench on the macOS disk-image fixture and offered to widen `external_drive_fixture`
to reach it. It isn't here, and the substitute above (`compute_all_aggregates_reported`, the bottom-up roll-up) covers
the same risk more cheaply. Four reasons, in weight order:

1. **Criterion is the wrong instrument.** It wants many cheap, idempotent iterations. A scan is seconds to minutes,
   filesystem-bound, and mutates the DB it writes into, so every iteration needs a torn-down fixture. The statistics
   would be shaped by the setup, not the scan.
2. **Reaching the scanner would pre-commit public surface.** `scanner` is `pub(crate)`; a bench compiles as an external
   crate. Making it conditionally `pub` for a benchmark is exactly the "`pub` as a compile fix" the plan's Decision 3
   exists to prevent.
3. **The disk-image route costs more than it returns.** It would promote `tempfile` from a dev-dependency to a shipped
   optional dependency, and it would run `hdiutil` attach/detach inside a benchmark loop, against the attach-once
   /detach-once FSKit discipline in `apps/desktop/src-tauri/src/indexing/tests/CLAUDE.md`. That discipline exists
   because the alternative once kernel-panicked the machine. A 64 MB synthetic FAT32 image also says nothing about
   throughput on a real tree.
4. **A better scan measurement already exists.** `docs/notes/indexing-benchmarks-2026-07-21.md` records fresh-scan and
   reconcile numbers from the real app on a real 6-million-entry boot volume, with the method written down. That's what
   a scan-throughput re-measure should re-run.

`compute_all_aggregates_reported` gives up the filesystem walk but keeps what the plan actually wants guarded: a
per-entry loop over the index, in Criterion's shape, through a genuinely public entry point. If a trait call or an
allocation lands on a per-entry path during the extraction, it shows up there.

## Build times

### Release, and what thin LTO costs

Full clean workspace build (`cargo clean --release && cargo build --release` from the repo root, so all three members
build), measured both ways by toggling the profile line:

- **Default profile** (`lto = false`, `codegen-units = 16`): **159.2 s**, `target/release/Cmdr` **76,272,096 bytes**
- **Thin LTO**: **214.3 s** (+34.6%), `Cmdr` **78,574,560 bytes** (+2.2 MB, +3.0%)

So thin LTO costs about 55 s per clean release build, and the same tax lands on every CI build in the sign-and-notarize
pipeline. The binary got slightly _bigger_, which is the expected direction: more cross-crate inlining means more
duplicated code. Both are worth it here, because after the split the index's hot paths sit on the far side of a crate
boundary, where Cargo's default inlines only `#[inline]` and generic functions.

**Startup time is not in this set.** Measuring it honestly needs a full `pnpm build` bundle plus an isolated data dir
(running `target/release/Cmdr` directly attaches to the real prod data dir, which `apps/desktop/CLAUDE.md` forbids), and
the app has no startup instrumentation to read a number off. The extraction is behavior-neutral and shouldn't touch
startup; if that assumption ever needs checking, the work is to add the instrumentation first.

### Debug, and the build-time separation goal

Goal 2 of the plan is that editing indexing shouldn't rebuild the app and vice versa. Today the app crate is one
compilation unit, so both edits rebuild all 332k lines. Run from `apps/desktop/src-tauri`:

- **Clean app-crate build** (`cargo clean -p cmdr && cargo build`, dependencies warm): **49.2 s**
- **One-line change in `indexing/read/enrichment.rs`, then `cargo build`**: **9.7 s**, **9.5 s**
- **One-line change in `commands/indexing.rs`, then `cargo build`**: **15.7 s**, **16.0 s**

The edits are real (a changed log-message string), not `touch`es, so incremental compilation has to redo codegen for the
touched unit. `commands/` is consistently the slower of the two: it's a `#[tauri::command]` module, so changing it
re-expands the macro surface the IPC builder is generated from.

Post-extraction the comparable numbers are:

- clean app-crate build after `cargo clean -p cmdr -p cmdr-index -p cmdr-fs`
- the `indexing/` edit at its new path under `crates/cmdr-index/`, which should rebuild `cmdr-index` plus a relink of
  the app rather than the app's own 239k lines
- the `commands/` edit, which is where the clearer win should land: the app crate loses 28% of its lines, and an app
  edit stops touching the index at all

## Thread QoS after the runtime swap

The property that lets indexing run inside the app process at all: the heavy walking, writing, and reconciling threads
sit at macOS `QOS_CLASS_UTILITY`, so a runaway scan can never outrank the webview for CPU. The extraction replaced ~66
`tauri::async_runtime::{spawn, spawn_blocking, block_on}` calls with an injected `tokio::runtime::Handle`, which is a
named risk for exactly this, so it was measured rather than assumed.

**Structurally, the runtime can't matter.** All seven `set_current_thread_qos` call sites sit at the top of a
**dedicated** `std::thread::Builder::spawn` body — `scanner/mod.rs`, `scanner/walker/mod.rs` (worker and watchdog),
`writer/mod.rs`, `reconcile/local_reconcile.rs` (reader and walk), `reconcile/reconciler/rescan.rs`. Those threads are
created by index code and aren't tokio's to schedule; a tokio task that starts one is just the caller. macOS QoS is
per-thread and set explicitly here, so nothing is inherited from whoever spawned the task either.

**Measured anyway, in-process** (2026-07-31, macOS 26.5.2, `pnpm dev` build, full fresh scan of `/` plus a reconcile).
`set_current_thread_qos` was temporarily instrumented to read its own class back with `pthread_get_qos_class_np` and log
it with the thread name; the instrumentation was reverted afterwards. Every call site fired and every one reported
`set_rc=0 get_rc=0 class=0x11` — `QOS_CLASS_UTILITY`:

- **`index-walk`**: 16 threads (the walker pool)
- **`index-writer`**: 2
- **`index-scanner`**: 1
- **`index-walk-watchdog`**: 1
- **`index-local-reconcile`**: 1
- **`reconcile-read`**: 1
- **`rescan-subtree`**: 1

23 threads, no exceptions.

**Don't use `ps -M` alone to re-check this.** Its `PRI` column does report `20T` for a Utility thread (verified against
a purpose-built C probe), but the walker pool is short-lived and the app carries 20+ webview and tokio threads at `46T`
legitimately, so a snapshot taken a moment late reads like a regression that isn't there. The in-process read-back is
the measurement that means something.

Redo it if the spawn topology changes. `set_current_thread_qos` is a no-op in test builds, so no unit test can catch a
regression here.
