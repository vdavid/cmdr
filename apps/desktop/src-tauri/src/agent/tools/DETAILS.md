# Agent tools — details

The toolset the Ask Cmdr chat agent dispatches in-process: the read families, the `Propose` tier over them, and the
two `Memory` tools. Must-knows: `CLAUDE.md`.

## The two-view registry model

There is ONE authored tool table (`mcp/tool_registry/mod.rs`, `mcp_tools!`). Each entry declares `consumers`
(`AiClient` / `Agent`) and `access` (`Read` / `Propose` / `Memory` / `Write`). The agent's tools are
`consumers: [Agent]` entries, never `access: Write`; `operations_list` / `operations_get` / `search_photos` /
`image_facts` are shared `[AiClient, Agent]`. `agent_tool_view()` is the agent's slice;
`get_all_tools()` is the ai-client slice (agent-only entries filtered out, so the ai-client wire snapshot is unchanged).
`execute_tool(app, Consumer::Agent, name, params)` dispatches only the agent view. See
[`mcp/tool_registry` + `mcp/DETAILS.md`](../../mcp/DETAILS.md) § Consumer and access views for the mechanism.

Why the handlers live under `agent/` but the entries under `mcp/`: the registry is one authored source (D49), but a
tool's handler, schema, and typed result belong with the feature that owns them (organized by feature, not layer). So
the `mcp_tools!` entry names the handler/schema by path into `crate::agent::tools::read::*`, and `agent::tools` reaches
back for the dispatch surface (`execute_tool`, `agent_tool_view`, `tool_access`, `Consumer`, `Access`, `ToolError`,
`ToolResult`), all re-exported from `mcp` for this. It's a same-crate module cycle, which Rust allows and which D49
makes intentional.

## The tool catalog

Each handler is `async fn(&AppHandle<R>, &Value) -> ToolResult` (the `app_params` macro shape), reuses a shipped core,
and returns a typed serde shape as the tool-result JSON the model reads. Every tool maps 1:1 to a `ToolId` variant.

- **`app_state`** (`read/state.rs`) — both panes (path, cursor item, selection count, view/sort) plus the volume list.
  Built from `PaneStateStore` (`get_focused_pane` returns the SIDE; the path comes from that side's state) +
  `snapshot_volumes`. Not the private `build_state_yaml` — typed data, not parsed YAML. `selectedEntries` lists the exact
  selection only when it's complete AND fits one result; otherwise it's absent with a typed
  `selectedEntriesOmitted` (`outsideWindow` / `tooMany`), never a half list that would read as the whole selection.
  `selectedCount` is always honest, and `list_pane_files` pages the names.
- **`list_pane_files`** (`read/pane_listing.rs`) — up to 200 compact rows from the focused pane's existing Rust
  listing cache. It uses the current selection when present, otherwise the whole folder, and returns the exact volume
  ID plus one shared parent path for `propose_rename_plan`. It never queries the index or starts a filesystem listing.
- **`list_dir`** (`read/listing.rs`, shared `[AiClient, Agent]`) — a directory's immediate children
  (`indexing::list_dir_children`, a path-based helper beside `get_dir_stats`) plus its own recursive size stats
  (`get_dir_stats`) and a `Coverage` block. `Ok(None)` children ⇒ typed "not in index" / "no index", distinguished by
  whether the volume is indexed. Ordered by `sortBy` (`name` / `size` / `modified`) and paged by `limit` / `offset`;
  `type` narrows to files or folders. **`sortBy: "size"` is the disk-usage answer** — see § One tool, both questions.
  Every number also arrives spoken (`sizeHuman`, `modifiedHuman`, `recursiveSizeHuman`, `totalHuman` /
  `availableHuman`) and a paged answer carries a `remainder` — see § Numbers arrive already spoken.
- **`important_folders`** (`read/importance.rs`) — top-N or above-threshold across scored volumes, reusing
  `mcp::resources::importance::{snapshot_top, snapshot_threshold, snapshot_overview}` (which read every scored volume,
  including offline ones). The overview carries each volume's current generation for staleness.
- **`folder_importance`** (`read/importance.rs`) — one folder's `PathImportance` (`snapshot_path`): Scored (score +
  `Explanation` breakdown + `stale` from asOf vs the volume's current `recompute_generation`), Floored (with reason), or
  Unscored. Offline-capable.
- **`inspect_file`** (`read/inspect/`) — "what's in this file?" for up to 200 paths in one call. Each row: metadata
  (`sizeBytes` + `sizeHuman`, `modified` + `modifiedHuman`), the extension's `mime` beside `content.kind` (so a lying
  extension shows), and a typed `content` per kind. Text is a line window (`startLine` + `maxLines`, default 200, max
  2,000, capped at 16,000 chars and 2,000 chars a line) read through the viewer's own backends, with `encoding`,
  `totalLines` when known, `lineNumbersApproximate` on the ByteSeek fallback, and `truncated` / `linesCut` on the
  window. With `find: { query, regex?, caseSensitive? }` the window is replaced by `find`: the matching lines (up to
  50, each `{ line, matches, text }` with `text` a 300-char snippet around the first match), `totalMatches`,
  `matchesCapped` (the viewer's 10,000-match cap), `returnedLines` / `truncated`, and `scanIncomplete` with
  `bytesScanned` / `totalBytes` when the deadline stopped the scan. One `find` applies to every text path in the call;
  images give `format` + dimensions and point at `image_facts`; an archive (`.zip`, tar, 7z), or a directory inside
  one, lists its immediate children (`format`, `inner`, up to 200 `entries` with `isDir`, `size` + `sizeHuman`,
  `modified` + `modifiedHuman`, `encrypted`, plus `total` / `returned` / `truncated` and `hasEncryptedEntries`), and a
  FILE inside an archive is read as its own kind through the viewer's bounded temp; empty and binary (which today
  includes PDFs) carry metadata only. Per-path statuses: `ok` / `folder` / `missing` /
  `unreadable { permission | io | encrypted | corrupt | tooLargeToExtract }` / `unreachable` / `unsupportedVolume`. The call reports `total` / `returned` / `truncated` and names every path with
  no row in `unanswered`. The sole disk reader among the handlers; how it reads and how it times out: § Reading a
  file the way the viewer does.
- **`list_volumes`** (`read/volumes.rs`) — every volume with `indexStatus` (`fresh`/`scanning`/`stale`/`off`) and, for
  SMB, `smbConnectionState` (`direct`/`os_mount`/`disconnected`), straight from `snapshot_volumes` so tokens can't drift.
  Space rides along as `totalBytes` / `availableBytes` plus `totalHuman` / `availableHuman`, each pair present exactly
  when the poller has a reading (the same pair `cmdr://state`'s `volumes:` renders; see `mcp/DETAILS.md`).
- **`operations_list` / `operations_get`** — the shipped executors (`mcp/executor/operation_log.rs`), shared into the
  agent view unchanged (their schemas + coverage flags already fit an agent reader).
- **`search_photos`** (`mcp/executor/photos.rs`, shared `[AiClient, Agent]`) — photo search by description (CLIP),
  in-image text (OCR), or Vision tag. Shapes the `media_index` read API into a TEXT-ONLY DTO (path + volume + typed
  `matchKind` + optional score + optional OCR snippet / no image bytes), reuses `media_index`'s own `volume_state` for
  per-volume coverage honesty, and returns a typed status when indexing is off, still building, or the CLIP model isn't
  installed. Privacy: the OCR snippet + tags it returns are image-derived text that egresses to the provider — named in
  the Ask Cmdr consent copy (see `mcp/executor/photos.rs` and `docs/security.md`).
- **`image_facts`** (`mcp/executor/image_facts.rs`, shared `[AiClient, Agent]`) — the lookup direction of the same
  index: given paths the agent already has, the FULL stored OCR text (capped at 2,000 characters per file, a cut
  flagged) plus the Vision tags for each. It accepts up to 200 paths but answers as many as fit one result (see § The
  size contract), reporting `total` / `returned` / `truncated` so the caller batches the rest. Backs naming/describing files the user is looking at. Same text-only DTO, same coverage
  honesty (it reuses `photos.rs`'s helpers), and a typed per-path `indexed` / `notIndexed` so a not-yet-enriched file
  is never read as an empty one. Privacy: this is the widest derived-content egress the agent has (full recognized
  text, not a snippet) — same consent gate, same copy.

- **`list_suggestions` / `get_suggestion_group` / `propose_suggestions`** (`suggestions/`) — the suggested-ops surface
  over the proposal spine: what the agent has already put in front of the user (summaries and counts, never op rows),
  one group's ops paged, and the one tool that stages a sweep or amends a pending group. Access classes, the
  resolve-check-write order, the selector schema, and the last-opened gap: `suggestions/DETAILS.md`.
- **`nothing_to_suggest`** (`quiet.rs`) — one argument, a short `reason`, and no effect at all. See § A tool that is
  only a signal.
- **`memory_write` / `memory_edit`** (`memory.rs`) — the two `Access::Memory` tools. See § The two tools that write.

## A tool that is only a signal (`nothing_to_suggest`)

Every other entry here answers a question or stages a proposal. This one exists so a wake can say "I looked, and none
of it is worth your attention" in a form the code can read.

**Why typed rather than phrased.** A wake that finds nothing must leave no thread behind, and deciding that from the
model's wording would classify control flow by text — what `error-string-match` forbids, and what breaks on the first
copy edit or non-English reply. The call resolves to `ToolId::NothingToSuggest`, and that is what the wake path matches
on.

**Why the handler is inert.** There is ONE `agent_tool_view()`, so the rail sees this tool too. A handler that deleted
the conversation would be `Access::Write` under the registry's tiebreaker (failing `test_agent_tool_view_never_writes`),
and it would delete a USER's thread the moment a rail turn called it. So the handler acknowledges and returns; the
delete lives on the wake path, after the turn (`agent/wake/`), and a rail turn calling this changes nothing —
`wake/tests/job.rs` pins that.

**Why the `reason` never reaches a log.** It exists for the agent's own memory (M3) and is trimmed to
`MAX_REASON_CHARS`. ❌ It must never be logged verbatim: `cmdr.log` ships inside error reports, including the
auto-dispatched ones the user never previews, and `redact::redact_line_salted` is path-shaped, so it does nothing to a
sentence about which of the user's folders were boring. Log that a wake was quiet, never what it said.

**What it costs everyone else.** The schema is prefix, so all 17 declarations are paid on every rail turn: this one is
97 tokens of the 5,257 fixed overhead (`agent/chat/DETAILS.md` § What the budgets buy). That's the price of the wake
being able to stay silent, and it's why the description is two sentences.

## The two tools that write (`memory_write`, `memory_edit`)

Everything either tool DECIDES lives in `../memory/` — the jail, the two caps, the edit's uniqueness rule — so this
file holds only root resolution (`memory::store_for`, which reads the app data dir) and result shaping. That split is
not tidiness: there is no Tauri mock runtime in the tree and every registry handler takes an `AppHandle`, so a rule
placed here is a rule no test can reach. `../memory/DETAILS.md` § Testing shape.

**A refusal is an `Ok` carrying a typed token, ❌ never a `ToolError`.** `view::dispatch` flattens a `ToolError` to
`{ "problem": <sentence> }`, and a model that has to read prose to learn its memory is full will keep writing into a
folder that is saving nothing. So the result is `{ saved: false, refused: <token>, detail: <sentence> }`, where
`refused` is `MemoryRefusal::token()` and the sentence names a next move — a refusal with no next move gets retried
verbatim. Same rule as `error-string-match`, pointed at the model instead of at our own code.

A landed write answers `{ saved: true, path, bytes, remainingBytes }`. `remainingBytes` is what the folder has left
against its 64 KB disk cap, so the model can see pruning coming rather than discovering it at a refusal.

⚠️ **Both are callable from the RAIL, not only from a wake.** "Remember that I keep invoices by year" is what the
folder is for. It is also the mechanism behind the injection risk the prompt fences against
(`../memory/DETAILS.md` § The injection surface), so it is stated rather than implied.

## Reading a file the way the viewer does (`inspect_file`)

The tool re-derives nothing the viewer already ships. Per behavior, the symbol it calls (`read/inspect/mod.rs`,
`text.rs`, `archive.rs`):

- **Head**: one 64 KB read per file. `encoding::detect_from_head` takes all of it; the classifier takes the first
  `content_kind::CLASSIFY_HEAD_LEN` bytes of the same buffer.
- **Kind**: `file_viewer::content_kind::classify_viewer_content(head, None, true)`. `ext = None` keeps SVG on the text
  path (its markup says more to a model than "an image"); `is_local = true` because the row is already a local file.
  `Image` → `content_kind::media_mime` for the format and `media::read_image_dimensions` for the size (`None` for HEIC).
  `Pdf` → `binary` (no text parser here).
- **Text vs binary**: `content_kind::looks_binary(head, encoding)`, the seam the viewer doesn't need (it leaves the
  warning to the FE's extension list). UTF-16 is never binary; a NUL or a control-byte share over 5% is.
- **Encoding**: `encoding::detect_from_head` → `FileEncoding::label()` is the string the row carries. Never
  `String::from_utf8_lossy` on the raw bytes: that read every UTF-16 file as binary once.
- **The backend**: `file_viewer::headless::open_text_backend(path, encoding, cancel)`: FullLoad up to 1 MB, else a
  LineIndex built under the cancel flag, falling back to ByteSeek with `line_numbers_exact = false` when the deadline
  flips the flag (`file_viewer/DETAILS.md` § Headless reads). No session, no watcher, nothing to tear down.
- **The window**: `backend.get_lines(Line(startLine - 1), maxLines + 1)`. The extra line says exactly whether more
  exist, on every backend, without leaning on `total_lines`. `window_from_chunk` (pure) joins with `\n`, strips one
  trailing `\r` per line (the backends keep it on CRLF files), cuts a line at `MAX_LINE_CHARS` (`linesCut`), stops at
  `MAX_WINDOW_CHARS` (`truncated`), and answers a past-the-end `startLine` with an empty, un-truncated window (the
  exact backends clamp such a target to the last line; the shaper must not present that line as line 50). `totalLines`
  counts the trailing empty line after a final newline, as the viewer's line numbers do, so "line 812" means the same
  thing in both places.
- **`find`** (`find.rs`): `file_viewer::Matcher::build(query, SearchMode { use_regex, case_sensitive })` once per call
  at param time (a `MatcherBuildError`, invalid or cross-line regex, is `INVALID_PARAMS` carrying the matcher's own
  text), shared by every path through `TextAsk::Find(Arc<Matcher>)`. Per text row: `headless::open_scan_backend`
  (FullLoad, or ByteSeek with no index: a scan streams from byte 0 and numbers lines exactly, so an index would only
  read the file twice), then `backend.search(matcher, cancel, matches, progress)`, the viewer's own loop, capped at
  `MAX_SEARCH_MATCHES`. Matches are grouped by line in arrival order, the first `MAX_FIND_LINES` (50) lines are fetched
  by `SeekTarget::ByteOffset(match.byte_offset)` (exact on every backend; `Line(n)` is a guess on ByteSeek), `\r`
  stripped, and cut by `snippet_around` to `FIND_SNIPPET_CHARS` (300) around the first match, a third before it, with
  `…` at each cut end. The match column is UTF-16 (the viewer's JS-facing unit) and goes through
  `range_read::clamp_utf16_offset_to_byte`, the one UTF-16→byte conversion in the tree; read as a char index it lands
  twice as far along a line of emoji. `matchesCapped` is `totalMatches ≥ 10,000`; `scanIncomplete` is a flag-stopped
  scan that didn't reach the cap or the end (`bytesScanned` / `totalBytes` say where, spoken twins beside them). A
  `find` row never sets `lineNumbersApproximate` (nothing in it is estimated) and has `totalLines` only for a FullLoad
  file. Non-text rows are untouched by `find`.
- **Not local**: `mcp::is_virtual_path` (`mtp://`, direct `smb://`) → `unsupportedVolume`, and so does a scheme-less
  path whose owning volume (`VolumeManager::mount_id_for_path`, else `root`) reports
  `!supports_local_fs_access()`. A `missing` there would be a lie the model relays. An OS-mounted share
  (`/Volumes/share`) is a real path and flows through; the timeout is what protects the turn.
- **Archives** (`archive.rs`): the pane's own routing, before any `std::fs`. A path with an archive-named component
  (`cmdr_archive::archive_boundary_candidate`, a pure string check) goes through `VolumeManager::resolve(volume_id,
  path)` (`block_on` from the blocking thread, as `archive_extract` does): the shared boundary detector confirms the
  format by name and magic bytes and hands back the on-demand `ArchiveVolume`, or a passthrough for a mislabeled
  `.zip`, which then reads as text or binary. The row is then built from the archive's cached index
  (`ArchiveVolume::index()`, the seam the volume opened for this: `FileEntry` has no `encrypted` field, and
  `list_directory` is a thin map over the same nodes; the key is the inner path the boundary candidate split off): a
  directory node (the root, or one inside) →
  `Content::Archive` from `index.list(inner)`, cut at `MAX_ARCHIVE_ENTRIES` (200), dirs first as the pane lists them,
  format from `ArchiveFormat::label()`; the archive root's row metadata is the `.zip` file's own `std::fs` stat, an
  inner directory's `sizeBytes` is absent (never a zero). A file node → refused `unreadable { encrypted }` from the
  node's flag BEFORE extraction (the tool has no password path), else
  `archive_extract::extract_if_archive_inner(path, volume_id)` streams it to the viewer's bounded temp (the same
  256 MiB refuse-before-extract cap; `ExtractTooLarge` → `tooLargeToExtract`, `ViewerError::Archive` → `corrupt`),
  `read_content` runs the normal per-kind pipeline on `temp_file` (so `find` and the window work inside a zip), and
  `TempCleanup` removes `cleanup_dir` in `Drop`, so an early return or a panic can't leak it. A zip inside a zip is
  `binary`: the boundary is the leftmost archive component, as in the pane. The parse errors map typed:
  `NeedsPassword` (a header-encrypted 7z) → `encrypted`, `IoError` / `NotSupported` (the archive layer's damaged /
  unsupported / over-cap collapse) → `corrupt`. The extract step is injected (`ExtractFn`) so the tests shrink the cap
  and watch the temp dir.
- **Statuses from I/O**: `NotFound` / `NotADirectory` → `missing`; `PermissionDenied` → `unreadable { permission }`
  (EACCES and a Full Disk Access refusal are one kind of `std::io::Error`, so the enum doesn't pretend to tell them
  apart); anything else, including a read that panicked → `unreadable { io }`.

**The runner (`runner.rs`).** Every path is its own `spawn_blocking`, `PATH_CONCURRENCY` (4) in flight
(`buffer_unordered`, slots re-sorted into request order). A path's budget is `PATH_TIMEOUT` (5 s) in two phases: the
cooperative window, then the deadline flips the path's `AtomicBool` and waits `CANCEL_GRACE` (1 s) for a partial,
flagged answer (a LineIndex still scanning falls back to ByteSeek in milliseconds), then the row is `unreachable` and
the task is dropped, which detaches it. A thread stuck in a kernel call cannot be cancelled: the tool ABANDONS it,
holding a blocking-pool thread until the syscall returns (the same posture as `commands/file_viewer.rs`'s
`blocking_viewer_op`), so `unreachable` never means "we stopped reading". `CALL_TIMEOUT` (20 s) bounds the call: past
it no new path starts, and each unlaunched path is an empty slot. The policy is a `RunnerConfig` value and the per-path
work an injected `InspectFn`, so the tests drive both phases with millisecond budgets and no hung mount.

**The `unanswered` contract.** `shape_ok` runs `fit_to_result_budget` over the rows that exist, then names every
requested path with no row in the kept prefix: cut by the size ceiling, never launched, or abandoned without a row.
`total` is the paths asked, `returned` the rows carried, `truncated` is `returned < total`. The model joins rows back
by `path` and asks again for exactly `unanswered`.

**Text-only by construction**: no field on any row can hold bytes; `tests::every_row_shape_is_text_only_no_byte_fields`
walks the serialized result and requires every leaf to be a string, a number, or a flag.

## One tool, both questions (`list_dir`)

"What's in this folder" and "where is my disk space going" are one query with two orderings, so they're one tool. A
second by-size tool would have overlapped it on every axis but the sort, and an agent facing two near-identical listing
tools guesses.

Three properties make the size ordering an honest disk-usage answer:

- **Files and folders rank together.** A folder's `size` is its RECURSIVE total from `dir_stats`, not its inode size, so
  it's comparable with a file's. A folders-only ranking hides the case that motivated this: a single ~900 GB sparse
  VM disk image outweighing every folder on the volume.
- **Unknown sorts last, both directions.** A folder with no `dir_stats` row is unknown, not empty. Leading a `desc`
  ranking with it claims "biggest"; leading `asc` claims "smallest". Both are claims the index can't back, so
  `compare_unknown_last` pins it to the tail either way.
- **The enrichment order follows the sort.** Ranking by size needs every child folder's size BEFORE the sort, so the
  `dir_stats_batch` covers the whole folder; any other order pages first and enriches only the surviving rows, so
  browsing a 20k-entry folder costs one small batch. Identical rows either way — only the lookup count differs.

`offset`/`limit` paging rides on a total order (the sort key, ties broken by name), which is what makes "resume with
`offset + returned`" safe: an unstable order would silently skip or double-count rows, and double-counting a folder is
double-counting its bytes.

## The honesty (coverage) contract

`read/listing.rs::coverage` is the single builder for index freshness honesty: it reuses `status_token` +
`Freshness::is_authoritative` (never re-derives the tokens) and attaches a plain-language note when a read isn't
authoritative or the path isn't indexed. `SizeStats::from_dir_stats` carries the exact-vs-lower-bound / stale / updating
/ has-symlinks flags verbatim from `DirStats`. Importance staleness is `asOfGeneration < recomputeGeneration`. These are
the flags spec §2.4 makes load-bearing; the system prompt requires the model to voice them.

**A caveat that lives only in a sibling flag is a caveat the model can shed.** A flag is honest only while the reader
carries it alongside the number, and an agent restating "1.8 TB" has already dropped `sizeIsLowerBound: true`. So the
qualifier rides INSIDE the human string (next section), and the flag stays for anything that branches on it.

## Numbers arrive already spoken

The Ask Cmdr agent can't run a script, so every number it might state out loud is formatted before it crosses the wire:
raw bytes and epochs make a model do arithmetic it does unreliably. Each raw field keeps a formatted twin — never one
instead of the other, since the raw value is what anything downstream computes with:

- `ChildEntry.sizeHuman` / `modifiedHuman`, present exactly when `size` / `modified` are. An unknown size gets NO
  string: a `"0 B"` would read as an empty folder, which is the wrong-zero the whole coverage contract exists to
  prevent.
- `SizeStats.recursiveSizeHuman` (always present, the folder's own total).
- `VolumeBlock.totalHuman` / `availableHuman`, present exactly when their byte counterparts are.
- `ListDirResult.remainder` (below).

**Uncertainty is inside the string.** `≥ 1.8 TB` when the number can only be higher (a lower bound), `~ 40 GB` when the
error runs in both directions. `human_size` / `qualified_size` (`read/listing.rs`) are the one place that decides;
`ChildEntry::new` / `set_size` are the only ways to set a size, so the number and its string can't drift apart.

**One formatter, `search::format_size` + `format_timestamp`.** ❌ Never a second one: two would round differently and
the same folder would read two sizes across two surfaces. Like the `search` results table, this path does NOT consult
the user's SI-vs-binary units setting; MCP/agent output stays internally consistent instead of tracking a UI preference.

### The remainder: what the page didn't show

A paged listing can say what the rows it left out add up to: `{ count, bytes, human, isApproximate }`, where `count` is
`total - returned` and `bytes` is the folder's own `recursiveSize` minus the sizes on this page (saturating). Without
it, "the other 3,000 files" is a number the model can only invent.

`isApproximate`, deliberately not `isLowerBound`: the bounds run in BOTH directions. An understated folder total pulls
the remainder down, an understated child size pushes it up, and both can be in play at once, so naming a direction
would be false precision. Its string wears `~`, never `≥`.

**Omitted rather than guessed** (`remainder()`), because the model would state a wrong one as fact:

- `count == 0` — this page is the whole folder; a zero would only invite interpretation.
- Any returned child has an unknown size. It's missing from the subtraction, so the difference silently absorbs it.
- No `dir_stats` total for the folder to subtract from.
- A `type` filter is active: `count` would be "folders not shown" while the recursive total still counts every loose
  file, so the pair would describe two populations in one sentence ("the other 3 folders come to 40 GB", where 38 GB of
  it is files that were never in the running).

Rejected: a `summary` sentence field. A model reuses such a string verbatim, which makes it user-facing copy needing
review in every state (scanning, unindexed, lower-bound, empty) — and Cmdr's rule is that human-facing text is
human-written (`AGENTS.md` § Principles, "Humans to humans").

## The size contract: a result never outgrows the caller's context

Every result that carries a LIST is cut to `agent::chat::budget::MAX_TOOL_RESULT_TOKENS` through
`mcp::executor::fit_to_result_budget`, and reports `total` / `returned` / `truncated` so the model can say what it saw
and ask for the rest. It applies to `list_dir` (children, under the caller's own `limit`), `list_pane_files` (entries, on
top of its 200-row cap), `image_facts` (per-path rows, on top of the 2,000-char per-file text cap),
`search_photos` (hits), `inspect_file` (per-path rows, on top of the 16,000-char per-row window cap, with the rows it
drops named in `unanswered`), the `operations_*` pages, and the suggested-ops reads (group summaries from
`list_suggestions`, the op page from `get_suggestion_group`).

**Why a size cut on top of the row caps:** a row cap can't bound a payload. `image_facts` at 200 paths × 2,000
characters is ~100k estimated tokens, and a `list_dir` on a 20k-entry folder had no cap at all. A result that doesn't
fit doesn't just get itself dropped — it pushes the rest of the turn out of the prompt, which is how a bulk-rename turn
lost the very evidence it was reasoning from (`agent/chat/DETAILS.md` § Budget enforcement). The ceiling is derived from
the CONSERVATIVE default prompt budget, not the resolved model, because a handler doesn't know the model and may be
answering an external MCP client.

The cut is never silent (`image_facts`'s founding principle, now the rule for every tool): the counts cross the wire,
and the system prompt requires the model to disclose "returned of total" whenever `truncated: true`.

## The dispatch gate

`view.rs::refuse_unavailable(call_id, tool)` is the runtime enforcement point:

- `ToolId::Unrecognized(_)` (any non-view name — a hallucinated `delete`, a typo) ⇒ a typed `{ available: false, … }`
  result, returned BEFORE `execute_tool`. The parse (`ToolId::from_wire_name`) is the choke point.
- A known name the registry classifies `Access::Write`, or doesn't classify at all ⇒ also refused (a runtime backstop
  against a mis-tagged entry; belt to the structural `test_agent_tool_view_never_writes` suspenders).
- Otherwise `None` ⇒ `dispatch` calls `execute_tool(app, Consumer::Agent, …)`, which itself refuses any name outside the
  agent view (a second, structural backstop).

The access half lives in the pure `access_is_dispatchable(Option<Access>) -> bool`: `Read`, `Propose`, and `Memory`
dispatch, `Write` and an unclassified name don't. It's separate so the rule is unit-testable against EVERY `Access` variant
without authoring a tool per variant — with zero `Propose` tools in the registry, a name-driven test would cover the
`Propose` arm vacuously, and the widened gate would go unexercised until some future commit.

The negative test (`view.rs`) drives the fake `AgentLlm`'s `CallRawTool("delete", …)` and asserts the refusal end to
end; it was proven red (gate disabled ⇒ "delete" not refused) before green.

The refusal copy says Ask Cmdr can prepare a rename plan, suggest file operations for the user to review, and save
notes in its own memory folder, but can't touch the user's files, approve a proposal, or read file contents. ⚠️ Keep it
accurate as the tiers grow: it used to promise the agent couldn't change anything at all, which `Access::Memory` made
false.

`dispatch` routes two tools specially rather than through the generic `execute_tool` call: `propose_rename_plan`,
which needs the evidence scope, and `propose_suggestions`, which needs the conversation id so a sweep records the
thread it came out of (`suggestions/DETAILS.md` § The conversation link). Both are gated first, like every other call.

`dispatch` also feeds `propose::evidence::ImageFactsLedger` from every non-elided `image_facts` result. That's what makes
a rename plan's content claim checkable; the guardrail, its refusal shape, and the revocation seam are in
`propose/DETAILS.md`.

## Cross-module symbols the toolset reuses

- `indexing::read::queries::list_dir_children` — a path-based helper (re-exported from `crate::indexing`); the child-listing
  analog of `get_dir_stats`, wrapping the read-pool + `index_read_path` + `resolve_path` + `IndexStore::list_children_on`
  wiring so the tool stays path-based (it lives in `indexing`, its elegant home).
- `mcp::resources::volumes::VolumeKind::token` — `pub(crate)` so the volume mapper reuses the one kind→token mapping.
- `mcp` re-exports `Access`, `Consumer`, `agent_tool_view`, `execute_tool`, `tool_access`, `ToolError`, `ToolResult` as
  `pub(crate)` for the agent runtime. `snapshot_volumes` and the `importance::snapshot_*` functions are `pub(crate)` too.

## Not covered here (the runtime harness)

A full fake-driven dispatch of a REAL agent tool (success path through `execute_tool`) needs a Tauri app with managed
state (`PaneStateStore`, the index registry, a data dir). That app harness is the chat runtime's concern, so this layer
covers the success path with per-tool pure-shaper tests (fixtures for the coverage flags) and the refusal path in full
(no app needed). The dispatch entry point the runtime calls is `agent::tools::view::dispatch`; the declaration API is
`agent::tools::agent_tool_declarations`.
