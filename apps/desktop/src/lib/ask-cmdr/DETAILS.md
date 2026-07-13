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
result rows into their assistant tool line by `callId`, so the thread shows one line per call.

## Sessions, search, message paging (M7)

- **Sessions panel** (`AskCmdrSessions.svelte`, opened from the rail header's "Chats" button) overlays the rail body
  (`position: absolute; inset: 0`) with a search box, an active/archived filter, and the thread list. Its state lives in
  a separate slice, `ask-cmdr-sessions.svelte.ts` (`sessionsState`), which calls the trigger's `switchToThread` /
  `newChat`; the trigger never imports it back (no cycle). Selecting a thread switches the rail and closes the panel.
- **List paging mirrors the operation-log dialog**: the offset is `conversations.length` (one source of truth), so an
  append can't overlap or desync; a full page (`SESSIONS_PAGE`) means "load more" is offered. Rename edits the row's
  title in place. The archived filter has two states: active-only (default) and "show archived", which shows ALL threads
  with archived ones badged (the backend `include_archived=true` returns everything, so the reverse label is "Hide
  archived", not "Show active"). `setArchived` drops a row only when archiving in the active-only view; in the all view
  a flip just updates the badge in place.
- **Search** is debounced (`SEARCH_DEBOUNCE_MS`) and guarded by a monotonic `searchSeq` so a slow earlier response can't
  overwrite a newer one. A non-empty query replaces the list with FTS hits (`searchAskCmdrConversations`); clearing it
  restores the list. Each hit's `snippet` is backend FTS text rendered as plain `{text}` (never `{@html}`).
- **Message paging is tail-first** (a chat shows newest at the bottom). `loadConversation` probes page 0 to learn
  `totalMessages`, then refetches the newest page when the thread exceeds `MESSAGE_PAGE`; `historyCount` tracks how many
  rows are loaded from the tail. "Load earlier" (`loadOlderMessages`) prepends the previous page, its offset derived
  from `messageTotal - historyCount` so pages tile without overlap and live-streamed rows (newer than the load-time
  total) are never disturbed. The rail preserves the scroll position across a prepend (capture `scrollHeight` before,
  restore after) and its auto-scroll-to-bottom only fires when the user was already near the bottom (`wasNearBottom`,
  tracked on scroll), so streaming follows but loading older doesn't jump. Page-boundary caveat: `buildRailMessages`
  folds each loaded page independently, so a tool result split across a page seam may render unfolded — negligible in
  practice (threads sit under the ~40 soft cap, well below a 50-message page, so paging rarely fires at all).

## Attachments by reference (M7)

- The composer stages `AttachmentRef { path, kind }` chips (`askCmdrState.attachments`), sent with the next message and
  cleared after. They ride into the context envelope as `attached: <path> (<kind>)` on the latest user turn — **path +
  kind only, structurally never contents** (the read-only privacy line). History user rows carry no chips (the refs were
  envelope text, not stored blocks).
- **"Ask about selection"** (the paperclip button) calls `ask_cmdr_selection_attachments`, which reads the focused pane
  from `PaneStateStore` (the same source the envelope uses) and returns its selection (or cursor item) as typed refs —
  no filesystem stat.
- **Drag-onto-composer is a NATIVE webview drag, not HTML5** (`ask-cmdr-drop.ts`): a Cmdr pane drag is delivered through
  `getCurrentWebview().onDragDropEvent`, so a DOM `ondrop` would never fire. The composer subscribes to that event and
  hit-tests its own rect (via `toViewportPosition`, mirroring the pane drag-drop controller). For an in-app drag the
  trustworthy source is the recorded self-drag identity (`getSelfDragIdentity`), not the pasteboard-round-tripped
  payload paths; only LOCAL (`'root'`) self-drags are supported (virtual-volume paths mis-resolve). A Finder drop uses
  the payload paths (genuine local absolute). Kinds are resolved backend-side (`ask_cmdr_resolve_attachments`) from
  known pane state, defaulting to file. The Tauri APIs load lazily and swallow failures, so the composer still mounts
  outside a Tauri webview (unit tests).
- Chips render the escaped basename (`attachmentBasename`) as plain `{text}` — filesystem names are
  attacker-controllable on a network share, so never `{@html}` (see the shared XSS-boundary rule).

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
full event model (tool lines, stop, soft cap, message paging, attachments) with mocked events;
`ask-cmdr-sessions.test.ts` covers list paging/search/rename/archive. The E2E spec also drives the sessions path
end-to-end (create two threads, search finds the right one via real FTS over the persisted messages, switch works) — it
seeds a per-run nonce into the message text so search never matches a thread left by an earlier run.

## Consent gate, cost, and settings (M8)

- **Consent** (`ask-cmdr-consent.svelte.ts` + `AskCmdrConsent.svelte`): the opt-in gate. `consentState.accepted` is
  `null` (loading) / `false` (show the gate) / `true` (show the chat). The backend records consent in `main.db` (version
  - timestamp) via `ask_cmdr_accept_consent`; the rail reads it with `ask_cmdr_consent_status` on open. The gate copy is
    `askCmdr.consent.*`, human-reviewed (principle 6) and shared verbatim with the settings section's disclosure.
    Nothing is sent to a provider until `accepted === true` for the CURRENT copy version. "Not now" closes the rail;
    accepting re-runs `openRail` to bootstrap history + focus the composer.
- **Cost footer** (`AskCmdrCostFooter.svelte` + pure `ask-cmdr-cost.ts`): the active thread's cumulative tokens + cost,
  refetched (`ask_cmdr_conversation_cost`) when the thread changes or a turn finishes streaming. Honest miss-path: a
  local-only thread reads "free, on-device", an unpriced model reads "cost unknown", a priced thread shows "about
  {amount}" — never a silent $0. Hidden until a metered turn exists.
- **Settings section** (`settings/sections/AskCmdrSection.svelte`, top-level `Ask Cmdr`): the enable toggle (drives the
  same consent accept/revoke — enable state is consent, NOT a settings boolean), the "what Ask Cmdr sends" disclosure
  (same copy as the gate), the provider hint (reads `ai.provider`) + the interactive-model row
  (`askCmdr.interactiveModel`), and the per-day spend rollup (`ask_cmdr_cost_summary`). The interactive slot picks the
  MODEL only; provider/keys stay in Settings › AI.

## i18n

Copy lives in `intl/messages/en/askCmdr.json` (`askCmdr.*`, including the M7 `askCmdr.sessions.*`,
`askCmdr.composer.attach`/`dropHint`, `askCmdr.attachment.*`, `askCmdr.loadEarlier`, and the M8 `askCmdr.consent.*` +
`askCmdr.cost.*` keys), the settings copy in `settings.json` (`settings.askCmdr.*`, `settings.section.askCmdr`), and the
command label in `commands.json` (`commands.askCmdrToggle.*`), each with an `@key` translator description. Translated
across all 10 locales in M8, so `desktop-i18n-coverage` is green. The name and the consent copy are the re-translation
surface if David adjusts the product calls. Tool + error labels are literal-keyed records in `ask-cmdr-labels.ts` (a
computed prefix would trip the unused-key check).

## Decisions

- **Markdown-lite escaper is narrower than the error path's on purpose** (§ CLAUDE.md): the error path escapes untrusted
  _params_ inside a trusted template, but here the whole message is model-generated and we want its markdown to render —
  so we escape only HTML/link-forming chars and keep the formatting chars. Links aren't in the markdown-lite spec, so
  dropping them is safe.
- **The send command returns early and streams on a worker thread.** `run_turn` holds a non-`Send` rusqlite `Connection`
  across awaits, so its future can't live on the Tauri command future or a multi-thread tokio task; a dedicated thread
  with a current-thread runtime sidesteps that. See `commands/agent.rs`.
