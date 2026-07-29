# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr, the read-only chat rail: a right-side panel where the user chats with a BYO-key LLM about
their files. Backend + IPC: `src-tauri/src/agent/`, `commands/agent/`. Depth: `DETAILS.md`.

## Module map

- `ask-cmdr-trigger.svelte.ts`: the core `$state` store + mutators (open/close/focus, active thread, the streaming
  `RailMessage[]`, paging, attachments, the rename review). The one place core state changes. `ask-cmdr-messages.ts` has
  its rail-item types, `ask-cmdr-history.ts` the pure history→rail fold.
- `ask-cmdr-sessions.svelte.ts`: a SEPARATE slice for the sessions panel (thread list + paging, cross-thread search,
  rename/archive, switch-thread). Calls the trigger's `switchToThread`/`newChat`; the trigger never imports it back.
- `AskCmdrRail.svelte`: the panel (header, thread, load-earlier, soft-cap nudge, composer, resize handle), hosting
  `AskCmdrSessions.svelte` as an overlay. Mounted by `routes/(main)/+page.svelte` beside `DualPaneExplorer`. Its parts:
  `AskCmdrMessage` (one thread item), `AskCmdrToolLine`, `AskCmdrComposer`, `AskCmdrAttachmentChip`.
- `BulkRenameReviewDialog.svelte` + `rename-evidence-coverage.ts` / `rename-name-provenance.ts`: the rename review and
  its two display judgments (how thin a quote is; where a name came from).
- `ask-cmdr-markdown.ts`: the XSS boundary (escape + snarkdown). `ask-cmdr-labels.ts`: enum → localized string maps.
  `ask-cmdr-drop.ts`: the native-webview drop target. `ask-cmdr-consent.svelte.ts`: the opt-in gate (shared with
  settings). `ask-cmdr-attachments.ts` / `ask-cmdr-cost.ts`: pure helpers.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (a crafted filename it echoes is an injection
  vector), so it renders ONLY through `renderAssistantMarkdown`; everything else (tool labels, paths, user text, error
  copy, rename evidence) is plain `{text}`, NEVER `{@html}`. ❌ Don't swap the narrow `escapeForMarkdownLite` for
  `errors/markdown-escape.ts` (nothing would render). `ask-cmdr-markdown.test.ts` pins it.
- **The rail gates on consent and sends NOTHING until the user opts in.** `consentState.accepted`: `false` shows the
  gate, `true` the chat, `null` neither (no flash). Never render the composer or thread outside that branch.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` / `setRailFocused()` is a
  boolean ALONGSIDE the `'left'|'right'` `focusedPane` union; never widen it. The rail is NON-modal: ❌ never add it to
  `isModalDialogOpen()` (that suppresses every shortcut while it's open). Escape refocuses `.dual-pane-explorer`.
- **No reasoning blob reaches the frontend.** `MessageView` carries display blocks only; provider state is a
  backend-only column. Never add a wire field that leaks it.
- **Streaming events mutate the LAST assistant message in place** (Svelte deep-proxies the `$state` array). **Cancel
  finalizes locally**: the runtime returns `Cancelled` with NO terminal event, so `stopStreaming` stops the bubble
  itself; don't wait for a `done`/`failed` after a stop.
- **The toggle is wired in four places and a miss fails silently.** The sites, and why `⌘⌥A` is registered
  Command-then-Option, are in `DETAILS.md`. `ask-cmdr-shortcut.test.ts` pins it.
- **Opening the rail GROWS the main window so panes keep their size; closing shrinks it back** (`rail-window.ts`). ❌
  Never grow on hydration or a re-open: the window is already rail-inclusive, so doubling breaks.
- **The rename review is a guardrail surface, not a table** (all of it in `DETAILS.md`). Every state that says nothing
  inside the file was read must keep saying so: the no-content evidence labels, and the `nothingRead` / `nameKept`
  badges. A thin `imageText` quote must look thin (display-only, never a refusal). Thumbnails own their `cmdr-media://`
  tokens, minted per proposal and dropped on close. The proposed name is EDITABLE and the SERVER owns the outcome:
  `reviseRenameRow` posts it, the row takes the backend's answer, and it re-preflights. ❌ Never patch
  `destinationName` locally; never disable the field (an occupied name is fixed by typing another one).
- **Attachments cross into the envelope as path + kind ONLY, never contents** (the read-only privacy line). A pane drag
  is a NATIVE webview drag (`onDragDropEvent`), so a DOM `ondrop` never fires. Paging is tail-first with load-older
  prepend. Both in `DETAILS.md`.
