# Ask Cmdr can suggest things, but it never notices anything on its own

**Problem**: the whole proactive half of the agent is built and nothing drives it. The suggestion store, the executors,
the review dialog, the approval bridge, the status-corner indicator, and all ten locales ship. So does `agent/wake/`:
coalescing, interest scoring, compaction, the inbox with its deadlines, `agent_inbox` persistence, and the readiness
gates, under 54 tests. But `run_wake` and `Inbox::admit_if_permitted` have **no production caller anywhere in the
tree**, only `wake/tests/` (verified 2026-08-20). Nothing feeds the pipeline and nothing fires it, so "AI file
organization", which ships as an alpha feature, can never volunteer anything.

**Size**: four milestones, about four to six days. M1 alone makes the agent notice things; M3 is what makes it useful
rather than a nagging renamer.

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

## M1: the loop

**Intent**: the agent notices, decides, and either proposes or stays quiet. No new surfaces, so this milestone is
provable by tests alone and can't be blocked on copy review.

### 1. The tap adapter

Map the crate-side per-batch rollup into an `EventBundle` and call `Inbox::admit_if_permitted`, as a second observer
inside `process_live_batch`, placed after `detect_renames_by_inode` and the storm coalescing. Per-folder rollups, ❌
never per-file: `INGESTION_HARD_CAP` is 5,000,000 and a per-file message would put five million of them across the
boundary on exactly the path the counters exist to survive.

**The crate boundary is the constraint that shapes this.** `cmdr-index` may never name the agent
(`index-crate-isolation` fails the build), so the rollup crosses on the existing `IndexEvent` / `EventSink` seam and the
agent-side vocabulary starts on the app side. Concretely:

- A new observer type in `crates/cmdr-index/src/indexing/watch/`, shaped like `ChurnObserver`: constructed per volume,
  passed `&mut` into `process_live_batch`, folding each batch's paths into per-folder counters and emitting one
  `IndexEvent` per batch through the sink it holds.
- A new `IndexEvent::FolderActivity { volume_id, window_start, folders: Vec<FolderChangeRollup> }`, where the rollup
  carries the folder path and the four counts. `volume_id` is load-bearing: importance resolves per volume.
- A new `Destination` arm in `apps/desktop/src-tauri/src/events/index_mapping.rs`. `route()` is an exhaustive match, so
  the variant must be handled; `Destination::AnalyticsOnly` is the precedent for an event the frontend never sees.
- The app-side handler maps the rollup into `EventBundle` and admits it.

⚠️ **Inherit both of `ChurnObserver`'s guarantees.** It is passed `&mut` so a live batch cannot be processed without
one, and `churn_monitor/tests.rs:253` runs a source scanner over every live-batch driver that fails when one does not
build a real observer. Extend that scanner (or add its sibling) to cover the tap. Skip either and the cold-start replay
path silently taps nothing, a failure `live.rs`'s own comment records having happened once already. `process_live_batch`
has ten call sites (five production, five test); the two production drivers are `live.rs:246` and `replay.rs:461`.

**Importance lookup.** `admit_if_permitted` needs a `FolderImportance` per bundle, which resolves through
`ImportanceIndex::open(data_dir, volume_id, SignalSet)`, a per-volume SQLite read. ❌ Don't open an index per batch.
Hold one `ImportanceIndex` per volume for the process, and cache `lookup` results per folder behind a small bounded map
with a short TTL (60 s is generous; folders repeat heavily across batches and a stale weight only misprices one wake).
Map `WeightLookup` to `FolderImportance` variant for variant, ❌ never through `score()`, which collapses `Floored` and
`Unscored` into the same `0.0`.

**The coalescing window** is the tap's choice and is not defined anywhere today. Use 60 s. Rationale: a bundle should
cover "this burst of activity", the deadline tiers start at 5 s, and `merge_bundles` places a rollup by its own
`window_start`, so a window shorter than the fastest deadline would split one burst across bundles that each wake
separately.

### 2. Inbox ownership and persistence

Nothing owns an `Inbox` today, so `wake/persist.rs` and `store::inbox` are a third undriven seam. One owner in managed
state, holding the `Inbox` behind a mutex:

- Launch: `persist::load` then `Inbox::reconcile(launched_at)`, logging the `ReconcileReport` (it counts what the user
  was not told and why).
- Admit: `persist::save_row` for the touched row.
- Drain: `persist::clear`.

⚠️ The tap runs on the indexer's threads and the scheduler on its own; the mutex is the only thing serializing them.
Keep the critical sections free of I/O beyond the row write.

### 3. The scheduler

A timer that fires at `Inbox::next_deadline` and calls `run_wake`, re-arming whenever an admit pulls a deadline earlier.
`run_wake` already declines cheaply on every gate, so the scheduler needs no gate logic of its own beyond
`askCmdr.proactive`.

⚠️ **Not a plain tokio task.** `run_turn` holds a rusqlite `Connection` across awaits, which is why
`ask_cmdr_send_message` spawns a dedicated `std::thread` with a current-thread runtime (`chat.rs`, "the turn runs on a
dedicated thread"). Copy that shape, or the future won't compile and the workaround will be worse.

**Share the resolution with the rail.** `resolve_agent_llm`, `resolve_prompt_budget`, and `capture_envelope` are private
in `commands/agent/chat.rs`. Extract them so both callers use one path: the budget is read fresh per send, and a wake
reading a stale one would think with a different window than the rail, silently.

The envelope a wake captures describes an app nobody is looking at. Capture it the same way anyway (the pane state is
whatever it is), so the two paths can't diverge in what the model is told about the world.

### 4. The dispatcher seam

`AppHandleDispatcher::new(app, conversation_id)` scopes evidence to a thread, and `LlmLogContext::agent_chat(id)` keys
the LLM log the same way, but `run_wake` creates the conversation itself and takes a built dispatcher as a parameter.
Change `run_wake` to take a factory (`&dyn Fn(i64) -> Box<dyn ToolDispatcher>`, or a generic closure) so a wake's tool
calls are scoped to the thread they actually happen in. Same for the LLM: build it once the id is known, as the command
layer already does with `ResolvedAgentLlm::into_llm`.

**Why it matters beyond tidiness**: evidence scope is what stops a claim in one thread being backed by facts delivered
to another. A wake dispatching under the wrong scope would make `ImageFactsLedger` refuse every content-citing proposal,
or worse, accept one it shouldn't.

### 5. `nothing_to_suggest`

A wake that finds nothing must be able to say so. A typed tool call, ❌ never inferred from the model's wording
(`error-string-match` forbids classifying control flow by text, and this is exactly that).

- One argument: a short reason, for the log and for memory (M3).
- `Access::Read` (it mutates nothing), authored into the agent view like any other tool.
- When a turn ends having called it and staged nothing, the wake **deletes its own conversation**. `NewSweep`'s FK
  already tolerates a deleted thread (it NULLs `conversation_id` rather than cascading), so this is anticipated.
  Considered and rejected: archiving instead. Archived threads still accumulate, and "we looked and found nothing" fifty
  times is not a record worth keeping.
- ⚠️ Needs a store-level `delete_conversation`; only `ask_cmdr_archive_conversation` exists today.

### 6. Cold rides along

`COLD_DELAY`'s doc comment says a cold bundle never causes a wake of its own, and the code does the opposite: cold rows
get `now + 1h` like any other and `due_at` fires on them, so a trickle in a barely-scored folder spends a turn. Make the
comment true:

- `deadline_for` returns `Option<u64>`; `InboxRow.deliver_by` becomes `Option<u64>` and its column nullable (a migration
  on `agent_inbox`).
- `next_deadline` and `due_at` skip rows without one. `drain` still takes everything, which IS the riding-along.
- A cold row with no other traffic ages out at `STALE_AFTER`.

⚠️ `deliver_by` is the merge asymmetry's anchor (`row.deliver_by.min(...)`). With an `Option`, "no deadline" must lose
to any real deadline on merge, so a folder that goes from cold to warm gets the warm one. Encode that in the merge, and
test it: getting it backwards means a folder that warms up never wakes.

### 7. Three settings

In `apps/desktop/src/lib/settings/definitions/ai.ts`, section `['AI', 'Ask Cmdr']`, rendered by `AskCmdrSection.svelte`.

- **`askCmdr.proactive`** (boolean, default true). The middle tier between "no AI" and "AI that starts conversations".
  Default-on is only reachable when consent and a provider both exist, because the readiness gates already refuse
  otherwise; the setting is a fourth gate, not a replacement for them.
- **`askCmdr.wakeDelay`** (number, seconds, default 5). A slider over the HOT tier with stops at 5 s, 15 s, 30 s, 1 min,
  2 min, 5 min, 15 min, 30 min, 1 h, 2 h. Warm derives as `min(hot × 60, 6h)`; cold rides along. The description shows
  both live values, so the user reads "reacts within 30 seconds, quieter folders within 30 minutes".

  ⚠️ **The stops are non-linear and the slider is linear.** A 5-to-7200 track puts 5 s, 15 s, and 30 s inside one pixel.
  Two ways out: teach `SettingSlider` an index-mapped mode (thumb moves across evenly spaced stops, the stored value is
  the stop's value) which is reusable and correct; or fall back to `component: 'select'`, exactly as
  `askCmdr.chatMemorySize` does two rows above. Prefer the slider mode, take the select if the shared component fights
  back. ❌ Don't persist the stop INDEX: reordering the table would silently change every user's setting.

- **`askCmdr.wakeToast`** (boolean, default true). Whether a staged proposal raises a toast.

All three are read fresh where they're used, so none needs a `settings-applier` case (the pattern the two existing
`askCmdr.*` rows document).

### 8. A dev-only force-wake command

So verification doesn't mean waiting out a deadline. Gate it the way `test_mode` gates the scripted fake.

### M1 tests

TDD, red first (`tdd-red-green.md`), in this order:

- **Interest and delay under the new setting** (unit, `wake/tests/interest.rs`): every slider stop keeps hot < warm, and
  warm caps at 6 h. This is the pinned ORDER contract, now parameterized.
- **Cold sets no deadline** (unit, `wake/tests/inbox.rs`): a cold bundle alone leaves `next_deadline` empty and `due_at`
  false; a warm bundle arriving later gives the merged row the warm deadline; a drain still takes the cold one.
- **The tap scanner** (source-scanning test beside `churn_monitor/tests.rs:253`): every live-batch driver builds a real
  tap observer. Red first, by writing it before the observer exists.
- **Rollup → bundle mapping** (unit, app side): counters and window survive the crossing; `WeightLookup` maps variant
  for variant, and `Unscored` does not become zero.

Written after:

- **Live batch to `WakeOutcome::Ran`** (integration, Rust): a synthetic batch through `process_live_batch`, the tap, the
  inbox, and a `run_wake` against the fake LLM.
- **A noop wake leaves no thread** (integration): the fake calls `nothing_to_suggest`; assert the conversation is gone
  and the inbox drained.
- **Restart reconciliation** (integration): rows persisted, reloaded, settled, and the stale ones counted.

**Docs**: `agent/wake/CLAUDE.md` and `DETAILS.md` (the two seams are driven now; the tap's window and the importance
cache are new must-knows), `crates/cmdr-index/src/indexing/watch/CLAUDE.md` (the second observer), `agent/CLAUDE.md`
(the scheduler joins `start`), `settings/definitions` comments, `docs/architecture.md` if the subsystem map gains a
scheduler line.

**Checks**: `pnpm check rust` while iterating, then `pnpm check` at the milestone, plus `index-isolation`,
`error-string-match`, `bindings-fresh`, `ipc-enum-camelcase`, and `invariant-density` (this milestone adds ❌ rules).

## M2: the surfaces

**Intent**: make the agent's noticing visible and interruptible. Everything here meets human eyes, so all copy is a
draft for David's review, in English first, then all ten locales.

1. **A wake indicator in the status corner**, left of the indexing hourglass. ⚠️ Read `status-corner/DETAILS.md` first:
   the corner owns placement, members are plain inline boxes, and the hourglass stays last. A `bot` or `brain-circuit`
   Lucide icon while a wake runs, tooltip saying what it's doing, click opens the Ask Cmdr rail at that conversation
   (`openRail()` exists; opening AT a conversation is the new part). This is what the scheduler's `ChatEventSink`
   drives, bridged to the frontend the way `ask_cmdr_send_message` bridges its `Channel`.

2. **The digest renders condensed.** A wake's first message IS the rendered digest and can run long. Mark it as a digest
   block so the rail collapses it to a few lines, with the agent's tool use and first reply below.

3. **A distinct icon for wake-created threads** in the session list. `store::query.rs` already carries
   `origin: Option<ConversationOrigin>` on the row; the wire view and `ask_cmdr_list_conversations` need the field.

4. **A toast when a wake stages a proposal**, via `$lib/ui/toast` (`addToast`, with its own `toastGroup`), gated by
   `askCmdr.wakeToast`. Nothing for a quiet wake.

5. **Readiness states in the indicator.** A user who declined Full Disk Access and a user with a tidy Downloads folder
   currently see identical silence. Each `WakeReadiness` gap is a typed state and each needs a rendered state with an
   action; ❌ none of them is silence.

**Tests**: component tests for the indicator's states and the thread icon; an a11y test per new component (the
`*.a11y.test.ts` convention is enforced by `a11y-coverage`); an E2E driving the scripted fake through a forced wake to a
visible toast and badge.

**Checks**: `pnpm check svelte`, plus `i18n-parity`, `i18n-coverage`, `message-keys-fresh`, `a11y-coverage`,
`a11y-contrast`, `ui-primitive-coverage`, `shipped-locales-fresh`.

## M3: memory

**Intent**: without this the agent relearns nothing and re-proposes what was already rejected. It is the difference
between a colleague and a nag.

1. **Location**: `<data-dir>/ai/memory/`, with `AGENTS.md` as the hub. The app data dir, ❌ not `~/.cmdr/`: it is
   app-managed state rather than user config, `app_data_dir()` is already the canonical per-OS path on all three
   platforms, and it inherits `CMDR_DATA_DIR` isolation for free, so dev, E2E, and every worktree get their own memory.
   Shared memory would mean an E2E run writing personal facts into David's real agent memory.

   `~/.cmdr/CMDR.md` stays put: it is user-authored, and a dotfile in home is where a hand-edited, dotfiles-repo-able
   config belongs. Both are fed and labelled distinctly in the prompt: **what the user tells the agent**, and **what the
   agent learned**. When a second platform arrives, check the OS config dir first and fall back to `~/.cmdr/`.

2. **Bug**: `read_cmdr_md()` (`chat/runtime/mod.rs:231`) calls `dirs::home_dir()` directly, so it ignores
   `CMDR_DATA_DIR`. The real `~/.cmdr/CMDR.md` currently bleeds into every E2E run and every worktree. Honor the
   override. Fix this first, TDD, since it's a bug and it makes every later memory test deterministic.

3. **`Access::Memory`**, a fourth variant beside `Read`, `Propose`, and `Write`, with its own hand-authored allowlist
   mirroring `EXPECTED_PROPOSE_TOOL_NAMES`. `test_agent_tool_view_never_writes` widens to admit exactly this and nothing
   else, so the guarantee becomes "the agent writes only into its memory folder", structural rather than a rule in a
   doc. ⚠️ This is a deliberate widening of the app's central agent-safety invariant; the allowlist is what keeps it
   from being acquired as a side effect of editing a registry line.

4. **Two tools**, path-aware from day one so the second file costs nothing:
   - `memory_write(path, content)`: create or fully replace.
   - `memory_edit(path, old_string, new_string)`: exact match, refuses a non-unique match.

   ❌ No read or list tool yet: `AGENTS.md` is auto-fed and it is the only file. Add both the moment there is a second
   one. Every schema rides in the cached prefix of every turn, including the interactive rail, so two tools cost less
   than four on calls that never touch memory.

5. **The jail**, one function both tools call, unit-tested: reject absolute paths, reject any `..`, resolve symlinks and
   re-check containment, allow `.md` only, cap a file at 8 KB and the directory at 64 KB. The cap is not housekeeping:
   memory rides in every turn's prefix, so an unbounded file quietly eats the context budget of every conversation,
   including the rail's.

6. **The system prompt** encourages capturing what matters, on request or on meeting something worth keeping, and
   pruning what has gone stale.

7. **Consent copy changes, and everyone re-accepts.** Memory is a sixth category of what leaves the machine and the most
   personal one; the consent screen enumerates exactly what is sent, so omitting it would make that screen false.
   Version the copy so `has_current_consent` re-prompts.

8. **Two controls** in the Ask Cmdr section: "Open memory folder" (revealed in Cmdr itself) and "Forget everything",
   with a confirm.

9. **Verify** crash and error report bundles don't sweep up the data dir. Memory must never ride out in a report.

**Tests**: TDD the jail (every escape attempt, red first) and the `CMDR_DATA_DIR` fix. After: tool round-trips, the size
caps, prompt assembly carrying both files labelled, and a consent test proving the new copy re-prompts.

**Checks**: `pnpm check rust svelte`, plus `i18n-*` for the consent copy and the two new controls.

## M4: the feedback loop

**Intent**: an approval or a rejection the agent never hears about is a lesson it can't learn.

`NewSweep.conversation_id` already exists (its doc comment claims a background wake has none, which M1 makes obsolete),
so an outcome knows which thread to report to.

- **Always**: append a typed outcome event to the originating thread. No model call, no cost. Follow `ModelChanged`'s
  shape: a `ConversationEvent` variant, persisted through `append_event`, mirrored in the TypeScript channel enum (⚠️
  hand-mirrored, so `ipc-enum-camelcase` and `bindings-fresh` both apply).
- **On rejection only**: one follow-up turn, so the agent can record why in memory and, if it wants, ask. Gated by
  `askCmdr.proactive`, at most once per group, so rejecting ten groups doesn't trigger ten turns. The question lands in
  the thread; the indicator surfaces it; the user answers whenever, or never.

**Tests**: TDD the once-per-group guard (it's the cost-control invariant). After: the outcome event round-trips, and a
rejection with `askCmdr.proactive` off runs no turn.

## Execution order

Sequential. M1 → M2 → M3 → M4, committing at each milestone and at each numbered step inside it where the tree is green.

Two exceptions worth taking:

- **M3.2 (the `CMDR_DATA_DIR` bug) can land first**, before M1. It's independent, it's a real bug, and it makes every
  later test deterministic.
- **M2's copy drafting can happen during M1**, since David reviews asynchronously and translation is the long pole.

❌ Don't parallelize M1's own steps across agents. The tap, the inbox owner, and the scheduler meet at one mutex and one
event variant, and three agents converging on `process_live_batch`'s signature is how you get a merge that compiles and
taps nothing.

## Open for David

1. **The wake-delay slider's shape**: index-mapped slider (needs a small `SettingSlider` addition, reusable) or a select
   like `askCmdr.chatMemorySize` (zero new component work, less pretty). Recommending the slider.
2. **The follow-up turn on rejection** costs a model call per rejected group. Confirm that's wanted before M4.

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
