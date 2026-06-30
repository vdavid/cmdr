# Image ML index: searchable photos by text, tags, faces, and OCR

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

Three existing subsystems this plugs into. Read their colocated `CLAUDE.md` + `DETAILS.md` before touching them. Claims
below were verified against the code on 2026-06-29 (file refs may drift — confirm with `codegraph_search`).

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
  - **Phase events are frontend-only (verified):** `set_phase_for` does one outward thing — `.emit(app)` a Tauri event
    _to the webview_. There is **no in-process backend pub/sub** a Rust subsystem can subscribe to. Network volumes emit
    only `Scanning → Live` (no Aggregating/Reconciling). (Drives Decision 7.)
- **`src-tauri/src/ai/`** — on-device model **download** infra. Verified reality: `download.rs` is a generic resumable
  HTTP GET (**genuinely reusable**); `install.rs` is **GGUF/llama-server-specific** orchestration; `extract.rs` only
  **`fs::copy`s the bundled llama-server binary** (no archive extractor); verification is **file-size only**, not
  checksum. We reuse `download.rs`; the rest of the model-install path is **new code** (Decision 9). Also: the cloud
  BYOK client (`client.rs`, `genai`) and the `is_local_ai_supported()` Apple-Silicon gate shape.
- **`src-tauri/src/search/`** — read-only, one-way consumer of `indexing/` via a defined read surface (`ReadPool`,
  `IndexStore`); in-memory filename index; **pure** `engine.rs` (no I/O); NL→`SearchQuery` AI translation
  (`search/ai/`). Image search is a **new query path** (vectors + FTS), and it must reach `media.db` **through a
  `media_index` read API that mirrors the `ReadPool`/`IndexStore` boundary — never a raw `rusqlite` dependency**
  (Decision 8). It surfaces through the same `query-ui` primitives.

macOS FFI precedent already in the codebase: `objc2` + Cocoa/ObjC threads with autoreleasepools, `NSWorkspace`,
`QLPreviewPanel`, swizzling, `security-framework`. Vision/Core ML via `objc2-vision` / `objc2-core-ml` fit this — but
each `unsafe` block needs a specific `// SAFETY:` per `src-tauri/CLAUDE.md` (Decision 1).

## Key decisions (with intent — adapt if reality differs, but know the why)

1. **macOS-native inference, with a pre-validated `ort` fallback and explicit verification gates.** Use Apple **Vision**
   (OCR, face detection, scene tags, image feature print) and **Core ML** (MobileCLIP, ArcFace) through `objc2-vision` /
   `objc2-core-ml`, and **Foundation Models** (Swift bridge) for the optional caption path.
   - _Why:_ macOS-only app → native frameworks give ANE acceleration and the smallest binary (no bundled ONNX Runtime
     native lib). "Ideal over cheap" + "rely on macOS where reasonable."
   - **Gates (a) + (c) RESOLVED by spike** (2026-06-30;
     [`docs/notes/clip-coreml-rust-spike.md`](../../notes/clip-coreml-rust-spike.md)). The Core ML text encoder and the
     Rust round-trip both work: a minimal `objc2-core-ml` 0.3.2 spike loaded a compiled model, predicted, and returned
     an embedding **bit-identical** to the `coremltools` reference; text→image alignment runs correctly on-device (ANE);
     native Core ML adds **zero binary weight**; the `unsafe` surface is ~12–15 mechanical objc2 calls behind a ~150–250
     line safe wrapper (`encode_text`/`encode_image`). **The real constraint is licensing, not capability:** Apple's
     MobileCLIP/MobileCLIP2 weights are **research-only** (Apple ML Research Model Terms of Use — verified against
     Apple's `LICENSE_MODELS`), so a commercial product can't ship them. **Resolution, no architecture change: use a
     commercially-licensed CLIP** (OpenAI CLIP = MIT, or SigLIP 2 = Apache-2.0), converted once with `coremltools` and
     shipped pre-converted — the plumbing is model-agnostic. Trade-off: heavier than MobileCLIP-S0, still fine on the
     ANE.
   - (b) "Foundation Models is multimodal (image input) as of macOS 26" — still unverified; gate at M5 (optional).
   - **Bounded fallback:** if the chosen CLIP won't cleanly convert to Core ML (or loses accuracy), run _that one model_
     via `ort` + CoreML execution provider — but that costs **~25–35 MB of native binary** (`libonnxruntime.dylib` +
     ONNX artifacts; `ort` is pre-1.0) the native path avoids. Per-model last resort, not the default.

2. **Vectors in SQLite, never Postgres — brute-force first.** Below ~100k vectors, brute-force cosine in Rust (low-ms,
   zero deps) is enough and ships in M2/M3. `sqlite-vec` is a **loadable extension** and our `rusqlite` is built without
   `load_extension` (verified: `features = ["bundled","collation","fallible_uint"]`); enabling it also runs into
   hardened-runtime/notarization constraints for loading a dylib into a signed app. So **`sqlite-vec` is a real
   build+signing project, not a flag flip** — adopt it only if a real library crosses the threshold, behind the same
   vector-store trait. **FTS5 is expected-fine** (rusqlite `bundled` almost certainly compiles it in) but the whole M1
   OCR headline rests on it, so **gate it with a `CREATE VIRTUAL TABLE … USING fts5` smoke at M1 start** — if absent it
   needs a `libsqlite3-sys` build flag, which isn't free. _Why:_ a single user's library is small; Postgres+pgvector is
   multi-user server overhead. Kills the "ship/download Postgres" question entirely.

3. **A separate per-volume media DB (`media.db`), keyed on PATH identity.** Don't add ML tables to the index DB.
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
     reconciler's actual delete of a **known** entry (the index "deletes only a known entry"), and/or runs **only
     against a completed/Fresh scan** — never while a volume is `Scanning`. (LOCAL rescans of a populated index
     reconcile in place via `local_reconcile.rs`; the hazard is specifically the truncate path.)

4. **Disposable derived data vs durable human work — split the stores, and harden the durable side.** Detections,
   embeddings, tags, OCR text, and _computed_ clusters are **disposable** (`media.db`, regenerable). **Human work**
   survives a wipe in a separate app-data store modeled on `favorites/` (atomic JSON, seed-once, pure versioned core).
   Human work is **not just names** — it includes **merge/split/"not this person" corrections**. The durable store
   holds, per named/curated identity: the assigned name, the corrections, and one or more **embedding centroids tagged
   with the embedding model's id+version**.
   - **Re-attach after a wipe is conservative, not silently automatic** (this is the data-safety crux):
     - If `media.db` survived (the common case — a crash, not a schema wipe), the face rows and their identity links
       survived too; nothing to re-attach. (Identity links by `face_id`, not by path — don't rebind faces by path, which
       is wrong for multi-face photos.)
     - On a true face-embedding regenerate, re-attach candidates by centroid cosine **only when the centroid's model
       id+version matches** the current model. **Model mismatch ⇒ do NOT cosine-match across incompatible spaces** (it
       would mislabel); instead mark identities "needs re-confirm" and re-surface them in the People UI.
     - **Negative/cannot-link corrections are hard vetoes consulted on every re-attach AND re-cluster.** A face the user
       removed from "Dóri" ("not this person") will, after a regenerate, again be cosine-nearest to Dóri's centroid — so
       a purely positive matcher would silently re-introduce the exact mislabel the user fixed. Any candidate suppressed
       by a durable negative is **never auto-attached**, only offered as "needs re-confirm." Likewise re-clustering must
       honor durable cannot-link/must-link, or a manual split silently re-merges. **Cannot-link is the hard
       constraint:** when a transitive must-link closure (a–b, b–c) would force a cannot-link violation (a–c), the
       must-link is dropped and flagged, never silently applied. (This is the hole positive-only re-attach leaves; the
       M4b tests target it explicitly, including the transitive-conflict case.)
     - Even on a clean match, a **high threshold** plus a lightweight "Still <name>?" confirmation for low-confidence
       re-attaches — a silent false attribution is worse than asking. Mis-attach is a first-class failure mode here, not
       just "failed to attach."
   - _Why:_ the index is explicitly throwaway; we must never silently lose or corrupt the human labeling/curation. This
     is the single most important data-safety decision in the plan, and the M4 red→green tests target exactly it.

5. **Feed a downscaled in-memory decode to the models, never the original.** Decode via ImageIO/CoreGraphics (native;
   HEIC/RAW), downscale to model input (~224–512 px), feed the `CGImage`. No thumbnail _files_. _Why:_ CLIP/OCR need
   small inputs; decoding originals twice is the dominant cost.

6. **Opt-in, gated, conservative by default.** Whole feature off until enabled; **faces a separate opt-in** with privacy
   copy. Heavy/identity paths gate on **Apple Silicon** (`is_local_ai_supported()` shape); Vision OCR/tags work on older
   Macs, so don't over-gate. **Local volumes enrich by default when enabled; SMB/MTP are opt-in per volume** and
   conservative (no pulling every image over the network by default).

7. **Enrichment subscribes to a NEUTRAL in-process bus AND does an initial registry sweep.** Since phase events are
   frontend-only today, add a small neutral publish surface in `indexing/` emitted from the existing neutral chokepoint
   **`apply_freshness_event_on`** (where `FreshnessEvent::ScanCompleted` funnels for BOTH local and network — verified),
   alongside the Tauri `.emit`. `media_index` (and any future consumer) subscribes; `indexing/` publishes without
   knowing who listens.
   - **Use a per-volume `watch` (last value retained), not `broadcast`.** `tokio::sync::broadcast` does **not** replay
     backlog to a receiver created after a `send`, so a `ScanCompleted` fired during `setup()` before `media_index`
     subscribes is lost. A per-volume `watch` retains the last state for a late subscriber.
   - **An event subscription is not enough — sweep `INDEX_REGISTRY` at startup.** A previously-completed volume can load
     ready at launch from persisted state **without re-firing a scan/completion event** (freshness loads from
     `meta.scan_completed_at`, not a fresh scan). A scheduler that only waits for future events would never enrich an
     already-indexed volume after a restart — the common case. So on startup, scan the registry for already-ready
     volumes and schedule them, then rely on the bus for subsequent transitions. Guarantee subscribe-before-publish
     ordering. **Scheduling is idempotent/coalescing per `volume_id`:** the sweep and a concurrent startup-scan's
     `ScanCompleted` can both target one volume — a pass already running or queued sets a re-run flag instead of
     enqueuing a second (single-writer + the `(path, mtime, size)` predicate make the duplicate a near-no-op anyway, but
     don't rely on that for correctness). Cover with a coalescing test in M1.
   - _Why this shape:_ satisfies "subscribe, don't poll" **without** making `indexing/` depend on `media_index` (which
     would reverse the clean one-way direction). The bus is a neutral publish surface, not a back-reference.
   - **Network caveat:** SMB/MTP emit only `Scanning → Live` at the phase layer, but **both kinds fire
     `FreshnessEvent::ScanCompleted`** (and `IndexAggregationCompleteEvent`) — drive "ready to enrich" off that, not off
     a phase the network path never emits.

8. **`search/` reaches `media.db` only through a `media_index` read API** that mirrors `ReadPool`/`IndexStore` (owns the
   connection pool, `platform_case` registration, one-writer discipline). `search/` stays a read-only consumer; it must
   not take a raw `rusqlite` dep on `media.db`, or the collation/one-writer invariants leak into a second subsystem.

9. **Model install is new code (generic archive unpack + checksum verify), reusing only `download.rs`.** Core ML models
   ship as `.mlpackage` **directory bundles** (typically zipped); nothing in `ai/` unpacks an archive, and `ai/`
   verifies size only. M3/M4 add: a generic archive extractor, **checksum** verification (not size-only), and a model-
   install gate distinct from the GGUF two-flag gate. Don't describe this as "reuse the install infra."

10. **The cloud is opt-in and only for premium captions** (M5), through the existing `ai/` BYOK client, behind a
    _distinct_ explicit egress consent. On-device captions (Foundation Models) are the default for that feature.

## Architecture

```
indexing/ (existing)                 media_index/ (new subsystem)                      search/ (existing, extended)
  per-volume size DB                   neutral lifecycle bus  ◄── subscribe              new image-query path:
  ReadPool (read entries)              scheduler: walk image entries (path-keyed),         text → MobileCLIP text vec → vec search
  publishes lifecycle bus ──────────►    throttle / cancel / priority, shared mem ceiling   tag/OCR → FTS5
  (neutral broadcast)                  decode (ImageIO downscale)                            face name → durable identity → clusters → paths
                                       ▼ per image, via objc2 on dedicated threads        reaches media.db ONLY via media_index read API
                                       Vision: OCR, tags, feature-print, face detect      surfaced via query-ui/
                                       Core ML: MobileCLIP embed, ArcFace face embed
                                       ▼ write (one writer/DB, platform_case, disposable)
                                       per-volume media.db (path-keyed, disposable):
                                         media_status, media_tags, media_ocr(FTS5),
                                         media_embedding, media_face, face_cluster
                                       GC reconcile vs index deletions
                                       ▼
                                       durable app-data store (survives wipe):
                                         named/curated identities + corrections
                                         + model-versioned centroids
```

Inference sits behind a Rust trait (`VisionBackend` / `MediaModel`) with a **fake backend** for tests, so scheduler,
storage, clustering, GC, and search logic are all testable without GPU/ANE.

## Milestones

Each milestone is independently shippable and leaves the tree green. Sequential is the default.

### M1 — Plumbing + OCR search (zero model download, proves the whole pipeline)

The thinnest end-to-end slice: decode + Vision FFI + path-keyed per-volume `media.db` + the neutral lifecycle bus + a
search surface, with **no model download and no vector math** — so the risky plumbing (including the corrected join key
and the Core ML-adjacent Vision FFI) is proven before any ML model lands.

- **Image-qualification predicate FIRST** (M1's literal first need): decide what index entry counts as "an image"
  (UTType via `UTTypeConformsTo`/Vision, with an extension fast-path), and explicitly classify Live Photos (still+motion
  pair), videos (out of scope here, note it), and RAW+JPEG / `.aae` sidecar pairs (enrich the primary, skip sidecars).
- New `src-tauri/src/media_index/` subsystem: per-volume `media.db` with the index's disposable-cache discipline
  (`platform_case`, delete+recreate on schema mismatch, one writer thread); **path-keyed** `media_status`
  (`(path, mtime[, size])` staleness); the **neutral lifecycle bus** in `indexing/` + the scheduler subscribing to it
  (handle both phase vocabularies; SMB out of M1 — local only); the `VisionBackend` trait + real `objc2-vision` impl
  (OCR only) on dedicated OS threads with `autoreleasepool` and per-block `// SAFETY:`; a `fake` backend.
- **GC reconcile:** when a source file vanishes (index deletion), GC its media rows. Design the reconcile against index
  deletions now, even though faces/embeddings arrive later.
- Vision `VNRecognizeTextRequest` → `media_ocr` FTS5 table. Decode via ImageIO downscale.
- Search: a new image-OCR query path via the `media_index` read API (Decision 8); surface "text in images" through
  `query-ui`.
- Settings: master "Index image contents" toggle (off by default); local-only in M1.
- **Docs:** new `media_index/CLAUDE.md` + `DETAILS.md` (sibling, enforced); `media_index/` row in
  `docs/architecture.md`; note the new search read-API boundary in `search/DETAILS.md`; document the lifecycle bus in
  `indexing/DETAILS.md`; new settings string in the i18n catalog.
- **Tests:**
  - _Smoke first:_ an FTS5 availability check (`CREATE VIRTUAL TABLE … USING fts5`) before building on it (Decision 2).
  - _TDD red→green (pure/risky):_ the **path-keyed staleness predicate** (stale vs `(path, mtime, size)`); the **GC
    reconcile is deletion-driven** (a _known_ entry deleted ⇒ rows gone) **and must NOT fire during an in-progress
    rescan** (transient truncate absence ⇒ rows kept) — this is a data-safety test, not a nicety; the **scheduler
    throttle/cancel decision**; and **FTS query building** — fail first for the right reason, then implement
    (`tdd-red-green`).
  - _After:_ scheduler integration test using the **fake `VisionBackend`** over a synthetic index (no FFI); a macOS-
    gated integration test running real Vision OCR on a committed fixture image (asserts known words); a bus test that a
    volume reaching the completion signal wakes the scheduler; **a "volume Fresh-at-launch with no new scan still gets
    scheduled" test** (the registry-sweep path, Decision 7).
  - _E2E:_ a Playwright smoke that the settings toggle persists (this IS the one small E2E for M1).
- **Checks:** `pnpm check --fast` iterating; full `pnpm check` at end (clippy, rust tests, i18n-coverage,
  `claude-md-details-sibling`, `docs-reachable`, file-length). Smoke-test the scheduler on 1–2 images first
  (`test-infra-smoke-first`).

### M2 — Tags + image-similarity (Vision-only, zero download)

- Vision `VNClassifyImageRequest` → `media_tags` (label + score), folded into the FTS index so tags are keyword-
  searchable. Vision `VNGenerateImageFeaturePrintRequest` → `media_embedding` (image↔image only).
- The **vector-store trait** lands here: brute-force cosine impl first (no `sqlite-vec`); "Find similar images" + dedup
  grouping.
- **Docs:** `media_index/DETAILS.md` — note Vision's fixed tag taxonomy and **anchor the count**
  (`~1,303 on <macOS version>, verified <date>`) per `docs.md`; architecture note for "find similar".
- **Tests:** _TDD red→green:_ cosine/top-k ranking, dedup threshold, tag-score filtering. _After:_ fake-backend
  scheduler extended to tags + feature prints. _E2E:_ "Find similar" from a result.
- **Checks:** as M1 + `--include-slow` before wrapping (vector paths).

### M3 — Natural-language semantic search (first model: MobileCLIP via Core ML)

- **Gate RESOLVED (spike, 2026-06-30):** the Core ML text encoder + `objc2-core-ml` round-trip work (bit-identical to
  the `coremltools` reference), so the native path stands. **Use a commercially-licensed CLIP — NOT Apple's MobileCLIP**
  (research-only weights, can't ship; see Decision 1 and
  [`docs/notes/clip-coreml-rust-spike.md`](../../notes/clip-coreml-rust-spike.md)). Candidates: OpenAI CLIP (MIT) or
  SigLIP 2 (Apache-2.0); convert once with `coremltools` on a dev box, ship the pre-converted `.mlpackage` (image + text
  towers). **Verify the chosen model's license + Core ML conversion fidelity at impl time.**
- Wrap the `objc2-core-ml` calls in a safe `encode_text`/`encode_image` API (~150–250 lines, per-block `// SAFETY:`).
  **Compile the `.mlpackage` to `.mlmodelc` on-device at first run and cache** (`.mlmodelc` is OS-version-specific —
  don't bundle a prebuilt one); ship the `.mlpackage`.
- New model-install path (Decision 9): generic archive unpack + **checksum** verify, on-demand download via
  `download.rs`.
- Image embeddings → `media_embedding`; **query-time text encode runs async/off the IPC thread** (Decision: never on the
  synchronous IPC handler — it would block the app per `src-tauri/CLAUDE.md`), with the same autoreleasepool discipline.
  Text vector → vec search, wired into `search/` + `query-ui` as the headline "search photos by description".
- Settle brute-force vs `sqlite-vec` cutover on a real library; record in `docs/notes/`.
- **Docs:** `media_index/DETAILS.md` model section (evidence-anchored: id, size, source, license, date); architecture
  map.
- **Tests:** _TDD red→green:_ text-query → vector-search with a fake encoder (deterministic vectors); the brute-force↔
  store selection boundary. _After:_ macOS-gated embed-a-fixture + text-query asserts the right image ranks top. _E2E:_
  type a description, get the photo.
- **Checks:** full `--include-slow`; `cargo deny` + ≥14-day version for any new crate (`use-latest-dep-versions`).

M4 is split because "detect + embed + cluster + hardened durable store + re-attach + People UI + a11y" is two
milestones; M4a de-risks the faces FFI/pipeline before the curation/durable-store/UI surface.

### M4a — Faces pipeline: detect, embed, cluster (no naming yet)

- Vision detect (`VNDetectFaceRectanglesRequest` + `VNDetectFaceCaptureQualityRequest` best crop). Download an
  **ArcFace/AuraFace Core ML** model (verify license — AuraFace commercial-friendly) via the M3 model-install path →
  embeddings in `media_face`. Cluster (agglomerative/HDBSCAN on cosine) → `face_cluster`. Search by **cluster id** (no
  names) to prove the pipeline end-to-end. **Separate faces opt-in + privacy copy** lands here.
- **Docs:** `media_index/DETAILS.md` faces pipeline; architecture map; `docs/security.md` on on-device face data +
  consent; i18n strings for the opt-in.
- **Tests:** _TDD red→green:_ cluster **merge/split** correctness; clustering honors **durable must-link/cannot-link**
  (forward ref to the store M4b adds — stub the store in M4a). _After:_ fake-backend faces pipeline; macOS-gated
  detect+embed on a fixture with known faces. _E2E:_ faces detected, cluster-id search returns the right photos.
- **Checks:** full incl. `--include-slow`.

### M4b — Naming + durable identity store + conservative re-attach + People UI (the data-safety core)

- **Durable identity store (Decision 4), hardened:** names + corrections (incl. **negative/cannot-link**) +
  **model-versioned centroids**, modeled on `favorites/` (atomic JSON, versioned). Re-attach is conservative: links
  intact when `media.db` survives; model-version-gated cosine otherwise; **negatives veto**; mismatch ⇒ "needs
  re-confirm", never cross-space match; high threshold + confirm for low confidence.
- People UI: largest unnamed clusters first, best-quality crop avatar, name/merge/split/"not this person"/propagate;
  search by person name → paths.
- **Docs:** `media_index/DETAILS.md` durable-store + re-attach rationale; frontend `people/CLAUDE.md`+`DETAILS.md`;
  update `docs/security.md`; i18n strings.
- **Tests (data-safety critical — re-run yourself, don't trust delegation, per `verify-delegated-work`):**
  - _TDD red→green:_ **names+corrections re-attach after a simulated `media.db` wipe** (must re-bind); **refuse to
    cross-space match** on model-version mismatch (assert no mislabel); **a face with a durable "not this person: X"
    veto must NOT re-attach to X after a regenerate, even when X's centroid is cosine-nearest** (the C-NEW-1 hole); the
    centroid-match threshold and the "needs re-confirm" path.
  - _After:_ fake-backend identity-store round trips.
  - _E2E:_ name a cluster, search the name, find photos; rename/merge; remove a face then regenerate and assert it does
    NOT snap back; simulate a model-version bump and assert the UI asks to re-confirm rather than silently relabeling.
- **Checks:** full incl. `--include-slow`; a11y on the People UI (AA+ contrast, screen reader).

### M5 — LLM captions (premium, opt-in; on-device default, cloud optional) — clearly later, genuinely optional

- On-device captions via **Foundation Models** (verify multimodal per Decision 1b). **Swift bridge = a Swift-toolchain
  build subproject** (Foundation Models is Swift-only; no Rust bindings) — a linked Swift static lib/framework or a
  sidecar, called over FFI. Spike the bridge early (can run in parallel as research); keep it isolated.
- Captions feed the FTS index. **Optional cloud route** (frontier VLM) via the existing `ai/` BYOK client, behind a
  _distinct_ explicit egress consent — never default.
- **Docs:** the Swift-bridge build wiring (own `docs/guides/` doc); consent + privacy copy; `ai/DETAILS.md`.
- **Tests:** _TDD red→green:_ provider-selection + consent gate (on-device vs cloud vs off); **verify the cloud gate
  blocks egress when off** (security-critical, re-run yourself). _After:_ Swift-bridge smoke (gated). _E2E:_ enable
  captions, search a described scene.
- **Checks:** full suite.

## Cross-cutting

- **Resources + memory ceiling.** Enrichment runs on dedicated low-priority OS threads (not rayon), bounded concurrency,
  cancel token, starts only after the base index signals ready, yields to foreground. The existing watchdog already
  measures **process-wide** resident memory but only stops _indexing_ — so **hook `media_index` cancellation into that
  same watchdog's stop action**, rather than standing up a second independent 16 GB ceiling (two ceilings over one
  shared resident pool each see headroom and can sum to ~2×). Decode of full-res HEIC/RAW + Core ML can spike RAM, so
  this must be **wired**, not asserted.
- **Query-time vector residency.** Brute-force cosine is cheap, but loading ~200 MB of embedding BLOBs from `media.db`
  per text query is not — and it's real work that must run **off the synchronous IPC thread** (alongside the text
  encode, not just it). Mirror `search/`'s warm in-memory arena (`SEARCH_INDEX`): keep a **resident vector cache**
  (load-once, invalidated on writes), counted against the same watchdog budget. Embedding storage on disk is small (512
  floats ≈ 2 KB/image; int8 if huge).
- **Cancellation + crash-safety.** Every pass is resumable from path-keyed `media_status`; a crash resumes. `media.db`
  is disposable; only the durable identity store must survive (separate atomic, versioned).
- **Deletion/GC.** `media_index` reconciles against index deletions (file vanished ⇒ media rows, face crops, embeddings
  GC'd). Resume ≠ cleanup; both are required.
- **Privacy.** On-device by default; faces a separate opt-in; cloud captions a separate egress opt-in. Mirror
  `onboarding/`'s consent pattern; document in `docs/security.md`. **Sensitive-document awareness:** real user folders
  mix ID scans (passport, driver's license, medical) in with photos — the index will OCR/tag/face-detect them. On-device
  keeps that local (fine), but it sharpens the M5 cloud-caption egress consent (don't silently upload an ID scan) and is
  a `docs/security.md` must-note. (Spike side finding, 2026-06-30.)
- **i18n.** Every user-facing string via the catalog with a `@key` description (`cmdr/no-raw-user-facing-string`).
- **Dependencies.** `objc2-vision`, `objc2-core-ml`, maybe a clustering crate / `ort` fallback / `sqlite-vec` binding:
  each needs `cargo deny check` + a verified ≥14-day-old version (`use-latest-dep-versions`, project `dependencies`
  rule).
- **No string-matching for classification** (`no-string-matching`): typed enums for model/provider/consent/identity-
  state across IPC; the frontend never branches on message substrings.

## Parallelization (only where extremely safe; sequential is default)

- **M1 must land first** — media DB, path-keyed identity, lifecycle bus, backend trait, GC, the read-API boundary.
- After M1: **M2 (Vision tags/feature-print)** and the **M5 Swift-bridge spike** are independent and can parallelize.
- **M3 (CLIP)** and **M4a (faces pipeline)** both depend on M2's vector store and the M3 model-install path (so M4a
  follows M3's install code even if the CLIP search work parallelizes); independent of each other in their `media.db`
  tables and UI surfaces (low conflict). **M4b follows M4a.** Prefer sequential unless we want speed; worktree per
  effort if parallelized.

## Definition of done

- Image indexing is opt-in, on-device by default, producing OCR-text search, tag search, image-similarity, natural-
  language text→image search, and named-face search — all via `query-ui`.
- **No human work is silently lost or mis-attributed across an index wipe or a model change** (proven by M4b tests,
  including the model-version-mismatch refuse-to-mislabel case AND the negative-veto "doesn't snap back" case). No image
  leaves the device unless the user opts into cloud captions.
- The only downloads are two small Core ML models, fetched on demand and **checksum-verified**. No Postgres. Binary
  lean.
- Enrichment is throttled, cancelable, crash-resumable, GC'd against deletions, and under an explicit memory ceiling;
  SMB/MTP are conservative opt-ins.
- Full `pnpm check --include-slow` green; new subsystem has `CLAUDE.md`+`DETAILS.md`; architecture map + lifecycle bus
  documented; privacy posture in `docs/security.md`.

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
- **`sqlite-vec` adoption cost** (load_extension feature + notarization/signing) if brute-force is outgrown.
- **HEIC/RAW decode** hostile cases via ImageIO (broken files, huge dims) — principle 3.
- **Foundation Models Swift bridge** (M5) is the least-proven integration; isolated, optional, spike early.
- **Vector-cache invalidation granularity** (M3): the warm resident cache is invalidated on writes, but enrichment
  writes embeddings continuously during a pass — naive whole-cache invalidation would thrash-reload ~200 MB per query
  mid-pass. Specify incremental/append cache update, or accept eventual consistency until the pass completes (perf, not
  correctness).
- **Late-registering volumes on the lifecycle bus** (SMB/MTP milestone, not M1): the per-volume `watch` needs a way to
  subscribe to a volume that registers _after_ startup (a share mounted later). Moot for M1 (local `root` only); a
  latent design point for when SMB/MTP enrichment lands.
