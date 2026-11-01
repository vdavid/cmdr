# Tool actions details

Depth for the handlers shared with outside clients. `CLAUDE.md` holds the must-knows.

## Tools by category file

- **`app.rs`**: `switch_pane`, `swap_panes`, `tab` (unified action verb).
- **`quit.rs`**: the `quit` tool, plus the two answers to the quit confirmation that `dialogs.rs` routes here
  (`confirm` / `close` on `quit-confirmation`). Everything goes through `crate::quit`'s gate, the same one ⌘Q uses: a
  straight `app.exit(0)` here killed a running transfer with no prompt and no warning, where a person pressing ⌘Q gets
  a dialog and 15 seconds, and an agent must not have a quieter, more destructive exit than the keyboard. Adapter
  shape, like `queue.rs` and `conflicts.rs`: no FE action to dispatch, so no ack to invent, and the verdict is read
  straight out of the gate rather than hoped for. `quit` answers a typed `outcome`: `quitting` (nothing to lose, the
  app is going) or `held` (the gate is asking; the reply carries the operations holding it, `countdownMs`, and the two
  calls that answer it). `confirm` answers `quitting`, `close` answers `kept_working` plus a typed `promptClosed` (it waits for the dialog to
  go, but a wedged webview costs that flag, ❌ never the outcome: the gate already took the answer, and a refusal would
  send the caller back to `quit` over an answer that landed), and either is a refusal carrying
  `data.outcome: "no_quit_pending"` when the gate had nothing pending — the deadline claimed the decision, or another
  surface answered first. **A held quit nobody answers ends with the app quitting anyway** at the countdown, stopping
  those operations; the reply says so, and the trade is deliberate (`../../quit/DETAILS.md` § "Answering from outside
  the dialog").
- **`view.rs`**: `toggle_hidden`, `set_view_mode`, `sort`.
- **`nav.rs`**: `nav_to_path`, `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`, `select_volume`, `move_cursor`,
  `open_under_cursor`.
- **`file_ops.rs`**: `copy`, `move`, `delete`, `mkdir`, `mkfile`, `refresh`, `select`.
- **`dialogs.rs`**: unified `dialog` tool: open / focus / close / confirm for settings, file-viewer, about, and
  confirmation dialogs. `quit-confirmation` is the one type whose `confirm` / `close` it delegates (to `quit.rs`), and
  the one it refuses to `open`: only the gate raises it.
- **`async_tools.rs`**: `await`, `connect_to_server`, `remove_manual_server`, `upgrade_smb_to_direct`, `set_setting`.
- **`search.rs`**: `search` and `ai_search` (LLM-driven), both a thin wrapper on `search::run_live_collected` — the
  SAME live run the dialog starts, walking whatever the index doesn't cover, folded into one reply because a tool call
  can't carry a stream (`search/DETAILS.md` § "Decision 10"). ❌ No walk-versus-don't parameter. `maxWaitSeconds` is a
  transport budget only: when it runs out the reply carries what had arrived plus a typed note, and the walk keeps
  going. `limit` clamps to the house `MAX_LIMIT` of 200. `search` is shared `[AiClient, Agent]`; ❌ `ai_search` stays
  ai-client-only, because Ask Cmdr is already an LLM holding the user's prose and nesting a second model call to write
  a query it can write itself buys two providers and a translation it can't see (`../DETAILS.md` § The
  tool catalog). The result shape and every rule about it live in `search/result.rs` (§ The search result, below).
  Where `search` covers (`volumeId`, a scope under a dead mount) is `search/placement.rs`'s, through the one location
  resolver (`../DETAILS.md` § The location contract); `ai_search` routes by its scope only.
- **`queue.rs`**: the `queue` tool (pause / resume / cancel one id, pause_all / resume_all, `continue_without_gone`,
  the answer to a `pendingSourcesGone` question, and `move_locked` / `skip_locked`, the answers to a
  `pendingLockedSources` one). Thin adapter over the manager: no FE action, so no ack. Only a `Continued` or
  `Answered` outcome earns an `OK`.
- **`conflicts.rs`**: `resolve_conflict` — answers ONE Stop-mode clash a running operation is parked on. Same adapter
  shape as `queue.rs`, over `Operations::resolve_write_conflict`, and the whole point of it is reporting the
  ARBITRATION honestly: `Resolved` / `AlreadyResolved` answer `OK` (the clash is settled either way), `StaleAnswer` /
  `NoPendingConflict` / `UnknownOperation` are refusals, and every one of them crosses the wire as a typed `outcome`
  field or `data.outcome`, never as prose an agent would have to parse. `stop` is rejected as a resolution: it is the
  policy that RAISES the question. Discovery is the `pendingConflict:` block in `cmdr://state` under `operations:`
  (`resources/operations.rs`), which is also the only place the `conflictId` an answer must carry comes from.
- **`unlock.rs`**: `unlock` — answers the password prompt of a locked archive or restic repo. Same shape as
  `conflicts.rs`: the answer must NAME what it answers (`path`, off the `archive-password` entry in
  `cmdr://state` `dialogs:`), and `no_password_prompt` / `different_container` are refusals carrying a typed
  `data.outcome` rather than prose. It hands the password over through `commands::file_system::unlock_within` (the
  same seam the dialog uses), reports the container's typed answer (a repo's `wrong_password` keeps nothing and stops
  there), and otherwise emits a payload-free `mcp-confirm-dialog` so the frontend does the mode's
  follow-up; the secret never crosses into the webview. ❌ **It must never dispatch an operation.** `browse` answers
  `retrying_listing` (a listing is a read, so the unlock finishes it); `transfer` answers `password_stored` and stops
  there, because starting the extraction is a write and goes through `copy` / `move` like every other one. Full
  rationale, including why the parked operation is already settled and why the gate is `Always`:
  § "Answering the password prompt".
- **`downloads.rs`**: `go_to_latest_download` (resolves via `downloads::commands::go_to_latest_download`, then
  `mcp-nav-to-path` + `mcp-move-cursor`).
- **`operation_log.rs`**: `operations_list`, `operations_get` (short-lived read-only connection over the query API,
  the `commands/operation_log.rs` pattern), `operations_rollback` (dispatches the rollback engine via
  `Operations::dispatch_rollback`; returns after dispatch — see § `operations_rollback`: dispatch-then-poll).
  The pure filter/param parsers and the typed-refusal shape are unit-tested in `operation_log/tests.rs`. Both read
  responses go through `fit_to_result_budget` and carry `returned` / `total` / `truncated` (`operations_list` keeps its
  original `count`, equal to `returned`), so a page cut for size is visible and resumable with `offset`.
- **`photos.rs`**: `search_photos` (shared `[AiClient, Agent]` read). Shapes the `media_index` read API
  (`search_semantic` / `search_ocr` / `images_with_tag`) into a TEXT-ONLY DTO (no image bytes), resolves the mode like
  the search UI (Auto composes semantic + OCR, degrades to OCR with no CLIP model), reuses `media_index::commands::volume_state`
  for coverage honesty, and returns a typed status (`imageIndexingOff` / `semanticModelNotInstalled` / `ok`). Pure mode
  resolution, hit merging, coverage derivation, and the no-bytes property are unit-tested in-file.
- **`image_facts.rs`**: `image_facts` (shared `[AiClient, Agent]` read), the LOOKUP direction of the same index that
  `search_photos` queries: the caller already has the paths and needs to know what's IN each image (a natural-language
  bulk rename). Shapes `MediaIndex::facts_for_paths` into the same kind of TEXT-ONLY DTO, and imports `photos.rs`'s
  `resolve_search_volumes` / `derive_coverage` / `build_note` rather than re-deriving them, so the two tools can't drift
  on volume resolution or coverage honesty. Per-path `state` is a typed `indexed` / `notIndexed`, never an absent
  field the caller has to sniff for. Bounded twice: at most 200 paths (over that is `INVALID_PARAMS`, never a silent
  cut) and 2,000 characters of text per file (a cut sets `textTruncated`). Params parse, truncation, the
  first-volume-wins merge, and the no-bytes property are unit-tested in-file.
- **`memory.rs`**: `memory_diagnostics`, the one tool that answers a question about Cmdr itself rather than the user's
  files. Adapter shape like `queue.rs`: it dispatches no FE action, invents no ack, and passes
  `commands::memory_diagnostics::get_memory_diagnostics` straight through. It exists so the reading can be taken from
  OUTSIDE the app — the command ships in release builds precisely because the interesting numbers only appear in one
  under a real workload, and until this tool nothing could call it there. `sizesPerTag` defaults to 8 (the command
  clamps at 24). macOS only: elsewhere it refuses rather than returning a payload of zeros a reader would take for a
  measurement. How to read the payload: `../../commands/memory_diagnostics.rs` module docs and
  `docs/tooling/memory-debugging.md`.
- **`multi_rename.rs`**: `multi_rename`, the Multi-Rename Tool (⌃M) for an external agent, `Access::Propose`. Every
  sheet knob is an optional param under `MultiRenameSpec`'s own camelCase name (a test pins the schema declares each).
  Files come from `resolve_pane_target_paths` (`names`, else the selection, else the cursor item); the preview is the
  sheet's own `multi_rename::plan::preview_names` against the pane's cached listing, so a status here is the sheet's.
  A mask with a `[=source.field]` reads the files' EXIF and tags inline (`read_media`) through the sheet's own
  `SessionMedia` cache and loader (`multi_rename/DETAILS.md` § Media fields), seeded from the same listing entries the
  preview runs on, so values, `NoValue` / `Unreadable`, and the `|modified` fallback are the sheet's. **Decision:** an
  open sheet over the same folder lends what it already read (`session::media_over` + `SessionMedia::adopt`, matched
  by listing size and mtime); the rest is read before answering, at most `MAX_MEDIA_READS` (2,000) files left to read,
  else the typed `data.specError: { type: 'tooManyMediaFiles', files, max }` before any read. **Why:** a dry run on S3
  or MTP at a few files a second must still answer in minutes, and the sheet reads bigger batches in the background
  with progress. A dropped call (the client hung up) cancels the read (`CancelOnDrop`). No progress is reported.
  `dryRun` (default true) pages the rows with `start` / `limit` (max 200, then `fit_to_result_budget`) and an
  `all` / `problems` filter, with counts over every row. `dryRun: false` stages ONLY the ready rows as one rename group
  on the suggested-ops spine (`agent::suggested_ops::propose`, no conversation), each op carrying its listing size,
  mtime, and inode for the bridge's drift check; nothing ready or more than 10,000 ready is a typed refusal
  (`data.reason`). Paging params beside a proposal are refused, since they'd read as proposing a slice. It never
  renames: the user approves the group in the suggested-ops dialog. ai-client only for now; Ask Cmdr proposes renames
  through `propose_suggestions`.
- **`cpu.rs`**: `cpu_diagnostics`, `memory_diagnostics`' CPU sibling, over `cmdr_fs::process_cpu` (the FFI and the
  diff live there). `measure` (default) blocks for `windowSeconds` (default 10, max 60, the `await` tool's ceiling);
  `mark` stores one process-wide baseline and `sinceMark` diffs against it without blocking, so an idle census of
  20 minutes never holds an MCP call open. The mark stays put, so repeated reads are cumulative. macOS only, refusing
  elsewhere like its sibling. ai-client only.
- **`tests.rs`**: unit tests for the dispatcher and shared helpers; per-category tests live alongside their handlers.

## The search result

`search/result.rs` folds a `LiveAnswer` into the ONE typed JSON shape both search tools answer with. It is pure, so
every rule below is unit-tested against fabricated answers with no harness and no running search.

```
{ targetVolumeId, matchCount, matchCountHuman, returned, truncated,
  entries: [{ name, path, parentPath, isDirectory, sizeBytes, sizeHuman, modified, modifiedHuman }],
  coverage: { complete, stillWalking, foldersFound, capped, hiddenByExcludes,
              permissionDenied[], declined[], stillCovering[], unresolvedScopes[],
              abandonedGround, abandonedLocations },
  notes: [ "…" ] }
```

`ai_search` returns the same object with `interpretedQuery` flattened on top, and the translator's caveat as the first
note.

A `search` whose place doesn't resolve answers the location contract's miss INSTEAD of this object
(`{ miss, location, detail, nearby?, … }`, `noVolumeAtPath` / `unknownVolume` / `pathOutsideVolume` /
`volumeNotConnected`), because no search ran: a caller branches on `miss` being present. `unresolvedScopes` keeps its
meaning (a scope on a volume that resolved, but with no row for that path). `targetVolumeId` is the INDEX that answered,
which for a cloud drive named by its own `volumeId` is `root`. An answer for a volume that isn't connected leads
`notes` with when its index was last walked.

- **`matchCount` is ❌ NOT the `total` the paged tools report.** There, `total` is what the page was cut from, so
  `returned == total` means "you saw everything". Here the engine stops emitting rows at the row cap while the count
  keeps rising, so `matchCount` can exceed `returned` by orders of magnitude with nothing wrong; `coverage.capped` says
  which. `entries` goes through `fit_to_result_budget` on top of the 200-row cap, and `returned` / `truncated` ride out
  with it.
- **`coverage.complete` is the one field to read before saying "that's all of them"**: settled, walk finished (or
  nothing to walk), and `permissionDenied` / `declined` / `stillCovering` / `unresolvedScopes` / `abandonedGround` all
  clear. It exists because a seven-way conjunction is one a model gets wrong once in ten. The seven stay beside it,
  because each one is a different sentence to the user.
- **`capped` and `hiddenByExcludes` deliberately don't clear `complete`.** Neither is ground Cmdr failed to cover: the
  first stopped the rows and not the count, the second is the caller's own filter. Both still make the number a floor,
  which is why `matchCountHuman` wears `≥` for the first and the second always gets a note.
- **The uncertainty rides INSIDE `matchCountHuman`** (`≥ 1,240 matches` versus `1,240 matches`), because a flag a
  sibling field carries is a flag the model sheds the moment it restates the number. It is `≥` whenever `complete` is
  false or the cap was hit.
- **`sizeHuman` / `modifiedHuman` come from `search::format_size` / `format_timestamp`**, the dialog's own pair.
  ❌ Never a second formatter. `iconId` is dropped: a model can't render an icon. An absent size stays absent, ❌ never
  `0` (a NULL logical size is a hardlink-deduped row).
- **`notes` keeps the authored prose beside the typed flags**, because one line is genuinely actionable copy no flag
  replaces ("granting Cmdr Full Disk Access … opens them"). Same pattern as `SearchPhotosResult::ImageIndexingOff`'s
  `note`. ❌ Not a `summary` field: it never restates what the fields already carry.

## Ack contract

Each action tool: (1) captures a precondition snapshot (typically `snapshot_generation(app)`); (2) emits its event /
runs its command; (3) calls `wait_for_ack(app, signal, DEFAULT_ACK_TIMEOUT)` (default 1500 ms; nav family uses
`NAV_ACK_TIMEOUT` = 5 s); (4) returns `OK` on signal, or `ToolError::outcome_unknown` naming the missing signal and elapsed
budget on timeout.

`AckSignal` variants, when they fire, and who uses them:

- **`GenerationAdvanced`**: fires when `PaneStateStore.generation` is strictly greater than the captured value. Used by
  pane mutators: `set_view_mode`, `sort`, `toggle_hidden`, `tab`, `nav_*`, and auto-confirmed `copy`/`move`/`delete`.
  NOT `select`/`refresh` (both round-trips), and NOT `dialog confirm` (below).
- **`SoftDialogAppeared(id)`**: fires when a soft dialog with that id is in `SoftDialogTracker`. Used by confirmation
  dialogs from `copy`/`move`/`delete` (`autoConfirm: false`), `mkdir`, `mkfile`, and `dialog open about`.
- **`SoftDialogDisappeared(id)`**: fires when a soft dialog with that id is no longer tracked. Used by
  `dialog close <confirmation>` (the FE `ModalDialog` fires `notifyDialogClosed` on unmount), and by
  `dialog confirm <transfer|delete>`, AFTER its round-trip: the press carries the request id into the start, which
  answers with the spawned `operationId` (as an auto-confirmed `copy` does), and the FE takes the dialog down in the
  same tick it starts the operation. ❌ Don't ack a confirm on `GenerationAdvanced`: a compress, or a copy onto a slow
  volume, changes nothing in either pane until its first file lands, so the tool answered "not acknowledged" about an
  operation that had started (a 300 MB compress onto a phone, 2026-09-30). ❗ This signal is also true of a dialog that
  was never open, so `confirm_and_await_start` checks the tracker FIRST and refuses a confirm of nothing with
  `invalid_params`. A press the dialog's own guard stops (a refused source, a press already in flight, a cloud trash
  handed back) answers `refusal: "notStarted"` at once, typed, and the dialog stays up.
- **`WindowAppeared(label)`**: fires when a `webview_windows()` entry matches (exact, or `viewer-*`). Used by
  `dialog open settings|file-viewer` and `dialog focus file-viewer`. `dialog focus settings` needs no ack: the backend
  raises that window itself (`set_focus`), and a closed one is `invalid_params`.
- **`WindowDisappeared(label)`**: fires when the matching `webview_windows()` entry is gone. Used by
  `dialog close settings` (single-window family).
- **`WindowCountBelow {prefix, threshold}`**: fires when the matching window count is `< threshold`. Used by
  `dialog close file-viewer` (snapshot count, ack when one closes; don't wait for all viewers to vanish).
- **`ArchivePromptAdvanced {from}`**: fires when the archive-password prompt mirror
  (`ui_state/archive_password.rs`) moves past `from` — the frontend either took the prompt down or put a new one up. Used by
  `unlock`, and ❌ never `SoftDialogDisappeared` for it: a browse unlock re-lists at once, and a wrong password
  raises the prompt again fast enough that the dialog may never unmount, so a "closed" wait would spend its whole budget
  on a flow that worked. A generation moves whichever way it went.
- **`Any([...])`**: fires on a logical OR over inner signals. Reserved for multi-mode tools.

Polling cadence: 250 ms for state-driven signals (matches the `await` tool); 100 ms for window/soft-dialog signals (both
react faster than a full pane state push).

## `mcp_round_trip` for explicit FE responses

When the backend can't fully validate preconditions (or has to wait on the OS), the tool emits an event with a
`requestId` and waits for the FE to reply via `mcp-response` carrying `{ requestId, ok, error? }`. One helper,
`mcp_round_trip_parsed`, owns the id + listener + timeout for all of them; each caller brings the parser that says what
its reply is allowed to mean (`parse_mcp_response`, `parse_operation_start_response`, `parse_nav_response` in `mod.rs`,
`parse_tab_move_response` in `app.rs`, all pure and unit-tested). Per-tool:

- **Holds.** A reply of `{ requestId, holdMs }` instead of an answer moves that round-trip's deadline
  to `holdMs` from now plus its usual budget (capped at `MAX_ROUND_TRIP_HOLD`, 300 s), and the answer still ends the
  wait the moment it lands. For a frontend that knows its answer will be slow on purpose, so a silent one keeps timing
  out on the short budget. Its one sender today is the delete dialog waiting on a cloud folder's walk before it knows
  whether a press may trash (`holdOperationStartReply` in `transfer-dispatch.ts`;
  `src/lib/file-operations/delete/DETAILS.md` § Cloud storage), which covers both `dialog confirm` and an auto-confirmed
  `delete`. A transfer has no such phase: its press waits only on the scan-preview IPC, which mints an id and returns.
  The hold is checked BEFORE the caller's parser, which would read a reply with no `ok` as a failure.

- `move_cursor`, `set_setting` (5 s). The FE verifies the cursor actually landed (filename found, index in range), then
  (move_cursor) flushes the MCP state push (`syncStateToMcpNow`) before replying, so a follow-up `copy`/`move`/`delete`
  reads the new cursor instead of the stale pre-move one. A silent no-op (cursor never moved) was the original
  false-positive-OK bug.
- `select` (5 s, all modes): the FE applies the selection (names mode maps names → indices via the `findFileIndices`
  batch IPC first), then flushes the state push before replying, so a follow-up `copy` reads fresh selection state.
  Missing names come back as the round-trip error.
- **A stalled folder answers early, wherever a tool waits on a LISTING.** `nav_to_path` and `select_volume` (above), and
  `await`'s row conditions (`has_item`, `not_has_item`, `item_count_*`), which answer `folderStalled` the moment the
  pane pushes `listing: stalled`: a stalled pane holds no rows, so "not there" would be a lie and "there" would wait out
  the timeout. Its path conditions still read the path, which stays true. `nav_to_parent` / `nav_back` / `nav_forward`
  ack on the first state push and `open_under_cursor` on its own 5 s round trip, neither on the listing, and both
  budgets end before `StallPolicy::stall_after` (8 s) could report a stall, so they have nothing to answer early.
- `refresh` (5 s): the FE forces a backend re-read via `refreshListing(listingId, true)`, which bypasses the
  watcher-backed short-circuit, so `OK` means the directory was actually re-read on every volume. In the network
  browser the same command re-scans hosts instead.
- `nav_to_path` (30 s, `mcp_nav_round_trip`): the reply carries a typed `outcome` plus the pane's resting location, and
  `nav_result` (in `nav.rs`) words the tool result from that discriminant — `navigated` is the only `OK`; `fell-back`
  and `did-not-settle` are errors naming both the request and where the pane actually is, and `stalled` (the folder's
  server or drive stopped answering, reported the moment the pane shows it rather than after the budget) is an error
  carrying `data: { reason: "folderStalled", path }` so an agent can branch on it. The FE holds the response
  until the pane comes to rest, which for a cross-volume switch is well past `settled` (that arm resolves it on the
  optimistic commit, before the new volume lists anything — the last false-positive `OK`), then flushes the state push,
  so `cmdr://state` read right after shows the landing. `go_to_latest_download`
  rides the same helper for its navigation leg, so it can't move a cursor in a directory the pane never reached.
- `select_volume` (30 s, the same helper on `mcp-volume-select`): the FE holds its reply until the switch's
  remembered-folder correction has landed and the pane has come to rest (or stalled), and `select_volume_result` words
  the same four outcomes, its `OK` naming the folder the pane opened. Resolve the completed volume rows by stable `volumeId`;
  a legacy name is accepted only when unique. A `navigated` reply is followed by a short `volume_name` poll so
  `cmdr://state` agrees. The request id reaches the FE through the command bus (`volume.selectByName`'s
  `mcpRequestId`). The bus lets MCP through behind an open dialog, so the select runs there; only the tools that start
  a file operation refuse (`refuse_while_dialog_blocks`). Why: `apps/desktop/src/routes/(main)/DETAILS.md` § The
  dialog gate.
- `tab` with `action: move` (5 s, on `mcp-tab` like the other tab actions, which stay on the generation ack): a move
  can be refused, and the frontend owns the rules, so the reply carries a typed `outcome` that `parse_tab_move_response`
  reads into `TabMoveAck` and `tab_move_result` words (both in `app.rs`). A refusal is an `invalid_params` error whose
  `data.reason` is `tabPinned` / `onlyTab` / `tabLimitReached` / `tabNotFound`; `unchanged` is an `OK`. The FE flushes
  both panes' tab lists before replying, so `cmdr://state` is current when the tool returns. The backend checks only
  what it can see without the rules (the tab exists in `pane`, and at least one of `toPane` / `toIndex` was given).
  Rules: `apps/desktop/src/lib/file-explorer/tabs/DETAILS.md` § Moving a tab.
- `open_under_cursor`: 5 s via `mcp_round_trip_with_timeout`; opening a file delegates to the OS default app, so neither
  `GenerationAdvanced` nor `WindowAppeared` would fire.
- Resources that need FE data use `resource_round_trip` (same pattern, returns the `data` field). Used by
  `cmdr://settings`.

## Agent-supplied paths

`user_path_param(params, key)` for a required path param (extract, missing-param error, tilde expansion);
`expand_user_path(s)` for optional or conditional sites (the `dialog` tool's optional `path`, the `await` tool's `value`
when path-shaped). Both in `mod.rs`. Virtual paths (`mtp://…`) don't start with `~`, so expansion is a no-op for them.

## Empty-operation fast-fail (`file_ops.rs`)

`empty_operation_error` (pure, unit-tested) mirrors the FE fallback semantics: a selection wins; no selection falls back
to the cursor file; cursor on `..` (or an empty pane) means the FE would silently drop the dialog, so the tool rejects
fast. Unsynced state (`path` empty) passes through. Without the `select` / `move_cursor` pre-reply flush, select → copy
reads a stale empty selection and move_cursor → copy reads a stale cursor (still on `..`), and either wrongly rejects
here.

"Empty pane" is `files` empty AND no actionable rows, where actionable is `total_files` minus the `..` row the pane's
`has_parent_row` declares. ❌ Don't go back to reading `total_files <= 1` as "only the parent": a search-results snapshot
pane and a pane at a volume root have no `..`, so one counted row there is one real file, and the shortcut refused a
delete over a file the user could see.

## The tool surface

42 semantic tools grouped by category:
- Navigation (6): `select_volume` (prefer `volumeId` from `cmdr://state`; legacy `name` works only when unique), `nav_to_path` (accepts absolute, `~`-relative, and `mtp://` paths; a scheme path and a path a ROUTE serves — inside a `.zip`, or inside a repo's virtual `.git` trees — both skip the local existence check, because `Path::exists()` answers a confident false for a path with no inode and would refuse every nav into a snapshot; ordinary local paths get a timed check — see `mod.rs::validate_path_exists`. An `smb://` path is refused by `navigate()`'s `smb-path-unsupported` arm: `resolve_location` maps every one of them onto the virtual `network` volume, whose only navigable path is the `smb://` host-list sentinel, so anything longer used to report success from the host list. A mounted share is its own volume, reached with `select_volume`), `nav_to_parent`, `nav_back`, `nav_forward`, `scroll_to`. The pane-targeting tools (`nav_to_path`, `scroll_to`, `select_volume`, `move_cursor`, `select`) focus the target pane in BOTH the backend store and the FE (the FE side is aligned in `mcp-listeners.ts` — see `apps/desktop/src/routes/(main)/DETAILS.md` § Focus follows the navigated pane), so a follow-up focused-pane op (`mkdir` / `copy` / `move`) acts on the pane just navigated/selected.
- Cursor/Selection (3): `move_cursor` (honest errors: a missing filename or out-of-range index is a round-trip failure, never a false OK), `open_under_cursor`, `select` (index ranges, `all`, `count: 0` to clear, or `names: [...]`; every mode is a round-trip that replies after the new selection landed in `PaneStateStore`, and the names mode errors listing any names not in the listing. Focuses the target pane — both the backend store AND the FE focused pane, so a follow-up focused-pane operation (copy/delete) acts on the pane you just selected in)
- File operations (7): `copy`, `move`, `delete`, `rename`, `mkdir`, `mkfile`, `refresh` (a round-trip that forces a backend re-read of the focused pane's listing on every volume, watcher-backed MTP/SMB included; the same command the user's ⌘R runs, so in the network browser it re-scans hosts). `copy`/`move`/`delete` fast-fail with the real cause when there's nothing to act on (no selection and the cursor is on `..`, or the pane shows no files) instead of a misleading ack timeout. `copy`/`move` accept optional `autoConfirm` (bool) and `onConflict` (`skip_all`|`overwrite_all`|`rename_all`). `onConflict` governs clashing **files only** — folders always merge (a source folder landing on a same-named dest folder merges into it; the policy then applies to the files inside). `delete` accepts optional `autoConfirm`. When `autoConfirm` is true, the dialog opens and immediately confirms, and the OK text carries the spawned `operationId` (via the `mcp_await_operation_start` round-trip — the FE replies with the id the manager minted) so the agent can drive `queue` / `await operation_complete` next. `compress` on an existing target keeps its dialog open and acks without an id. `rename` (`newName` required; optional `pane`, `name` (defaults to the cursor item), `autoConfirm`) targets the named item, else the cursor item, resolved off the pane state. Without `autoConfirm` it's a round-trip: the FE moves the cursor to the target row and starts the inline rename editor prefilled with `newName` for the user to review (`StartRenameOptions.initialName`, pinned to the row via `expectedName`) — the human-review affordance. With `autoConfirm` it calls the `rename_file` backend directly (`force: false`, so an existing target name is an honest error), and the managed op notifies the listing cache so the pane refreshes. `mkdir` / `mkfile` take optional `name` + `autoConfirm`: no name opens the naming dialog; `name` alone opens it prefilled (`initialName`); `name` + `autoConfirm` creates directly — a round-trip where the FE calls `create_directory` / `create_file` with its LIVE focused-pane path (never a backend `PaneStateStore` read, which lags a nav by the debounced sync and could create in the pane's previous directory), returning OK or an honest conflict error. `delete` gains `mode` (`trash` | `delete`): without `autoConfirm` the dialog's trash/permanent toggle is preset to it; with `autoConfirm` the FE routes to `trash_files` vs `delete_files`. `mode` maps to the FE's `permanent` bool (a typed IPC flag) and only rides the event when given — omitted, the FE applies its per-volume default (trash where supported, forced permanent on no-trash volumes and inside archives), so the volume clamp stays single-sourced in the FE.
- Tags (1): `tag` (`action` = `set` | `toggle` | `clear`, `colors` array of the seven Finder color names, optional `pane` (default focused) and `names` (default selection, else cursor)). A thin adapter over `file_system::tags_io` (`toggle_color` / `apply_merge` / `clear_tags`), the context-menu toggle's primitives, on the pane's volume. Resolves target paths off the pane state (`resolve_pane_target_paths`: names, else selection, else cursor — an unresolvable name/selection is an honest error), then patches the focused pane's cached listing via `apply_tags_to_listing` (the `enrich_tags` refresh path) so the dots re-render. Before resolving, it `flush_pane_state`s the target pane (the shared `mcp-sync-state` round-trip — the `syncStateToMcpNow` freshness `move_cursor` / `select` get as a side effect, without moving the cursor or changing the selection), so a tag right after a bare `nav` resolves against the live listing rather than a same-named file from the pane's previous directory. `set` keeps colorless custom tags and preserves a custom-named tag of a requested color; `toggle` uses Finder's all-have→remove rule per color; `clear` writes the empty set. macOS-only (Finder tags don't exist elsewhere), so off macOS it returns a clean not-supported error. No FE dispatch, no invented ack (the `indexing` precedent).
- View (3): `sort`, `toggle_hidden`, `set_view_mode`
- Tabs (1): `tab` (unified: `action` = `new` | `close` | `close_others` | `activate` | `set_pinned` | `reopen` (restore the last-closed tab) | `move`; `tabId` defaults to active tab for close/close_others/set_pinned/move, required for activate, unused for new/reopen; `pinned` boolean for set_pinned; for move, `pane` is where the tab is now, `toPane` defaults to it (a reorder), `toIndex` is the 0-based index the tab ends up on and defaults to the end, and at least one of the two is required. A move never activates the tab or changes pane focus, and is refused with a typed `data.reason` for a pinned tab, a pane's only tab, and a pane at its tab limit)
- Dialogs (2): `dialog` (unified open/focus/close/confirm). `action: "confirm"` programmatically confirms an open dialog (its strict enum — `transfer-confirmation` / `delete-confirmation` — stays narrow: confirm bypasses user review, so that surface must not widen). For `transfer-confirmation`: accepts optional `onConflict`. For `delete-confirmation`: just confirms. Optional `background: true` presses the dialog's Background button (F2) instead, so the operation starts with no progress dialog (`apps/desktop/src/lib/file-operations/transfer/DETAILS.md` § "Starting in the background"). **Both are a round-trip that returns the started `operationId`**, before the usual `SoftDialogDisappeared` ack: the request id rides the press (`ConfirmOptions.mcpRequestId` → the confirm payload → the birth props, winning over the dialog's own) into `transfer-dispatch.ts::startTransferOperation`, which answers it exactly as it answers an auto-confirmed `copy`. So the OK reads `OK: Transfer dialog confirmed (operationId: <id>).` (or `Delete dialog …`), and in the background `OK: Started in the background (operationId: <id>). …`, ready for `await operation_complete` or `queue`; a refused start is an error with no id, and a foreground start the progress slot refuses is the typed `data.blockingDialog` (`dialog_block_error`). The id also keeps a single-item Move inside its own folder a transfer instead of an inline rename, as for an auto-confirm (`pane/rename-in-place.ts`). A press that never reaches a start answers from the frontend's `ProgrammaticConfirmVerdict` (`pane/programmatic-confirm.ts`), typed in `refusal`: `permanentDelete` (a background press on a dialog that would delete permanently, which never runs out of sight) and `notStarted` (the dialog's own guard stopped the press: a refused source, a press already in flight, a cloud trash handed back), both `invalid_params` with `data.refusal` and the dialog left open (branch on that, ❌ never the message). Parsed by `dialogs.rs::parse_confirm_response`. Any other dialog type with `background` is `invalid_params` up front. The budget is an auto-confirm's 10 s, which a cloud folder's trash stretches by asking the round-trip to hold while its walk decides trash versus delete (`DETAILS.md`, the Holds bullet), so it still answers with its id. `type: "transfer-confirmation"` is the primary name (covers copy and move); `"copy-confirmation"` is accepted as an alias. **`action: "close"` is generic**: settings / file-viewer close via their window; about + the confirmations keep their dedicated close events; every OTHER registered soft dialog (whats-new, go-to-path, search, `selection-add` / `selection-remove`, feedback, drive-index-stale, …) closes through one `mcp-close-dialog { id }` event validated against the FE-registered known dialogs (`is_registered_soft_dialog`) and acked on `SoftDialogDisappeared(id)`. The `type` schema param is a free string (not an enum) so close reaches any dialog in `cmdr://dialogs/available`; an unregistered id is an honest `invalid_params`, an already-closed dialog acks immediately. The FE routes the id to the dialog's own close via the close registry (`apps/desktop/src/lib/ui/dialog-close-registry.ts`, populated by `ModalDialog` / `QueryDialog`). `open_search_dialog` opens the whole-drive search overlay with optional pre-filled `query`, `mode` (`ai`/`filename`/`regex`), `sizeMin`/`sizeMax` (bytes), `modifiedAfter`/`modifiedBefore` (ISO date), `isDirectory` (true = folders only, false = files only, omit for both), `scope` (chip syntax), `caseSensitive`, `excludeSystemDirs`, and `autoRun` (default true: runs the search after open). Acks on `SoftDialogAppeared("search")` within the 1500 ms budget. **Race-with-close caveat**: if the dialog is mid-close when the event lands, the new mount may race; the ack times out and the tool surfaces a clean failure rather than a false-positive OK. There is deliberately no `open_select_dialog` twin (see the decision below).
- App (3): `switch_pane`, `swap_panes`, `quit`
- Search (2): `search` (structured file search across the drive index, optional `scope` for path/exclude filtering), `ai_search` (natural language search using configured LLM, optional `scope` merged with AI-inferred scope). `ai_search` resolves its backend like every LLM call (`ai/DETAILS.md` § Cloud AI consent), so on Cloud without "Allow cloud AI" it refuses with `invalid_params` and a typed `data.reason: "cloudAiNotAllowed"`, and under the organization's managed policy with the refusal's own name (`aiOff` / `cloudAiOff` / `hostNotAllowed`, `ai/DETAILS.md` § Managed policy); only the user can turn that on, in the app
- Settings (1): `set_setting` (change a setting value via round-trip to frontend). A setting that records a person's
  consent answer (the terms acceptance, a held "no" to cloud AI or Ask Cmdr) is REFUSED with a typed
  `notSettableOverMcp` refusal: the registry marks it `mcpSettable: false`, and the frontend bridge
  (`apps/desktop/src/lib/settings/mcp-main-bridge.ts`) reads that mark, ❌ never an id list. An AI client must never undo
  a consent answer. The data-sharing switches (usage stats, crash and error reports) are `mcpSettable: 'offOnly'`: a
  client may turn them off to help someone opt out, and turning one on is refused as `onlyOffOverMcp`. `askCmdr.enabled` is marked too: it isn't consent, but on Local it starts a proactive loop, so a
  client mustn't switch it on. A setting the organization's managed policy locks is refused in Rust BEFORE the round
  trip, with `data.reason: "managedByOrganization"` (`managed_policy::refuses_write`, which reads `locked_settings`: a
  `Fixed` lock refuses every write, a `DisallowedValues` lock only its values, so `ai.provider: "local"` still goes
  through under `DisableCloudAI`). Every other setting, hidden ones included, stays settable.
- Indexing (1): `indexing` (`action` = `enable` | `disable` | `rescan` | `forget`, `volumeId`). A thin
  adapter over `commands::indexing` (`enable_drive_index` / `disable_drive_index` / `rescan_drive_index` /
  `forget_drive_index`) — no FE dispatch, no invented ack (the `connect_to_server` precedent). `enable`/`rescan` map the
  typed `EnableIndexingOutcome` to honest text and carry the ordering contract (below); `disable`/`forget` map
  `Result<(), String>` directly. Either deferral (`DeferredUntilSearchEnds`, `DeferredUntilScanEnds`) returns straight
  away, saying which holder the scan is waiting on: the index remembers the request and runs it when that holder ends
  (model: `crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` § The one walk a volume remembers), so the freshness the
  ordering contract waits for isn't going to move yet and waiting for it would just burn the budget and then lie.
  `DriveLeaving` (an unmount of the drive was under way, so the drive-release gate ran no start) and `IndexingDisabled`
  (the master switch) are tool errors. A `rescan` whose freshness never moved answers `outcome_unknown`, ❌ never `OK`. Because the generic executor can't supply the concrete `AppHandle` that
  `enable`/`rescan` need, they route through handle-free `*_via_handle` wrappers backed by a startup-cached handle
  (`set_app_handle` in `setup()`). Status is NOT an action — it lives in `cmdr://indexing`.
- Queue (1): `queue` (`action` = `pause` | `resume` | `cancel` | `pause_all` | `resume_all`; `operationId` for the
  per-op actions, `operationIds` array for multi-cancel, `rollback` bool on cancel). A thin adapter over the
  `cmdr-operations` manager (`pause_operation` / `resume_operation` / `pause_all` / `resume_all` / `cancel_operation` /
  `cancel_operations` / `cancel_write_operation(id, rollback)`) — direct backend calls, no FE ack (the `indexing`
  precedent). Per-op actions validate the id against `list_operations` for an honest "unknown operationId" instead of a
  silent backend no-op — skipping the RETAINED FAILURES that snapshot also carries (`is_controllable`), since acting on
  one of those is precisely the no-op the guard exists to refuse. Pause / resume then answer from the manager's
  `PauseOutcome` rather than assuming (`pause_reply` / `resume_reply`): `Applied` is the plain OK — it covers a
  SCANNING operation too, since the walk parks on the same gate the drivers do — `AlreadyInState` is an OK for a
  repeated request (the intent holds, so a retrying agent isn't refused), and `NotApplicable` is a refusal —
  a QUEUED operation is the everyday case there, since pause deliberately leaves one alone, and "OK: Paused …" for it
  would send the agent on believing the queue had stopped. The SWEEPS answer the same way, from a `PauseAllOutcome`'s
  counts (`pause_all_reply` / `resume_all_reply`, sharing one assembler so the two directions can't drift): an empty set
  reads "Nothing to pause: no operation is running", a mixed sweep names how many flipped, how many were already
  there, and how many finished before the request reached them. ❌ Never collapse a sweep to a
  flat "OK: Paused every running operation" — that sentence is true of a sweep that touched nothing. A
  `rollback: true` cancel deletes already-copied files and therefore remains an explicit opt-in. Discover ids + status
  in `cmdr://state` `operations:`. `connect_to_server` (add a manual SMB server by address, checks TCP reachability), `remove_manual_server` (remove a manually-added server by host ID), `upgrade_smb_to_direct` (upgrade an OS-mounted SMB volume to a direct smb2 session for faster I/O; runs the same `network::smb_connect_directly::connect_directly` as the "Connect directly" button: tries Keychain creds, answers OK or needs-credentials, and refuses a server it can't reach as an internal error, and a volume that's gone or isn't an SMB mount as invalid params, since the `volumeId` is what's wrong)
- Conflicts (1): `resolve_conflict` (`operationId` + `conflictId` + `resolution` = `skip` | `overwrite` | `rename` |
  `overwrite_smaller` | `overwrite_older`, optional `applyToAll`). Answers ONE name clash a running
  operation is parked on, the state a person reaches by leaving the transfer dialog on "Ask for each" and clicking Skip
  on one file. See § "Answering one clash" below for how it's reached, what it returns, and why the id is required.
- Locked containers (1): `unlock` (`path` + `password`). Answers the password prompt of an encrypted archive or a
  locked restic repo, the other modal state a hand used to be the only way to drive. See § "Answering the password
  prompt" below for the discovery block, the two modes, and why it may hand over a password but never start the write.
- Favorites (1): `favorites` (`action` = `add` | `rename` | `remove` | `reorder`; `path` (+ optional `name`) for add, `id` for rename/remove, `name` for rename, `orderedIds` (the COMPLETE new ordering) for reorder). A thin adapter over `commands::favorites` (`add_favorite` / `rename_favorite` / `remove_favorite` / `reorder_favorites`), which persist `favorites.json` and re-emit `volumes-changed` themselves, so the switcher refreshes live — no FE dispatch, no invented ack (the `indexing` precedent). `add` maps the gate's two refusals to `invalid_params` with words naming the rule: `NotAPlace` (favorites point at folders on disks, shares, servers, and phones) and `PlaceNotConnected` (connect first; the gate only favorites a folder it has seen live). `reorder` is a pass-through of the full ordering (a `(id, position)` shape would force the MCP layer to re-implement splicing). Discover ids in `cmdr://state` `favorites:`.
- Eject (1): `eject` (`volumeId`, optional `scope` = `volume` (default) | `disk`: `volume` takes one volume of a disk that keeps others, answering `volumeEjectedDiskStays`, and the disk with the last one; `disk` is the menu's Eject all, refusing `heldBeforeEject` with nothing unmounted when it can tell someone's holding a volume). A thin adapter over `file_system::volume::eject::eject` — parity with the one-click Eject button, and it detaches every kind that button does: a disk or disk image ejects, an SMB share unmounts, a phone disconnects, and an SFTP or WebDAV connection closes while the server stays SAVED (forgetting it is `remove_manual_server`, a different act). Named `eject` although the frontend says "detach" internally: it's a shipped tool name, and renaming it would break scripts. The backend refuses honestly (surfaced as errors, not false OKs) while a write op reads from or writes to the volume (`EjectError::Busy`) and for a volume that can't be detached at all. A call for a volume whose eject is still running (from the UI or another call) joins it and answers with that eject's result, never a second teardown.
  - **Both answers are typed.** Success returns `{ "outcome": <EjectOutcome>, "volumeId", "message" }`, so an agent learns WHICH teardown ran (`deviceDisconnected`, `remoteDisconnected`, `unmounted`, `diskEjected`, `volumeEjectedDiskStays`, `alreadyGone`) rather than parsing a sentence. Every refusal sets `data.outcome` to that `EjectError` variant's own serde tag, plus `holders` on `unmountRefused` and `heldBeforeEject`, `provider` on `deviceDisconnectRefused`, and `step` on `notResponding`. The tool's tags and the wire type are pinned to each other by `eject_test.rs`, not by discipline.
  - ❌ Never branch on the message. ❌ Never read `holders` with an empty `named` as "nobody is holding it": only the `complete` arm means that, while `incomplete` means the scan couldn't cover every mount (`file_system/volume/DETAILS.md` § "A refusal names who held the drive"). ❌ Never read an ABSENT key in `data` as an empty answer either: a key the tool couldn't fill is left out rather than nulled.
- Async (1): `await` (poll until a condition is met. Pane conditions (`pane` required): `has_item`, `not_has_item`, `item_count_gte`, `item_count_lte`, `path`, or `path_contains` — the absence conditions are for "wait until the delete finished" flows; `~` expands in path-condition values; `afterGeneration` avoids matching stale state. Volume condition (`volumeId` + `value`, no pane): `index_status` waits until a volume's indexing freshness equals `fresh` / `scanning` / `stale`, reading the single freshness store each tick (never re-deriving — the transition table lives in `indexing/lifecycle/freshness.rs`). Deliberately two fields, NOT a packed `<volumeId>:<status>` string: MTP volume ids embed colons. Operation conditions (no pane): `operation_complete` (`value` = operationId) resolves when the op settles and reports the terminal status (completed / cancelled / failed) — an id in neither the live registry (`list_operations`) nor the terminal-ops ring is an honest "unknown operationId" error, never a hang; `operations_idle` (no `value`) resolves when no op is running or queued (paused ops excluded, so it can't hang on a parked op) and logs the snapshot it decided on at `debug` under target `mcp::await` — it once answered "idle" while a copy admitted eight seconds earlier was ~20% done and has not reproduced, so the id-and-status list plus the poll count is the evidence the next occurrence needs (empty list = `list_operations` never saw the op; rows present = every one read as paused or settled). `timeoutSeconds` up to 60.)
- Downloads (1): `go_to_latest_download` (no args; navigates the focused pane to `~/Downloads` and selects the most recently observed eligible file. Errors when no eligible file exists or FDA is missing. Reuses the same backend code path as the `⌘J` shortcut and the `go_to_latest_download` Tauri command, then drives `mcp-nav-to-path` + `mcp-move-cursor` round-trips for the navigation + cursor placement)
- Operation log (3): `operations_list` (filter the durable journal by time range, item `name` (exact/prefix on the folded name, not substring), `kind`, `initiator`, `executionStatus`, `rollbackState`; paged, newest first — a bare call reads the recent feed so a still-running op sorts first), `operations_get` (`operationId` + a page of item rows with full paths and per-item outcome), `operations_rollback` (reverse an operation via the rollback engine — see the dispatch-then-poll contract below). All read through the query API / rollback engine; classification params + result fields cross the wire as the typed `operation_log::types` enums in camelCase (`createFolder`, `aiClient`, `partiallyRolledBack`), never a hand-parsed string.

## `operations_rollback`: dispatch-then-poll (how an agent observes the terminal result)

`operations_rollback` **requires `autoConfirm: true`** — a rollback writes to disk, so an authenticated caller must opt in explicitly rather than trigger it through a default (MCP has no interactive confirmation of its own; the app's `RollbackConfirmDialog` sits in front of the human-driven paths, the operation log's Roll back button among them). It calls the backend `Operations::dispatch_rollback` directly (tagging `initiator = ai_client`), NOT through an FE round-trip: unlike copy/delete, a rollback's whole plan lives in the backend engine, so there's no FE knowledge to defer to.

**It returns after DISPATCH, not completion.** The inverse operation spawns as an async managed op (cancelable, shown in the operation queue), so the tool replies `{ status: "dispatched", inverseOpId }` the moment the original op is set to `rollingBack`. To observe the terminal result, the caller **polls `operations_get` on the original `operationId` until `rollbackState` leaves `rollingBack`** — settling to `rolledBack` (everything reversed), or `partiallyRolledBack` (some items skipped: drift, an occupied restore target, an unverifiable precondition, or a copy whose original is gone — the data-safety rechecks never overwrite, and never delete the only copy left). The MCP `await` tool can gate the poll cadence on a pane signal for the ack. A domain refusal (unknown op, already rolling back, not rollbackable, a disconnected volume) comes back as a typed `{ status: "refused", refusal: { kind, detail } }` body — branch on `kind`, never a message.

## Answering one clash (`resolve_conflict`)

**The state was unreachable, and that is why a bug lived in it.** `dialog confirm` used to accept only `skip_all` / `overwrite_all` / `rename_all`, so every agent-driven transfer decided all its clashes before it started, and nothing automated could put an operation into Stop mode and answer a single file. A wedge in exactly that path — answering one clash swallowed the NEXT one and parked the transfer forever, escapable only by Cancel or Rollback — survived a six-milestone rewrite of the code it lived in, an adversarial review, and a QA agent driving the app, and was finally found by clicking. ❌ Don't narrow this surface back: what an agent can't drive is where bugs accumulate invisibly.

The loop, all MCP: `copy` → `dialog confirm transfer-confirmation` with **`onConflict: "stop"`** (the policy that decides nothing) → read the `pendingConflict:` block in `cmdr://state` `operations:` → `resolve_conflict` with the `operationId` and that block's `conflictId` → the operation raises its next clash under a new id → repeat, or answer with `applyToAll: true` to stop being asked. `copy` / `move` with `autoConfirm: true` take the same `onConflict`, so the one-shot path can ask per file too.

**One policy list, three tools, one frontend map.** `mod.rs::CONFLICT_POLICIES` (`stop`, `skip_all`, `overwrite_all`, `rename_all`, `overwrite_smaller_all`, `overwrite_older_all`) is what `copy`, `move`, and `dialog confirm` all validate against, mirrored once in `#lib/file-operations/transfer/conflict-policy.ts`. Before this there were two private frontend maps, and they had drifted: one spelled the conditional policies `overwrite_all_smaller`, the other `overwrite_smaller_all`, and neither spelling was reachable through any tool, so nothing ever surfaced it. The failure mode is silent — an unmapped name falls back to `skip`, turning "ask me about each file" into "skip every file" over the user's data — so the frontend now logs a name it doesn't know instead of quietly defaulting.

**The `conflictId` is required, never defaulted.** An operation raises its clashes one at a time, but an answer's round trip can outlast the clash it belongs to; a tool that answered "whatever is pending" would silently decide a question nobody was shown. The backend arbitrates by id (`crates/cmdr-operations/DETAILS.md` § "Answering a conflict is arbitrated"), and this tool reports that verdict rather than flattening it:

- `Resolved` → `OK` + `outcome: "resolved"`. This answer is the one the operation carried on with.
- `AlreadyResolved` → `OK` + `outcome: "already_resolved"`. Somebody answered the SAME clash first; the clash is settled, but with THEIR answer.
- `StaleAnswer` / `NoPendingConflict` / `UnknownOperation` → `INVALID_PARAMS`, with the same token in `data.outcome`. Nothing happened, and the caller's picture of the operation is wrong — the `queue` `pause_reply` mapping, for the same reason: an `OK` for something that didn't happen sends an agent on acting against a transfer that never moved.

❌ Never branch on the message; branch on `outcome` / `data.outcome`. `stop` is rejected as a resolution (it's the policy that raises the question). Gate `Always`: this answers, with no dialog, a question that was put to the user, and `overwrite` destroys a file — the same bypass the token guards for `dialog confirm`. Proven end to end by `apps/desktop/test/e2e-playwright/mcp-conflicts.spec.ts`, which drives a Stop-mode clash entirely through MCP, answers one file, and shows the second clash surviving the first one's answer.

## Answering the password prompt (`unlock`)

**The other state only a hand could drive.** An encrypted archive raises a password prompt, and until this tool the
prompt was IPC-only: `cmdr://state` showed a bare `- type: archive-password` with no archive named and no way to answer,
so an agent's only exit was cancel. Both flows sat in that blind spot — browsing into a header-encrypted archive, and
copying out of an encrypted one — which is the same shape the conflict wedge lived in for months. ❌ Don't narrow it
back.

**Discovery is the `dialogs:` entry** (`../resources/mod.rs::format_archive_password_dialog`), fed by a mirror the frontend
pushes as it raises the prompt (`ui_state/archive_password.rs`, `ArchivePasswordPromptStore`; cleared on dismissal and on
`register_known_dialogs`, since a webview reload never fires the dismiss half). It carries `container` (`archive` |
`resticRepo`, supplied by the frontend), `name`, `path` (the string an answer must echo back), `mode`, `wrongAttempt`,
`answerWith`, and for a transfer the `settledOperationId`. `wrongAttempt` is what makes the loop closeable from outside
for an archive: storing a password proves nothing about it, so the observable that a stored one was just REJECTED is the
whole feedback signal.

**The container's own answer decides the reply.** The tool goes through the same unlock seam as the dialog
(`commands::file_system::unlock_within` → `Volume::as_password_locked`), whose typed `UnlockOutcome` it reports: an
archive answers `stored` (unchecked, `verified: false`), a repo checks first and answers `unlocked` (`verified: true`)
or `wrongPassword`, which the tool returns as `outcome: "wrong_password"` with nothing kept and the prompt left up (no
event, no ack). A container it couldn't ask is a typed refusal (`not_locked`, `timed_out`, `unreachable`). An agent's
unlock always passes `remember: false`: saving to the Keychain is the person's call.

**The two modes are genuinely different events, and the reply says which.**

- `browse` → `outcome: "retrying_listing"`. Re-listing is a READ, so unlocking completes it exactly as it does for a
  person: the pane lists the contents, or (for an archive) the prompt comes back with `wrongAttempt: true`.
- `transfer` → `outcome: "password_stored"`. The password is held and **nothing is running**. Extracting means
  running `copy` / `move` again.

**❌ `unlock` must never start a write. This is the whole design constraint.** A person's submit re-dispatches
the parked copy for them; the agent path settles it instead (`unlock-flow.svelte.ts::supplyStoredPassword` →
`settleBirthOperation`, ❌ never `redispatchBirthOperation`). Two reasons it isn't merely conservative: that re-dispatch
is a genuinely NEW operation — the backend already settled the old one, because `retain_failure` excludes
`ArchiveNeedsPassword` by typed variant (`crates/cmdr-operations/DETAILS.md` § "Retained failures") — and a new operation
started from inside an unlock would be the one write on this surface reaching disk with no confirmation dialog and no
`autoConfirm` token in front of it. Sending the caller back through `copy` also sidesteps the re-dispatch's known hazard
(it re-scans against birth context captured before the prompt, so a pane that moved meanwhile aims it at the wrong
place — `file-explorer/pane/DETAILS.md` § "Birth context"); a fresh `copy` reads current pane state.

**The secret's whole path is JSON param → `commands::file_system::unlock_within` → the container's
`PasswordLocked::unlock`.** It never enters the prompt mirror, never renders in a resource, never appears in a log line, and is
never echoed in the reply (pinned by a test). The frontend is then told to do its follow-up by the ordinary
`mcp-confirm-dialog` event with no payload beyond the type, so the password never crosses into the webview at all —
which is also why the frontend arm asserts that it calls neither `setArchivePassword` nor `clearArchivePassword`.

**The tool starts nothing, but still handles a secret.** It hands a password into the app, and `wrongAttempt` turns
the surface into a password oracle if exposed without transport authentication: a loopback process could otherwise
grind guesses against an archive the user opened, reading the verdict straight out of `cmdr://state`. `dialog confirm
archive-password` is refused and points here, since a confirm carries nowhere to put the password; `dialog close
archive-password` is still the cancel, and forgets the stored password as a person's cancel does.

Proven end to end by `apps/desktop/test/e2e-playwright/mcp-archive-password.spec.ts`, which drives both flows entirely
over MCP and holds the boundary open for three seconds after the unlock: no operation appears, and the destination file
does not exist, until the agent starts the copy again through the normal confirmation.

## MCP action tools wait for backend ack before returning success

**Decision:** Every fire-and-forget action tool waits for a typed ack signal (`AckSignal::GenerationAdvanced`, `GenerationAdvancedOrSoftDialog` — the one honest two-arm OR, used by auto-confirmed `compress` where the op either starts OR the confirm dialog stays open on an existing target, see `ack.rs`, `SoftDialogAppeared`/`Disappeared`, `WindowAppeared`/`Disappeared`, `WindowCountBelow`, or `Any`) within a 1500 ms budget (5 s for the nav family) before returning `OK`. On timeout, the tool returns a `ToolError::outcome_unknown` (`data.outcome: "unknown"`) whose message names the missing signal and elapsed budget: the action was dispatched and may still run, so it's neither an `OK` nor a failure.

**Why.** Real QA hit a paper-cut: MCP tools were returning `OK` while the FE was stalled (modal blocking input, error pane up, race during startup), so the dispatched action was silently dropped. That made MCP unreliable as an automation surface. The ack contract makes `OK` a real promise: the FE actually processed the dispatched action.

**Why 1500 ms.** Most state pushes complete within ~100–300 ms in practice (FE debouncing, IPC round-trip). 1500 ms gives a generous margin for the slow cases (cold start, large directory listings) while still failing fast when the FE genuinely isn't responding. Latency-sensitive tools (`nav_to_path`) keep their existing higher budgets via `mcp_round_trip_with_timeout`.

**Why not a per-tool client-facing timeout knob.** The timeout is a backend-side latency budget, not a client concern. MCP clients shouldn't have to tune it per call — they expect tools to either succeed or report a clear failure.

## The `indexing` ordering contract (race-free `await index_status`)

**Decision.** `indexing` `enable` / `rescan` don't return until the volume's freshness has LEFT its pre-scan state, so a
follow-up `await index_status <volume> fresh` can't instantly match the pre-rescan Fresh state. The handler captures the
freshness before the backend call, then polls until it differs (bounded 5 s, timeout-tolerant); a pre-state that's
already `scanning` needs no wait.

**Why it's not a spurious ack.** `force_scan` (every rescan's walk, `Index::rescan_volume`) fires `ScanStarted` → `Scanning`
synchronously before returning, so the first poll already differs and the handler returns immediately. Only the
enable-first-scan path (a not-yet-active volume, especially async SMB via `start_indexing_for_smb`) can lag the flip,
and that's exactly where the wait earns its keep. This replaces a generation counter for the new `await` condition (the
tool contract sequences it instead), which is cheaper and honest.

## `select_volume` replies once the pane has come to rest

`select_volume` resolves against the completed volume pipeline, the same rows the switcher receives. `volumeId` is the stable selector and wins if both fields arrive, so two rows called "Pixel 9" still choose MTP or ADB exactly; a name-only call with multiple matches is refused with typed `ambiguousVolumeName` data and the matching ids. This also retires the hand-built MTP naming that could drift from its provider. The tool is a round-trip on `mcp-volume-select` (`mcp_nav_round_trip`). The FE switches the pane, waits for the switch's background correction (which reopens the folder last used on that volume), then for the pane to go quiet, and replies with the typed landing `outcome` plus the folder the pane opened (`src/routes/(main)/mcp-volume-select.ts`). `select_volume_result` (`nav.rs`) words it: `navigated` is the only `OK`, and it names that folder. ❌ Don't ack earlier, when the pane first shows the volume: the correction then lands after the reply and supersedes the agent's next `nav_to_path` with "Superseded by new navigation". After a `navigated` reply the handler still polls the pane's `volume_name` in `PaneStateStore` (5 s) until it equals the selected row's name, so a `cmdr://state` read right after the tool returns names the volume. A favorite skips that poll: it is a folder on some volume, so the pane reports THAT volume ("Macintosh HD"), as `paneVolumeOf` does. A favorite the pane can't go to (an unplugged phone, a forgotten server; its `reach` in `cmdr://state` `favorites:`) leaves the pane put and replies `ok: false` with the sentence the user's toast shows. Worth knowing:

- **Re-selecting the volume a pane already shows is fast**: nothing re-lists, so the FE doesn't wait for a new listing, and the name already matches.
- **The virtual `Servers` volume** has no listing, so the FE replies once the correction settles, and the hub pushes its own name, which the poll waits for. ❗ That name is `volume_listing::SERVERS_VOLUME_NAME`, which has to equal the English `fileExplorer.navigation.networkVolume` catalog value byte for byte and the name the frontend pushes: a drift makes `select_volume` on the hub time out instead of answering.
- **The servers hub's rows carry their columns inside `name`**, as `<name>  protocol=<p>  status=<s>  address=<a>` (plus `shares=<n>` on an SMB host whose shares were listed), because `PaneFileEntry` has only `name` / `path` / `isDirectory`. The servers that were only found nearby follow one `Found nearby  kind=group  state=<expanded|collapsed>  servers=<n>` entry (`smb://nearby`) and are listed whatever its state; `move_cursor` onto a hidden one opens the group, and `open_under_cursor` on the entry itself toggles it. The status token is deliberately NOT translated: it is a wire an agent parses, and `smb.spec.ts` polls on it. A one-place server's `path` is its app root (`sftp://user@host:port/…`), an SMB host's is `smb://<address>`, and the last row is always `+ Add server…` at `smb://add`. Owned by `src/lib/file-explorer/network/servers-hub-mcp.ts`.

`volume_name` flows through `PaneState` from the FE via `update_left_pane_state` / `update_right_pane_state` on every state push (`FilePane.svelte`).

## Tool execution is async; action tools wait for ack

`execute_tool()` is an async function. Action tools follow the ack contract (see "Action-tool ack contract" above): dispatch the event, then `wait_for_ack` for a small backend-side signal before returning. The tool's reported "OK" thus means "the FE accepted the dispatched action," not "the underlying operation completed." For long-running operations (a copy of 10 GB), the agent still has to poll via the `await` tool to observe completion. The ack-contract change made the FE-accepted line meaningful — before it, the tool returned `OK` even when the FE wasn't listening.

Two categories of latency-sensitive tools exist beyond the ack contract: (1) `mcp_round_trip` tools (`nav_to_path`, `select_volume`, `move_cursor`, `select`, `refresh`, `set_setting`, `open_under_cursor`) that wait up to 5–30 s for an explicit `mcp-response` event with success/failure — the per-request correlation means the ack works even when the action produced no pane-state push (a `refresh` whose re-listing is byte-identical to the cached state still acks), (2) search tools (`search`, `ai_search`) that load the search index via `spawn_blocking` and (for `ai_search`) call the LLM API.
