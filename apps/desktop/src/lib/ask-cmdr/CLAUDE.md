# Ask Cmdr rail (`lib/ask-cmdr/`)

Ask Cmdr's frontend: a right-side chat panel over a BYO-key LLM. Backend: `src-tauri/src/agent/`, `commands/agent/`.

## Module map

- `ask-cmdr-state.svelte.ts` DEFINES the core `$state`. Three slices mutate it and never each other:
  `ask-cmdr-trigger.svelte.ts` (open/close, focus, width, threads; also the entry point re-exporting the rest),
  `ask-cmdr-stream.svelte.ts` (send, the streaming reducer, the watchdog), `ask-cmdr-rename-review.svelte.ts`.
  `ask-cmdr-sessions.svelte.ts` is a SEPARATE slice for the sessions panel: it calls the trigger, never the reverse.
- `ask-cmdr-turn-stream.svelte.ts`: the window's one turn-event subscription, fanned out to the stream reducer and the
  sessions slice.
- `wake-indicator.svelte.ts` + `WakeIndicator.svelte`: the status corner's word on the PROACTIVE half (its own event).
- `ask-cmdr-slot-status.svelte.ts`: provider readiness and the gauge's baseline, before any send.
- `AskCmdrRail.svelte`: the panel (mounted by `routes/(main)/+page.svelte`), overlaid by `AskCmdrSessions.svelte`, with
  its `AskCmdr*` parts. `BulkRenameReviewDialog.svelte` is the rename review.

## Must-knows

- **Assistant prose is the XSS boundary** (a filename it echoes can inject): it renders ONLY through
  `renderAssistantMarkdown`. Everything else (paths, user text, evidence) is plain `{text}`, NEVER `{@html}`.
- **Two gates (`railGate`)**: Ask Cmdr off, then Cloud without consent; `loading` shows nothing. ❌ No composer outside
  `chat`, and ❌ neither gate grants cloud consent. The provider notice sits above both.
- **Footer cost = the backend meter, re-read on `costMetered`.** ❌ Never sum per-call usage in the rail.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` is a boolean ALONGSIDE the
  `'left'|'right'` `focusedPane` union; never widen it. It is NON-modal: ❌ never add it to `isModalDialogOpen()`.
- **No reasoning blob or provider state reaches the frontend**: `MessageView` carries display blocks only.
- **Turn events are subscribed by CONVERSATION, never per send**, so a reload mid-answer keeps rendering. Any live event
  means a turn runs, except `proposalDecided` and a stopped turn's (its terminal settles it).
- **A fresh chat's id comes from its send's answer, ❌ never an event** (`DETAILS.md` § A send waiting for its thread
  id).
- **A turn is ONE bubble of rounds; live must equal reload** (`ask-cmdr-turn-parity.test.ts`). A line is
  `(messageId, callId)`, ❌ never the call id alone; turn edges come from the backend (`turnId`).
- **Tool lines: our words per typed variant** (`ask-cmdr-labels.ts`); paths reach the panes ONLY via the `revealPath`
  prop. ❌ Never a live region. Group/expand state and the raw result: `ask-cmdr-tool-groups.svelte.ts`, ❌ never the
  message list.
- **The wake indicator is SILENT while any gate is shut or `askCmdr.proactive` is off**, and shows a running wake either
  way (it's spending money now). `wakeIndicatorMode` is the gate. ❌ Subscription stays in the `.svelte.ts`.
- **The toggle is wired in four places** (`ask-cmdr-shortcut.test.ts`).
- **Opening the rail GROWS the main window** (`rail-window.ts`); ❌ never on hydration or re-open.
- **The rename review is a guardrail surface, not a table.** Every state saying nothing inside the file was read keeps
  saying so (`nothingRead` / `nameKept`). The name is EDITABLE and the SERVER owns it: ❌ never patch `destinationName`
  locally or disable it.
- **One review holds a job's BATCHES; the grouping is presentational.** `proposalReady` stages, the turn's end shows. ❌
  Never widen a per-proposal thing (preflight, revise, apply, cancel) to the review: every guardrail stays per row.
- **A finished batch leaves an UNDO in the thread.** ❌ Never reverse the ids: `undoOperations` takes them in APPLY
  order and the backend reverses newest-batch-first. ❌ Never report success on dispatch; a partial result is `partial`.
- **Attachments cross as path + kind ONLY.** A pane drag is a NATIVE webview drag: a DOM `ondrop` never fires.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
