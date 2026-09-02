# `inspect_file` v2: answer "what's in this file?" from the shipped viewer core

**The problem.** The first cut of the Ask Cmdr tool `inspect_file`
(`apps/desktop/src-tauri/src/agent/tools/read/inspect.rs`) re-derives what `apps/desktop/src-tauri/src/file_viewer/`
already ships: its own magic sniff beside `content_kind::classify_viewer_content`, a UTF-8-only text test beside
`encoding::detect_from_head` (so a UTF-16 or Latin-1 file reads as binary), a char-offset window beside the three line
backends, its own image-dimension read beside `media::read_image_dimensions`, and `std::fs` on an archive-inner path
(`/x.zip/inner`) that the viewer routes through `archive_extract`. It also has bugs: in the over-8 MB branch
`read_up_to` seeks to 0 so `offset` is ignored while the result claims `offset: N` and `truncated` is unconditionally
`true`; under 8 MB every page re-reads and re-counts the whole file; and the result never passes
`mcp::fit_to_result_budget`. And it stops short of the question it exists for: PDFs get no text, archives answer `{}`,
images carry no EXIF, one path per call, no search within a file.

**The goal.** Make `inspect_file` the tool an agent reaches for when a user asks "what's in this file?", "which of these
is the invoice?", or "does this log mention X?": one call, up to 200 paths, typed per-kind content (a line window of
text, PDF text by page, an archive's entries, an image's EXIF), a `find` that searches inside text and PDFs, and the
whole answer bounded by the tool-result budget. Built on `file_viewer` and `cmdr-archive` through small named seams, so
the agent reads a file exactly the way the viewer shows it.

**KPI.** For a folder of mixed files the agent can, in one or two calls, say what each file is and quote the evidence (a
text line, a PDF page, an entry name, a camera model), with every cut and every approximation visible in the result. No
wrong zero, no silent truncation, no hung turn.

## Decisions already made

David decided (2026-09-02): rebase the tool on `file_viewer`, fix the bugs, add multi-path, `find`, archive listing,
EXIF via `kamadak-exif`, PDF text plus partial view and search, and rewrite the consent copy. Six milestones, one
implementer session each, sequential.

## Decisions this spec makes (the ones to override, if any)

1. **Line-based text window, not char offsets.** `startLine` + `maxLines` with a char cap, over the viewer's line
   backends. Agents reason in lines (every `find` hit is a line number, and "read around line 812" is the natural
   follow-up), `line_index` already exists for exact line seeking, and a char offset can't be stable across encodings
   (the v1 bug was exactly a char offset that couldn't be honored past 8 MB).
2. **One `find` for every path in the call.** `find` applies to every text and PDF path in `paths`, so "which of these
   files mention the tenant?" is one call. Hits are lines (or PDF page + line), capped, with the match count honest.
3. **A scoped panic-report suppression seam for the PDF parser.** `pdf-extract` carries ~100 `unwrap` / `expect` /
   `panic!` sites (counted in its `src/lib.rs` at v0.12.0), and Cmdr's panic hook (`crash_reporter/mod.rs`,
   `install_panic_hook`) writes a crash file and notifies the in-session courier on EVERY panic, including one caught by
   `catch_unwind` on a blocking thread. Without a seam, one malformed PDF is one crash report. The spec adds
   `crash_reporter::contain_panics(|| …)`: a thread-local flag the hook checks first and, when set, logs a warning and
   returns. It is the one deliberate hole in crash reporting, scoped to a third-party parser on untrusted input.
   Alternative if David rejects it: use `lopdf`'s own `Document::extract_text` (Result-based, fewer panics, but no CMap
   handling for CID fonts, so many modern PDFs yield garbage or nothing).
4. **A path on a device or share the reader can't open gets its own typed status, `unsupportedVolume`**, beside the five
   the lead named. An `mtp://` or direct-`smb://` path through `std::fs` would otherwise answer `missing`, which is a
   lie the model relays.
5. **SVG reads as text.** The viewer renders `.svg` as an image; the agent gets more from its markup. The classifier is
   called with `ext = None`, so its SVG rule never fires.
6. **Search-within-archive-names is out of scope.** `find` skips archive rows (a follow-up, listed at the end).

## Tool API

Name stays `inspect_file`: it already has a `ToolId` variant, a registry entry, a rail label, and a place in
`EXPECTED_AGENT_TOOL_NAMES`; nothing about the v2 shape argues for a rename, and a rename would orphan the label keys.

Parameters (the schema rides the cached prefix of every turn, so each description is one sentence):

- `paths: string[]` (required, 1–200). Absolute paths, `~` expanded via `agent::tools::read::expand_tilde` (the same
  helper the other read tools use). Over 200 is `INVALID_PARAMS`, never a silent cut, mirroring `image_facts`.
- `startLine: integer` (1-based, default 1). Text: the first line of the window. PDF: ignored.
- `maxLines: integer` (default 200, max 2,000). Text: how many lines the window holds at most.
- `find: { query: string, regex?: boolean, caseSensitive?: boolean }`. Search inside every text and PDF path; when
  present, the text window is omitted and the hits are the content. `regex` and `caseSensitive` default `false`, and map
  1:1 onto `file_viewer::SearchMode { use_regex, case_sensitive }`.
- `pageStart: integer` (1-based, default 1) and `maxPages: integer` (default 3, max 20). PDF: which pages to extract
  text from. A 300-page manual is read three pages at a time, by choice, never dumped.

Constants (in `inspect.rs`, each with a one-line why):

- `MAX_PATHS = 200` (mirror `image_facts::MAX_PATHS`).
- `DEFAULT_MAX_LINES = 200`, `MAX_MAX_LINES = 2_000`.
- `MAX_WINDOW_CHARS = 16_000`: the most text one row returns, in chars, whatever the line count. The outer
  `fit_to_result_budget` still decides how many rows fit.
- `MAX_LINE_CHARS = 2_000`: a single line longer than this is cut and the row says so (`linesCut: true`). A minified
  bundle is one line of 4 MB; the agent needs to know it IS one line, not read it.
- `MAX_FIND_LINES = 50`: the most matching lines one row carries. `find.totalMatches` stays honest above it.
- `FIND_SNIPPET_CHARS = 300`: the matched line is cut to this many chars around the first match on that line.
- `DEFAULT_MAX_PAGES = 3`, `MAX_MAX_PAGES = 20`, `MAX_PDF_BYTES = 64 MiB` (above it, no parse: the row reports
  `textUnavailable: tooLarge` and keeps the header version).
- `MAX_ARCHIVE_ENTRIES = 200`: immediate children one archive row lists.
- `PATH_TIMEOUT = 5 s`, `CALL_TIMEOUT = 20 s`, `PATH_CONCURRENCY = 4`. See § Timeout and hang policy.

Schema sketch (terse on purpose; the registry `desc` says what the tool is for, the schema says only what each field
means):

```json
{
  "type": "object",
  "properties": {
    "paths": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Absolute file paths (~ ok), at most 200. Rows carry their path; check returned, total, truncated, and unanswered."
    },
    "startLine": {
      "type": "integer",
      "minimum": 1,
      "description": "Text: first line of the window (1-based, default 1)."
    },
    "maxLines": {
      "type": "integer",
      "minimum": 1,
      "maximum": 2000,
      "description": "Text: lines in the window (default 200)."
    },
    "find": {
      "type": "object",
      "properties": {
        "query": { "type": "string" },
        "regex": { "type": "boolean" },
        "caseSensitive": { "type": "boolean" }
      },
      "required": ["query"],
      "additionalProperties": false,
      "description": "Search inside text files and PDFs; returns matching lines instead of the window."
    },
    "pageStart": { "type": "integer", "minimum": 1, "description": "PDF: first page to read (default 1)." },
    "maxPages": { "type": "integer", "minimum": 1, "maximum": 20, "description": "PDF: pages to read (default 3)." }
  },
  "required": ["paths"],
  "additionalProperties": false
}
```

Registry `desc` (one authored source, `mcp/tool_registry/mod.rs`): "Look inside files to say what they are: metadata,
the format the bytes really are, and per kind the content: a line window of text (any encoding), PDF text by page, an
archive's entries, an image's dimensions and camera data. `find` searches inside text and PDFs. Up to 200 paths; every
cut is reported."

## Result DTOs

All in `inspect.rs` (or an `inspect/` folder if it outgrows one file: `mod.rs`, `text.rs`, `pdf.rs`, `archive.rs`,
`exif.rs`, each with its pure shapers and tests). Serde `camelCase`, `skip_serializing_if` on every optional, so the
JSON the model reads stays terse.

### The call result

```text
InspectResult::Ok {
  files: Vec<FileRow>,      // request order, possibly with gaps (see `unanswered`)
  total: usize,             // paths asked about
  returned: usize,          // rows carried
  truncated: bool,          // returned < total, for either reason below
  unanswered: Vec<String>,  // paths with no row: cut by the size ceiling, or not finished by the call deadline (absent when empty)
}
```

`fit_to_result_budget(rows)` gives the prefix cut and `total` / `returned` / `truncated`; `unanswered` is then every
requested path with no row in the kept prefix. The model can join rows back by `path` and ask again for the rest.

### One row: a typed status

```text
FileRow (tag = "status"):
  Ok { path, name, extension?, sizeBytes, sizeHuman, modified?, modifiedHuman?, mime?, content }
  Folder { path }                       // use list_dir
  Missing { path }
  Unreadable { path, reason }           // reason: permission | io | encrypted | corrupt | unsupported | tooLargeToExtract
  Unreachable { path }                  // PATH_TIMEOUT passed: a disconnected drive or a hung mount
  UnsupportedVolume { path }            // mtp:// or direct smb://: no local byte path; the viewer has the same limit
```

- `modified` is RFC 3339 UTC seconds (`chrono`, as v1); `modifiedHuman` is `search::format_timestamp(secs)`
  (`YYYY-MM-DD`), `sizeHuman` is `search::format_size`. Never a second formatter (`agent/tools/DETAILS.md` § Numbers
  arrive already spoken).
- `mime` is the extension guess (`mime_guess`), kept beside `content.kind` / `content.format` so a lying extension
  shows.
- `permission` covers EACCES and a Full Disk Access refusal; the two are not distinguishable from `std::io::Error`, so
  the enum doesn't pretend to.

### `content`, tagged by kind

```text
Content (tag = "kind"):
  Empty {}
  Binary {}
  Text {
    encoding: String,                 // FileEncoding::label(): "UTF-8", "UTF-16 LE", "Western (Windows-1252)"
    totalLines?: usize,               // known: FullLoad, or a LineIndex that finished under the deadline
    lineNumbersApproximate: bool,     // true only on the ByteSeek fallback (serialize only when true)
    window?: TextWindow,              // absent when `find` is present
    find?: FindHits,                  // present when `find` is present
  }
  Image {
    format: String,                   // content_kind::media_mime → "image/jpeg", "image/heic", …
    width?, height?,                  // media::read_image_dimensions (None for HEIC; the image crate can't parse it)
    exif?: ExifFacts,                 // absent when the container carries none
    hint: &'static str,               // "For recognized text and tags inside the picture, call image_facts with this path."
  }
  Pdf {
    version?: String,                 // "1.7", from the header
    pageCount?: usize,                // exact, from lopdf's page tree; absent when the parse failed
    title?, author?,                  // Info dictionary, when present
    pages?: Vec<PdfPage>,             // [{ page, text, truncated }] for the requested range; absent with `find`
    find?: FindHits,
    hasTextLayer: bool,               // false when every requested page decoded to whitespace only (a scan)
    textUnavailable?: PdfTextUnavailable,  // encrypted | tooLarge | unparseable
    pagesReturned, pagesTotal, truncated   // paging honesty for the page range
  }
  Archive {
    format: String,                   // ArchiveFormat display: "zip", "tar.gz", "7z", …
    inner: String,                    // "" for the archive root, else the inner dir listed
    entries: Vec<ArchiveEntry>,       // { name, isDir, size?, sizeHuman?, modified?, modifiedHuman?, encrypted }
    total, returned, truncated,       // immediate children: how many, how many listed, cut?
    hasEncryptedEntries: bool,
  }
```

```text
TextWindow { startLine, returnedLines, content: String, truncated: bool, linesCut: bool }
  // `content` is the lines joined with "\n"; `truncated` means lines exist past the window (or the char cap hit);
  // `linesCut` means at least one line was cut at MAX_LINE_CHARS.

FindHits {
  totalMatches: usize, matchesCapped: bool,       // capped at MAX_SEARCH_MATCHES (10,000), the viewer's own cap
  lines: Vec<FindLine>, returnedLines, truncated, // lines carried vs matching lines found
  scanIncomplete: bool,                           // the deadline stopped the scan; bytesScanned / totalBytes say where
  bytesScanned?, totalBytes?,
}
FindLine { page?: usize, line: usize, matches: usize, text: String }   // `page` for PDFs; `text` is the snippet
```

Every number a model might say out loud has its spoken twin where it's a byte count or a timestamp; counts (lines,
pages, matches) stay plain integers, as `list_dir`'s `total` does.

Text-only by construction: no field on any DTO can hold bytes. Pin it with a `*_is_text_only_no_byte_fields` test like
`image_facts.rs` has (walk the serialized object; every leaf is a string, number, or bool).

## Reuse map

Per behavior, the exact symbol to call and any seam that has to be extracted. Seams are small, named, `pub(crate)`,
documented in the module they land in; nothing is copied.

- **Kind classification**: `file_viewer::content_kind::classify_viewer_content(head, None, true)`. `ext = None` keeps
  SVG on the text path (decision 5); `is_local = true` because the row is already known to be a local file. Head length
  is `content_kind::CLASSIFY_HEAD_LEN` (1,024) for the classifier, and 64 KB for encoding detection (below), so read 64
  KB once and slice.
- **Image format**: `file_viewer::content_kind::media_mime(head, ViewerContentKind::Image)`.
- **Archive detection**: `cmdr_archive::format_for_name(name)` (the suffix single source of truth, re-exported through
  `file_system::volume::backends::archive`) confirmed by `cmdr_archive::bytes_match_archive_magic(format, head)`
  (`boundary.rs`; needs a 512-byte prefix for tar's `ustar` at offset 257, so `ARCHIVE_MAGIC_PREFIX_LEN` bytes of the
  head). Runs before the viewer classifier: an archive is never "text".
- **Text vs binary**: NEW seam `content_kind::looks_binary(head, encoding) -> bool`. The viewer classifies every
  non-media file as `Text` and leaves the warning to the frontend's extension-based `binary-warning.ts`; the agent needs
  a byte-level answer. Rule: if `encoding` is UTF-16 (LE or BE) it's text (NULs are code-unit halves); otherwise binary
  when the head holds a NUL, or when C0 controls other than `\t` `\n` `\r` `\x0c` `\x1b` exceed 5% of bytes (the same
  threshold `encoding::utf16_looks_like_text` uses for the mirror case). Lives in `content_kind.rs` because that's where
  "what should this render as" is decided; unit-tested there.
- **Encoding**: `file_viewer::encoding::detect_from_head(&head[..64 KB])` → `FileEncoding`; `FileEncoding::label()` is
  the string the row carries. Never `String::from_utf8_lossy` on the raw bytes: that was v1's UTF-16-as-binary bug.
- **Opening a text backend without a session**: NEW seam
  `file_viewer::headless::open_text_backend(path, encoding, cancel: &AtomicBool) -> Result<HeadlessBackend, ViewerError>`
  in a new `file_viewer/headless.rs` (`pub(crate)`), where
  `HeadlessBackend { backend: Box<dyn FileViewerBackend>, line_numbers_exact: bool }`. Rule: size ≤
  `FULL_LOAD_THRESHOLD` → `FullLoadBackend::open_with_encoding`; else
  `LineIndexBackend::open_with_encoding(path, encoding, cancel)` (exact lines; ~2 s for 1 GB on an SSD, per
  `file_viewer/DETAILS.md`), and on `ViewerError::Cancelled` (the deadline flipped `cancel`) fall back to
  `ByteSeekBackend::open_with_encoding` with `line_numbers_exact = false`. No `SESSIONS` entry, no watcher, no token, no
  upgrade thread: a backend is an immutable value the caller drops. This is the seam `open_session_core` would use too
  if it were ever split; keep it free of session concerns so it stays reusable.
- **The text window**: `backend.get_lines(&SeekTarget::Line(start_line - 1), max_lines)` →
  `LineChunk { lines, first_line_number, total_lines, .. }`. A pure
  `window_from_chunk(chunk, max_chars, max_line_chars) -> TextWindow` in `inspect` joins lines with `\n`, cuts at the
  caps, and sets `truncated` when `total_lines` says more exist or the char cap stopped the join. Line readers keep `\r`
  on CRLF files (`file_viewer/CLAUDE.md`); strip a trailing `\r` per line in the shaper, since the model gains nothing
  from it. Never `range_read`: it stitches a selection by UTF-16 offsets for the copy flow, and a line window needs none
  of that.
- **`find`**: `file_viewer::Matcher::build(&query, SearchMode { use_regex, case_sensitive })` (rejects invalid and
  cross-line regexes with `MatcherBuildError`, which becomes `INVALID_PARAMS` with the error's `Display` text), then
  `backend.search(&matcher, &cancel, &matches, &progress)` on the headless backend. It returns
  `SearchMatch { line, column, length, byte_offset }` with UTF-16 `column`, capped at `MAX_SEARCH_MATCHES`. Group
  matches by line, take the first `MAX_FIND_LINES` lines, fetch each line's text with `get_lines(Line(n), 1)`, and cut
  the snippet around the first match using `range_read::clamp_utf16_offset_to_byte(line, column)` (make it `pub(crate)`;
  it is the one UTF-16→byte conversion in the tree). `scanIncomplete` is `progress < total_bytes` after a cancel.
- **Archive-inner paths and the archive itself**: `crate::file_system::volume::manager::get_volume_manager()`, then
  `mount_id_for_path(path).unwrap_or("root")` for the parent volume id (the `roots.rs` helper skips `/` and picks the
  longest mount root), then `manager.resolve(volume_id, path)` (`manager/archive_routing.rs`; `block_on` from the
  blocking thread, as `archive_extract` does). `ResolvedVolume { volume, path, is_archive }`:
  - `is_archive` and the resolved node is a directory (the archive root, or an inner dir) →
    `volume.list_directory(&path, None)` → `Vec<FileEntry>` (dirs first, name-sorted, `size` / `modified` /
    `is_directory` / `is_symlink`; `encrypted` comes through `FileEntry`'s archive fields, check
    `cmdr_fs::entry::FileEntry`). Shape to `ArchiveEntry`, cut to `MAX_ARCHIVE_ENTRIES`. This is the same call the pane
    uses to browse a zip, so the agent and the user see the same tree. `VolumeError::NeedsPassword` (a header-encrypted
    7z) → `Unreadable { reason: encrypted }`.
  - `is_archive` and the node is a FILE → make `archive_extract::extract_if_archive_inner` and `ExtractedEntry`
    `pub(crate)`, extract to the bounded temp, run the normal per-kind pipeline on `temp_file`, and `remove_dir_all`
    `cleanup_dir` in a `Drop` guard so a panic or an early return can't leak the temp. `ViewerError::ExtractTooLarge` →
    `Unreadable { reason: tooLargeToExtract }`; `ViewerError::Archive` → `corrupt`. The 256 MiB cap and the
    refuse-before-extract zip-bomb guard come along unchanged.
  - The `.zip` FILE as a `paths` entry: `archive_boundary_candidate` returns an empty inner, so `resolve` treats it as
    the archive root; metadata comes from `std::fs` on the file, content is the root listing.
- **Not local**: `mcp::executor::is_virtual_path` (make `pub(crate)`) for `scheme://` paths, plus
  `!volume.supports_local_fs_access()` on the owning volume → `UnsupportedVolume`. An OS-mounted SMB share
  (`/Volumes/share`) is a real POSIX path and flows through; the timeout is what protects the turn there.
- **Image dimensions**: `file_viewer::media::read_image_dimensions(path)`.
- **EXIF**: `exif::Reader::new().read_from_container(&mut BufReader<File>)` (`kamadak-exif`; JPEG, TIFF, HEIF/HEIC, PNG,
  WebP per its README; GIF and BMP carry none). Pure shaper `exif_facts(&exif::Exif) -> Option<ExifFacts>` with fields
  `dateTaken` (`DateTimeOriginal`, falling back to `DateTime`, as the EXIF ASCII string, no time zone invented),
  `cameraMake`, `cameraModel`, `lens` (`LensModel`), `orientation` (1–8 plus a spoken twin from a fixed table),
  `exposureTime`, `fNumber`, `iso` (`PhotographicSensitivity`), `focalLength` (each via `Field::display_value()`), and
  `gps: { latitude, longitude }` as decimal degrees from `GPSLatitude` / `GPSLatitudeRef` / `GPSLongitude` /
  `GPSLongitudeRef`. Present only when the container has an EXIF block; an empty block is `None`, not a struct of
  absents.
- **PDF**: `pdf_extract::Document::load_mem(&bytes)` (`pdf-extract` re-exports `lopdf::*`, so no direct `lopdf` dep) →
  `doc.version`, `doc.get_pages()` (a `BTreeMap<u32, ObjectId>`; its `len()` is the exact page count; v1's `/Type /Page`
  grep was a lower bound), Info dict `Title` / `Author` through `doc.trailer.get(b"Info")`. Text per page through
  `pdf_extract::output_doc_page(&doc, &mut PlainTextOutput::new(&mut buf), page_num)`, so a page range never decodes the
  rest of the document. `doc.is_encrypted()` → `textUnavailable: encrypted` (no password path; the viewer has none
  either). `find` over a PDF: the same `Matcher`, run with `Matcher::find_matches` per line of each page's text in page
  order until `MAX_FIND_LINES` lines or the deadline; `FindLine.page` set. All of it inside
  `crash_reporter::contain_panics` (decision 3), on the blocking thread, under the path deadline.
- **Panic containment**: NEW seam `crash_reporter::contain_panics<T>(f: impl FnOnce() -> T) -> Option<T>`: sets a
  `thread_local! static CONTAINED: Cell<bool>`, runs `std::panic::catch_unwind(AssertUnwindSafe(f))`, clears the flag,
  returns `None` on a panic. `handle_panic` reads the flag FIRST and, when set, emits one `log::warn!` (target
  `cmdr_lib::crash_reporter`, message plus thread name, never the file's path or bytes) and returns before the crash
  file, the watchdog, and the courier. A thread-local read cannot panic, so the hook's own rule holds. Document it in
  `crash_reporter/CLAUDE.md` as the one exemption and why.
- **Result budget**: `mcp::executor::fit_to_result_budget(rows)` (already `pub(crate)`), then the `unanswered` set.
- **Params**: `agent::tools::read::expand_tilde` for paths; `ToolError::invalid_params` for every rejection.

## Timeout and hang policy

- Every path runs in its own `tokio::task::spawn_blocking`, bounded by `tokio::time::timeout(PATH_TIMEOUT)`, with
  `PATH_CONCURRENCY` (4) in flight (`futures_util::stream::iter(..).buffer_unordered(4)`, results re-sorted into request
  order). On expiry the row is `Unreachable`.
- The deadline also flips a per-path `Arc<AtomicBool>` cancel flag that the LineIndex build, `backend.search`, the PDF
  page loop, and the archive listing (`list_directory_with_cancel`) check. So a slow-but-alive read stops cooperatively
  with partial, flagged results (`lineNumbersApproximate`, `scanIncomplete`, `pagesReturned < pagesTotal`).
- A thread stuck in a kernel call (a `read` on a dead NFS or SMB mount) cannot be cancelled. **The tool abandons it**:
  the `spawn_blocking` task keeps running until the syscall returns, holding a blocking-pool thread, and the row reports
  `unreachable` anyway. This is the same posture as `commands/file_viewer.rs`'s `blocking_viewer_op` ("on timeout the
  detached blocking task still finishes its work"), stated here so nobody reads `unreachable` as "we stopped reading".
  Two hundred paths on a dead mount can park up to 4 threads for as long as the mount is dead.
- `CALL_TIMEOUT` (20 s) bounds the whole call: after it, no new path is launched, in-flight ones are cancelled and
  awaited up to `PATH_TIMEOUT`, and everything unfinished lands in `unanswered`. The model sees `truncated: true` and
  can retry the rest.

## Consent and privacy

`inspect_file` v2 egresses, to the user's provider, on request and bounded:

1. Windows of a text file's contents (any encoding the viewer decodes).
2. PDF text (by page), plus a PDF's title and author.
3. Archive entry names, sizes, and dates (one level per call).
4. Image EXIF: date taken, camera make and model, lens, exposure settings, and GPS coordinates when the photo carries
   them. GPS is the item to name explicitly: a photo's coordinates are a home address.
5. Matching lines from `find`, across many files at once.

Today's `askCmdr.consent.noContents` promises "no file contents", and `askCmdr.consent.item.names` is described as
"never the file contents". Both become false the moment this ships, so the copy changes, `CONSENT_COPY_VERSION` in
`apps/desktop/src-tauri/src/agent/consent.rs` goes 3 → 4 (every existing acceptance then re-prompts; the `whatsNew`
block is what tells those users why), and `docs/security.md` § Ask Cmdr agent egress gains a bullet.

Draft en copy (David reviews all human-facing copy; this is a v0 in `docs/style-guide.md` voice, with the U+2019
apostrophe as the catalog uses):

- `askCmdr.consent.item.names`: unchanged text; its `@description` drops "Never the file contents" and points at the new
  item.
- NEW `askCmdr.consent.item.contents`: "Parts of files Cmdr opens to answer you: text, PDF pages, what’s inside an
  archive, and a photo’s camera details and location"
  - `@description`: Consent list item naming the file contents Ask Cmdr can read on request through its inspect tool:
    windows of text, PDF text by page, the entry names inside an archive, and a photo's EXIF data including GPS.
    Sentence case, no period. The apostrophes are U+2019.
- `askCmdr.consent.noContents` (rewritten; consider renaming the key to `askCmdr.consent.contentsRule`, since
  "noContents" now lies about itself): "Cmdr never sends whole files, photos, or thumbnails. When you ask about a file,
  it can read a bounded part of it: some lines of text, a few PDF pages, the list of what’s in a zip, or a photo’s
  camera details, including where it was taken. Photo search works the same way: the text Cmdr recognized inside the
  matching photos and their tags go to your provider so it can find them. Ask Cmdr can suggest renames, moves, and
  cleanups, and nothing happens to a file until you approve it."
  - `@description`: Reassurance paragraph on the Ask Cmdr consent screen, and in the same disclosure in Settings > Ask
    Cmdr. Four facts to keep: whole files, photos, and thumbnails are never sent; on request the assistant reads a
    bounded part of a file (text lines, PDF pages, archive entry names, photo camera data including location); photo
    search sends recognized text and tags; and the assistant proposes file changes but never carries one out without
    approval. Must NOT promise that no file contents are sent. The apostrophes are U+2019.
- `askCmdr.consent.whatsNew.body` (rewritten): "Ask Cmdr can now look inside a file you ask about: some of its text, a
  few pages of a PDF, what’s in an archive, or a photo’s camera details and location. That’s a bigger promise than the
  one you agreed to, so here it all is again."
  - `@description`: Paragraph under askCmdr.consent.whatsNew.title, shown only to someone re-accepting after the copy
    changed. Names the new thing (reading parts of a file on request, including a photo's location) and says why the
    screen is shown again. The apostrophes are U+2019.
- Rail labels `askCmdr.tool.inspectFile.doing` / `.done` (currently "Looking at a file" / "Looked at a file", en only):
  now plural-neutral, since one call covers many files: "Looking inside files" / "Looked inside files".

Other copy that the branch already made stale and this effort fixes (not consent, but the same promise):

- `agent/chat/system_prompt.rs` still tells the model it has "no tool that reads the contents of a file. Only names,
  paths, and metadata reach you". Rewrite that sentence: the one content tool is `inspect_file`, bounded and on request;
  and add the honesty lines the tool needs (quote `find` snippets verbatim; when `lineNumbersApproximate` or
  `scanIncomplete` is set, say so; a `hasTextLayer: false` PDF is a scan, not an empty document). Pin each with a
  `SYSTEM_PROMPT.contains(..)` test like the existing `image_facts` ones.
- `docs/security.md` § Ask Cmdr agent egress: "no tool that reads a file's bytes" becomes a bullet naming
  `inspect_file`, the five egress items above, the caps, and the statuses. Keep "image bytes and thumbnails never
  egress" and extend it: no raw bytes of any file, pinned by the text-only DTO test.
- `agent/CLAUDE.md` and `agent/tools/CLAUDE.md` both carry a ❌ "the consent copy must name it before release" line. M6
  deletes both lines (the invariant becomes true, so the rule is paid for by no one).

## Docs to update

- `apps/desktop/src-tauri/src/agent/tools/CLAUDE.md`: the `inspect_file` must-know (multi-path, the seams it rides, the
  abandoned-thread caveat) and the module map line; drop the consent ❌ once M6 lands.
- `apps/desktop/src-tauri/src/agent/tools/DETAILS.md`: the catalog entry (rewrite), a new § "Reading a file the way the
  viewer does" (reuse map, seams, the `unanswered` contract), and add `inspect_file` to § The size contract.
- `apps/desktop/src-tauri/src/agent/CLAUDE.md`: the egress-line must-know names `inspect_file` v2's five items; drop the
  ❌.
- `apps/desktop/src-tauri/src/file_viewer/CLAUDE.md` + `DETAILS.md`: `headless.rs` in the module map; `looks_binary`
  beside the classifier; the note that `archive_extract::extract_if_archive_inner` has a second caller, so its cap and
  cleanup contract are now shared.
- `apps/desktop/src-tauri/src/crash_reporter/CLAUDE.md` (+ `DETAILS.md`): `contain_panics`, what it suppresses, why it
  exists, and that a caught panic still logs a warning.
- `docs/security.md`: as above.
- `docs/architecture.md`: only if `inspect/` becomes a folder; then one line under `agent/`.
- `docs/specs/index.md`: this spec's entry; at the end, move leftovers to `later/` and wipe per `docs/specs/DETAILS.md`.
- `apps/desktop/src/lib/ask-cmdr/CLAUDE.md` if the label keys change name (they don't in this plan).

## Test plan

TDD per milestone: write the failing test, see it fail for the right reason, then make it pass (`tdd-red-green`). Rust
unit tests live in the module's `mod tests` or a sibling `*_test.rs`, scratch dirs through
`crate::test_support::TestDir`, no sleeps or poll loops (`docs/testing.md`). Fixtures are generated in the test, never
checked in: the archive crate's `cmdr_archive::test_fixtures::build_zip` (behind its `testing` feature, already a
dev-dependency of the app) for zips; `pdf_extract::Document` (lopdf) to author a two-page PDF with a text content stream
in the test; `exif::experimental::Writer` to author an EXIF blob for `exif::Reader::read_raw`, plus one hand-assembled
JPEG (SOI, APP1 with `Exif\0\0` + the blob, EOI) for the container path; the existing UTF-16 / Windows-1252 text
fixtures in `apps/desktop/test/fixtures/encodings/` can be reused by path from `CARGO_MANIFEST_DIR` when a test wants a
real file rather than a written one. `apps/desktop/test/e2e-shared/media-fixtures/sample.pdf` (1 page, 583 bytes) is the
E2E asset, not a unit-test fixture.

Per area:

- **Params**: defaults, caps, `MAX_PATHS + 1` rejected, `~` expanded, `find` with an invalid regex rejected with
  `INVALID_PARAMS`, cross-line regex rejected.
- **Kinds**: a UTF-16 LE file with a BOM is text with `encoding: "UTF-16 LE"` (the v1 regression); a Windows-1252 file
  is text; a file with NULs is binary; an SVG is text; an empty file is `empty`; a `.txt` that is really a PNG has
  `mime: text/plain` and `content.kind: image`.
- **Window**: `window_from_chunk` on LF and CRLF (`\r` stripped), `startLine` past EOF gives an empty window with
  `truncated: false`, the char cap sets `truncated`, a 5,000-char line sets `linesCut`, `totalLines` present for
  FullLoad and absent when the ByteSeek fallback ran (drive the fallback by pre-setting the cancel flag).
- **Bug regressions**: a 9 MB file read at `startLine: 100_000` returns lines from there (not from 0); `truncated` is
  false on the last page; the file is opened once per call (count `File::open`s through a `LineIndexBackend`
  `test_only_open_call_count`-style hook, or assert by timing budget, whichever the code allows without a sleep).
- **`find`**: literal and regex, case-insensitive, hits grouped by line with `matches` counts, `MAX_FIND_LINES` cap with
  honest `totalMatches`, snippet cut around a UTF-16 column on a line with emoji (the surrogate clamp), `scanIncomplete`
  when the cancel flag is pre-set, `matchesCapped` on a corpus over 10,000 matches (reuse the shape of
  `search_cancel_test_support.rs`).
- **Archive**: root listing of a built zip (dirs first, sizes and dates spoken), an inner dir, an inner text file read
  through the temp with the temp gone afterwards, `ExtractTooLarge` → `unreadable: tooLargeToExtract` with a
  test-injected cap, an encrypted entry flagged, a header-encrypted 7z → `unreadable: encrypted`, the `.zip` path itself
  lists the root.
- **EXIF**: the pure shaper over a `Writer`-built blob (every field, GPS sign from the Ref, missing block → `None`), and
  the container read on the hand-assembled JPEG; a PNG without EXIF has no `exif` key.
- **PDF**: page count exact on the authored two-page PDF, `pages` for `pageStart: 2, maxPages: 1`, `hasTextLayer: false`
  on a page with only a drawn rectangle, `find` hits carry `page`, an encrypted PDF (author with lopdf's `encrypt`) →
  `textUnavailable: encrypted`, a truncated file → `unparseable`, and a deliberate `panic!` inside `contain_panics`
  returns `None` while the hook wrote no crash file (drive `handle_panic` through the flag directly).
- **Statuses**: folder, missing, `mtp://x` → `unsupportedVolume`, permission (chmod 000 in the test, skipped as root).
- **Budget**: 200 dense rows page with `fit_to_result_budget` and `unanswered` names exactly the rows not carried; the
  text-only DTO walk.
- **Registry and prefix**: `test_agent_tool_view_is_exactly_expected_set` stays green (no new name); the label test in
  `agent/tools/mod.rs` stays green; `context/cost_tests.rs` re-pins `FIXED_PROMPT_OVERHEAD_TOKENS` and
  `TOOL_DECLARATION_TOKENS` after the schema change (a conscious bump, recorded in the commit message per
  `agent/chat/DETAILS.md` § What the budgets buy).
- **Prompt**: `SYSTEM_PROMPT.contains(..)` pins for the rewritten content sentence and the new honesty lines.
- **i18n**: `pnpm check i18n-parity i18n-coverage i18n-stale message-keys-fresh` green for every catalog after M6.

## Milestones

Each is one implementer session, ends in a commit (impact-first message, no AI attribution), and runs the checks it
names. Scope the checks to what changed (`pnpm check rust-tests clippy rustfmt` for a Rust-only milestone); plain
`pnpm check` at M3 and M5; `--include-slow` once at M6.

### M1: rebase on `file_viewer`, multi-path, bug fixes

- Files: `file_viewer/headless.rs` (new) + `file_viewer/mod.rs` (`mod headless;`), `file_viewer/content_kind.rs`
  (`looks_binary`) + `content_kind_test.rs`, `file_viewer/range_read.rs` (`clamp_utf16_offset_to_byte` → `pub(crate)`),
  `mcp/executor/mod.rs` (`is_virtual_path` → `pub(crate)`), `agent/tools/read/inspect.rs` (rewrite: params, DTOs,
  text/image/binary/empty kinds, multi-path runner with per-path timeout and concurrency, `unanswered`,
  `fit_to_result_budget`), `mcp/tool_registry/mod.rs` (schema and `desc`), `agent/chat/context/cost_tests.rs` (re-pin),
  `file_viewer/CLAUDE.md` + `DETAILS.md`, `agent/tools/CLAUDE.md` + `DETAILS.md`.
- Acceptance: every v1 test survives in v2 form; the UTF-16, Windows-1252, SVG, and lying-extension tests pass; the 9 MB
  `startLine` regression passes; a text row never re-reads the file per page; 200 rows page with `unanswered`; `mtp://`
  → `unsupportedVolume`; the image row keeps dimensions and the `image_facts` hint; archive and PDF rows are
  `Archive { entries: [], total: 0, … }` and `Pdf { version, textUnavailable: none-yet }` placeholders ONLY if M3/M5
  can't land in the same release, otherwise leave them as `Binary` until their milestone (prefer the latter; a
  placeholder that says "not wired" is what v1 did).
- Checks: `pnpm check rust-tests clippy rustfmt file-length`.

### M2: `find`

- Files: `agent/tools/read/inspect.rs` (or `inspect/find.rs`), `file_viewer/headless.rs` if a `search_lines` helper
  wants to live beside the backend opener, schema + `desc`, `cost_tests.rs` re-pin, `agent/tools/DETAILS.md`.
- Acceptance: the `find` tests above; `find` over two text paths in one call; the window is omitted when `find` is
  present; an invalid regex is `INVALID_PARAMS` with `MatcherBuildError`'s text.
- Checks: `pnpm check rust-tests clippy rustfmt`.

### M3: archives (listing and archive-inner routing)

- Files: `file_viewer/archive_extract.rs` (`extract_if_archive_inner`, `ExtractedEntry` → `pub(crate)`),
  `agent/tools/read/inspect.rs` (or `inspect/archive.rs`: detection, `resolve`, listing shaper, the extract-then-inspect
  path with a `Drop` cleanup guard), `file_viewer/DETAILS.md` (§ Preview inside an archive gains its second caller),
  `agent/tools/DETAILS.md`.
- Acceptance: the archive tests above; the temp is gone after an inner-file read even when the inner pipeline returns
  early; a `.zip` on an OS-mounted SMB parent still resolves through `mount_id_for_path`.
- Checks: `pnpm check` (plain; the archive crate's `testing` feature and the volume manager are in play).

### M4: EXIF

- Files: `apps/desktop/src-tauri/Cargo.toml` (`kamadak-exif = "0.6.1"`, with a why comment and the verification date),
  `agent/tools/read/inspect.rs` (or `inspect/exif.rs`), `agent/tools/DETAILS.md`, `docs/security.md` (the GPS sentence
  can land here or in M6; M6 owns the consent copy).
- Acceptance: the EXIF tests above; `cargo deny check` green (it was, see § Crate verification); a JPEG without EXIF has
  no `exif` key; HEIC EXIF parses (author a minimal HEIF in the test only if `Writer` can; otherwise mark the container
  as covered by kamadak-exif's own tests and pin the JPEG path here).
- Checks: `pnpm check rust-tests clippy rustfmt cargo-deny`.

### M5: PDF text, partial view, and `find` over PDF

- Files: `Cargo.toml` (`pdf-extract = "0.12.0"`; it re-exports lopdf 0.42, so no direct `lopdf` line),
  `crash_reporter/mod.rs` (`contain_panics` + the hook check) + `crash_reporter/CLAUDE.md` + `DETAILS.md`,
  `agent/tools/read/inspect.rs` (or `inspect/pdf.rs`), schema (`pageStart`, `maxPages`) + `desc`, `cost_tests.rs`
  re-pin, `agent/tools/DETAILS.md`.
- Acceptance: the PDF tests above, including the contained-panic test proving no crash file; `MAX_PDF_BYTES` respected;
  a PDF `find` hit carries `page`; the header `version` still answers when the parse fails.
- Checks: `pnpm check` (plain).

### M6: consent copy, version bump, i18n, prompt, docs sweep

- Files: `apps/desktop/src/lib/intl/messages/en/askCmdr.json` (the four consent keys, the two label keys, descriptions),
  `apps/desktop/src-tauri/src/agent/consent.rs` (`CONSENT_COPY_VERSION = 4`), the consent screen component if a new list
  item needs rendering (find it by the `askCmdr.consent.item.*` keys; the list is data-driven or a fixed set, check
  which), `agent/chat/system_prompt.rs` + its tests, `docs/security.md`, `agent/CLAUDE.md`, `agent/tools/CLAUDE.md`,
  `apps/desktop/src/lib/intl/messages/{de,es,fr,hu,nl,pt,sv,vi,zh,zh-Hant}/askCmdr.json` (the 10 full catalogs; the
  `en-GB` / `en-AU` overlays only if they override any of these keys today), `docs/specs/index.md` (move leftovers to
  `later/`, wipe this spec per `docs/specs/DETAILS.md`).
- i18n process: follow `docs/guides/i18n-translation.md` exactly (the reference pile lives ONLY in the main clone at
  `~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/`, a worktree can't see it; read each language's
  `docs/i18n/<tag>/style.md` first and extend it; record `sourceHash` via the tooling; never special-case Hungarian).
  One translator subagent per language is the documented shape; the keys are: `askCmdr.tool.inspectFile.doing`,
  `askCmdr.tool.inspectFile.done`, `askCmdr.consent.item.contents`, `askCmdr.consent.noContents` (or its renamed
  successor), `askCmdr.consent.whatsNew.body`, and the `@description` refresh of `askCmdr.consent.item.names`. Then
  `pnpm intl:keys`.
- Acceptance:
  `pnpm check i18n-parity i18n-coverage i18n-stale i18n-dont-translate message-keys-fresh message-keys-unused` green;
  the consent screen renders the new item (David QAs visuals himself; a build is enough); `has_current_consent` tests
  still pass with the new constant; the two ❌ lines are gone; `docs-reachable` and `docs-dead-links` green after the
  spec wipe.
- Checks: `pnpm check --include-slow`.

## Crate verification (2026-09-02)

Checked on crates.io and GitHub, then `cargo deny check` with both crates tentatively added to
`apps/desktop/src-tauri/Cargo.toml` (reverted afterwards; the implementers add them in M4 and M5).

- **`kamadak-exif` 0.6.1** (crate name `exif` in code): published 2024-11-06, BSD-2-Clause (allowed in `deny.toml`), one
  dependency (`mutate_once`), 13.1M downloads. GitHub `kamadak/exif-rs`: 257 stars, last push 2025-10-21, not archived,
  29 open issues. Reads EXIF from JPEG, TIFF, HEIF/HEIC/AVIF, PNG, and WebP containers (README). Mature and quiet rather
  than dead: the EXIF spec doesn't move.
- **PDF: `pdf-extract` 0.12.0**, picked. Published 2026-06-25 (69 days old), MIT, 4.5M downloads. GitHub
  `jrmuizel/pdf-extract`: 596 stars, last push 2026-06-25, not archived, 75 open issues. It is the one mature crate
  whose purpose is text extraction: fonts, CMaps, ToUnicode, Type1 and CFF encodings are handled, and it exposes
  `output_doc_page` for a page range and `pub use lopdf::*` so page count, version, and the Info dict come from lopdf
  0.42 without a second dependency line. Cost: ~100 `unwrap` / `expect` / `panic!` sites, hence decision 3. Pulls
  `adobe-cmap-parser`, `cff-parser`, `euclid`, `postscript`, `type1-encoding-parser`, `unicode-normalization`, and
  lopdf's `pom`, `md-5`, `ecb`, `rangemap`, `stringprep`, `ttf-parser` into the lock.
- **Rejected**: `lopdf` 0.44.0 alone (2026-07-10, 2,239 stars, last push 2026-08-24): the healthiest parser, but its
  `extract_text` has no CMap handling, so CID-font PDFs (most modern exports) decode to garbage; it rides along under
  `pdf-extract` anyway. `pdf` 0.10.0 (`pdf-rs`, 2026-03-02, 1,691 stars): a parser, not an extractor; text would mean
  writing the content-stream walk and font decoding ourselves. `pdf_oxide` 0.3.77 (2026-07-28, 1,010 stars): a release
  every few days in 0.3.x, 216 open issues, too young to pin. `pdfium-render` 0.9.3: needs the pdfium C library shipped
  beside the app.
- **`cargo deny check`**: `advisories ok, bans ok, licenses ok, sources ok` with both crates added. The only output was
  the pre-existing `chacha20 0.10.1` yanked warning, reached through `rand 0.10` and `russh`, unrelated to this change.

## Out of scope, and follow-ups to route to `later/`

- `find` over archive entry names (recursive), and a `pageOffset`-style paging of an archive's children beyond
  `MAX_ARCHIVE_ENTRIES`.
- Reading a file on a direct SMB, MTP, or SFTP volume through its `Volume` byte stream (the viewer has the same gap;
  when the viewer gains a `Volume` seam, `headless.rs` is where the agent inherits it).
- Password-protected PDFs and archives (the tool has no password path; the viewer prompts the user, the agent can't).
- OCR for scanned PDFs (`media_index` skips PDFs; a `hasTextLayer: false` row is the honest answer today).
- Exposing `inspect_file` to the ai-client (MCP) view. It stays `consumers: [Agent]`; the external view has no consent
  gate of its own.
