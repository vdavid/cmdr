# Image ML index: searchable photos by text, tags, faces, and OCR

## Status (re-derived from the tree, 2026-08-27)

**Everything except faces and captions is shipped and in users' hands.** M1 (plumbing + OCR search), M1.5 (SMB opt-in
enrichment), M2 (tags + image-similarity), M3 (natural-language CLIP semantic search), and M6 (photo search as an Ask
Cmdr + MCP tool) are all live. Two later efforts then hardened and scaled the result well past this plan's scope: the
settings/privacy/progress pass, and a resource pass that brought f16 embeddings, integer-id keying, CLIP palettization,
and an ANN index.

- **M4a + M4b (faces) and M5 (LLM captions) are the only unbuilt milestones**, and both are parked on purpose. David
  wants to be closer in the loop for faces; captions are genuinely optional. Their design below is the live part of this
  document.
- **M6 deviation:** photo search shipped as an **agent/MCP tool, not a `cmdr://` resource** (see the M6 note below).

⚠️ **This document is kept for its Decision log, which shipped code cites by bare number.** About 40 sites across
`crates/cmdr-index/src/media_index/` say "plan Decision N" and mean § "Key decisions" here. The milestone sections are
kept only for the parked design; ❌ don't read a shipped milestone's checklist as work to do, and ❌ don't restate a
mechanism from it — the `CLAUDE.md` / `DETAILS.md` pairs under `crates/cmdr-index/src/media_index/` are canonical for
everything that shipped.

⚠️ **A bare "plan M<n>" in `media_index` code does NOT mean this plan's milestones.** That numbering belongs to the
wiped `docs/specs/resource-use-plan.md`, where M3 was f16 embeddings, M4 integer-id keying, M5a/M5b the CLIP
palettization and `.mlpackage` reclaim, M6 ANN vector search, and M9 WAL checkpoint hygiene. Read "plan M4" in
`media_index/writer/mod.rs` as this plan's faces milestone and you will be badly wrong. "plan Decision N" IS this
document.

### The CLIP path is live, and has been since v0.36.0

An earlier status here said the CLIP path was "gated off until David uploads the model artifacts" and that the feature
"stays dark until the `.mlpackage` is hosted". **That is false.** Both towers are hosted and pinned with real SHA-256
digests at `https://huggingface.co/veszelovszki/cmdr-clip-vit-b32-coreml/...` (`media_index/clip/install.rs`; the
`PLACEHOLDER_SHA` sentinel that would refuse an install is gone from both entries), semantic search ships with its own
settings card (`MediaIndexClipModel.svelte`) and a `mediaIndex.semanticSearch.enabled` setting that defaults on, and
v0.36.0 (2026-07-24) shipped it to users along with Delete model. There is no David-only handoff left on M3.

## Why this exists

We want the user's images (across local disk, and opt-in on SMB/MTP) to be **searchable by their content**: type "beach
sunset" and find the photo, search the text printed inside a screenshot, find every shot of a named person, or filter by
auto-detected tags. This is the "AI-native file manager" promise applied to photos.

The research that motivated this plan (Immich teardown + 2026 macOS/Rust landscape, verified 2026-06-29) landed one big
reframe:

> In 2026, macOS ships OCR, face **detection**, scene tagging, image-similarity embeddings, and a free on-device LLM
> (reportedly multimodal) — all on-device, Neural-Engine-accelerated, zero model download. The only real gaps we must
> fill ourselves are **face identity** (Apple exposes detection but deliberately not recognition) and **text→image
> natural-language search** (Apple's image feature print has no text encoder). So this is a "fill two gaps + glue"
> effort, not a "build the whole ML stack like Immich" effort — **but see Decision 1's verification gates: the "glue" is
> real `unsafe` Core ML FFI, and three external claims must be proven before we lean on them.**

Immich's own architecture (separate Python ML service + Postgres/pgvector + HTTP) is **multi-user server overhead we
must not copy**. A single-user desktop app does all of it in-process, on-device, and stores vectors in SQLite.

This plan deliberately scopes **out** the discovery / metadata / thumbnail layers (file walk, EXIF, mtime change
detection) — those already exist in `indexing/`, and David is explicitly less interested in them here. We build the **ML
enrichment + search** layer on top of the existing drive index.

### Product values in play (from `docs/design-principles.md` and `AGENTS.md` § Principles)

- **Protect the user's data + privacy.** Everything defaults to **on-device** (no image leaves the machine). Faces are
  sensitive: explicit opt-in, clear copy, all-local, and **never silently mis-labeled** (see Decision 4). The single
  cloud path (LLM captions) is a separate, explicit, BYOK opt-in.
- **Respect the user's resources (CPU, RAM, disk, wallet).** Near-free by default: the only downloads are two small Core
  ML models (face + CLIP), both ANE-accelerated. Enrichment is throttled, cancelable, low-priority, and runs after the
  base index is live, under an explicit shared memory ceiling. Slow volumes (SMB/MTP) are opt-in and conservative.
- **Rock solid + everything cancelable.** Enrichment is a background, resumable, cancelable pass that never blocks the
  UI and survives crashes mid-run. `media.db` is a disposable cache; the only human work that must survive a wipe lives
  in a separate durable store, hardened against silent corruption.
- **Elegance above all.** macOS-native where it's clearly better (Vision + Core ML + Foundation Models via `objc2`
  bindings), not a bolted-on Python/ONNX-server stack — with a bounded `ort` fallback if a model won't convert.
- **Humans to humans.** All user-facing copy goes through the i18n catalog and gets human review.

## Current state (the map the implementer needs)

Four existing subsystems this plugs into. Read their colocated `CLAUDE.md` + `DETAILS.md` before touching them. Claims
below were verified against the code on 2026-06-29, and the `importance/` + lifecycle-bus claims re-verified 2026-07-13
(file refs may drift — confirm with `codegraph_search`).

- **`crates/cmdr-index/src/indexing/`** — per-volume SQLite index DBs (one writer thread per DB; local + SMB + MTP each
  get their own DB), recursive size aggregates, `ReadPool` for reads, per-volume registry (`INDEX_REGISTRY`), freshness
  model, phase events. **Hard invariants we must respect:** the index is a **disposable cache** (schema mismatch /
  corruption ⇒ delete + recreate, no migrations); **one writer thread per DB**; `platform_case` collation on every
  connection; reconciler/event loops hold a READ connection only; **no rayon for macOS-framework calls** (dedicated OS
  threads + `objc2::rc::autoreleasepool`); one global 16 GB memory watchdog, which is no longer indexing-only — it runs
  registered `subsystem_stop` hooks alongside `stop_all_indexing`, and `media_index` is registered. FDA gates only
  `root` auto-start.
  - **Identity model (verified, load-bearing for us):** `entries` has no stable cross-rebuild id. `id` is assigned by
    insert order over a table **truncated before each full scan** (`store/entries.rs`), so the same file gets a
    different id after any wipe/rescan, and jwalk's parallel order isn't even deterministic. The table's real identity
    is **`(parent_id, name_folded)` UNIQUE** — i.e. the **path**. An `inode` column exists and is used **only to follow
    renames/moves** in the live loop (`find_entry_by_inode`); inode is unstable on copy and unreliable on SMB/MTP, so
    it's a rename hint, not an identity. **We key on path, exactly as the index itself does.** (Decision 3.)
  - **Phase events are frontend-only, but a neutral in-process bus now ships (verified 2026-07-13):** `set_phase_for`
    still only `.emit(app)`s a Tauri event _to the webview_, so it's no subscription surface for a Rust subsystem. But
    `indexing/lifecycle_bus.rs` IS that surface now: `state::apply_freshness_event_on` calls `publish_scan_completed` on
    `FreshnessEvent::ScanCompleted` (the neutral chokepoint BOTH local and network scans funnel through — verified at
    `state.rs`), and `importance/`'s scheduler is a live subscriber. `media_index` subscribes the same way. Network
    volumes still emit only `Scanning → Live` at the phase layer, so drive "ready to enrich" off the bus, never a phase
    the network path never sends. (Decision 7 — now "subscribe to the existing bus", not "add one".)
- **`src-tauri/src/ai/`** — on-device model **download** infra. Verified reality: `download.rs` is a generic resumable
  HTTP GET (**genuinely reusable**); `install.rs` is **GGUF/llama-server-specific** orchestration; `extract.rs` only
  **`fs::copy`s the bundled llama-server binary** (no archive extractor); verification is **file-size only**, not
  checksum. We reuse `download.rs`; the rest of the model-install path is **new code** (Decision 9). Also: the cloud
  BYOK client (`client.rs`, `genai`) and the `is_local_ai_supported()` Apple-Silicon gate shape.
- **`src-tauri/src/search/`** — read-only, one-way consumer of `indexing/` via a defined read surface (`ReadPool`,
  `IndexStore`); in-memory filename index; **pure** `engine.rs` (no I/O); NL→`SearchQuery` AI translation
  (`search/ai/`). Image search is a **new query path** (vectors + FTS), and it must reach `media.db` **through a
  `media_index` read API — never a raw `rusqlite` dependency** (Decision 8). It surfaces through the same `query-ui`
  primitives.
- **`crates/cmdr-index/src/importance/`** — the shipped sibling that already solved most of this plan's hardest plumbing
  (verified 2026-07-13). A pure read-consumer of `indexing/`, sibling to `search/`, whose own docs name **media-ML
  enrichment** as an intended consumer. `media_index` COPIES its patterns rather than re-deriving them; read
  `importance/CLAUDE.md` + `DETAILS.md` before M1. It ships:
  - a per-volume disposable `importance.db` (`store/`) carrying the index's cache discipline verbatim — `platform_case`
    collation, delete-and-recreate on `SCHEMA_VERSION` mismatch, **path-keyed** rows, ONE long-lived writer per volume
    via a `WriterRegistry`, and a full pass that REPLACES the whole table in one transaction stamping each row with its
    as-of `recompute_generation` (importance's own persisted meta counter — NOT the lifecycle-bus generation) — the
    reference implementation for Decision 3's STORE DISCIPLINE (media_index copies the discipline but drops the per-row
    generation stamp; see Decision 3);
  - a `scheduler/` driven by the lifecycle bus + a startup registry sweep (`ready_volumes_with_kind`) + a registration
    bus for late-mounting volumes, coalesced per `volume_id` — the reference implementation for Decision 7;
  - the `ImportanceIndex` consumer read API (`read/`): the ONLY entry point, no raw `rusqlite` dep, reads the DB
    directly so it answers OFFLINE after a volume unmounts — the reference implementation for Decision 8;
  - the per-folder **importance score** (`0..1`, floor overrides to `0.0` for denylisted/hidden/system dirs like
    `node_modules`), which `media_index` reads (via `top_above_threshold` / `above_threshold`) to enrich HIGH-importance
    folders first (see Cross-cutting § Importance-prioritized enrichment).

macOS FFI precedent already in the codebase: `objc2` + Cocoa/ObjC threads with autoreleasepools, `NSWorkspace`,
`QLPreviewPanel`, swizzling, `security-framework`. Vision/Core ML via `objc2-vision` / `objc2-core-ml` fit this — but
each `unsafe` block needs a specific `// SAFETY:` per `src-tauri/CLAUDE.md` (Decision 1).

## Key decisions (with intent — adapt if reality differs, but know the why)

1. **macOS-native inference, with a pre-validated `ort` fallback and explicit verification gates.** Use Apple **Vision**
   (OCR, face detection, scene tags, image feature print) and **Core ML** (MobileCLIP, ArcFace) through `objc2-vision` /
   `objc2-core-ml`, and **Foundation Models** (Swift bridge) for the optional caption path.
   - _Why:_ macOS-only app → native frameworks give ANE acceleration and the smallest binary (no bundled ONNX Runtime
     native lib). "Ideal over cheap" + "rely on macOS where reasonable."
   - **Gates (a) + (c) RESOLVED by spike** (2026-06-30; `../../notes/clip-coreml-rust-spike.md`). The Core ML text
     encoder and the Rust round-trip both work: a minimal `objc2-core-ml` 0.3.2 spike loaded a compiled model,
     predicted, and returned an embedding **bit-identical** to the `coremltools` reference; text→image alignment runs
     correctly on-device (ANE); native Core ML adds **zero binary weight**; the `unsafe` surface is ~12–15 mechanical
     objc2 calls behind a ~150–250 line safe wrapper (`encode_text`/`encode_image`). **The real constraint is licensing,
     not capability:** Apple's MobileCLIP/MobileCLIP2 weights are **research-only** (Apple ML Research Model Terms of
     Use — verified against Apple's `LICENSE_MODELS`), so a commercial product can't ship them. **Resolution, no
     architecture change: use a commercially-licensed CLIP** (OpenAI CLIP = MIT, or SigLIP 2 = Apache-2.0), converted
     once with `coremltools` and shipped pre-converted — the plumbing is model-agnostic. Trade-off: heavier than
     MobileCLIP-S0, still fine on the ANE.
   - (b) "Foundation Models is multimodal (image input) as of macOS 26" — still unverified; gate at M5 (optional).
   - **Bounded fallback:** if the chosen CLIP won't cleanly convert to Core ML (or loses accuracy), run _that one model_
     via `ort` + CoreML execution provider — but that costs **~25–35 MB of native binary** (`libonnxruntime.dylib` +
     ONNX artifacts; `ort` is pre-1.0) the native path avoids. Per-model last resort, not the default.

2. **Vectors in SQLite, never Postgres — brute-force first.** Below ~100k vectors, brute-force cosine in Rust (low-ms,
   zero deps) is enough and ships in M2/M3. `sqlite-vec` is a **loadable extension** and our `rusqlite` is built without
   `load_extension` (verified: `features = ["bundled","collation","fallible_uint"]`); enabling it also runs into
   hardened-runtime/notarization constraints for loading a dylib into a signed app. So **`sqlite-vec` is a real
   build+signing project, not a flag flip** — adopt it only if a real library crosses the threshold, behind the same
   vector-store trait. **FTS5 needs NO rusqlite feature flag — PROVEN, not assumed** (`agent/store/`'s `main.db`,
   verified 2026-07-13): its external-content FTS5 index compiles under the same `bundled` SQLite, and rusqlite 0.39 has
   no `fts5` feature to flip anyway. The plan's earlier "might need a `libsqlite3-sys` build flag" worry is CLOSED.
   Still keep a `CREATE VIRTUAL TABLE … USING fts5` runtime smoke at M1 start (as `agent/store`'s
   `fresh_open_builds_current_schema` guards — a bundled build without FTS5 fails there), but there's no build-flag gate
   to fear. _Why:_ a single user's library is small; Postgres+pgvector is multi-user server overhead. Kills the
   "ship/download Postgres" question entirely.

3. **A separate per-volume media DB (`media.db`), keyed on PATH identity.** Don't add ML tables to the index DB.
   - ⚠️ **Refined since, and the colocated doc records it:** storage is now one `media_file(id, path)` identity table
     with every other table keying on `file_id`, which is what makes a rename a one-row update. The reasoning below is
     unchanged and still load-bearing — PATH is the identity, and `(path, mtime[, size])` is the staleness key — but
     "every table stores the path" is no longer the shape. `crates/cmdr-index/src/media_index/store/DETAILS.md` is
     canonical.
   - **Reference implementation to COPY, not re-derive: `importance/store/`** (verified 2026-07-13). It already carries
     the index's disposable-cache discipline verbatim — `platform_case` on every connection, delete-and-recreate on a
     `SCHEMA_VERSION` mismatch, path-keyed rows, ONE long-lived writer per volume via a `WriterRegistry`, and a full
     pass that clears + repopulates the table in ONE transaction while stamping each row with its as-of
     `recompute_generation` (`importance`'s own persisted meta counter — NOT the lifecycle-bus generation).
     `media_index/store/` mirrors the cache discipline, with two deliberate divergences: media enrichment is expensive
     and incremental (it does NOT rewrite the whole table each scan), so it keeps a real GC (below) rather than
     importance's wholesale-replace; and it does NOT copy the per-row generation stamp — its staleness is
     `(path, mtime[, size])`, which makes a generation column redundant (see the last bullet of this decision).
   - _Why separate DB:_ respects "one writer thread per DB" (no contention with the size-index writer), independent
     disposable lifecycle, mirrors the per-volume registry pattern (SMB/MTP slot in naturally).
   - _Why path-keyed:_ there is no stable cross-rebuild entry id (see Current state). `media.db` rows key on the **same
     path identity the index uses** (parent chain + `name_folded`, or a normalized full-path hash with `platform_case`
     folding). A rebuild of either DB re-joins by path. **The staleness key is `(path, mtime[, size])` from the index
     row, not the entry id.** This corrects the v1 "stable id" error that invalidated the whole rebuild story.
   - _Rename/move = delete+add (recompute), no inode fast path._ The index preserves its entry id across an
     inode-matched rename but the **path changes**; `media_index` only subscribes to the lifecycle bus (no per-entry
     move events), so it sees a rename as the old path vanishing + a new path appearing and re-enriches. Derived data is
     cheap to recompute; don't chase an inode "follow" optimization that isn't wired.
   - **GC must be deletion-driven, never absence-during-a-rescan (data-safety).** A true full rescan **truncates**
     `entries` and repopulates, so mid-scan _every_ path transiently "vanishes." GC keyed on "absent from the index
     tree" would then delete media rows for files that still exist and force full re-enrichment. So GC reacts to the
     reconciler's actual delete of a **known** entry (the index "deletes only a known entry"), and/or reconciles **only
     against a completed scan** — never while a volume is `Scanning`. (LOCAL rescans of a populated index reconcile in
     place via `local_reconcile.rs`; the hazard is specifically the truncate path.)
   - **The lifecycle bus is a transient wake signal, NOT a persisted watermark — this is the "only against a completed
     scan" gate** (verified against `indexing/lifecycle_bus.rs` + `importance/scheduler/` on 2026-07-13). Use the bus
     exactly as `importance/`'s scheduler does: `matches!(state, ScanState::Completed { .. })` triggers a coalesced
     reconcile + GC pass, and that is the whole contract. **Never persist or compare `ScanState.generation`.** That
     `generation` is an in-memory `watch` counter in a process-global map that starts empty every launch — the first
     completion after any restart is always `generation: 1`, it isn't persisted, and it resets on restart. If GC
     persisted it as a row stamp + "last reconciled" watermark and gated on "a higher generation," GC would break after
     the first restart (the bus resets to 1, never exceeds the persisted value, so deleted-file rows would leak
     forever). `importance/`'s scheduler pattern-matches `Completed { .. }` and coalesces via its `PassCoordinator`; it
     never reads the generation value. (Do not confuse the bus generation with `importance/`'s `recompute_generation`,
     which is a SEPARATE persisted meta counter `importance` mints itself via `ImportanceWriter::next_generation` and
     has nothing to do with the bus.)
   - **The GC safety guarantee comes entirely from "only sweep when triggered by a `Completed` signal," plus serialized
     passes — no generation arithmetic.** The `Completed` signal is verified to fire AFTER the index writer flushes the
     truncate + repopulate: `indexing/scan_completion.rs` calls `writer.flush().await` before
     `apply_freshness_event_on(ScanCompleted)`, which is the only caller of `publish_scan_completed`. So a triggered
     sweep always observes a complete tree, never the mid-scan truncate window. A `PassCoordinator`-style clone
     (Decision 7) serializes passes so a sweep and a concurrent trigger collapse to one pass. A volume mid-`Scanning`
     has published no `Completed`, so the truncate window can never trigger a sweep. The startup sweep is safe for the
     same reason: `ready_volumes_with_kind()` filters to `Fresh` only (verified — it excludes `Scanning`/`Stale`), so a
     mid-scan volume is never swept at launch.
   - **No per-row scan-generation column.** Per-row staleness is already `(path, mtime[, size])` from the index row
     (below), which makes a generation stamp redundant. (If `media_index` ever needs a durable offline "as-of" marker,
     it mints its OWN persisted counter à la `importance`'s `next_generation`, independent of the bus — but that's a
     future option, not a requirement.)

4. **Disposable derived data vs durable human work — split the stores, and harden the durable side.** Detections,
   embeddings, tags, OCR text, and _computed_ clusters are **disposable** (`media.db`, regenerable). **Human work**
   survives a wipe in a separate durable app-data store. Human work is **not just names** — it includes
   **merge/split/"not this person" corrections**. The durable store holds, per named/curated identity: the assigned
   name, the corrections, and one or more **embedding centroids tagged with an enrichment-provenance stamp** (below).
   - **The compatibility key is an enrichment-provenance stamp, NOT a bare model-id string** (data-safety). A model-id +
     version string alone is too weak: `.mlmodelc` is recompiled on-device per OS version, and ANE inference can drift
     across an OS upgrade while the id + version string is unchanged, so a silent mislabel could slip the gate. The
     durable compatibility key is `{model id + version, Core ML / OS version, tag-taxonomy version}`, stamped per
     durable centroid AND per relevant `media.db` row. (This durable provenance stamp is a DIFFERENT concept from the
     dropped ephemeral bus generation in Decision 3 — that one is transient and reset-on-launch; this one is durable and
     gates mislabel safety. Don't conflate them.) The **tag-taxonomy-version** component also drives tag re-enrichment:
     an OS upgrade that changes the Vision tag taxonomy bumps that component, so stale-taxonomy rows re-tag (this closes
     the separate "OS upgrade changes the taxonomy → must re-index" gap in one field).
   - **Cheap self-check on regenerate, before trusting ANY cosine re-attach:** re-embed one known-good stored face and
     verify its cosine to its stored centroid stays above a sanity floor. If it fails, treat it as a model change (→
     "needs re-confirm"), even when the provenance stamp claims a match — the stamp catches the labeled case, the
     self-check catches silent drift the stamp missed.
   - **Storage substrate is a genuine re-decision at impl time (lean: migrating SQLite ladder).** When the plan was
     written, the only durable precedent was `favorites/` (atomic JSON), so JSON was the default. Two durable
     **MIGRATING SQLite** stores now ship — `operation-log.db` (`operation_log/store/migrations.rs`, explicitly "the
     template future durable DBs follow") and `agent/main.db` (`agent/store/migrations.rs`, which mirrors it) — with a
     shared ladder discipline: an append-only forward ladder, NEVER edit or renumber a shipped step, refuse a downgrade
     (`SchemaDowngrade`, never wipe), and delete-and-recreate ONLY on a typed unparseable-file sqlite code (never a
     string). The trade-off: **atomic JSON** is dead-simple for a tiny human-work set and trivially inspectable, no
     schema; **a migrating ladder** buys relational queries (names, corrections, negatives, centroids joined and
     indexed), safe schema evolution as the identity model grows, and it matches the two existing durable siblings a
     maintainer already knows — at far lower cost than when this plan assumed no precedent. For a relational, queried,
     evolving set (names + negative/cannot-link corrections + provenance-stamped centroids + space-independent anchors)
     the ladder is likely the more elegant fit; **make the final call in M4b**. Whichever substrate wins, ALL the
     data-safety semantics below (conservative re-attach, provenance-stamp gating, self-check, negative vetoes,
     space-independent anchors) are unchanged — only the storage shape is in question.
   - **Re-attach after a wipe is conservative, not silently automatic** (this is the data-safety crux):
     - If `media.db` survived (the common case — a crash, not a schema wipe), the face rows and their identity links
       survived too; nothing to re-attach. (Identity links by `face_id`, not by path — don't rebind faces by path, which
       is wrong for multi-face photos.)
     - On a true face-embedding regenerate, re-attach candidates by centroid cosine **only when the centroid's
       enrichment-provenance stamp matches** the current one AND the regenerate self-check passed. **Any mismatch ⇒ do
       NOT cosine-match across incompatible spaces** (it would mislabel); instead mark identities "needs re-confirm" and
       re-surface them in the People UI.
     - **Every correction/negative carries a space-independent anchor, not only an embedding.** A "not this person" veto
       stored only as an old-space embedding can't be cosine-checked after a model change, so the re-confirm UI couldn't
       warn the user and they could silently re-approve the exact face they rejected. So each correction/negative stores
       a **space-independent anchor** — `(path, an IoU-tolerant face-locator / bounding box within the image)` — in
       ADDITION to any embedding, so it survives BOTH a `media.db` wipe AND a model change. The locator is IoU-tolerant
       because re-detection can shift crops and face counts across OS versions.
     - **Negative/cannot-link corrections are hard vetoes consulted on every re-attach AND re-cluster.** A face the user
       removed from "Dóri" ("not this person") will, after a regenerate, again be cosine-nearest to Dóri's centroid — so
       a purely positive matcher would silently re-introduce the exact mislabel the user fixed. Any candidate suppressed
       by a durable negative (matched by embedding when spaces agree, else by its space-independent anchor) is **never
       auto-attached**, only offered as "needs re-confirm." Likewise re-clustering must honor durable
       cannot-link/must-link, or a manual split silently re-merges. **Cannot-link is the hard constraint:** when a
       transitive must-link closure (a–b, b–c) would force a cannot-link violation (a–c), the must-link is dropped and
       flagged, never silently applied. (This is the hole positive-only re-attach leaves; the M4b tests target it
       explicitly, including the transitive-conflict case.)
     - Even on a clean match, a **high threshold** plus a lightweight "Still <name>?" confirmation for low-confidence
       re-attaches — a silent false attribution is worse than asking. Mis-attach is a first-class failure mode here, not
       just "failed to attach."
   - _Why:_ the index is explicitly throwaway; we must never silently lose or corrupt the human labeling/curation. This
     is the single most important data-safety decision in the plan, and the M4 red→green tests target exactly it.

5. **Feed a downscaled in-memory decode to the models, never the original.** Decode via ImageIO/CoreGraphics (native;
   HEIC/RAW), downscale to model input (~224–512 px), feed the `CGImage`. No thumbnail _files_. _Why:_ CLIP/OCR need
   small inputs; decoding originals twice is the dominant cost.
   - **Carve-out — face-crop avatars are durable curated OUTPUT, not an enrichment input, so they don't violate this.**
     This decision bans thumbnail _files_ as enrichment INPUTS (the decode stays in-memory). The face-crop avatars the
     People UI needs are a different thing: durable curated output stored as BLOBs in the disposable `media.db`, GC'd
     with their rows (Decision 4, M4a). Likewise, search results reuse the existing QuickLook/preview path for their
     grid, never a `media_index`-produced thumbnail file.

6. **Opt-in, gated, conservative by default.** Whole feature off until enabled; **faces a separate opt-in** with privacy
   copy. Heavy/identity paths gate on **Apple Silicon** (`is_local_ai_supported()` shape); Vision OCR/tags work on older
   Macs, so don't over-gate. **Local volumes enrich by default when enabled; SMB/MTP are opt-in per volume** and
   conservative by default.
   - **"Conservative" for an opted-in network volume has teeth, not just "don't pull everything":** idle-gated (enrich
     when the volume isn't serving foreground work), bandwidth-bounded, resumable, and — as a candidate to weigh at impl
     time — on-demand-per-folder-visit rather than a full up-front sweep, so a rarely-browsed NAS archive doesn't get
     dragged over the wire wholesale.
   - **Navigation-based importance would starve a photo archive — so add an "always index this folder / this volume"
     override** (user-set, complements the per-folder exclude in the Privacy cross-cutting). `importance/` scores by
     navigation, so a NAS photo archive the user rarely browses folder-by-folder scores LOW everywhere; importance-first
     ordering plus the slider would then defer the user's actual photos indefinitely — the opposite of intent. The
     override forces enrichment regardless of importance. Consider a **photo-density signal** as a candidate importance
     input for THIS feature (a folder that's mostly images is likely a photo archive regardless of visit count) — see
     Cross-cutting § Importance-prioritized enrichment.
   - **Disabling image indexing defines its data behavior explicitly:** stop in-flight work (cancel token), and OFFER to
     delete `media.db`. The durable identity store (human work) **survives with a clear notice** (or is exportable) — it
     is never silently wiped and never silently retained without saying so (M1 toggle; Cross-cutting § Cancellation).

7. **Enrichment subscribes to the SHIPPED neutral lifecycle bus and does an initial registry sweep — copy the
   `importance/` scheduler.** `indexing/lifecycle_bus.rs` already exists and is exactly the surface this plan asked for
   (verified 2026-07-13); `media_index`'s scheduler subscribes to it the same way `importance/`'s scheduler does.
   Nothing to add in `indexing/` FOR THE BUS; the bus plumbing below is a copy, not a build. (The memory-watchdog hook
   is separate, real `indexing/` work — see Cross-cutting § Resources and M1.) The bus's design already resolves every
   adversarial point this decision once flagged:
   - **Per-volume `watch`, not `broadcast` — done.** `publish_scan_completed` (fired from `apply_freshness_event_on` on
     `FreshnessEvent::ScanCompleted`, the chokepoint BOTH local and network funnel through) uses `send_replace` on a
     `tokio::sync::watch<ScanState>`, so a completion fired during `setup()` before `media_index` subscribes is RETAINED
     and a late subscriber replays it. A consumer coalesces repeats IN MEMORY via `borrow_and_update` (as
     `importance/`'s scheduler does), never by reading or persisting `ScanState.generation` — that generation is a
     transient in-memory counter that resets to 1 every launch, so it must NOT be stamped or compared (Decision 3).
     `subscribe(volume_id)` returns the receiver; the senders live in a process-global map that outlives
     `INDEX_REGISTRY`, so a receiver keeps replaying after the volume unmounts.
   - **Startup registry sweep — done.** `indexing::ready_volumes_with_kind()` enumerates volumes already Fresh at launch
     (loaded from `meta.scan_completed_at` without re-firing a completion), WITH each volume's typed kind. The
     `importance/` scheduler subscribes to the bus, then sweeps, so a volume that never re-fires a completion after a
     restart still gets scheduled — the common case. `media_index` copies this ordering (subscribe-before-sweep).
   - **Late-registering volumes — RESOLVED (was an M1-deferred open question).** A share mounted mid-session reaches a
     subscriber through the registration `broadcast` bus (`publish_volume_registered` / `subscribe_registrations`,
     carrying the typed `IndexVolumeKind`). The `importance/` scheduler subscribes to registrations ONCE before its
     sweep (closing the gap), then wires each late volume's subscriptions on arrival. `media_index` reuses this
     registration/scheduling wiring for its SMB/MTP milestone (M1.5) — no new _bus_ mechanism needed. The **byte-fetch
     policy is genuinely new work**, though: only the scheduling, registration, and bus wiring is a copy; the
     conservative network-fetch path has no `importance/` sibling to reuse (importance never reads bytes off the wire —
     Decision 6, M1.5).
   - **Coalescing per `volume_id` — pattern to copy.** `importance/`'s `PassCoordinator` guarantees ONE pass per volume;
     a request arriving mid-pass sets a single re-run flag rather than starting a second. `media_index` copies this so
     the sweep and a concurrent `ScanCompleted` collapse to one pass, then at most one re-run. Cover with a coalescing
     test in M1 (over the fake backend, as `importance/` does).
   - **Network caveat (unchanged):** SMB/MTP emit only `Scanning → Live` at the phase layer, but **both kinds fire
     `FreshnessEvent::ScanCompleted`**, which is what the bus publishes — drive "ready to enrich" off the bus, never off
     a phase the network path never emits.
   - **Incremental (optional, later):** the bus also carries a per-volume `dir-changed` `watch` (`publish_dirs_changed`
     / `subscribe_dirs_changed`) that `importance/` drives its incremental rescore off. If `media_index` ever wants
     incremental re-enrichment of just-changed folders, that channel is already there — but note its accepted
     last-value-wins caveat (a burst can drop an earlier batch; the next full pass heals it), so treat it as advisory,
     never the sole trigger for anything data-safety-critical.

8. **`search/` reaches `media.db` only through a `media_index` read API.** The closest mirror to copy is now
   `importance/`'s `ImportanceIndex` (`read/`, verified 2026-07-13): the ONE consumer entry point, no raw `rusqlite`
   dep, owning a `platform_case`-registered read connection and reading the DB directly so it answers OFFLINE after the
   volume unmounts (the same posture `media_index` wants — search a volume's photos after the NAS is unplugged). Model
   `media_index`'s read API on `ImportanceIndex` (it in turn mirrors `search/`→`indexing/`'s `ReadPool`/`IndexStore`).
   `search/` stays a read-only consumer; it must not take a raw `rusqlite` dep on `media.db`, or the
   collation/one-writer invariants leak into a second subsystem.

9. **Model install is new code (generic archive unpack + checksum verify), reusing only `download.rs`.** Core ML models
   ship as `.mlpackage` **directory bundles** (typically zipped); nothing in `ai/` unpacks an archive, and `ai/`
   verifies size only. M3/M4 add: a generic archive extractor, **checksum** verification (not size-only), and a model-
   install gate distinct from the GGUF two-flag gate. Don't describe this as "reuse the install infra."

10. **The cloud is opt-in and only for premium captions** (M5), reusing the whole shipped `agent/` LLM stack behind a
    _distinct_ explicit egress consent. On-device captions (Foundation Models) are the default for that feature. What to
    reuse (all shipped as of Ask Cmdr M1–M8, verified 2026-07-13 — see `agent/CLAUDE.md`):
    - **The `AgentLlm` seam** (`agent/llm/`): a provider-agnostic trait (`AgentLlm::respond`), its genai-backed impl
      (`genai_impl.rs`), a deterministic zero-network `FakeAgentLlm` (`fake.rs`) for tests, and the typed message-part
      model (`types.rs`). **Caveat to check at impl time:** `AgentLlm::respond` is chat/tool-loop-shaped — one streaming
      call carrying an ordered list of typed parts plus opaque provider reasoning state (`ReasoningState.blob`). A
      caption is a stateless one-shot VLM (image → text) call with no tool loop and no reasoning to round-trip. **Expect
      a thinner sibling seam (image-in → text-out) as the DEFAULT outcome, not a coin-flip:** `AgentPart` today is
      `Text | ToolCall | ToolResult | Reasoning` (verified 2026-07-13) — there is no image part, so a VLM caption isn't
      representable in `AgentLlm::respond`'s type model. The consent gate and cost meter below reuse cleanly either way.
    - **The consent gate** (`agent/consent.rs`): `has_current_consent` + `CONSENT_COPY_VERSION`, enforced in the BACKEND
      send path (before any provider is resolved), fails CLOSED. A cloud-caption egress consent is a second, distinct
      copy version of the same shape — nothing reaches a frontier VLM without a recorded acceptance, even if the UI is
      bypassed.
    - **The cost meter** (`agent/pricing.rs` + the store's `cost_meter`): honest per-call cost,
      `PricedCost { cost_micros, priced }` where an unknown cloud model is `priced = false` (shown "unknown", never a
      silent $0). Extend the price table with the caption VLM.
    - **On-disk request/response logging is the egress AUDIT TRAIL** the sensitive-doc concern (Cross-cutting § Privacy
      — don't silently upload an ID scan) needs. NOTE: this is `agent/`'s M9 (`llm-logs/`, auth-header redacted,
      failure-isolated), **designed but not yet shipped** (Ask Cmdr landed through M8, verified 2026-07-13). Land or
      reuse it before the cloud caption path so every egress is inspectable; if it hasn't shipped by M5, M5 carries it.
      **The same consent gate and audit log must also cover the M6 photo-search tool** on the cloud agent path — that
      tool egresses derived text content (OCR snippets, captions, tags, person names), not only captions, so it isn't
      exempt.

## Architecture

```
indexing/ (existing)                 media_index/ (new subsystem)                      search/ (existing, extended)
  per-volume size DB                   subscribe lifecycle bus ◄── (SHIPPED,             new image-query path:
  ReadPool (read entries)                indexing/lifecycle_bus.rs; watch, not             text → CLIP text vec → vec search
  lifecycle_bus.rs ─────────────────►    broadcast) + registry sweep + coalesce          tag/OCR → FTS5
  (ScanCompleted = wake signal)        scheduler: walk image entries (path-keyed),        face name → durable identity → clusters → paths
                                         importance-first (top_above_threshold),          reaches media.db ONLY via media_index read API
importance/ (existing)                   throttle / cancel, shared mem ceiling            surfaced via query-ui/
  ImportanceIndex (read/) ──────────►  decode (ImageIO downscale)
  per-folder score 0..1, floors        ▼ per image, via objc2 on dedicated threads
  (enrich important folders first)     Vision: OCR, tags, feature-print, face detect
                                       Core ML: CLIP embed, ArcFace face embed
                                       ▼ write (one writer/DB, platform_case, disposable;
                                         staleness = (path, mtime[, size]), no generation)
                                       per-volume media.db (path-keyed, disposable):
                                         media_status, media_tags, media_ocr(FTS5),
                                         media_embedding, media_face + crop BLOB, face_cluster
                                       GC reconcile vs index deletions (only when a
                                         Completed signal fires post-flush, never mid-Scanning)
                                       ▼
                                       durable store (survives wipe; JSON or SQLite
                                         ladder — decide M4b, D4):
                                         named/curated identities + corrections
                                         (+ space-independent anchors)
                                         + provenance-stamped centroids
```

Inference sits behind a Rust trait (`VisionBackend` / `MediaModel`) with a **fake backend** for tests, so scheduler,
storage, clustering, GC, and search logic are all testable without GPU/ANE.

## Milestones

M4a, M4b, and M5 below are the live ones. The rest shipped; this block records what each was and where its mechanism now
lives, and nothing more.

### M1, M1.5, M2, M3, and M6 — ✅ SHIPPED

❌ **No checklist here is work to do, and no mechanism here is canonical.** Each area's `CLAUDE.md` + `DETAILS.md` pair
under `crates/cmdr-index/src/media_index/` owns its own account; `media_index/DETAILS.md` § "Area map" is the way in.

- **M1 — plumbing + OCR search** (~2026-07-14). The per-volume `media.db`, path identity, the lifecycle-bus
  subscription, the backend trait, GC, and the read-API boundary. `media_index/store/`, `scheduler/`, `backend/`,
  `read/`.
- **M1.5 — network-volume enrichment** (~2026-07-14), the SMB opt-in and its conservative byte-fetch policy, validated
  on the NAS. `media_index/network/`.
- **M2 — tags + image-similarity** (~2026-07-14), Vision-only, zero download. `media_index/backend/vision/`, `vector/`.
- **M3 — natural-language CLIP semantic search** (2026-07-16; to users in v0.36.0, 2026-07-24). `media_index/clip/`. ⚠️
  Its cost model has moved a long way since and is easy to get wrong from memory: each tower loads on demand and
  reclaims its own `.mlpackage` source, the text tower is about 80% of the resident bill, and the compute-unit
  assignment decides the whole number. `media_index/clip/DETAILS.md` § "What holding the towers costs" and § "The query
  path" are canonical, measured, and dated. ❌ Don't restate them.
- **M6 — photo search as an agent + MCP tool** (2026-07-16). Shipped as a **tool, not a `cmdr://` resource**. Its
  privacy framing turned out to be the durable half and now lives in `docs/security.md`: what this surface returns is
  sensitive derived TEXT CONTENT, not "just metadata" (a passport scan's OCR text IS the passport number), so
  `image_facts` and the search tool are gated accordingly.

Two later efforts, both since wiped from `docs/specs/`, hardened and scaled all of the above: a settings/privacy/
progress pass, and a resource pass (f16 embeddings, integer-id keying, CLIP palettization, ANN vector search, WAL
checkpoint hygiene). The second is the source of the conflicting `plan M<n>` numbering warned about in § Status. M4 is
split because "detect + embed + cluster + hardened durable store + re-attach + People UI + a11y" is two milestones; M4a
de-risks the faces FFI/pipeline before the curation/durable-store/UI surface.

### M4a — Faces pipeline: detect, embed, cluster (no naming yet) — ⏸ PARKED (not started; David wants to be closer in the loop for faces)

- Vision detect (`VNDetectFaceRectanglesRequest` + `VNDetectFaceCaptureQualityRequest` best crop). Download an
  **ArcFace/AuraFace Core ML** model (verify license — AuraFace commercial-friendly) via the M3 model-install path →
  embeddings in `media_face`. Cluster (agglomerative/HDBSCAN on cosine) → `face_cluster`. Search by **cluster id** (no
  names) to prove the pipeline end-to-end. **Separate faces opt-in + privacy copy** lands here.
- **Face crops at rest:** the People UI's avatar crops live as **BLOBs in the disposable `media.db`** (or app-support),
  GC'd with their rows, **explicitly excluded from crash/error-report bundles**, and covered by the redactor + backup
  posture in `docs/security.md`. This does NOT violate Decision 5 (which bans thumbnail _files_ as enrichment INPUTS — a
  face crop is durable curated OUTPUT, a different thing).
- **Provenance stamp lands with the first stored embeddings:** stamp each durable centroid AND the relevant `media.db`
  rows with the `{model id + version, Core ML / OS version, tag-taxonomy version}` provenance key (Decision 4), so the
  M4b re-attach gate has it to check.
- **Docs:** `media_index/DETAILS.md` faces pipeline + face-crop-at-rest storage + the provenance-stamp shape;
  architecture map; `docs/security.md` on on-device face data + face-crop redaction/backup posture + consent, and — for
  the M5/M6 cloud path — that OCR snippets, captions, tags, and person names are derived text content that egresses to
  the provider (not "just metadata"); i18n strings for the opt-in.
- **Tests:** _TDD red→green:_ cluster **merge/split** correctness; clustering honors **durable must-link/cannot-link**
  (forward ref to the store M4b adds — stub the store in M4a). _After:_ fake-backend faces pipeline; macOS-gated
  detect+embed on a fixture with known faces. _E2E:_ faces detected, cluster-id search returns the right photos.
- **Checks:** full incl. `--include-slow`.

### M4b — Naming + durable identity store + conservative re-attach + People UI (the data-safety core) — ⏸ PARKED (not started; follows M4a)

- **Durable identity store (Decision 4), hardened:** names + corrections (incl. **negative/cannot-link**, each carrying
  a **space-independent anchor** `(path, IoU-tolerant face-locator)`) + **provenance-stamped centroids**
  (`{model id + version, Core ML / OS version, tag-taxonomy version}`). **Decide the substrate here (Decision 4): atomic
  JSON (`favorites/`-style) vs a migrating SQLite ladder (`operation-log.db` / `agent/main.db`-style), leaning ladder.**
  Re-attach is conservative whichever wins: links intact when `media.db` survives; provenance-stamp-gated cosine + the
  regenerate self-check otherwise; **negatives veto** (matched by anchor when spaces disagree); any mismatch ⇒ "needs
  re-confirm", never cross-space match; high threshold + confirm for low confidence.
- People UI: largest unnamed clusters first, best-quality crop avatar, name/merge/split/"not this person"/propagate;
  search by person name → paths.
- **Docs:** `media_index/DETAILS.md` durable-store + re-attach rationale; frontend `people/CLAUDE.md`+`DETAILS.md`;
  update `docs/security.md`; i18n strings.
- **Tests (data-safety critical — re-run yourself, don't trust delegation, per `verify-delegated-work`):**
  - _TDD red→green:_ **names+corrections re-attach after a simulated `media.db` wipe** (must re-bind); **refuse to
    cross-space match** on a provenance-stamp mismatch (assert no mislabel); **same model version but drifted
    embeddings** — the regenerate self-check fails the cosine sanity floor even though the stamp matches, so re-attach
    falls back to "needs re-confirm" (distinct from the stamp-bump test); **a face with a durable "not this person: X"
    veto must NOT re-attach to X after a regenerate, even when X's centroid is cosine-nearest** (the C-NEW-1 hole);
    **the same negative veto is re-surfaced in the re-confirm UI after a model-version change** (matched by its
    space-independent anchor, since the embedding can't be cosine-checked across spaces); the centroid-match threshold
    and the "needs re-confirm" path.
  - _After:_ fake-backend identity-store round trips.
  - _E2E:_ name a cluster, search the name, find photos; rename/merge; remove a face then regenerate and assert it does
    NOT snap back; simulate a provenance-stamp bump and assert the UI asks to re-confirm rather than silently
    relabeling.
- **Checks:** full incl. `--include-slow`; a11y on the People UI (AA+ contrast, screen reader).

### M5 — LLM captions (premium, opt-in; on-device default, cloud optional) — clearly later, genuinely optional — ⏸ PARKED (not started; genuinely optional)

- On-device captions via **Foundation Models** (verify multimodal per Decision 1b). **Swift bridge = a Swift-toolchain
  build subproject** (Foundation Models is Swift-only; no Rust bindings) — a linked Swift static lib/framework or a
  sidecar, called over FFI. Spike the bridge early (can run in parallel as research); keep it isolated.
- Captions feed the FTS index. **Optional cloud route** (frontier VLM) reusing the shipped `agent/` LLM stack per
  Decision 10 — the `AgentLlm` seam (check `respond`-fits-a-VLM vs a thinner image-in/text-out sibling), the
  backend-enforced consent gate (`agent/consent.rs`, a distinct egress copy version), the cost meter
  (`agent/pricing.rs`), and the on-disk request/response audit log (`agent/`'s M9 `llm-logs/` — land it if it hasn't
  shipped). Never default.
- **Docs:** the Swift-bridge build wiring (own `docs/guides/` doc); consent + privacy copy; point at `agent/CLAUDE.md`
  for the reused seam/consent/cost/logging (single-source, don't restate).
- **Tests:** _TDD red→green:_ provider-selection + consent gate (on-device vs cloud vs off); **verify the cloud gate
  blocks egress when off** (security-critical, re-run yourself — reuse `agent/consent`'s fail-closed pattern). _After:_
  Swift-bridge smoke (gated). _E2E:_ enable captions, search a described scene.
- **Checks:** full suite.

### M6 — photo search as an agent + MCP tool — ✅ SHIPPED

Covered in the shipped block above. One design property is worth keeping here because M4b and M5 both inherit it: the
tool's result is a **typed DTO that structurally cannot carry image bytes** (text fields only — matched OCR snippet,
person name, tag, path), enforced by a test rather than by prose. When faces land, a person name joins that DTO and
inherits the same constraint and the same egress gate.

## Cross-cutting

- **Importance-prioritized enrichment (the highest-value new capability).** The scheduler enriches HIGH-importance
  folders first and defers or skips low-importance junk, reading `importance/`'s `ImportanceIndex` (`read/`) — the
  per-folder score is `0..1` with floor overrides to `0.0` for denylisted/hidden/system dirs (`node_modules`, `.git`,
  caches), which `importance/`'s own docs already name media-ML enrichment as a consumer of. Pull the ranked candidates
  with `top_above_threshold(n, t)` / `above_threshold(t)`; enrich in score order; drop anything below the user's slider
  threshold (Settings, M2). On a NAS-sized volume this is the difference between first useful results fast and a uniform
  slog through hundreds of thousands of cache/build folders. The score reads OFFLINE (the read API answers after a
  volume unmounts) and carries an as-of `recompute_generation` (importance's own persisted marker, NOT the lifecycle-bus
  generation), so the priority signal survives an unmount and is honestly stale-marked. This replaces the plan's earlier
  hand-waved "priority" with a real, shipped signal.
  - **But navigation-based importance starves a rarely-browsed photo archive — so two escape hatches are decisive for
    the NAS use case** (Decision 6). A NAS photo archive the user seldom opens folder-by-folder scores LOW everywhere,
    so importance-first ordering plus the slider would defer their actual photos indefinitely. The **user-set "always
    index this folder / this volume" override** forces enrichment regardless of score (it complements the per-folder
    exclude in § Privacy). And a **photo-density signal** — a folder that's mostly images is likely a photo archive
    regardless of visit count — is a candidate importance input for THIS feature, to lift a dense archive above the
    slider floor without a manual override. Weigh both at impl time; don't ship importance-first without at least the
    manual override.
- **Resources + memory ceiling — ✅ WIRED, keep it that way.** Enrichment runs on dedicated low-priority OS threads (not
  rayon), bounded concurrency, cancel token, starts only after the base index signals ready, yields to foreground. The
  one-ceiling rule this section argued for is now code: `indexing/resources/subsystem_stop.rs` is a hook registry the
  global 16 GB watchdog runs alongside `stop_all_indexing`, and `media_index/scheduler/lifecycle.rs` registers into it
  at startup. ❌ A future subsystem sharing the resident pool registers a hook; it does NOT stand up a second budget
  (two ceilings over one pool each see headroom and can sum to ~2×). Standing cost is canonical in
  `media_index/DETAILS.md` § "Standing cost".
- **Honest progress + coverage (a core Cmdr transparency value, not polish).** Background enrichment must surface honest
  per-volume progress with counts, ETA, and state ("12,000 of 38,900 images indexed on naspi, about 15 min left"; "naspi
  still scanning") — a minimal per-volume state in M1, the full count-and-ETA surface in M2 (where it shares the
  slider's count machinery). And a search run while enrichment is incomplete must voice it in the MAIN `query-ui`
  results ("still indexing, results may be incomplete"), not only in the M6 agent tool — pull the coverage-honesty
  requirement up into M1/M3's query surface.
- **Query-time vector residency — ✅ SOLVED, and past what this section imagined.** The resident vector cache landed,
  and then an ANN index landed behind it: a per-volume usearch HNSW file beside `media-{id}.db`, f16, mmap-read, mutated
  through the one `MediaWriter` thread. `media_index/ann/` and `media_index/vector/DETAILS.md` are canonical; the
  numbers that justified it are in `docs/notes/ann-vector-search-spike-2026-07-24.md`. ❌ Don't re-derive the
  brute-force-vs-index cutover from this paragraph.
- **Cancellation + crash-safety.** Every pass is resumable from path-keyed `media_status`; a crash resumes. `media.db`
  is disposable; only the durable identity store must survive (separate, crash-safe, versioned — substrate per Decision
  4).
  - **Mid-pass unmount (not a crash) is its own case.** An unmount mid-decode over SMB throws I/O errors on live work.
    `media_index` abandons the in-flight item cleanly, **KEEPS every completed path-keyed row** (they're valid), and
    marks the volume "paused, resumes when reconnected" — it resumes via the bus's registration path on remount. It
    **never marks rows permanently failed** on an unmount; a disconnect isn't a bad file.
  - **Disabling image indexing** stops in-flight work via the cancel token and OFFERS to delete `media.db`; the durable
    identity store survives with a clear notice (or is exportable), never silently wiped or silently retained (Decision
    6, M1 toggle).
- **Deletion/GC.** `media_index` reconciles against index deletions (file vanished ⇒ media rows, face crops, embeddings
  GC'd). Resume ≠ cleanup; both are required.
- **Privacy.** On-device by default; faces a separate opt-in; cloud captions a separate egress opt-in. Mirror
  `onboarding/`'s consent pattern; document in `docs/security.md`. **Sensitive-document awareness:** real user folders
  mix ID scans (passport, driver's license, medical) in with photos — the index will OCR/tag/face-detect them. On-device
  keeps that local (fine), but it sharpens the M5/M6 cloud egress consent: the OCR snippets, captions, tags, and person
  names those paths send are sensitive **derived text content** (a passport scan's snippet IS the passport number), not
  "just metadata" — so don't silently upload an ID scan's derived text either. A `docs/security.md` must-note. (Spike
  side finding, 2026-06-30.)
  - **Per-folder "don't index for photo search" exclusion — ✅ SHIPPED** (`mediaIndex.excludedFolders`), together with
    the privacy retro-delete that removes what was already enriched under a newly excluded folder. Why it was needed:
    the threshold slider can't protect a high-importance `~/Documents/IDs/` folder, because the user uses it, so it
    scores HIGH. Canonical: `media_index/DETAILS.md` § "Per-folder photo-search exclude + the privacy retro-delete".
- **i18n.** Every user-facing string via the catalog with a `@key` description (`cmdr/no-raw-user-facing-string`).
- **Dependencies.** `objc2-vision`, `objc2-core-ml`, maybe a clustering crate / `ort` fallback / `sqlite-vec` binding:
  each needs `cargo deny check` + a verified ≥14-day-old version (`use-latest-dep-versions`, project `dependencies`
  rule).
- **No string-matching for classification**: typed enums for model/provider/consent/identity- state across IPC; the
  frontend never branches on message substrings.

## Parallelization (only where extremely safe; sequential is default)

- **M1 must land first** — media DB, path-keyed identity, lifecycle-bus subscription, backend trait, GC, the read-API
  boundary.
- After M1: **M1.5 (network-volume enrichment)**, **M2 (Vision tags/feature-print)**, and the **M5 Swift-bridge spike**
  are independent and can parallelize. M1.5 touches only the SMB opt-in, the fetch policy, and the network resumability
  paths (no `media.db` schema conflict with M2), so it can land alongside M2.
- **M3 (CLIP)** and **M4a (faces pipeline)** both depend on M2's vector store and the M3 model-install path (so M4a
  follows M3's install code even if the CLIP search work parallelizes); independent of each other in their `media.db`
  tables and UI surfaces (low conflict). **M4b follows M4a.** Prefer sequential unless we want speed; worktree per
  effort if parallelized.

## Definition of done

**Met, except the two face clauses** (named-face search, and the human-work-survives-a-wipe proof, which is M4b's to
earn). Kept as the bar the parked milestones are still measured against.

- Image indexing is opt-in, on-device by default, producing OCR-text search, tag search, image-similarity, natural-
  language text→image search, and named-face search — all via `query-ui`.
- **No human work is silently lost or mis-attributed across an index wipe or a model change** (proven by M4b tests,
  including the provenance-stamp-mismatch refuse-to-mislabel case, the same-version-drifted-embeddings self-check case,
  AND the negative-veto "doesn't snap back" case — including its re-surfacing by space-independent anchor after a model
  change). No image leaves the device unless the user opts into cloud captions.
- The only downloads are two small Core ML models, fetched on demand and **checksum-verified**. No Postgres. Binary
  lean.
- Enrichment is throttled, cancelable, crash-resumable, GC'd against deletions, under an explicit memory ceiling, and
  **importance-prioritized** (high-importance folders first, below-threshold junk deferred/skipped per the settings
  slider, with an "always index this" override for rarely-browsed photo archives). SMB is a conservative per-volume
  opt-in with a genuine byte-fetch policy (idle-gated, bandwidth-bounded, resumable across unmount), validated on the
  NAS (M1.5); MTP enrichment is on-demand-per-visit, never a background sweep. Enrichment surfaces honest per-volume
  progress and coverage.
- Full `pnpm check --include-slow` green; new subsystem has `CLAUDE.md`+`DETAILS.md`; architecture map updated and the
  `media_index` lifecycle-bus subscription linked to `indexing/DETAILS.md`; privacy posture in `docs/security.md`.

## Open questions / risks (resolve during impl, before the dependent milestone)

- **Decision 1 gates (a)+(c): RESOLVED** (spike 2026-06-30 — native Core ML text encoder + Rust round-trip proven). The
  remaining M3 task is to pick and license-verify the **commercial** CLIP (OpenAI CLIP MIT / SigLIP 2 Apache-2.0 — NOT
  Apple MobileCLIP, research-only) and confirm its Core ML conversion fidelity. Foundation Models multimodal (1b) stays
  an M5 (optional) gate.
- **Path identity edge cases:** case folding (`platform_case`), normalization (NFD on APFS), and rename/move following
  via inode where inode is reliable — and the SMB/MTP cases where it isn't. Get this right in M1; every M1 test rides
  it.
- **Core ML conversion fidelity** for ArcFace/MobileCLIP vs the ONNX original; `ort` fallback per model if it degrades.
- **Clustering + re-attach thresholds** on real libraries — measure, record in `docs/notes/`, never hardcode blind; the
  re-attach threshold is privacy-sensitive (mis-attach > miss).
- **Enrichment-provenance drift** (Decision 4): confirm empirically that an OS upgrade can drift ANE embeddings and/or
  the Vision tag taxonomy while a model-id string is unchanged, and pick the self-check's cosine sanity floor from real
  before/after data — record in `docs/notes/` with an evidence anchor. This is the volatile-OS-behavior claim the
  provenance stamp + self-check defend against.
- **`sqlite-vec`: CLOSED, and never adopted.** Brute-force was outgrown and the answer was an in-process usearch HNSW
  index (`media_index/ann/`), which needs no `load_extension` feature and no dylib-loading signing story. Decision 2's
  "behind the same vector-store trait" escape hatch is what made that swap cheap.
- **HEIC/RAW decode** hostile cases via ImageIO (broken files, huge dims) — principle 3.
- **Foundation Models Swift bridge** (M5) is the least-proven integration; isolated, optional, spike early.
- **Vector-cache invalidation granularity** (M3): the warm resident cache is invalidated on writes, but enrichment
  writes embeddings continuously during a pass — naive whole-cache invalidation would thrash-reload ~200 MB per query
  mid-pass. Specify incremental/append cache update, or accept eventual consistency until the pass completes (perf, not
  correctness).
- **Durable identity store substrate** (Decision 4, decide at M4b): atomic JSON (`favorites/`-style) vs a migrating
  SQLite ladder (`operation-log.db` / `agent/main.db`-style). Leaning ladder for a relational, evolving set (names +
  corrections + provenance-stamped centroids + space-independent anchors); all data-safety semantics hold either way.
- **Lifecycle-bus concerns RESOLVED** (were open when the plan was written): the shipped `indexing/lifecycle_bus.rs`
  handles watch-vs-broadcast (late-subscriber replay), the Fresh-at-launch registry sweep (`ready_volumes_with_kind`),
  and **late-registering volumes** (the registration `broadcast` bus, `subscribe_registrations`, carrying the typed
  kind). `media_index` copies `importance/`'s subscription, so none of these are open for it — SMB/MTP enrichment reuses
  the same **scheduling, registration, and bus wiring** `importance/` already ships for network volumes. The one part
  with NO sibling to copy is the **byte-fetch path**: `importance/` hard-rules out ever reading bytes off an SMB/MTP
  mount (`importance/CLAUDE.md`: "NEVER a filesystem syscall against an SMB/MTP mount — read only the local index DB"),
  whereas media enrichment MUST read image bytes to decode and run Vision/CLIP. That conservative-fetch policy is
  genuine new work, scheduled in M1.5.
