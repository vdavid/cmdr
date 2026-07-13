# Media index subsystem

Image-ML enrichment: makes a volume's images searchable by their content. A read-consumer of `indexing/`, sibling to
`importance/` and `search/`. **M1 (this slice): OCR-text search only, local volumes only, off by default, fake OCR
backend.** Full plan: [`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md). Depth,
port rationale, GC safety argument, and schema: [DETAILS.md](DETAILS.md).

## Module map

- `predicate.rs` — the PURE image-qualification predicate (`qualify_dir`): images vs non, Live Photos, `.aae` sidecars,
  RAW+JPEG pairs. Sibling-aware, unit-tested.
- `store/` — per-volume `media.db` (ported from `importance/store/`): `media_status` (path-keyed) + `media_ocr` (FTS5) +
  `meta`, the disposable-cache discipline, and the `needs_enrichment` staleness predicate.
- `writer.rs` + `writer_registry.rs` — the ONE writer thread per volume and its lazy registry.
- `backend/` — the `VisionBackend` seam + the deterministic `FakeVisionBackend`. The real objc2-vision OCR impl is the
  NEXT slice; it implements this trait.
- `scheduler/` — the bus-driven, coalesced pass (`PassCoordinator` clone) and its registry-free walk+enrich+GC core
  (`enrich.rs`).
- `read/` — the `MediaIndex` consumer read API (OCR search + `build_ocr_match_query`), the ONLY consumer entry point.
- `gate.rs` — the master-toggle + emergency-stop atomics the scheduler gates on.

## Must-knows

- **This is a deliberate PORT of `importance/`** (store, writer registry, scheduler, coalescer, read API). Mirror its
  patterns; don't re-derive. Read `importance/CLAUDE.md` before changing structure here.
- **Path-keyed, disposable cache.** Rows key on the index's path identity (no stable entry id); a schema-version bump or
  corruption delete-and-recreates `media.db` (no migrations). Staleness is `(path, mtime, size)` from the index row PLUS
  the OS/Vision engine stamp — see `needs_enrichment`.
- **NEVER persist the lifecycle-bus `generation`.** It's a transient in-memory wake counter that resets to 1 every
  launch; there is deliberately NO per-row scan-generation column (plan Decision 3). Staleness needs none.
- **GC is deletion-driven AND edge-triggered — a data-safety line.** A pass runs (and GCs) ONLY on a `Completed` bus
  edge (consumed via `borrow_and_update`, NEVER a `borrow()` poll) or the Fresh registry sweep. The `Completed` fires
  post-writer-flush, so the tree is whole; a mid-`Scanning` truncate window never triggers a sweep, so GC can't wipe
  rows for files that still exist. Don't switch the consumption to a poll, and don't GC on volume-absence.
- **The lifecycle bus is documented in [`indexing/DETAILS.md`](../indexing/DETAILS.md)** (single-source) — link it, don't
  re-document the mechanism here.
- **Inference sits behind `VisionBackend`.** M1 wires `FakeVisionBackend` (zero FFI) so the scheduler/store/GC are fully
  testable and shippable off-by-default; the real objc2-vision backend drops in behind the same seam with no change
  above it. When it lands: dedicated OS threads + `objc2::rc::autoreleasepool`, per-block `// SAFETY:` (`src-tauri/CLAUDE.md`).
- **Off by default + shared memory ceiling.** The master toggle (`mediaIndex.enabled`) defaults off; the scheduler
  no-ops until it's on. Cancellation hooks into the EXISTING indexing memory watchdog via
  `indexing::register_subsystem_stop_hook` — do NOT stand up a second 16 GB ceiling over the same resident pool.
- **`search/` reaches `media.db` ONLY through `MediaIndex`** (plan Decision 8) — no raw `rusqlite` dep, or the
  collation/one-writer invariants leak.

## Not yet (later slices)

Real objc2-vision OCR FFI; the search/query-ui frontend + thumbnail grid; the settings toggle UI + E2E; SMB/MTP
enrichment (M1.5); tags, embeddings, faces (M2+).
