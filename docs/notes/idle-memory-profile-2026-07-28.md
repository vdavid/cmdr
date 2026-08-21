# Idle memory profile: 2.5 GB, two independent causes (2026-07-28)

A profile of prod v0.36.2 after ~10 h uptime, and the fixes it drove. Unlike the two earlier memory investigations,
neither cause here is a runaway: both are steady-state costs the app pays forever while idle.

Read `docs/tooling/memory-debugging.md` first — it holds the measurement recipes and the `IOAccelerator` trap.

## The measurement

Prod Cmdr v0.36.2, PID 3062, macOS 26.5.2, launched 12:34, sampled 22:17. `footprint -s`:

```
Physical footprint:        2.5 GB   (peak 3.6 GB)
IOAccelerator             1386 MB   ← the Rust heap (mimalloc arenas)
MALLOC_LARGE               730 MB   ┐
MALLOC_SMALL               405 MB   ┘ the system C heap
everything else            < 50 MB
```

⚠️ **Correction (2026-08-03): "for us, ~all SQLite" was wrong about `MALLOC_LARGE`.** SQLite page-cache overflow can
only ever land in `MALLOC_SMALL`: the bundled build defines `SQLITE_ENABLE_MEMORY_MANAGEMENT`, so
`pcache1.separateCache = 0`, there is no bulk allocation, and every overflow page is an individual ~4.1 KB
`sqlite3Malloc` — below macOS's 127 KB large-zone threshold. The shared slab this note led to proved it from the other
side: `MALLOC_SMALL` fell 405 → 152 MB (−62%) while `MALLOC_LARGE` moved 730 → 643 MB (−12%), in regions of 9 MB and
2.25 MB. What that 643 MB IS remains unidentified: `idle-cpu-attribution-2026-08-03.md` § "Still open".

WebKit and the compositor were NOT involved: `WebKit malloc` was 4.6 MB, `IOAccelerator (graphics)` 1.3 MB. CPU was 122
minutes over 10 hours.

## Cause 1 — SQLite page cache across many connections (~1.15 GB)

`lsof` showed **156 open SQLite connections**: 57 × `importance-root.db`, 53 × `index-root.db`, 30 ×
`index-smb-…naspi.db`, 10 × `importance-smb-…naspi.db`, 6 × `media-root.db`. Every one ran the writer's
`PRAGMA cache_size = -16384` (16 MiB), a 2.5 GB ceiling.

They accumulate because read connections are **thread-local and live as long as their thread**
(`indexing/read/enrichment.rs`'s `THREAD_CONN`, `ImportanceIndex`'s `READ_CONN`), so the count tracks tokio's
blocking-thread pool (69 threads at sample time), not anything semantic.

**Fixed** in two steps:

1. Split the per-connection budget by role in one place (`sqlite_util::apply_page_cache`): 16 MiB for the single writer
   per DB, a small budget for read-only opens. That dropped the ceiling from 2.5 GB to ~310 MB, but it was a ceiling,
   not a cure: the connections still accumulated, and 310 MB still scaled with a number nothing controls.
2. Made total page memory ONE number: a 64 MiB process-wide slab handed to SQLite via
   `sqlite3_config(SQLITE_CONFIG_PAGECACHE, …)` before the first connection opens. Page memory is now independent of
   connection count and shared dynamically, which also let the read budget go back UP (to 8 MiB per connection, an upper
   bound out of the slab rather than a reservation).

Rationale, the sizing, the ordering guarantee, and the alternative weighed:
`crates/cmdr-index/src/indexing/store/DETAILS.md` § "SQLite page memory is one process-wide slab".

The connection count itself was a separate, real cost: both read paths held a SINGLE thread-local connection, so a
thread alternating between two volumes reopened on every alternation and threw away its `prepare_cached` statements.
They now keep a three-slot LRU (`sqlite_util::ThreadConnCache`), affordable precisely because memory no longer tracks
connection count.

## Cause 2 — a 60-second rescore treadmill

Every ~60 s for the whole session:

```
incremental rescore of 'root' updated 90308 folders
```

The count moved by **2 folders in an hour** (90308 → 90384). Each pass walked the whole index (O(dirs)), rewrote ~90 k
rows, flushed, ran `wal_checkpoint(TRUNCATE)`, then woke the search weight subscriber to reload all **161 094** weights
while the old map was still live. The footprint oscillated 2.6 → 3.0 GB on exactly that cycle.

**Root cause: the dir-changed bus conflated two facts.** A live listing change produces both "these dirs' listings
changed" (small) and "these dirs' recursive sizes need refreshing" (the former plus every ancestor up to `/`, because a
file's size propagates all the way up). `process_fs_event` returned the second and published it, but both bus consumers
expand each entry DOWNWARD — importance into the whole subtree (floor transitions), media into the dir's image children.
So one cargo build deep under `~/projects-git/…` put `/Users` into the batch, and the rescore matched every folder under
the home directory.

**Correction (2026-08-04): the four steps below fixed the cause described above, but not the treadmill.** Prod v0.37.0
still ran a full-walk pass rewriting ~51 k rows, in bursts, for hours. The cause named here (an ancestor riding in via
the size-refresh set) was real and is gone; a SECOND, independent cause produces the same shape — a dotfile written
directly in `~` makes `$HOME` itself an origin, and `$HOME` covers 83% of the volume's directories. Step 4's delta also
never ran in v0.37.0, which was tagged before it landed. What was measured, what was refuted, and what is still open:
`importance-treadmill-2026-08-04.md`.

**Fixed** in three steps:

1. The two facts now travel separately: `DirsChanged.origins` carries only the dirs whose own listings changed, and
   `reconciler::with_ancestor_closure` rebuilds the size-refresh set where it's consumed (the FE emit, the hourglass).
2. `sanitize_incremental_batch` drops paths that floor by path, before the read pool opens and before the walk — the
   idle gate. Machine churn (`target/`, `Library/Caches`, dot-directories) can no longer cost a pass at all.
3. An allocation-free `is_in_changed_subtree` (it built a `format!` needle per folder per changed path).
4. The weight reload the pass woke is now a DELTA: an incremental reports the rows it wrote and the paths it cleared,
   and `search::volumes` patches its map in place instead of re-reading all 161,094 weights. Contract and the
   before/after numbers: `crates/cmdr-index/src/importance/read/DETAILS.md` § The reload contract and
   `crates/cmdr-index/src/importance/scheduler/DETAILS.md` § Throttle.

Contracts, the accepted lossiness, and the guardrails: `crates/cmdr-index/src/importance/scheduler/DETAILS.md` and
`crates/cmdr-index/src/indexing/lifecycle/DETAILS.md`.

## A latent bug this surfaced

Only the cold-start replay loop published on the dir-changed bus. A volume that took the post-scan route
(`run_live_event_loop`) refreshed the UI but never woke the importance rescore or the media live tick, so their derived
data went stale until the next `ScanCompleted`. Every unit test passed on that route.
`every_live_loop_publishes_its_changed_dirs` now scans the `event_loop` sources so a third live loop can't slip past.

## What did NOT matter

Worth recording so the next investigation doesn't re-test them: WebKit, the compositor, GPU surfaces, DOM churn, and the
frontend generally. Consistent with `memory-runaway-rust-heap-2026-07-25.md` — for this process, if the number is big,
it's the Rust heap or SQLite.
