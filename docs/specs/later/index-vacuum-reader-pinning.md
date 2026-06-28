# Index DB freelist: reader-pinned incremental vacuum can't reclaim during a session

Status: deferred. Captured from the index-disk-reclaim work (2026-06-28); see the shipped
`local-reconcile-rescan-plan.md` for the two fixes that made this low-priority.

## The diagnosis

The per-volume index DB runs `auto_vacuum = INCREMENTAL` with a 30s maintenance tick (`state.rs`) that sends
`IncrementalVacuum` + `wal_checkpoint(TRUNCATE)` to the writer (`writer/maintenance.rs`). The intent: drain
the freelist (free pages from deletes/rescans) back to the OS within ~10 min.

In practice, on the `root` volume it reclaims almost nothing during a live session. Measured on a 2.5 GB
`index-root.db` with ~1.6 GB freelist: over 5 minutes of live running, the file shed **40 KB** (~8 KB/min); at
that rate the 1.6 GB would take ~135 days. Independently of the maintenance tick firing.

Cause: in WAL mode, the main DB file can only be truncated down to the size the **oldest live read snapshot**
still needs, and a checkpoint can't truncate the WAL past the oldest reader. The `root` index always has
long-lived readers — open panes' enrichment `ReadPool` connections and the live event loop's read connection.
While any of them holds a snapshot older than the mass-delete that freed the pages, `incremental_vacuum`
returns ~nothing and the file stays bloated. The freed pages get recycled for new writes (so `freelist_count`
churns slowly) but are never returned to the OS. (Note: the WAL hovering at ~4 MB is just the default
`wal_autocheckpoint = 1000` high-water mark, NOT evidence of a stuck reader — the real signal is
`incremental_vacuum` freeing ~0 pages despite a large freelist.)

## Why deferred

The shipped reclaim work removed the two SOURCES of large freelist:
- schema-bump rebuilds now recreate the DB file fresh (zero freelist) instead of `DROP TABLE` on the live file;
- local rescans reconcile in place instead of truncate + rebuild.

So the acute multi-GB bloat no longer forms. What remains is small, steady live-event churn (FSEvents
deletes/moves over a long session), which the incremental vacuum reclaims at whatever rate the readers allow.
Building reader-quiesce/barrier machinery into this notoriously-hard area to reclaim that residual isn't worth
it now. Revisit only if a long-running session is observed materially bloating from live churn — with data.

## Candidate fix shapes (when revisited)

The lever is: ensure no connection holds a read snapshot older than the vacuum at the moment of checkpoint.
Two flavors, pick by what a fresh diagnosis shows:

1. **Surgical (if one connection holds a continuous snapshot).** Identify the persistent reader holding a
   long-lived read transaction (suspects: the live event loop's read connection, or a `ReadPool` connection
   left mid-transaction) and make it release between operations. Then the existing 30s tick reclaims as
   designed — no new machinery, no UX cost.
2. **Maintenance barrier (if it's checkpoint starvation from many overlapping short reads).** Briefly quiesce
   enrichment + reconciler reads (flip a flag, let in-flight `ReadPool::with_conn` calls drain — that closure
   is the natural choke point), run `wal_checkpoint(TRUNCATE)` + `incremental_vacuum`, then resume. Schedule on
   an idle trigger so the sub-ms read pause is invisible.

A startup `VACUUM` is the wrong tool (adds 1-2s to launch; and it would itself be blocked by the same readers
during a session). The diagnosis to run first: enable the writer's `BEGIN IMMEDIATE`/`COMMIT` debug logging to
see if the root writer sits in an open transaction in steady state, plus a careful external
`wal_checkpoint(TRUNCATE)` probe that reports blocked frames.
