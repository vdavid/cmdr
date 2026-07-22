# Drive indexing subsystem

Background-indexes each volume (local disk, local external, SMB, MTP) into its own per-volume SQLite DB with recursive
size aggregates. The headline UX win: showing directory sizes in listings. `mod.rs` is a thin public-API facade; the
real code is in the area subdirs below.

## The one cross-area invariant

**Every invariant holds PER volume id.** The subsystem was generalized from one hardwired volume to a registry keyed by
`VolumeId`, so multiple volumes index concurrently without corrupting each other: single-writer-per-DB, lock-first
reservation, reads-via-`ReadPool`-never-under-the-lifecycle-lock, freshness, and the `dir_stats` ledger all hold
independently per key. When you touch any area, keep the reasoning per-volume.

## Areas (routing map)

Each area subdir has its own `CLAUDE.md` (must-knows) + `DETAILS.md` (depth). Touch a dir and its `CLAUDE.md`
autoloads; read it before non-trivial work there.

- **[`lifecycle/`](lifecycle/CLAUDE.md)** — the registry + `IndexPhase` machine + `IndexManager` coordinator + scan
  completion + freshness + failure + the lifecycle bus. Owns the per-volume registry and lock discipline.
- **[`resources/`](resources/CLAUDE.md)** — process-wide caps: the 16 GB memory watchdog, subsystem stop-hooks, and the
  external-index-DB retention cap.
- **[`scanner/`](scanner/CLAUDE.md)** — the LOCAL guarded parallel walker (hang-tolerant) + the scope-aware exclusion
  policy. **[`network_scanner/`](network_scanner/CLAUDE.md)** — the SMB/MTP `Volume`-trait BFS scanner + scan pacing +
  NAS system-dir skips.
- **[`watch/`](watch/CLAUDE.md)** — the local FS watcher (FSEvents/inotify) + the event loop (live / replay /
  verification / storm) + the churn-monitor spike.
- **[`reconcile/`](reconcile/CLAUDE.md)** — keep the index matching disk: event-triggered reconciler, full local
  rescan-in-place, and the per-navigation verifier.
- **[`writer/`](writer/CLAUDE.md)** — the single writer thread per DB. **Owns the `dir_stats` ledger, honest sizes, and
  coverage epochs** (canonical). **[`aggregator/`](aggregator/CLAUDE.md)** — bottom-up dir-stats computation.
  **[`store/`](store/CLAUDE.md)** — the `IndexStore` handle + SQLite schema.
- **[`read/`](read/CLAUDE.md)** — serve sizes back: enrichment (the hot path), IPC queries, write-op expected totals,
  the "size updating" hourglass. **[`paths/`](paths/CLAUDE.md)** — path->volume routing, `IndexPathSpace`, firmlink
  normalization. **[`events/`](events/CLAUDE.md)** — FE event payloads + the scan-progress loop + partial aggregation.
- **[`transports/`](transports/CLAUDE.md)** — per-transport enable + live watch: `smb/`, `mtp/`, `local_external/`.
- **[`tests/`](tests/CLAUDE.md)** — whole-pipeline integration + stress tests + the disk-image fixture.

`metadata.rs` is a loose shared leaf: the single platform-specific metadata-extraction primitive (`extract_metadata`,
`MetadataSnapshot`) used by scanner, reconcile, watch, and verifier. Homing it in any one area would invert a
dependency.

IPC commands are in `commands/indexing.rs`; the FE is `src/lib/indexing/`; search is the top-level `search/` module.

## Docs

Architecture map, data flow, the two-axis capability model, and the disposable-cache pattern, plus the canonical homes
for cross-cutting mechanisms: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
