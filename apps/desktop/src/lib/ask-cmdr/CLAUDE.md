# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr, the read-only chat rail: a right-side panel for chatting with a BYO-key LLM about your files.
Backend + IPC: `src-tauri/src/agent/`, `commands/agent/`.

## Module map

- `ask-cmdr-state.svelte.ts`: the core `$state` store and its accessors — the one place core state is DEFINED. Three
  slices mutate it and never each other: `ask-cmdr-trigger.svelte.ts` (open/close, focus, width, threads; also the entry
  point re-exporting the rest), `ask-cmdr-stream.svelte.ts` (send, the streaming reducer, the watchdog),
  `ask-cmdr-rename-review.svelte.ts` (the review and its undo). `ask-cmdr-messages.ts` holds the rail-item types,
  `ask-cmdr-history.ts` the pure history→rail fold.
- `ask-cmdr-sessions.svelte.ts`: a SEPARATE slice for the sessions panel. Calls the trigger; never imported back.
- `ask-cmdr-turn-stream.svelte.ts`: the window's one turn-event subscription, fanned out to the stream reducer and the
  sessions slice — the only module knowing both, so that loop stays open.
- `AskCmdrRail.svelte`: the panel, mounted beside `DualPaneExplorer` by `routes/(main)/+page.svelte`, overlaid by
  `AskCmdrSessions.svelte`, with `AskCmdrMessage` / `ToolLine` / `Composer` / `AttachmentChip` parts.
- `BulkRenameReviewDialog.svelte` + `rename-evidence-coverage.ts` / `rename-name-provenance.ts` / `rename-undo.ts`: the
  rename review and its three display judgments.
- `ask-cmdr-markdown.ts` (the XSS boundary), `ask-cmdr-labels.ts`, `ask-cmdr-drop.ts` (native-webview drop target),
  `ask-cmdr-consent.svelte.ts` (opt-in gate, shared with settings), `ask-cmdr-attachments.ts`, `ask-cmdr-cost.ts`.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (a crafted filename it echoes is an injection
  vector), so it renders ONLY through `renderAssistantMarkdown`; everything else (tool labels, paths, user text, error
  copy, rename evidence) is plain `{text}`, NEVER `{@html}`. ❌ Don't swap `escapeForMarkdownLite` for
  `error-messages/markdown-escape.ts`. `ask-cmdr-markdown.test.ts` pins it.
- **The rail gates on consent and sends NOTHING until the user opts in.** `consentState.accepted`: `false` shows the
  gate, `true` the chat, `null` neither (no flash). ❌ Never render the composer or thread outside it.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` is a boolean ALONGSIDE the
  `'left'|'right'` `focusedPane` union; never widen it. The rail is NON-modal: ❌ never add it to `isModalDialogOpen()`,
  which would suppress every shortcut.
- **No reasoning blob reaches the frontend.** `MessageView` carries display blocks only. ❌ Never add a wire field
  leaking provider state.
- **Turn events are subscribed by CONVERSATION, never per send**, so a reload mid-answer keeps rendering: ❌ never key a
  turn to the invoke that started it. Any live event means a turn is running; `discarded` means a quiet wake deleted the
  thread under the rail. Each mutates the LAST assistant message in place, and cancel finalizes LOCALLY (nothing follows
  `Cancelled`, so `stopStreaming` stops the bubble and lists the thread stopped). ❌ Don't wait for a terminal event.
- **The toggle is wired in four places; a miss fails silently** (`ask-cmdr-shortcut.test.ts` pins it).
- **Opening the rail GROWS the main window so panes keep their size**, closing shrinks it back (`rail-window.ts`). ❌
  Never grow on hydration or re-open: the window is already rail-inclusive.
- **The rename review is a guardrail surface, not a table.** Every state saying nothing inside the file was read must
  keep saying so (no-content evidence labels, the `nothingRead` / `nameKept` badges); a thin `imageText` quote must look
  thin. The name is EDITABLE and the SERVER owns the outcome. ❌ Never patch `destinationName` locally or disable it.
- **A finished batch leaves an UNDO in the thread.** ❌ Never reverse the ids: `undoOperations` takes them in APPLY
  order and the backend reverses newest-batch-first. ❌ Never report success on dispatch; what's left behind is
  `partial`, never `undone`.
- **Attachments cross as path + kind ONLY, never contents** (the read-only privacy line). A pane drag is a NATIVE
  webview drag (`onDragDropEvent`), so a DOM `ondrop` never fires.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
