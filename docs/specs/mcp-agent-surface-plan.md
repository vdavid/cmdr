# MCP agent-surface plan: catch the MCP server up with the app, ready it for the internal agent

Status: planned 2026-07-09, not started. Worktree: `.claude/worktrees/mcp-agent-surface`, branch
`david/mcp-agent-surface`.

## 1. Why (intent)

The MCP server was built alongside the app's early features and hasn't tracked the last ~2 months of shipping. Two
consumers need it to be current and excellent:

1. **The future in-app agent** (`later/agent-spec.md`) — an agentic loop triggered by FS events and user interactions.
   It must find things (search, importance, indexing state), understand things (state, listings, queue), and perform
   anything a user would reasonably expect (file ops, settings, indexing control) — all gated behind user approval. The
   agent-spec (§11.1, D49) says the agent-first registry should _extend_ the consolidated MCP registry, so every
   capability we add here is substrate the agent inherits.
2. **David's debugging** (secondary) — driving and inspecting a live app instance from a coding agent.

The security model is unchanged: **parity** (agents do only what users can do, no raw fs/shell), and **anything that
bypasses a user confirmation stays behind the bearer token** via the per-entry `TokenGate` (see `src/mcp/CLAUDE.md` and
DETAILS § Authentication). Backward compatibility is explicitly NOT a constraint: interfaces may be renamed/reshaped to
be more streamlined (the `tool_snapshot_tests` fixture gets updated deliberately).

### The gap list (verified against the tree, 2026-07-09)

Features shipped roughly April–July 2026 with no or partial MCP coverage:

- **Drive indexing** (0.25–0.31): per-volume indexing across local/SMB/MTP with freshness statuses, a step checklist
  with progress + ETA, per-drive enable/disable/rescan. MCP's `cmdr://indexing` reads only `ROOT_VOLUME_ID`
  (`resources/indexing.rs` calls `get_debug_status(ROOT_VOLUME_ID)`), and there are zero control tools. Backend has it
  all: `commands/indexing.rs` (`get_volume_index_status_by_id`, `enable_drive_index`, `disable_drive_index`,
  `forget_drive_index`, `rescan_drive_index`, `get_index_debug_status`).
- **Importance subsystem** (0.33): per-volume folder-importance scores with an explain API. No MCP exposure. Backend:
  `importance/read/mod.rs` (`ImportanceIndex::{lookup, weight_for, top_n, above_threshold, signals_for, explain}`),
  works offline for unmounted volumes.
- **Operation queue + pause/resume** (0.29): the Queue window, pause/resume/cancel per op. MCP shows in-flight ops
  read-only (`cmdr://state?include=transfers`) but can't control them and can't see queued/paused entries. Backend:
  `commands/file_system/write_ops.rs` (`pause_operation`, `resume_operation`, `pause_all`, `resume_all`,
  `cancel_operation`, `cancel_operations`, `cancel_write_operation(id, rollback)`, `list_operations`).
- **Rename**: no MCP tool at all — glaring for an agent whose spec literally says "name those screenshots". Backend:
  `commands/rename.rs::rename_file`.
- **Named create**: `mkdir`/`mkfile` only open the naming dialog; there's no way to pass a name, so an agent can't
  create `Invoices/` without a human typing. Backend: `create_directory` / `create_file` take names.
- **Trash vs delete** (0.17/0.29): the delete dialog has a Trash/Delete toggle; the MCP `delete` tool can't choose.
  Backend: `trash_files` vs `delete_files`.
- **Finder tags** (0.31): see/set colored tags. No MCP read or write. Backend: `file_system/tags.rs` (`read_tags`,
  `toggle_color`, `set_tags`).
- **Favorites** (0.27): add/rename/reorder/remove. No MCP. Backend: `commands/favorites.rs`.
- **Eject** (0.22): no MCP tool. Backend: `commands/eject.rs` (blocked during active ops by design, 0.24).
- **`await` is pane-only**: no way to wait for an operation to finish, indexing to go fresh, or a dialog to close — the
  exact conditions an agent loop polls for.
- **`cmdr://state` volumes are thin**: no filesystem type (0.32), no read-only flag, no per-volume index freshness badge
  (0.29), inconsistent `volumeId` presence (only SMB + MTP entries carry ids today).
- **Archives** (0.33): mostly covered already — archive paths are transparent (`/foo.zip/inner` navigates via
  `nav_to_path`, extract = copy out of an archive listing, `compress` shipped with the feature). Gaps are only the
  password-prompt dialog type visibility and docs. No new archive tools needed.

Covered fine today (no work): navigation, cursor/selection, copy/move/compress/delete dialog flows, tabs, dialogs,
search + AI search (0.33's importance-ranked search rides the shared search backend automatically), settings
(`set_setting` + `cmdr://settings`), network/SMB tools, logs, transfers read-only, downloads.

### Non-goals (deliberate)

- **No `read_file` / file-content resource.** "Understand stuff" tempts a content read, but content access is the
  privacy + prompt-injection surface the agent-spec designs carefully (§11.3: size caps, read budgets, sensitive-path
  denylist, per-read logging, consent). Bolting a content read onto the parity MCP server now would ship that surface
  without its guardrails. It arrives with the agent effort, in the agent-first registry.
- **No transport-agnostic registry refactor.** The agent-spec (§11.1) notes the registry core is MCP/Tauri-shaped and
  the in-process agent will need a typed core factored out. That's the agent effort's first infrastructure milestone,
  not this one. This plan only _avoids making it harder_: new handlers keep params extraction at the edge and call typed
  backend functions (`enable_drive_index(volume_id)`, not inline logic), so the future factoring lifts clean bodies.
- **No proposals/memory/notify tools.** Those are the agent-first tools (agent-spec §11.2), designed against the agent's
  storage; premature here.

## 2. Design principles for this surface (govern every entry below)

1. **Parity + TokenGate, mechanically.** Every new tool declares its gate in its `mcp_tools!` entry. The crisp line
   (matching the threat model in `mcp/DETAILS.md` § Authentication — a local non-Cmdr process doing silent damage):
   **gate silent mutations of user data or persistent config; leave reversible, non-destructive runtime actions open.**
   Concretely: tags (user-file metadata), favorites/indexing enable-disable-forget (persistent per-drive and app
   config), `set_setting`, and rollback-cancel (deletes already-copied files) are gated; eject, queue
   pause/resume/plain-cancel (transient runtime actions, no persistent state touched, crash-safe pipeline) are `Open`,
   as are all reads. Ops that have a confirmation dialog keep `IfAutoConfirm` on the bypass flag. When a new tool
   doesn't fit the line cleanly, gate it — an over-gated tool costs a token read; an under-gated one is a security bug.
2. **Unified verb tools over tool sprawl** (the `tab`/`dialog` precedent): one `indexing` tool with an `action` enum
   beats four `indexing_*` tools. Agents pattern-match; fewer, richer tools with typed enums beat many near-duplicates.
3. **Reads are resources, mutations are tools.** Status lives in `cmdr://indexing` / `cmdr://importance` /
   `cmdr://state`; tools change things. (Exception kept from the existing design: `search`/`ai_search` are tools because
   they take rich parameters.)
4. **camelCase params, snake_case tool names, plain-text/YAML output for LLM eyes** — the existing conventions, pinned
   by tests.
5. **Wrap typed backend APIs; never re-implement.** Every handler is a thin adapter over an existing command/module
   function. If the backend function doesn't exist, that's a backend change first, not MCP logic.
6. **Volume-id-first.** New params take `volumeId` (the stable id from `cmdr://state` volumes), matching the `Location`
   direction (`location-type-nav-plan.md`). Paths remain for path-shaped things only.

## 3. The target surface

### 3.1 New tools

- **`rename`** — rename the item under the cursor or a named item in a pane. Params: `pane`, `name` (optional; defaults
  to cursor item), `newName` (required), `autoConfirm` (optional bool). Without `autoConfirm`: starts the inline rename
  editor prefilled with `newName` (the user reviews and presses Enter — the same "open the dialog" pattern as `copy`).
  With `autoConfirm` (gated): calls `rename_file` directly and acks on the listing change. Gate: `IfAutoConfirm`.
  Intent: rename is THE canonical agent proposal ("name those screenshots"); the prefilled-editor flow is the
  human-review affordance. Note: the inline-prefill flow needs a small FE affordance (the rename editor accepts an
  initial value from an `mcp-rename` event); the editor is path-keyed since 0.33 so it follows reorders safely.
- **`indexing`** — control drive indexing. Params: `action` (`enable` | `disable` | `rescan` | `forget`), `volumeId`.
  Gate: `Always` (all four mutate per-drive config or throw away/start heavy background work with no confirmation
  dialog; consistent with the `set_setting` rationale). Status is NOT an action — it lives in `cmdr://indexing`.
  Handlers call `enable_drive_index` / `disable_drive_index` / `rescan_drive_index` / `forget_drive_index`. Note the two
  result shapes: enable/rescan return the typed `EnableIndexingOutcome` (map to honest text); disable/forget return
  `Result<(), String>` (map success/error directly). Intent: the agent must be able to notice "this SMB share has a
  stale index" and fix it; David needs the same for debugging scan bugs.
- **`queue`** — control the operation queue. Params: `action` (`pause` | `resume` | `cancel` | `pause_all` |
  `resume_all`), `operationId` (required for the per-op actions; also accepts `operationIds` array for multi-op cancel),
  `rollback` (optional bool, cancel only). Gate: `Open` for pause/resume/cancel (parity: one visible click in the Queue
  window / progress dialog; the pipeline is crash-safe and cancel is a first-class safe operation), EXCEPT
  `rollback: true` which is gated (it actively deletes already-copied files — that's the
  auto-confirm-a-destructive-thing shape). Implemented as a new `TokenGate::IfArg { key: "rollback" }`-style variant or
  a dedicated `IfRollback` variant — implementer's choice, but it must be a declared gate variant so the structural gate
  tests keep their by-construction property.
- **`eject`** — eject an ejectable volume. Params: `volumeId`. Gate: `Open` (parity: one click; backend refuses while
  operations touch the volume, and refuses non-ejectables — surface those as honest errors).
- **`tag`** — set Finder tags. Params: `pane`, `names` (optional array; defaults to selection, else cursor item),
  `action` (`toggle` | `set` | `clear`), `colors` (array of the 7 Finder color names, typed enum). Gate: `Always`
  (silent metadata mutation on user files, no dialog exists in the UI to piggyback on). macOS-only: return a clean "not
  supported on this OS" error on Linux.
- **`favorites`** — manage favorites. Params: `action` (`add` | `rename` | `remove` | `reorder`), `path` (add), `id`
  (rename/remove), `name` (add/rename), `orderedIds` (reorder — the complete new ordering, a thin pass-through to the
  backend's `reorder_favorites(ordered_ids)`; a `(id, position)` shape would force the MCP layer to re-implement
  splicing, violating principle 5). Gate: `Always` (persistent config mutation). A `favorites:` list (id, name, path)
  joins `cmdr://state?include=favorites` so agents can discover ids.

### 3.2 Changed tools

- **`mkdir` / `mkfile`** gain optional `name` and `autoConfirm`. No params: unchanged (opens the naming dialog). `name`
  without `autoConfirm`: opens the dialog prefilled. `name` + `autoConfirm`: creates directly (acks on listing change;
  errors honestly on conflicts — `create_file` already refuses existing paths). Gate moves from `Open` to
  `IfAutoConfirm`.
- **`delete`** gains `mode` (`trash` | `delete`), defaulting to what the UI dialog defaults to for that volume. Without
  `autoConfirm` the dialog opens with the toggle preset to `mode`. Gate unchanged (`IfAutoConfirm`).
- **`await`** gains non-pane conditions (pane becomes optional; required only for the pane-scoped conditions):
  - `operation_complete` (value: operationId) — resolves when the op settles; reports terminal status (completed /
    cancelled / failed) in the result text. **Design constraint the implementation must solve**: the manager removes
    settled ops (removal-on-terminal, `write_operations/DETAILS.md`), and `operations-changed` fires AFTER the removal,
    so its snapshot never carries a terminal status (`LifecycleStatus` never reaches a terminal value on a record in
    production). The terminal outcome lives only in the dedicated terminal events — `write-complete` / `write-cancelled`
    / `write-error` (and `write-settled`, which always fires after the outcome event for the same id). The MCP layer
    therefore keeps a small bounded **terminal-ops ring** (the `listing_errors` precedent: id, kind, terminal status,
    `settledAtUnixMs`, capacity ~20) populated from THOSE events, not from `operations-changed`. `await` on an id found
    in neither the live set nor the ring errors honestly ("unknown operationId") instead of hanging.
  - `operations_idle` (no value) — no running or queued operations.
  - `index_status` (params: `volumeId` + `value` = status ∈ `fresh` | `scanning` | `stale`) — the agent's "wait for the
    rescan I just started". Deliberately two fields, NOT a packed `<volumeId>:<status>` string: MTP volume ids embed
    colons (`mtp-{device_id}:{storage_id}`, and the device id itself may contain `:` — see
    `mtp/identity.rs::split_volume_id`'s `rsplit_once` comment), so a packed value invites naive-split bugs on exactly
    the volume kind this flow serves.
  - `dialog_closed` / `dialog_open` (value: dialog type) — completes flows like "opened the transfer dialog, user
    confirmed or dismissed it". Works only for dialogs that route through `ModalDialog` with a registered `SoftDialogId`
    (those auto-notify the `SoftDialogTracker` on mount/destroy); M4's `dialogs/available` sync confirms the
    transfer/delete confirmation dialogs qualify and the description names the limitation. Intent: these are exactly the
    poll loops an agent runtime would otherwise busy-loop on resources for. The implementation subscribes where a
    subscription exists (the `operations-changed` event, `index-phase-changed` / freshness events, `SoftDialogTracker`)
    rather than polling, matching the ack-contract philosophy. **Staleness guard**: the pane conditions have
    `afterGeneration`; the new conditions get their sequencing from tool contracts instead — see the ordering contract
    below. Where that's not enough (`operation_complete` on an id that settled long ago), the ring's `settledAtUnixMs`
    keeps the answer honest.
- **Ordering contract for race-free awaits** (this replaces a generation counter for the new conditions, and is
  cheaper): (a) `indexing` `rescan`/`enable` return only after the volume's status has actually left `fresh`/entered the
  scanning pipeline, so a follow-up `await index_status` (that volume, value `fresh`) can't instantly match the
  pre-rescan state; (b) the auto-confirmed `copy`/`move`/`delete`/`compress` responses gain the spawned
  **`operationId`** in their OK text (today they return a bare OK, leaving agents to fish ids out of `cmdr://state` and
  race completion), so `await operation_complete <id>` is directly sequenced after the start. These land in M1 and M3
  respectively. **(b) is a correlation-mechanism task, not a text edit**: the current autoConfirm flow acks on a
  pane-generation bump, and the operationId is minted in the manager when the FE-triggered
  `copy_files`/`move_files`/`delete_files` spawns the op — it flows to the FE, never back to the waiting MCP call. The
  honest design is to switch these autoConfirm paths to the `mcp_round_trip` pattern: the event carries a `requestId`,
  the FE replies `mcp-response` with the spawned `operationId` once the command returns it. Do NOT implement it as a
  before/after `list_operations()` diff — that's racy under concurrent ops and misses instant ops that settle before the
  diff, reintroducing the exact race this exists to kill.
- **Tool descriptions pass**: rewrite every description for the agent reader (what it does, when to prefer it, what the
  result means — e.g. `copy`'s "OK means started, poll `await`"). Descriptions are the agent's only manual; treat them
  as UI copy for agents. This deliberately churns the snapshot fixture once.

### 3.3 Resources

- **`cmdr://indexing` — rewritten per-volume.** Default: one block per known volume (enabled/disabled, freshness
  (`fresh`/`scanning`/`stale`), current phase + per-step checklist with counts and ETA where scanning, last completed
  scan time, DB entry/dir counts + file size). `?volume=<id>` adds the deep debug view (phase timeline, trigger history,
  watcher/live-event stats — today's root-only content, generalized). Sources: `get_volume_index_status_by_id`,
  `get_index_debug_status`, the freshness store, and the step-checklist state that feeds the FE (`drive-index-progress`
  work, 0.31). Intent: one read answers both "can I trust search on this volume?" (agent) and "why is the scan stuck?"
  (David).
- **`cmdr://importance` — new.** Modes:
  - `?path=<abs-path>` → the folder's `WeightLookup` (Scored with score + signal breakdown via `explain`, or Floored
    with the floor reason, or Unscored), YAML.
  - `?top=<n>&volume=<id>` (volume optional, defaults to all scored volumes) → top-N folders with scores.
  - `?threshold=<f>` → folders above threshold (bounded: cap the row count, note truncation). All reads go through
    `ImportanceIndex` (never raw SQLite), so offline volumes work — the agent-spec's headline "answer about unmounted
    drives" capability starts here. Paths in output are user paths the caller can already see in the UI (no redaction
    needed beyond what state applies — they're listings, not logs). Intent: the agent's context assembly (which folders
    matter) and David's weight-tuning debugging (`explain` breakdown) are the two named consumers.
- **`cmdr://state` enrichment:**
  - `volumes:` becomes uniformly structured: every entry gets `name`, `id`, `kind` (`local`/`smb`/`mtp`/`virtual`), plus
    present-when-known `filesystem` (APFS/exFAT/…), `readOnly`, `ejectable`, `indexStatus`
    (`fresh`/`scanning`/`stale`/`off`), and the existing `smbConnectionState`. Breaks the old mixed bare-string shape —
    intended; agents stop guessing which entries carry ids.
  - `transfers:` renamed to **`operations:`** (it already includes deletes; compress and archive edits flow through the
    same manager) and extended with queued + paused entries: each op gets `operationId`, `kind`, `status`
    (`running`/`paused`/`queued`), progress, speed, ETA. This is the discovery surface for the `queue` tool.
    Implementation note: this is a **join of two sources** — membership + lifecycle status come from `list_operations()`
    (`OperationSnapshot` carries no progress by design), while progress/speed/ETA come from the write-operations status
    cache today's `transfers.rs` builder already reads. Join by operation id; queued ops simply have no progress fields.
  - `favorites:` new include-section (see 3.1).
  - File entries gain a `[tags:red,blue]` marker only when tags exist (zero cost in the common case).
- **`cmdr://dialogs/available`**: verify the soft-dialog registry now includes the newer dialog types (queue window,
  password prompts, feedback, onboarding) and that `dialog`'s `type` enum matches reality; sync as needed.

### 3.4 Gate summary (the security review in one place)

- `Open`: `eject`; `queue` pause/resume/cancel (rollback excluded); all reads.
- `IfAutoConfirm`: `rename`, `mkdir`, `mkfile`, `delete` (with `mode`), `copy`, `move`, `compress` (unchanged).
- `IfConfirmAction`: `dialog` (unchanged).
- `Always`: `set_setting` (unchanged), `indexing`, `tag`, `favorites`.
- New variant for `queue`'s `rollback: true` (naming up to the implementer; must be a declared `TokenGate` variant).

The existing structural tests extend: the full-table set-equality test forces a conscious gate per new tool, and a new
structural test asserts every tool whose schema has a `rollback` property declares the rollback gate (mirroring
`test_autoconfirm_tools_are_gated`).

### 3.5 Ack story for the new tools

`indexing`, `queue`, `eject`, `tag`, and `favorites` wrap synchronous/async **backend** functions and don't dispatch FE
actions, so the executor ack contract ("wait for a typed ack before OK") doesn't apply — they return the backend result
directly, per the `connect_to_server` / `remove_manual_server` precedent. Don't invent an ack signal for them. **One
deliberate exception**: `indexing` `rescan`/`enable` carry the § 3.2 ordering contract — they must not return before the
freshness status has left its pre-scan state. The active-index `force_scan` path already flips to Scanning synchronously
before returning; the enable-first-scan path (especially async SMB via `start_indexing_for_smb`) may not, and there the
handler explicitly waits for the flip. That wait is the ordering contract, not a spurious ack. `rename`,
`mkdir`/`mkfile` (dialog paths), and `delete` `mode` DO drive the FE and follow the normal ack/round-trip rules
(`executor/CLAUDE.md`).

## 4. Milestones

Sequential is fine. M1–M3 are independent of each other and could run as parallel worktree efforts if ever desired, but
they all touch `tool_registry.rs` + the snapshot fixture, so sequential avoids merge noise. M4 depends on nothing; M5
depends on all.

Every milestone ends with: `pnpm check rust` (scoped iteration: `cargo test mcp::` from `apps/desktop/src-tauri`), plus
`pnpm check` at the milestone boundary, and the `tool_snapshot_tests` fixture updated intentionally (never
accidentally). Docs per milestone are listed inline; all land in the same commit as the code per `docs.md`.

### M1 — Indexing: resource rewrite + control tool + await condition

The headline gap, named first by David.

1. Rewrite `resources/indexing.rs` per-volume (3.3). **TDD (red→green)**: builder unit tests first — a fake status
   provider trait (or injected snapshot struct) returning multi-volume fixtures; assert the per-volume text, the
   `?volume=` deep view, and the "no volumes indexed" empty state. The current builder calls a global; introduce a thin
   injectable seam (same pattern as `select_log_lines`).
2. `indexing` tool (3.1): handler in a new `executor/indexing.rs`, registry entry, gate `Always`. Tests: registry
   schema + gate tests (written with the entry, snapshot-style), handler param-validation unit tests. The handler calls
   the typed commands and maps `EnableIndexingOutcome` to honest text. Per the ordering contract (3.2),
   `rescan`/`enable` return only after the status has left the pre-scan state — verify what `rescan_drive_index` already
   guarantees and add the wait only if it returns before the flip.
3. `await` `index_status` condition (two-field form: `volumeId` param + status `value`, per 3.2). **TDD**:
   condition-evaluation unit tests first (incl. an MTP volume id containing colons), then wire the subscription.
4. E2E (Playwright, `pnpm check desktop-e2e-playwright`): one spec — read `cmdr://indexing`, `rescan` the fixture
   volume, `await index_status` fresh. Written after (it's a wiring proof, not risky logic).
5. Docs: `mcp/CLAUDE.md` module map + `DETAILS.md` resources/tools sections; `resources/indexing.rs` module doc; note in
   `indexing/DETAILS.md` that MCP consumes the status APIs (consumer note, one line).

### M2 — Importance resource

1. `cmdr://importance` (3.3). **TDD**: builder unit tests against a temp `importance.db` written via the store's own
   writer (fixtures exist in `importance/fixtures.rs` — reuse `SyntheticHome` where practical). Cover: scored path with
   explain breakdown, floored path with reason, unscored, top-N, threshold cap + truncation note, missing/never-scored
   volume (honest "no importance data for volume X" text).
2. Registry: resource entry + `resource_tests.rs` count/URI updates.
3. Docs: `mcp/DETAILS.md` resource section; one consumer line in `importance/DETAILS.md`. Mention explicitly: reads via
   `ImportanceIndex` only (its CLAUDE.md invariant), never raw SQLite.

### M3 — Operation queue: visibility + control + await conditions

1. `cmdr://state` `operations:` section (rename from `transfers:` + queued/paused entries, 3.3). **TDD** on the pure
   YAML builder (`resources/transfers.rs` grows or is renamed `resources/operations.rs`): fixtures for
   running/paused/queued mixes.
2. `queue` tool (3.1) incl. the new rollback gate variant. **TDD for the gate**: `TokenGate` unit tests for the new
   variant first (the auth classification is the risky part), then the structural "rollback ⇒ gated" test, then the
   handler. Handler wraps `pause_operation` / `resume_operation` / `cancel_operation` / `cancel_operations` /
   `cancel_write_operation(id, rollback)` / `pause_all` / `resume_all`.
3. The terminal-ops ring (3.2): a bounded store populated from the terminal outcome events (`write-complete` /
   `write-cancelled` / `write-error`; NOT `operations-changed`, whose post-removal snapshot never carries a terminal
   status). **TDD**: ring unit tests first (capacity, ordering, unknown-id lookup, status mapping per event kind).
4. `await` `operation_complete` + `operations_idle`. **TDD** on condition evaluation against op-status snapshots + the
   ring; terminal-status reporting in the result text; unknown-id honest error.
5. `operationId` in the auto-confirmed `copy`/`move`/`delete`/`compress` OK responses via the round-trip correlation
   design in 3.2 (a real task: MCP event carries `requestId`, FE replies with the spawned id — touches
   `mcp-listeners.ts` and the dialog-confirm path). **TDD** on the response parsing; E2E in item 6 proves it end-to-end.
6. E2E: start a slow fixture copy via MCP with `autoConfirm` (capturing the returned `operationId`), `queue pause` it,
   assert paused in `operations:`, `resume`, `await operation_complete`. Written after.
7. Docs: `mcp/CLAUDE.md` (the gate list gains the rollback variant — that's a must-know), `DETAILS.md`, and the
   `write_operations` DETAILS gets a one-line MCP-consumer note (including that MCP holds a terminal-ops ring off the
   settle event).

### M4 — File-op completeness: rename, named create, trash mode, tag, eject, favorites

The bundle of smaller tools; each follows the mcp-development.md two-edit recipe (handler + registry entry) plus FE
listener where the flow drives the UI.

1. `rename` (3.1). The FE side: `mcp-listeners.ts` handles `mcp-rename` → starts the inline rename editor prefilled
   (non-autoConfirm path). **TDD** on the handler's param validation + the autoConfirm path's backend call/ack; the FE
   prefill is covered by an E2E spec (open rename prefilled, assert editor state via `cmdr://state` typeToJump-style
   sync or the existing rename-editor test hooks — implementer picks the honest observation point, no arbitrary sleeps).
2. `mkdir`/`mkfile` `name` + `autoConfirm` + gate change (3.2). **TDD**: red first on the gate test (they move to
   `IfAutoConfirm` — the full-table gate test fails until updated deliberately), then handler paths (prefill dialog /
   direct create / conflict error).
3. `delete` `mode` param (3.2): plumb the toggle preset through the delete-dialog event; autoConfirm path routes to
   `trash_files` vs `delete_files`. Unit tests on param mapping; the destructive paths already have E2E coverage —
   extend one spec with `mode: trash`.
4. `tag` (3.1): handler over `tags::toggle_color`/`set_tags` (macOS), typed color enum, Linux honest error. Unit tests
   incl. the color-name mapping; `[tags:…]` marker in the state file formatter (**TDD**, it's a pure formatter).
5. `eject` (3.1): wraps the eject command; error passthrough for busy/non-ejectable. Unit tests on param validation; no
   E2E (ejecting real volumes in CI is hostile).
6. `favorites` (3.1) + `favorites:` state section. Unit tests on the YAML builder + handler params.
7. Docs: `mcp/DETAILS.md` tool catalog; `dialogs/available` sync check (3.3 last bullet) rides along here.

### M5 — Streamlining + description pass + doc sync

1. `cmdr://state` volumes restructure (3.3) — **TDD** on the volumes YAML builder with local/SMB/MTP/virtual fixtures.
2. The tool-description rewrite (3.2 last bullet) — one deliberate snapshot-fixture churn; each description reviewed
   against the style guide's agent-copy sensibilities (active voice, says when to use it).
3. Optional, judgment call at execution time: `open_select_dialog` mirroring `open_search_dialog` (the Select dialog
   gained AI selection in 0.23; programmatic `select` covers most agent needs, so add only if it's cheap — the dialog
   executor path is shared).
4. Full doc sync: `mcp/CLAUDE.md` (tool count, gate list, module map), `mcp/DETAILS.md` (catalog, decisions from this
   plan's § 2–3 rationale — notably the gate line and the no-content-read decision), `mcp-development.md` (nothing
   structural changed, verify examples still compile), `AGENTS.md` untouched. Also fix the pre-existing drift found
   during planning: both docs still say "32 tools" (registry ships 33, `compress`), and `DETAILS.md`'s `tab` action list
   is missing `reopen`.
5. Final `pnpm check --include-slow`, real-app QA per David's preference (he QAs UI-adjacent flows himself; the
   rename-prefill flow is the one visual to show).

## 5. Testing philosophy for this effort

- **Red→green TDD** where the logic is risky or classification-critical: the `TokenGate` variant, `await` condition
  evaluators, and every pure YAML/text builder (they're value-in/value-out, cheap to test-first, and regressions there
  silently mislead agents).
- **Written-with (not test-first)**: registry schema/gate entries (the structural tests catch drift by construction),
  thin handler wrappers over typed commands.
- **E2E, written after**: one per milestone where the flow crosses FE (M1 rescan, M3 pause/resume, M4 rename prefill).
  Keep them on fixture volumes/files only; no arbitrary sleeps (`cmdr/no-arbitrary-sleep-in-e2e`).
- The snapshot fixture (`tool_snapshot_tests`) is updated once per milestone that touches the wire, in its own commit
  hunk with the change called out in the commit body.

## 6. Risks and open questions

- **`queue` gate on plain cancel**: `Open` per the § 2.1 line (transient runtime action; the pipeline preserves the
  source until the copy leg is durable, so no data loss). The strongest counter-case is canceling a **move** mid-run:
  crash-safe (source intact) but it leaves the operation half-applied across two locations — annoyance-plus, not data
  loss. If real-world use disproves that judgment, gating `cancel` is a one-line registry change; decide finally at M3
  review.
- **`index_status` freshness source**: freshness has ONE transition table (`indexing/freshness.rs`) — the await
  condition must read the same store the FE badge reads, never re-derive. Implementer: find the exact read API during
  M1; if only an event exists, subscribe and cache.
- **Terminal-ops ring lifetime**: capacity ~20 like `listing_errors`; a busy batch session can push a settled op off
  before a slow agent awaits it. Acceptable (the error is honest), but the ring capacity is a constant to revisit if
  agent flows hit it.
- **Rename prefill FE affordance**: smallest honest version is an event → the existing inline-rename entry point with an
  initial value. If the editor can't take an initial value cheaply, fallback: `rename` without `autoConfirm` just starts
  a plain inline rename (no prefill) and the description says so.
- **`cmdr://importance` volume enumeration**: listing "all scored volumes" needs a registry of importance DBs on disk
  (they outlive mounts by design). `WriterRegistry`/store layout knows; keep the read offline-capable.
- **Description-pass scope creep**: timebox it; it's a copy pass, not a redesign.

## 7. Relationship to the agent effort (so nobody re-litigates)

This plan ships the _UI-parity substrate_ richer and current. The agent effort later adds: the transport-agnostic
registry core (agent-spec §11.1), the agent-first tools (knowledge/proposals/memory/notify, §11.2), and `read_file` with
its guardrails (§11.3). Everything here is designed to be inherited: typed backend calls behind thin handlers,
volume-id-first params, and gates that map cleanly onto "proposal-gated" semantics. One thing the agent effort must do
structurally, not by policy: D26 ("proposals are the only write path, safety **by construction**") means the in-process
agent must have the direct write tools (autoConfirm file ops, `tag`, `favorites`, `indexing`) **gated off per-consumer**
(the capability gating agent-spec §11.1 names), while MCP/dev clients keep them. "The agent holds the token but chooses
not to use it" would be policy, not construction — don't ship that.
