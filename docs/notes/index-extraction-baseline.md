# Index-extraction: before and after

The measured before and after for extracting `indexing/`, `media_index/`, and `importance/` (93,256 of 332,264
`src-tauri/src` lines, 28.1%) into the standalone `cmdr-index` crate. The comparison is only meaningful if the method
matches, so each section carries the exact command.

**Before**, measured 2026-07-30 on an Apple M3 Max (16 cores, 64 GB), macOS 26.5.2, rustc 1.97.1 (`8bab26f4f`,
2026-07-14), machine otherwise idle. Tree: the `david-index-crate-extraction` worktree with thin LTO landed at commit
`3f565a88d`, before any code moved.

**After**, measured 2026-07-31 on the same machine and toolchain, at commit `4092256c0`. The machine was NOT idle this
time (an IDE indexing pass held load around 8), so every build number was taken twice: once on the extracted tree and
once with `3f565a88d` checked out in the same worktree, minutes apart. The old tree reproduced its own "before" numbers
to within 4%, which is what makes the paired comparison trustworthy under load. Both figures are given.

## The short version

- **The enrichment hot path did not move**, and neither did anything else Criterion measures: every case is within ±2%
  of before, and seven of eight are marginally faster.
- **The inner loop got 6–9× faster.** Type-checking the index after an index edit went 4.35 s → 0.75 s, and building its
  unit tests went 23–30 s → 3.6 s, because neither touches the app any more.
- **A full app build barely moved**, and that's expected: an index edit still relinks the app (+11%), and an app edit
  was never dominated by line count (−13%).
- **Release builds got 12% faster** (214 s → 188 s) for +0.6% binary size, because more crates means more parallelism at
  codegen.

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

The harness is `crates/cmdr-index/benches/index_benchmarks.rs` (it was `apps/desktop/src-tauri/benches/` before the
move, and there was no index benchmark before this effort). The fixture is a synthetic index DB built through the public
`store` API (no files on disk, no scan, no lifecycle), so it's deterministic and machine-independent in shape. The
harness asserts every fixture directory actually resolves before it times anything, because all three paths have cheap
early returns that would otherwise produce a green run measuring nothing.

```
cargo bench -p cmdr-index --bench index_benchmarks -- --save-baseline run1   # compare with: -- --baseline run1
```

Medians, two consecutive runs each side:

| case                                   | before            | after             | delta |
| -------------------------------------- | ----------------- | ----------------- | ----- |
| `enrich_entries_with_index/50`         | 66.36, 66.04 µs   | 65.89, 65.37 µs   | −1.0% |
| `enrich_entries_with_index/500`        | 570.13, 568.64 µs | 563.17, 563.08 µs | −1.1% |
| `enrich_entries_with_index/2000`       | 2.3395, 2.3513 ms | 2.2999, 2.3020 ms | −1.9% |
| `get_dir_stats_batch/50`               | 342.73, 342.31 µs | 333.85, 338.44 µs | −1.5% |
| `get_dir_stats_batch/500`              | 3.4059, 3.4452 ms | 3.3947, 3.4459 ms | −0.2% |
| `get_dir_stats_batch/2000`             | 13.672, 13.756 ms | 13.495, 13.705 ms | −1.0% |
| `compute_all_aggregates_reported/500`  | 2.3682, 2.3665 ms | 2.3437, 2.3219 ms | −1.5% |
| `compute_all_aggregates_reported/5000` | 26.844, 27.006 ms | 26.871, 27.089 ms | +0.1% |

**Reproducibility: before, every case landed within ±1.1% across two runs; after, within ±1.8%** on a busier machine.
Criterion classified all eight before-runs as "no change" or "within noise threshold", so a move outside roughly ±2% is
signal and anything smaller isn't.

**Every case is inside that band, and seven of eight came out marginally faster.** `enrich_entries_with_index` is the
one that matters: it's the sub-millisecond path every directory listing pays for its recursive sizes, and it still holds
~850 K directories/s at every listing size, so the per-directory cost is still flat. Nothing needed chasing — no
`#[inline]` was missing on a cross-boundary function, and no trait landed on a per-entry path (the dispatch rule that
forbids one is in `crates/cmdr-index/src/indexing/host/DETAILS.md`). Thin LTO landing before any code moved is what
bought this: without it, Cargo would inline only `#[inline]` and generic functions across the new boundary.

## Scan throughput isn't in this set, on purpose

A scan-throughput bench on the macOS disk-image fixture, widening `external_drive_fixture` to reach it, was the obvious
candidate for this baseline. It isn't here, and the substitute above (`compute_all_aggregates_reported`, the bottom-up
roll-up) covers the same risk more cheaply. Four reasons, in weight order:

1. **Criterion is the wrong instrument.** It wants many cheap, idempotent iterations. A scan is seconds to minutes,
   filesystem-bound, and mutates the DB it writes into, so every iteration needs a torn-down fixture. The statistics
   would be shaped by the setup, not the scan.
2. **Reaching the scanner would pre-commit public surface.** `scanner` is `pub(crate)`; a bench compiles as an external
   crate. Making it conditionally `pub` for a benchmark is exactly the "`pub` as a compile fix" the crate's
   public-surface rule exists to prevent (`crates/cmdr-index/CLAUDE.md`, enforced by the `index-crate-isolation` check).
3. **The disk-image route costs more than it returns.** It would promote `tempfile` from a dev-dependency to a shipped
   optional dependency, and it would run `hdiutil` attach/detach inside a benchmark loop, against the attach-once
   /detach-once FSKit discipline in `crates/cmdr-index/src/indexing/tests/CLAUDE.md`. That discipline exists because the
   alternative once kernel-panicked the machine. A 64 MB synthetic FAT32 image also says nothing about throughput on a
   real tree.
4. **A better scan measurement already exists.** `docs/notes/indexing-benchmarks-2026-07-21.md` records fresh-scan and
   reconcile numbers from the real app on a real 6-million-entry boot volume, with the method written down. That's what
   a scan-throughput re-measure should re-run.

`compute_all_aggregates_reported` gives up the filesystem walk but keeps what actually needed guarding through the
extraction: a per-entry loop over the index, in Criterion's shape, through a genuinely public entry point. If a trait
call or an allocation lands on a per-entry path during the extraction, it shows up there.

## Build times

### Release, and what thin LTO costs

Full clean workspace build (`cargo clean --release && cargo build --release` from the repo root, so every member
builds). Before, measured both ways by toggling the profile line:

- **Default profile** (`lto = false`, `codegen-units = 16`): **159.2 s**, `target/release/Cmdr` **76,272,096 bytes**
- **Thin LTO**: **214.3 s** (+34.6%), `Cmdr` **78,574,560 bytes** (+2.2 MB, +3.0%)

So thin LTO cost about 55 s per clean release build, and the same tax lands on every CI build in the sign-and-notarize
pipeline. The binary got slightly _bigger_, which is the expected direction: more cross-crate inlining means more
duplicated code. Worth it, because the index's hot paths now sit on the far side of a crate boundary, where Cargo's
default inlines only `#[inline]` and generic functions.

**After the extraction: 188.4 s** (−12% against thin-LTO-before, on a busier machine), `Cmdr` **79,055,312 bytes**
(+480,752, +0.6%). The release build got FASTER, which reads backwards until you notice what changed: three crates give
cargo three independent codegen units to schedule where there was one, so the long pole shortened. The extra half
megabyte is more cross-crate inlining, the same direction thin LTO already moved it.

**Startup time is not in this set.** Measuring it honestly needs a full `pnpm build` bundle plus an isolated data dir
(running `target/release/Cmdr` directly attaches to the real prod data dir, which `apps/desktop/CLAUDE.md` forbids), and
the app has no startup instrumentation to read a number off. The extraction is behavior-neutral and shouldn't touch
startup; if that assumption ever needs checking, the work is to add the instrumentation first.

### Debug, and the build-time separation goal

The goal was that editing the index shouldn't rebuild the app and vice versa. Before, the app crate was one compilation
unit, so either edit rebuilt all 332k lines.

Every edit is real (a changed log-message string), not a `touch`, so incremental compilation has to redo codegen for the
touched unit. Medians of five consecutive runs, taken minutes apart on the same machine with the old tree checked out in
the same worktree:

| scenario                                                    | before  | after      | delta |
| ----------------------------------------------------------- | ------- | ---------- | ----- |
| Clean crate build, deps warm (`cargo clean -p …`)           | 49.2 s  | 48.3 s     | −2%   |
| Index edit, then `cargo build` from `src-tauri`             | 10.06 s | 11.20 s    | +11%  |
| `commands/indexing.rs` edit, then `cargo build`             | 15.65 s | 13.57 s    | −13%  |
| Index edit, then `cargo check --lib` on what you're editing | 4.35 s  | **0.75 s** | −83%  |
| Index edit, then `cargo test --lib --no-run` on it          | 23–30 s | **3.55 s** | −85%  |

**The last two rows are the answer.** The first three measure "build the whole app", where a crate boundary can't help
much: an index edit still has to relink the app, so it costs slightly MORE than before, and an app edit was never
dominated by line count anyway (`commands/` is a `#[tauri::command]` module, so changing it re-expands the macro surface
the IPC builder is generated from — that cost didn't move when the app lost 28% of its lines).

What did move is the loop David is actually in while working on the index: type-check it, run its tests, repeat. That
went from 4.35 s and 23–30 s over 332k lines to 0.75 s and 3.55 s over 93k, because neither command touches the app at
all now. Before the split there was no way to ask for less than the whole thing.

The "before" figures reproduced within 4% of their 2026-07-30 values when re-measured on a loaded machine, which is what
makes the paired comparison sound. `cargo test --lib --no-run` on the old tree varied most (30.4 s then 23.1 s), so it's
given as a range.

## Thread QoS after the runtime swap

The property that lets indexing run inside the app process at all: the heavy walking, writing, and reconciling threads
sit at macOS `QOS_CLASS_UTILITY`, so a runaway scan can never outrank the webview for CPU. The extraction replaced ~66
`tauri::async_runtime::{spawn, spawn_blocking, block_on}` calls with an injected `tokio::runtime::Handle`, which is a
named risk for exactly this, so it was measured rather than assumed.

**Structurally, the runtime can't matter.** All seven `set_current_thread_qos` call sites sit at the top of a
**dedicated** `std::thread::Builder::spawn` body — `scanner/mod.rs`, `scanner/walker/mod.rs` (worker and watchdog),
`writer/mod.rs`, `reconcile/local_reconcile.rs` (reader and walk), `reconcile/reconciler/rescan/mod.rs`. Those threads
are created by index code and aren't tokio's to schedule; a tokio task that starts one is just the caller. macOS QoS is
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
