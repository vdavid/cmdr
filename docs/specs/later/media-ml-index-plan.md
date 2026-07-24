# Image ML index: searchable photos by text, tags, faces, and OCR

## Status (2026-07-16)

- **Shipped:** M1 (plumbing + OCR search), M1.5 (SMB opt-in enrichment), M2 (tags + image-similarity) all landed in the
  initial effort (~2026-07-14); M3 (natural-language CLIP semantic search) and M6 (photo search as an Ask Cmdr + MCP
  tool) landed on this branch (2026-07-16). A separate polish effort then hardened
  everything shipped (dead-start fixes, never-scored detection, progress-indicator integration, privacy retro-delete,
  settings moved to AI › Image search).
- **Parked (deliberate):** M4a + M4b (faces: detect/embed/cluster, then naming + durable identity + People UI) and M5
  (LLM captions). David wants to be closer in the loop for faces before building them; captions are genuinely optional.
- **M3 deviation:** the CLIP path is complete but **gated off until David uploads the model artifacts** — that upload is
  the one remaining David-only handoff. The feature stays dark until the `.mlpackage` is hosted.
- **M6 deviation:** photo search shipped as an **agent/MCP tool, not a `cmdr://` resource** (see the M6 note below).

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

- **`src-tauri/src/indexing/`** — per-volume SQLite index DBs (one writer thread per DB; local + SMB + MTP each get
  their own DB), recursive size aggregates, `ReadPool` for reads, per-volume registry (`INDEX_REGISTRY`), freshness
  model, phase events. **Hard invariants we must respect:** the index is a **disposable cache** (schema mismatch /
  corruption ⇒ delete + recreate, no migrations); **one writer thread per DB**; `platform_case` collation on every
  connection; reconciler/event loops hold a READ connection only; **no rayon for macOS-framework calls** (dedicated OS
  threads + `objc2::rc::autoreleasepool`); one global 16 GB memory watchdog (`stop_all_indexing`, **indexing-specific —
  it does not know about other subsystems**). FDA gates only `root` auto-start.
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
- **`src-tauri/src/importance/`** — the shipped sibling that already solved most of this plan's hardest plumbing
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
   - **Gates (a) + (c) RESOLVED by spike** (2026-06-30; `../../notes/clip-coreml-rust-spike.md`). The Core ML text encoder
     and the Rust round-trip both work: a minimal `objc2-core-ml` 0.3.2 spike loaded a compiled model, predicted, and
     returned an embedding **bit-identical** to the `coremltools` reference; text→image alignment runs correctly
     on-device (ANE); native Core ML adds **zero binary weight**; the `unsafe` surface is ~12–15 mechanical objc2 calls
     behind a ~150–250 line safe wrapper (`encode_text`/`encode_image`). **The real constraint is licensing, not
     capability:** Apple's MobileCLIP/MobileCLIP2 weights are **research-only** (Apple ML Research Model Terms of Use —
     verified against Apple's `LICENSE_MODELS`), so a commercial product can't ship them. **Resolution, no architecture
     change: use a commercially-licensed CLIP** (OpenAI CLIP = MIT, or SigLIP 2 = Apache-2.0), converted once with
     `coremltools` and shipped pre-converted — the plumbing is model-agnostic. Trade-off: heavier than MobileCLIP-S0,
     still fine on the ANE.
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

Each milestone is independently shippable and leaves the tree green. Sequential is the default.

### M1 — Plumbing + OCR search (zero model download, proves the whole pipeline) — ✅ SHIPPED (~2026-07-14)

The thinnest end-to-end slice: decode + Vision FFI + path-keyed per-volume `media.db` + a search surface, with **no
model download and no vector math**. Its original "prove the risky plumbing" premise is now much weaker: the lifecycle
bus (Decision 7), the per-volume disposable store (Decision 3), the consumer read API (Decision 8), the scheduler
skeleton, the registry sweep, the coalescing coordinator, and the dedicated-OS-thread-not-rayon discipline all have a
SHIPPED sibling in `importance/` to COPY (verified 2026-07-13). So M1 splits cleanly into **copied** plumbing and
**genuinely new** work.

- **Copied/adapted from `importance/` (not invented):** the per-volume store scaffolding (`store/` — `platform_case`,
  delete+recreate on `SCHEMA_VERSION` mismatch, path-keyed rows, one long-lived writer per volume via a
  `WriterRegistry`; no per-row generation stamp — staleness is `(path, mtime[, size])`, Decision 3), the scheduler that
  subscribes to `indexing/lifecycle_bus.rs` + sweeps `ready_volumes_with_kind()` + coalesces per `volume_id` (a
  `PassCoordinator` clone), and the `media_index` read API modeled on `ImportanceIndex` (`read/`). Read `importance/`'s
  `store/` + `scheduler/` + `read/` and port them; don't re-derive.
  - **`media_index` gets its OWN `start()`, mirroring `importance/`'s ordering** (subscribe to registrations → sweep
    `ready_volumes_with_kind()` → wire per-volume subscriptions): it can't piggyback `importance/`'s subscription.
    Because `app.manage` is keyed by type, an `Arc<MediaScheduler>` coexists fine alongside `importance/`'s scheduler.
- **Genuinely new in M1 (where the real work is):**
  - **Image-qualification predicate FIRST** (M1's literal first need): decide what index entry counts as "an image"
    (UTType via `UTTypeConformsTo`/Vision, with an extension fast-path), and explicitly classify Live Photos
    (still+motion pair), videos (out of scope here, note it), and RAW+JPEG / `.aae` sidecar pairs (enrich the primary,
    skip sidecars).
  - **The Vision OCR FFI**: the `VisionBackend` trait + real `objc2-vision` impl (OCR only), on dedicated OS threads
    with `objc2::rc::autoreleasepool` and a per-block `// SAFETY:` (`src-tauri/CLAUDE.md`); plus a `fake` backend so the
    scheduler/store/GC/search logic is testable without ANE (mirroring `importance/`'s fake seams).
  - **The `media.db` schema + the FTS5 OCR table**, `media_status` path-keyed staleness (`(path, mtime[, size])`), the
    **GC reconcile** (below), and the **search OCR query path** (below). SMB out of M1 — local only.
  - **A minimal OS/Vision-engine-version stamp on OCR rows, from the start.** `media_status` (or `media_ocr`) rows carry
    a lightweight OS/Vision-engine-version component so an OS upgrade that changes the Vision OCR engine re-runs OCR.
    The `(path, mtime, size)` staleness key alone never re-runs on an engine change, so coverage would go silently stale
    (the same text, decoded by a newer engine, would keep its old result forever). This is data-**coverage**, not
    data-safety — OCR text is disposable — so OCR needs ONLY the OS/Vision-version component, NOT the full
    `{model id, OS version, taxonomy}` provenance stamp that faces/tags carry (Decision 4). Adding the column in M1
    avoids an awkward retrofit when the full provenance stamp first lands in M2 (tags) / M4a (faces); it unifies with
    how tags re-enrich on a taxonomy bump (M2).
  - **The memory-watchdog hook (real `indexing/` work).** Hook `media_index` cancellation into the existing indexing
    memory watchdog's stop action (Cross-cutting § Resources), rather than standing up a second 16 GB ceiling over the
    same resident pool. The watchdog is currently indexing-specific — `stop_all_indexing` "does not know about other
    subsystems" (`indexing/CLAUDE.md`) — so this IS a modification to `indexing/`, distinct from the bus (which is pure
    reuse; see Decision 7). M1 already decodes HEIC/RAW, whose RAM spikes are exactly what the ceiling defends, so wire
    it here, in the first milestone that can spike.
- **GC reconcile:** when a source file vanishes (index deletion), GC its media rows — but run the deletion sweep **only
  when triggered by a `Completed` signal off the bus (which fires post-flush), never mid-`Scanning`** (Decision 3), so
  the index-truncate window can't wipe rows for files that still exist. Design the reconcile against index deletions
  now, even though faces/embeddings arrive later.
- Vision `VNRecognizeTextRequest` → `media_ocr` FTS5 table. Decode via ImageIO downscale.
- Search: a new image-OCR query path via the `media_index` read API (Decision 8); surface "text in images" through
  `query-ui`.
- **Result presentation + "why matched" (a path list is a poor photo-search UX).** Present results as a **thumbnail grid
  reusing the existing QuickLook/preview path** (NOT a `media_index`-produced thumbnail — Decision 5), each result
  showing a **match reason** (the highlighted OCR snippet in M1; growing to "matched: person Dóri + tag beach" through
  M3/M4b). M1 ships the basic grid + OCR-snippet reason.
- **Coverage honesty in the main results.** If enrichment is still running or off, the main `query-ui` results say so
  ("still indexing, results may be incomplete") — never a confident-looking result set that's really 30% of the library
  (Cross-cutting § Honest progress + coverage).
- Settings: master "Index image contents" toggle (off by default); local-only in M1. **Disabling it** stops in-flight
  work and offers to delete `media.db`; the durable identity store survives with a notice (Decision 6). A minimal
  **per-volume enrichment state** ("indexing / done / paused") surfaces here; the full count-and-ETA progress lands in
  M2.
- **Docs:** new `media_index/CLAUDE.md` + `DETAILS.md` (sibling, enforced); `media_index/` row in
  `docs/architecture.md`; note the new search read-API boundary in `search/DETAILS.md`; new settings string in the i18n
  catalog. The lifecycle bus is already documented in `indexing/DETAILS.md` (it ships) — link it from
  `media_index/DETAILS.md`, don't re-document the mechanism (single-source).
- **Tests:**
  - _Smoke first:_ an FTS5 availability check (`CREATE VIRTUAL TABLE … USING fts5`) before building on it — Decision 2's
    build-flag worry is closed (`agent/store` proves `bundled` compiles FTS5), so this is a cheap runtime guard, not a
    gate on the milestone.
  - _TDD red→green (pure/risky):_ the **path-keyed staleness predicate** (stale vs `(path, mtime, size)`); the **GC
    reconcile is deletion-driven** (a _known_ entry deleted ⇒ rows gone) **and must NOT fire during an in-progress
    rescan** (transient truncate absence ⇒ rows kept; the sweep runs only when a `Completed` signal fires) — this is a
    data-safety test, not a nicety; **GC fires on a bus EDGE, never a poll** — assert the sweep consumes a `Completed`
    **transition** (copy `importance/`'s `borrow_and_update` / `has_changed` edge-triggered consumption), never a
    `borrow()` of the retained state, because the `watch` retains the last `Completed` across a new scan's truncate
    window, so a poll of retained state could observe a stale `Completed` mid-truncate and sweep live rows
    (edge-triggered = safe; this test makes the safety property explicit); the **scheduler throttle/cancel decision**;
    and **FTS query building** — fail first for the right reason, then implement (`tdd-red-green`).
  - _After:_ scheduler integration test using the **fake `VisionBackend`** over a synthetic index (no FFI); a macOS-
    gated integration test running real Vision OCR on a committed fixture image (asserts known words). The bus mechanism
    itself (late-subscriber replay, generation, registration) is already tested in `indexing/lifecycle_bus.rs`; M1's own
    bus tests assert only that the **media_index scheduler reacts** — a `ScanCompleted` wakes it, and **a volume
    Fresh-at-launch with no new scan still gets scheduled** via the registry sweep (copying `importance/`'s
    `ready_volumes_with_kind` sweep + coalescing tests).
  - _E2E:_ a Playwright smoke that the settings toggle persists (this IS the one small E2E for M1).
- **Checks:** `pnpm check --fast` iterating; full `pnpm check` at end (clippy, rust tests, i18n-coverage,
  `claude-md-details-sibling`, `docs-reachable`, file-length). Smoke-test the scheduler on 1–2 images first
  (`test-infra-smoke-first`).

### M1.5 — Network-volume enrichment (SMB opt-in), validated on the NAS — ✅ SHIPPED (~2026-07-14)

Network enrichment is the **headline use case**, not an afterthought: the user's photo library lives on a NAS
(`/Volumes/naspi`, over SMB), and "search my NAS photos" is the whole point. So it earns a scheduled milestone, right
after the local pipeline proves out, rather than being deferred to "impl time." It **can't** be deferred, because the
`importance/` reuse precedent that carries the rest of the plan does NOT cover the one part M1.5 needs. `importance/`
carries a hard rule — "NEVER a filesystem syscall against an SMB/MTP mount — read only the local index DB"
(`importance/CLAUDE.md`, verified 2026-07-13) — so importance never pulls a single byte over the wire. Media enrichment
is the opposite: it MUST read image **bytes** off the SMB mount to decode and run Vision/CLIP. So the bus, registration,
and scheduling wiring is a straight copy from `importance/` (Decision 7), but the **byte-fetch policy is genuinely new
work with no sibling to copy** (Decision 6). This milestone is scoped to the **OCR pipeline only** — it inherits M1's
Vision OCR and adds no new models, so it proves the transport without model-download risk piled on top.

- **Per-volume SMB opt-in (off by default).** A per-volume opt-in in Settings, distinct from M1's master toggle: turning
  on image indexing does NOT auto-enrich network volumes. Extends M1's master toggle + per-volume state. **Disabling a
  network volume** follows M1's disable-data behavior (stop in-flight work via the cancel token, offer to delete that
  volume's `media.db` rows; the durable identity store survives with a notice — Decision 6).
- **The conservative-fetch policy with teeth** (defined in Decision 6 — scheduled and built here): idle-gated (enrich
  only when the volume isn't serving foreground work), bandwidth-bounded, bounded concurrency, resumable, with
  **on-demand-per-folder-visit** as a candidate strategy to weigh (so a rarely-browsed archive isn't dragged over the
  wire wholesale). **Dependency to flag — "idle-gated" needs a foreground-activity/idle signal to gate on, and there is
  NONE in `indexing/` today** (grep verified 2026-07-13 — the only `Idle` is `ActivityPhase::Idle`, an indexing
  work-state, not a user-foreground signal). Treat the idle signal as **new work**, not reuse.
- **The "always index this folder / this volume" override** (user-set) — M1.5 owns building it; Decision 6 and
  Cross-cutting § Importance-prioritized enrichment give the why, but no milestone owned it before. Navigation-based
  importance **starves** a NAS photo archive: a library the user rarely browses folder-by-folder scores LOW everywhere,
  so importance-first ordering plus the M2 slider would defer the user's actual photos indefinitely — the opposite of
  intent. The override forces enrichment regardless of importance. (Optionally weigh a **photo-density**
  importance-input candidate — a folder that's mostly images is likely an archive regardless of visit count — but keep
  it a candidate, not load-bearing, per the plan's "measure, don't hardcode" posture; don't ship importance-first
  without at least the manual override.)
- **Resumability across unmount** (Cross-cutting § Cancellation defines it — scheduled and tested here): a mid-pass
  unmount is not a crash. Abandon the in-flight item cleanly, **KEEP every completed path-keyed row** (they're valid),
  and mark the volume "paused, resumes when reconnected" — it resumes via the bus's registration path on remount.
  **Never mark rows permanently failed on a disconnect** — a disconnect isn't a bad file.
- **Offline search after unmount** (a property of Decision 8 — proven here on `/Volumes/naspi`): the `media_index` read
  API answers from `media.db` with the NAS unplugged, so a user can search their NAS photos while disconnected. Prove it
  with a test that unmounts and still returns results.
- **GC vs unmount interaction:** a volume that is merely disconnected must **NOT** GC its rows. Only an index deletion
  on a **completed** scan GCs rows (Decision 3); a disconnect is neither, so a paused volume's coverage survives intact
  until it reconnects.
- **MTP stays on-demand, never background.** Keep `importance/`'s precedent: `ScoringPolicy::for_kind` **excludes MTP**
  from background scoring (verified 2026-07-13). MTP media enrichment is likewise **on-demand-per-visit, NOT a
  background sweep** — a phone or camera on MTP is transient and slow, so enrich what the user opens, when they open it.
- **Docs:** `media_index/DETAILS.md` — the network-fetch policy (idle/bandwidth/concurrency gating) and the
  resumable-across-unmount behavior; per-volume opt-in strings in the i18n catalog with `@key` descriptions; a note in
  `docs/architecture.md` that `media_index` enriches opt-in network volumes.
- **Tests:**
  - _TDD red→green (data-safety + risky):_ the **conservative-fetch decision** (the idle/bandwidth gate defers or
    proceeds correctly, over a fake clock/bandwidth signal); the **resumable-across-unmount row preservation** (a
    mid-pass unmount keeps completed rows, marks the volume paused, and never marks rows failed); the **GC must NOT fire
    on a mere disconnect** (a disconnect is not a completed-scan deletion ⇒ rows kept) — this is a data-safety case,
    fail first for the right reason.
  - _After:_ a macOS-gated integration test against **`/Volumes/naspi`** plus the existing `test/smb-servers/` fixtures
    — enrich over SMB, unmount, and assert offline search still answers from `media.db`.
  - _E2E:_ the per-volume SMB opt-in persists.
- **Checks:** full `pnpm check` + `--include-slow` before wrapping (the network + resumability paths).

### M2 — Tags + image-similarity (Vision-only, zero download) — ✅ SHIPPED (~2026-07-14)

- Vision `VNClassifyImageRequest` → `media_tags` (label + score), folded into the FTS index so tags are keyword-
  searchable. Vision `VNGenerateImageFeaturePrintRequest` → `media_embedding` (image↔image only). **Stamp `media_tags`
  rows with the tag-taxonomy-version component of the provenance key** (Decision 4), so an OS upgrade that changes the
  Vision taxonomy drives a re-tag of stale rows rather than leaving mislabeled tags.
- The **vector-store trait** lands here: brute-force cosine impl first (no `sqlite-vec`); "Find similar images" + dedup
  grouping.
- **Settings: the importance-threshold slider (the "how deep do I index?" control).** M1 shipped the master
  off-by-default "Index image contents" toggle; M2 adds a slider under it for the **lowest folder-importance level** the
  user wants image-indexed. It reads the same signal the scheduler enriches by (Cross-cutting § Importance-prioritized
  enrichment), so the control and the behavior can't drift.
  - **Human-facing named buckets as the primary labels** (the user's explicit "make it feel nice" ask). Label the
    positions in plain language — "Only my most-used folders" ⟶ "Everywhere, even rarely-used" — with the live honest
    count as the anchor beneath, NOT a raw `0.0..=1.0` or a bare percent (the typed value stays under the hood). Pick a
    **sensible default position** that covers active folders and skips junk, and say why in the copy.
  - **Live preview, honest numbers.** As the user drags, show how much the current threshold covers: query
    `ImportanceIndex::above_threshold(t)` for the folder count and the drive index (the image-qualification predicate)
    for the file count, rendered like "About 38,900 images across 1,240 folders" — thousands separators, sentence case,
    `t()`-resolved from the i18n catalog with a `@key` description, all counts honest. The preview set counts only
    **ENABLED** volumes — `(importance ≥ t) AND (volume opted-in)` — never promising images on a non-opted-in SMB/MTP
    volume. The **multi-volume aggregate** composes per-volume counts and voices per-volume state ("about 38,900 images;
    naspi still scanning"), never a single confident wrong number. On drag, also show the **incremental work + ETA**
    ("this adds about 12,000 images, ~20 min"), tying the slider to the enrichment progress surface (§ Honest progress +
    coverage). Compute counts in Rust behind an IPC command (backend does the work, thin frontend), debounced so
    dragging doesn't thrash queries.
  - **Make it feel nice** (design-principles § delightful): a smooth slider with the covered counts updating live, the
    floor made legible ("junk like `node_modules` is always skipped"), and `prefers-reduced-motion` respected. The
    slider only refines the master toggle — it's disabled/hidden when image indexing is off.
  - Threshold semantics are typed across IPC (a bounded level or a `0.0..=1.0` value), never a string
    (`no-string-matching`); the scheduler reads it via the same importance read API, so a below-threshold folder is
    deferred/skipped, not enriched.
- **Full enrichment progress surface** (§ Honest progress + coverage): the per-volume progress/coverage indicator with
  honest counts, ETA, and state ("12,000 of 38,900 images indexed on naspi, about 15 min left") lands here, sharing the
  slider's count machinery. M1 shipped only the minimal per-volume state.
- **Per-folder "don't index for photo search" exclusion** (Cross-cutting § Privacy): the user-set exclude that protects
  a sensitive high-importance folder (`~/Documents/IDs/`) the slider alone can't. Small; sits next to the slider.
- **Docs:** `media_index/DETAILS.md` — note Vision's fixed tag taxonomy and **anchor the count**
  (`~1,303 on <macOS version>, verified <date>`) per `docs.md`; the slider + preview design and the covered-count IPC
  command; architecture note for "find similar".
- **Tests:** _TDD red→green:_ cosine/top-k ranking, dedup threshold, tag-score filtering; the **covered-count query**
  (folders above `t` from a synthetic `ImportanceIndex` + images from a synthetic index) and the **scheduler defers a
  below-threshold folder**. _After:_ fake-backend scheduler extended to tags + feature prints. _E2E:_ "Find similar"
  from a result; the slider updates its preview and persists.
- **Checks:** as M1 + `--include-slow` before wrapping (vector paths); a11y on the slider (AA+ contrast, screen reader,
  keyboard-operable).

### M3 — Natural-language semantic search (first model: a commercially-licensed CLIP via Core ML) — ✅ SHIPPED (2026-07-16, this branch; gated off until David uploads the model artifacts)

- **Gate RESOLVED (spike, 2026-06-30):** the Core ML text encoder + `objc2-core-ml` round-trip work (bit-identical to
  the `coremltools` reference), so the native path stands. **Use a commercially-licensed CLIP — NOT Apple's MobileCLIP**
  (research-only weights, can't ship; see Decision 1 and `../../notes/clip-coreml-rust-spike.md`). Candidates: OpenAI CLIP
  (MIT) or SigLIP 2 (Apache-2.0); convert once with `coremltools` on a dev box, ship the pre-converted `.mlpackage`
  (image + text towers). **Verify the chosen model's license + Core ML conversion fidelity at impl time.**
- Wrap the `objc2-core-ml` calls in a safe `encode_text`/`encode_image` API (~150–250 lines, per-block `// SAFETY:`).
  **Compile the `.mlpackage` to `.mlmodelc` on-device at first run and cache** (`.mlmodelc` is OS-version-specific —
  don't bundle a prebuilt one); ship the `.mlpackage`.
- New model-install path (Decision 9): generic archive unpack + **checksum** verify, on-demand download via
  `download.rs`.
- Image embeddings → `media_embedding`; **query-time text encode runs async/off the IPC thread** (Decision: never on the
  synchronous IPC handler — it would block the app per `src-tauri/CLAUDE.md`), with the same autoreleasepool discipline.
  Text vector → vec search, wired into `search/` + `query-ui` as the headline "search photos by description". The
  thumbnail-grid results (M1) grow a **semantic match reason** here, and the main-results coverage-honesty line (still
  indexing ⇒ say so) rides the same surface (Cross-cutting § Honest progress + coverage).
- Settle brute-force vs `sqlite-vec` cutover on a real library; record in `docs/notes/`.
- **Docs:** `media_index/DETAILS.md` model section (evidence-anchored: id, size, source, license, date); architecture
  map.
- **Tests:** _TDD red→green:_ text-query → vector-search with a fake encoder (deterministic vectors); the brute-force↔
  store selection boundary. _After:_ macOS-gated embed-a-fixture + text-query asserts the right image ranks top. _E2E:_
  type a description, get the photo.
- **Checks:** full `--include-slow`; `cargo deny` + ≥14-day version for any new crate (`use-latest-dep-versions`).

M4 is split because "detect + embed + cluster + hardened durable store + re-attach + People UI + a11y" is two
milestones; M4a de-risks the faces FFI/pipeline before the curation/durable-store/UI surface.

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

### M6 (follow-on, low-scope) — photo search as an Ask Cmdr agent tool and an MCP tool — ✅ SHIPPED (2026-07-16, this branch; shipped as a tool, not a `cmdr://` resource)

Once `media_index` lands and surfaces through the search read API, photo search becomes a natural read-only tool for the
in-app agent and the MCP server — "find my passport scan", "photos of Dóri at the beach", answered in chat. Forward-
looking, not a blocker for M1–M5; a short milestone or a future note.

- **What egresses here is derived TEXT CONTENT, not "just metadata"** (privacy framing that shapes the whole tool). When
  the agent runs against a cloud provider, the OCR snippet, caption, tags, and person names this tool returns ARE
  sensitive derived content — a passport scan's OCR snippet IS the passport number. So on the cloud path this tool is
  gated by the same M5 egress consent + audit log (Decision 10), not exempt because "it's only metadata".
- **Agent tool:** add a "photos by description / OCR text / person" read family to `agent/tools/` as a
  `consumers: [Agent], access: Read` entry in the shared `mcp_tools!` registry, whose handler calls the `media_index`
  read API and only SHAPES the result (reuse-the-core rule; see `agent/tools/CLAUDE.md`). It keeps the "no image bytes /
  no thumbnails to the provider" property as a **typed result DTO the tool structurally can't populate with bytes**
  (text fields only: matched OCR snippet, person name, tag, path), enforced by a test — not just prose. A new `ToolId`
  variant + its name in `EXPECTED_AGENT_TOOL_NAMES` / `ToolId::KNOWN`.
- **MCP tool + resource:** the same registry entry exposes it to external agents via `mcp/executor/`, alongside the
  existing `cmdr://importance` / `cmdr://indexing` surfaces.
- **Honest coverage** (spec §2.4): the result voices its own staleness/coverage (image indexing off, a volume still
  scanning, a below-threshold folder not enriched) — never a confident empty answer that's really "not indexed yet".
- Depends on: M1 (OCR text search) at minimum; person search needs M4b, semantic description search needs M3. Scope it
  to whatever has shipped. **Tests:** fake-backend tool dispatch + the coverage-honesty path + a test that the result
  DTO is text-only (can't carry image bytes); keep it small.

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
- **Resources + memory ceiling.** Enrichment runs on dedicated low-priority OS threads (not rayon), bounded concurrency,
  cancel token, starts only after the base index signals ready, yields to foreground. The existing watchdog already
  measures **process-wide** resident memory but only stops _indexing_ — so **hook `media_index` cancellation into that
  same watchdog's stop action**, rather than standing up a second independent 16 GB ceiling (two ceilings over one
  shared resident pool each see headroom and can sum to ~2×). Decode of full-res HEIC/RAW + Core ML can spike RAM, so
  this must be **wired**, not asserted. Note the standing cost too: `media_index` adds a **THIRD long-lived writer
  thread per volume** (index + importance + media) plus a third set of per-volume `watch` listeners and a registration
  listener. Fine at a few-volumes scale, but not free — it scales 3× per mounted volume.
- **Honest progress + coverage (a core Cmdr transparency value, not polish).** Background enrichment must surface honest
  per-volume progress with counts, ETA, and state ("12,000 of 38,900 images indexed on naspi, about 15 min left"; "naspi
  still scanning") — a minimal per-volume state in M1, the full count-and-ETA surface in M2 (where it shares the
  slider's count machinery). And a search run while enrichment is incomplete must voice it in the MAIN `query-ui`
  results ("still indexing, results may be incomplete"), not only in the M6 agent tool — pull the coverage-honesty
  requirement up into M1/M3's query surface.
- **Query-time vector residency.** Brute-force cosine is cheap, but loading ~200 MB of embedding BLOBs from `media.db`
  per text query is not — and it's real work that must run **off the synchronous IPC thread** (alongside the text
  encode, not just it). Mirror `search/`'s warm in-memory arena (`SEARCH_INDEX`): keep a **resident vector cache**
  (load-once, invalidated on writes), counted against the same watchdog budget. Embedding storage on disk is small (512
  floats ≈ 2 KB/image; int8 if huge).
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
  - **Per-folder "don't index for photo search" exclusion** (user-set) is the privacy complement to the opt-in: the
    threshold slider can't protect a high-importance `~/Documents/IDs/` folder (the user uses it, so it scores HIGH and
    would be indexed). Mirror `importance/`'s floor/denylist shape, but user-set. Small to build; lands next to the
    slider in M2.
- **i18n.** Every user-facing string via the catalog with a `@key` description (`cmdr/no-raw-user-facing-string`).
- **Dependencies.** `objc2-vision`, `objc2-core-ml`, maybe a clustering crate / `ort` fallback / `sqlite-vec` binding:
  each needs `cargo deny check` + a verified ≥14-day-old version (`use-latest-dep-versions`, project `dependencies`
  rule).
- **No string-matching for classification** (`no-string-matching`): typed enums for model/provider/consent/identity-
  state across IPC; the frontend never branches on message substrings.

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
- **`sqlite-vec` adoption cost** (load_extension feature + notarization/signing) if brute-force is outgrown.
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
