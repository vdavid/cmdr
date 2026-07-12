# Ask Cmdr rail details

Pull-tier docs for `lib/ask-cmdr/`. Must-knows live in [CLAUDE.md](CLAUDE.md). Plan:
[`docs/specs/ask-cmdr-plan.md`](../../../../../docs/specs/ask-cmdr-plan.md) § M6. Backend:
[`src-tauri/src/agent/CLAUDE.md`](../../../src-tauri/src/agent/CLAUDE.md) and
[`commands/agent.rs`](../../../src-tauri/src/commands/agent.rs).

## The IPC surface (M6)

Wrappers in [`lib/tauri-commands/ask-cmdr.ts`](../tauri-commands/ask-cmdr.ts):

- `sendAskCmdrMessage(conversationId, text, onEvent)` — streaming, over a raw `invoke` + Tauri `Channel` (Channel isn't
  specta-friendly, so it's one of the sanctioned raw-invoke sites, with the eslint opt-out). `conversationId` is `null`
  for a new thread; the resolved id arrives both in the first `started` event and as the promise value. The command
  returns the id at once and keeps streaming on a worker thread.
- `cancelAskCmdr(id)`, `getAskCmdrConversation(id, limit, offset)`, `listAskCmdrConversations(...)` — plain specta
  commands.

`AskCmdrStreamEvent` is hand-mirrored from the Rust `Channel`-only enum (absent from `bindings.ts`). `MessageView` /
`MessageBlock` / `ConversationRow` / `ConversationDetailView` ARE in the generated bindings and re-exported.

## The streaming model

`sendMessage` optimistically appends a `{ kind: 'user' }` item and flips `streaming` on, then calls
`sendAskCmdrMessage`. Events drive the render (`handleStreamEvent`), each delegating to a tiny mutator so the switch
stays simple:

- `started` → set `conversationId` (the stop button + a new thread key on it).
- `assistantStarted` → push a streaming `{ kind: 'assistant', text: '', tools: [] }`.
- `textDelta` → append to the last assistant's `text`; clear its `thinking`.
- `reasoningTick` → set the last assistant's `thinking` (a subtle "thinking…" line; the reasoning content itself never
  crosses).
- `toolCallStarted` / `toolCallFinished` → push / update a `RailToolCall` (the collapsible "looked at X" line;
  `ok = false` is a refusal or handler problem).
- `done` → finalize the bubble, stamp its persisted id, `streaming = false`.
- `failed` → drop an empty bubble, push a typed `{ kind: 'error' }` item, `streaming = false`.

**Cancel finalizes locally.** The runtime returns `Cancelled` with no terminal event, so `stopStreaming` cancels the
backend AND finalizes the current bubble itself (a late `textDelta` that races in is harmless — it just appends a little
more text to a non-streaming bubble).

History loads through `getAskCmdrConversation` on rail open (bootstrapping the most recent thread) and folds `tool`-role
result rows into their assistant tool line by `callId`, so the thread shows one line per call. Real paging is M7; v1
loads up to `HISTORY_LOAD_LIMIT` (threads are small — no retention, soft cap ~40).

## Layout, persistence, focus

- Hosted in a flex row (`.explorer-rail-row`) beside `DualPaneExplorer`: the panes take the remainder
  (`flex: 1; min-width: 0`), the rail its fixed px width. Below ~900px a media query flips the rail to
  `position: absolute` so it OVERLAYS the right pane instead of squeezing the panes below their min-width.
- Rail open flag + width persist via `app-status-store.ts` (`askCmdrRailOpen`, `askCmdrRailWidth`, clamped 280–520),
  mirroring `leftPaneWidthPercent`. `hydrateRail` applies them once at startup from `loadPersistedState` (reopening
  bootstraps the active thread).
- The left-edge drag handle resizes (double-click resets to 340). Focus: an `$effect` focuses the composer on mount (the
  rail mounts on open); `markRailFocused` on composer focus; Escape → `returnFocusToPane`
  (`.dual-pane-explorer.focus()`).

## The E2E fake-LLM path

The app has no real AI provider under E2E, so `commands/agent.rs::resolve_agent_llm` routes the send through a scripted
`FakeAgentLlm` when `CMDR_E2E_ASK_CMDR_FAKE=1` (set for the whole E2E run by the `desktop-svelte-e2e-playwright` check).
It streams a fixed "Hi! I'm the test assistant." so `ask-cmdr.spec.ts` can assert send-and-render deterministically,
zero network. The scripted turn is Say-only (no tools), so no tool dispatch runs. `ask-cmdr-trigger.test.ts` covers the
full event model (tool lines, stop, soft cap) with mocked events.

## i18n

Copy lives in `intl/messages/en/askCmdr.json` (`askCmdr.*`) + the command label in `commands.json`
(`commands.askCmdrToggle.*`), each with an `@key` translator description. English-only in M6; the other nine locales are
M8 (so `desktop-i18n-coverage` reports the gap until then). Tool + error labels are literal-keyed records in
`ask-cmdr-labels.ts` (a computed prefix would trip the unused-key check).

## Decisions

- **Markdown-lite escaper is narrower than the error path's on purpose** (§ CLAUDE.md): the error path escapes untrusted
  _params_ inside a trusted template, but here the whole message is model-generated and we want its markdown to render —
  so we escape only HTML/link-forming chars and keep the formatting chars. Links aren't in the markdown-lite spec, so
  dropping them is safe.
- **The send command returns early and streams on a worker thread.** `run_turn` holds a non-`Send` rusqlite `Connection`
  across awaits, so its future can't live on the Tauri command future or a multi-thread tokio task; a dedicated thread
  with a current-thread runtime sidesteps that. See `commands/agent.rs`.
