# Tool questions (`tool-ask/`)

The dialog a `Sensitive` tool's question shows (`src-tauri/src/tools/ask.rs`), and the state behind it.

## Module map

`tool-ask.svelte.ts` (the one open question, the two events, the answers), `tool-ask-sentence.ts` (typed facts to
words), `ToolAskDialog.svelte` (presentational, also the gallery's), `ToolAskHost.svelte` (mounted by the main page).

## Must-knows

- **Every way a question comes down answers the backend**, or a tool waits out five minutes behind nothing. Escape, ×,
  and `dialog close` go through `ModalDialog`'s `onclose` to a decline; an error boundary answers `gone`.
- **Always guarded** (`startedByWhenGuarded(startedBy, true)`): whoever asked isn't at the keyboard, and direct mode
  never gets here.
- **What fills a sentence's blanks is untrusted**: a file name or a model's chosen value. It arrives plain and bounded
  from `fact_text`, renders as text nodes, and scrolls inside the body. ❌ No markup, ❌ no `<Trans>` with these.

Rationale: `DETAILS.md`.
