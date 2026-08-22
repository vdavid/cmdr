# Ask Cmdr can suggest things, but it never notices anything on its own

**Problem**: the whole proactive half of the agent is built and nothing drives it. The suggestion store, the executors,
the review dialog, the approval bridge, the status-corner indicator, and all ten locales ship. So does `agent/wake/`:
coalescing, interest scoring, compaction, the inbox with its deadlines, `agent_inbox` persistence, and the readiness
gates, under 54 tests. But `run_wake` and `Inbox::admit_if_permitted` have **no production caller anywhere in the
tree**, only `wake/tests/` (verified 2026-08-20). Nothing feeds the pipeline and nothing fires it, so "AI file
organization", which ships as an alpha feature, can never volunteer anything.

**Size**: four milestones, five to seven days. M1 alone makes the agent notice things; M3 is what makes it useful rather
than a nagging renamer.

**Read first**: `apps/desktop/src-tauri/src/agent/wake/DETAILS.md` (the pipeline and the contract for both undriven
seams) and `apps/desktop/src-tauri/src/agent/suggested_ops/CLAUDE.md` for the guiding principle, which resolves most
design questions in this area.

## What the agent can already propose

`propose_suggestions` covers move, copy, trash, delete, rename, compress, and extract, up to 200 named paths per group
or thousands through a selector resolved once against the drive index. `folder_importance` and `important_folders`
answer "where does this user keep things like this?" offline. So the loop below is the only missing piece between a file
landing on disk and a reviewable proposal.

## Decisions already taken

❌ Don't reopen these while implementing. Each has a reason, recorded where it bites.

- **Cadence is a user setting**, a slider over the hot tier. Not a daily cap, not quiet hours.
- **A quiet wake leaves no thread**, via a typed `nothing_to_suggest` tool.
- **Wake threads appear in the session list** with their own icon, never filtered out.
- **Memory lives in the app data dir**, jailed behind a new `Access::Memory`, and its arrival re-prompts consent.
- **No OS notifications.** A toast and the status-corner indicator only.
- **Cold bundles ride along** and set no deadline of their own.

## Needs David's consent before M1 can land

Four ratcheted budgets move, and `.claude/rules/file-length-allowlist.md` forbids raising any of them as a side effect.
Ask once, up front, rather than discovering them at the end of the milestone.

1. **`index-crate-isolation` root promises, 51 → 52.** `scripts/check/checks/index-crate-isolation.go:152` pins the
   count, and its own comment says a new payload enum always spends one. This is an ERROR-level check; the tap's rollup
   payload cannot land without it.
2. **`invariant-density`** for `crates/cmdr-index` (372) and `apps/desktop/src-tauri` (326). M1 adds ❌ rules. Where an
   invariant can be encoded in a type instead, do that first; what remains needs the bump.
3. **`claude-md-length`.** `crates/cmdr-index/src/indexing/watch/CLAUDE.md` is at 598 words and
   `apps/desktop/src-tauri/src/agent/CLAUDE.md` at 585, against a 600 warn. Default to putting every addition in the
   `DETAILS.md` sibling; ask only if something genuinely must be a must-know.
4. **`desktop-bundle-size`** (M2/M3). New components, two icons, and roughly a hundred new catalog strings across ten
   locales will move the baseline.

## M1: the loop

**Intent**: the agent notices, decides, and either proposes or stays quiet. No new surfaces, so this milestone is
provable by tests alone and can't be blocked on copy review.

⚠️ **Order matters here and the obvious order is wrong.** The inbox's own signature changes twice (nullable deadlines,
then tunable delays), and both ripple into the tap's call site. Do the `Inbox` work first, then wire the tap once
against a settled signature.

### 1. Nullable deadlines, so cold rides along

`COLD_DELAY`'s doc comment says a cold bundle never causes a wake of its own; the code gives it `now + 1h` like any
other and `due_at` fires on it, so a trickle in a barely-scored folder spends a turn. Make the comment true.

- `deadline_for` returns `Option<u64>`; `InboxRow.deliver_by` becomes `Option<u64>`.
- ⚠️ **`Option::min` is exactly backwards here and compiles silently.** `inbox.rs:109` is
  `row.deliver_by.min(deadline_for(scored, now))`, and Rust's derived `Ord` makes `None < Some(_)`, so the naive port
  makes a cold contribution ERASE a warm row's deadline. That folder then never wakes. Write the merge as an explicit
  match: no-deadline loses to any real deadline.
- `next_deadline` (`inbox.rs:145`) needs a `filter_map`, not a bare `.min()`.
- ⚠️ `reconcile` (`inbox.rs:191`) does `if row.deliver_by < settled { row.deliver_by = settled }`. A `None` row would be
  handed a real deadline on every restart, undoing this whole item and inflating `ReconcileReport.deferred`.
- `drain` still takes everything. That IS the riding-along.
- A cold row with no other traffic ages out at `STALE_AFTER`.

**Migration v7 is a table rebuild.** `agent_inbox.deliver_by INTEGER NOT NULL` (`agent/store/migrations.rs:399`) and
SQLite cannot drop NOT NULL, so v7 is CREATE-INSERT-DROP-RENAME, recreating the `agent_inbox_deliver_by` index (`:401`).
`StoredInboxRow.deliver_by` becomes `Option<i64>`, with the four mappings in `wake/persist.rs:41-71`.

### 2. Tunable delays

`wake_delay(Interest) -> Duration` reads three consts (`interest.rs:120`). Thread the user's choice through `wake_delay`
→ `deadline_for` → `Inbox::admit` → `admit_if_permitted` as a parameter, so the pure core stays pure and the setting is
a value like every other input here.

Hot comes from the setting; warm derives as `min(hot × 60, 6h)`; cold is `None`. ⚠️ The tier ORDER is a pinned contract:
test it at every slider stop, not just the default.

### 3. Inbox ownership and persistence

⚠️ **Not managed Tauri state.** The indexer starts before the agent does (`lib.rs:317` and `:709` versus `agent::start`
at `:766`), so anything registered in `agent::start` misses launch replay, which is the busiest window the tap will ever
see. Use a process-global with lazy init, the way `restricted_paths` does for `PathAccessDenied`.

- Launch: `persist::load` then `Inbox::reconcile(launched_at)`, logging the `ReconcileReport` (it counts what the user
  was not told and why).
- Admit: `persist::save_row` for the touched row.
- Drain: `persist::clear`.

⚠️ **The lock must never be held across a model turn.** `TauriEventSink::emit` calls `route` synchronously on the
caller's thread (`index_mapping.rs:660`), and that caller is the indexer's live-loop thread. `run_wake` currently takes
`inbox: &mut Inbox` (`job.rs:59`) and awaits `run_turn` at `:107`, so a naive mutex would block every live batch for the
length of an LLM call, stalling the event loop and journal replay. **Change `run_wake` to take the already-drained rows
plus a way to report back**, so the guard is dropped before the turn starts. See item 5.

**Crash semantics, stated on purpose**: `persist::clear` runs with the drain, before the turn. A process that dies
mid-turn loses that digest rather than re-delivering it on restart. Re-delivery would mean the user hears about the same
activity twice, and the folder is still there to be looked at again.

### 4. The tap adapter

Map the crate-side per-batch rollup into an `EventBundle` and call `Inbox::admit_if_permitted`, as a second observer
inside `process_live_batch`, after `detect_renames_by_inode` and the storm coalescing. Per-folder rollups, ❌ never
per-file: `INGESTION_HARD_CAP` is 5,000,000 and a per-file message would put five million of them across the boundary on
exactly the path the counters exist to survive.

⚠️ **The corrected stream is not sitting there waiting.** At the placement point, three of the four counters are
unreachable, and taking the placement literally ships a tap that counts almost nothing:

- **Renames are gone.** `detect_renames_by_inode` (`live.rs:636`) returns a bare `usize` and `retain`s matched events
  out of `other_events`. `ChangeCounters.renamed` would be permanently zero, and `intent_share()` loses half its intent
  signal. A user who only renames produces a bundle that never wakes. **Change it to return the matched paths.**
- **Storm-coalesced removals are gone.** The storm path (`live.rs:561`) queues anchors as rescans and `continue`s the
  strict-descendant removals out of `kept`, so a 60,000-file delete contributes neither the removals nor the anchor.
  **Surface the anchors to the tap** and count one removal event at the anchor folder.
- **Directory creations are in a separate Vec.** `pending_events.drain()` at `live.rs:496` empties the input map and
  Pass 1 consumes `dir_creations` (`:497`). **Fold that Vec in explicitly.**

⚠️ **The flags are not one-hot.** One coalesced `FsChangeEvent` can carry `item_created`, `item_removed`, and
`item_renamed` at once. Specify the `flags` → `ChangeKind` priority in one documented function: renamed, then created,
then removed, then modified. Different orders move `intent_share()` materially, so this is a decision, not a detail. For
a directory's own event, count it in its PARENT folder: `wake/mod.rs:78` says a bundle describes the folder a change
happened IN.

**The crate boundary shapes the rest.** `cmdr-index` may never name the agent (`index-crate-isolation`), so:

- A new observer type in `crates/cmdr-index/src/indexing/watch/`, shaped like `ChurnObserver`, folding a batch into
  per-folder counters and emitting one `IndexEvent` per batch through the sink it holds.
- ⚠️ **Bundle it with `ChurnObserver` into one struct rather than adding a parameter.** `process_live_batch` is at
  exactly seven arguments (`live.rs:467`) and `clippy::too_many_arguments` defaults to seven, which `clippy.toml`
  doesn't raise.
- A new `IndexEvent::FolderActivity { volume_id, window_start, folders: Vec<FolderChangeRollup> }`, where the rollup
  carries the folder path, the four counts, and **`last_event_at`** (`EventBundle` needs it, and it's what `reconcile`'s
  staleness horizon reads).
- ⚠️ **Adding an `IndexEvent` variant is six more compiler- or test-enforced edits**: the `IndexEventKind` variant,
  `ALL: [Self; 21]` → 22, a `slot_of` arm (`sink.rs:355-471`), `IndexEvent::kind()` (`:493`), `volume_id()` (`:520`),
  `testing::events::one_of_every_kind()` (`:591`), and the non-frontend-destination list in
  `events/index_mapping/tests.rs:44`.
- ⚠️ **The app side must NOT route through managed state.** `route()` takes `app: Option<&AppHandle>`
  (`index_mapping.rs:406`) and the completeness test calls `route(event, None)`, so a handler reaching for `app.state()`
  silently drops every bundle in that test and whenever `app` is `None`. Use the process-global from item 3, exactly as
  `PathAccessDenied` → `restricted_paths::record_denial` does.

⚠️ **Inherit both of `ChurnObserver`'s guarantees.** It is passed `&mut` so a live batch cannot be processed without
one, and `every_live_loop_owns_a_real_churn_observer` (`churn_monitor/tests.rs:216`, the `process_live_batch(` scan at
`:249`) fails when a driver doesn't build a real one. Note it asserts an EXACT driver list, not a subset. Write the
sibling scanner for the tap FIRST, red, before the observer exists. Skip this and the cold-start replay path silently
taps nothing, a failure `live.rs`'s own comment records having happened once already. `process_live_batch` has eight
call sites: production at `live.rs:285`, `live.rs:368`, `replay.rs:499`, `replay.rs:563`; tests at
`indexing/tests/stress_tests_concurrency.rs:864`, `event_loop/tests/rename.rs:112` and `:708`,
`watch/branches/tests.rs:122`.

**Importance lookup.** `admit_if_permitted` needs a `FolderImportance` per bundle. `ImportanceIndex::open` is already
cheap (`importance/read/mod.rs:167`: the connection is lazy and thread-local), so the cost to avoid is the per-folder
`lookup`, not the open. Cache lookups behind a small bounded map with a short TTL (60 s; folders repeat heavily across
batches and a stale weight only misprices one wake). Open per volume with `available_for(&volume_id)`, ❌ not
`SignalSet::all()` — a network volume degrades its signal set, and `mcp/resources/importance.rs:120` is the precedent.
Map `WeightLookup` to `FolderImportance` variant for variant, ❌ never through `score()`, which collapses `Floored` and
`Unscored` into the same `0.0`.

**Windowing.** The app side quantizes, ❌ not the crate: a 60 s agent policy must not leak into `cmdr-index`, and
`Inbox::admit` merges on exact `(folder, window_start)` equality (`inbox.rs:101`). So the event's field is the batch's
own instant, and the app floors it to the 60 s window before admitting. Left unresolved, every ~1 s batch becomes its
own inbox row.

**One contract note to amend.** `EventSink::emit` is fire-and-forget by design (`sink.rs:557`: "a dropped event costs a
UI update, never correctness"). For this variant a drop costs signal. That's acceptable (the folder will change again),
but say so at the variant rather than leaving the contract silently violated.

### 5. The wake runner

⚠️ **A wake must not bypass `ChatRuntime`.** The rail never calls `run_turn` directly: `ChatRuntime::send_message`
(`chat/runtime/mod.rs:147`) opens its own write connection and takes the per-thread single-flight lock. A wake thread is
a real conversation the user can reply to, so skipping `ConversationLocks` lets a user reply and the wake's own turn run
concurrently on one thread. Give `ChatRuntime` a `wake()` method that owns the connection and the lock and calls
`run_wake` inside them. That also answers where the wake's `Connection` comes from, and keeps `main.db` to one
long-lived writer discipline against its 5 s busy timeout.

**Reshape `run_wake`** while its only callers are tests:

- Take the drained rows rather than `&mut Inbox`, so the inbox lock is released before the turn (item 3).
- Take a dispatcher FACTORY (`&dyn Fn(i64) -> Box<dyn ToolDispatcher>`) rather than a built dispatcher.
  `AppHandleDispatcher::new(app, conversation_id)` scopes evidence to a thread and `LlmLogContext::agent_chat(id)` keys
  the LLM log the same way, but `run_wake` creates the conversation itself. Same for the LLM, built once the id is
  known, as `ResolvedAgentLlm::into_llm` already does. **Why it matters**: evidence scope is what stops a claim in one
  thread being backed by facts delivered to another; the wrong scope makes `ImageFactsLedger` refuse every
  content-citing proposal.

**The scheduler** is a timer that fires at `Inbox::next_deadline` and calls `ChatRuntime::wake`, re-arming when an admit
pulls a deadline earlier or a setting changes. ⚠️ **Not a plain tokio task**: `run_turn` holds a rusqlite `Connection`
across awaits, which is why `ask_cmdr_send_message` spawns a dedicated `std::thread` with a current-thread runtime. Copy
that shape.

**Share the resolution with the rail.** `resolve_agent_llm` (`chat.rs:111`), `resolve_prompt_budget` (`:253`), and
`capture_envelope` (`:289`) are private in `commands/agent/chat.rs`, which sits ABOVE `agent/`. ❌ Don't import upward:
move them into `agent/chat/session.rs` and have the command call down. Note `capture_envelope` is `async` and generic
over `R: Runtime`, and depends on `PaneStateStore`. The budget is read fresh per send; a wake reading a stale one would
think with a different window than the rail, silently.

### 6. `nothing_to_suggest`

A wake that finds nothing must be able to say so. A typed tool call, ❌ never inferred from the model's wording
(`error-string-match` forbids classifying control flow by text, and this is exactly that).

- ⚠️ **The tool itself must be a pure signal**, `Access::Read`, mutating nothing. A handler that deleted the
  conversation would be `Write` under the registry's tiebreaker and would fail `test_agent_tool_view_never_writes`
  (`mcp/tests/tool_registry_tests.rs:772`). **`run_wake` does the delete, after the turn.**
- ⚠️ **`TurnResult` carries no tool-call information** (`chat/runtime/turn.rs:49`: `Answered | Failed | Cancelled`), and
  `ToolDispatchOutcome.proposal` is rename-specific. Add a `ToolId::NothingToSuggest` variant (`agent/llm/types.rs:145`)
  and have the dispatcher record the call typed, so the outcome is observable without reading message text.
- One argument: a short reason, for the log and for memory (M3).
- Needs a store-level `delete_conversation`; only `ask_cmdr_archive_conversation` exists today. Considered and rejected:
  archiving. Archived threads still accumulate, and "we looked and found nothing" fifty times is not a record worth
  keeping.
- ⚠️ **Preserve the cost record.** `cost_meter.conversation_id` is `ON DELETE CASCADE` (`migrations.rs:210`), so
  deleting the thread erases what that wake spent from the one place the user can see what the proactive agent costs. ❌
  **`ON DELETE SET NULL` is not available here**: the column is `NOT NULL` on purpose, and `migrations.rs:203` spells
  out why (SQLite treats NULLs as distinct in a PK, so a nullable column inside it breaks `ON CONFLICT DO UPDATE` and
  every write inserts a duplicate instead of upserting).

  **Do this instead**: create one reserved "quiet wakes" conversation row at migration time, hidden from the session
  list by its origin. Before deleting a noop wake's thread, fold its `cost_meter` rows into the reserved id with an
  upsert that SUMS tokens and micros, since `(day, conversation_id, provider, model)` may already exist there. Then
  delete the thread. A quiet wake still cost money and the daily total must say so.

### 7. Three settings

⚠️ **A registry entry renders nothing on its own.** Only `AdvancedSection` auto-renders. Each setting needs a key in
`SettingsValues` (`settings/types.ts`, a compile error until added), the `definitions/ai.ts` entry, AND a
`{#if shouldShow(id)}<SettingRow …>` block in `AskCmdrSection.svelte` (`:173` is the pattern). Follow
`docs/guides/adding-a-new-setting.md`.

⚠️ **All three need `settings-applier` cases**, contrary to the two existing `askCmdr.*` rows. Those are read fresh at
send time; these drive a sleeping timer. `settings/CLAUDE.md` is explicit: "Every setting MUST apply immediately without
restart. Restart-required is a bug." Concretely: flipping `proactive` on must wake a parked scheduler, and changing the
delay must re-arm the timer AND re-price the rows already waiting. ⚠️ Re-pricing is not automatic: the merge is
min-only, so a LENGTHENED delay would otherwise never apply to anything queued. Recompute `deliver_by` across the inbox
on change.

- **`askCmdr.proactive`** (boolean, default true). The middle tier between "no AI" and "AI that starts conversations",
  and the fourth gate the scheduler checks. ⚠️ `settings.json` is sparse, so the Rust loader needs an explicit
  `.unwrap_or(true)`; `unwrap_or_default()` silently ships default-off and nothing compares the Rust fallback to the TS
  registry default. Pin it with a Rust unit test.
- **`askCmdr.wakeDelay`** (number, seconds, default 5). A slider over the HOT tier with stops at 5 s, 15 s, 30 s, 1 min,
  2 min, 5 min, 15 min, 30 min, 1 h, 2 h.

  **Use the index-mapped slider.** A linear 5-to-7200 track puts the first three stops inside one pixel. Add a
  `stopsAreDiscrete` flag to `SettingConstraints` (`settings/types.ts:71`; it flows into `SettingConstraintsSource` for
  free) and map index↔value in `SettingSlider.svelte` at four points: the `$state` seed (`:49`), the
  `onSpecificSettingChange` arm (`:52`), `commit` (`:62`), and `onThumbDoubleClick` (`:81`). `ui/Slider.svelte` needs no
  change: `positionOf` is linear over min/max, which is correct in index space. Two traps: `ariaValueText` is handed the
  raw Ark value (`ui/Slider.svelte:94`), so map back before formatting or screen readers announce "3"; and a stored
  value not in the table must resolve to the nearest stop, ❌ never `indexOf → -1`.

  Costs about 40 lines plus a `SettingSlider.svelte.test.ts` (none exists today) and two doc edits in
  `settings/components/DETAILS.md:44`. **The `select` fallback is NOT cheaper**: `type` flips to `'enum'` so the value
  becomes a string, and ten options mean ten new keys, which is a hundred translated strings. The slider needs zero new
  keys if the readout uses `formatDuration` from `$lib/units`.

  ❌ Don't persist the stop INDEX: reordering the table would silently change every user's setting.

- **`askCmdr.wakeToast`** (boolean, default true). Whether a staged proposal raises a toast.

**The two-value description** ("reacts within 30 seconds, quieter folders within 30 minutes") can't come from
`descriptionKey`, which resolves to a static string. `SettingRow` takes `description` as a prop, so the section passes a
computed `tString(...)`; the search index keeps the static registry text. Write it as an ICU message taking two
**preformatted string** params, ❌ not `{n, number}` (`messages/CLAUDE.md`). ❌ Don't add a `formatWakeDelay` helper:
`cmdr/no-private-unit-format` rejects new formatter-shaped names doing unit work.

### 8. A dev-only force-wake command

So verification doesn't mean waiting out a deadline. Gate it the way `test_mode` gates the scripted fake.

### M1 tests

TDD, red first (`tdd-red-green.md`):

- **The `Option` merge** (`wake/tests/inbox.rs`): a cold contribution must not erase a warm row's deadline. This is the
  one that compiles wrong, so it earns a test before the port.
- **`reconcile` leaves a `None` row alone**, and `next_deadline` skips it.
- **Tier order at every slider stop** (`wake/tests/interest.rs`), including the 6 h warm cap.
- **The tap scanner** (beside `churn_monitor/tests.rs:216`): every live-batch driver builds a real tap observer.
- **The flags → `ChangeKind` priority**, with a multi-flag event asserting the documented winner.

Written after:

- **Rollup → bundle mapping**: counters, `last_event_at`, and window survive the crossing; `Unscored` does not become
  zero.
- **A rename-only batch produces a non-empty bundle** (the regression anchor for the retained-paths change).
- **Live batch to `WakeOutcome::Ran`** (integration): a synthetic batch through `process_live_batch`, the tap, the
  inbox, and a wake against the fake LLM.
- **A noop wake leaves no thread but keeps its cost row** (integration): the fake calls `nothing_to_suggest`.
- **Restart reconciliation**: rows persisted, reloaded, settled, stale ones counted.

⚠️ The fake LLM can call tools (`ScriptedTurn::CallTools`, `agent/llm/fake.rs:24`), but `scripted_fake_llm()`
(`chat.rs:139`) — the one the E2E harness gets — is a fixed `Say` script that never proposes. It needs a wake-aware
variant before any test can drive a proposal or a noop through it.

**Docs**: `agent/wake/DETAILS.md` (the seams are driven; the tap's window policy, the flags mapping, the importance
cache, the crash semantics), `crates/cmdr-index/src/indexing/watch/DETAILS.md` (the second observer), `agent/DETAILS.md`
(the scheduler), `docs/architecture.md` if the subsystem map gains a line. ⚠️ Prefer the `DETAILS.md` sibling in every
case: both relevant `CLAUDE.md`s are within 15 words of the 600 warn.

**Checks**: `pnpm check rust` while iterating, then `pnpm check` at the milestone. Named: `index-isolation`,
`error-string-match`, `clippy`, `bindings-fresh`, `invariant-density`, `claude-md-length`.

## M2: the surfaces

**Intent**: make the agent's noticing visible and interruptible. Everything here meets human eyes.

⚠️ **`i18n-coverage` is an ERROR, so "English first, translate later" is not a landable sequence.** A key missing from
any of the nine non-en locales, or byte-identical to English without a justification, fails the build. Every commit
adding a key carries: the nine translations, an `@key.description`, an `@key.sourceHash` (`i18n-stale`), a
`pnpm intl:keys` regen (`message-keys-fresh`), and a real call site (`message-keys-unused` is an ERROR). David's review
is asynchronous and is not the gate; the translations are.

1. **A wake indicator in the status corner.** ⚠️ Read `status-corner/DETAILS.md` first. It is a **named import in
   `StatusCorner.svelte`**, not `children`: the corner owns ordering. Say where it sits relative to
   `SuggestedOpsIndicator` (two AI glyphs adjacent). Follow the badge pattern: subscription in a `*.svelte.ts` started
   from `routes/(main)/+page.svelte`, component reads `$state`. ⚠️ `StatusCorner.svelte.test.ts:118` pins the ordering
   contract and `StatusCorner.a11y.test.ts` mounts the real corner, and neither mocks suggested-ops today, so a member
   that opens a subscription at mount breaks both.

   ⚠️ **Not a `Channel`.** `Channel<AskCmdrStreamEvent>` is a per-invoke reply channel the frontend hands in
   (`chat.rs:372`); a wake has no invoke. Use a `tauri_specta::Event`, the way `SuggestionsChanged` does
   (`suggested_ops/changed.rs:45`, registered at `ipc.rs:842`, consumed via `onSuggestionsChanged`). That means: event
   struct, `collect_events!` line, a `$lib/tauri-commands` wrapper, bindings regen.

   Clicking opens the rail at that conversation. `switchToThread(id)` already exists (`ask-cmdr-trigger.svelte.ts:141`);
   call it before `openRail()`, which otherwise bootstraps the most recent thread on a closed→open transition and wastes
   a fetch.

2. **The digest.** ⚠️ **The rendered digest is untranslated English generated in Rust** (`compact.rs:196`: literal
   "new", "changed", "removed", "renamed", "+ N more folders under X") and `job.rs:98` persists it as the thread's
   user-role message. Rendering that bubble ships English UI copy into ten locales and freezes it in `main.db`, where no
   later locale pass can reach it. `job.rs:114`'s comment on `thread_title` refuses to author English for exactly this
   reason.

   **Resolution**: the rendered digest stays prompt-only, and the wake persists a STRUCTURED first block (folders plus
   counts) that the rail localizes and renders collapsed. Needs a `MessageBlock` variant and its reducer arm.

3. **A distinct icon for wake-created threads.** Mostly done already: `ConversationRow.origin` exists and ships in
   `bindings.ts:4354`. The work is one icon in `AskCmdrSessions.svelte:161` plus an `icon-map.ts` entry. ⚠️ The only
   real Rust change: `ConversationSearchHit` (`bindings.ts:4372`) has no `origin`, so search results can't show the
   icon.

4. **A toast when a wake stages a proposal**, gated by `askCmdr.wakeToast`. ⚠️ A string toast has no action button, so
   "Review" needs component content with props, shaped like `OperationFailedToastContent`. ⚠️ A `tauri_specta::Event`
   reaches EVERY window, so the listener lives in the main window's `$effect.root`, or the settings window raises its
   own copy.

5. **Readiness states in the indicator.** ⚠️ No IPC exists: `WakeReadiness` (`readiness.rs:31`) has no `Serialize` and
   no `specta::Type`, and there is no `commands/agent/wake.rs`. Needs a typed enum, a command, and a re-evaluation
   trigger, since the API key is set in another window and FDA is granted outside the app entirely.

   ⚠️ **Two docs currently contradict each other**: `readiness.rs:10` says "❌ none of them is silence";
   `SuggestedOpsIndicator.svelte:5` says the corner hides at zero because "an always-present control for a feature that
   has nothing to say is noise". Read literally, `NeedsConsent` puts a permanent AI nag in front of every user who never
   wanted AI. **Resolution**: silent when consent was never given or `askCmdr.proactive` is off; render the gap only for
   users who opted in. Fix both doc comments so they agree.

6. **Icons**: `bot` and `brain-circuit` are not in `icon-map.ts`, which imports each glyph explicitly. Both need an
   import and a map entry (`docs/guides/icons.md`).

**Open question this milestone must answer**: what the rail shows while a wake streams into a thread the user happens to
have open. The turn's events don't reach it (no channel), so today it would only update on reload.

**Tests**: component tests for the indicator's states and the thread icon; a colocated `*.a11y.test.ts` per new
component (`a11y-coverage`); an E2E driving the wake-aware fake through a forced wake to a visible toast and badge.

**Checks**: `pnpm check svelte`, plus `i18n-parity`, `i18n-coverage`, `i18n-icu`, `i18n-plural`,
`i18n-tag-param-collision`, `i18n-stale`, `message-keys-fresh`, `message-key-naming`, `message-keys-unused`,
`message-screenshots-fresh`, `a11y-coverage`, `a11y-contrast`, `knip`, `desktop-bundle-size`.

## M3: memory

**Intent**: without this the agent relearns nothing and re-proposes what was already rejected. It is the difference
between a colleague and a nag.

1. **Fix `read_cmdr_md()` first** (`chat/runtime/mod.rs:231`). It calls `dirs::home_dir()` directly, so it ignores
   `CMDR_DATA_DIR`, and the real `~/.cmdr/CMDR.md` bleeds into every E2E run and every worktree today. TDD, and it makes
   every later memory test deterministic. **This item is independent of everything else and can land before M1.**

2. **Location**: `<data-dir>/ai/memory/`, with `AGENTS.md` as the hub. The app data dir, ❌ not `~/.cmdr/`: it is
   app-managed state rather than user config, `app_data_dir()` is already the canonical per-OS path on all three
   platforms, and it inherits `CMDR_DATA_DIR` isolation for free, so dev, E2E, and every worktree get their own memory.
   Shared memory would mean an E2E run writing personal facts into David's real agent memory.

   `~/.cmdr/CMDR.md` stays put: it is user-authored, and a dotfile in home is where a hand-edited, dotfiles-repo-able
   config belongs. When a second platform arrives, check the OS config dir first and fall back to `~/.cmdr/`.

3. **Feeding both files.** ⚠️ `TurnParams.cmdr_md` is a single `Option<&str>` and `run_wake` passes `None`
   (`job.rs:99`). Add a second field rather than concatenating, so the prompt can label them distinctly: **what the user
   tells the agent**, and **what the agent learned**. `run_wake` must pass both.

4. **`Access::Memory`**, a fourth variant beside `Read`, `Propose`, and `Write`, with its own hand-authored allowlist
   mirroring `EXPECTED_PROPOSE_TOOL_NAMES`. `test_agent_tool_view_never_writes` widens to admit exactly this and nothing
   else, so the guarantee becomes "the agent writes only into its memory folder", structural rather than a rule in a
   doc. ⚠️ This is a deliberate widening of the app's central agent-safety invariant; the allowlist is what stops it
   being acquired as a side effect of editing a registry line.

5. **Two tools**, path-aware from day one so the second file costs nothing:
   - `memory_write(path, content)`: create or fully replace.
   - `memory_edit(path, old_string, new_string)`: exact match, refuses a non-unique match.

   ❌ No read or list tool yet: `AGENTS.md` is auto-fed and it is the only file. Add both the moment there is a second
   one. Every schema rides in the cached prefix of every turn, including the rail's, so two tools cost less than four on
   calls that never touch memory.

6. **The jail**, one function both tools call, unit-tested: reject absolute paths, reject any `..`, resolve symlinks and
   re-check containment, allow `.md` only, cap a file at 8 KB and the directory at 64 KB. The cap is not housekeeping:
   memory rides in every turn's prefix, so an unbounded file quietly eats the context budget of every conversation.

7. **The system prompt** encourages capturing what matters, on request or on meeting something worth keeping, and
   pruning what has gone stale.

8. **Consent copy changes, and everyone re-accepts.** ⚠️ This costs more than a re-prompt. Bumping
   `CONSENT_COPY_VERSION` (`agent/consent.rs:16`) revokes every beta user, and:
   - The rail gates on `consentState.accepted`, so a user's whole thread history sits behind the consent screen until
     they re-accept.
   - `AskCmdrSection.svelte:133` then renders a plain "Off", indistinguishable from never having opted in. Needs "here's
     what changed" copy that doesn't exist.
   - The disclosure list is duplicated in `AskCmdrConsent.svelte:42` AND `AskCmdrSection.svelte:151`. Edit both.
   - ⚠️ **`askCmdr.consent.noContents` currently ends "Ask Cmdr only looks and speaks; it never changes anything."**
     `Access::Memory` makes that false. That sentence is what the whole read-only promise rests on, so it needs a
     rewrite, not a sixth bullet.
   - These keys carry `@key.screenshot: ask-cmdr-consent.png`, so `pnpm i18n:shots` needs a re-run.

9. **Two controls** in the Ask Cmdr section: "Open memory folder" and "Forget everything".
   - ⚠️ **"Open memory folder" has no mechanism today.** The settings window must learn the resolved path (Rust-only,
     `CMDR_DATA_DIR`-dependent) and tell the main window to navigate. `ExecuteCommand` (`window_events.rs:30`) carries a
     bare `command_id` and nothing else, and it's the only settings→main dispatch. Needs a command returning the path
     plus a payload-carrying event.
   - ⚠️ **"Forget everything" is a soft dialog**, so it needs an id in `lib/ui/dialog-registry.ts` AND a row in
     `lib/dialog-gallery/gallery-registry.ts`, or `dialog-gallery-coverage` fails. `DeleteAiModelDialog.svelte` is the
     precedent. Plus its colocated `*.a11y.test.ts`.

10. **Verify** crash and error report bundles don't sweep up the data dir. Memory must never ride out in a report.

**Tests**: TDD the jail (every escape attempt) and the `CMDR_DATA_DIR` fix. After: tool round-trips, the size caps,
prompt assembly carrying both files labelled, and a consent test proving the new copy re-prompts.

## M4: the feedback loop

**Intent**: an approval or a rejection the agent never hears about is a lesson it can't learn.

`NewSweep.conversation_id` already exists (its doc comment claims a background wake has none, which M1 makes obsolete),
so an outcome knows which thread to report to.

- **Always**: append a typed outcome event to the originating thread. No model call, no cost.
- **On rejection only**: one follow-up turn, so the agent can record why in memory and, if it wants, ask. Gated by
  `askCmdr.proactive`, at most once per group.

⚠️ **The surface is wider than one enum.** `ModelChanged` is the shape to copy, and it lives in three Rust places:
`ConversationEvent` (`store/query.rs:47`), `MessageBlock` (`views.rs:279`), and `AskCmdrStreamEvent` — plus
`to_message_view`, the reducer arm (`ask-cmdr-stream.svelte.ts:114`), `ask-cmdr-messages.ts`, `AskCmdrMessage.svelte`,
and a message key.

⚠️ **Nothing mechanically guards the hand-mirrored enum.** `AskCmdrStreamEvent` derives only `Clone, Serialize`
(`views.rs:22`), so it is absent from `bindings.ts` and out of `ipc-enum-camelcase`'s scope, and `check-type-drift.ts`
covers six unrelated files. The mirror between `views.rs:24` and `ask-cmdr.ts:95` is maintained by hand and by tests
only.

**Tests**: TDD the once-per-group guard (it's the cost-control invariant). After: the outcome event round-trips, and a
rejection with `askCmdr.proactive` off runs no turn.

## Execution order

Sequential: M3.1 (the `CMDR_DATA_DIR` bug, independent) → M1 → M2 → M3 → M4. Inside M1 the order is the numbered one,
and it is deliberate: the inbox's signature settles before the tap is wired against it.

❌ Don't parallelize M1's steps across agents. The tap, the inbox owner, and the runner meet at one lock and one event
variant, and three agents converging on `process_live_batch`'s signature is how you get a merge that compiles and taps
nothing.

M2's copy can be DRAFTED during M1, but it lands with its nine translations or not at all.

## Open for David

1. **The four budget bumps** listed under "Needs David's consent" above, `index-crate-isolation` most urgently, since M1
   cannot land without it.
2. **The follow-up turn on rejection** costs a model call per rejected group. Confirm before M4.
3. **M2's open question**: what the rail shows while a wake streams into a thread the user has open.

## Deliberately deferred

- **Reading file contents** (PDFs and friends). The agent proposes from names, sizes, dates, and importance today.
- **The two tuning knobs** (the unknown-importance weight at 0.35, and the hot/warm thresholds at 0.7 and 0.3) stay
  guesses. M1's slider makes the DELAYS a user choice; the thresholds want real use before anyone moves them.
- **Per-rule approval for a long job's tail** is a policy question, not a task. See `open-decisions.md`.

## One unrelated gap in the same area

Changing the chat memory size records no thread-timeline event. The thread logs `ModelChanged` honestly through two
cooperating paths, but there is no equivalent for a budget change, so a user who shrinks their window mid-thread sees no
note explaining why the replies changed. Needs its own event plumbing on both sides, and the channel enums are
hand-mirrored in TypeScript. About half a day, unblocked, and independent of the wake loop.
