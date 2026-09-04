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
  going. `limit` clamps to the house `MAX_LIMIT` of 200. The result shape and every rule about it live in
  `search/result.rs` (§ The search result, below).
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
  pane mutators: `set_view_mode`, `sort`, `toggle_hidden`, `tab`, `nav_*`, auto-confirmed `copy`/`move`/`delete`, and
  `dialog confirm`. NOT `select`/`refresh` (both round-trips).
- **`SoftDialogAppeared(id)`**: fires when a soft dialog with that id is in `SoftDialogTracker`. Used by confirmation
  dialogs from `copy`/`move`/`delete` (`autoConfirm: false`), `mkdir`, `mkfile`, and `dialog open about`.
- **`SoftDialogDisappeared(id)`**: fires when a soft dialog with that id is no longer tracked. Used by
  `dialog close <confirmation>` (the FE `ModalDialog` fires `notifyDialogClosed` on unmount).
- **`WindowAppeared(label)`**: fires when a `webview_windows()` entry matches (exact, or `viewer-*`). Used by
  `dialog open settings|file-viewer` and `dialog focus`.
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
`requestId` and waits for the FE to reply via `mcp-response` carrying `{ requestId, ok, error? }`. Response correlation
lives in the pure, unit-tested `parse_mcp_response` in `mod.rs`. Per-tool:

- `move_cursor`, `set_setting` (5 s). The FE verifies the cursor actually landed (filename found, index in range), then
  (move_cursor) flushes the MCP state push (`syncStateToMcpNow`) before replying, so a follow-up `copy`/`move`/`delete`
  reads the new cursor instead of the stale pre-move one. A silent no-op (cursor never moved) was the original
  false-positive-OK bug.
- `select` (5 s, all modes): the FE applies the selection (names mode maps names → indices via the `findFileIndices`
  batch IPC first), then flushes the state push before replying, so a follow-up `copy` reads fresh selection state.
  Missing names come back as the round-trip error.
- `refresh` (5 s): the FE forces a backend re-read via `refreshListing(listingId, true)`, which bypasses the
  watcher-backed short-circuit, so `OK` means the directory was actually re-read on every volume. In the network
  browser the same command re-scans hosts instead.
- `nav_to_path`: 30 s via `mcp_round_trip_with_timeout`; the FE delays the response until `handleListingComplete` fires.
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
to the cursor file; cursor on `..` (or an empty pane, where `files` is empty with `total_files <= 1`) means the FE would
silently drop the dialog, so the tool rejects fast. Unsynced state (`path` empty) passes through. Without the `select` /
`move_cursor` pre-reply flush, select → copy reads a stale empty selection and move_cursor → copy reads a stale cursor
(still on `..`), and either wrongly rejects here.
