# Tool lines — details

Must-knows: `CLAUDE.md`. The rail that renders these facts: `apps/desktop/src/lib/ask-cmdr/CLAUDE.md`. Origin: issue
#748.

## Why derived, not stored

Every fact here is a pure function of `AgentToolCall.arguments` and `AgentToolResult.content`, which `main.db` already
holds. So no column, no migration, and old threads get the same view as new ones. The live path and the history
projection call the same functions, which is what makes a live bubble and a reloaded one equal.

Elision doesn't get in the way: it's assembly-only (`chat/context.rs`), so the stored content is the full result the
model read in its own turn.

## `subject_of(call, result, memory_root)`

- `app_state` → `Panes`; `list_pane_files` → `FocusedPane`, read off its RESULT (it takes no arguments), both fields
  `None` until it returns or when it was refused.
- `list_dir`, `folder_importance` → `Folder { path }`; `inspect_file`, `image_facts` → `Files { firstPath, count }`.
- `search` → `Search { query, scopePaths }`: `pattern`, plus the scope's INCLUDE paths through the executor's own
  `search::parse_scope` (quotes, escaped commas, `~`), so the line names what the tool searched. Excludes are dropped:
  they aren't places. ❌ Never hand the raw scope string to navigation.
- `connect_volume` → `Volume { id }`, the `volumeId` it asked to connect.
- `search_photos` → `PhotoSearch { query }`; `operations_get` → `Operation { id }`; `propose_rename_plan` →
  `RenamePlan { count }` (rows in the plan); `propose_suggestions` → `SuggestionSweep { count }` (groups in the sweep);
  `propose_server` → `ServerSuggestion` (no name: it's the model's text, and the card shows it).
- `memory_write`, `memory_edit` → `MemoryFile { path, absolute }`. `path` is the model's, relative to the memory
  folder. `absolute` comes from `memory::shown_location`, the jail's LEXICAL checks only (inside, Markdown), joined onto
  `memory_root`: no syscall, no directory created. `None` (an escape, a non-Markdown name, or no root) keeps the path
  plain text.
- `Unrecognized(name)` → `UnknownTool { name }`; a legacy nameless call is `Unrecognized("")`.
- Arguments missing what a tool needs → `Unparsed`. Paths are `~`-expanded with `read::expand_tilde`. A place's
  `volumeId` argument is not part of the subject: the path is an app path the panes know either way.
- **A result's place is `location.path`** (the location contract, `../DETAILS.md` § The location contract), read
  through `mod.rs::location_path`, which falls back to the bare `path` a thread stored before results carried a
  `location`, so an old thread still names its folder.
- **Every subject path is a `LinePath { text, linkable }`** (`LinePath::of`): `linkable` only for an absolute path
  `bounded` left whole. A relative path (the model's own words) resolves against nothing the panes know, and a cut one
  ends in `…`, so both keep their text and lose the link. A preview row's `path` (its link) and `MemoryFile.absolute`
  go through `LinePath::linkable_text`: set only for such a path, the row's `name` carrying the text either way.

**Decision/Why `memory_root` is a parameter**: the root is `<data-dir>/ai/memory/`, which only an `AppHandle` knows.
Taking it as a value keeps the function pure; a caller with no root passes `None` and gets a plain-text path.

## `outcome_of(tool, result)`: the check order

1. `elided` → `Answered` (a stub carries none of the result's keys). Defensive: no code path has ever persisted
   `elided: true` (verified 2026-10-26 with `git log -S 'elided: true'` plus the store writers: only
   `chat/context.rs`'s assembled copy and a views test set it), so there's no `SetAside` outcome; the preview's
   `setAside` flag covers the case.
2. `available: false` → `Refused(ToolUnavailable)`, the view gate (a hallucinated tool).
3. `repeatedCall: true` → `Refused(RepeatedCall)`. The breaker KEEPS the original `problem`, so this must win.
4. A propose tool with `readyForReview` → `Staged { count }` or `NotStaged`. `ensure_review_verdict` stamps `false` onto
   every propose problem, so a propose problem is `NotStaged`, its sentence still in the preview. A `propose_suggestions`
   location refusal carries the read tools' `miss` instead of a `problem` (`suggestions/DETAILS.md` § Where a proposal
   may point), so it reads as `LocationMiss { miss }`: still not staged, and the line says why.
5. `problem` → `Refused(Problem)`: the schema gate and a handler problem look the same, and we won't add a key.
6. `refused: <token>` → `Refused(Memory { refusal })` via `MemoryRefusalKind::from_token` (unknown → `Other`).
7. `miss: <token>` → `LocationMiss { miss }`, the location contract's typed miss (`location/miss.rs::MissToken`), on
   any tool that takes a place. A token this build doesn't know falls through to the success shapes, so to `Answered`.
8. The per-tool success shape, else `Answered`.

Per-tool success shapes:

- The paged lists (`list_dir`, `list_pane_files`, `inspect_file`, `operations_list`, `list_suggestions`,
  `get_suggestion_group`, `image_facts`) → `Items { returned, total }`. `important_folders` has no total: `total` is its
  count only when not `truncated`. `list_volumes` returns every volume, so `total = returned`. A capped live
  `list_dir` read sets `totalIsLowerBound`, which makes the count a `lowerBound`.
- `search` → `Matches { returned, matchCount, lowerBound }`, or `NothingFound { lowerBound }` at zero. `lowerBound` is
  `!coverage.complete || coverage.capped`, the same rule that puts the `≥` on `matchCountHuman`
  (`mcp/executor/DETAILS.md` § The search result). `countOnly` gives `returned: 0` with a real count.
- `search_photos` → the same pair over its `total`, a lower bound when any volume is `incomplete` or `indexing`.
- `imageIndexingOff` / `semanticModelNotInstalled` (both photo tools) → `IndexUnavailable { reason }`, which is ok:
  the tool answered honestly that the index can't.
- `get_suggestion_group` with `found: false` → `NothingFound`.
- `propose_suggestions` staged → `Staged { count }` summing its groups' `opCount`; the rename plan reads its `count`;
  `propose_server` counts its card (its group has no ops), `0` when the user answered it first.
  So `count` is OPERATIONS staged for both.
- Memory `saved: true` → `Saved`.
- `connect_volume` → `Connection { outcome }`, its own `outcome` token (`connect.rs::ConnectOutcome`).

**`ok()`**: `Refused`, `NotStaged`, `LocationMiss`, and a `Connection` other than `connected` / `alreadyConnected` are
not ok. It agrees with `dispatch_ok` except on typed answers that carry no `problem` key, which `dispatch_ok` reads as
ok: a memory refusal (the user should see a note wasn't saved), a rename plan whose evidence was rejected, a location
miss, and a connect that didn't connect. A test pins both directions. `dispatch_ok` itself (repeat breaker, analytics)
doesn't change.

## `preview_of(tool, result)`

The expanded line. `rowsGivenToModel` / `total` / `lowerBound` come from `outcome_of`, so the preview and the outcome
can't disagree. Rows are a prefix of what the model received, at most `PREVIEW_ROWS` (20), so a preview never claims
the model saw rows it didn't.

- Rows: a listing's children (path joined onto the result's folder), search entries, photo hits (`snippet` = the OCR
  match), `image_facts` (`snippet` = recognized text, `status: notIndexed`), important folders, volumes (`path` =
  `mountPath`, absent for a phone), suggestion groups' display names, a group's ops (`snippet` = the new name).
- `inspect_file`: one row per file, or one per `find` hit (`line` + `snippet`; a PDF hit's `line` counts within its
  page, so it's dropped). A path that didn't read carries `status` (`missing`, `noVolumeAtPath`, `unreadable`,
  `unreachable`, `unsupportedVolume`, `protected`); a folder is `kind: folder`. A location miss has no list, so no rows.
- No rows for tools with no list (`app_state`, `folder_importance`, `operations_*`, memory, propose, quiet,
  `connect_volume`). An
  operation row carries ids and tokens, nothing a person reads, so `operations_list` shows counts only.
- `problem`: the verbatim sentence, cut at 2,000 chars. `setAside`: the stored row is an elided stub; no rows then.

## `repeat_marks(calls)`

For one turn's calls in order, each identical call (`AgentToolCall::identity_key`, the repeat breaker's key) maps to
the FIRST identical call of the turn. Keyed by `LineRef`, so two rounds both minting `fake-call-0` stay apart. A row
naming one call id twice can't tell its two lines apart, so it never marks a line as its own repeat.
