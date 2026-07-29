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
MALLOC_SMALL               405 MB   ┘ the system C heap — for us, ~all SQLite
everything else            < 50 MB
```

WebKit and the compositor were NOT involved: `WebKit malloc` was 4.6 MB, `IOAccelerator (graphics)` 1.3 MB. CPU was
122 minutes over 10 hours.

## Cause 1 — SQLite page cache across many connections (~1.15 GB)

`lsof` showed **156 open SQLite connections**: 57 × `importance-root.db`, 53 × `index-root.db`, 30 ×
`index-smb-…naspi.db`, 10 × `importance-smb-…naspi.db`, 6 × `media-root.db`. Every one ran the writer's
`PRAGMA cache_size = -16384` (16 MiB), a 2.5 GB ceiling.

They accumulate because read connections are **thread-local and live as long as their thread**
(`indexing/read/enrichment.rs`'s `THREAD_CONN`, `ImportanceIndex`'s `READ_CONN`), so the count tracks tokio's
blocking-thread pool (69 threads at sample time), not anything semantic.

**Fixed** by splitting the budget by role in one place (`sqlite_util::apply_page_cache`): 16 MiB for the single writer
per DB, 2 MiB for read-only opens. Same 156 connections now cap at ~310 MB. Rationale and the write budget's coupling
to `wal_autocheckpoint`: `apps/desktop/src-tauri/src/indexing/store/DETAILS.md`.

This is a ceiling, not a cure — the connections still accumulate; only their unit cost is bounded. If connection count
itself ever needs fixing, that's a separate change to the thread-local lifetime.

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
expand each entry DOWNWARD — importance into the whole subtree (floor transitions), media into the dir's image
children. So one cargo build deep under `~/projects-git/…` put `/Users` into the batch, and the rescore matched every
folder under the home directory.

**Fixed** in three steps:

1. The two facts now travel separately: `DirsChanged.origins` carries only the dirs whose own listings changed, and
   `reconciler::with_ancestor_closure` rebuilds the size-refresh set where it's consumed (the FE emit, the hourglass).
2. `sanitize_incremental_batch` drops paths that floor by path, before the read pool opens and before the walk — the
   idle gate. Machine churn (`target/`, `Library/Caches`, dot-directories) can no longer cost a pass at all.
3. An allocation-free `is_in_changed_subtree` (it built a `format!` needle per folder per changed path).

Contracts, the accepted lossiness, and the guardrails:
`apps/desktop/src-tauri/src/importance/scheduler/DETAILS.md` and
`apps/desktop/src-tauri/src/indexing/lifecycle/DETAILS.md`.

## A latent bug this surfaced

Only the cold-start replay loop published on the dir-changed bus. A volume that took the post-scan route
(`run_live_event_loop`) refreshed the UI but never woke the importance rescore or the media live tick, so their derived
data went stale until the next `ScanCompleted`. Every unit test passed on that route.
`every_live_loop_publishes_its_changed_dirs` now scans the `event_loop` sources so a third live loop can't slip past.

## What did NOT matter

Worth recording so the next investigation doesn't re-test them: WebKit, the compositor, GPU surfaces, DOM churn, and
the frontend generally. Consistent with `memory-runaway-rust-heap-2026-07-25.md` — for this process, if the number is
big, it's the Rust heap or SQLite.
