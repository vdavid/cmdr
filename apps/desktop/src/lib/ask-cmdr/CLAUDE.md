# Ask Cmdr rail (`lib/ask-cmdr/`)

The frontend of Ask Cmdr, the read-only chat rail: a toggleable right-side panel where the user chats with a BYO-key LLM
about their files. Backend seam + IPC: `src-tauri/src/agent/` and `commands/agent.rs`. Plan:
`docs/specs/ask-cmdr-plan.md`. Depth (the streaming flow, the fake E2E path, layout, decisions): `DETAILS.md`.

## Module map

- `ask-cmdr-trigger.svelte.ts`: the core `$state` store + mutators — open/close/focus, the active thread, the live
  streaming model (`RailMessage[]`), message paging, and staged attachments. The one place core state changes.
- `ask-cmdr-sessions.svelte.ts`: a SEPARATE state slice for the sessions panel — thread list + paging, cross-thread
  search, rename/archive, switch-thread. Calls the trigger's `switchToThread`/`newChat`; the trigger never imports it
  back (no cycle).
- `AskCmdrRail.svelte`: the panel (header + ALPHA badge, thread, load-earlier, soft-cap nudge, composer, resize handle),
  hosting `AskCmdrSessions.svelte` as an overlay. Hosted by `routes/(main)/+page.svelte` beside `DualPaneExplorer`.
- `AskCmdrMessage.svelte` / `AskCmdrToolLine.svelte` / `AskCmdrComposer.svelte` / `AskCmdrAttachmentChip.svelte`: one
  thread item, one collapsible tool line, the input (with attach button + drop target), one attachment chip.
- `ask-cmdr-markdown.ts`: the XSS boundary (escape + snarkdown). `ask-cmdr-labels.ts`: typed enum → localized string
  maps. `ask-cmdr-drop.ts`: the native-webview drop target. `ask-cmdr-attachments.ts`: pure chip helpers.
- `ask-cmdr-consent.svelte.ts`: the opt-in gate state + refresh/accept/revoke (shared by `AskCmdrConsent.svelte` and the
  settings section). `ask-cmdr-cost.ts`: pure cost-format helpers for `AskCmdrCostFooter.svelte`.

## Must-knows

- **Assistant prose is the XSS boundary.** Model text is untrusted (a crafted filename it echoes is an injection
  vector). Render it ONLY through `renderAssistantMarkdown` before `{@html}`; everything else — tool labels, paths, user
  text, error copy — renders as plain `{text}` (Svelte auto-escapes), NEVER `{@html}`. Don't swap its narrow
  `escapeForMarkdownLite` for `errors/markdown-escape.ts` (that escapes the formatting chars too, so nothing renders).
  Pinned by `ask-cmdr-markdown.test.ts`; rationale in `DETAILS.md` § Decisions.
- **The rail gates on consent; it sends NOTHING until the user opts in.** `openRail` refreshes `consentState`: `false`
  shows `AskCmdrConsent.svelte`, `true` shows the chat, `null` shows neither (no flash). Don't render the
  composer/thread outside the `consented` branch.
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
- **Opening the rail GROWS the main window so panes keep their size; closing shrinks it back** (`rail-window.ts`, skips
  fullscreen/maximized). ❌ Don't grow on hydration or a re-open — the window is already rail-inclusive, so
  `hydrateRail` passes `resizeWindow: false` and `openRail` grows only on `!wasOpen`. Doubling breaks. Depth:
  `DETAILS.md` § Window growth.
- **Attachments cross into the envelope as path + kind ONLY — never contents** (the read-only privacy line). Drag from a
  pane is a NATIVE webview drag (`onDragDropEvent`), not HTML5, so a DOM `ondrop` never fires. Message paging is
  tail-first with load-older prepend (don't reintroduce a single big page). Both detailed in `DETAILS.md`.

Depth: `DETAILS.md`.
