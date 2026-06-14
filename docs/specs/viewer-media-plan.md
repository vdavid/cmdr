# Viewer media rendering plan

Let the File viewer render images and PDFs inline, instead of showing the binary warning banner and lossy UTF-8 bytes.
Scope here is **the File viewer only**. Thumbnail pane mode and ML image search are separate future features that share
none of this code (they go through `QLThumbnailGenerator` and a Rust decode path, respectively); they are listed under
non-goals as motivation, not work items.

## Decision: lean on WKWebView, no Rust image decoder

We do **not** pull in Prvw's decoder or any raster/RAW crate for this. Tauri's webview is WKWebView, and on macOS that
engine decodes the formats we care about natively. Once camera RAW is out of scope (it needs a real decode pipeline and
is a power-user niche for a file manager), Prvw's decoder offers nothing a file-manager preview needs over what the
webview already does. The whole feature becomes "serve the file to an `<img>`/`<embed>` and get out of the way."

This stays true only while Cmdr is macOS-only. On Linux/Windows, webkit2gtk / WebView2 won't decode HEIC (and never
RAW), so cross-platform is the fork point where a Rust decoder (the `image` crate, or the Prvw crate) comes back. Called
out under non-goals.

## Naming (decided)

There is **no user-facing umbrella term**. The mode picker shows the concrete detected kind: **Image**, **PDF**, or
**Text**. This is honest (a PDF is a document, not "media", so any single umbrella word is half-wrong), it scales
(Markdown / HTML slot in later as more concrete kinds), and it needs no translation table. "Media" survives only as an
internal code word, invisible to users: the `cmdr-media://` scheme and the `ViewerContentKind` enum (`Image` / `Pdf` /
`Text`). Avoid "Preview" as a user-facing label: it collides with Quick Look (Shift+Space) and macOS Preview.app.

## What the rendering spike proved, and what it did NOT

Verified on macOS 26.5.1, WKWebView build 25F80, 2026-06-14, via a standalone Swift `WKWebView` harness loading local
files through `loadFileURL` and reporting `img.naturalWidth` / `takeSnapshot`.

- **JPEG** (control), **HEIC**, **SVG**: decode and render in a plain `<img>` (`naturalWidth` non-zero, visible in the
  snapshot). HEIC was the main open question: clean yes, no decoder.
- **PDF**: renders inline via `<embed type="application/pdf">` (text visible in the snapshot), WKWebView's PDFKit-backed
  viewer, including multi-page scroll.

So raster (JPEG/PNG/GIF/WebP/BMP/TIFF), HEIC, SVG, and PDF are all "free" at the **decode** layer. Animated GIF/WebP
animate in `<img>` for free too.

**The spike validated decoding, not delivery.** It used `loadFileURL` (a `file://` page), which does NOT exercise the
real path: a custom URI scheme handler serving bytes to an `<img>`/`<embed>` inside a Tauri-created `viewer-*` window
under Cmdr's CSP. Those are different risks. Delivery is de-risked by **step 1a below** (a thin vertical slice) before
we build the rest, not assumed done.

## Security model: per-open capability token (not path-matching)

This is the most important design point. The viewer renders the _content of an untrusted file_. A hostile file could try
to make the webview request `cmdr-media:///etc/ssh/id_rsa`. Defenses that "validate the requested path against the
session" are weak: the scheme handler can't reliably know which window is asking, and the window already knows its own
path anyway. So:

- **`viewer_open` mints a random unguessable token** (128-bit, CSPRNG) and stores
  `token → { canonical_path, kind, mime }` in a global map (std `Mutex`/`RwLock` via `*_ignore_poison()` per the `rust`
  rule).
- The result carries the token. The frontend builds the URL from the token with **no raw path in the URL at all**. The
  exact form (`cmdr-media://localhost/<token>` vs another authority/host shape) is whatever step 1a observes; don't
  hardcode it in the frontend before 1a confirms it.
- The scheme handler resolves token → path from the map. Unknown token → **404**. There is no way to name an arbitrary
  file: the backend only ever exposes files it chose to. (No separate expiry; a token is valid exactly while its session
  lives, so "unknown" covers both never-existed and already-dropped.)
- **Token lifetime = session lifetime.** Drop the entry at the **same `close_session` choke point** that already frees
  `SESSIONS`, so the two can't diverge. The viewer has two teardown paths (the `viewer_close` IPC and the
  `WindowEvent::Destroyed` branch via `WINDOW_TO_SESSION`); both funnel through `close_session`, so dropping the token
  there covers both. A closed-window viewer must not leave a live token mapping a real path.
- The handler `File::open`s a real path off the IPC path, so it inherits the viewer's existing FDA assumption: a viewer
  only opens after the user picked the file, so FDA is already decided by then. This is **not** a new pre-gate read path
  (`tauri-apis` / `fda_gate.rs`); state that so a stray TCC denial reading bytes isn't misdiagnosed as a scheme bug.

## Custom `cmdr-media://` async scheme

The viewer opens arbitrary user-chosen paths; widening Tauri's scope-gated asset protocol to the whole filesystem is a
security smell, and the asset protocol wouldn't carry our token model cleanly. So register a dedicated scheme the
backend fully controls. No such scheme exists in the codebase yet, so this is greenfield builder work.

- Register with `register_asynchronous_uri_scheme_protocol("cmdr-media", ...)` in the `tauri::Builder` chain (app init).
  This runs before any window exists, which is correct: `viewer-*` windows are created lazily at runtime and inherit the
  app-wide scheme. Step 1a confirms a runtime-created window actually loads it.
- **Don't reuse `blocking_with_timeout`** (that helper returns an `IpcError` for the _command_ path; a scheme handler
  must answer with an HTTP-shaped response). The handler does its own `tokio::task::spawn_blocking` +
  `tokio::time:: timeout`, mapping expiry to a **504**. Same network-mount hazard as commands (`docs/architecture.md` §
  Platform constraints), different return shape.
- **Content-Type comes from magic bytes, never the extension** (a `.jpg` that is actually a PDF/HTML polyglot must never
  be served as something an `<embed>`/`<iframe>` would execute). Images go to `<img>` (inert), PDF to `<embed>`.
- **Honor `Range` requests**: 206 with inclusive `Content-Range: bytes start-end/total`, `Accept-Ranges: bytes`, end
  clamped to size-1, 200 when no range. WKWebView issues byte-range requests for PDFs and large media; PDF correctness
  hinges on this. Re-stat the file safely (network-mount hazard) to compute size. Unit-test the range math matrix.
- This scheme is also the seam where a Rust decoder could slot in later (RAW → PNG on the fly) without the frontend
  changing. Worth noting, not building now.

### CSP, and why step 1a exists

The viewer inherits the global CSP in `apps/desktop/src-tauri/tauri.conf.json`:

`... img-src 'self' data:; ... frame-src 'none'; object-src 'none'; ...`

That blocks both our media source and the PDF `<embed>` today. We must add the `cmdr-media` scheme to `img-src` and to
`object-src` (which governs `<embed>`/`<object>`; `frame-src` instead if we end up using `<iframe>` for PDF). **The
exact CSP token form is verified by observation in step 1a, not assumed**: on macOS WKWebView a Tauri custom scheme
typically surfaces as `cmdr-media://localhost/...`, so the CSP source is the scheme token `cmdr-media:`, but Tauri
version / platform can rewrite custom schemes (for example `http://<scheme>.localhost` on Windows). Step 1a reads the
actual resource origin from a real load and pins the CSP token to match. Keep the allowance scoped to our scheme, never
`*`. CSP is global, so the scheme is permitted app-wide; that's acceptable because the handler enforces access via the
token (above). `withGlobalTauri` is `true` in dev (prod `false`), so confirm in both.

We considered the cheaper `data:` URL path for small images (already allowed by `img-src`): `viewer_open` could return a
`data:` URL and skip the scheme for the common case. We reject it for **elegance and uniformity**: PDFs and large images
need the scheme and its range support regardless, and one path (the token scheme for everything) beats two code paths
with a size cliff between them. Noted as a possible future optimization, not v1.

## Content classification

Decide what a file is at open time, by magic bytes. Add a pure function in the `file_viewer` module:

```
enum ViewerContentKind { Text, Image, Pdf }   // future: Markdown, Html, ...
fn classify_viewer_content(head: &[u8], ext: Option<&str>, is_local: bool) -> ViewerContentKind
```

- **Magic bytes decide** (and decide the served `Content-Type`): JPEG `FF D8 FF`, PNG `89 50 4E 47`, GIF `GIF8`, WebP
  `RIFF....WEBP`, BMP `BM`, TIFF `II 2A 00` / `MM 00 2A`, HEIC (ISO-BMFF `ftyp` with brand `heic`/`heix`/`mif1`/`msf1`
  at offset 4), PDF `%PDF-`. Extension is a tiebreaker only, never the Content-Type source. Hand-roll a small table with
  a unit-test matrix; it's a closed set, no dependency (the pure-Rust `infer` crate is the fallback if it grows).
- **SVG is conservative**: classify as `Image` only when the extension is `.svg` AND the content's first non-whitespace,
  post-BOM, post-prolog, post-comment, post-DOCTYPE token is an `<svg` root. Otherwise it stays `Text`. This avoids
  false-positiving an HTML file that merely contains an inline `<svg>`. Served as `image/svg+xml` to a sandboxed `<img>`
  (no script execution).
- **Local-only**: return `Image`/`Pdf` only for files on a local POSIX volume (`is_local`). MTP has no POSIX path to
  `File::open`, and SMB paths can block; non-local sources stay `Text` and flow through the existing pipeline. v1 scopes
  media rendering to local files on purpose.

`viewer_open` returns the kind in `ViewerOpenResult`. See the session-model section for how a media open differs from a
text open.

## Session model: how a media open differs

Today `open_session(path)` always eagerly builds a text backend (FullLoad/ByteSeek) and returns `initial_lines`,
`encoding`, `total_lines`, etc. We must not fight that:

- A **media open** still creates a session (so close/teardown and the token map stay uniform) but **does not build a
  text backend**; it classifies, mints the token, optionally reads image dimensions, and returns a `ViewerOpenResult`
  whose text fields are empty/None and whose `kind` is `Image`/`Pdf`. Use a **`Media` no-op backend** (not an `Option`)
  for the non-optional `backend: Arc<ArcSwap<Box<dyn FileViewerBackend>>>` field, so we don't have to touch every
  `load_backend()` caller to handle `None`. Dimension reading is **header-only and best-effort**: it must not extend the
  open past the header read (the open runs under the 2s `VIEWER_TIMEOUT`), so probe dimensions without full-decoding
  (and behind the same off-thread read if needed). The `image` crate is already a dep, so no new crate.
- **"View as text" override** does not try to upgrade the media session in place. It calls a dedicated
  `viewer_open_as_text(path)` that returns a fresh, full text session; the frontend swaps to it. Simpler than making the
  backend swap-in lazily, and it reuses the existing eager path verbatim.
- Every FE consumer of the now-maybe-empty text fields (`viewer-scroll.svelte.ts` effects on `totalLines`, the status
  bar, the encoding-picker fetch) must guard on `kind === 'text'`. This is a data-path change, not only a "hide
  controls" change.

## Frontend rendering

In `apps/desktop/src/routes/viewer/+page.svelte`, branch on `kind`:

- **Image**: render `<img src={mediaUrl} alt={fileName}>` (`mediaUrl = cmdr-media://localhost/<token>`), not the
  virtual-scroll line machinery. v1 interactions, simple but delightful:
  - Fit-to-window by default; click toggles 100% / fit.
  - Scroll / pinch to zoom, drag to pan (CSS `transform`, no library).
  - Checkerboard behind transparency (fixed in screen space).
  - **Loading + error state from v1, not deferred**: a spinner until `load`/`error` fires, and a friendly inline message
    on `error` (a multi-second decode showing an empty area violates the responsiveness principle). EXIF orientation:
    modern WebKit applies `image-orientation: from-image` by default, so phone photos display upright; verify with a
    rotated sample and evidence-anchor the claim in the `DETAILS.md`.
- **PDF**: render `<embed type="application/pdf" src={mediaUrl}>` filling the content area, plus the same load/error
  state. WKWebView supplies its own scroll/zoom/page UI.
- **Text** (default, unchanged): the current pipeline.

The inert `ViewModePicker.svelte` becomes real: it shows the detected kind and offers **"View as text"** (→
`viewer_open_as_text`). Text-only toolbar controls (search, encoding picker, tail mode, word wrap) are hidden in media
mode. Respect dark/light and `prefers-reduced-motion` for the spinner.

## Binary-warning narrowing (required, not optional)

The premise is "instead of the binary warning banner", so `binary-warning.ts` (`categorizeForViewerWarning`) and its
banner get **narrowed, not deleted**: drop the `image` set and **only `.pdf`** from the `document` set (those are now
rendered), but **keep the rest of `DOCUMENT_EXTS` warning** (`.docx`, `.xlsx`, `.pages`, `.epub`, etc., which the viewer
still can't render), and keep `OTHER_BINARY_EXTS` (archives, executables, video, audio, fonts: still unrendered, still
warned). `categorizeForViewerWarning` currently returns one `label: 'document'` for the whole document set, so the edit
must split PDF out of that set, not silently stop warning on every document type. Check the
`fileViewer.suppressBinaryWarning` setting tail (its restricted-window persistence allowlist in `commands/settings.rs` +
`capabilities/CLAUDE.md`) still makes sense once the warn set shrinks. This is a real edit with a settings/capability
tail, tracked as its own step.

## Window and menu

No new window. The macOS viewer menu is **app-level and shared across all viewer windows** (the recent menu-swap work,
`activate_window_menu("viewer")`), so "Word wrap" can't be per-window-state-driven: with an image viewer and a text
viewer open at once there's one shared `CheckMenuItem`. v1: Word wrap stays present but inert in media mode (no text
lines to wrap, so toggling does nothing visible). A later refinement can drive its enable/disable off the focused
viewer's kind via the existing focus-gain hook. No image-specific menu actions in v1.

## Touch points

- `apps/desktop/src-tauri/src/file_viewer/`: `classify_viewer_content` (pure + tests); the token map; media-aware
  `open_session`; `viewer_open_as_text`; `ViewerOpenResult` gains `kind` (+ optional dimensions, token).
- The `cmdr-media` scheme handler (new module under `file_viewer/` or `commands/`): token resolve, magic-byte
  Content-Type, range/206, own spawn_blocking+timeout → 504, 404 on unknown token. Keep the handler closure a **thin
  shell over pure functions** (classify, range math, token resolve, Content-Type) so only the un-testable Tauri glue
  needs the coverage allowlist, named specifically per the `testing` rule.
- `lib.rs` builder chain: `register_asynchronous_uri_scheme_protocol("cmdr-media", ...)`. (This is **Rust**, not a
  `tauri.conf.json` field; the config file only carries the CSP edit.)
- `tauri.conf.json`: extend CSP only (`img-src` + `object-src` for the step-1a-verified scheme token).
- `apps/desktop/src-tauri/capabilities/viewer.json` + `capabilities/CLAUDE.md`: confirm whether the frontend needs any
  capability to reference the scheme URL (custom schemes are usually CSP-governed, not capability-governed; confirm).
- `commands/file_viewer.rs`: `viewer_open` returns `kind`; add `viewer_open_as_text`. Thin pass-throughs only.
- `apps/desktop/src/routes/viewer/+page.svelte`, `ViewModePicker.svelte`, `viewer-scroll.svelte.ts`, the status bar,
  toolbar: render branch, guards on empty text fields, picker wiring, loading/error states.
- `apps/desktop/src/lib/file-viewer/binary-warning.ts`: narrow the warn set.
- `pnpm bindings:regen` after the IPC shape changes.
- Colocated docs: `file_viewer/DETAILS.md` + `CLAUDE.md`, `routes/viewer/CLAUDE.md`, capabilities doc.

## Phasing

- **Phase 0 (done)**: decode spike.
- **Phase 1a, delivery spike (de-risk first)**: register `cmdr-media://`, serve one hard-coded local file with a token,
  load it in a real `viewer-*` window via `<img>`, confirm it renders, read the actual resource origin, and pin the CSP
  token (img-src + object-src) in both dev and prod CSP. This resolves the two highest-churn unknowns (scheme delivery,
  CSP token) before building the rest. If the scheme can't deliver under CSP, we learn it here cheaply.
- **Phase 1b, backend**: `classify_viewer_content` + tests; the token map + scheme handler with range support and
  504/404 paths + tests; media-aware `open_session`; `viewer_open` returns `kind`; `viewer_open_as_text`; bindings
  regen.
- **Phase 2, frontend**: render branch (`<img>` + `<embed>`) with loading/error states, `ViewModePicker` wired with
  "View as text", guard text-only data paths and controls, image fit/zoom/pan + transparency checkerboard, optional
  status-bar dimensions; narrow `binary-warning.ts`.
- **Phase 3, polish**: keyboard (fit / 100% / zoom), a11y (alt text, focus), very-large-image guard, verify EXIF
  orientation, dark/light + reduced-motion pass.

## Tests

- **Rust unit**: `classify_viewer_content` matrix (every magic-byte case, SVG conservative cases, extension/magic
  disagreement, empty/short head, non-local → Text); scheme handler for magic-byte Content-Type, range math (206 +
  inclusive headers, clamp, 200-no-range), and **unknown/expired token → 404** (the capability model).
- **Vitest**: kind-based mode selection and the "View as text" override; control + data-path guards per kind.
- **Playwright E2E**: open a small fixture image and a small fixture PDF; assert the image `<img>` has
  `naturalWidth > 0`, assert the binary-warning banner is **absent**, and assert **no CSP-violation console error
  fired** (the most likely failure if the CSP token is wrong). For the PDF, asserting the `<embed>` is present is weak
  on its own, so pair it with the banner-absent + no-CSP-error checks. Ship tiny fixtures (a few-KB PNG and a 1-page
  PDF).

## Non-goals (and why)

- **Thumbnail pane mode**: separate feature, uses macOS `QLThumbnailGenerator` (Finder-quality thumbnails for everything
  incl. RAW/HEIC/PDF, no decoder). Shares no code with this.
- **ML image-content search**: separate, needs a Rust decode → tensor path.
- **Camera RAW**: WKWebView can't decode it; deferred until there's a Rust decode path or we go cross-platform. The
  `cmdr-media://` scheme is the seam to add it behind later.
- **Video / audio**: a conscious cut. WKWebView would render them in `<video>`/`<audio>` through the same scheme, but v1
  scopes to Image + PDF; video/audio stay on the (narrowed) binary-warning path for now.
- **Markdown / HTML rich rendering**: deferred. `ViewerContentKind` and the picker are shaped to accept them later
  (Markdown via pure-Rust `pulldown-cmark` → sanitized HTML; HTML via a sandboxed iframe).
- **Non-local volumes (MTP/SMB) media rendering**: deferred; non-local files stay Text in v1.
- **Editing, color management, slideshow, in-folder next/prev**: out of scope for v1.

## Cross-platform note

Everything here assumes macOS WKWebView + ImageIO + PDFKit. HEIC and PDF rendering, and EXIF auto-orientation, are
WebKit/macOS behaviors. When Linux/Windows land, re-verify each on webkit2gtk / WebView2 (the custom-scheme origin and
CSP token differ per platform too) and expect to add a Rust decoder for the formats those engines miss (HEIC for sure;
RAW always). That is the moment to reconsider the Prvw decode crate.
