# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr, the read-only chat rail: a toggleable right-side panel where the user chats with a BYO-key LLM
about their files. Backend seam + IPC: `src-tauri/src/agent/` and `commands/agent.rs`. Plan:
[`docs/specs/ask-cmdr-plan.md`](../../../../../docs/specs/ask-cmdr-plan.md) § M6. Depth (the streaming flow, the fake
E2E path, layout, decisions): [DETAILS.md](DETAILS.md).

## Module map

- `ask-cmdr-trigger.svelte.ts`: the `$state` store + all mutators — open/close/focus, the active thread, and the live
  streaming model (`RailMessage[]`). The one place state changes.
- `AskCmdrRail.svelte`: the panel (header + ALPHA badge, thread, soft-cap nudge, composer, resize handle). Hosted by
  `routes/(main)/+page.svelte` beside `DualPaneExplorer`.
- `AskCmdrMessage.svelte` / `AskCmdrToolLine.svelte` / `AskCmdrComposer.svelte`: one thread item, one collapsible tool
  line, the input.
- `ask-cmdr-markdown.ts`: the XSS boundary (escape + snarkdown). `ask-cmdr-labels.ts`: typed enum → localized string
  maps.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (and a crafted filename it echoes is an injection
  vector). Render it ONLY through `renderAssistantMarkdown` (HTML-entity escape via `escapeForMarkdownLite`, then
  snarkdown) before `{@html}`. Everything else — tool labels, paths, user text, error copy — renders as plain `{text}`
  (Svelte auto-escapes), NEVER `{@html}`. `escapeForMarkdownLite` escapes only `& < > [ ]` (kills raw HTML + links) and
  keeps
  `* _ \`` so markdown-lite still renders; don't swap in `errors/markdown-escape.ts`(it escapes the formatting chars too, so nothing renders). Pinned by`ask-cmdr-markdown.test.ts`.
- **The rail is a THIRD focus region via a parallel flag.** `explorerState.getRailFocused()` / `setRailFocused()` is a
  boolean ALONGSIDE the `'left'|'right'` `focusedPane` union — never widen that union. The rail is NON-modal: do NOT add
  it to `isModalDialogOpen()` in `+page.svelte` (that would suppress every shortcut while it's open). Opening focuses
  the composer; Escape in the composer returns focus to `.dual-pane-explorer`.
- **No reasoning blob ever reaches the frontend.** `MessageView` (the wire type) carries display blocks only; the opaque
  provider state is a backend-only DB column. Don't add a wire field that leaks it.
- **Streaming events mutate the LAST assistant message in place** (Svelte deep-proxies the `$state` array). **Cancel
  finalizes locally**: the runtime returns `Cancelled` with NO terminal event, so `stopStreaming` stops the bubble
  itself — don't wait for a `done`/`failed` after a stop.
- **The toggle is wired in four places** (a miss fails silently): the command registry + `COMMAND_IDS` + the
  `askCmdr.toggle` handler; Rust `command_map.rs` + the `macos.rs`/`linux.rs` View submenus; `shortcuts-store.ts`
  `menuCommands`. Default `⌘⌥A`, registered Command-then-Option (⌥⌘-order strings are native-menu-only). Pinned by
  `ask-cmdr-shortcut.test.ts`.

Depth: [DETAILS.md](DETAILS.md).
