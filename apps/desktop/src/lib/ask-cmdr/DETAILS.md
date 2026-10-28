# Ask Cmdr rail details

Pull-tier docs for `lib/ask-cmdr/`. Must-knows live in `CLAUDE.md`. Backend:
`apps/desktop/src-tauri/src/agent/CLAUDE.md` and `apps/desktop/src-tauri/src/commands/agent/`.

## The IPC surface

Wrappers in `../tauri-commands/ask-cmdr.ts`:

- `sendAskCmdrMessage(conversationId, text, attachments, deniedNames)` — a plain specta command. `conversationId` is
  `null` for a new thread; the resolved id comes back as the promise value, and the answer arrives on the subscription
  below. It answers an `AskCmdrSendOutcome`: `accepted` with the id, or a typed refusal decided before the turn existed
  (no consent, no slot, a local window too small). Those can't be streamed — half of them happen before there is a
  thread to key an event on.
- `onAskCmdrTurn(cb)` — the subscription every turn's progress arrives on, rail sends and wakes alike.
- `agentWakeStatus()` + `onAgentWakeStatus(cb)` — the wake indicator's seed and its subscription. See § The wake
  indicator.
- `cancelAskCmdr(id)`, `getAskCmdrConversation(id, limit, offset)`, `listAskCmdrConversations(...)` — plain specta
  commands.
- `askCmdrToolResult(conversationId, messageId, callId, occurrence)` — what one line's call returned (the preview plus
  the raw JSON for Copy), fetched on the user's expand: the one door a raw result crosses by. § Groups and the expanded
  line.
- `preflightBulkRename` / `reviseBulkRenameRow` / `applyBulkRename` / `cancelBulkRenameProposal` — the review dialog's
  four actions. Only opaque ids cross, plus the one name the user typed.

Nothing here is hand-mirrored: `AskCmdrTurn` / `AskCmdrStreamEvent` are generated like everything they carry
(`MessageView` / `MessageBlock` / `ConversationRow` / `ConversationDetailView`, and the review row's
`RenameProposalRowSnapshot` / `RenameEvidence` / `EvidenceCoverage`, re-exported as `RenameProposalRow` /
`RenameEvidence` / `RenameEvidenceCoverage`, the names the rail uses). ❌ Don't hand-mirror a type that has a generated
counterpart; that's a drift seam.

## The streaming model

**Subscribed by conversation, not by send.** One listener per window (`ask-cmdr-turn-stream.svelte.ts`, started from
`routes/(main)/+page.svelte` so it exists in the MAIN window only) hears every turn the backend is running and hands
each event to two places: `handleTurnEvent` in the stream slice, which keeps only the ones about the thread on screen,
and the sessions slice, which reacts to a thread appearing or going away. That module is the one place allowed to know
both, since the sessions slice calls into the trigger and the trigger never imports it back.

Three things follow from the subscription being conversation-keyed rather than per-invoke, and each has a test in
`ask-cmdr-turn-stream.test.ts`:

- **A reload keeps the answer.** The webview goes away mid-turn and the backend keeps writing into `main.db`; the
  reloaded rail lands on the same thread and the rest of the turn renders. There is no second `assistantStarted` to
  hear, so `adoptLiveTurn` treats ANY live event for the thread on screen as proof a turn is running, and the reducer
  creates the streaming bubble if there isn't one (`ensureAssistant`). The load itself adopts too when the backend says
  a turn runs there (`turnInFlightSinceSeq`, `resumeThread`), so a turn sitting in a long tool call doesn't look idle
  until its next event.
- **Only a STREAMING bubble is written into** (`streamingAssistant`). An ended turn's bubble stays closed, so a turn the
  rail never typed (a wake, or one a reload caught before its first row landed) opens its own. A reload that caught rows
  of the running turn folds that bubble still streaming, which is how the rest of the turn lands in it.
- **A thread can disappear under the rail.** A quiet wake opens a thread, thinks in it, and deletes it seconds later;
  `discarded` is the only way a subscriber can learn that, since there is nothing left to re-read. The rail steps off
  into an empty chat and the session list drops the row.
- **A stopped turn stays stopped.** After a cancel the backend dribbles a chunk or two more (and the round's remaining
  tool calls still run) before its terminal arrives. Those would otherwise read as a live turn and re-enter "working…",
  so the thread holds a pending stop until that terminal: § Stop is local first, settled by the backend.

**One bubble per turn, made of rounds.** An assistant item (`AssistantItem`, `ask-cmdr-messages.ts`) is one turn:
`rounds` (one per model call: `{ messageId, text, calls }`), `turnId` (the opener row's id), and `stats`. A round's text
renders BEFORE its calls whatever order the wire interleaved them in, because that is the order the stored row holds
(genai's `StreamEnd` yields one text, then all calls). `displaySegments` (`ask-cmdr-rounds.ts`) turns rounds into what
the bubble renders: each round's text, then its calls, with consecutive tool runs merged when the round between them
said nothing. Only the last round's prose is a live region. A line is `(messageId, callId)` everywhere, `each` keys
included (`r<index>:<callId>` while the round has no id, plus an occurrence suffix for an id repeated within one round),
because call ids collide across rounds as the norm.

`sendMessage` optimistically appends a `{ kind: 'user' }` item and flips `streaming` on, then calls
`sendAskCmdrMessage`. Events drive the render (`applyStreamEvent`), each delegating to a tiny mutator so the switch
stays simple:

- `started` → the thread exists and is being worked on. A fresh chat has no id until this arrives (or until the send's
  promise resolves, whichever is first); a wake emits it too, which is what tells the session list.
- `assistantStarted` → open a round in the current bubble (creating the bubble if needed).
- **A round with a `messageId` is SEALED** (its row is stored): `textDelta`, `reasoningTick`, and `toolCallStarted`
  arriving on a sealed round open a new one first (`openRound`). That is what makes a reload mid-turn come out right:
  every folded round is sealed, so the live chunks that follow get a round of their own.
- `textDelta` → append to the current round's text, even after its calls started; clear `thinking`.
- `reasoningTick` → set the bubble's `thinking` (a subtle "thinking…" line; the reasoning content itself never crosses).
- `userPersisted` → the user item `sendMessage` typed for this turn (held by reference, never "the last user item") gets
  its id, and the streaming bubble its `turnId` (it arrives after round 1 streamed, on the first `End`). A turn nobody
  typed here (a wake, a rejected sweep's follow-up, a send from before a reload) has no such item, so the event's
  `opener` view goes in above the streaming bubble as the item the fold would make of it (`openerItem`), unless the
  thread already shows that row. Patching the last user item instead gave an EARLIER turn's bubble this turn's id.
- `toolCallStarted` → add a running line to the current round, idempotent by `callId` within the round (ERR-FRMAR).
- `toolCallsResolved` → the round's stored truth, once per tool round: stamp its `messageId` (sealing it), set each
  line's subject and `repeatOf` (and the tool's wire name), add a line whose start never came, and drop a started line
  the batch doesn't list (a nameless call the backend discarded). The k-th call with an id takes the k-th started line
  with it.
- `toolCallFinished` → find the line by `(messageId, callId)` across the thread (newest bubble first) and set its
  outcome and refined subject. `ok` derives from the outcome through `outcomeOk` (`ask-cmdr-tool-line.ts`, an exhaustive
  `Record` mirroring the backend's `ToolOutcome::ok`, plus the `connection` arm, ok only for a live volume); a settled
  line with no outcome reads as done.
- `done` → stamp the current round with its `messageId` FIRST (the answer round is text-only, so no `toolCallsResolved`
  stamped it, and settling would drop it as never stored), attach `turn` as `stats`, finalize, `streaming = false`.
- `failed` → attach `turn` as `stats`, finalize, push a typed `{ kind: 'error' }` item, `streaming = false`. The item
  carries the source error's own wording (`detail`, when the backend has one — a retired model slug, a quota reset time)
  under the friendly headline, rendered as escaped plain text (never `{@html}`), so the user sees what to fix. Display
  only: the UI branches on `errorKind`, never on `detail`. `managedByOrganization` (the organization's MDM policy
  refused the turn, nothing sent) words the rule that refused through `managedAiRefusalMessage`: the send refusal and
  the `failed` event carry it as `managed` beside the unit kind. `askCmdr.error.managedByOrganization` is the fallback
  when none came.
- `stopped` → the backend's terminal for a cancelled turn. After the rail's own Stop it settles the stopped bubble (§
  Stop is local first). With no stop pending here (the status corner stopped a wake) it ends the turn like `done`
  (`endTurn`: stats, finalize, `streaming = false`, the review), so the watchdog never calls an ended turn a timeout.
- `modelChanged` / `chatMemoryChanged` → insert a `{ kind: 'modelChange' }` / `{ kind: 'chatMemoryChange' }` timeline
  line BEFORE the current user bubble (the change happened between the turns; the backend already persisted the event
  row). Both go through the one `insertBeforeCurrentTurn` helper. `chatMemoryChanged` carries a token COUNT and the rail
  owns the sentence, so nothing English is frozen in `main.db`.
- `contextTrimmed` → insert a `{ kind: 'contextTrimmed' }` timeline line before the streaming bubble: the prompt budget
  pushed older tool results out of this turn, so the reply that follows saw less than the whole chat. At most one per
  turn (the backend dedupes). Live-stream only, deliberately NOT persisted: it describes one assembly, not thread
  content, so reloading the thread doesn't replay it. Rationale (why the drop must be loud at all):
  `src-tauri/src/agent/chat/DETAILS.md` § Budget enforcement.
- `proposalDecided` → not a turn event at all, and handled before the reducer: § How an open thread stays current.

**Every terminal path uses the same finalizer, and it settles the bubble the way a reload reads the stored rows**
(`settleAssistant` + `settleRounds`): rounds without a `messageId` are dropped (they never reached `End`, so nothing was
stored: crash case a), calls still running settle with no outcome (no result row exists), empty rounds go, the bubble
takes its last persisted round's id, and a bubble with nothing left is removed. This covers `done`, `failed`, `stopped`,
a stopped turn's terminal, and a send invocation that rejects before stream events can report a typed failure. Tradeoff,
chosen for live-equals-reload: a turn that ends without an answer loses the text of the round it was streaming, because
a reload wouldn't show it either. A Stop defers that loss to the terminal: the round may still reach `End` and be
stored. Every terminal also settles the turn's tool groups per the setting (`settleTurnGroups`, § Groups and the
expanded line).

**Slot-change events, live path.** `settings-applier.ts` calls the trigger's `noteSlotSettingChanged()` on the five
slot-affecting settings (`ai.provider` / `ai.cloudProvider` / `ai.cloudProviderConfigs` / `askCmdr.interactiveModel` /
`askCmdr.chatMemorySize`), which debounces 1 s (outlasting the settings store's 500 ms disk flush, which the backend
re-reads, and the model text field's keystrokes) and then calls `ask_cmdr_record_slot_change` for the active thread. The
backend queues on the thread's single-flight lock — with a turn in flight the promise resolves right after that reply —
and answers one persisted event view per facet that moved, an empty list when nothing effectively changed (no turn yet,
same model and size, or the interactive override masks the changed shared model). A resolution that arrives after the
user switched threads is dropped locally (the rows still show on revisit). History renders the same lines via the
`event`-role fold in `buildRailMessages`. Backend mechanics: `src-tauri/src/agent/chat/DETAILS.md` § Slot-change events.

**Stop is local first, settled by the backend** (`stopStreaming`, `PendingStop`). The click cancels the backend and ends
the turn ON SCREEN at once: `streaming = false`, the bubble's cursor and "thinking…" off, the watchdog cleared, the
review opened over what was staged. The bubble itself keeps everything, the round still streaming and its running lines
included, because dispatch isn't cancellable: the round's calls run on and store their results, and the round may still
reach `End`. The thread holds a pending stop (oldest first, per thread) carrying the bubble, the user item typed for the
turn, and the send that started it. Until the stopped turn's terminal:

- `toolCallsResolved` resolves the stopped bubble's last round, `toolCallFinished` settles its line by
  `(messageId, callId)`, and `userPersisted` patches the stopped turn's own user item (or inserts its opener above its
  bubble). None of them adopts the turn.
- `proposalReady` stages AND opens the review at once: the turn is over on screen, so no turn end is coming to show it.
- Everything else is dropped. That holds after a new send too: the new turn queues behind the stopped one on the
  backend's single-flight lock, so until the stopped turn's terminal, every event of the thread is still its.

The FIRST terminal of any kind (`done`, `failed`, `stopped`) consumes the oldest pending stop of its thread, wherever
the rail is now: `done` stamps the last round as the answer and keeps it, `failed` and `stopped` drop a round that never
reached `End`, and all three attach `turn` and settle the bubble. No error item for a `failed`: the user stopped it. It
never touches `streaming`, the watchdog, or the review (they ended at the click, and a send since may own them).
`discarded` drops the thread's pending stops and goes on to the reducer. A send the backend refused ran no turn, so no
terminal will come: the refusal withdraws its own stop. A thread load forgets the thread's pending stops: if that turn
still runs, the fold shows it running and the load adopts it, and its terminal ends it like any other. The progress
watchdog's timeout is a Stop too (`stopStreaming`, then a `timeout` item).

**The thread waits for exactly the terminals still coming** (`matchStopsToTerminals`). `cancelAskCmdr` answers what the
Stop reached (`CancelOutcomeView`: `tripped`, the turns it newly stopped; `live`, the turns still registered).

- `live: 0`: no terminal is coming (the turn had ended, or a load adopted one that already finished), so every stop the
  thread holds settles now, as a reload reads it. ⚠️ Waiting anyway took the thread's NEXT turn for the stopped one: its
  events were dropped, its answer landed on the old bubble, and the watchdog fired a bogus timeout.
- `tripped` above one: the Stop also reached a turn nobody here waits for (a send queued behind a wake), so a
  placeholder stop (no bubble) takes each extra terminal, which would otherwise end a turn sent since.
- A turn thread that panics still emits a terminal (`Failed`, `unfinishedReply`) before it unregisters.
- Residual race: a terminal emitted just before the Stop can reach the rail AFTER the `live: 0` answer; it then goes to
  the reducer as a turn stopped from elsewhere.

A "load earlier" that joins a stopped bubble with its older half builds a new object, so the stop follows it
(`followJoinedBubble`).

**Terminal events carry the turn's stats.** `done`, `failed`, and `stopped` carry `turn: TurnStats | null` (tool calls,
wall time, summed tokens), `null` for a turn that ran no tool or a refusal from outside a turn. Each puts it on the
bubble's `stats`, a stopped turn's included, and the bubble renders it as its stats line (§ The tool line).

**History folds into the same bubbles** (`buildRailMessages`, `ask-cmdr-history.ts`), loaded through
`getAskCmdrConversation` on rail open. Rows group by the backend's `turnId` (never a frontend guess at turn edges): each
assistant row is one round (its text blocks joined, its `toolCall` blocks as lines carrying their own subject, outcome,
and repeat mark), the bubble's id is the turn's last assistant row, and its stats ride that row's `turnStats`. An event
row with `duringTurn` is hoisted above its turn's bubble (where the live rail's `showDecision` put it); the rest stay in
place. An event row with no blocks (a turn's `TurnFinished` record, an unknown kind) draws nothing. Result-less calls
fold settled with no outcome (their turn ended without one), except in the turn still running: rows after the backend's
`turnInFlightSinceSeq` are that turn's, so their result-less calls fold `running`, their bubble folds `streaming`, and
an event row among them is hoisted above it (the turn is still going, so it is mid-turn). With no row after the mark
yet, every bubble folds closed and the first live event opens a new one. `ask-cmdr-turn-parity.test.ts` pins that the
live reducer and the fold build EQUAL items for one scripted turn (interleaved deltas, two rounds reusing `fake-call-0`,
a mid-turn and a post-turn decision).

## The tool line

`AskCmdrToolLine.svelte` is one calm row: a glyph, the label naming what the call looked at, a "Same as above" tag, and
a quieter note saying what came back. Every word is ours: the backend sends typed facts (`ToolSubject`, `ToolOutcome`,
`repeatOf`, `TurnStats`) and `ask-cmdr-labels.ts` picks the catalog message per variant.

- **The label** (`toolLineLabel`): keys stay per TOOL (doing while running, done after), and a subject that names
  something swaps in a key with params (`SUBJECT_WORDS`, a `Record` over every subject kind, so a new kind is a compile
  error). `null` or `unparsed` subjects keep the tool's param-less words, an unknown name keeps "Working" / "Used a
  tool", and a named `unknownTool` says what it asked for. Search has four shapes: pattern plus folders, pattern only,
  folders only, neither. A count rides as `{countText}` plus `{count}`.
- **Decision: a refused note says "Tried to…".** The memory tools' done words ("Saved a note") claim the very thing a
  refusal denies, so those two tools carry a `tried` key the label uses when the outcome isn't ok; so does
  `connect_volume` ("Connected to your server" vs "Tried connecting…", its outcome note saying why). Every other tool's
  done words describe the activity, and its outcome note says it was turned down. A `volume` subject keeps the tool's
  own words: its id is a token, not a name a person knows the server by.
- **Paths are links.** A message names its paths with tags (`<place>` one, `<places>` a list), rendered through
  `<Trans>` snippets as `LinkButton`s; `<places>` joins its links with `conjunctionListParts` (`#lib/intl/list-format`),
  the language's own "and". A link shows the path's last segment (`placeName`), its full path in a tooltip and in its
  accessible name ("Show {path} in a pane"). The backend decides what's a place: a subject path is a `LinePath`
  (`{ text, linkable }`), linkable only when absolute and whole (a relative or cut path stays plain text), a preview
  row's `path` is set only then, and a note path is linked only when the backend resolved it inside the notes folder
  (`absolute`). Click → `revealPath({ path, kind })`, a prop handed down from `routes/(main)/+page.svelte`
  (`revealPathInBestPane`, over the `navigate-and-select` primitives), then `returnFocusToPane()`. Only subject paths
  reach it, ❌ never a raw search scope string.
- **Gotcha: a path link can't be ellipsized from outside.** A `LinkButton` is a `<button>`, an atomic inline box, so the
  label's `text-overflow` replaced the whole link and every word after it: a long PDF name rendered as "Looked inside …"
  (seen live, 2026-10-26). A label holding a path (`with-places`) is a flex row instead, where only the path shrinks and
  truncates itself; `white-space: pre` keeps the spaces a flex row would drop at its pieces' edges. The preview's linked
  row names truncate themselves the same way, beside a size that stays on their line.
- **What came back** (`toolOutcomeText`): a `Record` over every outcome kind. Counts, "showed N of M" when the model got
  part of a list, "so far" / "yet" for a lower bound (❌ never "nothing found" while ground is uncovered), one note per
  refusal reason and per `MemoryRefusalKind`, and nothing for `answered` / `saved` (the label and glyph say it), nor for
  a `files` subject's `items` count that answered every path the label names (`restatesSubject`: "Looked inside X and 1
  more" never adds "2 items"; a partial answer still says "showed 1 of 2"). A settled call with no outcome says no
  answer was recorded. A muted `·` (`aria-hidden`) sets the note apart on screen, and a visually hidden
  `askCmdr.tool.outcomeSeparator` (a comma) does it for a screen reader, so the two never read as one sentence.
- **The repeat tag** (`showsRepeatTag`) marks a call with `repeatOf`, and the link's `aria-describedby` points at it.
  The breaker's own `refused: repeatedCall` already says so in its note, so it gets no tag.
- **Not a live region.** Lines carry no `role="status"` / `aria-busy`: a 12-call turn would chatter. The last round's
  prose is the bubble's one live region.
- **The stats line** (`AskCmdrTurnStats.svelte`, `turnStatsText`) sits under any bubble whose `stats` ran a tool,
  answered, failed, or stopped: calls, wall time (`formatMilliseconds`, left out for an old turn with `wallMs: null`),
  and tokens. Tokens are the cost meter's (`totalTokens`: prompt plus completion per row), so the turns add up to the
  thread's `AskCmdrCostFooter`. A stopped turn leads with "Stopped" (`TurnStats.stopped`, set by `run_turn` on a cancel
  and stored in the `TurnFinished` row, so live and reload agree). A turn stored before that field existed reads as not
  stopped, and a stopped turn that ran no tool has no stats line, so it gets no marker either.

## Groups and the expanded line

A `tools` display segment of two or more lines renders as `AskCmdrToolGroup.svelte`: one quiet header ("Looked around: 6
folders, 2 searches", `groupSummaryText` in `ask-cmdr-tool-group-summary.ts`) over its lines. A single line renders
alone, with no group chrome.

- **The header counts per category** (`SUBJECT_CATEGORY`, a `Record` over every subject kind): folders, files (every
  path an `inspect_file` call asked about counts, so "3 files" stays true for one call), searches, checks (panes,
  drives, the operation log, suggestions), plans, notes, and "other" (an unknown tool, and a line with no subject yet,
  so an unresolved call never breaks the header). Categories join with `formatUnitList`. "Looking around" while any line
  runs.
- **Open state lives outside the message list** (`ask-cmdr-tool-groups.svelte.ts`): `AskCmdrRail.svelte` keys messages
  by index, so component state would jump on a prepend. A group is keyed by its first line's key once that round has a
  row (`groupKey`; calls only append, so it holds for the turn), a line by its display key. A group with no key yet
  can't be toggled. Nothing is persisted.
- **Lifecycle.** Live, an undecided group is open, and a toggle the person makes sticks. The turn's terminal (any kind,
  a pending stop's included) applies `askCmdr.toolGroupsOnAnswer` (Settings > Advanced > Ask Cmdr): `'collapse'` closes
  the turn's groups, `'keep'` freezes each as it stands (an untouched one stays open). A local Stop holds each untouched
  group open until that terminal (`holdTurnGroupsOpen`, again for a round the stopped turn resolves after the click,
  whose group gets a new key), since the bubble stops streaming at the click. Reloaded groups start closed under both
  values (decision: "as you left it" has nothing to remember across a reload), except one with a running line after a
  reload mid-turn. A thread switch forgets every group's state (`forgetToolDetails`), so reopening a thread in the same
  session matches a cold reload. Before a collapse drops a focused line, `$effect.pre` moves focus to the group's
  toggle.
- **The seam.** When "load earlier" folds a turn back together, the newer half's first group may now continue an older
  group under that group's key; `carryGroupStateAcrossSeam` moves its entry there.
- **a11y.** The header is a plain `aria-expanded` button, never a live region. Each bubble has ONE visually hidden
  polite status that says a live group's summary once, when all its lines settled (`claimGroupAnnouncement`); a reloaded
  bubble announces nothing.

**The expanded line.** Each line ends in its own expand control (a path link can't nest in a button), enabled only when
a stored result exists: not while running, not before the round has a row, not on a settled `outcome: null` line
(disabled, it's hidden but keeps its place so the row doesn't shift). The first expand calls `askCmdrToolResult` with
the line's `(messageId, callId)`, its `occurrence` (which of the round's calls with that id it is, so a provider naming
one id twice in a round still fetches each line's own result, the way the backend pairs the page), and the conversation
on screen.

- **The cache is private**: a `SvelteMap` keyed by line key (it never proxies its values), never inside the deep
  `askCmdrState` message list, and ❌ never logged (the raw result can hold file text and EXIF incl. GPS). An answer
  arriving after the person switched threads is dropped, and a thread switch forgets every fetched result
  (`forgetToolDetails`).
- **The preview** (`AskCmdrToolPreview.svelte`, notes from `ask-cmdr-tool-preview.ts`): the first rows (name, its path a
  `LinkButton` through `revealPath` then `returnFocusToPane`, size, `find` line and snippet, a per-path status), "Showed
  the model N of M" (or "found so far" for a lower bound), "Here are the first N" when the preview holds fewer than the
  model got, the verbatim `problem` labeled as what the model was told, the set-aside note, or "no list" for a
  facts-only tool. `notFound` (a thread deleted underneath) and `unavailable` each get one calm line.
- **Copy** (`Button variant="secondary" size="mini"`, `copyToClipboard`) copies the raw JSON, which renders nowhere.
  "Copied" for two seconds; a clipboard that refuses says so.

## What a wake's thread opens with

A thread the agent started for itself begins with a `wakeDigest` block in the user-role row rather than text, so
`buildRailMessages` folds it into its own `RailMessage` and `AskCmdrWakeDigest.svelte` renders it collapsed.

**Everything in that block is ours to word.** The backend sends folders, four counts each, and the rollups — numbers and
paths, no sentence (`agent/wake/DETAILS.md` § The rendered digest is prompt-only). The English digest the model reads
never crosses IPC, so nothing here can leak untranslated backend copy into ten locales. ❌ Don't "simplify" this by
having the backend send a ready line.

Two things the block owes the reader: the collapsed summary counts the rolled-up folders too (otherwise it disagrees
with what expanding shows), and the rollup line stays, because admitting how many folders the digest had no room for is
the point of having one.

**Live, the digest arrives on the next load, not mid-turn.** `userPersisted` carries the persisted opener as a
`MessageView` (`opener`), but the reducer only stamps the id today, so a rail opened onto a wake already in flight shows
the answer streaming above an empty spot until the thread is re-read. Inserting `opener` when the rail lacks it is the
fix.

## What the user answered about a suggestion

`AskCmdrProposalDecisions.svelte`, under the same "numbers and paths, never a sentence" contract as the digest above.

**Decision: two different transcript rows render through one block.** An `event`-role row carries a single decision,
written the moment somebody answered; a `user`-role row carries a whole sweep's worth, because it is the opener of the
follow-up turn a rejected sweep earns (`agent/wake/DETAILS.md` § The turn a rejection earns). Both fold to
`kind: 'proposalDecisions'`. **Why**: they say the same thing, in the same words, about the same kind of object. Two
shapes would be two renderers and two sets of strings drifting apart, and the only thing that actually differs is how
many decisions are in the list.

**An approved run always shows its result line, zeros included.** A group can be approved and then skip every file
behind a fingerprint mismatch, and the outcome the store records is what SETTLED, not what was claimed. Hiding a partial
run would leave the user believing their files moved.

The seven verb names are a spelled-out `Record`, ❌ not a key built from the verb token at runtime:
`desktop-message-keys-unused` reads a runtime-built key as dead translation work unless it is on that check's closed
dynamic-prefix allowlist, and seven literals are cheaper than an allowlist entry.

### How an open thread stays current

The single-decision row is announced as it is written: `agent/outcomes.rs` emits `proposalDecided` on the turn
transport, under the conversation the row went into, carrying the row's id and the decision. `handleTurnEvent` takes it
before any turn rule applies and `showDecision` adds ONE line to the thread on screen. Each case has a test in
`ask-cmdr-turn-stream.test.ts`.

**Decision: the rail patches from the event's payload, it doesn't re-read the thread.** **Why**: a re-read replaces the
whole message list. That throws away an answer that is mid-stream (its text isn't persisted until `done`) and the older
pages the user loaded, and its reply can come back after the user moved to another thread. A patch is synchronous, so
the conversation filter is the whole guard and there is nothing to arrive late.

**Decision: the signal is its own event, ❌ not `suggestions-changed`.** **Why**: that event says the PENDING SET moved,
which is a different fact at the one moment it matters. It says `approved` at the claim, while an approval's line is
written when the operation starts and rewritten when it settles (`agent/suggested_ops/DETAILS.md` § What the user's
answer teaches the agent), so a rail listening to it would look before there is anything to see. It also fires for a
dismissed review, which writes no line, and it goes to every window under a rule that it carries no display text.
Emitting where the row is written makes the other cases fall out: a sweep with no thread (or a deleted one) writes no
row and so announces nothing, and "reject all" over a sweep is one row and one line per group, in whichever thread each
group's sweep belongs to.

What the reducer has to get right:

- **It is the one event that isn't part of a turn.** It never sets `streaming`, never hands a fresh chat an id (only
  `started` does), and a thread on the stopped list still shows it.
- **It goes above a bubble that is still streaming.** The streaming bubble has to stay last, or the next chunk opens a
  second one and splits the answer.
- **One row is one line, by row id.** The row is written before the event is emitted, so a thread load can return the
  row and the event still follow. A row heard again REPLACES its line: an approval's says "running" at the start and is
  rewritten with its outcome at settle (#345). ❌ A `running` decision never replaces a line already shown: the two
  announcements come off different threads, and a load can return the settled row before a late "running" arrives.
- **A decision heard during a thread load is held and put back when the load ends** (`endThreadLoad` →
  `settleDecisionsHeardWhileLoading`). The load's read can be a moment older than the decision, and it replaces the list
  the line was shown in, or the rail isn't on that thread yet.

The sweep-wide `user`-role row still arrives on the next load, exactly like the digest: it opens a turn, and the reducer
doesn't insert `userPersisted`'s `opener` yet.

## The staged-proposal toast

The one time the proactive agent interrupts. `agent-wake-staged` (`src-tauri/src/agent/wake/staged.rs`) fires when a
wake's turn ends having staged at least one proposal; `wake-toast.svelte.ts` turns it into a toast and
`WakeStagedToastContent.svelte` renders it, started from `routes/(main)/window-services.ts` in the main window only (the
event reaches every window, and the settings window would otherwise raise its own copy).

- **`askCmdr.wakeToast` is read at ANNOUNCE time**, never at subscribe time, so turning it off silences the wake already
  in flight. That is the only reading of the switch that does what somebody flipping it mid-wake meant.
- **The backend never emits for zero.** A quiet wake, and a wake whose model proposed nothing, say nothing at all.
- **It AUTO-DISMISSES**, unlike the operation-failure toast. Nothing is lost when it goes: the proposals sit in the
  suggestions badge until they are reviewed, so a persistent toast would just be a thing to close.
- **Two actions, answering different questions**: "Review" opens the suggestions (WHAT it wants to do), and "See why"
  opens the thread it reasoned in (WHY). Nobody asked for any of this, so the second one earns its place. ⚠️
  `switchToThread` before `openRail`, for the same reason the indicator's click does it that way.
- Grouped under `agent-wake-staged` with a cap of two, so a run of wakes can't push unrelated toasts off the screen.

## The wake indicator

The status corner's word on the proactive half: `wake-indicator.svelte.ts` holds the state and the subscription,
`WakeIndicator.svelte` renders it, and `routes/(main)/window-services.ts` starts it in the main window only (the event
reaches every window, and only this one has a corner).

**Its own event, not the turn stream.** `agent-wake-status` carries a `WakePhase` (`idle`, or `thinking` with the
conversation id) plus the readiness gap. The turn stream carries a turn's PROGRESS to whoever is showing that thread;
this carries a phase to a corner showing no thread at all, so folding them would subscribe the corner to every text
delta of every rail send. The one read at startup is not redundant with the subscription: a wake already running when
the window opened announced itself before anyone was listening, and so did a gate that closed before then. The
subscription goes up first and the seed loses to any event that lands while its read is in flight (an event counter
compared across the `await`): that event is newer, and applying the seed after it would put a just-started wake back to
idle.

### What renders, and what does not

`wakeIndicatorMode` is the whole decision, and it exists because two docs used to state the rule differently.
`agent/wake/readiness.rs` held that every readiness gap is worth reporting: somebody who declined Full Disk Access and
somebody with a tidy Downloads folder otherwise see the identical nothing, and only one of those is the feature working.
`SuggestedOpsIndicator` held that an always-present control for a feature with nothing to say is noise. Read together
literally, they put a permanent AI nag in front of every user who never wanted AI.

The resolution: a gap is reported to somebody who opted IN and hit a wall, and to nobody else.

- `silent` — no consent, AI itself turned off (`'off'`), `askCmdr.proactive` off, or ready and idle. Nothing is being
  watched, or nothing has happened, so the corner says nothing. ⚠️ `'off'` is the sharpest case of the rule above:
  naming a gap there would tell somebody to finish setting up a provider they deliberately switched off.
- `thinking` — a wake is on a provider right now. ⚠️ This one renders REGARDLESS of the setting: it is spending the
  user's money at that moment, and a forced wake (or a setting turned off mid-turn) must not be able to run invisibly.
- `needsFullDiskAccess` / `needsApiKey` — the two closable gaps, each with the screen that closes it (the system privacy
  pane, and AI > Provider). Both wear the same `triangle-alert` glyph in `--color-warning-text`; the TOOLTIP is what
  separates them, so each gap's sentence has to name its own screen. Warning tint, never error: the gap is a step left
  to finish.

Every state renders as a bare tinted glyph with no chip around it. The corner overlays the titlebar, where a bordered
box on a lone icon reads as a blob; the hourglass beside it presents itself the same way. The `thinking` state is two
buttons (bot + stop) held together by the span's `--spacing-xxs` gap alone.

### Its two actions

Clicking the thinking glyph runs `switchToThread(id)` and THEN `openRail()`. That order matters: a closed→open
transition otherwise bootstraps the most recent thread and wastes a fetch on one we are about to replace. The turn's own
events keep arriving on the conversation-keyed stream, so the rail fills in as the wake writes.

The stop button is `cancelAskCmdr(id)`, the same command the composer's Stop calls. A wake registers its cancel token in
the one registry (`agent/chat/cancel.rs`), so there is no wake-specific stop to keep in step — which is also why the
registry lives below `commands/` rather than inside the chat command.

## Sessions, search, message paging

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
- **A thread the agent opened wears a `bot` glyph**, the same one the status corner shows while a wake thinks, in the
  list AND in search results (`ConversationSearchHit.origin` exists for that second half: a hit that lost the mark reads
  as a thread the user started and forgot). ❌ The test is `origin === 'notification'`, never "origin is not null": the
  reserved `quietWakes` ledger row carries an origin too, and so will whatever token comes next.
- **Message paging is tail-first** (a chat shows newest at the bottom). `loadConversation` probes page 0 to learn
  `totalMessages`, then refetches the newest page when the thread exceeds `MESSAGE_PAGE`; `historyCount` tracks how many
  rows are loaded from the tail. "Load earlier" (`loadOlderMessages`) prepends the previous page, its offset derived
  from `messageTotal - historyCount` so pages tile without overlap and live-streamed rows (newer than the load-time
  total) are never disturbed. The rail preserves the scroll position across a prepend (capture `scrollHeight` before,
  restore after) and its auto-scroll-to-bottom only fires when the user was already near the bottom (`wasNearBottom`,
  tracked on scroll), so streaming follows but loading older doesn't jump. The backend pairs each call with its result
  across a page seam (`src-tauri/src/agent/chat/DETAILS.md` § What a reloaded page says), so a history line reads its
  `outcome` and `subject` off its own `toolCall` block; tool rows carry no blocks. A 12-call turn is about 20 rows
  against a 50-row page, so seams cut turns routinely: `loadOlderMessages` joins the halves with `mergeAtSeam`
  (`ask-cmdr-rounds.ts`), which merges the older page's last bubble with the newer list's first bubble exactly when
  their `turnId`s match (rounds concatenated, the newer half's id and flags, the stats that exist), keeping any event
  the newer page hoisted above it.
- **The "start a fresh one?" nudge counts user items + ROUNDS + every other item** (`softCapCount`) against
  `THREAD_SOFT_CAP_MESSAGES` (40, mirroring the Rust constant). Each round counts, as each assistant row did when it was
  its own bubble, so folding a turn into one bubble didn't make the nudge fire far later. A `TurnFinished` row draws no
  item, so it doesn't count.
- **Only the latest thread load may touch the rail** (`latestThreadLoad` in `ask-cmdr-trigger.svelte.ts`). Every read is
  its own command, so the first one asked can be the last one answered: picking thread A and then B must leave B on
  screen whichever answers second, and a late answer must not repaint a chat the user cleared with "new chat". An
  overtaken load writes nothing, the `loadingHistory` flag included (the newer load owns ending it). "Load earlier"
  drops its page the same way when any thread load happened since it asked, because its offset tiled against a list that
  is no longer on screen. ❌ Don't add a read that writes thread state after an `await` without this check.

## Attachments by reference

- The composer stages `AttachmentRef { path, kind }` chips (`askCmdrState.attachments`), sent with the next message and
  cleared after. They ride into the context envelope as `attached: <path> (<kind>)` on the latest user turn — **path +
  kind only, structurally never contents** (the read-only privacy line). History user rows carry no chips (the refs were
  envelope text, not stored blocks).
- **"Ask about selection"** (the paperclip button) calls its `readSelection` prop (`+page.svelte` →
  `ExplorerAPI.readSelectionAttachments` → `../file-explorer/pane/selection-attachments-read.ts`): the focused pane's
  WHOLE selection (or cursor item) as typed refs, resolved against the BACKEND listing, revision-checked, no filesystem
  stat. ❌ Never resolve it from `PaneStateStore`: that holds only each pane's loaded window, so a selection scrolled
  past it attached short, silently. Past `SELECTION_ATTACHMENT_CAP` (100) it keeps the first rows in pane order and the
  composer toasts how many it left out (`askCmdr.composer.selectionCapped`); every ref rides into the next turn's
  context, so the cap protects the user's tokens. The chip row scrolls past `8lh`.
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
- **A broken rail closes; the window stays.** The route wraps `{#if askCmdrState.open}` in a `DialogBoundary` whose
  `onClose` is `closeRail` and whose toast is `askCmdr.railBoundary.closedToast`. A throw while the rail renders or
  tears down closes it the normal way (state, persistence, window shrink, focus home), and reopening works. Why it sits
  above the `{#if}`, and the lint rule (`cmdr/dialog-inside-boundary` covers `*Rail`): `apps/desktop/src/DETAILS.md` §
  Error boundaries. Regression anchor: cmdr-reports#78 (`each_key_duplicate` in the rail blanked the main window). Test:
  `rail-boundary.svelte.test.ts`.
- **Focus never falls to `<body>` from the rail** (what the focus watchdog's `activeElement=body` lines were). Four ways
  it did, each closed where it happens:
  - A click on rail text or a button: WebKit doesn't focus a clicked button, and with no focusable ancestor it CLEARS
    focus. The `<aside>` is `tabindex="-1"` (out of the tab order, no ring), so a click lands focus on the rail, and its
    `focusin` sets the rail-focused flag.
  - Send and Stop swap as a turn starts and ends, so a focused one leaves the page: both hand focus to the prompt box.
  - The sessions panel unmounts with its focused control: `closeSessions` asks for the composer; a keyboard rename hands
    focus to its row.
  - "Load earlier" vanishes with the last page: the rail takes focus back. A turn that ends on its own while Stop holds
    keyboard focus still drops it (nothing to hook without a focus event); the watchdog's log now names the cause ("…
    left the page while it had focus"), so a new leak points at its component.
- **New chat** (`openNewChat`: the header button, the getting-long nudge, the sessions panel, and `askCmdr.newChat`,
  default ⌘N) resets the thread, opens a closed rail on the FRESH chat (`openRail({ freshChat: true })` skips the
  last-thread bootstrap), and bumps `composerFocusRequest`, which the composer's focus effect follows. ⌘N was free: Cmdr
  has one main window (macOS's "New window" meaning has nothing to claim), tabs are ⌘T, and the search dialog's own ⌘N
  runs only while that dialog is up. Not a menu item, so it lives in the registry and handlers only.

## The toggle wiring

The rail's toggle lives in four places, and a miss fails silently (no error, just a shortcut or menu item that does
nothing):

- The frontend command registry, plus `COMMAND_IDS`, plus the `askCmdr.toggle` handler.
- Rust `command_map.rs`.
- Its row in the View menu in `menu/menu_bar.rs`.
- `shortcuts-store.ts` `menuCommands`.

The default is `⌘⌥A`, registered **Command-then-Option**: `⌥⌘`-order strings are native-menu-only, so writing the
shortcut that way in the registry leaves it unmatched. `ask-cmdr-shortcut.test.ts` pins all of it.

## Window growth (panes keep their size)

Opening the rail grows the MAIN window by the rail's width instead of squeezing the panes; closing shrinks it back.
`rail-window.ts` is the Tauri wrapper (`growMainWindowForRail` / `shrinkMainWindowForRail`) over the pure geometry in
`window-positioning-utils.ts` (`growRectForRail` / `shrinkRectForRail`, unit-tested). The main window's own
`capabilities/default.json` grants `set-size` + `set-position` (the read getters and `available-monitors` are already in
`core:default`).

- **Grows rightward** (left edge put, so the panes don't jump), **slides left** only when the right edge would leave the
  monitor, and **caps at the monitor width** — past that the panes do give up space (nowhere else to take it from). This
  is the "max width = screen width" case.
- **Fullscreen / maximized are left alone** (`fillsScreen` bails): the window already fills the screen, so the flex
  layout shrinks the panes — the same capped fallback.
- **E2E runs skip the resize entirely** (`isE2eRun()` guards both functions). E2E deliberately keeps the main window
  ordered to the back (`show_main_window` → `orderBack:`); a `setSize` / `setPosition` re-fronts the window, so it would
  pop over the developer's work and intercept clicks mid-run — the exact disruption the backgrounding exists to avoid.
  Skipping it costs nothing (no E2E spec asserts the window size).
- **Close reverses exactly what open did.** `growMainWindowForRail` records `{grewBy, shiftedLeftBy}`; close consumes
  it, so a manual window resize or a rail-width drag (absorbed into the panes) between open and close is preserved —
  only the rail's own contribution is removed. With no record (rail open at startup, so hydration never grew it — see
  below), close falls back to removing one rail width so a persisted-open window still shrinks.
- **Hydration must NOT grow.** `hydrateRail` calls `openRail({ resizeWindow: false })`: the window is restored by
  `apps/desktop/src-tauri/src/window_state` at its persisted (rail-inclusive) size, so growing again would double it.
  Re-opens (after consenting) also skip growth via the `!wasOpen` guard in `openRail`.
- The left-edge drag handle resizes (double-click resets to 340). Focus: an `$effect` focuses the composer on mount (the
  rail mounts on open); `markRailFocused` on composer focus; Escape → `returnFocusToPane`
  (`.dual-pane-explorer.focus()`).

## One review for one job

A rename too big for one model reply arrives as a run of staged plans: `budget::files_per_batch` caps a batch at about
101 rows, because the reply, not the context window, is the binding half, and overshooting cuts the plan off mid-JSON.
Each batch used to open its own dialog, and each `openRenameReview` cancelled the plan the previous one was showing, so
a user answering a 500-file job was asked the same question several times and could watch a half-answered plan vanish.

Now `proposalReady` STAGES a plan (`stageRenameProposal`) and the end of the turn opens one review over everything
staged (`openStagedRenameReview`). Every way a turn can end flushes: `done`, `failed`, and the user's own Stop, since
the plans exist on the spine either way and a staged plan nobody sees is one nobody can answer. A `discarded` thread is
the exception — there is no conversation left to review against, so its plans are handed back with
`cancelBulkRenameProposal`.

The cost of the turn boundary, taken deliberately: nothing shows while the model works through the batches. Opening
immediately and growing the list was the alternative, and it makes preflight a moving target under a user who is already
reading. Two more were considered and rejected:

- ❌ **Predicting the batch count** from the file total and `files_per_batch`: the model may propose fewer rows than
  hinted, and then the review never opens. A prediction that can silently fail to resolve is worse than a wait.
- ❌ **Per-rule approval** (approve the renaming rule, not each row), declined by David: approval stays per row, and the
  volume problem is already answered by the suggested-ops dialog's group-level Approve / Reject (`approveGroup` /
  `rejectGroup`), so 500 files never mean 500 clicks.

**The grouping is presentational; the proposal stays the unit.** `BulkRenameReview` is a list of
`BulkRenameReviewProposal`s, each with its own rows, `preflighting`, `expired`, `revision`, `changed`, and
`requestVersion`, because preflight, revise, apply, and cancel are all keyed by proposal id on the backend
(`propose/rename/store.rs`). So every guardrail stays per row: the evidence check, pane-scoped source validation, the
fingerprinted preflight, and the fingerprint recheck at write time. Apply loops over the batches in STAGING order, one
`apply_bulk_rename` each, and stops at the first refusal, leaving the rest revalidated and still answerable. A batch the
user turned down whole is consumed with the rest when Apply finishes: they answered it.

A rename group binds one parent folder (`start_bulk_rename` refuses a row that would change it), so a job across folders
is several batches. The dialog puts a folder heading above each run of rows for exactly that case, and shows none at all
when every row shares a folder.

Thumbnail tokens are minted per ROW rather than per proposal, so a later batch doesn't re-mint the rows already on
screen, and a row that leaves the review owes its token back right then (the token map has no window-close choke point).

## Rename review apply

`BulkRenameReviewDialog` owns the user's allow/deny decisions. Its Apply action sends only the staged proposal id and
the currently allowed row ids to `apply_bulk_rename`; it cannot supply a path, destination name, fingerprint, or
approval from the model. The backend requires that exact subset to have passed the latest preflight, rechecks it if the
client is stale, claims the proposal in one conditional transaction (so it can't be started twice), then returns a
queued operation id. The dialog closes only after that operation has started.

**Preflight and apply send the revision the review SHOWED** (`BulkRenameReviewProposal.revision`, from the
`proposalReady` snapshot), never one read at click time. An agent amend bumps the plan's revision, so a batch amended
since it was shown answers `listChanged` (a preflight status, or the typed `BulkRenameError` on apply): the batch is
marked `changed` and treated as stale. Nothing brings an amended rename plan back into this review (an amend comes
through `propose_suggestions`, which emits no `proposalReady`), so the suggestions dialog's "The AI changed this
suggestion while you were reading it." notice takes the batch's place, the way an expired batch's does, and Apply leaves
it out. Reading the revision at click time would bind whatever the agent amended the plan into, which nobody read. A
user's own revise doesn't bump it: the binding digest covers that.

Backend preflight verifies every source still exists and blocks any missing source, including one removed after the
dialog opened. The dialog deselects that row and shows a red, accessible warning; a matching watcher event rechecks it
if the source returns. Preflight also supplies additive row warnings. The dialog keeps extension changes allowed but
marks them with an accessible yellow badge explaining that a rename does not convert file contents; dependency cycles
use the same warning channel. Its cycle tooltip explains that Cmdr uses one temporary name while rotating the files.
Blockers remain separate and automatically clear the row's Allow decision.

The dialog subscribes to the same `directory-diff` stream that updates the file panes. A change whose filename matches a
proposal source or destination reruns the authoritative preflight for every displayed row, including denied and
previously blocked rows. This keeps target-exists and source-missing warnings live and lets a row recover when an
external process removes the clash or restores a missing source; matching names are only an IPC filter, never filesystem
authority. `TargetExists` and `SourceMissing` rows are deselected and show specific red warnings, while the write
engine's exclusive final rename remains the data-safety boundary.

## Undo after a batch lands

Apply hands back a queued operation id, and `noteRenameApplied` turns it into a `renameApplied` rail line: "Renamed 23
files." plus an Undo. A batch that ran as a move (a rename that copies on S3) can't swap names, so Apply also hands back
`swapsLeftOut`: those rows come off the count and get their own line (`askCmdr.renameUndo.swapsSkipped`). This is the
only safety net that fires after the names are real, which is the only moment the user can tell a name is wrong.

A line per batch, so a run of several reads as a run. One Apply over a multi-batch review produces that run in one go,
which is the same shape as a run built one turn at a time. Only the newest still-undoable line carries the job-wide
"Undo all N batches", and its `jobOperationIds` are built from the run's own lines — never from a previous line's set,
which already includes its predecessors (that bug shipped duplicate ids to the backend and a test caught it).

A batch applied while the operation log is off (`operationLog.enabled`) was never journaled, so its line lands as
`notRecorded`: the count, a note saying it can't be undone, and no button. It stays out of the run, so the job-wide undo
keeps covering the batches that were recorded (`#lib/operation-log/recording.ts`).

**The id order is the data-safety part.** `undoOperations` receives them in APPLY order and the backend reverses
newest-batch-first (`crates/cmdr-operation-log/src/rollback/order.rs`): a later batch can have renamed a file into a
name an earlier batch freed, and oldest-first then finds that name occupied, correctly refuses to overwrite, and leaves
the file unrestored. Apply order is also what breaks a same-second tie in the journal's whole-second clock.

The call resolves only when the reversal has actually finished, so the line reports what came back rather than claiming
success on dispatch. `rename-undo.ts` maps the report to a display state, and anything left behind outranks what
succeeded: a skipped file or a refused batch renders `partial`, never `undone`. A refused batch is counted apart from
skipped files because it carries no per-file numbers. Every line a job undo covered goes `unavailable`, so no line
offers an Undo the backend would now refuse. Live-stream only: undo needs the operation id, and a reopened thread could
otherwise offer an Undo for a batch reversed from the operation log meanwhile.

**Each reason gets its own line, naming the file when it applies to just one** ("Left `invoice-2026.pdf` alone: it
changed since the rename."). The backend carries a per-reason breakdown (`SkipBreakdown { reason, count, exampleName }`,
complete counts — see `crates/cmdr-operation-log/DETAILS.md` § Undoing a job); `undoStateFromReport` merges those groups
across the job's batches (counts sum; the first example wins, and the report arrives newest-batch-first) and
`undoSkipMessage` maps each typed reason to copy through an exhaustive record, so a new reason is a compile error until
it's decided.

Two catalog keys per reason (`.named` / `.counted`), not one ICU plural: "name the file" vs "count them" is a display
choice, and a locale whose plural has only `other` (`zh`, `vi`) couldn't express both from one message.

**Whatever no reason accounts for is still said by class**, with the leftover count ("Left 4 files alone: they changed…,
or the old name is taken again."). That covers a batch undone before the reason column existed (its rows read "reason
not recorded") and `alreadyGone`, which maps to no line because it counts as restored. A missing reason must never
shrink the skipped count the line admits to. `vanished` (the renamed file is gone and its old name absent too) has its
own line, since the class line ("Left … alone") would misstate a file that isn't there.

## Editing a proposed name

A row used to be allow-or-deny, so a plausible wrong name left the user two options: the model's name or the old one.
That's the pressure that produces "approved because it looked plausible", so the proposed name is a text field, and a
wrong name gets corrected in place.

- **The field is an edit buffer; the server is the authority.** `commitName` (blur, or Enter) calls the trigger's
  `reviseRenameRow`, which calls `revise_bulk_rename_row` and then writes the RETURNED row into state (name, evidence,
  coverage) before re-running the preflight. It never patches `destinationName` locally: the backend validates the name,
  swaps the row's evidence for `userEdited`, and clears the accepted preflight, so a locally-patched name would show a
  name the backend never took. After every commit the field is set back to the row's stored name, which is what reverts
  a refused edit. Escape abandons the edit (and stops propagating, so it doesn't close the review).
- **Never disabled.** Not while preflighting (a watcher event mid-typing would steal focus and drop the edit) and not on
  a blocked row — an occupied destination is fixed by typing a different name, so that's the row that needs the field
  most. A commit during an in-flight preflight is safe: `refreshRenamePreflight`'s `requestVersion` discards the older
  response.
- **A refused name says so on the row** (`nameRejected` → the red line plus `aria-invalid`), in ONE localized string.
  The backend's own wording is written for the model and stays in the log.
- **The row's provenance state is scannable** (`rename-name-provenance.ts`, unit tested): `contentRead` (an image
  source, so the backend verified something was read), `nothingRead`, `nameKept` (nothing read AND the name didn't
  change), `userEdited`. The last two badges exist because M4 tells the model to keep a neutral name when it couldn't
  read a file, and that instruction is worthless if the user can't see which rows took that path — so the copy keeps
  saying nothing inside the file was read, and never softens into something reassuring. A case-only change is a rename,
  not a kept name.
- A `userEdited` row renders the label alone ("You typed this name"): its detail is empty by construction, because the
  user's name claims nothing and must never inherit the model's quote. Provenance in the operation log follows: a batch
  carrying one user-edited row is logged as `agentEdited`, not `agent`.

## The "Why this name" column

Every row carries typed evidence from the backend (`evidence: { source, detail }`, the wire mirror of Rust
`RenameEvidence`), rendered as the list's rightmost column. `evidenceSourceLabel` in `ask-cmdr-labels.ts` maps the
source to its catalog string; the raw `detail` renders below it.

Three properties this column exists for:

- **A name with no content behind it must look like one.** `imageText` / `imageTags` are the only sources the backend
  verified against delivered image content (`agent/tools/propose/DETAILS.md`); the other three say plainly that nothing
  inside the file was read ("File details, not contents", "The old name", "What you asked for"). Rewording one of those
  into something that implies content would undo the guardrail's user-facing half.
- **A thin match must LOOK thin.** A bare quote made a 14-character hit inside 3,140 characters of recognized text read
  exactly as strong as a decisive one, which is the half of the failure no backend check can close: validation proves
  the model READ something, never that the name is right. So an `imageText` row with a `coverage` renders the quote
  inside the line it came from (`…before` + `<mark>` + `after…`, cut ends marked) plus "Matched 14 of 3,140 characters"
  underneath, and REPLACES the bare detail: the delivered text with the match highlighted proves more than the model's
  own retyping of it. Rows with no coverage (the other four sources) render `detail` as before.
- **`detail` is model-authored text, and the excerpt is OCR output.** Render both as plain `{text}` only, never
  `{@html}` (same boundary as assistant prose, `CLAUDE.md` § Must-knows). Backend caps `detail` at 160 characters and
  each side of the excerpt at 60.

**The thin/solid split is a display judgment, and it lives in the frontend** (`rename-evidence-coverage.ts`, unit
tested): thin is under 2 percent of the delivered text, and only once at least 200 characters were delivered (a short
quote of a short text is normal, and the excerpt beside it already shows nearly everything). The backend supplies only
the honest counts, because a "this evidence is weak" verdict must never become a refusal: the app can't know that the
name is wrong, only that the user should look. A thin row takes the warning tone AND a `triangle-alert` marker
(`role="img"`, so its label doesn't depend on text content the way the row badges' does), so it never reads by color
alone.

The layout is the house `ColumnList` (`lib/ui/DETAILS.md` § ColumnList), so the review reads like Search's results: one
header and row grid, the same cursor tint, the same width contract. It runs in `table` semantics (rows hold a checkbox,
a button, and a text field, which a listbox option can't) with content-sized rows (`virtualized={false}`): a row is one
line or five depending on its badges and evidence, and a review stays small (about 101 rows per batch). The batches
flatten into one row list (`ListRow`: a folder heading, an expired batch's notice, or a file row), headings and notices
rendering as the list's group-heading rows. Columns: Allow `fit` (its header or the 16 px checkbox), preview `fixed` 36
px, original and new name `share` measured from the row DATA (the new name from the STORED `destinationName`, never the
draft, so typing doesn't move the columns), the arrow `fixed`, and "Why this name" a `share` with no demand, so the
spare width lands on the evidence. Cells holding controls set `clip: false`, or their focus rings would be cut. Evidence
wraps (`overflow-wrap: anywhere`) and clamps at four lines. The clamp defends a hand-shrunk dialog; it isn't a routine
truncation, because hiding evidence from the reviewer is the failure this column fixes.

## The preview column

The reviewer has to be able to see the file, because a plausible wrong name only looks wrong beside the picture. Every
row shows its own 36 px thumbnail (scanning 50 rows for the odd wrong one is the actual review task, so a detail pane
would only show the row the user already suspects), and each thumbnail is a button that opens the file in the full
viewer with Space or Enter. The house list keys walk the buttons (↑/↓, PageUp/PageDown, Home/End, over the folder
headings; `ColumnList.cursorTarget`, claimed), so the preview follows the focused row with no mouse, and the focused row
is the list's cursor row (`cursorIndex`): a key moves the cursor by focusing the landing row's button. Hover doesn't
move it: the highlight marks the row Space acts on.

- **Thumbnails reuse the viewer's `cmdr-media://` preview scheme** through `mediaIndexThumbnailToken` + `mediaUrl`, the
  same path `lib/search/ImageSearchResults.svelte` takes. They do NOT depend on media-index enrichment: the token is
  minted from a magic-byte classification of the file itself, so a never-indexed image still previews.
- **The dialog owns the token lifecycle.** The backend token map has no window-close choke point, so a missed drop leaks
  path mappings for the session. One mint pass per proposal; every token dropped when the review closes, when another
  proposal replaces it, or on unmount. A monotonic sequence number discards a late mint for a closed review (and drops
  what it minted).
- **The mint effect depends on the proposal id ALONE.** Preflight mutates rows in place on every recheck, so reading the
  rows reactively there would re-mint 50 tokens per watcher event; the row ids and paths are read through `untrack`.
- **No thumbnail degrades to a neutral glyph** (not an image, unreadable, on a drive that isn't mounted here): never a
  broken image, never an empty cell, and the row stays fully reviewable.
- `source_path` + `volume_id` ride the row snapshot for this, and they are DISPLAY data. Apply still sends opaque row
  ids only, and the backend resolves every path from its own stored proposal.

## Limit notice

A turn that reached a per-answer ceiling (rounds of tool calls, or wall time) carries `stats.limit` (`TurnLimit`: kind,
the value that applied, `answered`), and `AskCmdrMessage` renders `AskCmdrLimitNotice` from it above the stats line. It
names the limit and its value (`limitNoticeText`), says it's fixed with no setting, links Send feedback
(`openFeedbackDialog`), and on the thread's latest turn, with nothing running, offers "Keep going", which sends
`askCmdr.limit.continueMessage` as an ordinary message: the next turn starts with a fresh budget and the whole history.
Reading it from the stats is what makes live equal reload, since the `TurnFinished` row stores them. A live `failed`
with `budgetExhausted` and a limit adds no separate error line (`applyFailed`): the notice already says it, better.
Backend side: `src-tauri/src/agent/chat/DETAILS.md` § Budgets and cancellation.

## The context gauge (how full the chat is)

`AskCmdrContextGauge.svelte` sits in the footer row beside the cost line, and the two answer different questions on
purpose: **cost is what the whole thread has SPENT** (cumulative, only ever rising), the **gauge is how much of ONE
prompt's room the last call used** (it drops when history is set aside). The gauge's tooltip spells out "N of M tokens
used (estimated)" so the two numbers can't read as a contradiction.

**The footer shows whenever the chat does, and both halves move after every model call**, not only at the turn's end:
the backend emits `contextUsage` and `costMetered` after each completed call (`agent/chat/DETAILS.md` § Reporting what a
turn cost). `contextUsage` replaces the figure; `costMetered` bumps `askCmdrState.costRevision`, and the footer re-reads
`ask_cmdr_conversation_cost` on each bump (the meter stays the one source of the total, so the rail never adds figures
up itself).

**Before anything is measured, the footer reads the slot status** (`ask-cmdr-slot-status.svelte.ts`):

- The gauge shows the **baseline** (`gaugeReading`): the prefix every call carries against the next send's budget. It's
  honest, not a placeholder: no call of this chat can cost less. Its tooltip says so
  (`askCmdr.context.baselineTooltip`).
- The cost line reads "0 tokens", priced the way the first call would be (`costReading` over `nextCall`): "free,
  on-device" for local, "about $0.00" for a priced cloud model, "cost unknown" for an unpriced one, and no price at all
  while AI is off. A thread whose turns all failed has nothing metered either, so it reads the same way.

State lives in `ask-cmdr-context-usage.ts`, pure and unit-tested, as four named states:

- `unmeasured` — no reading at all: neither a measured call nor a baseline (AI is off, or the slot status hasn't been
  read yet), so the gauge renders NOTHING. Deliberately not 0%: an empty bar reads as "plenty of room".
- `calm` — under `FILLING_THRESHOLD_PERCENT` (80).
- `filling` — at or over the threshold, nothing dropped yet.
- `setAside` — history left the model's view this turn. **Going over budget lands here too**, not in a fifth state: the
  turn worked, older material made room, and "over budget" is engine vocabulary, not something the user did wrong.

Two rules the tests pin: the state follows the percentage the gauge SHOWS (79.998% displays as 80%, so it reads as
`filling`, or the bar would say 80% while behaving as calm), and a measured turn never rounds down to 0%.

The figure survives a restart: the backend stores it per conversation and `ConversationDetailView.lastContextUsage`
returns it, so reopening a thread shows its last real reading. A restored reading reports `elidedResults: 0` because
whether THAT turn set anything aside isn't persisted, and inventing a count would be a false claim. A fresh chat clears
it, so no thread inherits another's fill, and falls back to the baseline.

## The E2E fake-LLM path

The app has no real AI provider under E2E, so `commands/agent/chat.rs::resolve_agent_llm` routes the send through a
scripted `FakeAgentLlm` when `CMDR_E2E_ASK_CMDR_FAKE=1` (set for the whole E2E run by the
`desktop-svelte-e2e-playwright` check). It streams a fixed "Hi! I'm the test assistant." so `ask-cmdr.spec.ts` can
assert send-and-render deterministically, zero network. A message starting with `[tools]` plays a three-round
tool-running turn instead (`chat/session.rs`, `rail_fake_turn`: narration plus `app_state`, `list_volumes`, and a
refused `delete`; narration plus `list_volumes` twice, reusing the first round's call ids; then the answer), which the
spec reads live and again after reopening the thread, and asserts equal: the end-to-end half of live-equals-reload. The
fake picks each round from the prompt it's handed (`FakeAgentLlm::by_prompt`), so a retry or a second thread can't
desync it. `app_state` and `list_volumes` read the E2E machine's real state, so the spec asserts their shape, never
their values, and never token counts (the fake reports none). `i18n-capture-ask-cmdr.ts` photographs the same turn
(`ask-cmdr-tools`) with its groups open and one line expanded. `ask-cmdr-trigger.test.ts` covers the full event model
(tool lines, soft cap, message paging, attachments) with mocked events, `ask-cmdr-stop-and-reload.test.ts` a Stop and a
reload mid-turn, and `ask-cmdr-turn-parity.test.ts` the round reducer and live-equals-reload;
`ask-cmdr-sessions.test.ts` covers list paging/search/rename/archive. The E2E spec also drives the sessions path
end-to-end (create two threads, search finds the right one via real FTS over the persisted messages, switch works) — it
seeds a per-run nonce into the message text so search never matches a thread left by an earlier run.

**The fake gets its own prompt budget.** It answers as a LOCAL provider with no local server behind it, so the real
resolution would size its budget from `ai.localContextSize` — a setting the harness has no reason to touch, and whose
value would then decide what the gauge shows in every E2E run. `resolve_prompt_budget` returns
`DEFAULT_PROMPT_TOKEN_BUDGET` (16,000) for the fake instead, so the harness keeps mirroring a real user's settings and
an E2E run's gauge reads as a normal calm chat rather than pinned.

The composer's Send gate (`AskCmdrComposer.svelte`) disables sending while no provider is set up, and `ai.provider` is
`off` by default, so the fake path (which never sets a real provider) needs the gate to treat the fake as ready. The
backend's slot status does that (`ask_cmdr_slot_status` reads `test_mode::ask_cmdr_fake_active`, the SAME accessor
`resolve_agent_llm` gates on), so "send is allowed" and "send is answered" can't drift.

## Gates, cost, and settings

- **Two gates** (`ask-cmdr-gate.svelte.ts` + `AskCmdrGate.svelte`). `railGate({ enabled, provider, cloudConsent })`, in
  order: `askCmdr.enabled` off → `off` (a "Turn on Ask Cmdr" button that sets the setting); Cloud with consent unknown →
  `loading` (the rail renders nothing, so nothing flashes); Cloud without consent → `cloudOff` (an "Open AI settings"
  button to the Allow cloud AI switch, `openCloudConsentSettings('ask-cmdr-cloud-gate')`); otherwise `chat`. Local and
  "off" need no consent (nothing leaves the Mac; the provider notice below says when no provider is set up). The rail
  reads both settings reactively and `cloudConsentState` from `#lib/ai/cloud-consent.svelte.ts` (`lib/ai/DETAILS.md` §
  Cloud AI consent owns consent itself).
  - `openRail` awaits `refreshRailGate()` and bootstraps history only when the gate reads `chat`; when a gate opens
    later with the rail up, the rail calls `ensureThreadLoaded()` on the transition.
  - The backend enforces both gates on every send, before a thread exists (`AskCmdrOff` / `NoCloudConsent` refusals). A
    send refused as either re-runs `refreshRailGate()`, which flips the rail to the matching gate; `ask-cmdr-labels.ts`
    keeps a line for each, for the rare race.
  - **`askCmdr.enabled` saves `settings.json` at once** (the applier forceSaves, then `askCmdrEnabledChanged()`
    refreshes the wake loop's cached readiness): the send path reads the file fresh, so "turn on, then send" can't beat
    the debounced flush. MCP can't set it (`mcpSettable: false`): on Local it starts a proactive loop.
  - **Existing installs get the switch once, at startup** (`ask-cmdr-enabled-mapping.ts`, the `askCmdrEnabledMapping`
    step): a recorded legacy opt-in (any copy version, no held "no") maps to on, anything else to off, an unreadable
    store to nothing (next launch asks again). It runs only after onboarding finished and only while the switch was
    never set explicitly; fresh installs get it from the onboarding AI pick.
- **The provider notice** (`AskCmdrProviderNotice.svelte`, over `ask-cmdr-slot-status.svelte.ts`): a yellow,
  non-dismissable strip at the top of the rail body whenever the slot status reads a setup gap (`off`, `cloudNeedsKey`,
  `cloudNeedsEndpoint`, `localNotSetUp`, `localNotRunning`), one sentence per gap plus "Open AI settings" (Settings >
  AI > Provider). It sits ABOVE whichever gate or chat follows, so with Ask Cmdr off the user sees both steps. It's
  independent of the gates: readiness is setup only, consent stays `cloudOff`'s. The composer's Send is disabled on the
  same gap (`providerGap`), with the reason as a tooltip on a wrapper (a disabled button gets no hover). An unread
  status counts as no gap, so Send never flashes disabled on open; the backend refuses an unset provider on its own.
  - **When the status is re-read**, each time on an event (`watchSlotStatus`, live while the rail is mounted): on mount;
    after ANY window's `pushConfigToBackend` lands (`onAiConfigPushed`, a cross-window `ai-config:pushed` emit, so a key
    saved in the Settings window reaches the main window's rail, never before the backend has it); after the trigger's
    `noteSlotSettingChanged` debounce for `askCmdr.interactiveModel` / `askCmdr.chatMemorySize` (1 s, outlasting the
    settings file flush the backend reads); on `ai-server-ready`, `ai-install-complete`, and
    `ai-model-capabilities-changed` (Automatic sizes the budget from the catalog); and on a thread switch or a turn's
    end (memory and budget may have moved). Window focus stays only as the backstop for a local server that stops on its
    own, which announces nothing. Each read is numbered, so a slow one can't overwrite a newer answer.
- **Cost footer** (`AskCmdrCostFooter.svelte` + pure `ask-cmdr-cost.ts`): the active thread's cumulative tokens + cost,
  refetched (`ask_cmdr_conversation_cost`) when the thread changes, after each metered call (`costRevision`), and when a
  turn finishes streaming. Honest miss-path (`costReading`): a local-only thread reads "free, on-device", an unpriced
  model reads "cost unknown", a priced thread shows "about {amount}" — never a silent $0. The amount rounds to cents
  ("about $0.83", `costEstimateText`), and a real cost below half a cent reads "under $0.01" so it never looks free. The
  money goes through `formatWorded` (symbol and placement from the UI language, digits from the OS region, so an English
  app on a Swedish-formatted Mac reads "$0,83"; `#lib/intl/number-format.ts`). Its tooltip says it's USD and ignores
  provider-side prompt caching (the meter prices every prompt token at the full rate, `src-tauri/src/agent/pricing.rs`).
  Token counts (here and in the per-turn stats line) go compact from 2,000 up through `formatTokenCount` ("2.2k",
  "1.3M", one decimal at most), with the exact count in a tooltip. Always shown with the chat; a thread with nothing
  metered reads zero (§ The context gauge). A fetched total is keyed by the thread it was read for: a thread switch
  drops it at once, and a slow read for a thread the user already left is dropped.
- **Settings section** (`settings/sections/AskCmdrSection.svelte`, top-level `Ask Cmdr`): the `askCmdr.enabled` switch,
  a "cloud AI is off" hint plus an "Open AI settings" button when it's on over Cloud without consent, the provider hint
  (reads `ai.provider`) + the interactive-model row (`askCmdr.interactiveModel`), the two memory controls, and the
  per-day spend rollup (`ask_cmdr_cost_summary`). The interactive slot picks the MODEL only; provider/keys stay in
  Settings › AI. The memory pair (open the folder, forget everything) is documented where the folder is:
  `apps/desktop/src-tauri/src/agent/memory/DETAILS.md` § The two controls the user gets.

## i18n

Copy lives in `intl/messages/en/askCmdr.json` (`askCmdr.*`, including the `askCmdr.sessions.*`,
`askCmdr.composer.attach`/`dropHint`, `askCmdr.attachment.*`, `askCmdr.loadEarlier`, `askCmdr.wake.*`,
`askCmdr.wakeDigest.*`, `askCmdr.gate.*`, `askCmdr.providerNotice.*`, and `askCmdr.cost.*` keys; the "what Ask Cmdr
sends" list moved to `ai.cloudConsent.askCmdr.*` in `ai.json`), the settings copy in `settings.json`
(`settings.askCmdr.*`, `settings.section.askCmdr`), and the command label in `commands.json`
(`commands.askCmdrToggle.*`, `commands.askCmdrNewChat.*`), each with an `@key` translator description. The name, the
gate, and the provider-notice copy are the re-translation surface if David adjusts the product calls. Tool + error
labels are literal-keyed records in `ask-cmdr-labels.ts` (a computed prefix would trip the unused-key check).

## Decisions

- **Markdown-lite escaper is narrower than the error path's on purpose** (§ CLAUDE.md): the error path escapes untrusted
  _params_ inside a trusted template, but here the whole message is model-generated and we want its markdown to render —
  so we escape only HTML/link-forming chars and keep the formatting chars. Links aren't in the markdown-lite spec, so
  dropping them is safe.
- **The send command returns early and streams on a worker thread.** `run_turn` holds a non-`Send` rusqlite `Connection`
  across awaits, so its future can't live on the Tauri command future or a multi-thread tokio task; a dedicated thread
  with a current-thread runtime sidesteps that. See `commands/agent/`.
