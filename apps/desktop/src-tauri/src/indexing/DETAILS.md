# Drive indexing details (hub)

Read this before any non-trivial work spanning the indexing subsystem: planning, reorganizing, or advising across
areas. This is the map and the cross-cutting depth; each area's own mechanisms live in its `DETAILS.md` (linked below),
and must-know invariants in its `CLAUDE.md`. Single-source: a mechanism is documented in ONE area doc; everywhere else
points to it.

The key UX win: showing directory sizes in listings. Design history is in git (former `docs/specs/drive-indexing/`).

## Subsystem map (what + where)

Background-indexes each volume into its own per-volume SQLite DB with recursive size aggregates. `mod.rs` is a thin
public-API facade; the areas:

- **[`lifecycle/`](lifecycle/DETAILS.md)** — the per-volume registry, `IndexPhase` machine, `IndexManager` coordinator
  (+ its `network_scan` trait-scan dispatch), scan completion, the freshness state machine, the Failed state, the
  lifecycle bus, and `IndexVolumeKind`'s two-axis capability model.
- **[`resources/`](resources/DETAILS.md)** — the global 16 GB memory watchdog, subsystem stop-hooks, retention cap.
- **[`scanner/`](scanner/DETAILS.md)** — the LOCAL guarded parallel walker + scope-aware exclusions.
  **[`network_scanner/`](network_scanner/DETAILS.md)** — the SMB/MTP `Volume`-trait BFS + scan pacing + NAS skips.
- **[`watch/`](watch/DETAILS.md)** — the FS watcher + the event loop (live / replay / verification / storm) + churn.
- **[`reconcile/`](reconcile/DETAILS.md)** — non-destructive rescan, the cost budget, the two verification teeth, the
  per-navigation verifier, the once-a-day shallow sweep, the per-subtree throttle, depth-split routing.
- **[`writer/`](writer/DETAILS.md)** — the single writer thread. **Canonical home for honest sizes, the `dir_stats`
  ledger, coverage epochs, the ID counter, and `WRITER_GENERATION`.** **[`aggregator/`](aggregator/DETAILS.md)** —
  bottom-up dir-stats compute. **[`store/`](store/DETAILS.md)** — the `IndexStore` handle + the SQLite schema.
- **[`read/`](read/DETAILS.md)** — enrichment, IPC queries, expected totals, the hourglass.
  **[`paths/`](paths/DETAILS.md)** — **canonical home for `IndexPathSpace`, the three-path-spaces discipline, routing,
  and firmlink normalization.** **[`events/`](events/DETAILS.md)** — FE payloads, `set_phase_for`, the progress loop.
- **[`transports/`](transports/DETAILS.md)** — per-transport enable + live watch (`smb/`, `mtp/`, `local_external/`).
- **[`tests/`](tests/DETAILS.md)** — whole-pipeline integration + stress tests + the disk-image fixture.

`metadata.rs` is a loose shared leaf (see below). IPC: `commands/indexing.rs`. FE: `src/lib/indexing/`. Search: the
top-level `search/` module (local-disk-only by design; the coupling is the shared `WRITER_GENERATION`, documented in
[`writer/DETAILS.md`](writer/DETAILS.md)).

## The shared metadata leaf (`metadata.rs`)

`MetadataSnapshot` + `extract_metadata()`: the single location for platform-specific metadata extraction (logical /
physical size, mtime, inode, nlink). Used by the scanner, the reconciler, the verifier, and the event loop — a true
four-area primitive, so it stays a loose top-level leaf rather than being homed in one area (which would invert a
dependency). Symlinks get `None` everywhere; files get sizes + inode + nlink; directories get inode but no sizes/nlink.
The inode is what the live rename pre-pass matches against.

## Data flow (the cross-area pipeline)

```
App startup
  |-- init(): register IndexManagerState in Tauri
  |-- start_indexing(): create IndexManager, open SQLite, spawn writer thread
  |-- resume_or_scan(): existing index + journal? -> replay; incomplete/none -> fresh scan
  |                     (macOS FSEvents journal replay; Linux always full rescan)
  |
Full scan (start_scan):
  |-- capture prior-scan calibration BEFORE truncating (for two-tier progress)
  |-- DeleteMeta(scan_completed_at) + Truncate entries/dir_stats
  |-- start the watcher (buffers events), guarded parallel walk -> InsertEntriesV2 -> writer -> SQLite
  |-- partial-agg passes every ~5 s (growing sizes) + progress events every 500 ms
  |-- on UNcancelled complete: persist meta, replay buffered events, compute all aggregates, go live
  |     (a cancelled scan writes NO meta -> heals to a fresh scan on restart)
  |
Live mode:
  |-- FSEvents / inotify -> reconciler (resolve path -> entry id) -> Upsert/Move/Delete -> writer -> SQLite
  |-- reconciler + event loops hold a READ connection; three-phase batch (dir-creates, rename pre-pass, remainder)
  |
Enrichment (per listing):
  |-- enrich_entries_with_index() -> resolve parent once -> batch child dir stats by id -> match by name
  |     (skips when the volume has no registered index; ReadPool, never the lifecycle lock)
  |
Navigation verification (after enrichment):
  |-- trigger_verification(path) -> dedup/debounce -> ReadPool DB snapshot vs read_dir disk snapshot -> corrections
```

Which area owns each stage: scan discovery → [`scanner/`](scanner/DETAILS.md) (local) and
[`network_scanner/`](network_scanner/DETAILS.md) (SMB/MTP); live change ingestion → [`watch/`](watch/DETAILS.md);
resync → [`reconcile/`](reconcile/DETAILS.md); persistence + size compute → [`writer/`](writer/DETAILS.md) +
[`aggregator/`](aggregator/DETAILS.md); serving sizes → [`read/`](read/DETAILS.md); path mapping →
[`paths/`](paths/DETAILS.md); lifecycle of it all → [`lifecycle/`](lifecycle/DETAILS.md).

## Cross-cutting patterns

- **Disposable cache.** The index DB is a cache, not a source of truth. Schema-version mismatch or corruption triggers
  delete + rebuild; there are no online migrations and no user-facing errors for DB issues. A Stale, SMB, or MTP index
  NEVER drives a destructive op — copy/move/delete re-stat live; the index is consulted only for non-load-bearing size
  estimates, each with an explicit "unknown" fallback. What counts as corruption (never widen it) and the schema:
  [`store/DETAILS.md`](store/DETAILS.md).
- **Per-volume everything.** Every invariant holds per `VolumeId`, keyed independently. The registry is the authority;
  reads route through the per-volume `ReadPool`, never under the lifecycle lock. See
  [`lifecycle/DETAILS.md`](lifecycle/DETAILS.md).
- **Two records of the pipeline phase.** A global app-wide `DEBUG_STATS` ring (debug window) and a per-volume
  `index-phase-changed` event; `set_phase_for` does both so they can't drift. The phase EVENT lives in
  [`events/DETAILS.md`](events/DETAILS.md); the phase MACHINE (`IndexPhase`) in [`lifecycle/DETAILS.md`](lifecycle/DETAILS.md).

## Canonical homes for the load-bearing mechanisms

When a claim about one of these belongs in a doc, it belongs in THAT doc; point everywhere else:

- Honest sizes, the `dir_stats` ledger (four hard rules), coverage epochs, single-writer discipline, the ID counter,
  `WRITER_GENERATION` → [`writer/DETAILS.md`](writer/DETAILS.md).
- The per-volume registry, `IndexPhase`, lock-first start, drop-guard-before-drain, typed-kind rescan routing, the
  freshness state machine, the Failed state, `IndexVolumeKind` axes → [`lifecycle/DETAILS.md`](lifecycle/DETAILS.md).
- `IndexPathSpace`, the three-path-spaces discipline, mount-relative strip, routing, firmlink canonical form →
  [`paths/DETAILS.md`](paths/DETAILS.md).
- The SQLite schema, `name_folded` / collation, what counts as corruption → [`store/DETAILS.md`](store/DETAILS.md).
- The reconcile cost budget, the two verification teeth, the once-a-day sweep → [`reconcile/DETAILS.md`](reconcile/DETAILS.md).
