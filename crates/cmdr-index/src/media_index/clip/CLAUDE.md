# CLIP semantic search

Natural-language text→image search: CLIP maps images and text into ONE shared 512-d space, so a typed query is encoded
and cosine-matched against stored image embeddings. `mod.rs` is the stamp + query seam, `backend.rs` the encode entry
points, `towers.rs` the lazy per-tower load + source reclaim, `macos.rs` the Core ML side of that plus the worker
thread, `install.rs` the pinned download/verify/unpack, `tokenizer.rs` the fixed `[1,77]` tokenization.

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
- **ONE dedicated 8 MB-stack `clip-worker` thread owns every loaded tower and serializes every predict.** `MLModel` is
  `!Send` and a synchronous ANE predict is an XPC round-trip that can overrun a small stack; only input ids / pixel
  `Vec`s go in and `Vec<f32>` embeddings come out. ❌ Never touch an `MLModel` off that thread.
- **Verify the SHA-256 BEFORE unpacking** (`install.rs`), not after: a truncated or tampered download must never reach
  the extractor, or a half-model loads and mis-embeds silently. The extractor also carries a zip-slip guard.
- **The pinned `url` must serve the exact pinned bytes.** Uploads to the Hugging Face repo happen only with David's
  explicit approval; if the bytes drift, the checksum fails and the feature stays honestly gated off.
- **`is_installed` means `.mlpackage` OR `.mlmodelc` present per tower** — the source package is reclaimed after a
  verified compile, so a package-only check would report the feature missing on every later launch.
- **Turning the toggle off ≠ erase.** Existing embeddings stay searchable; only `media_index_delete_clip_model` removes
  them.
- **Each tower loads on the first job that needs it, then stays for the process's whole life.** An enrichment pass holds
  the image tower alone (59.0 MB of `MALLOC_LARGE`); a typed query adds the fp32 text tower's 245.9 MB. `WORKER` is
  still a `OnceLock` and nothing drops a loaded tower, so the ceiling is unchanged; what moved is that a session pays
  only for the halves it uses. Core ML allocates through the SYSTEM allocator, so `query_mimalloc_heap` shows none of
  it. Numbers plus the compute-unit lever that moves them 35×: `DETAILS.md` § "What holding the towers costs". ❌ Don't
  change `load_model_at`'s `MLComputeUnits::All` on the memory number alone; the speed side is unmeasured.
- **A tower's `.mlpackage` source is reclaimed on THAT tower's own word.** ❌ Never gate the delete on anything but the
  tower losing its source having loaded AND encoded a sane embedding. A pair-wise precondition looks harmless and, with
  lazy loads, either never fires (trading RAM for ~550 MB of permanent disk) or deletes a source for a tower that never
  proved itself. Nothing measures either outcome.

The model evidence, the towers, install + reclaim, the gate, and the query path: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
