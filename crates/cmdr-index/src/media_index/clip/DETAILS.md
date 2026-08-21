# CLIP semantic search — details

The depth behind `CLAUDE.md` (plan M3). Read this before any non-trivial work here: editing, planning, reorganizing, or
advising. The enrichment + query plumbing rides the existing subsystem unchanged; only this directory is CLIP-specific.

## The model (evidence-anchored)

- **OpenAI CLIP ViT-B/32**, HF `openai/clip-vit-base-patch32`, **MIT-licensed** weights (a commercial product can ship
  them; Apple's MobileCLIP is research-only and can't — `docs/notes/clip-coreml-rust-spike.md`). Embedding dim 512,
  image 224×224, text context 77.
- **Two Core ML `.mlpackage` towers** pinned in `install.rs` (`CLIP_TOWERS`): the **image tower is 8-bit k-means
  palettized** (M5b, 2026-07-23), the **text tower stays fp**. Image ~83 MB (cosine min 0.9988 / mean 0.9995 vs torch
  fp32 over a 50-image fixture), text ~184 MB (cosine 1.0000), combined ~267 MB (down from ~392 MB non-palettized). The
  text tower stays fp because its 8-bit Core ML inference is all-NaN; 6-bit on the image tower falls below the 0.99 gate
  (min 0.957), so 8-bit is the floor.
- **Conversion is an out-of-tree dev script** (`apps/desktop/scripts/convert-clip-model/`), NEVER run by CI/pnpm: a
  throwaway `uv` venv (Python 3.11–3.12; coremltools/torch have no cp314 wheels), pinned `requirements.txt`. It bakes
  CLIP's per-channel `(x-mean)/std` normalization INTO the image model and prints each zip's SHA-256 + size + the
  David-upload handoff. `reference-tokenization.json` + `reference-vectors.json` are checked in (they back the Rust
  tests).
- **Both towers feed an `MLMultiArray`** (the exact path the spike proved), NOT a Core ML `ImageType` — a deliberate
  deviation that drops the CVPixelBuffer/MLImageConstraint FFI surface. So the image tower takes a float `[1,3,224,224]`
  CHW `[0,1]` tensor; the Rust side resizes + center-crops the decoded CGImage to 224 and divides by 255
  (`clip_pixels_from_cgimage` in `../backend/vision`), and the model bakes the normalization.
- **Models are hosted on the public Hugging Face repo `veszelovszki/cmdr-clip-vit-b32-coreml`** (uploads only with
  David's explicit approval; `hf` CLI + `secret HF_TOKEN`): the pinned `url` in `install.rs` must serve the exact pinned
  bytes, else the checksum-verified download fails and the feature stays honestly gated off. The `resolve` URLs redirect
  to a CDN (reqwest follows) and support Range resume (verified 2026-07-16).

## Two vector spaces, two-part staleness

CLIP's space is DIFFERENT from the Vision feature print, so its embeddings live in a SEPARATE `media_clip_embedding`
table (schema v3 added it + the `media_status.clip_stamp` column — `../store/DETAILS.md`). Cosine-comparing across the
two would silently rank across incompatible spaces.

Staleness is two-part (`store::needs_enrichment` for Vision by `engine_version`; `store::needs_clip` for CLIP by
`clip_stamp`), decoupled on purpose (plan M3 Q5): installing/upgrading the CLIP model re-embeds CLIP for every image
WITHOUT re-running OCR/tags for everyone, and a Vision engine bump re-runs OCR/tags WITHOUT re-embedding CLIP.
`clip_stamp` is `clip;model={id};os={major.minor.patch}` (the OS component re-embeds after an upgrade, which recompiles
`.mlmodelc` and can drift ANE output); `None` when no model is installed ⇒ CLIP is never attempted.

## One decode, two writer paths

The enrich core (`enrich_and_gc_scoped`, and the network core) computes `want_vision || want_clip` per image and calls
`backend.analyze_media(input, want_vision, want_clip)` — ONE decode runs the requested side(s). The macOS backend
decodes via ImageIO, runs the Vision requests when `want_vision`, and (when `want_clip`) resizes/center-crops the same
decode to 224 and hands the pixel buffer to the CLIP worker thread. Persistence is two INDEPENDENT writer messages
(`apply_media_upsert`): `upsert` writes the Vision row (identity + `engine_version` + OCR/tags/feature-print);
`upsert_clip` stamps `clip_stamp` + replaces `media_clip_embedding`, touching NO Vision column. A CLIP encode that can't
run yet (model still loading) yields `clip: None`, so the pass leaves `clip_stamp` unstamped and retries next pass — it
never fails the whole analysis on a transient CLIP miss. The `clip_stamp` reaches the passes via
`EnrichGates.clip_stamp` / `NetworkEnrichCtx.clip_stamp`, read once per pass from `clip::current_stamp(data_dir)`. The
whole-store `enrich_and_gc` wrapper is Vision-only (CLIP-agnostic, `clip_stamp: None`) and test-only now; production
reaches the scoped core directly with the installed stamp.

## The semantic-search on/off gate + delete-model

Semantic search is a real user toggle (`gate::semantic_search_enabled`, an atomic ON by default, seeded from the
FE-owned `mediaIndex.semanticSearch.enabled` at startup and live-applied by `media_index_set_semantic_search_enabled` in
`apps/desktop/src-tauri/src/commands/media_index/policy.rs`). Downloading the model is no longer the de-facto opt-in —
the toggle is.

**One atomic, both sides.** The gate is enforced at exactly two seams so read and write can't disagree:

- **Read:** `media_index_search_semantic` short-circuits to `[]` when the atomic is off (beside the master-toggle and
  empty-query checks).
- **CLIP write:** `clip::current_stamp` returns `None` when the atomic is off. Because `needs_clip(_, None)` is always
  false, EVERY pass type (full `run_pass_blocking`, network `run_network_pass_blocking`, live tick) computes
  `want_clip = false` and embeds no CLIP — without touching the per-pass `want_clip` line. Turning off mid-pass stops
  new CLIP work at the next image (the pass reads the stamp once per pass, so an in-flight pass finishes its current
  image's already-decided CLIP work; a `should_stop`-style per-image re-read isn't needed because turning off is
  non-destructive).

Enabling the toggle while a model is installed makes every image CLIP-stale again, so the command kicks the ready passes
(guarded on `is_installed`, so with no model it's a no-op — nothing to embed). Disabling kicks nothing and deletes
nothing: existing embeddings stay searchable until re-enabled or the model is deleted. Turning off ≠ erase.

**Delete model (`media_index_delete_clip_model`).** The explicit reclaim: `MediaScheduler::delete_clip_model` removes
the shared on-disk `clip-model` dir (both towers), then, for EVERY volume with a `media-{id}.db` (mounted or not —
`media_volume_ids` reads the data dir, so an unmounted NAS's embeddings are reclaimed too), prunes its
`media_clip_embedding` rows via the writer's `prune_all_clip`, `VACUUM`s, and drops the resident CLIP vector cache.
`prune_all_clip` deletes every embedding AND resets every `media_status.clip_stamp` to `''` in one transaction —
resetting the stamp is what makes a later re-download re-embed (the row goes CLIP-stale again against the reinstalled
stamp). Vision data (status/OCR/tags/feature print) is untouched, and CLIP embeddings aren't part of the `accounted`
aggregate (that counts `media_status` rows), so no aggregate delta. After the delete, `media_index_clip_model_status`
reads `installed: false`, so the UI returns to the download affordance.

## The Core ML towers + worker thread (`macos.rs`)

Mirrors the Vision backend's threading discipline: `MLModel` is `!Send` and a synchronous ANE predict is an XPC
round-trip that can overrun a small stack, so ONE dedicated 8 MB-stack `clip-worker` thread owns both loaded towers and
SERIALIZES every predict (Apple's pooled-inference recommendation). `encode_text` (query-time) and `encode_image` (from
the Vision worker) both send a job to it and block for the reply, so no `!Send` object crosses a boundary — only the
input ids / pixel `Vec` in and the embedding `Vec<f32>` out. `.mlpackage` is compiled to `.mlmodelc` on-device at first
load (`compileModelAtURL:error:`) and the compiled bundle is cached beside the model so later launches skip the 1–2 s
compile; after a verified compile the `.mlpackage` source is reclaimed (§ Model install). Every `unsafe` block carries a
per-site `// SAFETY:` (the objc2-core-ml `MLMultiArray` fills/reads via `dataPointer`, the `MLDictionaryFeatureProvider`
build, the CoreGraphics `CGBitmapContextCreate` render). The tokenizer (`tokenizer.rs`, `instant-clip-tokenizer`)
produces the fixed `[1,77]` int32 sequence (`[BOS] content [EOS]`, EOS-padded), pinned bit-exact to the HuggingFace
reference.

## What holding the towers costs

**Both towers loaded and predicted-through cost 307-412 MB of `MALLOC_LARGE` plus ~120-176 MB of `MALLOC_SMALL`, and the
process never gets it back** (measured on an M1 Max, macOS 26.5, debug build, `MLComputeUnits::All`, 2026-08-21, by
`clip::macos::residency_test`). `WORKER` is a `OnceLock`, so the first encode of the session loads both towers and they
stay for the process lifetime whether or not anything encodes again, whether or not the user turns semantic search off
afterwards. This is the steady-state idle cost named in
`../../../../../docs/notes/idle-malloc-large-clip-towers-2026-08-21.md`.

⚠️ **It is invisible to `query_mimalloc_heap`.** Core ML allocates through the SYSTEM allocator, and mimalloc is not a
registered macOS zone, so a Rust-side heap reading reports none of this.

The regions are the model's weight matrices, one malloc each, and they add up to the byte (310,444,032 on the reference
run):

- `101,187,584` x 1-2: the text tower's `49,408 x 512` fp32 token embedding. The sharpest fingerprint in the process,
  because nothing else is this size. The copy count varies run to run, and that variance IS the 307-412 MB spread.
- `4,194,304` x ~24: text-tower MLP matrices, `512 x 2048` fp32, two per block.
- `3,145,728` x ~14: text-tower fused QKV projections, `512 x 1536` fp32, one per block.
- `2,359,296` x ~25: image-tower MLP matrices at the shipped 8-bit palettization, `768 x 3072 x 1 byte`.

**The split between the towers is measured, not inferred** (`CMDR_CLIP_TOWER=image|text` loads one alone): the image
tower is 64.6 MB of `MALLOC_LARGE` plus 65.5 MB of `MALLOC_SMALL`, the **text tower 251.5 MB plus 84.8 MB**. So the
tower whose only job is encoding a typed query is about 80% of the bill, and the one enrichment runs in a loop is the
cheap half, because it ships 8-bit palettized and the text tower ships fp32.

**The compute-unit assignment decides the whole bill**, which is the lead any fix starts from (`CMDR_CLIP_COMPUTE_UNITS`
in the residency test switches it):

- `All` (shipped) and `CPUAndGPU`: ~410 MB. The GPU path materializes every weight matrix as its own buffer.
- `CPUOnly` and `CPUAndNeuralEngine`: **11.8 MB, two regions.** Core ML leaves the weights in the mmap'd `weight.bin`
  and allocates two scratch buffers (9,437,184 and 2,359,296 bytes).

⚠️ Those numbers are load-and-predict-once residency, not a claim about throughput. Dropping the GPU would trade ~400 MB
of permanent residency against enrichment speed, and that trade has NOT been measured. The `CLAUDE.md` guardrail against
changing `load_model_at`'s `MLComputeUnits::All` on the memory number alone rests on exactly this gap.

The second lead is scope rather than precision, and it is the bigger one: `load_towers` loads BOTH towers whichever one
is wanted, so an enrichment pass pays 251.5 MB for a text tower it will never call, and a single typed search query pays
64.6 MB for an image tower it will never call. Each is independently loadable today.

## Model install (`install.rs`, plan Decision 9)

New code reusing only `ai::download::download_file` (the resumable HTTP GET). Distinct from the GGUF two-flag gate: Core
ML models are `.mlpackage` DIRECTORY bundles (zipped), so this adds a zip extractor (with a zip-slip guard) and — unlike
`ai/`'s size-only check — a **SHA-256 verify BEFORE unpacking**. A truncated/tampered download never reaches the
extractor, so a half-model can never load and mis-embed (data safety, `verify_checksum` red→green tests).
`installed_stamp` builds the `clip_stamp`.

**The M5a package reclaim (plan M5a).** The model dir was 1.1 GB because it kept BOTH the ~550 MB combined downloaded
`.mlpackage` sources and the compiled `.mlmodelc`. Now, on the `clip-worker` thread's first load, once both towers load
AND a zero-input encode is sane (512-d, all-finite — `verify_sane`, guarding against a NaN-emitting model), each
`.mlpackage` source is deleted (`reclaim_source_package`), keeping only the compiled model (~350 MB dir, faster first
load). **Tradeoff:** the `.mlmodelc` is OS-version-specific, so an OS upgrade can invalidate it, and with the source
gone we can't recompile locally. `load_tower` handles this: it prefers the cached `.mlmodelc`; if it won't load it drops
the stale compiled and, if a `.mlpackage` is still present, recompiles from it; if NEITHER a loadable compiled model nor
a source remains, it returns `NotAvailable` having deleted the stale compiled — so `is_installed` (now **`.mlpackage` OR
`.mlmodelc` present per tower**) flips to `false` and the standard `media_index_download_clip_model` flow refetches the
pinned zip (same sha contract). A rare ~200 MB re-download vs ~550 MB saved on every launch, and never a crash or a
silently-dead feature. The filesystem decisions (`is_installed`, `reclaim_source_package`, `drop_compiled`) are unit
tested; the FFI compile/load around them isn't (needs Core ML + the real model).

## The query path

`media_index_search_semantic(volume_id, query, limit)` (IPC, registered in the `ipc.rs` manifest) runs OFF the IPC
thread (`spawn_blocking`): tokenize + warm-text-tower encode (`clip::encode_text_query`, which hops to the CLIP worker)
→ `MediaIndex::search_semantic(query_vec, limit)` brute-force top-k over a SECOND resident CLIP cache
(`vector::cache::get_or_load_clip`, keyed `(db_path, EmbeddingTable::Clip)`, invalidated per completed pass, dropped by
the memory watchdog with the feature-print cache). The read API takes the already-encoded query VECTOR, so it's a pure
vector query testable with deterministic vectors; the command owns the encode. `[]` (never an error) when indexing is
off, no model is installed, or the volume has no CLIP embeddings — so the UI voices coverage. Answers offline from
`media.db`.

**Latency:** the text tower is kept warm (a cold Core ML load is 1–2 s; a warm encode ~2 ms — spike numbers); the vector
top-k is brute force below ~50k stored vectors and the per-volume ANN index at or above it (`../ann/DETAILS.md` — the
engine decision went to `usearch` by a measured spike; `sqlite-vec` was disqualified as not actually ANN).

**The residency harness** behind § "What holding the towers costs" is `clip::macos::residency_test`, `#[ignore]`d and
env-gated because it needs the real ~267 MB model on disk:

```sh
CMDR_CLIP_MODEL_DIR=~/Library/Application\ Support/com.veszelovszki.cmdr/clip-model \
  cargo nextest run -p cmdr-index --run-ignored only clip::macos::residency_test --no-capture
```

`CMDR_CLIP_COMPUTE_UNITS=cpu|cpu-gpu|cpu-ane` re-runs the same measurement under the other assignments, and
`CMDR_CLIP_TOWER=image|text` loads one tower alone to attribute the bill between them. Only the shipped configuration
(both towers, `All`) carries assertions; every other combination reports its numbers and returns.

## Frontend

- **`search_semantic` is the PRIMARY text→image signal** in the Search dialog's image grid
  (`src/lib/search/ImageSearchResults.svelte`): each keystroke runs semantic + OCR in parallel, semantic hits lead
  (snippet-less tiles with a "matched description" reason via `search.imageResults.matchedDescription`), then OCR
  keyword hits not already shown (dedup by path). With no model, semantic returns `[]` and the grid degrades to
  OCR-only. The three test mocks (`ImageSearchResults.{gating,a11y}.test.ts`, `SearchDialog.svelte.test.ts`) stub
  `mediaIndexSearchSemantic → []`.
- **Settings download** (`MediaIndexClipModel.svelte` in the Image search card): self-gates on Apple Silicon
  (`is_local_ai_supported`), shows install state + a "Download model (~X MB)" button (honest size from
  `media_index_clip_model_status`), and downloads/installs via `media_index_download_clip_model`, which kicks a pass so
  already-enriched images gain CLIP embeddings (like a threshold decrease). An unconfigured status (`!configured`, no
  published artifact) renders "Coming soon" instead of the download button.

## Testing

The tokenizer is pinned bit-exact to `reference-tokenization.json`, and the encode path against `reference-vectors.json`
(both checked in from the conversion script). `verify_checksum` and the install/reclaim filesystem decisions are real
red→green unit tests. The delete-model path is pinned by
`writer::tests::prune_all_clip_drops_embeddings_resets_stamps_and_keeps_vision` and
`enrich_tests::delete_clip_model_removes_the_model_and_every_volumes_embeddings`. The Core ML FFI itself isn't unit
tested (it needs the real model on-device).
