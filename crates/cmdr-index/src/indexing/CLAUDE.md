# Drive indexing subsystem

Background-indexes each volume (local disk, local external, SMB, MTP) into its own per-volume SQLite DB with recursive
size aggregates. The headline UX win: showing directory sizes in listings. The crate root re-exports what a host may
rely on (`crates/cmdr-index/CLAUDE.md`); `mod.rs` here just declares the areas below, where the real code is.

## The one cross-area invariant

**Every invariant holds PER volume id.** The subsystem was generalized from one hardwired volume to a registry keyed by
`VolumeId`, so multiple volumes index concurrently without corrupting each other: single-writer-per-DB, lock-first
reservation, reads-via-`ReadPool`-never-under-the-lifecycle-lock, freshness, and the `dir_stats` ledger all hold
independently per key. When you touch any area, keep the reasoning per-volume.

## Rebuild, don't migrate

Pre-launch, we do NOT preserve index contents across a format or scope change. The drive index, media index, and
importance index are disposable caches, and a full rescan is acceptable (~10 minutes on a big NAS), so invalidate and
rebuild. ❌ Don't build machinery to preserve them unless David says so for a specific case: a migration is a permanent
maintenance cost for rows nobody would miss, and it only fixes the one thing it targets. The exclusion-list rebuild is
the pattern to copy (`network_scanner/DETAILS.md` § "Rebuilding an index that predates the current list").

## Areas (routing map)

Each area subdir has its own `CLAUDE.md` (must-knows) + `DETAILS.md` (depth). Touch a dir and its `CLAUDE.md` autoloads;
read it before non-trivial work there.

- **`handle/CLAUDE.md`** — `Index`, the public API: the handle the app holds and every method it can call. ❌ App code
  never reaches past it into an area below. The item-by-item audit that decided the surface is its `DETAILS.md`.
- **`host/CLAUDE.md`** — the four seams the subsystems reach their host through: the injected tokio runtime, the
  background-work priority policy, the volume registry + mount classification, and the config. ❌ Anything the app must
  answer arrives here, never as a `crate::<app module>` import. These three trees reference no app module at all.
- **`lifecycle/CLAUDE.md`** — the registry + `IndexPhase` machine + `IndexManager` coordinator + scan completion +
  freshness + failure + the lifecycle bus. Owns the per-volume registry, lock discipline, and the master drive-indexing
  switch (`indexing.enabled`), a hard gate over every per-drive choice.
- **`resources/CLAUDE.md`** — process-wide caps: the 16 GB memory watchdog, subsystem stop-hooks, and the
  external-index-DB retention cap.
- **`scanner/CLAUDE.md`** — the LOCAL guarded parallel walker (hang-tolerant) + the scope-aware exclusion policy.
  **`network_scanner/CLAUDE.md`** — the SMB/MTP `Volume`-trait BFS scanner + scan pacing + NAS system-dir skips.
- **`watch/CLAUDE.md`** — the local FS watcher (FSEvents/inotify) + the event loop (live / replay / verification /
  storm) + the churn-monitor spike.
- **`reconcile/CLAUDE.md`** — keep the index matching disk: event-triggered reconciler, full local rescan-in-place, and
  the per-navigation verifier.
- **`writer/CLAUDE.md`** — the single writer thread per DB. **Owns the `dir_stats` ledger, honest sizes, and coverage
  epochs** (canonical). **`aggregator/CLAUDE.md`** — bottom-up dir-stats computation. **`store/CLAUDE.md`** — the
  `IndexStore` handle + SQLite schema.
- **`read/CLAUDE.md`** — serve sizes back: enrichment (the hot path), IPC queries, write-op expected totals, the "size
  updating" hourglass, and the search COVERAGE frontier (what the index can't answer for yet; the walk that fills it in
  is `lifecycle/cover/`). **`paths/CLAUDE.md`** — path->volume routing, `IndexPathSpace`, firmlink normalization.
  **`events/CLAUDE.md`** — the `EventSink` seam + typed `IndexEvent` + the scan-progress loop + partial aggregation. The
  frontend payloads live app-side in `events/index_mapping.rs`.
- **`transports/CLAUDE.md`** — per-transport enable + live watch: `smb/`, `mtp/`, `local_external/`.
- **`tests/CLAUDE.md`** — whole-pipeline integration + stress tests + the disk-image fixture.

Two loose shared leaves sit beside the areas, both because homing them in any one area would invert a dependency:

- `metadata.rs`: the single platform-specific metadata-extraction primitive (`extract_metadata`, `MetadataSnapshot`),
  used by scanner, reconcile, watch, and verifier.
- `volume.rs`: a volume's identity — `VolumeId`, `ROOT_VOLUME_ID`, and `IndexVolumeKind` with its pure capability
  predicates. ❌ Don't put these back in `lifecycle/state.rs`: identity is what everything needs, the registry is what
  only `lifecycle` needs, and merging them welds the whole subsystem into one cycle. Nothing below `lifecycle` should
  import `lifecycle::state`.

All three live host-side, in the app: IPC commands in `apps/desktop/src-tauri/src/commands/indexing.rs`, the frontend in
`apps/desktop/src/lib/indexing/`, and search in `apps/desktop/src-tauri/src/search/`.

## Docs

Architecture map, data flow, the two-axis capability model, and the disposable-cache pattern, plus the canonical homes
for cross-cutting mechanisms: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing,
or advising.
