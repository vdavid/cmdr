# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr: a right-side panel for chatting with a BYO-key LLM about your files, read-only. Backend + IPC:
`src-tauri/src/agent/`, `commands/agent/`.

## Module map

- `ask-cmdr-state.svelte.ts` DEFINES the core `$state`. Three slices mutate it and never each other:
  `ask-cmdr-trigger.svelte.ts` (open/close, focus, width, threads; also the entry point re-exporting the rest),
  `ask-cmdr-stream.svelte.ts` (send, the streaming reducer, the watchdog), `ask-cmdr-rename-review.svelte.ts`.
  `ask-cmdr-sessions.svelte.ts` is a SEPARATE slice for the sessions panel: it calls the trigger, never the reverse.
- `ask-cmdr-turn-stream.svelte.ts`: the window's one turn-event subscription, fanned out to the stream reducer and the
  sessions slice — the only module knowing both, so that loop stays open.
- `wake-indicator.svelte.ts` + `WakeIndicator.svelte`: the status corner's word on the PROACTIVE half, on its own event
  rather than the turn stream.
- `AskCmdrRail.svelte`: the panel, mounted beside `DualPaneExplorer` by `routes/(main)/+page.svelte`, overlaid by
  `AskCmdrSessions.svelte`, with its `AskCmdrMessage` / `ToolLine` / `Composer` / `AttachmentChip` / `WakeDigest` parts.
  `BulkRenameReviewDialog.svelte` is the rename review and its three display judgments.
- The rest are named for what they do; `ask-cmdr-markdown.ts` is the XSS boundary.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (a crafted filename it echoes is an injection
  vector), so it renders ONLY through `renderAssistantMarkdown`. Everything else — tool labels, paths, user text, error
  copy, rename evidence — is plain `{text}`, NEVER `{@html}`.
- **The rail gates on consent and sends NOTHING until the user opts in.** `consentState.accepted`: `false` shows the
  gate, `true` the chat, `null` neither (no flash). ❌ Never render the composer outside that.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` is a boolean ALONGSIDE the
  `'left'|'right'` `focusedPane` union; never widen it. It is NON-modal: ❌ never add it to `isModalDialogOpen()`.
- **No reasoning blob reaches the frontend.** `MessageView` carries display blocks only. ❌ Never add a wire field
  leaking provider state.
- **Turn events are subscribed by CONVERSATION, never per send**, so a reload mid-answer keeps rendering: ❌ never key a
  turn to the invoke that started it. Any live event means a turn is running; `discarded` means a quiet wake deleted the
  thread under the rail. Each mutates the LAST assistant message in place, and cancel finalizes LOCALLY.
- **The wake indicator is SILENT without consent or with `askCmdr.proactive` off**, and shows a running wake either way
  (that one is spending money now). `wakeIndicatorMode` is the gate, reconciling the corner's "nothing to say is noise"
  rule with `agent/wake/readiness.rs`'s "every gap is worth reporting". ❌ Its subscription stays in the `.svelte.ts`:
  `StatusCorner`'s two suites mount the component for real and stub nothing.
- **The toggle is wired in four places; a miss fails silently** (`ask-cmdr-shortcut.test.ts`).
- **Opening the rail GROWS the main window so panes keep their size** (`rail-window.ts`). ❌ Never grow on hydration or
  re-open: the window is already rail-inclusive.
- **The rename review is a guardrail surface, not a table.** Every state saying nothing inside the file was read must
  keep saying so (`nothingRead` / `nameKept`). The name is EDITABLE and the SERVER owns the outcome. ❌ Never patch
  `destinationName` locally or disable it.
- **A finished batch leaves an UNDO in the thread.** ❌ Never reverse the ids: `undoOperations` takes them in APPLY
  order and the backend reverses newest-batch-first. ❌ Never report success on dispatch; a partial result is `partial`,
  never `undone`.
- **Attachments cross as path + kind ONLY, never contents** (the read-only privacy line). A pane drag is a NATIVE
  webview drag, so a DOM `ondrop` never fires.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
