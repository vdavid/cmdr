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

## M1: the loop

The agent notices, decides, and either proposes or stays quiet. No new surfaces.

1. **The tap adapter.** Map the crate-side per-batch rollup into an `EventBundle` and call `Inbox::admit_if_permitted`,
   as a second observer inside `process_live_batch`, placed after `detect_renames_by_inode` and the storm coalescing,
   crossing on the existing `IndexEvent` seam with a backend-only `Destination`. Per-folder rollups, ❌ never per-file.
   ⚠️ Inherit both of `ChurnObserver`'s guarantees when you add the hook in
   `crates/cmdr-index/src/indexing/watch/event_loop/live.rs`: it is passed `&mut` so a live batch cannot be processed
   without one, and `churn_monitor/tests.rs:253` runs a source scanner over every live-batch driver that fails when one
   does not build a real observer. Extend that scanner to cover the tap. Skip either and the cold-start replay path
   silently taps nothing, a failure `live.rs`'s own comment records having happened once already.

   The rollup event carries `volume_id`, because importance resolves per volume through
   `ImportanceIndex::open(data_dir, volume_id, SignalSet)`. Cache lookups per folder with a short TTL: the same folders
   repeat across batches, and this sits on the path the counters exist to survive.

2. **Inbox ownership and persistence.** Nothing owns an `Inbox` today, and `wake/persist.rs` plus `store::inbox` are a
   third undriven seam. One owner in managed state: `load` plus `reconcile` at launch, `save_row` on admit, `clear` on
   drain.

3. **The scheduler.** A timer that fires at `Inbox::next_deadline` and calls `run_wake`, re-arming when an admit pulls a
   deadline earlier. It resolves provider, model, and prompt budget the way the command layer does for a user send (the
   budget is read fresh per send, so a wake reading a stale one would think with a different window than the rail), and
   supplies a `ChatEventSink`. `run_wake` already declines cheaply on every gate, so the scheduler needs no gate logic
   of its own beyond the new setting.

   ⚠️ **Not a plain tokio task.** `run_turn` holds a rusqlite `Connection` across awaits, which is why
   `ask_cmdr_send_message` spawns a dedicated `std::thread` with a current-thread runtime. Copy that shape.

   Extract `resolve_agent_llm`, `resolve_prompt_budget`, and `capture_envelope` out of `commands/agent/chat.rs` so both
   callers share one resolution. Two copies drift, and the budget one drifts silently.

4. **The dispatcher seam.** `AppHandleDispatcher::new(app, conversation_id)` scopes evidence to a thread and
   `LlmLogContext::agent_chat(conversation_id)` keys the LLM log the same way, but `run_wake` creates the conversation
   itself and takes the dispatcher as a parameter. Give `run_wake` a factory closure rather than a built dispatcher, so
   a wake's tool calls are scoped to the thread they happen in.

5. **`nothing_to_suggest`.** A wake that finds nothing must be able to say so. A typed tool call, ❌ never inferred from
   the model's text (`error-string-match` forbids classifying control flow by wording, and this is exactly that). The
   tool takes one short reason for the log and the agent's memory. A wake that calls it deletes its own conversation, so
   a quiet week leaves no threads behind; the wake indicator is the only trace it looked.

6. **Cold rides along, and sets no deadline of its own.** `COLD_DELAY`'s doc comment already says this and the code does
   the opposite: cold rows get `now + 1h` like any other and `due_at` fires on them, so a trickle in a barely-scored
   folder wakes the agent. Make the comment true: `deadline_for` returns `Option`, `InboxRow.deliver_by` becomes
   nullable (and so does its column), and a cold bundle waits for something else to wake the agent or ages out at
   `STALE_AFTER`.

7. **Three settings**, in the Ask Cmdr section:
   - `askCmdr.proactive`, defaulting on when consent and a working provider are both present. This is the middle tier
     between "no AI" and "AI that starts conversations", and it is the fourth gate the scheduler checks.
   - `askCmdr.wakeDelay`, a slider over the HOT tier: 5 s, 15 s, 30 s, 1 min, 2 min, 5 min, 15 min, 30 min, 1 h, 2 h.
     Warm derives as `min(hot × 60, 6h)`; cold rides along. The description shows both live values, so the user reads
     "reacts within 30 seconds; quieter folders within 30 minutes" rather than a bare number. ⚠️ The tier ORDER is a
     pinned contract; add a test walking every stop.
   - `askCmdr.wakeToast`, whether a staged proposal raises a toast. On by default.

8. **A dev-only force-wake command**, so verification doesn't mean waiting out a deadline.

**Verification**: a Rust integration test from a synthetic live batch through to `WakeOutcome::Ran`, the extended
scanner test, and an E2E driving the scripted fake LLM. End-to-end proof needs a real run on David's laptop.

## M2: the surfaces

Everything the user sees. All copy is a draft for David's review, translated into all ten locales.

1. **A wake indicator in the status corner**, left of the indexing hourglass (the corner owns placement; read
   `status-corner/DETAILS.md` first). A `bot` or `brain-circuit` Lucide icon while a wake runs, with a tooltip saying
   what it's doing. Clicking it opens the Ask Cmdr rail at that conversation. This is what the scheduler's
   `ChatEventSink` drives.

2. **The digest renders condensed.** A wake's first message is the rendered digest, which can be long. Mark it as a
   digest block so the rail collapses it to a few lines, with the agent's tool use and first reply below it.

3. **A distinct icon for wake-created threads** in the session list. `ConversationOrigin::Notification` is already
   written on every one; the list view needs to read it. Wake threads show by default, ❌ not filtered out: a thread the
   user can't find is a decision they can't audit.

4. **A toast when a wake stages a proposal**, gated by `askCmdr.wakeToast`. Nothing for a quiet wake. No OS
   notification.

5. **Readiness states in the indicator.** A user who declined Full Disk Access and a user with a tidy Downloads folder
   currently see identical silence. Each `WakeReadiness` gap is a typed state and each needs a rendered state with an
   action; ❌ none of them is silence.

## M3: memory

Without this the agent relearns nothing and repeats rejected suggestions. It is the difference between a useful
colleague and a nagging renamer.

1. **Location**: `<data-dir>/ai/memory/`, with `AGENTS.md` as the hub. The app data dir, ❌ not `~/.cmdr/`, for three
   reasons: it is app-managed state rather than user config, `app_data_dir()` is already the canonical per-OS path on
   all three platforms, and it inherits `CMDR_DATA_DIR` isolation for free, so dev, E2E, and every worktree get their
   own memory. Shared memory would mean an E2E run writing personal facts into David's real agent memory.

   `~/.cmdr/CMDR.md` stays where it is: it is user-authored, and a dotfile in home is where a hand-edited,
   dotfiles-repo-able config belongs. The two are fed together and labelled distinctly: **what the user tells the
   agent**, and **what the agent learned**. When a second platform arrives, check the OS config dir first and fall back
   to `~/.cmdr/`.

2. **Bug**: `read_cmdr_md()` calls `dirs::home_dir()` directly, so it ignores `CMDR_DATA_DIR`. The real
   `~/.cmdr/CMDR.md` currently bleeds into every E2E run and every worktree. Honor the override.

3. **`Access::Memory`**, a fourth variant with its own hand-authored allowlist, mirroring how `Propose` works. The
   guarantee becomes "the agent writes only into its memory folder", enforced structurally rather than written as a
   rule. `test_agent_tool_view_never_writes` widens to admit exactly this and nothing else.

4. **Two tools**, path-aware from day one so the second file costs nothing:
   - `memory_write(path, content)`: create or fully replace.
   - `memory_edit(path, old_string, new_string)`: exact match, refuses on a non-unique match.

   ❌ No read or list tool yet: `AGENTS.md` is auto-fed and it is the only file. Add both the moment there is a second
   one. Every schema rides in the cached prefix of every turn, including the interactive rail, so two tools cost less
   than four on calls that never touch memory.

5. **The jail**, one function applied by both tools, unit-tested: reject absolute paths, reject any `..`, resolve
   symlinks and re-check containment, allow `.md` only, cap a file at 8 KB and the directory at 64 KB. The cap is not
   housekeeping: memory rides in every turn's prefix, so an unbounded file quietly eats the context budget of every
   conversation.

6. **The system prompt** encourages capturing what matters, on request or on encountering something worth keeping, and
   pruning what has gone stale.

7. **Consent copy changes, and everyone re-accepts.** Memory is a sixth category of what leaves the machine, and the
   most personal one. The consent screen enumerates exactly what is sent; leaving memory out of it would make that
   screen false. Version the copy so the machinery re-prompts.

8. **Two settings-section controls**: "Open memory folder" (revealed in Cmdr itself) and "Forget everything", with a
   confirm.

9. **Verify** that crash and error report bundles do not sweep up the data dir. Memory contents must never ride out in a
   report.

## M4: the feedback loop

An approval or a rejection the agent never hears about is a lesson it cannot learn.

`NewSweep.conversation_id` already exists (its doc comment claims a background wake has none, which M1 makes obsolete),
so an outcome knows which thread to report to.

- **Always**: append a typed outcome event to the originating thread. No model call, no cost.
- **On rejection only**: one follow-up turn, so the agent can record why in memory and, if it wants, ask. Gated by
  `askCmdr.proactive`, at most once per group, so rejecting ten groups does not trigger ten turns. The question lands in
  the thread; the wake indicator surfaces it; the user answers whenever, or never.

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
