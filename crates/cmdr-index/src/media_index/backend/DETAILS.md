# Vision backend seam — details

The depth behind `CLAUDE.md`. Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## The seam

The trait is `VisionBackend`: `ocr` (OCR only, for the focused OCR tests), `analyze` (the enrichment entry point — OCR +
tags + feature print from ONE decode), and the provenance stamps `engine_version` / `taxonomy_version` /
`analysis_stamp`. Two impls:

- `fake::FakeVisionBackend` — deterministic, zero-FFI (scripted/derived OCR text, tags, and a stem-derived unit
  embedding). Every test injects it via `MediaScheduler::new`; it's also the production fallback off-macOS. It exposes
  `with_engine_version` / `with_taxonomy_version` to simulate a stamp bump, and `missing_for` to script a vanished file.
- `vision::VisionOcrBackend` (macOS only) — the real OCR + classify + feature print. `MediaScheduler::start` selects it
  on macOS.

Faces become sibling methods on this trait as later work lands, each returning its own typed result, each fakeable the
same way. CLIP is deliberately NOT a method here: it rides `analyze_media(input, want_vision, want_clip)` and hands the
same decode to the CLIP worker (`../clip/DETAILS.md` § One decode, two writer paths).

## The real OCR path (`vision/mod.rs`, macOS)

1. **Decode downscaled, in-memory (Decision 5 — no thumbnail files).** Read the compressed bytes, wrap in a `CFData`,
   open a `CGImageSource`, and `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceThumbnailMaxPixelSize` = 3072
   (long edge) + `…FromImageAlways` + `…WithTransform` (EXIF-upright). This caps the decoded bitmap (~36 MB worst case)
   instead of letting Vision decode a 48-megapixel original (~190 MB). The compressed read is bounded; the decoded
   bitmap is the memory hazard the cap defends.
2. **Recognize.** `VNImageRequestHandler(cgImage:)` + `VNRecognizeTextRequest` (`.accurate`, language correction on),
   `performRequests`, then the top candidate per `VNRecognizedTextObservation`, newline-joined.

**Threading + the 8 MB stack.** Vision/ImageIO do synchronous XPC round-trips into system daemons (ANE) that can overrun
a small worker stack — the same hazard as calling AppKit off rayon (`src-tauri/CLAUDE.md`). So each backend owns a
dedicated OS thread with an 8 MB stack; `ocr`/`analyze_media` dispatch each image to it over a channel and block for the
reply. The single thread SERIALIZES that backend's Vision calls (Apple's recommendation for pooled inference) and
confines every `Retained`/`CFRetained` object to it. Each job runs inside `objc2::rc::autoreleasepool`, so framework
temporaries free per image, not per pass.

## `analyze`: one decode, three outputs

The enrichment path calls `VisionBackend::analyze`, not `ocr`. The real backend decodes the thumbnail ONCE
(`decode_thumbnail`, the shared downscale) and performs THREE Vision requests on a single `VNImageRequestHandler`:
`VNRecognizeTextRequest` (OCR), `VNClassifyImageRequest` (scene/object tags), and `VNGenerateImageFeaturePrintRequest`
(the image↔image feature print). Reusing one decode + one handler is the Decision-5 "decode once" applied across all
three — decoding the original three times would dominate cost.

- **Tags** (`read_tags`): the top `MAX_TAGS` (12) classifications above `MIN_TAG_SCORE` (0.1), highest confidence first
  (Vision returns them sorted, so the read breaks at the floor). The taxonomy is FIXED by the OS — **1,303 identifiers
  on macOS 26.5.1** (verified 2026-07-13 via `VNClassifyImageRequest::supportedIdentifiersAndReturnError().len()`). A
  taxonomy change on an OS upgrade re-tags via the provenance stamp below.
- **Feature print** (`read_feature_print`): the first `VNFeaturePrintObservation`'s raw bytes decoded per `elementType`
  (`Float` → `f32`, `Double` → `f64`→`f32`), length-checked against `elementCount` (a mismatch drops it rather than
  storing garbage). Vision's feature print is image↔image only (no text encoder — that's CLIP's job, and a SEPARATE
  vector space).
- Every new `unsafe` block carries a per-site `// SAFETY:` (the request `new()`s, the observation accessors, the
  `NSData` byte read is the safe `to_vec`), same discipline as the OCR path.

## The analyze provenance stamp (plan Decision 4)

`analysis_stamp` folds the OCR engine revision, the tag-taxonomy (classify) revision, and the feature-print revision
into ONE stamp stored in the `media_status.engine_version` column and used by `needs_enrichment`. Because one decode
produces all three outputs, re-running the whole analysis when ANY component changes costs nothing extra, so a single
combined stamp is simpler than three per-output stamps and still satisfies "an OS taxonomy change re-tags" (the
taxonomy-version component bumps → the row goes stale → analyze re-runs → tags refresh).

**`engine_version`** is `vision-ocr;os={major}.{minor}.{patch};rev={N}`: the macOS version (`NSProcessInfo`) plus the
current `VNRecognizeTextRequest` revision (read off a fresh instance). The `analyze` path additionally computes
`taxonomy_version` (the `VNClassifyImageRequest` revision) and folds all three revisions into `analysis_stamp`.

## FFI discipline and hostile input

Every `unsafe` block carries a per-site `// SAFETY:` naming the concrete invariant — pointer/buffer validity for
`CFData`/`CFNumber`/`CFDictionary` creation, Create-vs-Get ownership (the `+1 CFRetained` on every CF `Create`), the
extern-static reads for the ImageIO/CF constant keys, and the success-gate `Option`/`Result` on each framework call —
never a blanket file allow (`clippy::undocumented_unsafe_blocks`).

Hostile input fails closed to a typed `VisionError`, never a panic/hang: an unreadable/empty/non-image/undecodable file
returns `Decode`; a request failure returns `Ocr`; a file that vanished between the index walk and the analyze returns
`Missing` (classified from the local `std::fs::read` ENOENT by io kind, never a message match). The pass logs a `Decode`
/ `Ocr` and marks the row `Failed`; a `Missing` is a quiet DEBUG skip that writes no row at all (`../DETAILS.md` §
Progress events + vanished-file skip).

## Testing

The fixture for the macOS-gated real tests lives at `test-fixtures/ocr-sample.png` (a tiny PNG rendering "CMDR OCR" /
"hello 2026", generated once via CoreGraphics text drawing). `vision/tests.rs` (macOS-only, so it can't run off-macOS):
real Vision OCR reads the known words off the fixture; `analyze` returns real OCR + well-formed tags + a stable-length
feature print, and a real feature print's self-cosine is ~1.0; hostile inputs (non-image, empty, missing) each return a
typed `VisionError` with no panic. `fake.rs` tests its own determinism (tags + feature prints).

`vision/spike.rs` is the M2 throughput harness: its decode-vs-full-analyze scaling numbers and what they mean for the
worker pool are in `../scheduler/DETAILS.md` § Parallel enrichment.

`what_one_vision_analyze_leaves_resident` (`#[ignore]`d, run by name) measures what Vision's own models cost to keep
loaded, the companion to `../clip/DETAILS.md` § "What holding the towers costs". **Vision is cheap: ~49 MB of total
dirty growth for a first analyze, of which only 2.1 MB is `MALLOC_LARGE`** (M1 Max, macOS 26.5, debug build,
2026-08-21). Recorded because it's a clean negative — Apple runs OCR, classification, and feature print largely out of
process, so ❌ don't reach for Vision when attributing a large in-process block.
