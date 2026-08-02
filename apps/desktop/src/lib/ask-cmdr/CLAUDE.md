# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr, the read-only chat rail: a right-side panel where the user chats with a BYO-key LLM about
their files. Backend + IPC: `src-tauri/src/agent/`, `commands/agent/`.

## Module map

- `ask-cmdr-trigger.svelte.ts`: the core `$state` store + mutators, the one place core state changes.
  `ask-cmdr-messages.ts` holds its rail-item types, `ask-cmdr-history.ts` the pure history→rail fold.
- `ask-cmdr-sessions.svelte.ts`: a SEPARATE slice for the sessions panel. Calls into the trigger; never imported back.
- `AskCmdrRail.svelte`: the panel, mounted by `routes/(main)/+page.svelte` beside `DualPaneExplorer`, hosting
  `AskCmdrSessions.svelte` as an overlay, with `AskCmdrMessage` / `ToolLine` / `Composer` / `AttachmentChip` parts.
- `BulkRenameReviewDialog.svelte` + `rename-evidence-coverage.ts` / `rename-name-provenance.ts` / `rename-undo.ts`: the
  rename review and its three display judgments.
- `ask-cmdr-markdown.ts` (the XSS boundary), `ask-cmdr-labels.ts`, `ask-cmdr-drop.ts` (native-webview drop target),
  `ask-cmdr-consent.svelte.ts` (opt-in gate, shared with settings), `ask-cmdr-attachments.ts`, `ask-cmdr-cost.ts`.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (a crafted filename it echoes is an injection
  vector), so it renders ONLY through `renderAssistantMarkdown`; everything else (tool labels, paths, user text, error
  copy, rename evidence) is plain `{text}`, NEVER `{@html}`. ❌ Don't swap `escapeForMarkdownLite` for
  `error-messages/markdown-escape.ts`. Pinned by `ask-cmdr-markdown.test.ts`.
- **The rail gates on consent and sends NOTHING until the user opts in.** `consentState.accepted`: `false` shows the
  gate, `true` the chat, `null` neither (no flash). ❌ Never render the composer or thread outside that branch.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` is a boolean ALONGSIDE the
  `'left'|'right'` `focusedPane` union; never widen it. The rail is NON-modal: ❌ never add it to `isModalDialogOpen()`
  (it would suppress every shortcut).
- **No reasoning blob reaches the frontend.** `MessageView` carries display blocks only. ❌ Never add a wire field that
  leaks provider state.
- **Streaming events mutate the LAST assistant message in place, and cancel finalizes LOCALLY**: the runtime returns
  `Cancelled` with no terminal event, so `stopStreaming` stops the bubble itself. ❌ Don't wait for one after a stop.
- **The toggle is wired in four places and a miss fails silently** (`ask-cmdr-shortcut.test.ts` pins it).
- **Opening the rail GROWS the main window so panes keep their size**, closing shrinks it back (`rail-window.ts`). ❌
  Never grow on hydration or a re-open: the window is already rail-inclusive.
- **The rename review is a guardrail surface, not a table.** Every state saying nothing inside the file was read must
  keep saying so (no-content evidence labels, the `nothingRead` / `nameKept` badges); a thin `imageText` quote must look
  thin. The name is EDITABLE and the SERVER owns the outcome. ❌ Never patch `destinationName` locally, never disable
  it.
- **A finished batch leaves an UNDO in the thread.** ❌ Never reverse the ids: `undoOperations` takes them in APPLY
  order and the backend reverses newest-batch-first. ❌ Never report success on dispatch; anything left behind renders
  `partial`, never `undone`.
- **Attachments cross into the envelope as path + kind ONLY, never contents** (the read-only privacy line). A pane drag
  is a NATIVE webview drag (`onDragDropEvent`), so a DOM `ondrop` never fires.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
