# Operation-log capture benchmarks

Numbers behind the capture-layer performance decisions: the `search_only` leaf-enumeration cap, and the claim that
journaling never measurably slows an operation (requirement 8). Linked from `src-tauri/src/operation_log/DETAILS.md`.

Evidence anchor: measured on an Apple Silicon dev machine (macOS 15), release-adjacent `cargo nextest` test profile
(unoptimized + debuginfo — so absolute numbers are conservative vs a release build), 2026-07-10, branch
`david/action-log`. Reproduce with the ignored benches:

- `cargo nextest run --run-ignored ignored-only -E 'test(bench_enumeration_latency_by_subtree_size)' --no-capture`
- `cargo nextest run --run-ignored ignored-only -E 'test(bench_persist_throughput)' --no-capture`
- `cargo nextest run --run-ignored ignored-only -E 'test(bench_same_fs_move_latency_delta)' --no-capture`

The always-on 3-arm perf test (`capture_stays_off_the_hot_path_under_writer_load`) runs in `rust-tests`.

## 1. Leaf-enumeration latency vs subtree size — and why the cap is 50,000

A trash / same-FS move enumerates the subtree's leaves from the drive index SYNCHRONOUSLY before the OS mutation (the
reconciler prunes the subtree on the FSEvent, so it must run first). So its cost is paid before a sub-second rename.

Uncapped read of the whole subtree (flat folder, N direct children):

- N = 1,000: ~2.3 ms
- N = 10,000: ~11.9 ms
- N = 100,000: ~121.7 ms

Linear at ~1.2 µs/leaf, so a 1,000,000-child folder would cost ~1.2 s of synchronous index reads before the rename —
disproportionate, and exactly what the cap prevents.

Capped at 50,000 (`SEARCH_LEAF_CAP`), reading at most `cap + 1` rows via `list_children_on_limited`'s SQL `LIMIT`:

- N = 1,000: ~1.4 ms (Full — under cap)
- N = 10,000: ~11.5 ms (Full — under cap)
- N = 100,000: ~58.6 ms (TopLevelOnly `capped` — cap hit, read bounded at ~50k rows)

The ~59 ms at N = 100k is the CEILING for every larger subtree (1M included), because the `LIMIT cap + 1` bounds the row
read by construction — that's the whole point of the limited reader.

**Cap decision: keep 50,000.** ~59 ms worst-case synchronous cost, and it runs on the op's `spawn_blocking` thread (NOT
the main thread / not the UI), before a background trash/move. The vast majority of trashed/moved folders hold far fewer
than 50k descendants and pay the true (smaller) cost. Trashing a 50k+ folder is rare, and 59 ms of background work there
is acceptable. The cap only ever bounds SEARCH completeness (an over-cap op is `top_level_only` + `capped`, still fully
rollbackable) — never correctness — so it stays cheap to retune if real usage shows otherwise (David's framing:
benchmark from day one, cheap to change).

## 2. Background persist throughput

Draining a burst of `search_only` leaf rows through the single writer thread (batched at 512 per transaction, interning
dirs and folding names, then a `flush_blocking` barrier for the true drain time): 50,000 rows in ~1.28 s, or about
**39,000 rows/s**.

This is the writer's steady drain rate. It comfortably outpaces the rate any real op produces items (each item costs the
op far more in file I/O — copying bytes, trashing, renaming — than a batched row insert costs the writer), which is why
the bounded channel's backpressure is a theoretical backstop, not a hot-path cost (D4). A 1M-file delete journals ~1M
rows and drains in ~26 s of background writer time, well within the op's own runtime.

## 3. Same-FS move op-latency delta (journaling on vs off)

Per-operation overhead of journaling a same-FS move (open + one top-level `rollback_unit` row + a `VolumeNotLive`
enumerate with no index registered + finalize):

- OFF: ~9.7 ms/op — the baseline `move_files_with_progress_inner` cost for one dir.
- ON: ~10.9 ms/op — **~1.2 ms/op added**.

The delta is dominated by the finalize BARRIER — one synchronous round-trip to the writer thread per operation (the
completeness check needs the durable row counts back). Crucially this is paid ONCE per user action (one op per move
batch), NOT per item, so it never touches the per-item hot path. The per-ITEM claim (capture adds ~nothing to each
copied/deleted file) is covered by the 3-arm perf test below, which journals 1,500 per-leaf rows.

## 4. Capture off the hot path under writer load (the 3-arm perf test)

`capture_stays_off_the_hot_path_under_writer_load` deletes 1,500 small files (real file I/O, per-leaf journaling — the
heaviest capture) in three arms and asserts the op finishes within a generous budget (`base * 6 + 3 s`) in all three:

- (a) no journal — the baseline.
- (b) a keeping-up journal — asserts all 1,500 leaves journal AND the op stays in budget.
- (c) a journal whose writer thread is concurrently hammered with retention `Prune { vacuum: true }` from another thread
  (with seeded churn so the freelist has pages for `incremental_vacuum` to reclaim) — the arm a naive keeping-up test
  would miss.

Arm (c) is the point: even a writer busy with retention vacuum can't stall the op, because the vacuum runs in bounded
`incremental_vacuum` slices between insert batches (never one stop-the-world pass) and the channel's backpressure only
ever blocks briefly. A capture that went synchronous on the op thread would blow the budget in (b)/(c). The test passes
with wide margin.
