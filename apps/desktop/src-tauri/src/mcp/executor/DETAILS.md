# Executor details

Depth for the MCP tool-execution layer. `CLAUDE.md` holds the must-knows.

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
  a query it can write itself buys two providers and a translation it can't see (`../../agent/tools/DETAILS.md` § The
  tool catalog). The result shape and every rule about it live in `search/result.rs` (§ The search result, below).
- **`queue.rs`**: the `queue` tool (pause / resume / cancel one id, pause_all / resume_all). Thin adapter over the
  manager: no FE action, so no ack.
- **`conflicts.rs`**: `resolve_conflict` — answers ONE Stop-mode clash a running operation is parked on. Same adapter
  shape as `queue.rs`, over `write_operations::resolve_write_conflict`, and the whole point of it is reporting the
  ARBITRATION honestly: `Resolved` / `AlreadyResolved` answer `OK` (the clash is settled either way), `StaleAnswer` /
  `NoPendingConflict` / `UnknownOperation` are refusals, and every one of them crosses the wire as a typed `outcome`
  field or `data.outcome`, never as prose an agent would have to parse. `stop` is rejected as a resolution: it is the
  policy that RAISES the question. Discovery is the `pendingConflict:` block in `cmdr://state` under `operations:`
  (`resources/operations.rs`), which is also the only place the `conflictId` an answer must carry comes from.
- **`archive_password.rs`**: `unlock_archive` — answers the encrypted-archive password prompt. Same shape as
  `conflicts.rs`: the answer must NAME what it answers (`archivePath`, off the `archive-password` entry in
  `cmdr://state` `dialogs:`), and `no_password_prompt` / `different_archive` are refusals carrying a typed
  `data.outcome` rather than prose. It stores the password through `commands::file_system::store_archive_password` (the
  same one slot the dialog writes) and then emits a payload-free `mcp-confirm-dialog` so the frontend does the mode's
  follow-up; the secret never crosses into the webview. ❌ **It must never dispatch an operation.** `browse` answers
  `retrying_listing` (a listing is a read, so the unlock finishes it); `transfer` answers `password_stored` and stops
  there, because starting the extraction is a write and goes through `copy` / `move` like every other one. Full
  rationale, including why the parked operation is already settled and why the gate is `Always`: `../DETAILS.md`
  § "Answering the archive password".
- **`downloads.rs`**: `go_to_latest_download` (resolves via `downloads::commands::go_to_latest_download`, then
  `mcp-nav-to-path` + `mcp-move-cursor`).
- **`operation_log.rs`**: `operations_list`, `operations_get` (short-lived read-only connection over the query API,
  the `commands/operation_log.rs` pattern), `operations_rollback` (dispatches the rollback engine via
  `write_operations::rollback::dispatch_rollback`; returns after dispatch — see `mcp/DETAILS.md` § dispatch-then-poll).
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
`NAV_ACK_TIMEOUT` = 5 s); (4) returns `OK` on signal, or `ToolError::internal` naming the missing signal and elapsed
budget on timeout.

`AckSignal` variants, when they fire, and who uses them:

- **`GenerationAdvanced`**: fires when `PaneStateStore.generation` is strictly greater than the captured value. Used by
  pane mutators: `set_view_mode`, `sort`, `toggle_hidden`, `tab`, `nav_*`, and auto-confirmed `copy`/`move`/`delete`.
  NOT `select`/`refresh` (both round-trips), and NOT `dialog confirm` (below).
- **`SoftDialogAppeared(id)`**: fires when a soft dialog with that id is in `SoftDialogTracker`. Used by confirmation
  dialogs from `copy`/`move`/`delete` (`autoConfirm: false`), `mkdir`, `mkfile`, and `dialog open about`.
- **`SoftDialogDisappeared(id)`**: fires when a soft dialog with that id is no longer tracked. Used by
  `dialog close <confirmation>` (the FE `ModalDialog` fires `notifyDialogClosed` on unmount), and by
  `dialog confirm <transfer|delete>`: a confirm the FE acted on takes the dialog down in the same tick it starts the
  operation. ❌ Don't ack a confirm on `GenerationAdvanced`: a compress, or a copy onto a slow volume, changes nothing
  in either pane until its first file lands, so the tool answered "not acknowledged" about an operation that had
  started (a 300 MB compress onto a phone, 2026-09-30). ❗ This signal is also true of a dialog that was never open, so
  `confirm_open_dialog` checks the tracker FIRST and refuses a confirm of nothing with `invalid_params`. A confirm the
  dialog declines (an invalid path in its box) leaves the dialog up and times out, which is the honest answer.
- **`WindowAppeared(label)`**: fires when a `webview_windows()` entry matches (exact, or `viewer-*`). Used by
  `dialog open settings|file-viewer` and `dialog focus file-viewer`. `dialog focus settings` needs no ack: the backend
  raises that window itself (`set_focus`), and a closed one is `invalid_params`.
- **`WindowDisappeared(label)`**: fires when the matching `webview_windows()` entry is gone. Used by
  `dialog close settings` (single-window family).
- **`WindowCountBelow {prefix, threshold}`**: fires when the matching window count is `< threshold`. Used by
  `dialog close file-viewer` (snapshot count, ack when one closes; don't wait for all viewers to vanish).
- **`ArchivePromptAdvanced {from}`**: fires when the archive-password prompt mirror
  (`mcp/archive_password.rs`) moves past `from` — the frontend either took the prompt down or put a new one up. Used by
  `unlock_archive`, and ❌ never `SoftDialogDisappeared` for it: a browse unlock re-lists at once, and a wrong password
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
