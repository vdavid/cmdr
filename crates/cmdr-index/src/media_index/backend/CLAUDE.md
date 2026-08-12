# Vision backend seam

The inference boundary the scheduler, store, and GC sit behind, so all of that is testable with no GPU/ANE/FFI. `mod.rs`
defines the `VisionBackend` trait, `fake.rs` the deterministic impl, `vision/` the real macOS one (OCR + tags + feature
print), `vision/spike.rs` the throughput measurement harness.

## Must-knows

- **A backend is single-threaded BY CONSTRUCTION.** Each one owns a dedicated OS thread with an **8 MB stack** and
  dispatches every image to it over a channel: Vision/ImageIO do synchronous XPC round-trips into system daemons that
  overrun a small worker stack, and every `Retained`/`CFRetained` object stays confined to that thread (nothing `!Send`
  crosses a boundary — only the path `String` + bytes in, the analysis out). ❌ Never call one backend concurrently;
  parallelism is N whole backends (`../scheduler/CLAUDE.md`). Each job runs inside an `objc2::rc::autoreleasepool`, so
  framework temporaries free per image, not per pass.
- **One decode, three outputs.** `analyze` decodes the downscaled thumbnail ONCE and runs all three Vision requests on a
  single `VNImageRequestHandler`. ❌ Don't add a request that re-decodes; the decode dominates cost. The downscale cap
  (`kCGImageSourceThumbnailMaxPixelSize` = 3072 long edge) is the memory guard: it holds the decoded bitmap near ~36 MB
  instead of ~190 MB for a 48-megapixel original.
- **Hostile input fails CLOSED to a typed `VisionError`, never a panic or a hang.** Unreadable/empty/non-image/
  undecodable → `Decode`; a request failure → `Ocr`; a vanished file → `Missing`, classified by io kind, ❌ never by
  message text.
- **Every `unsafe` block carries a per-site `// SAFETY:`** naming the concrete invariant (pointer/buffer validity,
  Create-vs-Get ownership on each CF `Create`, the extern-static key reads, the success gate). ❌ Never a blanket
  file-level allow.
- **Provenance is ONE folded stamp.** `analysis_stamp` combines the OCR, classify, and feature-print revisions plus the
  OS version; `needs_enrichment` keys on it, so any component bump re-runs the whole analysis. Adding an output means
  folding its revision in, or an OS change silently leaves stale derived data.
- **Every test injects `FakeVisionBackend`** (deterministic, zero-FFI), which is ALSO the production fallback off-macOS.
  New trait methods need a fake impl, or the whole suite loses its seam.

The Vision request details, the analyze outputs, the taxonomy evidence, and the FFI notes: `DETAILS.md`. Read it before
any non-trivial work here: editing, planning, reorganizing, or advising.
