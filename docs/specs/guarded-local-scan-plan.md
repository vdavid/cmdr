# Guarded local scan: hang-tolerant, jwalk-free parallel walker

Status: planned (branch `david/scan-mount-timeout`). Owner: David + agent.

## Problem

A full-disk scan of Macintosh HD freezes indefinitely (observed: frozen at 505,687 entries for 10+ min, ETA
climbing). Root cause: the local scan descends into macOS File Provider mounts under `~/Library/CloudStorage/`
(Dropbox, Google Drive, **MacDroid** — a disconnected Android phone) and `~/Library/Mobile Documents/` (iCloud). A
`readdir` on a disconnected provider blocks on a network materialization that never returns (`fileproviderd … FP -1004`).

Two independent local walkers both hang:

1. **Fresh/first scan** — `scanner/mod.rs`, jwalk. jwalk owns its `readdir` (no timeout hook) and delivers results
   through a **strict-ordered** queue (`read_dir_iter.rs`, `Ordering::Strict`), so one hung read blocks delivery of
   every already-completed result behind it → the whole scan parks. This is the observed freeze.
2. **Reconcile rescan** — `local_reconcile.rs`, a **serial** BFS over `std::fs::read_dir`. Runs on every rescan of a
   populated index (the common case after first launch). Serial + no timeout ⇒ one hung read freezes the entire rescan.

The network scanner (`volume_scanner.rs`) already solves this for SMB/MTP: bounded-concurrency walk with a per-listing
`LIST_TIMEOUT` (120 s) and a consecutive-failure abort. The local scanners have none of it.

## Goals

1. **Never hang.** A hung directory read is abandoned after a wall-clock timeout; the rest of the scan proceeds.
2. **Index cloud metadata without downloads.** Online-only (dataless) files are indexed with correct name/size/mtime
   (via `readdir` + `lstat` only — neither triggers content materialization). Only a genuinely unreachable provider
   subtree is skipped (and skipped *honestly* — not marked listed, so freshness stays truthful).
3. **One hung dir blocks ≤1 worker for ≤ the timeout; other workers keep going.** (David's explicit requirement.)
4. **Shed jwalk** and its structural flaws (strict-ordered delivery; the global `HashMap<PathBuf,i64>` parent map),
   replacing both local walkers with one owned, guarded parallel walker engine.

Non-goals: touching the SMB/MTP `volume_scanner` path (already guarded); a guaranteed wall-clock speedup (see § Perf).

## Timeout value

`LOCAL_LIST_TIMEOUT = 15 s` (David's call). Rationale: healthy local `readdir` returns in µs (never arms); a
working-but-slow online provider lists a dir in well under a second, so 15 s sits comfortably above legitimate slowness
while abandoning a dead mount quickly. A timed-out dir prunes its subtree, so the dead-mount cost is a handful of
frontier dirs, not thousands. Contrast SMB's 120 s, tuned for WAN-latency listings.

## Design: the guarded parallel walker engine

New module `indexing/scanner/walker/` (name TBD): a bounded, hang-tolerant, parallel directory walker that both the
fresh scan and reconcile drive with different per-directory visitors.

### Core model — carry `parent_id`, no path map

Each unit of work is "read directory D, whose entry id is `id_D`". A worker:
1. `readdir(D)` (guarded — see below).
2. For each child: `lstat` it, allocate a fresh id from the shared `AtomicI64` counter, build the visitor's output with
   `parent_id = id_D` **known locally** (no lookup), and — if the child is a directory — enqueue a read task for it
   carrying its freshly-allocated id.

This deletes `ScanContext.dir_ids` (`store/mod.rs`, the `HashMap<PathBuf,i64>` holding every dir path for the whole
volume) and the per-entry `to_string_lossy` + normalize + hash that parent resolution costs today, and it removes the
post-walk `listed_paths → ids` second pass (each dir knows its own id, so it marks itself listed inline). Parent is
always known before children, so there's no ordering dependency and no "parent not found, skip" drop path.

### Concurrency + the timeout mechanism (watchdog + abandon/replace)

A worker pool alone does **not** give a timeout — a worker in `readdir` blocks like any thread. The timeout comes from
making the blocking read *abandonable*:

- N persistent worker threads pull read tasks from an MPMC queue and call `readdir` **directly** (cheap; no
  thread-per-read for the bulk — that would be ruinous at ~600k dirs).
- A **watchdog** records each worker's `(dir, start_instant)` before its read and clears it after. Every ~1 s it scans
  for a worker whose read has exceeded `LOCAL_LIST_TIMEOUT` and, for that worker:
  1. records the dir as **timed-out** (an error outcome — the subtree is pruned, the dir is NOT marked listed), and
  2. spawns a **replacement worker** so pool capacity is restored.
  The stuck worker is *abandoned*; it exits on its own when the FP layer finally errors the syscall. This is bounded and
  self-clearing: only dirs that actually exceed 15 s trigger it, and each prunes its subtree, so outstanding-abandoned
  ≈ number of independently-hung frontier dirs (a handful). An OS thread in a syscall can't be force-killed; abandon +
  replace gives the same user-visible behavior.
- Worker stacks: 8 MB (matching `file_system/sync_status.rs`) — File Provider `readdir`/`lstat` can descend deep XPC
  override chains that overflow rayon's 2 MB default. **Never rayon for FP-touching reads** (project rule).
- Concurrency cap: mirror `volume_scanner`'s reasoning; start at a similar bound, tunable.
- Cancellation: cooperative `AtomicBool` checked in the worker loop (preserves `ScanHandle::cancel`).

### Injectable reader (testability)

The engine takes a `ReadDirFn` (trait object or fn pointer) instead of calling `std::fs::read_dir` directly, so tests
inject a reader that blocks/sleeps for a chosen path. This makes the timeout, abandon/replace, and honest-skip behavior
unit-testable with **no real hung mount**. Production wiring passes the real `read_dir`.

### Exclusions, firmlinks, hardlink dedup, epoch/marking

Preserve exactly: `should_exclude` gate (single source), firmlink normalization, canonicalization-alias skip, one global
`seen_inodes` hardlink dedup, `physical_size` accounting rules (dirs/symlinks/2nd+ hardlinks contribute 0), the
mark→aggregate ordering invariant (a dir is marked listed only if its read succeeded; timed-out/errored dirs stay
`listed_epoch = 0` = honest unknown), and the `EmptyRoot` typed guard (never write `scan_completed_at` for a
zero-child root).

## Two visitors on one engine

- **Fresh scan visitor**: insert every child as a new `EntryRow` (today's `run_scan` body), batch to the writer.
- **Reconcile visitor**: diff each dir's live children against the DB (today's `local_reconcile` body), emit
  add/update/delete.

The engine owns traversal + guarding + parent_id/ids + listed-marking; visitors own per-dir semantics. This unifies the
two walkers so hang-tolerance (and future work) lands once.

## Test plan (TDD, red first)

New test classes (the current suite has zero of these):

1. **Hang tolerance** (engine, injected blocking reader):
   - a dir whose read blocks > timeout is abandoned within ~timeout; the scan completes; sibling/unrelated subtrees are
     fully indexed. Assert wall-clock << block duration.
   - multiple independently-hung dirs each cost ≤ timeout and don't starve the pool (replacement workers keep capacity).
2. **Honest skip**: a timed-out dir is **not** marked listed (`listed_epoch` stays 0), its subtree is absent, and
   `scan_completed_at` handling is unaffected for the rest. Pins the freshness-honesty invariant.
3. **Parallel parent-id correctness**: multi-threaded runs produce identical parent/child/id structure to a serial
   reference; add a **differential test** (engine output vs a plain `std` recursive walk over a generated tree) to
   broadly guard id/parent regressions.
4. **Reconcile hang tolerance**: the reconcile visitor over a hung tree abandons and keeps the prior index honest.

Keep every existing scanner + local_reconcile test green throughout (counts, sizes, ids, epoch stamping, hardlink dedup,
symlinks, cancellation, empty dir, subtree, empty-root, reconcile diff cases, reconcile-after-jwalk no-op — this last one
adapts to "reconcile-after-fresh-scan").

## Perf / resource expectations (§ Perf)

- **Pool ≈ rayon** for our pipeline: the throughput ceiling is the single SQLite writer thread (measured — see
  `volume_scanner::SCAN_CONCURRENCY` note) and the per-entry work, not listing parallelism, so read-scheduler choice
  isn't on the critical path.
- **Reliable wins: memory + CPU.** Dropping the whole-volume `HashMap<PathBuf,i64>` cuts hundreds of MB of PathBufs
  (relevant to the 16 GB indexing watchdog) and the per-entry parent-resolution hashing; dropping the post-walk
  resolution pass removes a second hashing pass + its mutex.
- **Wall-clock: not promised.** May stay flat if writer-bound. Capture a before/after benchmark (scan of `/`: wall-clock,
  peak RSS, CPU) to prove the memory/CPU win rather than assert it. Writer-side speedups (bigger batched inserts) are a
  separate, opt-in follow-up.

## Execution stages (each ends green: `pnpm check --fast` scoped, then a milestone `pnpm check`)

1. **Engine + hang tests (red→green)**: build `walker/` with the injectable reader, worker pool, watchdog + abandon/
   replace, cooperative cancel. Unit tests 1–3 above. No production wiring yet. **← check in here.**
2. **Port fresh scan**: reimplement `scan_volume`/`scan_subtree` bodies as a fresh-scan visitor on the engine; delete
   `ScanContext.dir_ids` and the `listed_paths` second pass; keep all existing scanner tests green; add honest-skip test.
3. **Port reconcile**: reimplement `local_reconcile` as a reconcile visitor on the engine; keep all reconcile tests
   green; add reconcile hang test.
4. **Remove jwalk**: drop the dependency from `Cargo.toml` (verify no other consumer — grep showed only the scanner);
   `cargo deny` + full `pnpm check --include-slow`.
5. **Benchmark + docs**: before/after numbers into `docs/notes/`; update `scanner/DETAILS.md` + `indexing/CLAUDE.md`
   must-knows (guarded walker, 15 s timeout, honest-skip, parent_id model); this spec's durable intent moves into those.

## Open questions / risks

- Worker-pool concurrency default: reuse `volume_scanner`'s bound or tune separately for local. (Decide with the
  benchmark.)
- Does any consumer depend on jwalk's ordered delivery beyond parent-before-child? (Grep says no; the differential test
  guards it.)
- Reconcile currently resolves against the DB per dir; confirm the parent_id engine model composes with DB-diff
  semantics before the port (stage 3 spike).
