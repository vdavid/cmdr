# Media index subsystem

Image-ML enrichment: makes a volume's images searchable by their content. A read-consumer of `indexing/`, sibling to
`importance/` and `search/`. **OCR-text search only, off by default. Local volumes enrich by default; opt-in SMB volumes
conservatively (M1.5); MTP never background-sweeps. Real macOS Vision OCR in production; a fake for tests.** Plan:
[`docs/specs/media-ml-index-plan.md`](../../../../../docs/specs/media-ml-index-plan.md). Port rationale, the network-fetch
decision, GC safety, FFI discipline, and schema: [DETAILS.md](DETAILS.md).

## Module map

- `predicate.rs` — the PURE image-qualification predicate (`qualify_dir`): images vs non, Live Photos, `.aae` sidecars,
  RAW+JPEG pairs. Sibling-aware, unit-tested.
- `store/` — per-volume `media.db` (ported from `importance/store/`): `media_status` (path-keyed) + `media_ocr` (FTS5) +
  `meta`, and the `needs_enrichment` staleness predicate.
- `writer.rs` + `writer_registry.rs` — the ONE writer thread per volume and its lazy registry.
- `backend/` — the `VisionBackend` seam, the deterministic `FakeVisionBackend`, and (macOS) `vision.rs`, the real
  `objc2-vision` OCR backend (decode + recognize on a dedicated 8 MB-stack OS thread; see DETAILS).
- `scheduler/` — the bus-driven, coalesced pass (`PassCoordinator` clone) + its registry-free walk+enrich+GC core
  (`enrich.rs`); routes each volume by kind (local + opted-in SMB enrich; MTP never background-swept).
- `network/` — SMB byte-fetch enrichment (M1.5): the OS-mount fetcher (`fetch.rs`), the conservative-fetch policy
  (`policy.rs`: idle/bandwidth/override, pure), the settings-seeded opt-in/override/paused state (`config.rs`), the
  disconnect-safe pass (`enrich.rs`). `foreground.rs` is the app-wide idle signal it gates on.
- `read/` — the `MediaIndex` consumer read API (OCR search + `build_ocr_match_query`), the ONLY consumer entry point.
- `commands.rs` — the thin IPC surface (OCR search, per-volume coverage signal, opt-in/override setters, `cmdr-media://`
  thumbnail tokens); `gate.rs` — the master-toggle + emergency-stop atomics the scheduler gates on.

## Must-knows

- **This is a deliberate PORT of `importance/`** (store, writer registry, scheduler, coalescer, read API). Mirror its
  patterns; don't re-derive. Read `importance/CLAUDE.md` before changing structure here.
- **Path-keyed, disposable cache.** Rows key on the index path identity (no stable entry id); a schema bump or corruption
  delete-and-recreates `media.db` (no migrations). Staleness is `(path, mtime, size)` PLUS the OS/Vision engine stamp
  (`needs_enrichment`).
- **NEVER persist the lifecycle-bus `generation`** (a transient wake counter, resets to 1 each launch); there is
  deliberately NO per-row scan-generation column (plan Decision 3). The bus mechanism is single-sourced in
  [`indexing/DETAILS.md`](../indexing/DETAILS.md) — link it, don't re-document.
- **GC is deletion-driven AND edge-triggered — a data-safety line.** A pass GCs ONLY on a `Completed` bus edge (consumed
  via `borrow_and_update`, NEVER a `borrow()` poll) or the Fresh sweep — post-writer-flush, so the tree is whole. Don't
  switch to a poll, and don't GC on volume-absence (a mid-`Scanning` truncate or a disconnect must never sweep).
- **Inference sits behind `VisionBackend`.** Production selects the real macOS `VisionOcrBackend` in `scheduler::start`;
  every test injects `FakeVisionBackend` (zero FFI) via `MediaScheduler::new`, never `start` (off-macOS also falls back to
  the fake). The real backend runs ALL Vision/ImageIO on ONE dedicated 8 MB-stack OS thread (never rayon/small stacks for
  macOS frameworks — `src-tauri/CLAUDE.md`); a hostile image returns a typed `VisionError`, never a panic/hang. It decodes
  from `ImageInput.bytes` when set (network pre-fetch), else reads `path` (local). FFI/threading depth: DETAILS.
- **Off by default + shared memory ceiling.** The master toggle (`mediaIndex.enabled`) defaults off; the scheduler
  no-ops until on. Cancellation hooks the EXISTING indexing memory watchdog (`indexing::register_subsystem_stop_hook`) —
  do NOT stand up a second 16 GB ceiling over the same pool.
- **A disconnect is NOT a bad file (a data-safety line).** A mid-pass SMB unmount PAUSES the pass (keeps completed rows,
  no GC per above); it writes NO `Failed` for the in-flight image. `Failed` is reserved for a genuinely bad file (good
  read, decode/OCR failure). The byte-fetch, idle/bandwidth policy, and override live in [DETAILS.md](DETAILS.md)
  § "Network-volume enrichment (M1.5)".
- **`search/` reaches `media.db` ONLY through `MediaIndex`** (plan Decision 8) — no raw `rusqlite` dep, or the
  collation/one-writer invariants leak. `media_index` commands register in BOTH `ipc.rs` and `ipc_collectors.rs` (missing
  from either breaks the typed bindings — regen with `pnpm bindings:regen`).

## Not yet (later slices)

The M1.5b UI (per-volume SMB opt-in + volume "always index" toggle) ships in Settings > File system watching > Image
search (see [DETAILS.md](DETAILS.md) § "The FE surface"). The Search dialog's image-OCR grid targets the focused pane's
volume (see [`src/lib/search/CLAUDE.md`](../../../src/lib/search/CLAUDE.md)). Still open: the per-FOLDER override's FE
trigger (a native menu item), full MTP on-demand wiring, the importance slider + progress/ETA (M2), and tags,
embeddings, faces (M2+).
