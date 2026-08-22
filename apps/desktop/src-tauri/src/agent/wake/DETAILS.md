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

## The digest budget

Enforced against the REAL rendered string, not a sum of per-line estimates: `div_ceil` per line does not add up to the
cost of the whole. It reuses `chat::budget::estimate_tokens_str` so the digest and the prompt cannot drift apart about
what a token costs.

The budget goes to the highest-interest folders first; the rest roll up by shared parent, or into one line at the
common ancestor when there are too many parents for that to read as a summary. Every folder is either a line or inside
a rollup, and a test sums both sides to prove nothing goes uncounted.

At an impossible budget the digest is EMPTY rather than over: an overrun would push the rest of the turn out of the
window, which is the failure that once cost a rename turn the evidence it was reasoning from.

## The inbox, and what a restart does

A merge can only pull a deadline earlier and can only raise the stored interest. The asymmetry is a starvation guard,
and it also stops a later, duller contribution from demoting what an earlier burst established.

**A cold row has NO deadline** (`deliver_by: Option<u64>`, nullable in the table since migration v7). That is what
"rides along" means mechanically: the row waits, any wake drains it, and nothing about it can cause a wake. Given a
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
identical nothing, and only one of those is the feature working. Every state is a value the indicator renders with an
action.

**Without consent the pipeline stores nothing** (`admits_to_inbox`). Admitting rows means keeping a record of what the
user has been doing with their files for a purpose they have not agreed to, and it would mean consenting on a Tuesday
hands somebody a backlog of everything they did since installing. With consent but no key, signal accumulates: the gap
is one the user can close and the backlog is theirs, bounded by the staleness horizon.

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

`run_wake` reuses `run_turn` rather than growing a second turn loop: budget enforcement,
elision, crash-safe persistence, and cost metering must not differ between the user asking and
the agent noticing, and two loops guarantee they eventually will. Single-flight and
cancellation come from the same guards for the same reason.

**The order of the steps is the safety property.** Gates, then the deadline, then the digest
shaped from the rows WITHOUT draining them, then the thread, and only then the drain. Every
step that can decline does so before anything is spent, so a budget too small to say anything,
or a store that will not take a new thread, leaves the backlog exactly as it was. Draining
first and discovering the problem afterwards would lose signal with nothing to show for it.

An empty digest means the wake stays quiet rather than opening a thread that reports silence.

**A wake opens a real conversation, with `ConversationOrigin::Notification`.** That token has
been in the schema since v1 with nothing writing it; this is its first writer. Three things
follow: the sweep links to the thread through the `EvidenceScope` plumbing without new
machinery, "why did it suggest this?" has an answer the user can read, and cost metering and
analytics work unchanged because they hang off a conversation.

**The thread is named for the PLACE, never with an authored sentence** (`thread_title`). A
folder name is data; a backend-written English title would be untranslated copy shipped into
the database, sitting in a list beside threads the user named themselves.

The sink is a plain `ChatEventSink`, the same unbounded channel the rail uses. Nobody is
watching a rail during a wake, so the caller supplies one that drives the indicator instead.

## The two seams nothing drives yet

The pipeline is whole and tested from `EventBundle` in to a sweep out, and **nothing in the
app calls either end**: `Inbox::admit_if_permitted` and `run_wake` have no production caller,
only tests. Two adapters close that, and each has constraints worth stating before somebody
builds it.

**The tap adapter** maps the crate-side per-batch rollup into `EventBundle` and calls
`admit_if_permitted`. It belongs beside the observer described above, and it follows
`ChurnObserver`'s shape deliberately: that type is passed by `&mut` so a live batch cannot be
processed without one, and a `churn_monitor/tests.rs` scanner walks every live-batch driver
and fails when one of them doesn't build a real observer. Inherit both, or the cold-start
replay path silently taps nothing, which is the failure `live.rs`'s own comment records
having already happened once. The mapping is the whole adapter: `cmdr-index` may never name
the agent (`index-crate-isolation`), so the rollup crosses on the existing `IndexEvent` seam
and the agent-side vocabulary starts here.

**The scheduler** owns a timer that fires at `Inbox::next_deadline` and calls `run_wake`. It
has to resolve provider, model, and prompt budget the way the command layer does for a user
send (the budget is read fresh per send, so a wake reading a stale one would think with a
different window than the rail), and it supplies the `ChatEventSink`: a wake has no rail
watching it, so the sink drives the indicator instead. `run_wake` already declines cheaply on
every gate, so the scheduler may call it whenever a deadline passes and needs no gate logic of
its own.

**Every `WakeReadiness` gap is a state the indicator renders with an action; none of them is
silence.** A user who declined Full Disk Access and a user with a tidy Downloads folder
otherwise see the identical nothing, and only one of those is the feature working.

**A wake creates a conversation, so wake threads appear in the rail session list.** Ten wakes
over a quiet week is ten threads the user never started, interleaved with their own. The
`origin` column is already `notification` on every one, so filtering needs no schema work; the
choice between filtering the default view and giving them their own affordance is a product
call nobody has made.
