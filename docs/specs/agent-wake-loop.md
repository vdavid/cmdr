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

**Intent**: the agent notices, decides, and either proposes or stays quiet.

⚠️ **This milestone is NOT copy-free.** `nothing_to_suggest` is an agent tool, and
`every_known_tool_has_an_ask_cmdr_rail_label` (`agent/tools/mod.rs:67`) is a Rust test that reads `ask-cmdr-labels.ts`
as text and fails on a missing entry. So M1 carries two message keys, their nine translations, an `@key.description`, an
`@key.sourceHash`, and an `intl:keys` regen, under the same error-level `i18n-coverage` gate M2 describes.

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
- ⚠️ `reconcile` (`inbox.rs:193`) does `if row.deliver_by < settled { row.deliver_by = settled }`. A `None` row would be
  handed a real deadline on every restart, undoing this whole item and inflating `ReconcileReport.deferred`.
- `drain` still takes everything. That IS the riding-along.
- A cold row with no other traffic ages out at `STALE_AFTER`.

**Migration v7 is a table rebuild.** `agent_inbox.deliver_by INTEGER NOT NULL` (`agent/store/migrations.rs:398`) and
SQLite cannot drop NOT NULL, so v7 is CREATE-INSERT-DROP-RENAME, recreating the `agent_inbox_deliver_by` index (`:401`).
`StoredInboxRow.deliver_by` becomes `Option<i64>`, with the four mappings in `wake/persist.rs:41-71`.

### 2. Tunable delays

`wake_delay(Interest) -> Duration` reads three consts (`interest.rs:120`). Thread the user's choice through `wake_delay`
→ `deadline_for` → `Inbox::admit` → `admit_if_permitted` as a parameter, so the pure core stays pure and the setting is
a value like every other input here.

Hot comes from the setting; warm derives as `min(hot × 60, 6h)`; cold is `None`. ⚠️ The tier ORDER is a pinned contract:
test it at every slider stop, not just the default.

### 3. Inbox ownership: a bounded channel and a writer thread

⚠️ **Nothing on the live-loop thread may take a lock or touch SQLite.** `TauriEventSink::emit` (the one in
`events/index_mapping.rs`, ❌ not the unrelated type of the same name in `file_system/write_operations/event_sinks.rs`)
calls `route` synchronously at `index_mapping.rs:689`, on the caller's thread, and that caller is the live loop, itself
a tokio TASK (`live.rs:163`). Two things would stall it:

- **A mutex around the inbox.** `run_wake` takes `inbox: &mut Inbox` (`job.rs:59`) and awaits `run_turn` at `:107`, so a
  guard held across the turn blocks every live batch for an LLM call, and a runtime worker with it.
- **A write connection per admit.** `open_write_connection` (`agent/store/connection.rs:43`) applies WAL pragmas and
  runs the FULL MIGRATION LADDER on every open, against `busy_timeout = 5000` (`:32`). With item 5 holding a long-lived
  writer across the turn, one admit could block the indexer for five seconds. That is worse than the mutex, and it
  violates principle 2 outright.

**So the tap never owns the inbox.** It hands the rollup to a bounded channel and returns. A dedicated writer thread
owns the `Inbox`, one long-lived write connection, and the timer:

- Launch: `persist::load` then `Inbox::reconcile(launched_at)`, logging the `ReconcileReport` (it counts what the user
  was not told and why).
- Admit: fold the rollup in, `persist::save_row` for the touched row.
- Drain: `persist::clear`.
- ⚠️ Bounded, so a pathological burst drops rather than growing without limit, and it logs what it dropped. The tap's
  payload is signal, not correctness (the folder will change again).

This also gives item 5 somewhere to live: the timer fires on this thread, which already holds the inbox and a writer.

⚠️ **Not managed Tauri state.** The indexer starts before the agent does (`lib.rs:317`, plus `:709` inside a `spawn`,
versus `agent::start` at `:766`, so it's a race, not a reliable ordering), and anything registered in `agent::start`
misses launch replay, the busiest window the tap will ever see. Use a process-global with lazy init, the way
`restricted_paths` does for `PathAccessDenied`.

⚠️ **Readiness is a cached atomic, not a per-batch query.** `admit_if_permitted` takes a `WakeReadiness`
(`inbox.rs:130`) whose consent bit lives in `main.db` (`consent::has_current_consent`). Reading it per batch is a second
SQLite round trip on the same hot path. Keep a process-global snapshot refreshed on consent change, settings change, and
FDA change. M2 item 5 needs exactly the same snapshot for its IPC, so build it once, here.

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

- **Successful renames are gone.** `detect_renames_by_inode` (`live.rs:636`) returns a bare `usize` and `retain`s
  matched events out of `other_events`. Only the FAILED matches survive (a failed stat, an inode with no moved row), so
  `renamed` counts the noise and drops the signal. `intent_share()` is `(created + renamed) / total`, and a rename-only
  batch yields `total == 0` → `Interest(0.0)` → never wakes. **Change it to return the matched paths.**
- **Storm-coalesced removals are gone.** The storm path (`live.rs:562`) queues anchors as rescans and `continue`s the
  strict-descendant removals out of `kept`, so a 60,000-file delete inside a surviving folder contributes nothing.
  (`storm::scope_to_requeue` does keep the anchor's OWN removal, so a deleted anchor still shows up as one event.)
  **Surface the anchors to the tap** and count one removal at the anchor folder.
- **Directory creations are in a separate Vec.** `pending_events.drain()` at `live.rs:496` empties the input map and
  Pass 1 consumes `dir_creations` (`:497`). **Fold that Vec in explicitly.**

⚠️ **The flags are not one-hot.** One coalesced `FsChangeEvent` can carry `item_created`, `item_removed`, and
`item_renamed` at once. Specify the `flags` → `ChangeKind` priority in one documented function: renamed, then created,
then removed, then modified. Different orders move `intent_share()` materially, so this is a decision, not a detail. For
a directory's own event, count it in its PARENT folder: `wake/mod.rs:78` says a bundle describes the folder a change
happened IN.

**The crate boundary shapes the rest.** `cmdr-index` may never name the agent (`index-crate-isolation`), so:

- A new observer type in `crates/cmdr-index/src/indexing/watch/`, following `ChurnObserver`'s LIFECYCLE (per volume,
  `&mut`, one fold per batch) but ❌ not its output: `ChurnObserver` writes to `log::info!` and holds no sink. This one
  needs the `Arc<dyn EventSink>` threaded in from the loop level (`live.rs:173`, `replay.rs:76`).
- ⚠️ **Bundle it with `ChurnObserver` into one struct rather than adding a parameter.** `process_live_batch` is at
  exactly seven arguments (`live.rs:467`) and `clippy::too_many_arguments` defaults to seven, which `clippy.toml`
  doesn't raise. ⚠️ **That bundling breaks the existing churn scanner**, which asserts each driver file literally
  contains `ChurnObserver::from_env(` (`churn_monitor/tests.rs:218`). Either keep that literal at the driver site or
  update the scanner in the same commit.
- A new `IndexEvent::FolderActivity { volume_id, window_start, folders: Vec<FolderChangeRollup> }`, where the rollup
  carries the folder path, the four counts, and **`last_event_at`** (`EventBundle` needs it, and it's what `reconcile`'s
  staleness horizon reads).
- ⚠️ **Adding an `IndexEvent` variant is nine more compiler- or test-enforced edits**: the `IndexEventKind` variant
  (`sink.rs:355`), `ALL: [Self; 21]` → 22 (`:407`), a `slot_of` arm (`:438`), `IndexEvent::kind()` (`:493`),
  `volume_id()` (`:525`), `testing::events::one_of_every_kind()` (`:591`), the exhaustive `route()` match
  (`index_mapping.rs:406`), a new `Destination` variant (`:363` — ❌ don't reuse `AnalyticsOnly`, which would lie in the
  one enum whose job is saying where an event went), and the non-frontend-destination list in
  `events/index_mapping/tests.rs:44`.
- ⚠️ **The app side must NOT route through managed state.** `route()` takes `app: Option<&AppHandle>`
  (`index_mapping.rs:406`) and the completeness test calls `route(event, None)`, so a handler reaching for `app.state()`
  silently drops every bundle in that test and whenever `app` is `None`. Use the process-global from item 3, exactly as
  `PathAccessDenied` → `restricted_paths::record_denial` does.

⚠️ **Inherit both of `ChurnObserver`'s guarantees.** It is passed `&mut` so a live batch cannot be processed without
one, and `every_live_loop_owns_a_real_churn_observer` (`churn_monitor/tests.rs:218`, the `process_live_batch(` scan at
`:248`) fails when a driver doesn't build a real one. Note it asserts an EXACT driver list, not a subset. Write the
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

**Split `run_wake` in two** while its only callers are tests. ⚠️ **Naively "taking the drained rows" would throw away
the guarantee `job.rs:50-54` spells out**: the inbox is drained only once a turn is CERTAIN to run, so a budget too
small to say anything, or a store that won't take a new thread, leaves the backlog exactly as it was. Rows handed over
up front are lost on `NothingDue` and `Unavailable`, which are ordinary paths, not crashes. So:

- **A prepare step, under the lock**: gates, `due_at`, `compact(&inbox.scored(), …)` WITHOUT draining, the empty-render
  bail, and `create_conversation`. Only if all of those pass does it `drain()` and `persist::clear`. Every step that can
  decline still does so before anything is spent, which is the property the original order exists for.
- **A run step, lock released**: takes the digest, the conversation id, and the drained rows, and runs the turn.
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
- One argument: a short reason, for memory (M3). ❌ **Never log it verbatim.** `cmdr.log` ships in error reports,
  including the auto-dispatched ones the user never previews, and its redactor is path-shaped
  (`redact::redact_line_salted`, `redact/mod.rs:14`) so it does nothing to prose. Log that a wake was quiet, not what it
  said about which folders.
- ⚠️ **Adding an agent tool is a six-part checklist** (`agent/tools/CLAUDE.md:41`), and the plan owes all of it: the
  registry entry, a `ToolId` variant, `ToolId::KNOWN` (`agent/llm/types.rs:190`, a fixed `[ToolId; 14]`),
  `from_wire_name` / `as_wire_name`, `EXPECTED_AGENT_TOOL_NAMES` (`mcp/tests/tool_registry_tests.rs:728`), and a rail
  label pair in `ask-cmdr-labels.ts:13`. Miss the `ToolId` half and the tool parses as `Unrecognized`, gets refused by
  `refuse_unavailable` before dispatch, and `tool_id_known_maps_one_to_one_onto_agent_view` (`agent/tools/mod.rs:51`)
  fails.
- ⚠️ **`FIXED_PROMPT_OVERHEAD_TOKENS = 4_972`** (`agent/chat/budget.rs:88`) is pinned by
  `every_call_pays_about_3_500_tokens_of_fixed_overhead` (`chat/context/cost_tests.rs:97`), which opens by asserting
  `tools.len() == 14`. A new tool moves both. Update the constant AND `agent/chat/DETAILS.md`'s "what the budgets buy"
  section, as the test's own failure message instructs.
- ⚠️ **It is visible to the rail too**, since there is one `agent_tool_view()`. In a user chat it is a dead schema cost,
  and it must never delete anything there. Guard on the wake path, and test that a rail turn calling it is inert.
- Needs a store-level `delete_conversation`; only `ask_cmdr_archive_conversation` exists today. Considered and rejected:
  archiving. Archived threads still accumulate, and "we looked and found nothing" fifty times is not a record worth
  keeping.
- ⚠️ **Preserve the cost record.** `cost_meter.conversation_id` is `ON DELETE CASCADE` (`migrations.rs:209`), so
  deleting the thread erases what that wake spent from the one place the user can see what the proactive agent costs. ❌
  **`ON DELETE SET NULL` is not available here**: the column is `NOT NULL` on purpose, and `migrations.rs:203` spells
  out why (SQLite treats NULLs as distinct in a PK, so a nullable column inside it breaks `ON CONFLICT DO UPDATE` and
  every write inserts a duplicate instead of upserting).

  **Do this instead**: create one reserved "quiet wakes" conversation row at migration time, hidden from the session
  list. Before deleting a noop wake's thread, fold its `cost_meter` rows into the reserved id with the same
  `ON CONFLICT (day, conversation_id, provider, model) DO UPDATE` shape `record_cost` already uses
  (`store/query.rs:549`), summing tokens and micros. ⚠️ Carry `priced` as `priced AND excluded.priced`, or the reserved
  row claims a complete price it doesn't have. Then delete the thread.

  ⚠️ Two edits this needs that don't exist yet: **`ConversationOrigin` has exactly one token** (`Notification`,
  `agent/types.rs:54`), so the reserved row needs its own rather than masquerading as a wake thread; and
  **`list_conversations` has no origin filter** (`query.rs:190` filters on `archived` only), so "hidden from the session
  list" is a new WHERE clause. Both land in M1, not M2, because M2's thread icon reads the same token set.

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
  change: `positionOf` is linear over min/max, which is correct in index space, and ticks and snap targets are consumed
  in the same space. Three traps: `ariaValueText` is handed the raw Ark value (`ui/Slider.svelte:94`), so map back
  before formatting or screen readers announce "3"; `SettingSlider.svelte:76` gates `ariaValueText` on `unit` being set,
  and this row has no unit, so that line needs changing too or there's no spoken text at all; and a stored value not in
  the table must resolve to the nearest stop, ❌ never `indexOf → -1`.

  Costs about 40 lines plus a `SettingSlider.svelte.test.ts` (none exists today) and two doc edits in
  `settings/components/DETAILS.md:44`. **The `select` fallback is NOT cheaper**: `type` flips to `'enum'` so the value
  becomes a string, and ten options mean ten new keys, which is a hundred translated strings. The slider needs zero new
  keys if the readout uses `formatDuration` from `$lib/units` — noting that it emits hardcoded English and renders `5s`
  / `30m` / `1h`, which is the same untranslated compact form every ETA in the app already uses.

  ❌ Don't persist the stop INDEX: reordering the table would silently change every user's setting.

- **`askCmdr.wakeToast`** (boolean, default true). Whether a staged proposal raises a toast.

**The two-value description** ("reacts within 30 seconds, quieter folders within 30 minutes") can't come from
`descriptionKey`, which resolves to a static string. `SettingRow` takes `description` as a prop, so the section passes a
computed `tString(...)`; the search index keeps the static registry text. Write it as an ICU message taking two
**preformatted string** params, ❌ not `{n, number}` (`messages/CLAUDE.md`). ❌ Don't add a `formatWakeDelay` helper:
`cmdr/no-private-unit-format` rejects new formatter-shaped names doing unit work.

### 8. A dev-only force-wake command

So verification doesn't mean waiting out a deadline. ⚠️ Behind the `playwright-e2e` Cargo feature, alongside
`set_test_throttle`, ❌ not an env-var hook: `test_mode.rs:8` draws the line at soft hooks being "strictly additive" and
never replacing production logic, and forcing a wake replaces the timer.

### M1 tests

TDD, red first (`tdd-red-green.md`):

- **The `Option` merge** (`wake/tests/inbox.rs`): a cold contribution must not erase a warm row's deadline. This is the
  one that compiles wrong, so it earns a test before the port.
- **`reconcile` leaves a `None` row alone**, and `next_deadline` skips it.
- **Tier order at every slider stop** (`wake/tests/interest.rs`), including the 6 h warm cap.
- **The tap scanner** (beside `churn_monitor/tests.rs:216`): every live-batch driver builds a real tap observer.
- **The flags → `ChangeKind` priority**, with a multi-flag event asserting the documented winner.

Written after:

- **Rollup → bundle mapping**: counters, `last_event_at`, and window survive the crossing; `WeightLookup::Unscored` maps
  to `FolderImportance::Unknown` (`interest.rs:59`), ❌ never to zero.
- **A rename-only batch produces a non-empty bundle** (the regression anchor for the retained-paths change).
- **A noop wake leaves no thread but keeps its cost row**, and a rail turn calling `nothing_to_suggest` deletes nothing.
  `wake/tests/job.rs` already drives `run_wake` against an in-memory migrated DB, `FakeAgentLlm`, and a dispatcher
  double, so both fit there directly.
- **Restart reconciliation**: rows persisted, reloaded, settled, stale ones counted.

⚠️ **The end-to-end test has to be two tests.** `process_live_batch` is `pub(in crate::indexing)` (`live.rs:467`), so
the app crate can't call it, and `cmdr-index` can't name the agent. Split:

- **Crate side**, in `cmdr-index`: a synthetic batch through `process_live_batch` asserts the emitted `FolderActivity`
  carries the right rollups, `last_event_at`, and flags-priority winner.
- **App side**: a hand-built `FolderActivity` through `route(event, None)` into the tap adapter, the inbox, and a wake
  against the fake. `route`'s `Option<&AppHandle>` is what makes this half possible.

⚠️ **The E2E fake is shared and must not simply be swapped.** `scripted_fake_llm()` (`chat.rs:139`) is a fixed `Say`
script returned unconditionally under `test_mode::ask_cmdr_fake_active()`, and item 5 moves that resolution into
`agent/chat/session.rs` where the wake shares it. Changing the script would change what the RAIL's existing E2E specs
see. Give the wake its own scripted variant selected by caller. `ScriptedTurn::CallTools` (`agent/llm/fake.rs:26`)
already exists, so the variant is cheap.

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

### 1. Fix `read_cmdr_md()` first, and say what the fix means

`chat/runtime/mod.rs:231` calls `dirs::home_dir()` directly, so `~/.cmdr/CMDR.md` bleeds into every E2E run and every
worktree. ⚠️ The resolution has to be stated, because "honor `CMDR_DATA_DIR`" and "`~/.cmdr/CMDR.md` stays put" sound
contradictory: **read `<CMDR_DATA_DIR>/CMDR.md` when the env var is set, else `~/.cmdr/CMDR.md`.** Production is
unchanged; only isolated environments move. While in there, give it the same size cap as memory (item 4): it has none
today, so a large hand-written `CMDR.md` already taxes every turn.

TDD, and it makes every later memory test deterministic. **Independent of everything else; can land before M1.**

### 2. Location

`<data-dir>/ai/memory/`, with `AGENTS.md` as the hub. The app data dir, ❌ not `~/.cmdr/`: it is app-managed state
rather than user config, `app_data_dir()` is already the canonical per-OS path on all three platforms, and it inherits
`CMDR_DATA_DIR` isolation for free. Shared memory would mean an E2E run writing personal facts into David's real agent
memory.

`~/.cmdr/CMDR.md` stays user-authored: a dotfile in home is where a hand-edited, dotfiles-repo-able config belongs.

### 3. Feeding it, and the injection problem

⚠️ **This is the security-critical item in the whole plan.** `build_system` (`chat/context.rs:251`) appends `CMDR.md`
raw and unfenced AFTER the entire system prompt, which is the strongest override position there is. Memory added the
same way would sit after every rule, in the cached prefix of every turn.

The write path is reachable from untrusted text: `image_facts` returns the full stored OCR of the user's images
(`mcp/tool_registry/mod.rs:632`, whose own comment calls it the most sensitive thing either photo tool emits), and file
names come off disk. So a crafted filename, or a picture of a sentence, could get the agent to write instructions into
`AGENTS.md`, where they would ride every later turn, including ones that call `propose_suggestions`. It survives
restarts and thread deletion.

So:

- **Memory goes BEFORE the rules, not after**, in a delimited block, introduced by a line saying it is data the agent
  wrote about the user and never overrides the rules that follow.
- **`TurnParams.cmdr_md` becomes two fields**, ❌ not one concatenation, so each is labelled in its own voice: **what
  the user tells the agent**, and **what the agent learned**. `run_wake` passes both (it passes `None` today,
  `job.rs:99`).
- **The write instruction (item 8) says memory records facts about the user and their preferences, never instructions to
  itself.**

### 4. Budget, and why a flat 8 KB was the wrong shape

⚠️ **The system string is never elided.** `assemble_prompt` (`context.rs:265`) tightens tool-result elision only, so
memory is a permanent, non-elidable tax. Run the numbers a byte cap doesn't: `MIN_LOCAL_CONTEXT_TOKENS = 16_384` at
`PROMPT_BUDGET_WINDOW_PERCENT = 60` is a 9,830-token budget (`budget.rs:56,66`); fixed overhead after M1 and M3 is
roughly 5,300; 8 KB of memory at `CHARS_PER_TOKEN_ESTIMATE = 4` is another 2,048. That leaves under 2,500 tokens for the
digest, the envelope, the history, and every tool result, on a configuration the app supports. And the agent writes this
file itself, so it can permanently degrade its own chat.

**Feed a slice sized as a PERCENTAGE of the resolved prompt budget**, not a byte count, and when memory exceeds it, feed
the head and say in the prompt that it was truncated. Keep a byte cap too, as a disk guard.

⚠️ Two caps, two reasons, ❌ don't conflate them: the per-file cap protects the prompt; the 64 KB directory cap protects
disk. When the directory cap is full a write returns a TYPED refusal telling the model to prune, ❌ never a silent
failure.

⚠️ **`FIXED_PROMPT_OVERHEAD_TOKENS = 4_972`** (`budget.rs:88`) moves again here: `cost_tests.rs:97` asserts
`tools.len() == 14` and then the measured overhead. Two more tools, so update the constant and `agent/chat/DETAILS.md`'s
"what the budgets buy".

### 5. `Access::Memory`

A fourth variant beside `Read`, `Propose`, and `Write`, with its own hand-authored allowlist mirroring
`EXPECTED_PROPOSE_TOOL_NAMES`. `test_agent_tool_view_never_writes` widens to admit exactly this and nothing else, so the
guarantee becomes "the agent writes only into its memory folder", structural rather than a rule in a doc. ⚠️ A
deliberate widening of the app's central agent-safety invariant; the allowlist is what stops it being acquired as a side
effect of editing a registry line.

Also needs a `Memory` arm in `access_is_dispatchable` (`agent/tools/view.rs:27`) plus a fourth assertion in its
per-variant test (`:117`), and a rewrite of the refusal copy at `view.rs:50`, which still says the agent can't change
anything.

### 6. Two tools

Path-aware from day one so the second file costs nothing:

- `memory_write(path, content)`: create or fully replace.
- `memory_edit(path, old_string, new_string)`: exact match, refuses a non-unique match.

❌ No read or list tool yet: `AGENTS.md` is auto-fed and it is the only file. Add both the moment there is a second one.
Every schema rides in the cached prefix of every turn, including the rail's.

⚠️ Both go through the same six-part tool checklist as `nothing_to_suggest` (M1 item 6), including two rail-label
message keys each, so four keys and thirty-six translations.

⚠️ **They are callable from the rail, not just from wakes.** That is intended ("remember this for me"), and it is also
the mechanism behind item 3's injection risk. State it rather than implying it.

### 7. The jail

One function both tools call, unit-tested: reject absolute paths, reject any `..`, resolve symlinks and re-check
containment, allow `.md` only, enforce both caps.

⚠️ Three things that will bite:

- **`canonicalize` fails on a file that doesn't exist yet**, so `memory_write` creating a file must canonicalize the
  PARENT and join a validated file name, then re-check containment.
- **Non-UTF8 or unreadable memory reads as absent** under the `read_to_string(...).ok()` shape, so the agent silently
  believes it has no memory and starts over. Log it.
- **Write durably.** `config::durable_write_json` (`config.rs:76`) is the existing temp-plus-fsync-plus-rename helper.
  Item 10 invites the user into the folder while the agent may be writing, so a torn file is reachable.

### 8. The system prompt

Encourages capturing what matters, on request or on meeting something worth keeping, and pruning what has gone stale.

⚠️ **`SYSTEM_PROMPT` currently says "You never act: you have no tool that changes, moves, deletes, or renames
anything"** (`system_prompt.rs:47`), and `prompt_states_the_read_only_self_description` (`:142`) pins the phrase "never
act". `Access::Memory` makes it false. A second rewrite of the same promise, in a different file, with its own guard.

### 9. Consent

Bumping `CONSENT_COPY_VERSION` (`agent/consent.rs:16`) revokes every beta user. ⚠️ What the bump carries:

- The rail gates on `consentState.accepted`, so a user's whole thread history sits behind the consent screen until they
  re-accept, and `AskCmdrSection.svelte:133` then renders a plain "Off", indistinguishable from never having opted in.
  Needs "here's what changed" copy that doesn't exist.
- The disclosure list is duplicated in `AskCmdrConsent.svelte:42` AND `AskCmdrSection.svelte:151`. Edit both.
- ⚠️ **`askCmdr.consent.noContents` ends "Ask Cmdr only looks and speaks; it never changes anything."** That sentence is
  what the read-only promise rests on, so it needs a rewrite, not a sixth bullet.
- ⚠️ **The bigger disclosure is not that the agent writes files.** It is that everything the agent remembers is sent to
  the user's provider on every message, indefinitely, including facts derived from OCR of their photos. Say that, or the
  re-prompt collects a signature on the wrong thing.
- ⚠️ **Purge the inbox on a consent miss.** `readiness.rs:44` is explicit that admitting rows means keeping a record of
  what the user has been doing for a purpose they haven't agreed to. After the bump every user is un-consented while
  their `agent_inbox` rows (folder paths, counts, timestamps) sit on disk. Clear them, and test it.
- These keys carry `@key.screenshot: ask-cmdr-consent.png`, so `pnpm i18n:shots` needs a re-run.

### 10. Two controls

"Open memory folder" and "Forget everything", in the Ask Cmdr section.

- ⚠️ **"Open memory folder" has no mechanism today.** The settings window must learn the resolved path (Rust-only,
  `CMDR_DATA_DIR`-dependent) and tell the main window to navigate. `ExecuteCommand` (`window_events.rs:30`) carries a
  bare `command_id` and nothing else, and it's the only settings-to-main dispatch. Needs a command returning the path
  plus a payload-carrying event.
- ⚠️ **"Forget everything" is a soft dialog**, so it needs an id in `lib/ui/dialog-registry.ts` AND a row in
  `lib/dialog-gallery/gallery-registry.ts`, or `dialog-gallery-coverage` fails. `DeleteAiModelDialog.svelte` is the
  precedent. Plus its colocated `*.a11y.test.ts`.

### 11. What does NOT need doing

Verified, so nobody re-audits it: crash and error report bundles don't touch the data dir beyond `index-*.db` sizes
(`diagnostics_snapshot.rs:86`), and the log bundle takes only `cmdr.log*` (`logging/mod.rs:117`), never `llm-logs/`.
**The residual hole is `cmdr.log` itself**: ❌ never log memory content, and never log a wake reason verbatim (M1 item
6).

### M3 tests

⚠️ **There is no Tauri mock runtime in the tree** (`chat/runtime/tests.rs:5` says so outright), and every registry
handler takes an `AppHandle`. So the design must split or these tests can't exist: **a pure `MemoryStore` parameterized
on a root `Path`**, holding the jail, the caps, the write, the edit's uniqueness refusal, and the directory-full
refusal, unit-tested against a `tempdir`; plus a thin handler that resolves the root from the `AppHandle`.

TDD: the jail (every escape attempt), the caps, and the `CMDR_DATA_DIR` fix. After: prompt assembly carrying both files
labelled and fenced, memory truncating rather than blowing the budget at `MIN_LOCAL_CONTEXT_TOKENS`, the consent
re-prompt, and the inbox purge on a consent miss.

## M4: the feedback loop

**Intent**: an approval or a rejection the agent never hears about is a lesson it can't learn.

⚠️ **A `ConversationEvent` cannot carry the lesson.** `store/query.rs:44` is explicit: conversation events "NEVER enter
the LLM transcript (they exist for the user's eyes and the history view only)". So an outcome recorded only that way
teaches the agent nothing, and approvals, which get no follow-up turn, would produce zero learning while rejections
produce all of it. That asymmetry would make the agent over-correct.

**Two channels, both needed:**

- **The user's timeline**: a typed `ConversationEvent`, for their eyes.
- **The agent's lesson**: the always-path writes the outcome into memory (M3) directly, with no model call. Cheap, and
  it covers approvals too. The follow-up turn, when it runs, gets the outcome as its input text the way `run_wake` hands
  over the digest (`job.rs:98`).

**Where the hook goes.** `reject_group` (`store/proposals/claim.rs:169`) is a conditional
`UPDATE … WHERE id = ?1 AND status = 'pending'` and returns `Rejected` only when a row actually moved, so putting the
hook inside the `if let (RejectOutcome::Rejected, Some(group))` arm of `suggested_ops::reject`
(`suggested_ops/mod.rs:134`) makes it once-per-group by construction, across restarts, with no new column.

⚠️ **Escape on a rename dialog is recorded as a rejection.** `cancel_bulk_rename_proposal`
(`commands/agent/bulk_rename.rs:253`) calls `suggested_ops::reject`. With the hook in that arm and `askCmdr.proactive`
defaulting on, dismissing a dialog would spend a model call and drop a "why did you say no?" turn into the user's ACTIVE
RAIL THREAD, since that sweep's `conversation_id` is the rail conversation. **The most likely bug in M4 to ship
unnoticed.** Either distinguish a dismissal from a rejection, or give `cancel_bulk_rename_proposal` its own outcome.

⚠️ **Coalesce per SWEEP, not per group.** "Reject all" over an eight-group sweep is eight `Rejected` outcomes, so eight
model calls, all serialized on one thread by `ConversationLocks`. One turn per sweep.

⚠️ **Approval's real outcome is not at `approve`.** `suggested_ops::approve` (`mod.rs:118`) is only the claim; what
actually happened lands later through `ProposalReportingSink` into `mark_group_completed` (`bridge/decorator.rs:29`). An
outcome recorded at claim time says "approved" for a group that then skipped every file. That seam holds only a
`Connection` on the write engine's thread, with `write-ops-isolation` watching.

⚠️ **Not `AskCmdrStreamEvent`.** A rejection arrives through `suggested_ops_reject`
(`commands/agent/suggested_ops.rs:187`) with no open chat channel, so mirroring `ModelChanged` there buys nothing.
`SuggestionsChanged` already fires on every approve and reject (`suggested_ops/mod.rs:121,135`) and the rail can refetch
on it, which removes one of the three Rust surfaces this milestone looked like it needed.

The conversation link survives to the hook: `get_group` gives `set_id` (`read.rs:63`), `get_sweep` gives
`conversation_id` (`:78`). It is nullable and NULLed when a thread is deleted, and M1 adds `delete_conversation` — a
quiet wake proposes nothing so it can't orphan a sweep today, but keep the two in view.

**Tests**: TDD the dialog-dismissal case (the one that would otherwise ship) and the per-sweep coalescing guard. After:
the outcome reaches memory on both approve and reject, and a rejection with `askCmdr.proactive` off runs no turn.

## Execution order

Sequential: M3.1 (the `CMDR_DATA_DIR` bug, independent) → M1 → M2 → M3 → M4. Inside M1 the order is the numbered one,
and it is deliberate: the inbox's signature settles before the tap is wired against it.

❌ Don't parallelize M1's steps across agents. The tap, the writer thread, and the runner meet at one channel and one
event variant, and three agents converging on `process_live_batch`'s signature is how you get a merge that compiles and
taps nothing.

Copy can be DRAFTED ahead, but every key lands with its nine translations or not at all. That applies to M1 too, which
carries `nothing_to_suggest`'s two rail-label keys.

## Open for David

1. **The four budget bumps** listed under "Needs David's consent" above, `index-crate-isolation` most urgently, since M1
   cannot land without it.
2. **The follow-up turn on rejection** costs a model call per rejected SWEEP (the plan coalesces; per group would be
   eight calls for one "reject all"). Confirm before M4.
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
