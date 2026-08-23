# Ask Cmdr rail details

Pull-tier docs for `lib/ask-cmdr/`. Must-knows live in `CLAUDE.md`. Backend:
`apps/desktop/src-tauri/src/agent/CLAUDE.md` and `apps/desktop/src-tauri/src/commands/agent/`.

## The IPC surface

Wrappers in `../tauri-commands/ask-cmdr.ts`:

- `sendAskCmdrMessage(conversationId, text, attachments, deniedNames)` — a plain specta command. `conversationId` is
  `null` for a new thread; the resolved id comes back as the promise value, and the answer arrives on the subscription
  below. It answers an `AskCmdrSendOutcome`: `accepted` with the id, or a typed refusal decided before the turn existed
  (no consent, no slot, a local window too small). Those can't be streamed — half of them happen before there is a
  thread to key an event on.
- `onAskCmdrTurn(cb)` — the subscription every turn's progress arrives on, rail sends and wakes alike.
- `agentWakeStatus()` + `onAgentWakeStatus(cb)` — the wake indicator's seed and its subscription. See § The wake
  indicator.
- `cancelAskCmdr(id)`, `getAskCmdrConversation(id, limit, offset)`, `listAskCmdrConversations(...)` — plain specta
  commands.
- `preflightBulkRename` / `reviseBulkRenameRow` / `applyBulkRename` / `cancelBulkRenameProposal` — the review dialog's
  four actions. Only opaque ids cross, plus the one name the user typed.

Nothing here is hand-mirrored: `AskCmdrTurn` / `AskCmdrStreamEvent` are generated like everything they carry
(`MessageView` / `MessageBlock` / `ConversationRow` / `ConversationDetailView`, and the review row's
`RenameProposalRowSnapshot` / `RenameEvidence` / `EvidenceCoverage`, re-exported as `RenameProposalRow` /
`RenameEvidence` / `RenameEvidenceCoverage`, the names the rail uses). ❌ Don't hand-mirror a type that has a generated
counterpart; that's a drift seam.

## The streaming model

**Subscribed by conversation, not by send.** One listener per window (`ask-cmdr-turn-stream.svelte.ts`, started from
`routes/(main)/+page.svelte` so it exists in the MAIN window only) hears every turn the backend is running and hands
each event to two places: `handleTurnEvent` in the stream slice, which keeps only the ones about the thread on screen,
and the sessions slice, which reacts to a thread appearing or going away. That module is the one place allowed to know
both, since the sessions slice calls into the trigger and the trigger never imports it back.

Three things follow from the subscription being conversation-keyed rather than per-invoke, and each has a test in
`ask-cmdr-turn-stream.test.ts`:

- **A reload keeps the answer.** The webview goes away mid-turn and the backend keeps writing into `main.db`; the
  reloaded rail lands on the same thread and the rest of the turn renders. There is no second `assistantStarted` to
  hear, so `adoptLiveTurn` treats ANY live event for the thread on screen as proof a turn is running, and the reducer
  creates the streaming bubble if there isn't one (`ensureAssistant`).
- **A thread can disappear under the rail.** A quiet wake opens a thread, thinks in it, and deletes it seconds later;
  `discarded` is the only way a subscriber can learn that, since there is nothing left to re-read. The rail steps off
  into an empty chat and the session list drops the row.
- **A stopped turn stays stopped.** A cancel gets no terminal event back, so the backend dribbles a chunk or two more.
  Those would otherwise read as a live turn and re-enter "working…" with nothing coming to clear it, so the thread goes
  on a stopped list until it is sent to again.

`sendMessage` optimistically appends a `{ kind: 'user' }` item and flips `streaming` on, then calls
`sendAskCmdrMessage`. Events drive the render (`applyStreamEvent`), each delegating to a tiny mutator so the switch
stays simple:

- `started` → the thread exists and is being worked on. A fresh chat has no id until this arrives (or until the send's
  promise resolves, whichever is first); a wake emits it too, which is what tells the session list.
- `assistantStarted` → push a streaming `{ kind: 'assistant', text: '', tools: [] }`.
- `textDelta` → append to the last assistant's `text`; clear its `thinking`.
- `reasoningTick` → set the last assistant's `thinking` (a subtle "thinking…" line; the reasoning content itself never
  crosses).
- `toolCallStarted` / `toolCallFinished` → push / update a `RailToolCall` (the collapsible "looked at X" line;
  `ok = false` is a refusal or handler problem).
- `done` → finalize the bubble, stamp its persisted id, `streaming = false`.
- `failed` → drop an empty bubble, push a typed `{ kind: 'error' }` item, `streaming = false`. The item carries the
  source error's own wording (`detail`, when the backend has one — a retired model slug, a quota reset time) under the
  friendly headline, rendered as escaped plain text (never `{@html}`), so the user sees what to fix. Display only: the
  UI branches on `errorKind`, never on `detail`.
- `modelChanged` → insert a `{ kind: 'modelChange' }` timeline line BEFORE the current user bubble (the switch happened
  between the turns; the backend already persisted the event row).
- `contextTrimmed` → insert a `{ kind: 'contextTrimmed' }` timeline line before the streaming bubble: the prompt budget
  pushed older tool results out of this turn, so the reply that follows saw less than the whole chat. At most one per
  turn (the backend dedupes). Live-stream only, deliberately NOT persisted: it describes one assembly, not thread
  content, so reloading the thread doesn't replay it. Rationale (why the drop must be loud at all):
  `src-tauri/src/agent/chat/DETAILS.md` § Budget enforcement.

Every terminal path uses the same assistant finalizer. It clears thinking/stalled state and removes tool rows that never
received `toolCallFinished`, while retaining completed tool history. This also covers local cancellation, the
progress-watchdog timeout, and a send invocation that rejects before stream events can report a typed failure.

**Model-change events, live path.** `settings-applier.ts` calls the trigger's `noteModelSettingChanged()` on the four
model-affecting settings (`ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` / `askCmdr.interactiveModel`),
which debounces 1 s (outlasting the settings store's 500 ms disk flush, which the backend re-reads, and the model text
field's keystrokes) and then calls `ask_cmdr_record_model_change` for the active thread. The backend queues on the
thread's single-flight lock — with a turn in flight the promise resolves right after that reply — and answers the
persisted event view, or `null` when nothing effectively changed (no turn yet, same model, or the interactive override
masks the changed shared model). A resolution that arrives after the user switched threads is dropped locally (the row
still shows on revisit). History renders the same lines via the `event`-role fold in `buildRailMessages`. Backend
mechanics: `src-tauri/src/agent/chat/DETAILS.md` § Model-change events.

**Cancel finalizes locally.** The runtime returns `Cancelled` with no terminal event, so `stopStreaming` cancels the
backend AND finalizes the current bubble itself, then puts the thread on the stopped list so the chunks still in flight
are ignored rather than re-adopted as a live turn.

History loads through `getAskCmdrConversation` on rail open (bootstrapping the most recent thread) and folds `tool`-role
result rows into their assistant tool line by `callId`, so the thread shows one line per call.

## What a wake's thread opens with

A thread the agent started for itself begins with a `wakeDigest` block in the user-role row rather than text, so
`buildRailMessages` folds it into its own `RailMessage` and `AskCmdrWakeDigest.svelte` renders it collapsed.

**Everything in that block is ours to word.** The backend sends folders, four counts each, and the rollups — numbers and
paths, no sentence (`agent/wake/DETAILS.md` § The rendered digest is prompt-only). The English digest the model reads
never crosses IPC, so nothing here can leak untranslated backend copy into ten locales. ❌ Don't "simplify" this by
having the backend send a ready line.

Two things the block owes the reader: the collapsed summary counts the rolled-up folders too (otherwise it disagrees
with what expanding shows), and the rollup line stays, because admitting how many folders the digest had no room for is
the point of having one.

**Live, the digest arrives on the next load, not mid-turn.** `userPersisted` carries an id and no content, so a rail
opened onto a wake already in flight shows the answer streaming above an empty spot until the thread is re-read.
Widening that event to carry the block is the fix if it ever matters.

## The staged-proposal toast

The one time the proactive agent interrupts. `agent-wake-staged` (`src-tauri/src/agent/wake/staged.rs`) fires when a
wake's turn ends having staged at least one proposal; `wake-toast.svelte.ts` turns it into a toast and
`WakeStagedToastContent.svelte` renders it, started from `routes/(main)/window-services.ts` in the main window only (the
event reaches every window, and the settings window would otherwise raise its own copy).

- **`askCmdr.wakeToast` is read at ANNOUNCE time**, never at subscribe time, so turning it off silences the wake already
  in flight. That is the only reading of the switch that does what somebody flipping it mid-wake meant.
- **The backend never emits for zero.** A quiet wake, and a wake whose model proposed nothing, say nothing at all.
- **It AUTO-DISMISSES**, unlike the operation-failure toast. Nothing is lost when it goes: the proposals sit in the
  suggestions badge until they are reviewed, so a persistent toast would just be a thing to close.
- **Two actions, answering different questions**: "Review" opens the suggestions (WHAT it wants to do), and "See why"
  opens the thread it reasoned in (WHY). Nobody asked for any of this, so the second one earns its place. ⚠️
  `switchToThread` before `openRail`, for the same reason the indicator's click does it that way.
- Grouped under `agent-wake-staged` with a cap of two, so a run of wakes can't push unrelated toasts off the screen.

## The wake indicator

The status corner's word on the proactive half: `wake-indicator.svelte.ts` holds the state and the subscription,
`WakeIndicator.svelte` renders it, and `routes/(main)/window-services.ts` starts it in the main window only (the event
reaches every window, and only this one has a corner).

**Its own event, not the turn stream.** `agent-wake-status` carries a `WakePhase` (`idle`, or `thinking` with the
conversation id) plus the readiness gap. The turn stream carries a turn's PROGRESS to whoever is showing that thread;
this carries a phase to a corner showing no thread at all, so folding them would subscribe the corner to every text
delta of every rail send. The one read at startup is not redundant with the subscription: a wake already running when
the window opened announced itself before anyone was listening, and so did a gate that closed before then.

### What renders, and what does not

`wakeIndicatorMode` is the whole decision, and it exists because two docs used to state the rule differently.
`agent/wake/readiness.rs` held that every readiness gap is worth reporting: somebody who declined Full Disk Access and
somebody with a tidy Downloads folder otherwise see the identical nothing, and only one of those is the feature working.
`SuggestedOpsIndicator` held that an always-present control for a feature with nothing to say is noise. Read together
literally, they put a permanent AI nag in front of every user who never wanted AI.

The resolution: a gap is reported to somebody who opted IN and hit a wall, and to nobody else.

- `silent` — no consent, or `askCmdr.proactive` off, or ready and idle. Nothing is being watched, or nothing has
  happened, so the corner says nothing.
- `thinking` — a wake is on a provider right now. ⚠️ This one renders REGARDLESS of the setting: it is spending the
  user's money at that moment, and a forced wake (or a setting turned off mid-turn) must not be able to run invisibly.
- `needsFullDiskAccess` / `needsApiKey` — the two closable gaps, each with the screen that closes it (the system privacy
  pane, and AI > Provider).

### Its two actions

Clicking the thinking glyph runs `switchToThread(id)` and THEN `openRail()`. That order matters: a closed→open
transition otherwise bootstraps the most recent thread and wastes a fetch on one we are about to replace. The turn's own
events keep arriving on the conversation-keyed stream, so the rail fills in as the wake writes.

The stop button is `cancelAskCmdr(id)`, the same command the composer's Stop calls. A wake registers its cancel token in
the one registry (`agent/chat/cancel.rs`), so there is no wake-specific stop to keep in step — which is also why the
registry lives below `commands/` rather than inside the chat command.

## Sessions, search, message paging

- **Sessions panel** (`AskCmdrSessions.svelte`, opened from the rail header's "Chats" button) overlays the rail body
  (`position: absolute; inset: 0`) with a search box, an active/archived filter, and the thread list. Its state lives in
  a separate slice, `ask-cmdr-sessions.svelte.ts` (`sessionsState`), which calls the trigger's `switchToThread` /
  `newChat`; the trigger never imports it back (no cycle). Selecting a thread switches the rail and closes the panel.
- **List paging mirrors the operation-log dialog**: the offset is `conversations.length` (one source of truth), so an
  append can't overlap or desync; a full page (`SESSIONS_PAGE`) means "load more" is offered. Rename edits the row's
  title in place. The archived filter has two states: active-only (default) and "show archived", which shows ALL threads
  with archived ones badged (the backend `include_archived=true` returns everything, so the reverse label is "Hide
  archived", not "Show active"). `setArchived` drops a row only when archiving in the active-only view; in the all view
  a flip just updates the badge in place.
- **Search** is debounced (`SEARCH_DEBOUNCE_MS`) and guarded by a monotonic `searchSeq` so a slow earlier response can't
  overwrite a newer one. A non-empty query replaces the list with FTS hits (`searchAskCmdrConversations`); clearing it
  restores the list. Each hit's `snippet` is backend FTS text rendered as plain `{text}` (never `{@html}`).
- **A thread the agent opened wears a `bot` glyph**, the same one the status corner shows while a wake thinks, in the
  list AND in search results (`ConversationSearchHit.origin` exists for that second half: a hit that lost the mark reads
  as a thread the user started and forgot). ❌ The test is `origin === 'notification'`, never "origin is not null": the
  reserved `quietWakes` ledger row carries an origin too, and so will whatever token comes next.
- **Message paging is tail-first** (a chat shows newest at the bottom). `loadConversation` probes page 0 to learn
  `totalMessages`, then refetches the newest page when the thread exceeds `MESSAGE_PAGE`; `historyCount` tracks how many
  rows are loaded from the tail. "Load earlier" (`loadOlderMessages`) prepends the previous page, its offset derived
  from `messageTotal - historyCount` so pages tile without overlap and live-streamed rows (newer than the load-time
  total) are never disturbed. The rail preserves the scroll position across a prepend (capture `scrollHeight` before,
  restore after) and its auto-scroll-to-bottom only fires when the user was already near the bottom (`wasNearBottom`,
  tracked on scroll), so streaming follows but loading older doesn't jump. Page-boundary caveat: `buildRailMessages`
  folds each loaded page independently, so a tool result split across a page seam may render unfolded — negligible in
  practice (threads sit under the ~40 soft cap, well below a 50-message page, so paging rarely fires at all).

## Attachments by reference

- The composer stages `AttachmentRef { path, kind }` chips (`askCmdrState.attachments`), sent with the next message and
  cleared after. They ride into the context envelope as `attached: <path> (<kind>)` on the latest user turn — **path +
  kind only, structurally never contents** (the read-only privacy line). History user rows carry no chips (the refs were
  envelope text, not stored blocks).
- **"Ask about selection"** (the paperclip button) calls `ask_cmdr_selection_attachments`, which reads the focused pane
  from `PaneStateStore` (the same source the envelope uses) and returns its selection (or cursor item) as typed refs —
  no filesystem stat.
- **Drag-onto-composer is a NATIVE webview drag, not HTML5** (`ask-cmdr-drop.ts`): a Cmdr pane drag is delivered through
  `getCurrentWebview().onDragDropEvent`, so a DOM `ondrop` would never fire. The composer subscribes to that event and
  hit-tests its own rect (via `toViewportPosition`, mirroring the pane drag-drop controller). For an in-app drag the
  trustworthy source is the recorded self-drag identity (`getSelfDragIdentity`), not the pasteboard-round-tripped
  payload paths; only LOCAL (`'root'`) self-drags are supported (virtual-volume paths mis-resolve). A Finder drop uses
  the payload paths (genuine local absolute). Kinds are resolved backend-side (`ask_cmdr_resolve_attachments`) from
  known pane state, defaulting to file. The Tauri APIs load lazily and swallow failures, so the composer still mounts
  outside a Tauri webview (unit tests).
- Chips render the escaped basename (`attachmentBasename`) as plain `{text}` — filesystem names are
  attacker-controllable on a network share, so never `{@html}` (see the shared XSS-boundary rule).

## Layout, persistence, focus

- Hosted in a flex row (`.explorer-rail-row`) beside `DualPaneExplorer`: the panes take the remainder
  (`flex: 1; min-width: 0`), the rail its fixed px width. Below ~900px a media query flips the rail to
  `position: absolute` so it OVERLAYS the right pane instead of squeezing the panes below their min-width.
- Rail open flag + width persist via `app-status-store.ts` (`askCmdrRailOpen`, `askCmdrRailWidth`, clamped 280–520),
  mirroring `leftPaneWidthPercent`. `hydrateRail` applies them once at startup from `loadPersistedState` (reopening
  bootstraps the active thread).

## The toggle wiring

The rail's toggle lives in four places, and a miss fails silently (no error, just a shortcut or menu item that does
nothing):

- The frontend command registry, plus `COMMAND_IDS`, plus the `askCmdr.toggle` handler.
- Rust `command_map.rs`.
- The `macos.rs` / `linux.rs` View submenus.
- `shortcuts-store.ts` `menuCommands`.

The default is `⌘⌥A`, registered **Command-then-Option**: `⌥⌘`-order strings are native-menu-only, so writing the
shortcut that way in the registry leaves it unmatched. `ask-cmdr-shortcut.test.ts` pins all of it.

## Window growth (panes keep their size)

Opening the rail grows the MAIN window by the rail's width instead of squeezing the panes; closing shrinks it back.
`rail-window.ts` is the Tauri wrapper (`growMainWindowForRail` / `shrinkMainWindowForRail`) over the pure geometry in
`window-positioning-utils.ts` (`growRectForRail` / `shrinkRectForRail`, unit-tested). The main window's own
`capabilities/default.json` grants `set-size` + `set-position` (the read getters and `available-monitors` are already in
`core:default`).

- **Grows rightward** (left edge put, so the panes don't jump), **slides left** only when the right edge would leave the
  monitor, and **caps at the monitor width** — past that the panes do give up space (nowhere else to take it from). This
  is the "max width = screen width" case.
- **Fullscreen / maximized are left alone** (`fillsScreen` bails): the window already fills the screen, so the flex
  layout shrinks the panes — the same capped fallback.
- **E2E runs skip the resize entirely** (`isE2eRun()` guards both functions). E2E deliberately keeps the main window
  ordered to the back (`show_main_window` → `orderBack:`); a `setSize` / `setPosition` re-fronts the window, so it would
  pop over the developer's work and intercept clicks mid-run — the exact disruption the backgrounding exists to avoid.
  Skipping it costs nothing (no E2E spec asserts the window size).
- **Close reverses exactly what open did.** `growMainWindowForRail` records `{grewBy, shiftedLeftBy}`; close consumes
  it, so a manual window resize or a rail-width drag (absorbed into the panes) between open and close is preserved —
  only the rail's own contribution is removed. With no record (rail open at startup, so hydration never grew it — see
  below), close falls back to removing one rail width so a persisted-open window still shrinks.
- **Hydration must NOT grow.** `hydrateRail` calls `openRail({ resizeWindow: false })`: the window is restored by
  `apps/desktop/src-tauri/src/window_state` at its persisted (rail-inclusive) size, so growing again would double it.
  Re-opens (after consenting) also skip growth via the `!wasOpen` guard in `openRail`.
- The left-edge drag handle resizes (double-click resets to 340). Focus: an `$effect` focuses the composer on mount (the
  rail mounts on open); `markRailFocused` on composer focus; Escape → `returnFocusToPane`
  (`.dual-pane-explorer.focus()`).

## Rename review apply

`BulkRenameReviewDialog` owns the user's allow/deny decisions. Its Apply action sends only the staged proposal id and
the currently allowed row ids to `apply_bulk_rename`; it cannot supply a path, destination name, fingerprint, or
approval from the model. The backend requires that exact subset to have passed the latest preflight, rechecks it if the
client is stale, claims the proposal in one conditional transaction (so it can't be started twice), then returns a
queued operation id. The dialog closes only after that operation has started.

Backend preflight verifies every source still exists and blocks any missing source, including one removed after the
dialog opened. The dialog deselects that row and shows a red, accessible warning; a matching watcher event rechecks it
if the source returns. Preflight also supplies additive row warnings. The dialog keeps extension changes allowed but
marks them with an accessible yellow badge explaining that a rename does not convert file contents; dependency cycles
use the same warning channel. Its cycle tooltip explains that Cmdr uses one temporary name while rotating the files.
Blockers remain separate and automatically clear the row's Allow decision.

The dialog subscribes to the same `directory-diff` stream that updates the file panes. A change whose filename matches a
proposal source or destination reruns the authoritative preflight for every displayed row, including denied and
previously blocked rows. This keeps target-exists and source-missing warnings live and lets a row recover when an
external process removes the clash or restores a missing source; matching names are only an IPC filter, never filesystem
authority. `TargetExists` and `SourceMissing` rows are deselected and show specific red warnings, while the write
engine's exclusive final rename remains the data-safety boundary.

## Undo after a batch lands

Apply hands back a queued operation id, and `noteRenameApplied` turns it into a `renameApplied` rail line: "Renamed 23
files." plus an Undo. This is the only safety net that fires after the names are real, which is the only moment the user
can tell a name is wrong.

A line per batch, so a run of several reads as a run. Only the newest still-undoable line carries the job-wide "Undo all
N batches", and its `jobOperationIds` are built from the run's own lines — never from a previous line's set, which
already includes its predecessors (that bug shipped duplicate ids to the backend and a test caught it).

**The id order is the data-safety part.** `undoOperations` receives them in APPLY order and the backend reverses
newest-batch-first (`src-tauri/src/operation_log/rollback/order.rs`): a later batch can have renamed a file into a name
an earlier batch freed, and oldest-first then finds that name occupied, correctly refuses to overwrite, and leaves the
file unrestored. Apply order is also what breaks a same-second tie in the journal's whole-second clock.

The call resolves only when the reversal has actually finished, so the line reports what came back rather than claiming
success on dispatch. `rename-undo.ts` maps the report to a display state, and anything left behind outranks what
succeeded: a skipped file or a refused batch renders `partial`, never `undone`. A refused batch is counted apart from
skipped files because it carries no per-file numbers. Every line a job undo covered goes `unavailable`, so no line
offers an Undo the backend would now refuse. Live-stream only: undo needs the operation id, and a reopened thread could
otherwise offer an Undo for a batch reversed from the operation log meanwhile.

**Each reason gets its own line, naming the file when it applies to just one** ("Left `invoice-2026.pdf` alone: it
changed since the rename."). The backend carries a per-reason breakdown (`SkipBreakdown { reason, count, exampleName }`,
complete counts — see `src-tauri/src/operation_log/DETAILS.md` § Undoing a job); `undoStateFromReport` merges those
groups across the job's batches (counts sum; the first example wins, and the report arrives newest-batch-first) and
`undoSkipMessage` maps each typed reason to copy through an exhaustive record, so a new reason is a compile error until
it's decided.

Two catalog keys per reason (`.named` / `.counted`), not one ICU plural: "name the file" vs "count them" is a display
choice, and a locale whose plural has only `other` (`zh`, `vi`) couldn't express both from one message.

**Whatever no reason accounts for is still said by class**, with the leftover count ("Left 4 files alone: they changed…,
or the old name is taken again."). That covers a batch undone before the reason column existed (its rows read "reason
not recorded") and `alreadyGone`, which maps to no line because it counts as restored. A missing reason must never
shrink the skipped count the line admits to.

## Editing a proposed name

A row used to be allow-or-deny, so a plausible wrong name left the user two options: the model's name or the old one.
That's the pressure that produces "approved because it looked plausible", so the proposed name is a text field, and a
wrong name gets corrected in place.

- **The field is an edit buffer; the server is the authority.** `commitName` (blur, or Enter) calls the trigger's
  `reviseRenameRow`, which calls `revise_bulk_rename_row` and then writes the RETURNED row into state (name, evidence,
  coverage) before re-running the preflight. It never patches `destinationName` locally: the backend validates the name,
  swaps the row's evidence for `userEdited`, and clears the accepted preflight, so a locally-patched name would show a
  name the backend never took. After every commit the field is set back to the row's stored name, which is what reverts
  a refused edit. Escape abandons the edit (and stops propagating, so it doesn't close the review).
- **Never disabled.** Not while preflighting (a watcher event mid-typing would steal focus and drop the edit) and not on
  a blocked row — an occupied destination is fixed by typing a different name, so that's the row that needs the field
  most. A commit during an in-flight preflight is safe: `refreshRenamePreflight`'s `requestVersion` discards the older
  response.
- **A refused name says so on the row** (`nameRejected` → the red line plus `aria-invalid`), in ONE localized string.
  The backend's own wording is written for the model and stays in the log.
- **The row's provenance state is scannable** (`rename-name-provenance.ts`, unit tested): `contentRead` (an image
  source, so the backend verified something was read), `nothingRead`, `nameKept` (nothing read AND the name didn't
  change), `userEdited`. The last two badges exist because M4 tells the model to keep a neutral name when it couldn't
  read a file, and that instruction is worthless if the user can't see which rows took that path — so the copy keeps
  saying nothing inside the file was read, and never softens into something reassuring. A case-only change is a rename,
  not a kept name.
- A `userEdited` row renders the label alone ("You typed this name"): its detail is empty by construction, because the
  user's name claims nothing and must never inherit the model's quote. Provenance in the operation log follows: a batch
  carrying one user-edited row is logged as `agentEdited`, not `agent`.

## The "Why this name" column

Every row carries typed evidence from the backend (`evidence: { source, detail }`, the wire mirror of Rust
`RenameEvidence`), rendered as the table's rightmost column. `evidenceSourceLabel` in `ask-cmdr-labels.ts` maps the
source to its catalog string; the raw `detail` renders below it.

Three properties this column exists for:

- **A name with no content behind it must look like one.** `imageText` / `imageTags` are the only sources the backend
  verified against delivered image content (`agent/tools/propose/DETAILS.md`); the other three say plainly that nothing
  inside the file was read ("File details, not contents", "The old name", "What you asked for"). Rewording one of those
  into something that implies content would undo the guardrail's user-facing half.
- **A thin match must LOOK thin.** A bare quote made a 14-character hit inside 3,140 characters of recognized text read
  exactly as strong as a decisive one, which is the half of the failure no backend check can close: validation proves
  the model READ something, never that the name is right. So an `imageText` row with a `coverage` renders the quote
  inside the line it came from (`…before` + `<mark>` + `after…`, cut ends marked) plus "Matched 14 of 3,140 characters"
  underneath, and REPLACES the bare detail: the delivered text with the match highlighted proves more than the model's
  own retyping of it. Rows with no coverage (the other four sources) render `detail` as before.
- **`detail` is model-authored text, and the excerpt is OCR output.** Render both as plain `{text}` only, never
  `{@html}` (same boundary as assistant prose, `CLAUDE.md` § Must-knows). Backend caps `detail` at 160 characters and
  each side of the excerpt at 60.

**The thin/solid split is a display judgment, and it lives in the frontend** (`rename-evidence-coverage.ts`, unit
tested): thin is under 2 percent of the delivered text, and only once at least 200 characters were delivered (a short
quote of a short text is normal, and the excerpt beside it already shows nearly everything). The backend supplies only
the honest counts, because a "this evidence is weak" verdict must never become a refusal: the app can't know that the
name is wrong, only that the user should look. A thin row takes the warning tone AND a `triangle-alert` marker
(`role="img"`, so its label doesn't depend on text content the way the row badges' does), so it never reads by color
alone.

The layout: three fixed-pixel columns (allow 56, preview 44, arrow 32) plus three shared text columns at 25 / 25 / 42
percent, inside a `min(1040px, calc(100vw - 48px))` resizable dialog. Evidence wraps (`overflow-wrap: anywhere`) and
clamps at four lines. The clamp defends a hand-shrunk dialog; it isn't a routine truncation, because hiding evidence
from the reviewer is the failure this column fixes.

## The preview column

The reviewer has to be able to see the file, because a plausible wrong name only looks wrong beside the picture. Every
row shows its own 36 px thumbnail (scanning 50 rows for the odd wrong one is the actual review task, so a detail pane
would only show the row the user already suspects), and each thumbnail is a button that opens the file in the full
viewer with Space or Enter. ArrowDown / ArrowUp walk the buttons, so the preview follows the focused row with no mouse,
and the focused row is highlighted.

- **Thumbnails reuse the viewer's `cmdr-media://` preview scheme** through `mediaIndexThumbnailToken` + `mediaUrl`, the
  same path `lib/search/ImageSearchResults.svelte` takes. They do NOT depend on media-index enrichment: the token is
  minted from a magic-byte classification of the file itself, so a never-indexed image still previews.
- **The dialog owns the token lifecycle.** The backend token map has no window-close choke point, so a missed drop leaks
  path mappings for the session. One mint pass per proposal; every token dropped when the review closes, when another
  proposal replaces it, or on unmount. A monotonic sequence number discards a late mint for a closed review (and drops
  what it minted).
- **The mint effect depends on the proposal id ALONE.** Preflight mutates rows in place on every recheck, so reading the
  rows reactively there would re-mint 50 tokens per watcher event; the row ids and paths are read through `untrack`.
- **No thumbnail degrades to a neutral glyph** (not an image, unreadable, on a drive that isn't mounted here): never a
  broken image, never an empty cell, and the row stays fully reviewable.
- `source_path` + `volume_id` ride the row snapshot for this, and they are DISPLAY data. Apply still sends opaque row
  ids only, and the backend resolves every path from its own stored proposal.

## The context gauge (how full the chat is)

`AskCmdrContextGauge.svelte` sits in the footer row beside the cost line, and the two answer different questions on
purpose: **cost is what the whole thread has SPENT** (cumulative, only ever rising), the **gauge is how much of ONE
prompt's room the last turn used** (it drops when history is set aside). The gauge's tooltip spells out "N of M tokens
used (estimated)" so the two numbers can't read as a contradiction. Each half hides independently — a local-only thread
has a real context reading and no money to report.

State lives in `ask-cmdr-context-usage.ts`, pure and unit-tested, as four named states:

- `unmeasured` — no turn finished yet, so the gauge renders NOTHING. Deliberately not 0%: an empty bar reads as "plenty
  of room" for a thread nobody measured.
- `calm` — under `FILLING_THRESHOLD_PERCENT` (80).
- `filling` — at or over the threshold, nothing dropped yet.
- `setAside` — history left the model's view this turn. **Going over budget lands here too**, not in a fifth state: the
  turn worked, older material made room, and "over budget" is engine vocabulary, not something the user did wrong.

Two rules the tests pin: the state follows the percentage the gauge SHOWS (79.998% displays as 80%, so it reads as
`filling`, or the bar would say 80% while behaving as calm), and a measured turn never rounds down to 0%.

The figure survives a restart: the backend stores it per conversation and `ConversationDetailView.lastContextUsage`
returns it, so reopening a thread shows its last real reading. A restored reading reports `elidedResults: 0` because
whether THAT turn set anything aside isn't persisted, and inventing a count would be a false claim. A fresh chat clears
it, so no thread inherits another's fill.

## The E2E fake-LLM path

The stream also carries a display-only `proposalReady` rename-plan snapshot. The review dialog owns it in the next
feature slice; until then the rail deliberately does not treat the event as approval or a filesystem action.

The app has no real AI provider under E2E, so `commands/agent/chat.rs::resolve_agent_llm` routes the send through a
scripted `FakeAgentLlm` when `CMDR_E2E_ASK_CMDR_FAKE=1` (set for the whole E2E run by the
`desktop-svelte-e2e-playwright` check). It streams a fixed "Hi! I'm the test assistant." so `ask-cmdr.spec.ts` can
assert send-and-render deterministically, zero network. The scripted turn is Say-only (no tools), so no tool dispatch
runs. `ask-cmdr-trigger.test.ts` covers the full event model (tool lines, stop, soft cap, message paging, attachments)
with mocked events; `ask-cmdr-sessions.test.ts` covers list paging/search/rename/archive. The E2E spec also drives the
sessions path end-to-end (create two threads, search finds the right one via real FTS over the persisted messages,
switch works) — it seeds a per-run nonce into the message text so search never matches a thread left by an earlier run.

**The fake gets its own prompt budget.** It answers as a LOCAL provider with no local server behind it, so the real
resolution would size its budget from `ai.localContextSize` — a setting the harness has no reason to touch, and whose
value would then decide what the gauge shows in every E2E run. `resolve_prompt_budget` returns
`DEFAULT_PROMPT_TOKEN_BUDGET` (16,000) for the fake instead, so the harness keeps mirroring a real user's settings and
an E2E run's gauge reads as a normal calm chat rather than pinned.

The composer's Send gate (`AskCmdrComposer.svelte`) disables sending when `ai.provider` is `off` (its default), so the
fake path — which never sets a real provider — needs the gate to treat the fake as an active provider. It reads
`ask_cmdr_fake_active()` (the `commands/e2e.rs` command over `test_mode::ask_cmdr_fake_active`, the SAME accessor
`resolve_agent_llm` gates on), so "send is allowed" and "send is answered" can't drift. Off E2E the command returns
`false` and the gate behaves normally.

## Consent gate, cost, and settings

- **Consent** (`ask-cmdr-consent.svelte.ts` + `AskCmdrConsent.svelte`): the opt-in gate. `consentState.accepted` is
  `null` (loading) / `false` (show the gate) / `true` (show the chat). The backend records consent in `main.db` (version
  - timestamp) via `ask_cmdr_accept_consent`; the rail reads it with `ask_cmdr_consent_status` on open. The gate copy is
    `askCmdr.consent.*`, human-reviewed (principle 6) and shared verbatim with the settings section's disclosure.
    Nothing is sent to a provider until `accepted === true` for the CURRENT copy version. "Not now" closes the rail;
    accepting re-runs `openRail` to bootstrap history + focus the composer.
  - ⚠️ **`consentState.needsReconsent` is what keeps a copy-version bump from looking like a bug.** A bump revokes
    everybody, so somebody with a whole thread history lands on the opt-in screen with no explanation, and the settings
    section would say a bare "off" at them, indistinguishable from never having wanted AI. The flag (`accepted` false
    but `acceptedVersion` present) adds `askCmdr.consent.whatsNew.*` above the gate and a third settings state, "Ask
    Cmdr is paused", with the disclosure already open.
- **Cost footer** (`AskCmdrCostFooter.svelte` + pure `ask-cmdr-cost.ts`): the active thread's cumulative tokens + cost,
  refetched (`ask_cmdr_conversation_cost`) when the thread changes or a turn finishes streaming. Honest miss-path: a
  local-only thread reads "free, on-device", an unpriced model reads "cost unknown", a priced thread shows "about
  {amount}" — never a silent $0. Hidden until a metered turn exists.
- **Settings section** (`settings/sections/AskCmdrSection.svelte`, top-level `Ask Cmdr`): the enable toggle (drives the
  same consent accept/revoke — enable state is consent, NOT a settings boolean), the "what Ask Cmdr sends" disclosure
  (same copy as the gate), the provider hint (reads `ai.provider`) + the interactive-model row
  (`askCmdr.interactiveModel`), the two memory controls, and the per-day spend rollup (`ask_cmdr_cost_summary`). The
  interactive slot picks the MODEL only; provider/keys stay in Settings › AI. The memory pair (open the folder, forget
  everything) is documented where the folder is: `apps/desktop/src-tauri/src/agent/memory/DETAILS.md` § The two controls
  the user gets.

## i18n

Copy lives in `intl/messages/en/askCmdr.json` (`askCmdr.*`, including the `askCmdr.sessions.*`,
`askCmdr.composer.attach`/`dropHint`, `askCmdr.attachment.*`, `askCmdr.loadEarlier`, `askCmdr.wake.*`,
`askCmdr.wakeDigest.*`, and the `askCmdr.consent.*` + `askCmdr.cost.*` keys), the settings copy in `settings.json`
(`settings.askCmdr.*`, `settings.section.askCmdr`), and the command label in `commands.json`
(`commands.askCmdrToggle.*`), each with an `@key` translator description. Translated across all 10 locales, so
`desktop-i18n-coverage` is green. The name and the consent copy are the re-translation surface if David adjusts the
product calls. Tool + error labels are literal-keyed records in `ask-cmdr-labels.ts` (a computed prefix would trip the
unused-key check).

## Decisions

- **Markdown-lite escaper is narrower than the error path's on purpose** (§ CLAUDE.md): the error path escapes untrusted
  _params_ inside a trusted template, but here the whole message is model-generated and we want its markdown to render —
  so we escape only HTML/link-forming chars and keep the formatting chars. Links aren't in the markdown-lite spec, so
  dropping them is safe.
- **The send command returns early and streams on a worker thread.** `run_turn` holds a non-`Send` rusqlite `Connection`
  across awaits, so its future can't live on the Tauri command future or a multi-thread tokio task; a dedicated thread
  with a current-thread runtime sidesteps that. See `commands/agent/`.
