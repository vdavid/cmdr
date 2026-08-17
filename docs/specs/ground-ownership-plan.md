# Finish the claim: one ownership authority for a volume's index

Grow `cover::live::Claim` into the mechanism that answers "who may walk this ground right now", and narrow the parallel
mechanisms that answer the same question in their own vocabulary down to the jobs only they can do.

## Why

`crates/cmdr-index` is the codebase's top bug source: 27 of 125 `fix(...)` commits since 2026-06-01, and 372 `❌`
never-do-X rules in its docs (3.06 per 1,000 source lines against 1.24 for `src-tauri` and 0.81 for the frontend, as the
`invariant-density` check now reports). Those two facts are the same fact.

Read the fixes as a group and they are one bug wearing different clothes:

- "a rescan can no longer blank the index underneath a search walk that is still writing to it"
- "a cover walk that ends during a rescan's shutdown window no longer holds its ground hostage for the session"
- "a remembered rescan can't be lost to a walk that ends mid-request"
- "a folder the user opens ahead of the walker stays the walk's to cover, instead of a second unthrottled indexer racing
  it"

Every one is a handoff between two concurrent actors contending for the same ground, arbitrated by whichever of several
partially-overlapping mechanisms the two happen to share.

`phased-indexing-plan.md` § M0 is the proof. Its first milestone was four shipped bug fixes, and they are precisely this
class. That plan reached this thesis independently and executed it by hand as four point fixes, instead of structurally.

## Two rejected designs, recorded so nobody revives them

**A `GroundBroker` module** (new module, priority queue, tickets, re-entrant leases, nine mechanisms migrated onto it).
Killed in review for three reasons, each verified against the code:

1. **Its headline milestone was impossible.** `IndexPhase::Running(Box<IndexManager>)` (`lifecycle/state.rs:105`) is a
   `Box`, and `with_running_manager` (`lifecycle/state.rs:321-328`) holds the registry lock by construction.
   `mem::replace` is the only route to an owned `&mut`. **A lease is a logical token; it does not give the borrow
   checker permission.** That is a custody refactor (now M5), not an arbitration one.
2. **It proposed a design the code refuses by name.** `lifecycle/cover/mod.rs:338-342`: "❌ not hung off `Claim`'s
   `Drop`: … a scan spawning out of a destructor is a side effect nobody reading `Claim` would expect."
3. **Its performance case cited a refuted number.** The 3.0 s / 2,503-roots figure sits in
   `cover-no-ground-block-2026-08-15.md` § "What this note does NOT settle", and `branch-set-cost-2026-08-15.md` § "What
   did NOT reproduce" refutes the attribution.

**A claim-derived `working` flag** (making `phases_have_work()` fall out of the claim table). Killed in the second
review: the phase machine takes an Additive claim per frontier group and holds **nothing between groups**, while
`working` is deliberately true across those gaps. The stitch produces 50 to 150 gaps per phase
(`lifecycle/manager/start.rs:337-339`), so a claim-derived `working` would be false in exactly the windows
`start.rs:340` exists to close. **`phases_have_work` stays an explicit, separate question.**

## The decisive observation

`Claim` is already RAII, already partial-grant, already per-volume, already tested, and its `overlaps`
(`lifecycle/cover/live.rs:139-141`) already treats an ancestor as covering. `lifecycle/manager/start.rs:349-352` and
`lifecycle/network_scan.rs:204` **already exploit that** by probing with the volume root. So the volume-level question
and the subtree-level question are already one namespace, written twice.

**The ideal end state is one arbitration mechanism, and the one that should survive is the one that is already
correct.**

**Honest sizing:** a scan entry asks three questions today (`mgr.scanning`, `ground_being_walked`, `phases_have_work`).
After this plan it asks two (one claim, plus `phases_have_work`). That is a real win, and it is smaller than "collapse
the two single-flight questions into one". Do not oversell it.

## The terrain

An implementing agent cannot check off what it changed without this table. It is the plan's spine.

### The actors that contend for ground

| #   | Actor                        | Entry point                                                 | Scope                                                                                            | Trigger          |
| --- | ---------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | ---------------- |
| 1   | First-index phase machine    | `state::start_pending_phases`                               | Whole volume, as N `cover::start` calls over frontier groups (`lifecycle/phases/mod.rs:537-556`) | Background       |
| 2   | Search-driven cover walk     | `Index::cover` (`indexing/handle/mod.rs:539`)               | Set of frontier-root subtrees                                                                    | User             |
| 3   | Manual rescan / turn-on      | `state::force_scan` (`lifecycle/state/scan_control.rs:128`) | Whole volume                                                                                     | User             |
| 4   | Automatic registry rescan    | `manager::perform_registry_rescan`                          | Whole volume                                                                                     | Background       |
| 5   | Completion-retry resume      | `completion_retry::nudge` → `resume_the_phases`             | Whole volume, phases only                                                                        | Background timer |
| 6   | Local full scan              | `IndexManager::start_scan` (executor for 3 and 4)           | Whole volume                                                                                     | —                |
| 7   | Journal replay               | `lifecycle/manager/start.rs:22`                             | Whole volume, **does not walk but WRITES** (holds an `Exclusive` claim as of M2)                 | Launch           |
| 8   | Network trait scan           | `lifecycle/network_scan.rs:187`                             | Whole volume                                                                                     | User + reconnect |
| 9   | Live event loop              | `watch/event_loop/live.rs`                                  | Per-event, per-dir                                                                               | Continuous       |
| 10  | Deep `MustScanSubDirs` drain | `reconcile/reconciler/rescan/`                              | Subtree (anchor)                                                                                 | Background       |
| 11  | Per-navigation verifier      | `state::trigger_verification`                               | Single dir, may recurse                                                                          | User             |

### The mechanisms, and what happens to each

| Mechanism                                  | Location                                                            | Fate                                                                                 |
| ------------------------------------------ | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `cover::live::IN_FLIGHT` + `Claim`         | `lifecycle/cover/live.rs:33`                                        | **Survives; absorbs the scan-entry guards and the `OWED` set**                       |
| `mgr.scanning: Arc<AtomicBool>`            | `lifecycle/manager.rs:78`                                           | **Keeps being SET by scans.** Loses only its two guard readers. Narrowed and renamed |
| `rescan_request::OWED`                     | `lifecycle/rescan_request.rs:81`                                    | Absorbed as a per-volume waiter. The module's error/outcome vocabulary relocates     |
| `mgr.pending_phases` + `working`/`walking` | `lifecycle/manager/phased.rs:35`, `lifecycle/phases/mod.rs:172,181` | **Unchanged.** `phases_have_work` stays a separate question                          |
| `INDEX_REGISTRY` + `IndexPhase`            | `lifecycle/state.rs:196`                                            | Keeps custody. Its `ShuttingDown` exclusion is M5's problem, not this plan's         |
| `watch::branches::WATCHES`                 | `watch/branches.rs:81`                                              | **Stays.** Already correct and measured                                              |
| Reconciler rescan scheduler                | `reconcile/reconciler/rescan/mod.rs:46-56`                          | Optional, M6                                                                         |
| `verifier::VERIFIER_STATE`                 | `reconcile/verifier.rs:29`                                          | Out of scope, already RAII                                                           |
| Cancellation-token parentage               | `signals.cancel.child_token()`                                      | Out of scope, needed by M7                                                           |

### Explicitly not in scope

- **❌ `Freshness`.** A total transition table driving a badge colour. Nothing consults it to decide whether it may
  walk. Folding a reporting state machine into arbitration would recreate the conflation that produced the rules.
- **❌ Policy gates.** Master switch, per-drive `user_enabled`/`user_disabled`, `PHASED_FIRST_INDEX`, the FDA gate. A
  search walk is deliberately carved out of all of them (`lifecycle/DETAILS.md:655`, Decision 13).
- **❌ `BranchWatch`.** The one mechanism already in the right shape, and the only one with a `Buffered` outcome. The
  live event loop cannot block, be refused, or queue.
- **❌ `listed_epoch` and the `EXCLUSION_POLICY_KEY` stamp.** The durable half of arbitration.
- **❌ Manager custody.** See M5.

## The constraints

### 1. Partial grant

A walk takes the non-overlapping subset and reports the rest as deferred (`lifecycle/cover/live.rs:62-91`).
All-or-nothing is a straight regression. `Claim` already does this; do not lose it.

### 2. Mode, and the two modes are not enough for every holder

`cover_context_for` (`lifecycle/state.rs:256`) refuses a cover walk **only** on `mgr.scanning`, never on
`phases_have_work`. A search walk therefore runs concurrently with the phase machine, arbitrated per-root by `Claim`
alone. That is Decision 13, guarded by
`cover::cold_drive_tests::switches::a_search_walks_a_drive_with_the_master_switch_off`.

So a claim needs `Exclusive` (a truncating scan; excludes everything) versus `Additive` (a cover walk; overlap-checked).

**Known gap, and it is deliberate.** Two holders want "block truncating scans and search walks, but not phase walks",
and neither mode expresses it:

- The **phase machine**, if it ever wanted a volume-wide hold. `Exclusive` would refuse its own per-group walks
  (`lifecycle/phases/CLAUDE.md:25`: "❌ Never `mgr.scanning`: `cover_context_for` returns `None` under it, so our own
  walks fail"), and `Additive` at the volume root conflicts with every subtree claim because `overlaps` counts an
  ancestor. **Resolution: the machine takes no volume-wide claim. `phases_have_work` stays a separate question.**
- **Journal replay** (see constraint 5).

❌ Do not solve this with holder identity or re-entrancy. That is the broker design, and it was killed for good reasons.

### 3. Release marks under the lock; the waiter runs on a runtime worker

**The rule is right; the earlier justification was wrong.** There is no ABBA today, because there is no registry→claim
nesting: `off_the_registry` (`lifecycle/state/scan_control.rs:223-238`) drops its guard before calling `work`, and
`Index::coverage` (`indexing/handle/mod.rs:492-500`) is sequential, not nested. The accurate statement is that putting
the registry lock under the claim lock would create the **first edge of a cycle that does not exist yet**.

The sharper constraint is a runtime one, and `lifecycle/rescan_request.rs:118-123` already states it: the releasing
thread is the cover walk's own `std::thread`, **not a runtime worker**, and `force_scan`'s prelude does
`tokio::task::block_in_place` twice (`lifecycle/manager/start.rs:405`, `:463`). Running the waiter inline would park the
walk thread on the registry lock plus a writer flush on its way out.

**The rule, in full:** release marks the waiter under the lock; a **lock-free peek** decides whether there is one; if
there is, it is **spawned onto the runtime**, never run inline, and ❌ never from `Drop`. This is what `run_if_owed`
does today. Preserve it.

### 4. Walkers take `Partial` and never wait

The phase machine always makes forward progress: it takes what it can and defers the rest. If walkers could queue behind
user walks, a machine under continuous search never converges. Only one-shot user intents (the manual rescan) become
waiters.

### 5. Journal replay needs an explicit decision, and it is not ground ownership

`start_replay` sets `scanning.store(true)` (`lifecycle/manager/start.rs:67`), commented only as "Suppress verifier until
replay completes". Its **undocumented second effect** is that `cover_context_for` (`lifecycle/state.rs:259`) returns
`None` for replay's whole duration, so `Index::cover` refuses every search walk on the boot disk while replay runs. It
is cleared at `watch/event_loop/replay.rs:409` plus the safety net at `lifecycle/manager/start.rs:121`.

Replay does not walk (actor 7). If M2 moves `cover_context_for` off `mgr.scanning` **without replacing this**, a search
cover walk runs while replay writes rows through the reconciler, and both allocate fresh ids for the same names: the
`INSERT OR IGNORE` collision the claim module exists to prevent (`lifecycle/cover/live.rs:6-11`).

**Because M2 keeps `mgr.scanning` set by scans and keeps `cover_context_for` reading it (see M2), replay's suppression
of new cover walks survives untouched. Do not "tidy" `cover_context_for` onto the claim table.**

⚠️ **This constraint saw half the coupling, and M2 found the other half.** The SCAN ENTRIES read the same flag, so it
was also the only thing refusing a truncating rescan during replay — and a user's "Rescan now" on a replaying boot disk
reaches `start_scan`. Replay therefore DOES take a claim as of M2: it writes the whole volume through the reconciler,
which is exactly the collision the table exists to prevent. What it doesn't do is WALK, which is why
`cover_context_for`'s read stays. See the M2 note under § Milestones.

Related, for whoever touches replay next: the fallback signals at `watch/event_loop/replay.rs:159`, `:246`, `:286` fire
`perform_registry_rescan` while replay's `scanning` is still true.

## Performance

- **`live.rs:71`'s `claimed.push` sits inside the loop over `frontier`, so the scan at `:69` grows as it goes**: O(n²)
  within a single call. A 2,503-root frontier is roughly 3.1M `overlaps` calls on the caller's thread before the walk
  starts.
- **Do not lead with this as the explanation for the missing 3.0 s.** These are plain string comparisons
  (`live.rs:139-141`), plausibly tens of milliseconds, whereas the 490.8 ms branch-set figure came from
  `finish_covering` doing allocating ancestor scans per entry. It is worth fixing on its own merits; whether it is the
  missing cost is a question for the bench, not the plan.
- The fix is the shape that worked for the branch set: path-keyed `BTreeMap` with prefix-range queries
  (`watch/branches.rs:614-633`). ❌ Never a `Vec` scan.
- **Measure, do not assert.** M1 lands the bench before anything migrates. Numbers to `docs/notes/` with method and
  date.
- ❌ Do not repeat the 3.0 s figure as established. It is refuted.

## Milestones

M0 is independent. M1 → M2 → M4 are sequential. M5, M6, M7 are independent of each other. **M3 is a rename and turned
out not to gate M4**, so it is still open with everything else shipped around it.

**M0, M1, M2, and M4 are shipped**, M4 together with product call 4 (a "Rescan now" during a running scan now QUEUES).
What executing M4 changed about the rest of this plan:

- **The waiter is a bit on the claim table's own per-volume entry**, not a set beside it, because "may that rescan
  start" is ONE question — is anything owed, and is the ground free — and two structures answering half each can
  disagree in the window between them, with a truncating scan riding on the answer. So constraint 3's "release marks the
  waiter under the lock, a lock-free peek decides" became something stronger and simpler: `a_rescan_can_start` asks the
  table once, and it can only answer yes when the ground is actually free. `run_if_owed` keeps its peek-then-spawn
  shape, and is now safe to call from anywhere.
- **A volume's claim-table entry now outlives its claims.** The request is recorded BEFORE the scan attempt, which is
  routinely a moment when nobody holds anything, so pruning on `roots.is_empty()` drops it. `is_idle()` is the test.
- **The plan's release-site list was right about WHICH three, and wrong that the fire belongs at the release.** A scan's
  completion task keeps writing after its walk thread joins — buffered events through the reconciler, then
  `scan_completed_at` — so a rescan fired where the ground goes back can truncate and then take the old scan's
  completion marker onto its own half-built index, the state that makes the next launch skip the healing rescan. **The
  ground comes back when nothing is WALKING; the rescan it owes runs when nothing is WRITING.** For a cover walk and for
  replay those are the same moment; for both scans they are not. That hazard existed before this milestone (a walk
  ending inside the post-scan window could already fire one); queueing behind scans is what would have made it routine.
- **Product call 4 is shipped**, so `force_scan` now answers `DeferredUntilScanEnds` where it used to report `Started`
  and forget the request. `ScanStartError` gained `GroundBeingRewritten` for the `Exclusive` conflict, leaving
  `AlreadyScanning` to mean only what `phases_have_work` means. **The silent loss at `state/scan_control.rs:145` is
  therefore half fixed**: a claim conflict is now kept, and a first-index machine with work still answers `Started` —
  honestly, since it IS walking the volume whole, in pieces.
- **Nothing bounds the queue but the one-bit-per-volume shape**, which is enough: five clicks describe the same work.
- **Invariant density: 383 → 387 for `crates/cmdr-index` (+4 net).** No mechanism was deleted (the `OWED` set MOVED into
  the claim table), so nothing became obsolete to delete, and the new hazards — the entry that outlives its claims, and
  the write-window the fire has to clear — needed saying. ⚠️ **So after four of the five milestones the count has still
  not fallen, and the thesis that this refactor would reduce it is wrong as stated.** What did improve is enforcement:
  every rule this milestone added names the test that catches it.

What executing M2 changed about the rest of this plan:

- **The plan was WRONG that the entries' `mgr.scanning` read was only about scans.** `start_replay` sets the same flag
  (`manager/start.rs:67`), so that read was ALSO the only thing refusing a truncating rescan during journal replay —
  reachable today, since a user's "Rescan now" on a replaying boot disk goes `force_scan` → `cover_or_scan` →
  `start_scan` (replay implies a completed scan, so `first_index_is_the_machines` is false). Constraint 5 spotted the
  `cover_context_for` half of the coupling and missed this one. Dropping the read without replacing it would have let a
  truncate land under a live replay: the `INSERT OR IGNORE` hazard, principle #1. **Resolution: `start_replay` takes an
  `Exclusive` claim**, dropped in `run_replay_event_loop` beside the `scanning.store(false)` that ends the replay phase.
  Replay does write the whole volume through the reconciler, so the claim is honest — what it does NOT do is walk, which
  is why `cover_context_for` still reads `mgr.scanning` exactly as constraint 5 says. ⚠️ Whoever revisits this: ❌ never
  drop replay's claim where the replay TASK ends. That task goes on to run the live loop for the session.
- **The claim is owned by the work, not by a slot on the manager.** M2 named the choice as open; this is the answer.
  `ScanCompletion` gets the local scan's, the spawned completion task gets the network one, `ReplayConfig` gets
  replay's, and each drops it where it clears `mgr.scanning`. So `stop_scan` and `shutdown` have nothing to release,
  which is the point: cancelling a walk is a REQUEST, and the thread keeps writing until it notices, so a slot would
  hand the ground back while rows were still landing. The cost is a divergence from today, and it is deliberate — a scan
  started immediately after a `stop_scan` is refused until the cancelled walk actually stops. The exposure is a
  debug-panel Stop-then-Start pair (`stop_drive_index` is the only door), and it closes a truncate-under-a-live-walk
  window that the flag left open. A hung walk thread now holds its volume, where before a stop-and-rescan could
  (unsafely) get out of it.
- **`Claim::take` reports the mode as `refused_by`, read PRE-take.** Precise rather than "somebody else is here": a
  claim's own roots can never refuse its first root, so a refusal is reported only when the claim got no ground at all.
  A self-overlapping frontier reads as refused by nobody.
- **`run_replay_event_loop` was already at clippy's 7-argument ceiling**, so replay's claim rides in `ReplayConfig`
  rather than as an eighth parameter.
- **Invariant density went UP again, by 3 net** across the four docs M2 touched (`lifecycle/CLAUDE.md` 18→18,
  `lifecycle/DETAILS.md` 23→25, `cover/CLAUDE.md` 11→12, `cover/DETAILS.md` 10→10; subsystem 380→383). One rule was
  deleted as obsolete ("❌ don't collapse the three questions" — they are collapsed) and one as a duplicate, but the
  claim's non-local lifetime and replay's invisible holding are new hazards that need saying. ⚠️ **So after three of the
  four milestones the count has not fallen, and M4 alone has to reverse +12.** M4 absorbs `OWED`, which is one
  mechanism's worth of rules; that is unlikely to be enough. Whoever runs M4 should report the number plainly and, if it
  is still up, say the thesis was wrong rather than shave docs to make it true.
- **A gap M2 leaves open, honestly:** nothing tests that replay holds its claim. There is no replay-loop test harness in
  the repo (`watch/event_loop/tests/` has none), and building one means a real `DriveWatcher` over a temp tree. What IS
  covered is the mechanism it relies on: an `Exclusive` holder refuses a scan, and a scan's ground comes back on every
  outcome.

What executing M0 and M1 changed:

- **The performance section understated the claim table by more than an order of magnitude, and M1's numbers are in.**
  It guessed "plausibly tens of milliseconds" for the `Vec` scan on the grounds that these are plain string comparisons.
  Measured: **446.77 ms** to take a 2,500-root frontier, 441.46 ms to re-ask under a live walk. The quadratic term
  dominates long before per-comparison cost matters. After the `BTreeMap`: 2.23 ms and 267 µs, flat at ~1.0 µs a root
  (`docs/notes/claim-table-cost-2026-08-17.md`). The plan's caution still stands on the point it was actually making:
  this names ~450 ms of the unattributed 3.0 s and ❌ does not close that question.
- **M1 needed no `#[allow(dead_code)]`.** Wiring `Mode` into `take` with `cover::start` passing `Additive` gave the enum
  a production caller, and `Exclusive` being constructed only by tests turned out not to warn. So M2 inherits a clean
  surface and has nothing to remove.
- **`Claim::take`'s signature is now `(volume_id, frontier, mode)`**, and the volume-wide half of the conflict rule is
  read BEFORE the claim takes anything. M2's `Exclusive` claim over several roots therefore can't refuse its own second
  root; ❌ don't reintroduce a per-root exclusivity test.
- **`Claim::take` does not yet report the conflicting holder's MODE.** M2's resolution needs that (Exclusive conflict →
  `AlreadyScanning`, Additive conflict → `GroundBeingWalked`), and M1 deliberately stopped short of adding a return
  channel with no caller. It is M2's first change, not a gap.
- **`ground_being_walked` is unfiltered and still answers for every holder.** Correct today, because nothing takes an
  `Exclusive` claim yet. ⚠️ M2 filtering it to Additive holders is not optional cleanup: the moment a scan holds an
  Exclusive volume-root claim, an unfiltered query reports every frontier root as being walked.
- **The stale comment named below is fixed**, and its second caller was wrong too: `perform_registry_rescan` also drops
  the registry guard before starting the scan.
- **Invariant density went UP, by 9 markers across the six docs these two milestones touched.** That is the honest shape
  of the first two milestones: M0 adds a guard and M1 adds a mechanism, and both need rules saying why they can't be
  tidied away. The reduction this effort is named for has to come from M2 to M4, where mechanisms are collapsed and
  their rules deleted. ⚠️ If M4 lands and the count still hasn't fallen, the thesis is wrong and someone should say so.

### M0 — The latent truncate coupling, on its own ✅ DONE

**Intent:** `start_volume_scan` (`lifecycle/network_scan.rs:195,204`) asks `scanning` and `ground_being_walked` and
**never** `phases_have_work`. It is safe today only by accident of typing: `first_index_is_the_machines`
(`lifecycle/manager/phased.rs:266-273`) requires `uses_local_scanner()`, and there are exactly four `IndexVolumeKind`
variants, so no volume is both trait-scanned and phase-covered. A fifth that was both would silently truncate under a
running phase machine.

It is a three-line check that needs neither M1 nor M2, so it ships alone and first.

**Tests: TDD**, and note the red test cannot be reached through public paths: no volume is both trait-scanned and
phase-covered today, so the test has to force `pending_phases` onto a trait-scanned manager. That is expected, not a
sign the cite is wrong. **Checks:** `pnpm check rust`.

### M1 — The claim table gets the right data structure and a mode ✅ DONE

- `IN_FLIGHT` moves from `HashMap<String, Vec<String>>` (`lifecycle/cover/live.rs:33`, note: `String`, not `PathBuf`) to
  a path-keyed `BTreeMap` with prefix-range overlap queries. Keep the component-aware overlap rule at `:139-141`
  exactly.
- Add `Mode::{Exclusive, Additive}`. `Exclusive` conflicts with everything on the volume; `Additive` conflicts only on
  overlapping paths.
- Keep partial grant. Keep RAII release. ❌ Add nothing to `Drop`.
- ❌ No `now: Instant` parameter. Nothing in `Claim` uses time; that was vestigial from the broker draft.
- **Dead-code note:** `#[cfg(test)]` callers do not suppress `dead_code`, and the project forbids leaving warnings.
  Either wire `Mode` into the existing `take` signature (defaulting to `Additive`) so M1 has a production caller, or
  carry a justified `#[allow]` that M2 removes.

**Tests: fully TDD, real red→green.** Pure and synchronous, so exhaustive coverage costs milliseconds. Every mode pair,
partial grant, ancestor/descendant overlap both directions, whole-volume versus subtree. The 6 existing `live.rs` tests
stay and grow.

**Bench:** criterion, take/release at 2,500 roots. Baseline to `docs/notes/`.

**Docs:** `lifecycle/cover/CLAUDE.md` + `DETAILS.md`. **Checks:** `pnpm check rust`.

### M2 — The scan entries take an Exclusive claim as their guard ✅ DONE

**Intent:** collapse `mgr.scanning`'s guard reader and `ground_being_walked` into one question. ❌ **This milestone does
NOT stop scans from setting `mgr.scanning`.**

- `start_scan` (`lifecycle/manager/start.rs:328,340,352`) and `start_volume_scan` (`lifecycle/network_scan.rs:195,204`)
  take an `Exclusive` whole-volume claim **in place of the guard reads**.
- **Keep `scanning.store(true)`** at `lifecycle/manager/start.rs:588` and `lifecycle/network_scan.rs:229`, and keep
  every reader listed in M3. Dropping the store would stop SMB/MTP buffering across the truncate, dark the hourglass,
  empty `walked_roots`, un-suppress the verifier, and make `awaits_its_first_scan` lie. See M3.
- **Keep `cover_context_for` (`lifecycle/state.rs:259`) reading `mgr.scanning`.** It is what suppresses cover walks
  during journal replay, which holds no claim. Constraint 5.
- ❌ **Do not make `phases_have_work()` claim-derived.** It stays the third question at
  `lifecycle/manager/start.rs:340`. Constraint 2.
- `indexing/handle/mod.rs:499` (`Index::coverage`) **stays a pure query.** Its caller may decide not to walk.
  `ground_being_walked` survives as a read-only query.

**The two guard reads produce two DIFFERENT outcomes, and one claim answer has to preserve both.** Today `scanning` →
`AlreadyScanning` → `force_scan` forgets the request and answers `Started` (`lifecycle/state/scan_control.rs:144-147`),
while `ground_being_walked` → `GroundBeingWalked` → keeps the request and answers `Deferred` (`:148-151`). Two
user-visible outcomes, two durability stories.

**Resolution: `Claim::take` reports the conflicting holder's MODE, and the entry maps it.** Exclusive conflict (another
scan) → `AlreadyScanning`. Additive conflict (a cover walk) → `GroundBeingWalked`. That is mode, ❌ not holder identity,
so constraint 2's prohibition does not bite.

⚠️ **This deliberately preserves today's behavior, including the part that looks like a bug**: "Rescan now" during a
running scan stays an idempotent no-op that reports `Started`. Making it queue instead is a **product call, not a
refactor detail** (it would mean a second full truncating rescan), and it contradicts documented behavior at
`lifecycle/state/scan_control.rs:110-111` and `lifecycle/rescan_request.rs:36-38`. ❌ Do not change it inside this plan.
See the product calls section.

⚠️ **Soften the API claim**: with a scan holding an Exclusive volume-root claim, `Index::coverage`'s `being_walked`
(`indexing/handle/mod.rs:499`) would report every frontier root for the scan's duration unless the query filters to
Additive holders only. **Filter it to Additive**, so the field keeps meaning what its doc says (another _walk_ has this
ground) and `StartOutcome::DeferredUntilSearchEnds` does not become a misnomer.

**Open mechanism, answered:** neither `stop_scan` (`lifecycle/manager.rs`) nor `shutdown` releases anything. The claim
belongs to the work, so the run's own ending frees the ground, and the two of them only cancel. See the M2 note above
for why that beats a shared slot on the manager and what it costs.

**The claim's lifetime is the hard part, and it is not RAII-local.** `start_scan` returns while the scan runs, and
`scanning` is cleared on other threads at `lifecycle/scan_completion.rs:112`, `lifecycle/manager.rs:573` (`stop_scan`),
and `lifecycle/manager.rs:742` (`shutdown`); `start_volume_scan`'s at `lifecycle/network_scan.rs:428`. So the claim must
be **moved into `ScanCompletion`** and dropped there, while still releasing on the early-`?` paths at
`lifecycle/manager/start.rs:568,585`. That widens `Claim`: it leaves `pub(super)` in `cover`, must be `Send`, and gains
a non-local lifetime. **Budget for that; it is not free.**

**Tests:** `mgr.scanning` is not untested: `state::set_scanning_for_test` exists, and
`cover::cold_drive_tests::walkable.rs:96-106`, `rescans.rs:132`, and `cover/network_tests.rs:322` anchor on it. Those
assertions stay valid, since the store stays. Add claim-level tests for the guard half. Keep
`phases/tests/coverage.rs:153` and `interleaving.rs` green.

**Checks:** `pnpm check rust`.

### M3 — `mgr.scanning` is renamed to say what it now is

**Intent:** after M2 the flag is a **reporting and buffering signal**, plus ONE remaining guard: `cover_context_for`,
which is how a replaying or scanning volume refuses a NEW cover walk. This is a rename plus a narrowed doc comment, ❌
not a split and ❌ not a deletion. ⚠️ A name that says "reporting only" would be a lie; pick one that covers the
`cover_context_for` reader too.

Full reader inventory (seven, and every one stays):

- `get_writer_and_scanning_for` (`lifecycle/state.rs:236`) → SMB `apply_smb_change` (`transports/smb/watch.rs:224-229`)
  and `buffer_mtp_handle_if_scanning` (`transports/mtp/watch.rs:162-167`). **Load-bearing across the truncate.**
- `cover_context_for` (`lifecycle/state.rs:259`) → replay suppression. Constraint 5.
- `get_status().scanning` (`lifecycle/manager.rs:617`) → **the hourglass. User-facing.**
- `walked_roots` (`lifecycle/manager.rs:626`).
- `awaits_its_first_scan` (`lifecycle/state/queries.rs:121`).
- `trigger_verification` (`lifecycle/state/scan_control.rs:79`).
- The safety-net clears at `lifecycle/manager/start.rs:118-121`.

❌ Never put a global claim lock on the transport buffering path; it is per-event and hot, with an ordering requirement
(`transports/DETAILS.md:109`).

**Tests:** the transport buffering ordering gets test-first coverage.

**Doc sweep:** ✅ done in M2 rather than deferred — `cover/cold_drive_tests/rescans.rs` and `cover/network_tests.rs`
carried doc comments describing the `mgr.scanning` guard, and a doc that stops being true is fixed where it stops being
true.

**Checks:** `pnpm check rust`, plus the SMB lane.

### M4 — The deferred rescan becomes a claim waiter ✅ DONE (with product call 4)

**Intent:** absorb the `OWED` set into the claim table. ❌ **Not "delete `rescan_request.rs`"**: the module also owns
`ScanStartError` and `RescanOutcome`, the master-switch check, and the runner. What moved is the STORAGE — `remember` /
`take` / `forget` are now `cover::remember_rescan` / `take_rescan` / `forget_rescan` over the claim table, which is what
the teardown sites and `force_scan` call. The module kept what a request MEANS and what running one does.

- The waiter is a bit on the claim table's per-volume entry (`cover/live.rs`), and `a_rescan_can_start` answers "owed,
  and the ground is free" in one look. `run_if_owed` keeps peek-then-spawn: never inline, ❌ never from `Drop`.
  Constraint 3.
- **Every post-M2 release site runs the waiter**, not just the cover walk's: `lifecycle/scan_completion.rs` (the local
  scan), the spawned completion task in `lifecycle/network_scan.rs` (the trait scan), and `watch/event_loop/replay.rs`
  (the replay phase's end). ❌ NOT `stop_scan` and ❌ NOT `shutdown` — they release nothing, because the claim belongs
  to the work. Miss one and a rescan deferred behind that holder never fires, which
  `rescan_request::tests::every_whole_volume_holder_runs_the_rescan_it_owes` now catches in the source.
- **The two scans fire it at the END of their completion task, not at the release**, because they keep writing after
  their walk thread joins. See the M4 note above.
- **The silent loss at `lifecycle/state/scan_control.rs:145` is half fixed.** The claim-conflict half is now kept and
  deferred (product call 4); the `phases_have_work` half still answers `Started`, which is honest — a first-index
  machine is walking the volume whole, in pieces — and constraint 2 keeps that question outside the claim table.
- `StartOutcome::DeferredUntilSearchEnds` keeps its meaning **because** M2 filters `being_walked` to Additive holders,
  and it gains a sibling, `DeferredUntilScanEnds`, for the holder that isn't walking.

**Tests: TDD**, `rescans.rs::a_remembered_rescan_waits_for_the_last_walk_out` as the anchor, plus
`::a_rescan_asked_for_during_a_scan_runs_when_that_scan_ends`,
`::clicking_rescan_five_times_during_a_scan_queues_one_walk`, and the two in `scan_completion::tests` that pin the local
scan's site and its ordering. **Checks:** `pnpm check`.

### M5 — Shareable manager custody (independent, spike first)

**Intent:** the honest version of the claim the broker draft got wrong. `Running(Box<IndexManager>)` plus a lock-held
`with_running_manager` forces `mem::replace` to get an owned `&mut`.

**⚠️ It is not pure ergonomics, and the spike must scope this.** `lifecycle/state/scan_control.rs:219-222` says:
"Holding the manager out is also the mutual exclusion a caller asking 'does this volume already have a machine?' depends
on: `start_pending_phases` finds nothing to start while we are away." `swap-scan-plan.md` item 5 states it more strongly
still. `Arc<Mutex<IndexManager>>` **removes that exclusion and forces a replacement.** So M5 does not simply "unblock"
swap-scan; it invalidates swap-scan's stated exclusion design.

Upside remains real: it retires three stranding hazards (poisoned lock after extraction, panic in `work(&mut mgr)` with
no `Drop` guard, `PendingPhases::BeingStarted` stuck if `PhaseStart::run` panics off the lock).

**Spike scope:** blast radius across `with_running_manager` callers, lock contention versus extraction, **and what
replaces the `ShuttingDown` exclusion.** If the numbers or the exclusion answer come back bad, stop and report. Nothing
else in this plan depends on M5.

**Checks:** `pnpm check rust`, then `pnpm check --include-slow`.

### M6 — The reconciler's rescan scheduler (optional)

`reconcile/reconciler/rescan/` is a second queue-and-lease scheduler scoped to one reconciler and unaware of actors 1
to 8. Its subtree semantics map onto claims directly. **Migrate the ownership half only**; the throttle, settle, churn
backoff, and hourglass hold are cost policy. A partial M6 is fine.

### M7 — Preemption (the product win; depends on M1 and M2 only)

**Intent:** today a folder the user opens can wait tens of seconds behind a large sibling (`~/projects-git` is 1.58M
entries on David's machine). `phased-indexing-plan.md` § "Interleaving without preemption" rules it out for **two**
reasons:

1. The claim is released by the walk thread on exit, so cancel-then-immediately-start makes the new walk defer the same
   ground and cover nothing. **Atomic handoff in the claim table fixes this.**
2. The join rule: the machine starts a walk only after `CoverWalk::finish()` returns, and `finish`
   (`lifecycle/cover/mod.rs:192-200`) joins the walk thread. **Handoff does not touch this.** Preemption latency is
   bounded by cancel-to-join, which this milestone must **measure and bound**, not assume.

**Also needed and missing from the primitive: a yield channel.** A refusal naming the holder gives no way to ask it to
stop. The claim must carry a cancel handle registered at acquire time, which runs into `signals.cancel` parentage:
cancelling one walk without killing the volume is in scope here.

❌ Do not couple this to M5.

**Tests:** integration, real red→green. **Bench:** time-to-index-a-visited-folder before and after, plus the
cancel-to-join bound. **Checks:** `pnpm check --include-slow`.

## Product calls for David

1. **M7 preemption: ship it?** Fixes a real, measured UX wart. Cost: a background first index takes marginally longer,
   and its progress ordering becomes non-monotonic. Recommendation: **yes, after M0 to M4 are green.**
2. **M5 custody: spike it?** Wide refactor, and the spike now has to answer what replaces the `ShuttingDown` exclusion.
   Recommendation: **spike it, decide on the numbers.**
3. **M6: defer indefinitely?** Consistency win, touches the live hot path. Easy to skip.
4. **Should "Rescan now" during a running scan queue instead of no-op?** ✅ **DECIDED: yes, and shipped with M4.** It
   used to forget the request and report `Started`, so a user clicking mid-scan got nothing and was told it worked. Now
   an `Exclusive` conflict is remembered like a walk conflict and answers `DeferredUntilScanEnds`, so a **second full
   rescan really does run** when the first ends — the run in flight was started against a state the user has moved past.
   One walk per volume however many times they click. The UI says so: the drive badge menu gains
   `fileExplorer.navigation.driveIndex.queuedBehindScan`, and MCP reports the queue rather than a scan that started.

## Honest scope note on tests

An earlier draft claimed a pure in-memory state machine would let us delete a large share of the expensive integration
tests. **That was too optimistic and the research refuted it.**

Reality: **9 cheap unit tests relocate** (6 from `live.rs`, 3 from `rescan_request.rs`), which is a file move, not a
cost reduction. **2 flaky tests get rewritten** (`e0aaa0116`, `2bac79626`, both of which fail precisely because they
observe transient intermediate state). **Zero expensive integration tests go away.** The ~35 handoff tests assert
observable outcomes that ownership-state assertions cannot substitute for.

The real gain is **diagnosis speed**: when a handoff test fails, a unit-level claim test says why.

Facts that shape test work here: **no in-memory SQLite anywhere** in the repo, **no fake clock** (pass `now` explicitly
where time matters), raw sleeps banned by the `test-sleep` check (`cmdr_fs::testing::wait_until` is sanctioned), and
benches are `#[ignore]`-gated and not run by `--include-slow`.

## On principle #1 (protect the user's data)

Mostly not applicable, and worth saying rather than checking a box: the index is an explicitly disposable cache
("Rebuild, don't migrate"), so a crash costs a rescan. The two real exposures are **truncating under a live walk**,
which this plan makes harder to get wrong, and **permanently wedged ground**, which constraint 3 exists to prevent.

## A stale comment to fix while nearby ✅ DONE (M0)

`lifecycle/manager/start.rs:546-547` claims "`force_scan` (and the journal-gap fallback) call `start_scan` while holding
the registry lock, so a registry re-lock here deadlocks." `off_the_registry` drops the guard before calling `work`
(`lifecycle/state/scan_control.rs:236-238`). The `apply_freshness_event_on` call it justifies is still correct for other
reasons, but the stated reason is not true. Unrelated to this plan; fix it in passing.

## Sequencing against other plans

`swap-scan-plan.md` (`docs/specs/later/indexing/swap-scan-plan.md`) is NOT STARTED. M0 and M2 help it. **M5 does not
simply unblock it; M5 invalidates its stated exclusion design and forces a replacement.** Whoever picks up swap-scan
should read M5's spike result first.

Independent, no interaction: `sealed-subtrees-plan.md`, `media-ml-index-plan.md`, `index-vacuum-reader-pinning.md`,
`drive-index-overall-eta.md`, `resource-use-plan.md`, `scoped-incremental-walk.md`, `importance-subsystem-plan.md`.
`later/indexing/out-of-process-indexing.md` gets easier if M5 lands.

## Definition of done

- Every `❌` rule a change makes obsolete is **deleted from the docs**, not kept as history. Report before/after counts
  per touched doc; `pnpm check invariant-density -v` prints the gauge (a passing check is suppressed in quiet mode).
- `CLAUDE.md` files stay in the 300 to 400 word band; depth to the sibling `DETAILS.md`. One that will not compress
  under 600 words means the module wants splitting: say so rather than bumping an allowlist.
- Benchmarks for M1, M2, and M7 in `docs/notes/` with method and date.
- Conventional commits leading with impact. No `Co-Authored-By`. No push.
