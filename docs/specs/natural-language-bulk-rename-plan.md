# Natural-language bulk rename plan

## Outcome

Let someone ask Ask Cmdr to rename a set of image files in natural language, then review and approve each exact rename
before Cmdr changes anything. The first use case is screenshots in a folder such as `~/Downloads`, using the local image
index's OCR text and Vision tags to produce useful names.

The model proposes. The user approves. Applying approved rows uses Cmdr's managed write-operation engine and records one
durable rename operation for later rollback work.

## Scope

- **Inputs**: the focused pane's selected files, when there is a selection, otherwise its direct file children. The user
  asks in the Ask Cmdr rail in ordinary language.
- **Intelligence**: the agent enumerates the candidate files and uses `image_facts` only when OCR or image tags would
  improve the names. It proposes a structured batch of new names and preserves file extensions unless the user
  explicitly asks otherwise.
- **Review**: a modal lists original and proposed names in a scrollable, keyboard-operable table. Each row can be
  allowed or denied; Allow all and Deny all act on the visible proposal. Cancel discards the whole proposal. OK applies
  only allowed rows.
- **Writes**: same-folder file renames only. No move, folder creation, deletion, overwrite, reorganization, or agent
  approval capability.
- **Limits**: at most 200 proposed rows, matching `image_facts`' path cap. The agent asks the user to narrow or split a
  larger request.

## Non-goals

- Arbitrary write tools, tool-driven approval, or an agent-controlled apply path.
- Recursive bulk rename, folders, non-image naming without image-index facts, or a generic proposal framework.
- Editing generated bindings by hand, changing the proposal-access structural tests, or implementing rollback UI. The
  existing operation journal captures the operation now; its later rollback surface consumes it.

## User flow

1. The user selects files, or opens a folder, opens Ask Cmdr, and describes the desired naming convention.
2. The agent reads `app_state` to learn the focused pane and effective scope. `app_state` gains the focused pane's
   `volumeId` and exact selected file entries/paths, not merely its selection count, so the agent can correctly act on a
   selection. If a selected index is outside the cached pane entries, the scope is unrepresentable and the tool refuses
   rather than silently omitting it.
3. The agent gets the in-scope direct children through `list_dir` when no selection is present. The current-folder
   listing is usable immediately; the agent does not wait for a drive scan to finish.
4. `image_facts` is optional enrichment. If image indexing is off or a file has no facts yet, the agent continues with
   names, dates, and other available metadata instead of withholding a rename proposal.
5. The agent calls `propose_rename_plan` with the source path, volume id, and a destination file name for every row. The
   tool validates and stages a proposal but performs no filesystem mutation.
6. Ask Cmdr receives a typed `ProposalReady { proposalId, snapshot }` stream event and opens the review dialog. The rail
   retains the normal tool status line and the agent can explain what it used to derive the names.
7. The user allows or denies rows. The dialog sends only the opaque proposal id and allowed row ids to one bounded
   backend preflight command whenever that allowed set changes. It marks malformed, missing-source,
   duplicate-destination, and external-target conflicts with an actionable reason. Valid case-only renames and cycles
   stay reviewable.
8. OK creates one managed bulk-rename operation. It sends only the opaque proposal id and allowed row ids, revalidates
   immediately before each write, updates the listing, records one operation-log header with per-file entries, and
   reports completion or the exact rows that did not run.

## Proposal contract

### Tool registration

Add one `mcp_tools!` registry entry:

- **Name**: `propose_rename_plan`.
- **Consumers**: `Agent` only.
- **Access**: `Access::Propose`.
- **Handler home**: `agent/tools/propose/rename.rs`, alongside its schema and typed input/result models.
- **Agent wiring**: add the `ToolId` variant, wire name, `ToolId::KNOWN` entry, tool label in `ask-cmdr-labels.ts`, and
  the literal name in `EXPECTED_PROPOSE_TOOL_NAMES`. Do not change the structural test's shape or weaken its assertions.
- **Refusal copy**: update `agent/tools/view.rs` from "Ask Cmdr is read-only" to explain that Ask Cmdr can prepare
  changes for review but cannot make them itself.

The existing dispatch gate already permits `Access::Read | Access::Propose` and rejects `Write`. Keep that gate and the
agent's lack of any approval tool unchanged.

### Agent instructions

Add a narrowly scoped rule to the system/tool instructions. For a natural-language image rename request, the agent must
use only the focused scope; get exact selected entries from `app_state`, or list the focused directory when there is no
selection; treat that listing as ready without waiting for indexing; use `image_facts` only when it improves the names;
preserve extensions unless the user explicitly requests otherwise; and submit a final plan only through
`propose_rename_plan`. The proposal tool is always available and is never gated on indexing state.

### Input and handler validation

The input is a single `renames` array, capped at 200:

```text
{
  renames: [{ sourcePath, volumeId, destinationName }]
}
```

`destinationName` is a filename, never a path. The handler rejects separators, `.` and `..`, blank names, invalid
filenames, duplicate sources, duplicate destinations under Cmdr's platform comparator, and an empty proposal. It also
rejects a row unless all of these hold:

- Its source is a non-directory file in the effective scope captured from `PaneStateStore`: the current selection when
  one exists, otherwise a direct child of the currently focused folder. v1 renames a selected symlink as the link, not
  its target, because pane state does not prove a richer file kind without touching the live filesystem.
- Its destination remains in that source's parent directory.
- Its volume matches the source's focused-pane `volumeId`.
- Archive-internal paths return a typed refusal. Image indexing state never affects proposal availability.

Proposal-time validation is entirely cache/state based. "Canonical" means normalized, validated data in a stable typed
shape, never `std::fs::canonicalize`: agent tools do not touch live mounts, follow symlinks, or probe the filesystem.
The handler returns a typed `imageIndexingOff` outcome instead of staging anything when indexing is disabled. It does
not call rename APIs, create temporary files, or probe unbounded directories.

### Proposal lifetime and authority

Add a feature-local in-memory `RenameProposalStore`, keyed by an unguessable proposal id. It stores the immutable
accepted rows and their opaque row ids. This is not a generic proposal framework.

- **Staging**: the proposal handler produces a dispatch outcome containing a concise model-facing `AgentToolResult` and
  an internal `RenameProposal` snapshot. The runtime persists only the former in the chat transcript, puts the latter in
  `RenameProposalStore`, and emits `ProposalReady` from its turn sink.
- **Frontend boundary**: the event snapshot is display-only. Cancel consumes and discards the proposal. Preflight and
  Apply accept only `{ proposalId, allowedRowIds }`; paths, names, volumes, and validation fields never round-trip from
  the frontend as authority. The store records the latest successful preflight's exact allowed-id set and its
  fingerprints; Apply accepts that exact set only, otherwise it reruns full preflight.
- **Expiry**: close the proposal on Cancel or terminal Apply, and expire abandoned proposals after a short bounded
  lifetime. An expired id returns a typed "review expired, ask Cmdr to prepare it again" result.

### Agent-to-frontend handoff

Extend the existing Ask Cmdr runtime and IPC stream with `ProposalReady { proposalId, snapshot }`. `ToolDispatcher` must
return the split dispatch outcome above, because today's handler returns only `ToolResult` and cannot emit a chat event
itself. `run_turn` emits the event only after it has stored the immutable proposal. The event is the review-surface
trigger, not an approval or a write.

Keep the model-facing `AgentToolResult` concise: it confirms that a named number of rows is ready for review and carries
per-row validation refusals where useful. It does not contain the canonical payload because tool results are persisted
and returned to the provider. The frontend receives its display snapshot through the stream event, not by parsing
assistant prose or a tool-result string.

## Review dialog

Create `BulkRenameReviewDialog` under the Ask Cmdr feature and mount it beside `AskCmdrRail`, not in the pane-local
`DialogManager`: the proposal belongs to an agent conversation rather than one explorer command.

- **Header**: say how many files the review covers and state that Cmdr will rename only the allowed rows.
- **Rows**: include a checkbox or Allow/Deny control, original name, proposed name, and a validation status. Long names
  truncate visually but expose their complete value to assistive technology.
- **Bulk controls**: Allow all and Deny all change every currently reviewable row. Blocked rows cannot be allowed until
  their condition is resolved; the initial state is allowed only for valid rows.
- **Keyboard and a11y**: focus enters the row list, Space toggles the focused row, bulk buttons and footer buttons have
  visible focus, and the list announces its allowed/blocked counts.
- **Footer**: Cancel closes and forgets the staged proposal. OK is disabled with no allowed valid rows. Its label states
  the number of files it will rename.
- **Copy**: all user-facing strings go through the Ask Cmdr i18n catalog with translator descriptions. Use the shared
  `ModalDialog`, `Button`, and existing list/scroll tokens; support dark mode and reduced motion.
- **Dialog contract**: add `bulk-rename-review` to `SOFT_DIALOG_REGISTRY` and pass that `dialogId` to `ModalDialog` so
  focus trapping, Escape, app-wide dialog state, and MCP dialog inspection remain intact.

The dialog owns the user's decisions. Neither the agent response nor a later tool call can change an allowed/denied
state or reopen a cancelled proposal.

### Backend preflight

Add one typed, asynchronous, timeout-guarded `preflight_bulk_rename(proposalId, allowedRowIds)` command. It reads the
server-owned proposal, checks all allowed rows together on the appropriate blocking/volume path, and returns one typed
status per row. The dialog reruns it whenever an allowed decision changes, because the rename graph can change with that
subset. The frontend has no filesystem authority and must not call the one-row rename-validity command 200 times.

Preflight builds a rename graph over the allowed rows:

- Duplicate final targets are blocked.
- A target owned by another allowed source is a valid dependency, including a cycle.
- A same-source case-only rename is valid and requires a temporary name.
- A target that already exists outside the allowed source set is blocked, with no overwrite option.

It captures a user-action-time source fingerprint for Apply: local files use device/inode plus metadata; SMB uses its
normalized volume-relative path plus current metadata, because SMB has no portable inode contract. Apply verifies the
matching backend-specific fingerprint and file kind again before renaming.

## Managed apply

Add a focused batch operation in `file_system/write_operations/rename.rs`, rather than calling `rename_managed` once per
row from the frontend. One batch must have one operation id, one operation-manager lifecycle, and one operation-log
header with `item_count = allowed rows`; each source-to-destination pair is an item row.

The operation:

1. Receives only `proposalId` and allowed row ids through a typed Tauri command with `Initiator::Agent`. The command is
   a normal user-initiated command, asynchronous and timeout-guarded like other filesystem commands.
2. Looks up the server-owned proposal and the accepted preflight fingerprint, then revalidates source identity, filename
   validity, volume membership, conflicts, and duplicate targets. It refuses conflict overwrite. Any row that has
   changed since review is skipped and reported, never guessed at.
3. Creates `WriteOperationState`, an operation event sink, and one queued `OperationDescriptor`; use
   `manager::spawn_managed`, not `rename_managed` / `run_instant`. The operation returns its id immediately, reserves
   its volume lane when admitted, reports row-count progress, and honours cancellation between filesystem calls.
4. Renames in a collision-safe dependency order. Independent rows rename directly; acyclic chains run backward from
   their free destination; each cycle uses one unique, same-directory temporary; and case-only paths retain one
   temporary on case-insensitive filesystems. The review marks cycle rows and explains the temporary rotation. On a
   mid-operation stop, it records completed and skipped rows honestly; it does not attempt an unreviewed rollback.
5. Opens one operation-log header on admission with `item_count = allowed rows`, writes one item row per final outcome,
   and reuses the existing local/volume rename routes, Downloads watcher write-ignore handling, listing notifications,
   busy state, and journal helpers. It never writes through a raw filesystem shortcut.

The first shipped scope is indexed local volumes and opted-in indexed SMB volumes. MTP cannot produce an image-facts
proposal because it has no background media index. Archive-internal paths are refused explicitly in this feature's first
version rather than accidentally invoking archive mutation semantics.

## Implementation milestones

1. **Scope and proposal tool**: add selected-entry plus volume-id reporting to `app_state`; implement the typed
   `Propose` tool, feature-local proposal store, split dispatch outcome, registry/`ToolId`/label wiring, 200-row cap,
   scope and image-index gates, agent instructions, and the narrow `ProposalReady` stream event. Update the
   proposal-name fixture only.
2. **Review UI and preflight**: add proposal state to Ask Cmdr, the registered accessible review dialog, i18n messages,
   row/bulk decisions, expiry/cancellation handling, and the bounded backend preflight command with its typed statuses.
3. **Batch write operation**: add the typed id-only apply command and `spawn_managed` batch rename with collision-safe
   sequencing, backend-specific revalidation, progress/cancellation, operation-log rows, and list refresh. Wire the
   dialog's OK action to it.
4. **Verification**: exercise the red-to-green unit seams, proposal-store lifecycle tests, graph tests for chains,
   swaps, cycles, case-only paths, targets appearing after review, and cancellation; frontend dialog/a11y tests; and a
   focused desktop E2E that uses a fake image-facts-backed plan. Run
   `cargo mutants --file src/file_system/write_operations/rename.rs` or the then-current scoped equivalent after the
   substantial write-engine change, `pnpm check -q` after each milestone, and `pnpm check --include-slow -q` before
   handoff.

## Acceptance scenarios

- A selected group of screenshots receives distinct, valid same-folder names based on their OCR/tags; the user can deny
  one row and only the allowed rows change.
- With no selection, the agent considers only direct files in the focused folder, not descendants or the other pane.
- With Image indexing off, a filename- or date-based proposal still opens; the agent only omits image-derived detail.
- A plan with 201 rows, a destination containing a path separator, a duplicate target, or a source outside scope is
  refused before review.
- A target that appears after review, a changed local or SMB source, a case-only rename, a chain, and an A-to-B/B-to-A
  swap are all safe and transparently represented.
- A selected file that has scrolled out of the pane's cached entries is refused rather than omitted from a plan.
- Cancel and Deny all produce no write operation. No agent tool can approve or apply a proposal.
- The operation log shows the successful bulk rename as one operation with one row per file, ready for the existing
  rollback work when that UI ships.

## Resolved choices

- **Why an agent-only `Propose` tool**: it keeps the proposal on the agent boundary and prevents the existing external
  AI-client MCP surface from gaining a new mutation-adjacent capability.
- **Why a modal, not chat buttons**: reviewing filenames needs a scrollable, columnar surface and independent row
  controls. The chat remains the place to state intent and explain the plan.
- **Why a dedicated batch operation**: repeated single-item `rename_managed` calls would create fragmented operation-log
  records, cannot safely solve rename cycles as a group, and gives poor cancellation/progress semantics.
- **Why proposal ids are server-owned**: a review dialog must never turn the frontend into a second plan author. Opaque
  ids plus allowed row ids preserve the exact plan the agent proposed while leaving every approval decision with the
  user.
