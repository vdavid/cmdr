# Manager custody: should `IndexPhase::Running` become shareable? (2026-08-18)

The spike `docs/specs/ground-ownership-plan.md` § M5 asked for. Question: replace
`IndexPhase::Running(Box<IndexManager>)` plus the `mem::replace` extract-work-reinsert dance with
`Arc<Mutex<IndexManager>>`, so callers get an owned `&mut` without taking the volume out of the registry.

## Verdict: no

**Drop M5 as specified.** `Arc<Mutex<IndexManager>>` does not remove the extraction dance's mutual exclusion; it keeps
the same exclusion and pays a lock for it, on paths the subsystem's whole lock discipline exists to keep lock-free. It
retires **none** of the three stranding bugs the plan credits it with: it relocates two into manager-mutex poisoning and
is irrelevant to the third.

**Ship a smaller replacement instead** (§ "What to do instead"). The dance's real defects are worth fixing, they are
about 150 lines of work, and none of them needs shareable custody.

The blast radius is genuinely small, so blast radius is not the reason to say no. The reason is that the primitive being
proposed is the wrong shape for what the window does.

## 1. What replaces the `ShuttingDown` exclusion?

**Nothing has to, because `Arc<Mutex<_>>` cannot give the exclusion up.**

`off_the_registry` (`crates/cmdr-index/src/indexing/lifecycle/state/scan_control.rs`) runs
`work: impl FnOnce(&mut IndexManager) -> T`. Under `Arc<Mutex<IndexManager>>` there is no way to hand `work` its
`&mut IndexManager` without holding the manager's `MutexGuard` for the whole call. So the exclusion survives verbatim,
expressed as a held mutex rather than an absent map entry. There is no design branch where you keep the shape and give
up the exclusion, and therefore no replacement mechanism to design.

That reframes the milestone. M5 is not "trade exclusion for ergonomics". It is "keep the exclusion, and convert a window
where **no lock is held** into a window where **a lock every reader of that volume must take** is held across blocking
I/O." That is the exact hazard `lifecycle/DETAILS.md` § "Lock discipline" records two QA incidents for.

### The callers that depend on the manager being out, and what each would do

Every registry reader changes its answer while the manager is out, because the published phase is `ShuttingDown`.
Sorting them by whether that is protection or collateral is the useful exercise, and it is where the plan's framing is
too narrow: `start_pending_phases` is not the load-bearing one.

Protection, and both already carried elsewhere (see below):

- **`start_pending_phases`** (`state/startup.rs`): `with_running_manager` finds nothing, so no machine starts.
- **`cover_context_for`** (`state.rs`): `None`, so a search walk gets no writer.

Correct refusal either way, since a mutex gives the same answer by waiting:

- **`stop_scan`** (`state/scan_control.rs`): `Err("Indexing not initialized")`.

Collateral, benign:

- **`get_writer_and_scanning_for`** (`state.rs`): `None`, so an SMB or MTP change is **dropped**. Benign only because
  the scan re-walks everything.
- **`awaits_its_first_scan`** (`state/queries.rs`): `false`.
- **`trigger_verification`** (`state/scan_control.rs`): no-op.

Collateral, and wrong:

- **`is_active`** (`state/queries.rs`): `false`, to nine callers including the drive badge, which then renders
  `enabled: false` alongside a live freshness color, a shape its own doc comment says can't occur.
- **`get_status` / `get_debug_status`** (`read/queries.rs`): `disabled_status_response()`, so `initialized: false` and
  `scanning: false`. The hourglass blanks at the moment a rescan starts.

A bug:

- **`stop_indexing` / `clear_index`** (`state/teardown.rs`) and **`fail_index`** (`state/supervisor.rs`): `Ok(())`,
  having done nothing. Proved in § 4.

Two entries carry the argument.

**`start_pending_phases`'s exclusion is already provided by `PendingPhases`.** The doc comment at
`state/scan_control.rs:263-266` names it as the reason for the dance, but the machine start is _already_ not protected
by extraction: `start_pending_phases` releases the registry guard across `PhaseStart::run` while the manager sits in the
registry as `Running`. `PendingPhases::BeingStarted` exists precisely to cover that gap, and `take_the_phase_start` is a
compare-and-set on it (`manager/phased.rs:289-292`). So "does this volume already have a machine?" is answered by
`phases_have_work()`, which is true in every state but `No`, in both designs. The extraction adds nothing here.

**`cover_context_for`'s exclusion is already provided by the claim table**, as of M2. `start_scan` asks
`phases_have_work()` and then `claim_the_volume()` **before** the truncate and before every blocking call. A cover walk
that reached `cover_context_for` during the window would still be refused one line later by `Claim::take`. The
`ShuttingDown` refusal is belt over braces. (Worth a claim-level test before anyone relies on this in a change, rather
than an assertion.)

So the two exclusions the plan and the code comments name as the reason for extraction are both already carried by
mechanisms that shipped since the comments were written. **What extraction uniquely still provides is the collateral,
and the collateral is a bug.**

## 2. Blast radius: small, and not the deciding factor

- **23 `IndexPhase::Running` match sites**, in eight files, all inside `crates/cmdr-index`: `state/scan_control.rs` (9),
  `state.rs` (3), `state/queries.rs` (3), `read/queries.rs` (2), `state/teardown.rs` (2), `manager.rs` (2),
  `state/startup.rs` (1), `state/supervisor.rs` (1).
- **`with_running_manager` is private to `state`**, with five call sites (`state.rs` ×2, `state/startup.rs` ×3). The
  plan's framing implies a wide caller surface; there isn't one.
- **Four test call sites** for the two extraction test helpers plus `set_scanning_for_test`.
- Nothing outside `cmdr-index` touches `IndexPhase` at all.

Mechanical: the 23 matches, and the field accesses inside the five `with_running_manager` closures.

Needs thought, and this is the whole of it:

1. **Lock order.** Two orders would exist: registry → manager (any `with_running_manager`-shaped reader) and manager
   alone (any extraction-shaped caller that clones the `Arc` and drops the registry guard). The phase driver calls
   `cover::context_for_walk` per frontier group, 50 to 150 times per phase (`lifecycle/phases/mod.rs:578`), and
   `context_for_walk` goes through `cover_context_for`, which takes the registry lock. So the inverted order is not
   hypothetical, it is on the hottest lifecycle path. A reader that takes the manager mutex **while holding the registry
   guard** converts a per-volume stall into a process-wide one: exactly the QA-observed UI freeze
   (`lifecycle/DETAILS.md` § "Lock discipline"). Every reader would have to be rewritten two-phase (clone the `Arc`
   under the registry lock, release, then lock the manager), which reintroduces a check-then-act gap each one has to
   handle.
2. **The two hot readers.** `get_writer_and_scanning_for` is called per directory change on SMB
   (`transports/smb/watch.rs:224`) and per handle on MTP (`transports/mtp/watch.rs:163`), documented as a "cheap gate",
   with an ordering requirement (`transports/DETAILS.md`). `cover_context_for` is called 50 to 150 times per phase plus
   once per search walk. Both would move onto the manager mutex.

## 3. Lock contention: measured

Method: `off_the_registry` and `perform_registry_rescan` instrumented with an `Instant` around `work`, release build,
`cargo test -p cmdr-index --lib --release`, `cold_drive_tests::rescans` (seven tests, real managers over temp trees).
MacBook (Apple silicon, `darwin 25.5.0`), 2026-08-18. Instrumentation was reverted; it is three lines to redo.

**Window length, small temp drive:**

- A **refused** scan start (claim conflict, or the machine already has work): **7 to 57 µs**, n = 9.
- A **started** scan: **2.76 to 2.95 ms**, n = 5.

The started-scan figure is a **floor**, for two reasons.

**The volume-space query is free in tests and is not free in the app.** `FakeVolumeProvider::volume_used_bytes` returns
`None` immediately; production runs `get_space_info_for_path`, which on macOS is
`NSURLVolumeAvailableCapacityForImportantUsageKey`. Measured directly (Swift, `swiftc -O`, 20 iterations per path, same
machine, 2026-08-18):

| Path                        | Cold     | Median       | Max       |
| --------------------------- | -------- | ------------ | --------- |
| `/` (boot, APFS)            | 6 260 µs | **6 500 µs** | 26 476 µs |
| `/Volumes/naspi` (SMB)      | 18 µs    | 19 µs        | 5 146 µs  |
| `/Volumes/PiHDD` (external) | 19 µs    | 19 µs        | 6 517 µs  |

The boot disk is the expensive one because "important usage" accounts for purgeable space (APFS snapshots, iCloud
caches). So a real boot-disk scan start is roughly **9 ms typical with a 30 ms tail**, before the second reason.

**The flush has no upper bound.** `start_scan` sends `DeleteMeta`, `BumpCurrentEpoch`, and (on the fresh-scan arm)
`TruncateData` plus the exclusion stamp, then calls `block_in_place(writer.flush_blocking())`. `flush_blocking` queues a
`Flush(tx)` message **behind** those and blocks on the reply (`writer/mod.rs:768`), so the window contains whatever the
writer has to do first. `TruncateData` is `DELETE FROM dir_stats; DELETE FROM entries;` plus an **uncapped** incremental
vacuum (`writer/entries.rs:756-769`). From David's production log, real root index:

```
2026-08-12T12:05:18.609+02:00 INFO cmdr_index::indexing::writer::entries
  Writer: truncated entries + dir_stats (2613ms)
```

That particular sample came from a launch-time `IncompletePreviousScan`, which runs during `Initializing` and so is
**not** in the window. But the same `start_scan` body runs inside `off_the_registry` for "Rescan now" and inside
`perform_registry_rescan` for a journal-gap fallback, a coalesced `MustScanSubDirs`, and an ingestion backlog; and
`network_scan.rs` has the identical `TruncateData` → `flush_blocking` pair for an SMB or MTP rescan. The truncate arm is
taken whenever `local_rescan_reconciles(entry_count, prior_scan_completed)` is false (`manager.rs:204`), that is, an
empty or never-completed index. **So a rescan that truncates puts a seconds-long operation inside the window**, and the
`block_in_place` wrappers exist because both blocking calls can stall on a wedged mount, which is unbounded.

**Is there a realistic path to holding a manager's lock across blocking I/O?** There is no path that avoids it.
`work(&mut mgr)` needs the guard for its whole body, and its body is the blocking prelude. The choice `Arc<Mutex<_>>`
offers is not "hold it or don't", it is "hold this lock or hold the registry lock", and the current design's answer is
"hold neither". A **3 to 10 ms typical, seconds-in-the-truncate-arm, unbounded-on-a-wedged-mount** hold, on a mutex that
every SMB change event and every frontier group on that volume has to take, is a regression against a window that costs
nothing today.

## 4. Does it retire the three stranding bugs? No. And there is a fourth

**1. The registry lock poisons after extraction** (`scan_control.rs:287`): the early return drops `mgr` and the phase
stays `ShuttingDown` forever.

- Today: stranded.
- `Arc<Mutex<IndexManager>>`: **not fixed.** The registry mutex is still there and still poisonable, and there is now a
  second poisonable mutex per manager, poisoned by a panic anywhere in manager code.
- A `Drop` guard, no custody change: fixed, by `IgnorePoison` plus a restore guard.

**2. `work(&mut mgr)` panics**: the unwind drops `mgr` and the phase stays `ShuttingDown` forever.

- Today: stranded.
- `Arc<Mutex<IndexManager>>`: **relocated.** The phase stays `Running`, but the manager mutex is poisoned, so every
  later `lock()` fails and the volume is stranded under a different name. With `IgnorePoison`, the manager is left
  half-mutated instead.
- A `Drop` guard, no custody change: fixed.

**3. `PendingPhases::BeingStarted` sticks if `PhaseStart::run` panics** (`state/startup.rs:388`).

- Today: `phases_have_work()` is true forever, so every scan entry answers `AlreadyScanning` for the session.
- `Arc<Mutex<IndexManager>>`: **unaffected.** This lives entirely inside `start_pending_phases` and has nothing to do
  with custody; `PhaseStart::run` runs off every lock in both designs.
- A `Drop` guard, no custody change: fixed.

**4. A teardown landing in the window is silently swallowed.**

- Today: the request is lost, proved below.
- `Arc<Mutex<IndexManager>>`: fixed, by removing the transient `ShuttingDown`.
- A `Drop` guard, no custody change: fixed, by making the transient state one a teardown can claim.

**A poisoned `Arc<Mutex<_>>` is its own stranding mode, and it has a larger surface than the one it replaces.** Today
the registry mutex is held for microseconds over infallible work; a manager mutex held across the whole scan prelude,
the whole shutdown drain, and every field access is exposed to every panic in the subsystem's largest type.

### Bug 4, which the plan does not list

`stop_indexing`, `clear_index`, and `fail_index` all extract with the same `mem::replace`, and all three have an arm
that puts a non-`Running` phase back and returns success. During the extraction window the phase is `ShuttingDown`, so
all three take that arm, **report success, and do nothing**, and then `off_the_registry` restores the volume to
`Running` and starts its phases.

Consequences, in order of how much they matter:

- **`fail_index` swallowed.** The failure supervisor awaits a one-shot signal and calls `fail_index` once
  (`state/supervisor.rs`). If a fatal storage error trips during the window, the volume never reaches `Failed`, the
  badge stays normal, and the manager is restored as `Running` over a dead writer for the rest of the session. Every
  write is dropped silently. This is a principle #1 exposure, and it is the one worth fixing regardless of what happens
  to M5.
- **"Turn indexing off for this drive" swallowed.** The persisted `user_disabled` intent still lands, so the next launch
  honors it; this session keeps indexing and writing.
- **"Clear this drive's index" swallowed.** Logs "already shutting down" and returns `Ok(())`.

Two red tests, run against `93cc47a05` in release. Both fail. They use the existing `while_shutting_down_for_test`
helper, which publishes exactly the window `force_scan` and `perform_registry_rescan` publish, so the reproduction needs
no new machinery. Paste them into `cover/cold_drive_tests/rescans.rs` as the red step of whichever milestone picks this
up:

```rust
#[test]
fn a_stop_during_the_extraction_window_really_stops() {
    let drive = ColdDrive::new("stop-in-window");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    drive.cover(&drive.path("scope"));
    drive.mark_scan_completed();

    let mut stop_result = None;
    crate::indexing::lifecycle::state::while_shutting_down_for_test(drive.volume_id, || {
        stop_result = Some(crate::indexing::lifecycle::state::stop_indexing(drive.volume_id));
    });

    assert_eq!(stop_result, Some(Ok(())), "the caller is told the stop worked");
    assert!(
        !crate::indexing::lifecycle::state::is_active(drive.volume_id),
        "and the volume really stopped"
    );
}

#[test]
fn a_clear_during_the_extraction_window_really_clears() {
    let drive = ColdDrive::new("clear-in-window");
    std::fs::create_dir_all(drive.tree.path().join("scope")).expect("dirs");
    drive.cover(&drive.path("scope"));
    drive.mark_scan_completed();

    let mut clear_result = None;
    crate::indexing::lifecycle::state::while_shutting_down_for_test(drive.volume_id, || {
        clear_result = Some(crate::indexing::lifecycle::state::clear_index(drive.volume_id));
    });

    assert_eq!(clear_result, Some(Ok(())), "the caller is told the clear worked");
    assert!(!drive.db_path().exists(), "and the database really went away");
}
```

A corollary: **`Detached::TornDownWhileAway` is unreachable in production.** It fires only when the instance is gone or
is no longer `ShuttingDown` on the re-lock, and no teardown path produces either from an extracted `Running` volume; all
of them bail. The ceremony that makes `off_the_registry` complicated is wired to a state nothing can reach, while the
case it was written for is swallowed instead. The plan already has the right shape here. It needs connecting, not
replacing.

## 5. What it does for `swap-scan-plan.md`

`docs/specs/later/indexing/swap-scan-plan.md` § 2.3 step 5 runs the whole quiesce → meta → durable marker → promote →
re-point sequence on an owned `&mut mgr` taken out of the registry, and calls extraction "the single mutual-exclusion
point: while it is out, a concurrent `stop_indexing` / `fail_index` / `clear_index` sees `ShuttingDown` and cannot win a
second extract".

**That is true, and the plan's conclusion from it is right.** The ground-ownership plan's claim that M5 "invalidates
swap-scan's stated exclusion design" is **wrong**, in both directions:

- Under `Arc<Mutex<IndexManager>>`, the swap holds the manager mutex for the whole commit and a concurrent teardown
  **blocks** instead of losing the race. The exclusion is preserved; only the vocabulary changes. It costs the eject and
  quit paths a wait on a multi-second commit, which is its own product problem, but it is not an invalidation.
- What swap-scan actually inherits from today's design is **bug 4**: a teardown during the swap is not merely refused,
  it is _lost_. Eject a drive mid-swap and it keeps indexing. Swap-scan's own § 4 already worries about mid-build
  teardown ("the plan must ALSO wire build-triple teardown into `shutdown()`, `fail_index`, and `stop_all_indexing`");
  the lost-teardown hole is the same worry one level up, and neither plan names it.

**So: M5 neither unblocks nor invalidates swap-scan. The replacement in § 6 actively improves it**, by giving the swap
window a teardown request it records rather than drops. Whoever picks up swap-scan should read this section and § 6, and
correct § 2.3 step 5's prose about what the exclusion guarantees.

## 6. What to do instead

Everything M5 was reaching for, minus the lock. Roughly 150 lines plus tests, one milestone.

1. **Make the window unstrandable.** An RAII guard that restores the phase on unwind, at the four extraction sites
   (`off_the_registry`, `perform_registry_rescan`, `stop_indexing`/`clear_index`, `fail_index`), plus `IgnorePoison` on
   the restore re-locks instead of `?`. Retires stranding bugs 1 and 2. `IgnorePoison` is already a production pattern
   here (`manager.live_event_task.lock_ignore_poison()`).
2. **Make the window claimable.** Give the transient extraction its own phase, carrying whether a teardown asked for the
   volume while it was out: `stop_indexing` / `clear_index` / `fail_index` set the flag and return, and the restore path
   reads it and shuts down instead of restoring. That connects `Detached::TornDownWhileAway` to a state that can
   actually occur, and retires bug 4, including the `fail_index` exposure. ⚠️ Keep it distinct from the real teardown
   `ShuttingDown`; conflating the two is what produced the bug.
3. **Make the window honest.** Give the readers that must stay truthful a source that does not vanish with the manager:
   hoist the volume's stable handles (`writer`, `scanning`, path space) into a bundle on `IndexInstance`, exactly as
   `VolumeSignals` already does for `{freshness, events, cancel}` and for the same stated reason. Then
   `get_writer_and_scanning_for` stops dropping SMB changes, `cover_context_for` stops depending on a phase, and the
   badge and hourglass stop blanking mid-rescan. This is also the prerequisite that would make `Arc<Mutex<_>>` viable
   later, so it is not wasted work if the call is ever revisited.
4. **Guard `BeingStarted`.** A `Drop` guard around `start_pending_phases`'s off-lock window. Retires bug 3, and it is
   independent of everything above.

Items 1 and 4 are small and clearly good. Item 2 is the one with real design content. Item 3 is the biggest and needs a
claim-table proof that dropping `cover_context_for`'s phase dependence is safe (§ 1 argues it is; argue it in a test).

**The alternative worth naming, and not taking now.** If someone wants the dance gone rather than fixed, the thing to
remove is the _blocking under custody_, not the custody: `volume_used_bytes` is a pure input that could be fetched
before extraction, and the `DeleteMeta` / `BumpCurrentEpoch` / `TruncateData` / stamp / flush sequence only needs a
`writer` clone and a read connection, both of which are cheap to hold outside the manager. A `start_scan` whose prelude
does no blocking work under `&mut self` needs no window at all. That is a larger and more speculative refactor than this
note can size, and it is the honest end state, so record it rather than doing it here.

## Method and hardware

- Machine: MacBook, Apple silicon, `darwin 25.5.0`, 2026-08-18. Other work running; treat maxima as noisy.
- Window timings: `Instant` around `work` in `off_the_registry` and `perform_registry_rescan`,
  `cargo test -p cmdr-index --lib --release -- --nocapture cold_drive_tests::rescans`, seven tests, real `IndexManager`s
  over temp trees, `FakeVolumeProvider` (so `volume_used_bytes` is free). Instrumentation reverted.
- Volume-space query: standalone Swift, `swiftc -O`, `URL.resourceValues` for
  `volumeAvailableCapacityForImportantUsageKey` plus `volumeTotalCapacityKey`, 20 iterations per path, first sample
  reported as cold.
- Truncate cost: David's production log, `~/Library/Logs/com.veszelovszki.cmdr/`, 2026-08-12, real root index.
- Bug 4: the two tests in § 4, run in release against `93cc47a05`; both fail. `fail_index` is a code read
  (`state/supervisor.rs`), not a run.
- Call-site counts: `grep` over `crates/` at `93cc47a05`.

## What this note does NOT settle

- **How often bug 4 actually fires in the field.** The window is 3 to 10 ms on a reconciling boot-disk rescan, so the
  race is narrow there; it is much wider on a truncating SMB rescan. Nothing here samples real-world hit rate, and the
  `fail_index` case is worth fixing on consequence rather than on frequency.
- **What item 3 costs.** No prototype was built. The claim that `cover_context_for` can safely stop depending on the
  phase rests on reading `manager/start.rs:415` and `cover/mod.rs:304`, not on a test.
- **Whether the `Arc<Mutex<_>>` contention would be tolerable if item 3 landed first.** It would be smaller, because the
  two hot readers would no longer touch the manager. It would not be zero, and the lock-order inversion in § 2 would
  still have to be designed away. This note recommends against revisiting, but does not claim to have measured the
  post-item-3 world.
