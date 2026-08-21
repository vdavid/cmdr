# Ask Cmdr can suggest things, but it never notices anything on its own

**Problem**: the whole proactive half of the agent is built and nothing drives it. The suggestion store, the executors,
the review dialog, the approval bridge, the status-corner indicator, and all ten locales ship. So does `agent/wake/`:
coalescing, interest scoring, compaction, the inbox with its deadlines, `agent_inbox` persistence, and the readiness
gates, under 54 tests. But `run_wake` and `Inbox::admit_if_permitted` have **no production caller anywhere in the tree**,
only `wake/tests/` (verified 2026-08-20). Nothing feeds the pipeline and nothing fires it, so "AI file organization",
which ships as an alpha feature, can never volunteer anything.

**Size**: about a day and a half for a working loop. Fully design-settled; needs no decision from David to start.

**Read first**: `apps/desktop/src-tauri/src/agent/wake/DETAILS.md` (the pipeline and the contract for both undriven
seams) and `apps/desktop/src-tauri/src/agent/suggested_ops/CLAUDE.md` for the guiding principle, which resolves most
design questions in this area.

## The work

1. **The tap adapter.** Half a day. Design fully settled.
   Map the crate-side per-batch rollup into an `EventBundle` and call `Inbox::admit_if_permitted`, as a second observer
   inside `process_live_batch`, placed after `detect_renames_by_inode` and the storm coalescing, crossing on the
   existing `IndexEvent` seam. Per-folder rollups, ❌ never per-file.
   ⚠️ Inherit both of `ChurnObserver`'s guarantees when you add the hook in
   `crates/cmdr-index/src/indexing/watch/event_loop/live.rs`: it is passed `&mut` so a live batch cannot be processed
   without one, and `churn_monitor/tests.rs:253` runs a source scanner over every live-batch driver that fails when one
   does not build a real observer. Skip those and the cold-start replay path silently taps nothing, a failure
   `live.rs`'s own comment records having happened once already.

2. **The scheduler.** Half a day to a day.
   Something owning a timer that fires at `Inbox::next_deadline`, resolving provider, model, and prompt budget the way
   the command layer already does for a user send, and supplying a `ChatEventSink` that drives the indicator rather than
   a rail. `run_wake` already declines cheaply on every gate, so the scheduler needs no gate logic of its own.
   Depends on item one for signal.

3. **Readiness states in the indicator.** A few hours. Depends on item two.
   A user who declined Full Disk Access and a user with a tidy Downloads folder currently see identical silence. Each
   `WakeReadiness` gap is a typed state and each needs a rendered state with an action; ❌ none of them is silence.

## Needs David before it ships (not before it starts)

**Should wake-created threads be filtered out of the Ask Cmdr session list by default, or get their own affordance?**
Every wake opens a conversation, so ten quiet wakes is ten threads the user never started, interleaved with theirs.
`origin` is already `notification` on every one, so filtering needs no schema work and either answer is cheap. Items one
through three can land first and default to showing them.

## Deliberately deferred

- **The two tuning knobs** (the unknown-importance weight at 0.35, and the hot/warm/cold tiers at five seconds, five
  minutes, and one hour) are acknowledged guesses. Only their ORDER is pinned as a contract by a test. Tune them after
  items one and two produce real behavior, ❌ not before: there is nothing to tune against today.
- **Per-rule approval for a long job's tail** is a policy question, not a task. See `open-decisions.md`.

## One unrelated gap in the same area

Changing the chat memory size records no thread-timeline event. The thread logs `ModelChanged` honestly through two
cooperating paths, but there is no equivalent for a budget change, so a user who shrinks their window mid-thread sees no
note explaining why the replies changed. Needs its own event plumbing on both sides, and the channel enums are
hand-mirrored in TypeScript. About half a day, unblocked, and independent of the wake loop.
