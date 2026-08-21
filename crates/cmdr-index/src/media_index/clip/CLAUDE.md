# CLIP semantic search

Natural-language text→image search: CLIP maps images and text into ONE shared 512-d space, so a typed query is encoded
and cosine-matched against stored image embeddings. `mod.rs` is the stamp + query seam, `backend.rs` the encode entry
points, `macos.rs` the Core ML towers + worker thread, `install.rs` the pinned download/verify/unpack, `tokenizer.rs`
the fixed `[1,77]` tokenization.

## Must-knows

- **`clip::current_stamp` is the SINGLE CLIP-write seam.** It returns `None` when the semantic-search toggle is off, and
  `needs_clip(_, None)` is always false, so EVERY pass type computes `want_clip = false` without a per-pass check. ❌
  Don't re-gate `want_clip` in a pass; you'd create a second seam that can disagree with the read side.
- **Staleness is two-part and decoupled on purpose.** `needs_enrichment` (Vision, by `engine_version`) and `needs_clip`
  (by `clip_stamp`) move independently, so installing a model re-embeds CLIP without re-running OCR/tags, and a Vision
  bump doesn't re-embed CLIP. `clip_stamp` is `clip;model={id};os={major.minor.patch}` — the OS component matters
  because an upgrade recompiles `.mlmodelc` and can drift ANE output.
- **A CLIP encode that can't run yet yields `clip: None`**, so the pass leaves `clip_stamp` unstamped and retries next
  pass. ❌ Never fail the whole analysis on a transient CLIP miss.
- **ONE dedicated 8 MB-stack `clip-worker` thread owns both towers and serializes every predict.** `MLModel` is `!Send`
  and a synchronous ANE predict is an XPC round-trip that can overrun a small stack; only input ids / pixel `Vec`s go in
  and `Vec<f32>` embeddings come out. ❌ Never touch an `MLModel` off that thread.
- **Verify the SHA-256 BEFORE unpacking** (`install.rs`), not after: a truncated or tampered download must never reach
  the extractor, or a half-model loads and mis-embeds silently. The extractor also carries a zip-slip guard.
- **The pinned `url` must serve the exact pinned bytes.** Uploads to the Hugging Face repo happen only with David's
  explicit approval; if the bytes drift, the checksum fails and the feature stays honestly gated off.
- **`is_installed` means `.mlpackage` OR `.mlmodelc` present per tower** — the source package is reclaimed after a
  verified compile, so a package-only check would report the feature missing on every later launch.
- **Turning the toggle off ≠ erase.** Existing embeddings stay searchable; only `media_index_delete_clip_model` removes
  them.
- **The first encode of a session buys ~435 MB for the process's whole life**, and 80% of it is the TEXT tower, which an
  enrichment pass never calls. `WORKER` is a `OnceLock`, both towers load together, and Core ML allocates their weights
  through the SYSTEM allocator, so `query_mimalloc_heap` shows none of it. Per-tower numbers plus the compute-unit lever
  that moves it 35×: `DETAILS.md` § "What holding the towers costs". ❌ Don't change `load_model_at`'s
  `MLComputeUnits::All` on the memory number alone; the speed side is unmeasured.

The model evidence, the towers, install + reclaim, the gate, and the query path: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
