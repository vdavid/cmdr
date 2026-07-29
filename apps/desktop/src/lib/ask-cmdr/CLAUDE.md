# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr, the read-only chat rail: a right-side panel where the user chats with a BYO-key LLM about
their files. Backend + IPC: `src-tauri/src/agent/`, `commands/agent/`. Depth: `DETAILS.md`.

## Module map

- `ask-cmdr-trigger.svelte.ts`: the core `$state` store + mutators (open/close/focus, active thread, the streaming
  `RailMessage[]`, paging, attachments, the rename review and its undo). The one place core state changes.
  `ask-cmdr-messages.ts` has its rail-item types, `ask-cmdr-history.ts` the pure history→rail fold.
- `ask-cmdr-sessions.svelte.ts`: a SEPARATE slice for the sessions panel (thread list + paging, cross-thread search,
  rename/archive, switch-thread). Calls the trigger's `switchToThread`/`newChat`; never imported back.
- `AskCmdrRail.svelte`: the panel (header, thread, load-earlier, soft-cap nudge, composer, resize handle), hosting
  `AskCmdrSessions.svelte` as an overlay. Mounted by `routes/(main)/+page.svelte` beside `DualPaneExplorer`. Parts:
  `AskCmdrMessage` (one thread item), `AskCmdrToolLine`, `AskCmdrComposer`, `AskCmdrAttachmentChip`.
- `BulkRenameReviewDialog.svelte` + `rename-evidence-coverage.ts` / `rename-name-provenance.ts` / `rename-undo.ts`: the
  rename review and its three display judgments (how thin a quote is; where a name came from; how loud to be about what
  an undo put back).
- `ask-cmdr-markdown.ts`: the XSS boundary (escape + snarkdown). `ask-cmdr-labels.ts`: enum → localized strings.
  `ask-cmdr-drop.ts`: the native-webview drop target. `ask-cmdr-consent.svelte.ts`: the opt-in gate (shared with
  settings). `ask-cmdr-attachments.ts` / `ask-cmdr-cost.ts`: pure helpers.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (a crafted filename it echoes is an injection
  vector), so it renders ONLY through `renderAssistantMarkdown`; everything else (tool labels, paths, user text, error
  copy, rename evidence) is plain `{text}`, NEVER `{@html}`. ❌ Don't swap `escapeForMarkdownLite` for
  `errors/markdown-escape.ts`. Pinned by `ask-cmdr-markdown.test.ts`.
- **The rail gates on consent and sends NOTHING until the user opts in.** `consentState.accepted`: `false` shows the
  gate, `true` the chat, `null` neither (no flash). ❌ Never render the composer or thread outside that branch.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` / `setRailFocused()` is a
  boolean ALONGSIDE the `'left'|'right'` `focusedPane` union; never widen it. The rail is NON-modal: ❌ never add it to
  `isModalDialogOpen()` (it would suppress every shortcut). Escape refocuses `.dual-pane-explorer`.
- **No reasoning blob reaches the frontend.** `MessageView` carries display blocks only; provider state is a
  backend-only column. ❌ Never add a wire field that leaks it.
- **Streaming events mutate the LAST assistant message in place** (Svelte deep-proxies the `$state` array). **Cancel
  finalizes locally**: the runtime returns `Cancelled` with NO terminal event, so `stopStreaming` stops the bubble
  itself. ❌ Don't wait for a terminal event after a stop.
- **The toggle is wired in four places and a miss fails silently.** The sites, and why `⌘⌥A` is registered
  Command-then-Option: `DETAILS.md`. `ask-cmdr-shortcut.test.ts` pins it.
- **Opening the rail GROWS the main window so panes keep their size; closing shrinks it back** (`rail-window.ts`). ❌
  Never grow on hydration or a re-open: the window is already rail-inclusive.
- **The rename review is a guardrail surface, not a table** (`DETAILS.md`). Every state saying nothing inside the file
  was read must keep saying so: the no-content evidence labels, the `nothingRead` / `nameKept` badges. A thin
  `imageText` quote must look thin (display-only, never a refusal). Thumbnails own their `cmdr-media://` tokens, minted
  per proposal, dropped on close. The name is EDITABLE and the SERVER owns the outcome: `reviseRenameRow` posts it and
  it re-preflights. ❌ Never patch `destinationName` locally, never disable it.
- **A finished batch leaves an UNDO in the thread** (`DETAILS.md`). ❌ Never reverse the ids: `undoOperations` takes
  them in APPLY order and the backend reverses newest-batch-first. ❌ Never report success on dispatch (it resolves when
  the reversal finished); anything left behind renders `partial`, never `undone`.
- **Attachments cross into the envelope as path + kind ONLY, never contents** (the read-only privacy line). A pane drag
  is a NATIVE webview drag (`onDragDropEvent`), so a DOM `ondrop` never fires. Paging is tail-first with load-older
  prepend. Both: `DETAILS.md`.
