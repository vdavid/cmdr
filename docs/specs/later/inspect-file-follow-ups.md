# What `inspect_file` still owes

Ask Cmdr's `inspect_file` shipped as the one tool that reads inside a file: up to 200 paths per call, a text window in
any encoding the viewer decodes, `find` across text and PDFs, PDF text by page with title and author, one level of an
archive's entries and any file inside one, a photo's EXIF with GPS, every cut and every approximation visible in the
row, and the consent copy, system prompt, and `docs/security.md` all saying so. The canonical account of how it reads is
`apps/desktop/src-tauri/src/agent/tools/DETAILS.md` § Reading a file the way the viewer does; the seams it rides are
documented in `apps/desktop/src-tauri/src/file_viewer/DETAILS.md` § Headless reads and
`apps/desktop/src-tauri/src/crash_reporter/DETAILS.md` § The one exemption.

Eight things are open. Each says what it costs and what would trigger it.

❌ Nothing here restates a mechanism that already has a home. Every item points at the doc or the code that owns it.

## 1. `find` skips archive entry names

**The gap**: `find` applies to text and PDF rows only; an archive row is untouched by it, so "which of these zips has a
`README` inside?" is one call per zip, each listing 200 entries the model then scans itself. Recursive name search and a
`pageOffset`-style paging of an archive's children beyond `MAX_ARCHIVE_ENTRIES` are the same gap.

**Cost**: small for the top level (the `Matcher` already exists and `ArchiveVolume::index()` holds every name);
recursion needs a walk budget under the same per-path deadline.

**Trigger**: a real question that needs it. Decision 6 of the original spec deferred it on purpose.

## 2. Files on a direct SMB, MTP, or SFTP volume answer `unsupportedVolume`

**The gap**: the reader needs a local byte path (`std::fs`), so a path on a volume without `supports_local_fs_access()`
is refused with a typed row rather than read through its `Volume` stream. The viewer has the same limit.

**Cost**: a `Volume`-backed byte source in `file_viewer/headless.rs` is where the agent inherits it the day the viewer
gains one; nothing to build in `inspect/` itself.

**Trigger**: the viewer growing a `Volume` seam.

## 3. Password-protected PDFs and archives stay closed

**The gap**: `textUnavailable: encrypted` and `unreadable { encrypted }` are the whole answer; the tool has no password
path. The viewer prompts the user; the agent can't, and a password typed into a chat would ride the transcript to the
provider.

**Cost**: a design question before any code: where a password would be asked, and how it stays out of the thread.

**Trigger**: a user asking about a protected file and finding the honest refusal insufficient.

## 4. Scanned PDFs have no OCR

**The gap**: a `hasTextLayer: false` row is the honest answer today; `media_index` skips PDFs, so there is no recognized
text to hand over. The system prompt tells the model it is a scan, not an empty document.

**Cost**: an OCR pass over rendered pages, which needs a PDF rasterizer Cmdr doesn't ship.

**Trigger**: demand, weighed against the rasterizer dependency.

## 5. The viewer's search splits on raw `\n` before decoding

**The gap**: `ByteSeek` / `LineIndex` search in `file_viewer` splits lines on raw `\n` bytes before decoding, so a
UTF-16 file over 1 MB (the `FULL_LOAD_THRESHOLD`) searches misaligned: the `\n` code unit is two bytes and the split
lands between them. `file_viewer` and `inspect_file` share it, since `find` rides the viewer's own loop.

**Cost**: decode-aware line splitting in the two backends, plus a test with a >1 MB UTF-16 fixture.

**Trigger**: a user searching a large UTF-16 file in either surface. It is a correctness bug, so it wants fixing before
the others; it is here because it belongs to the viewer, not to the tool.

## 6. An unsupported codec met at EXTRACT time still reads `corrupt`

**The gap**: detection and listing say `unsupported` for a codec the archive layer can't serve, but a file inside such
an archive is refused at extraction, where `archive_extract` folds the cause into `ViewerError::Archive { message }`,
which carries no kind, so the row says `corrupt`.

**Cost**: a typed kind on `ViewerError::Archive` (the archive layer already has `NotSupported`), mapped in
`inspect/archive.rs`.

**Trigger**: a report of a "damaged" archive that opens fine elsewhere.

## 7. GPS has no gate

**The gap**: a photo's coordinates egress whenever the row is an image with a GPS block; the consent copy names it
("including where it was taken") and that is the whole control. A setting, or an on-request parameter (`exif: true`),
would let a user keep location out of every turn.

**Cost**: small: a param on the schema and one branch in `exif_facts`; a setting also needs a settings row and copy.

**Trigger**: David's call on whether consent-copy disclosure is enough, or a user asking for it.

## 8. The `cfg(test)` secrets store falls back to the REAL `secrets.json`

**The gap**: `secrets::TestStore` resolves its directory through `secret_store_dir()`, which honors `CMDR_DATA_DIR` and
otherwise uses the platform data dir, so any test path that reaches the store without `isolate_secrets()` having set the
variable writes to `~/Library/Application Support/com.veszelovszki.cmdr/secrets.json`, the running app's own file. A
killed test run left test credentials there twice on 2026-09-02. It belongs to `apps/desktop/src-tauri/src/secrets/`,
not to this tool; it is recorded here because this effort is where it bit.

**Cost**: small: under `cfg(test)`, make the fallback a per-process temp dir (the shape `crate::test_support::TestDir`
gives every other test artifact) instead of the platform data dir, so an un-isolated test can't reach the real file.

**Trigger**: none needed; it is a data-safety bug in the test setup and should be fixed the next time someone is in that
module.
