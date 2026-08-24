# Wake pipeline details

Pull-tier docs for `agent/wake/`. Must-knows: `CLAUDE.md`. What a wake produces:
`../suggested_ops/DETAILS.md`.

## The tap point (agent-spec 18.14, resolved)

The agent subscribes as a **second observer inside `process_live_batch`**
(`crates/cmdr-index/src/indexing/watch/event_loop/live.rs`), where `ChurnObserver` already sits. Four things decide it:

- **That function is the single funnel both live loops go through**, `live.rs` and `replay.rs` Phase 3. Its own comment
  records that hooking only one of them once measured nothing on the cold-start replay path, and `ChurnObserver` is
  passed by `&mut` precisely so a batch cannot be processed without one. The agent observer inherits that guarantee.
- **It sits AFTER correction, not beside `ChurnObserver`.** The churn observer runs first thing on raw deduplicated
  paths, which is right for measuring churn and wrong here: a rename would arrive as a create plus a delete, and an
  `rm -rf` as sixty thousand removals. After `detect_renames_by_inode` and `storm::detect_storm_anchors`, a rename is
  one `Renamed` and a storm is one anchor. That is what the plan means by a second interest-oriented stage over an
  already-corrected stream.
- **It crosses the crate boundary on the existing `IndexEvent` / `EventSink` seam**, never a new channel:
  `cmdr-index` cannot know about the agent (`index-crate-isolation`), and this is the seam it already reports outward
  through.
- **The payload is a per-batch, per-folder ROLLUP, not one message per file.** `INGESTION_HARD_CAP` is 5,000,000; a
  per-file message would put five million of them across the boundary on exactly the path the counters exist to
  survive. A rollup is bounded by distinct folders in one batch.

**The `downloads/` watcher coexists rather than merging.** It is FDA-gated, single-folder, `notify`-based, and
browser-rename-aware, serving go-to-latest-download. Merging would tie a user-facing navigation feature to a lifecycle
that is consent-gated, key-gated, and may be entirely off: go-to-latest-download must not stop working because somebody
declined the AI consent screen. **The agent must never assume it is running.**

## Why two entry points over one fold

The pipeline has two sources: individual events (user actions) and per-batch rollups (the tap). `Merger` is the single
fold, with `coalesce` and `merge_bundles` as thin entry points, so the windowing rule, the deadline anchor, and the
ordering are written once. A test builds the same changes both ways and asserts the two agree.

A rollup carries no per-event times, so it is placed by its own window start: two batches straddling a boundary stay
two bundles. Exact as long as one input bundle lies inside one window, which a per-batch rollup does, since a live
batch spans milliseconds and a window at least a second.

## Windows tumble and are anchored to the epoch

`at / window * window`, never to the first event in a batch. Two consequences the pipeline depends on: the same events
coalesce identically however they arrive, and a morning burst and an evening burst in one folder can never share a
deadline. Merged, the later burst would inherit the earlier one timing and the agent would report tonight arrivals as
this morning.

A zero window degrades to one second rather than dividing by zero: a caller passing `Duration::ZERO` is asking for no
coalescing, and per-second bundles are the honest answer.

## The interest formula

`interest = importance_weight * max(intent_share, volume_signal)`, clamped to `0..=1`.

**The stronger of the two, not the average.** A single file landing in Downloads is the feature headline case;
averaging would dilute it to lukewarm with its own low volume. Volume saturates logarithmically at 1,000 changes so
one pathological folder cannot out-shout every other bundle in the inbox.

**`FolderImportance` has three variants because `WeightLookup` does**, and deliberately does not inherit its `score()`,
which reports `Floored` and `Unscored` as the same `0.0`. `UNKNOWN_IMPORTANCE_WEIGHT` is 0.35: above zero so a folder
the scorer has not reached stays visible, below any folder actually scored as mattering.

**Both numbers are tuning knobs, not settled design** (agent-spec 18.5). The importance weight and the hot/warm
thresholds stay guesses; what the user gets to move is the CADENCE.

## The three tiers, and the one number the user moves

`wake_delay(interest, hot_delay) -> Option<Duration>` takes the user's cadence as a value, so the core stays pure and
the setting is an input like every other one here. It threads on through `deadline_for` → `Inbox::admit` →
`Inbox::admit_if_permitted`; nothing under `wake/` reads a setting, and `DEFAULT_HOT_DELAY` (5 s) is what a caller with
no user answer yet passes.

- **Hot IS the setting**, whatever stop the slider is on (5 s through 2 h).
- **Warm derives**: `min(hot × 60, MAX_WARM_DELAY)`, a minute of patience for every second of attentiveness. One number
  moves both tiers, so "calmer, please" means calmer everywhere rather than in the one place the user happened to look.
  The six-hour cap stops the quiet end from turning warm into five days.
- **Cold is `None`**: no deadline, so it rides along and never wakes the agent on its own.

⚠️ **The ORDER is a pinned contract** and a derived tier is exactly the arithmetic that inverts at one end, so a test
walks every slider stop, not just the default. (The cap can only invert the order for a hot setting above six hours,
which the slider cannot reach.)

## The three settings, and where each one is read

`settings.rs` is the ONE place the loop reads user settings: `askCmdr.proactive` (the fourth gate, beside the three in
`readiness.rs`) and `askCmdr.wakeDelay` (the hot tier's delay). The third row, `askCmdr.wakeToast`, is a frontend
choice about whether a staged proposal raises a toast; it reaches the backend only as a reason to re-read, so nothing
here holds it.

**`askCmdr.proactive` defaults to TRUE**, which is the feature: the agent watches whenever consent and a working
provider both exist. It is not what protects somebody who never wanted AI — `readiness.rs` holds consent, disk access,
and the key, and a wake spends nothing until all three are open. This row is the user's own "no thanks". It shipped
FALSE while the surfaces were being built, because a release landing then would have created threads with no indicator,
toast, or readiness surface to find them by.

⚠️ **`settings.json` is SPARSE**, so both reads are `Option` and both defaults are spelled out in `from_parts`.
`unwrap_or_default()` would ship `false` forever for the boolean — silently turning the feature off for every user who
never opened the row — and a ZERO-second cadence for the delay, which is a wake per live batch. A value off the slider's track (a hand-edited file) is clamped to `MIN_HOT_DELAY..=MAX_HOT_DELAY`:
below the shortest stop the agent wakes on its own noise, and above the longest the warm tier is pinned to its cap
anyway.

⚠️ **`WAKE_DELAY_STOPS` is mirrored by `constraints.sliderStops`** on the registry entry
(`apps/desktop/src/lib/settings/definitions/ai.ts`), and the warm derivation is mirrored by the section's readout copy
(`AskCmdrSection.svelte`). Nothing enforces either pair mechanically, so a stop or a multiple changes on both sides at
once or the settings row describes a cadence the loop does not run.

**A change arrives as a control message, never as a poll.** `settings-applier.ts` calls `ask_cmdr_wake_settings_changed`
for all three rows, which sends `WakeControl::SettingsChanged`; the writer thread re-reads and the loop recomputes its
park, so a shortened cadence re-arms the timer immediately.

⚠️ **Re-pricing is the half that is easy to miss.** `Inbox::admit` merges min-only on purpose (the starvation guard), so
a LENGTHENED cadence would reach only bundles arriving AFTER the change: somebody asking for a calmer agent would keep
being woken on the old schedule by everything already queued. `Inbox::reprice` shifts each waiting deadline by its
tier's delta, so the row keeps the moment it arrived and moving the slider there and back is a no-op. A cold row stays
without a deadline at every cadence.

## Forcing a wake, for a test

`force_agent_wake` (`commands/e2e.rs`, `playwright-e2e` only) stages one folder's activity through the real
`send_rollup` lane and then sends `WakeControl::ForceWake(ForcedWake { only_folder })`. Verifying the loop otherwise
means sitting out a cadence that runs up to half an hour, and hoping the fixture tree is somewhere the indexer walks.

⚠️ **A Cargo feature, ❌ not an env-var hook.** `test_mode.rs` draws the line at soft hooks being "strictly additive",
and forcing a wake REPLACES the timer.

What the force skips is exactly two things: the timer (`not_before` and `Inbox::due_at`, via
`PrepareParams::ignore_deadlines`) and the `askCmdr.proactive` toggle. ❌ It skips no GATE: consent, disk access, and a
configured provider are all still checked, so a forced wake on an unconsented profile stores nothing and runs nothing.
An empty inbox still renders an empty digest and opens no thread. A force arriving while a wake runs is held rather
than dropped, and lands on the pass after `WakeFinished`.

### The force reports on ITS folder and nothing else

⚠️ **The inbox is not empty under a test run, and a wake reports on everything waiting.** The indexer's tap feeds the
same inbox from whatever files the rest of the suite churns. Left alone, a spec staging one folder and asserting
"What changed in 1 folder" reads a number nobody controls: measured on the Linux lane, forced wakes covered 3, 11, and
30 folders, and a quiet one looked at 40. `thread_title` names the thread after the TOP-RANKED folder, so the thread
the spec goes looking for ends up titled after somebody else's directory.

So `ForcedWake::only_folder` carries the staged folder, and `WakeLoop::isolate_inbox_to` cuts the inbox down to it
(`Inbox::retain_folder`) as the wake is prepared. The dropped rows go from disk too, via `persist::save_all`.

- **The narrowing happens at the WAKE, ❌ never where the rollup was staged.** A force arriving while another wake runs
  waits out that whole model call (`wake_in_flight`), and the tap keeps filling the inbox for all of it, so clearing at
  staging time would leave a seconds-wide hole.
- **One message carries the whole request**, so no caller can order the clear and the staging wrongly. The command
  stages and names the folder from one binding; a second spelling could name a folder nobody staged, and the wake would
  find an empty inbox.
- `folder: None` still wakes on everything waiting and narrows nothing.
- `stage_agent_rollup` (same file, same feature) is the other half: it puts a folder in the inbox WITHOUT waking, so
  `ask-cmdr-wake.spec.ts` stands that noise up on purpose rather than waiting for CI to supply it on some unlucky run.
  ❌ Don't drop those decoys as redundant: without them the narrowing is untested and the premise silently returns to
  luck.

⚠️ **The E2E fake counts as a configured provider** (`snapshot.rs::has_api_key`), because `resolve_agent_llm` answers
`Ok` under `CMDR_E2E_ASK_CMDR_FAKE` with `ai.provider` still off. Without that branch the gate would report
`NeedsApiKey` for a slot that resolves fine, and no wake could run under the harness at all.

⚠️ **A wake's scripted fake says something different from the rail's** (`chat/session.rs`, selected by `AgentSlot`).
The rail's E2E specs count replies by matching their exact sentence, so one shared script would both break them on any
change here and make a wake's thread indistinguishable from a chat the user started.

**The wake slot has THREE scripts**, because a wake ends three materially different ways and two of them are decided
by a TOOL CALL rather than by what it says. `force_agent_wake(folder, script)` picks one through
`test_mode::WakeFakeScript`:

- `"reply"` — the ordinary answer. A thread appears and nothing is staged.
- `"quiet"` — calls `nothing_to_suggest`, so the wake deletes its own thread.
- `"propose"` — calls `propose_suggestions`, so a group is staged and the toast fires.

⚠️ **The choice STICKS**, so every spec says which one it wants, and a spec using `"propose"` puts the selector back
before it finishes or the next forced wake anywhere in the run stages another group. ⚠️ The proposing script uses an
explicit `paths` group, ❌ never a selector: a selector resolves against the drive index at creation, which would make
what it stages depend on whether the fixture tree happens to be indexed yet.

⚠️ A spec asserting that nothing was left behind can only do so AFTER the wake finished: the thread is opened before
the turn and deleted after it, so a mid-flight poll legitimately sees the row. `ask-cmdr-wake.spec.ts` waits, then uses
a second (loud) wake as the control that proves the lane ran at all — without one, "no thread appeared" passes just as
well with the whole loop dead.

## The digest budget

Enforced against the REAL rendered string, not a sum of per-line estimates: `div_ceil` per line does not add up to the
cost of the whole. It reuses `chat::budget::estimate_tokens_str` so the digest and the prompt cannot drift apart about
what a token costs.

The budget goes to the highest-interest folders first; the rest roll up by shared parent, or into one line at the
common ancestor when there are too many parents for that to read as a summary. Every folder is either a line or inside
a rollup, and a test sums both sides to prove nothing goes uncounted.

At an impossible budget the digest is EMPTY rather than over: an overrun would push the rest of the turn out of the
window, which is the failure that once cost a rename turn the evidence it was reasoning from.

### The rendered digest is prompt-only

`Digest::render()` is English, and deliberately so: it is a prompt. What gets PERSISTED as the thread's first message is
`Digest::to_wire()`, a `WakeDigest` (`agent/llm/types.rs`) carrying folders, four counts each, and the rollups.
`AgentPart::WakeDigest` is the transcript part it rides in, and `UserTurn::Wake` is how `run_turn` is handed one.

**Decision**: the wake persists structure and renders English only at the provider boundary (`genai_impl.rs` maps the
part to a `ContentPart::Text`). **Why**: the rail shows that first message in ten locales, and a `main.db` row outlives
every locale pass we will ever run, so an English sentence stored there could never be translated. It also keeps the
prompt's wording free to change without rewriting anybody's history. The rail's own copy is
`askCmdr.wakeDigest.*`; the block renders collapsed (`AskCmdrWakeDigest.svelte`).

The row's `text_for_search` is the digest's PATHS, joined. Paths are the user's own data rather than authored copy, and
they are what somebody searching their threads would actually type.

## The inbox, and what a restart does

A merge can only pull a deadline earlier and can only raise the stored interest. The asymmetry is a starvation guard,
and it also stops a later, duller contribution from demoting what an earlier burst established.

**A cold row has NO deadline** (`deliver_by: Option<u64>`, nullable in the table since migration v7). That is what
"rides along" means mechanically: the row waits, any wake drains the WHOLE inbox and takes it along, and nothing about
it can cause a wake. That whole-inbox drain is also where a MAX-interest reporting policy comes from without anybody
writing one: whatever is waiting gets reported on, ranked by interest inside the digest's budget. Given a
real time like every other row, a trickle in a barely-scored folder comes due on its own and spends a model turn
reporting that a cache directory changed.

⚠️ **`Option::min` is exactly backwards for this and compiles silently.** Rust's derived `Ord` puts `None` below every
`Some`, so a naive `existing.min(incoming)` merge lets a cold contribution ERASE the deadline a hot one established,
and that folder then never wakes. Having no deadline is the LONGEST wait there is. Three places have to say so
explicitly, and each has a test: the merge (`soonest`), `next_deadline` (a `filter_map`, since the plain minimum
answers "nothing waiting" for a full inbox holding one cold row), and `reconcile` (only a row that HAS a deadline can
be overdue; deferring a null one would hand every cold row a deadline at each launch and inflate
`ReconcileReport.deferred`).

**A deadline missed while the app was closed waits out `SETTLE_AFTER_LAUNCH` (60s).** Launch replays the index journal,
and that roll-forward is itself a burst of corrected events; waking mid-burst would have the agent report the app own
catch-up as though the user had just done it. Announcing your own noise back at the user is worse than silence.
agent-spec 6.4 covers restart reconciliation but does not say this.

**Rows whose newest change is older than `STALE_AFTER` (7 days) are dropped and COUNTED.** Pre-proposal signal goes
stale in a way a proposal never does: a proposal is a decision the user still owes an answer to, while a three-week-old
bundle is archaeology and the folder state today is something the agent can look up.

## Degraded modes

`readiness(AgentGates) -> WakeReadiness`, in precedence order: consent, then disk access, then the key.

The order is the design, because each state asks the user for something. Asking somebody to grant Full Disk Access, or
to paste a key, for a feature they have not opted into is asking them to widen access for something they may not want.
Disk access outranks the key because it decides whether the agent can SEE anything.

**Silence lies under a pending FDA decision**: a user who declined and a user with a tidy Downloads folder see the
identical nothing, and only one of those is the feature working. So `NeedsFullDiskAccess` and `NeedsApiKey` both render
in the status corner, each with the action that closes them.

⚠️ **`NeedsConsent` is the one state that renders as silence**, and `askCmdr.proactive` being off silences the corner
the same way. Read literally, rendering every state puts a permanent AI nag in front of every user who never wanted AI,
which is the noise `SuggestedOpsIndicator` hides at zero to avoid. The gap is for a user who opted IN and hit a wall.
Both gates live in the frontend's wake indicator; the enum here stays complete, because the writer thread and the
inbox still need all four answers.

**Without consent the pipeline stores nothing** (`admits_to_inbox`). Admitting rows means keeping a record of what the
user has been doing with their files for a purpose they have not agreed to, and it would mean consenting on a Tuesday
hands somebody a backlog of everything they did since installing. With consent but no key, signal accumulates: the gap
is one the user can close and the backlog is theirs, bounded by the staleness horizon.

⚠️ **Refusing new rows is only half of that, so consent going away takes the backlog with it.**
`Inbox::purge_if_not_permitted` drops everything waiting, and the writer thread clears `agent_inbox` with it, on two
occasions: at launch, right after the reconcile and before the write-back (`agent::start` refreshes the gates just
before the thread comes up, so that is the first moment a launch can tell), and on every `ReadinessChanged`. Both
matter, and the launch one is what a `CONSENT_COPY_VERSION` bump needs: it un-accepts everybody at once, and their rows
would otherwise sit on disk until somebody re-accepted. Only `NeedsConsent` purges. The other two are gaps the user can
close, not a purpose they withdrew.

## Persistence

`agent_inbox` (migration v6, `deliver_by` made nullable by v7's table rebuild — SQLite cannot drop a `NOT NULL` in
place). `(folder, window_start)` is the PRIMARY KEY **because it is the merge key**, so the table
cannot hold two rows the in-memory inbox would have merged. No conversation link and no foreign key: the inbox is
pre-proposal signal and nobody has been asked anything yet. Counters are four columns rather than a blob, so `main.db`
stays inspectable in any stock `sqlite3` browser.

`persist.rs` maps onto the store flat row type rather than the store importing this vocabulary, the direction
`proposals/` takes with `NewGroup`. Times saturate at the u64/i64 boundary rather than wrapping: an absurd clock must
not turn a waiting row into one overdue by an epoch.

## Gotcha: a module missing from `agent/mod.rs` reports zero tests, not an error

`cargo test --lib agent::wake` on an undeclared module prints `0 tests, N filtered out` and exits 0. A suite that does
not exist and a suite that passes look identical from a distance. If a new test file seems to be doing nothing, check
the `mod` declaration before debugging the tests.

Related: an intra-doc link that was unambiguous when written can become ambiguous when a module of the same name
appears beside the function (`[`interest`]`). That fails only in the whole-crate doc build, never in `cargo test`, so
run `pnpm check rustdoc` after adding a module whose name matches an existing item.

## The wake job

A wake reuses `run_turn` rather than growing a second turn loop: budget enforcement,
elision, crash-safe persistence, and cost metering must not differ between the user asking and
the agent noticing, and two loops guarantee they eventually will. Single-flight and
cancellation come from the same guards for the same reason.

**The order of the steps is the safety property.** Gates, then the deadline, then the digest
shaped from the rows WITHOUT draining them, then the thread, and only then the drain. Every
step that can decline does so before anything is spent, so a budget too small to say anything,
or a store that will not take a new thread, leaves the backlog exactly as it was. Draining
first and discovering the problem afterwards would lose signal with nothing to show for it.
That order is also why the job splits at exactly that seam (§ The two halves of a wake).

An empty digest means the wake stays quiet rather than opening a thread that reports silence.

**A wake opens a real conversation, with `ConversationOrigin::Notification`.** That token has
been in the schema since v1 with nothing writing it; this is its first writer. Three things
follow: the sweep links to the thread through the `EvidenceScope` plumbing without new
machinery, "why did it suggest this?" has an answer the user can read, and cost metering and
analytics work unchanged because they hang off a conversation.

**The thread is named for the PLACE, never with an authored sentence** (`thread_title`). A
folder name is data; a backend-written English title would be untranslated copy shipped into
the database, sitting in a list beside threads the user named themselves.

The sink is a plain `ChatEventSink`, the same unbounded channel the rail uses, drained onto the
same conversation-keyed transport (`agent/chat/stream.rs`). So a wake's thread reads live while
it is still being written, and the rail needs nothing wake-specific to show one.

## What drives it: three threads, and which lock each holds

⚠️ Say this once, explicitly, because "the lock" means two different things here and conflating
them produces either a stalled indexer or a raced thread.

- **The live loop** owns nothing of ours and holds nothing. `TauriEventSink::emit`
  (`events/index_mapping.rs`) calls `route()` SYNCHRONOUSLY on the caller's thread, and that
  caller is the live loop, itself a tokio task. So the tap builds a `FolderActivity`, calls
  `channel::send_rollup`, and returns.
- **The writer thread** (`writer.rs`) owns the `Inbox`, ONE long-lived write connection, and the
  timer. It never blocks on a turn.
- **The wake thread** (`runner.rs`) owns the turn and holds the per-conversation
  `ConversationLocks` guard across it.

Two things would stall the live loop if the tap owned the inbox instead. A **mutex around the
inbox** would be held across `run_turn`'s awaits, blocking every live batch for an LLM call and a
runtime worker with it. A **write connection per admit** would be worse: `open_write_connection`
applies the WAL pragmas and runs the FULL MIGRATION LADDER on every open, against a 5 s
`busy_timeout`, with the writer thread already holding a connection.

**The INBOX is released before the turn.** The writer thread prepares, hands off, and goes
straight back to servicing the channel. Blocking there would leave the bounded channel
unserviced for minutes and drop rollups wholesale, which is a different thing entirely from the
pathological-burst drop the bound sanctions.

**The CONVERSATION lock is held across the turn**, taken on the wake thread. A wake thread is a
real conversation the user can reply to, so skipping `ConversationLocks` would let a reply and
the wake's own turn run concurrently in one thread. **At most one wake is in flight**: the writer
thread keeps a flag and clears it on the `WakeFinished` control message.

⚠️ **A declined attempt backs off, and that is not a nicety.** The timer parks with
`recv_timeout`, and a deadline that has passed keeps having passed, so a park computed from the
inbox alone is zero-length and the thread spins a core flat. "Due and declined" is the ordinary
state for anybody without consent or an API key, not a rare one. `park_for` is floored by a `not_before` stamp that
every path through `try_wake` sets, and any control message clears it, so a gate opening is felt
at once rather than after the backoff.

⚠️ **That means TWO write connections to `main.db`**, the writer thread's and the turn's. WAL
makes it fine, and the writer thread's writes are single-row and never held across an await, so
the worst case is a brief wait on the busy timeout rather than a multi-second stall.

⚠️ **The wake thread is not a tokio task.** `run_turn` holds a rusqlite `Connection` across
awaits, which is why `ask_cmdr_send_message` spawns a dedicated `std::thread` with a
current-thread runtime. `runner.rs` copies that shape.

## The channel, and why it is a process-global

`channel.rs` holds one `std::sync::mpsc` pair behind a `OnceLock`, created by whichever side
reaches it first.

⚠️ **❌ Not managed Tauri state.** The indexer starts before the agent does (`lib.rs`, plus a
second start inside a `spawn`, so it is a race rather than a reliable ordering), and anything
registered in `agent::start` would miss launch replay, the busiest window the tap will ever see.
`restricted_paths` is the precedent for the shape.

Rollups arriving before `agent::start` sit in the buffer and are consumed once the thread comes
up, so some of launch replay survives. The buffer is deliberately NOT sized to catch all of it:
readiness cannot even be evaluated before the store is open (consent lives in `main.db`), so
anything older than that would be refused admission anyway.

**The bound is for ROLLUPS only** (`MAX_QUEUED_ROLLUPS`), so a pathological burst drops rather
than growing without limit, and the drops are counted and logged. The tap's payload is signal,
not correctness: the folder will change again. ⚠️ **Control messages never drop.** A settings
change re-arms the timer and re-prices queued rows, and the force-wake command is a message too;
dropping one is a bug rather than degraded signal. One channel carries both, and the bound
applies only to the rollup variant, so the loop can service messages and its timer in one
`recv_timeout`.

`FolderActivity` is where the agent-side vocabulary starts. It carries the volume id (for the
importance lookup), the folder, the four counters, `last_event_at`, and `observed_at` — **the
batch's own instant, ❌ never a window start**.

## Readiness is a cached atomic

`snapshot.rs` keeps one `AtomicU8`, refreshed by `refresh_readiness` on consent, on the Full Disk
Access decision, and on `configure_ai`. ⚠️ Reading `WakeReadiness` per batch would mean a SQLite
round trip on the live loop's path, since the consent bit lives in `main.db`. It fails CLOSED:
before the store is open the answer is `NeedsConsent`, so nothing is stored for a purpose the
user has not agreed to. M2 item 5's IPC reads the same snapshot.

`refresh_readiness` returns nothing on purpose: `readiness_snapshot()` is the one way to read the
value, so no caller can act on a copy a later refresh has already moved past. On a real move it
also emits `agent-wake-status`, because the two gates that render are set outside the main
window: the API key in the settings window, Full Disk Access outside the app entirely.

## The indicator's own event

`indicator.rs` carries one `tauri_specta::Event`, `agent-wake-status`, holding both facts the
status corner needs: the `WakePhase` (`Idle`, or `Thinking { conversation_id }`) and the
readiness gap. Two facts on one event because a subscriber reconciling two would render a state
neither meant, and because they answer the same question.

⚠️ **Separate from the turn stream** (`agent/chat/stream.rs`). That one carries a turn's
PROGRESS to whoever is showing that thread; this one carries a phase to a corner showing no
thread. Folded together, the corner would subscribe to every text delta of every rail send.

The phase lives in an `AtomicI64` holding the thinking thread's id, `0` for none (rowids are
always positive, so the sentinel cannot collide). `runner::spawn` sets it before anything slow
and clears it on EVERY exit — a stale `Thinking` would leave a spinner up forever and offer a
click into a thread a quiet wake has since deleted. `agent_wake_status()` reads it as the
frontend's one seed, for the wake already running when the window opened.

⚠️ **Cancel is not new machinery.** A wake registers its `CancellationToken` in
`agent::chat::cancel` under its conversation id, the same registry a rail send uses, so the
corner's stop button is `ask_cmdr_cancel` with the id this event carries. That registry sits in
`agent/chat/` rather than `commands/agent/chat.rs` precisely so the wake runner can reach it
without importing upward.

## Saying a wake staged something

`staged.rs` carries a second `tauri_specta::Event`, `agent-wake-staged`, holding the conversation and the proposal
count. A THIRD event rather than a field on either of the other two, because it is a one-shot fact: the turn stream
carries progress, the indicator carries state, and a subscriber reconnecting to a state event would re-raise the same
toast on every reload.

`runner::run` emits it from `WakeToolWatch::proposals()` (`watch.rs`), after the quiet-wake branch and BEFORE the
outcome line, whatever the turn then ended as. A cancel or a provider failure after the model already staged a group
leaves that group in the store, waiting; staying quiet about it would hide work the user is expected to review.
`announce_staged` no-ops at zero, so the caller does not have to remember.

⚠️ **The count comes from the tool CALLS, ❌ never from the streamed events.** Only `propose_rename_plan` emits a
`ProposalReady` — it opens a review dialog — while the whole `propose_suggestions` half (move, copy, trash, delete,
compress, extract) streams nothing at all. A count taken off the stream reads zero for most of what a wake stages, and
the toast never fires.

**The `askCmdr.wakeToast` gate is the FRONTEND's.** The event says what happened; whether a window makes a noise about
it is that window's business, and the settings store already updates live there. Gating the emit instead would mean a
user who turns the toast back on mid-wake silently misses the one it was about to raise. The rest of the toast:
`apps/desktop/src/lib/ask-cmdr/DETAILS.md` § The staged-proposal toast.

## Importance, on the writer thread

⚠️ Never in `route()`: `lookup` is SQLite behind a shared cache, and the live loop may do neither.
`ImportanceIndex::open` is already cheap (the connection is lazy and thread-local), so the cost
to avoid is the per-folder `lookup`. `importance.rs` caches it for 60 s behind a bounded map —
folders repeat heavily across batches, and a stale weight only misprices one wake. Opened with
`available_for(&volume_id)`, ❌ not `SignalSet::all()`, because a network volume degrades its
signal set. `WeightLookup` maps to `FolderImportance` variant for variant, never through
`score()` (the `CLAUDE.md` must-know says what that collapse would cost).

## The window is the APP's, not the crate's

A 60 s agent policy must not leak into `cmdr-index`, and `Inbox::admit` merges on exact
`(folder, window_start)` equality. So the crate reports its batch instant and
`FolderActivity::into_bundle` floors it to `WAKE_WINDOW`. Left unresolved, every ~1 s batch would
become its own inbox row.

## The two halves of a wake, and what each may spend

`prepare_wake` runs on the writer thread and is everything that can DECLINE plus the commit: the
gates, `due_at`, the digest shaped from `scored()` WITHOUT draining, the empty-render bail, and
`create_conversation`. Only once all of those pass does it `drain()` and `persist::clear`.

⚠️ Naively handing the drained rows over up front would throw that guarantee away: rows handed
over before the thread exists are lost on `NothingDue` and `Unavailable`, which are ordinary
paths, not crashes.

`run_prepared_wake` takes the digest, the conversation id, and the drained rows, and runs the
turn. In production the wake thread routes it through `ChatRuntime::wake` instead, which adds the
connection and the single-flight guard; `wake_turn_params` is the one place a wake's `TurnParams`
is composed, so the two cannot drift.

**The slot resolves BEFORE the thread is opened.** `resolve_agent_llm` and `resolve_prompt_budget`
(`agent/chat/session.rs`, shared with the rail) run on the writer thread, so a wake with nowhere
to think declines without leaving an empty conversation behind. The concrete `AgentLlm` and the
`AppHandleDispatcher` are built only once the conversation id exists: evidence scope is what stops
a claim in one thread being backed by facts delivered to another, and the wrong scope makes
`ImageFactsLedger` refuse every content-citing proposal.

**The sink** is a plain `UnboundedSender<AgentChatEvent>` the wake thread drains itself, through
`stream::forward_to_windows` — the same projection a rail send uses, so the two can't drift. What
the turn DECIDED is read off `WakeToolWatch` instead (§ Saying a wake staged something), never off
this stream.

**The thread is announced at both ends.** `Started` goes out as the turn begins, which is what
puts a wake's thread in the session list as it is created (nothing else says so: `SuggestionsChanged`
fires on proposals, and a wake that only looks makes none). `Discarded` goes out when a quiet wake
deletes it again, because anyone watching that conversation cannot learn it any other way — there
is nothing left to re-read.

**The envelope** is captured exactly as the rail captures it. With no main window (a
routine-launched app on macOS) `PaneStateStore` is absent and the pane fields come back empty,
which is the honest answer rather than a reason to skip the capture.

**Crash semantics, stated on purpose**: `persist::clear` runs with the drain, BEFORE the turn. A
process that dies mid-turn loses that digest rather than re-delivering it on restart.
Re-delivery would mean the user hears about the same activity twice, and the folder is still
there to be looked at again.

## A wake that finds nothing leaves no thread

The agent is allowed to look and decide none of it is worth a person's attention, and when it does
the user's session list must look exactly as it did. It says so by calling `nothing_to_suggest`
(`agent/tools/quiet.rs`), and this module acts on the call.

**Typed, never phrased.** Reading "nothing to report" out of the model's prose would classify
control flow by text, which `error-string-match` forbids and which breaks on the first copy edit
or non-English reply. `WakeToolWatch` (`watch.rs`) matches `ToolId::NothingToSuggest`.

**The watch is a dispatcher decorator, and only a wake builds one.** It forwards every call
unchanged and records that this one happened. That placement is the whole design: the tool itself
is `Access::Read` with an inert handler, because there is ONE `agent_tool_view()` and a tool that
deleted its own conversation would delete a USER's thread the moment a confused model reached for
it in the rail. `wake/tests/job.rs` pins both halves — a noop wake leaves no thread, and a rail
turn calling the same tool deletes nothing.

**The delete runs after the turn, under the single-flight guard.** `run_wake` (the single-thread
path the tests drive) calls `discard_quiet_thread` directly; production goes through
`ChatRuntime::discard_quiet_wake`, which takes the conversation's lock first — a wake thread is a
real conversation, so without it a reply the user is typing could be persisting into a thread
being deleted underneath them.

**What it spent survives.** `cost_meter` cascades on the conversation, so a plain delete would
erase the proactive agent's cost from the one place the user can see it. The rows fold onto the
reserved quiet-wakes thread first, in one transaction with the delete (`agent/store/DETAILS.md`
§ v8). A failure leaves the thread standing WITH its cost rather than gone without it.

**The `reason` is memory, not a log line.** ⚠️ Never log it verbatim: `cmdr.log` ships inside error
reports, including the auto-dispatched ones the user never previews, and `redact_line_salted` is
path-shaped, so a sentence naming which of the user's folders were boring travels intact. The
outcome line says THAT a wake was quiet; `WakeOutcome::Quiet` carries the reason for M3's memory.

## The turn a rejection earns

A wake is one of TWO background turns this module drives. The other is the follow-up a rejected sweep
earns: the memory ring already recorded what happened, with no model call, and this is where the agent
gets to turn a raw log line into something it will act on. An approval earns no turn: it is the agent
being right, and there is nothing to ask about.

**Decision: one turn per SWEEP, coalesced behind a TRAILING window.**
**Why**: "reject all" over an eight-group sweep produces eight `Rejected` outcomes, and a turn each
would be eight model calls, every one of them queued behind the same `ConversationLocks` guard, for
one decision the user experienced as one click. `FollowUpQueue` keys on the sweep, and each further
rejection pushes the window out again; a LEADING window would fire on the first group and ask about
a fraction of what was turned down. The burst is reported from where it STARTED (`decided_at >=
since`), so a rejection weeks later reports only itself rather than re-teaching the whole sweep.

**Decision: a closed gate DROPS the ask rather than parking it.**
**Why**: "why did you say no?" is only worth asking while the answer is still in the user's head. A
question that surfaces the week they finally set an API key reads as the app having sat on it. Both
ends of the window check `followup::may_ask` (`askCmdr.proactive` plus the three readiness gates),
because a gate can shut inside those five seconds.

**Decision: one code path in `runner.rs`, branching on `BackgroundTurn`.**
**Why**: the machinery around the turn is identical (the same envelope, memory, transport, cancel
registration, and corner spinner), and only the opener, the thread's provenance, and what a quiet
answer means differ. Three things a follow-up does NOT do:

- ❌ **Never discard its thread** on `nothing_to_suggest`. A wake thinks in a thread it opened for
  itself; a follow-up speaks in the user's, and a confused model reaching for that tool must not take
  it with them.
- ❌ **Never emit `Started`.** The thread has been in the session list all along, and claiming it was
  just created would put a duplicate row there.
- It shares `wake_in_flight`, so at most one background turn runs at a time whichever kind it is, and
  it reports through the same `WakeControl::WakeFinished` message.

Its opener is a `ProposalOutcomes` part, structured for exactly the reason a digest is: the row is
persisted, and rendered English would freeze one locale's copy in `main.db`.

## What the wake loop reports

Nothing else reports on it at all, so `runner::record_outcome` writes one counted log line per
outcome (`ran`, `quiet`, `nothing_due`, `not_ready`, `unavailable`, `cancelled`, `failed`, and the
`followup_*` twins for the other kind of turn) with the tier that triggered it, plus the matching
anonymous `agent_wake` analytics event. Without it the two
deferred tuning knobs can only be ranked by a support message, and "the agent is twitchy" arrives
as a complaint rather than a number. ❌ Every property is categorical: an outcome token, a tier
token, and coarse count buckets. Never a path, never a folder name, never what the digest said.

## The tap adapter

The producer, in two halves either side of the crate boundary.

**Crate side** (`crates/cmdr-index/src/indexing/watch/activity_monitor.rs`, canonical in that module's
`../DETAILS.md`): an `ActivityObserver` bundled with `ChurnObserver` into the `BatchObservers`
that `process_live_batch` takes by `&mut`, so a live batch cannot be processed without one. Two
source scanners walk every live-batch driver and fail when one doesn't build a real bundle. Both
guarantees are inherited on purpose, or the cold-start replay path silently taps nothing — the
failure `live.rs`'s own comment records having happened once already.

**App side** (`events/index_mapping.rs`): `cmdr-index` may never name the agent
(`index-crate-isolation`), so its rollups cross on the existing `IndexEvent` seam as
`FolderActivity`, and `route()` maps each one into this module's [`FolderActivity`] and calls
`send_rollup`. ⚠️ **❌ Never through managed state**: `route()` takes `app: Option<&AppHandle>`,
the completeness test calls `route(event, None)`, and nothing is registered before
`agent::start` — so a handler reaching for `app.state()` would drop every rollup in the test AND
through launch replay. `PathAccessDenied` → `restricted_paths::record_denial` is the precedent.

**What the counters mean, and the one decision behind them.** The crate reduces each event's
flags to a single kind with a documented priority: **renamed, then created, then removed, then
modified**. The flags are not one-hot (a coalesced event can carry created, removed, and renamed
at once), and a different order moves `intent_share()` materially, so it is a decision rather
than a detail. Three of the four counters would otherwise be unreachable at the tap's placement:
matched renames leave the batch in the inode pre-pass, storm removals are dropped for a subtree
rescan (the anchor contributes one `removed` instead), and directory creations sit in their own
vector. A directory's own event counts in its PARENT, matching this module's rule that a bundle
describes the folder a change happened IN.

**Every `WakeReadiness` gap is a state the indicator renders with an action; none of them is
silence.** A user who declined Full Disk Access and a user with a tidy Downloads folder otherwise
see the identical nothing, and only one of those is the feature working.

**A wake creates a conversation, so wake threads appear in the rail session list.** Ten wakes over
a quiet week is ten threads the user never started, interleaved with their own. The `origin`
column is already `notification` on every one, so filtering needs no schema work; the choice
between filtering the default view and giving them their own affordance is a product call nobody
has made.

