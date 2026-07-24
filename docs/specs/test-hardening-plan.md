# Test hardening: kill the flake class at its root

Status: in progress, started 2026-07-24.

## Why

Since 2026-01-01: 3,854 commits, 121 of them de-flaking work (~3%, one every ~1.7 days). Of the 108 that touched code,
68 (63%) also changed production files. The tests aren't the problem: test:prod line ratio is 0.78:1 and the pyramid
shape is right (4,601 Rust + 5,870 vitest + 272 E2E). The problem is that Rust tests have no sanctioned way to wait for
"done", so they guess with fixed sleeps.

Scope boundary, stated plainly: this effort fixes the **Rust test-synchronization** class. It does NOT touch the
frontend production-race class (24 of the 96 de-flake commits changed `apps/desktop/src` production code: `$effect`
cleanup races, pane-load races, operationId capture races). That's a separate effort; the FE test surface itself is
already healthy (`vi.useFakeTimers` dominates, zero `sleep()`).

## Loud rules

- **Never fix a flake by sleeping longer or widening a poll budget.** Wait on a real condition, or add the signal that
  makes the condition observable. A fix that only changes a number is rejected.
- **Keep the test's teeth.** M3 (production signals): true verify-red, break the guarded behavior and see the test fail.
  M4 (bulk conversion): the cheap credible substitute is **delete the wait and run** — red means it was load-bearing, so
  convert it; green means it was dead weight, so delete it. Never claim verify-red you didn't run.
- **Production changes stay behavior-preserving.** Adding a signal must not change ordering, timing guarantees, or lock
  discipline for existing callers. A real bug found on the way gets REPORTED to David, not folded into the commit.
- **Concurrency cap: 3 subagents in flight.** Absolute write paths in every delegation (subagents inherit the main
  clone's cwd, not the worktree). Terse returns (≤350 words), artifacts to files.
- Doc discipline per `.claude/rules/docs.md`: `CLAUDE.md` must-knows only, depth in `DETAILS.md`, current state not
  history, single-source.

## Ground truth (measured, and corrected by review)

- **~163 sleep call sites in Rust test code**: 136 in test-named files plus 27 inside `#[cfg(test)]` modules in
  production files. Of those, ~49 are the tail of a hand-rolled poll loop (M1 converts them) and ~87 are bare waits.
- **~86 further sleep sites are production code** (`delete/walker.rs:850`, `session.rs:490`, `writer/mod.rs:523`, …) and
  must never be flagged. Test-vs-production detection is M5's real landmine, not the poll-loop regex.
- **~11 ad-hoc wait helpers**, not six: `media_index/scheduler/kick_tests.rs` (two), `archive_edit/test_support.rs`,
  `listing/streaming_test.rs`, `file_viewer/session_test.rs`, `file_viewer/watcher_test.rs`, `session/test_hooks.rs:218`,
  `ai/llm_log/tests.rs:134`, `ai/client_streaming_test.rs:241`, plus inline loops at `stress_tests_concurrency.rs:885`
  and `reconciler/tests.rs:1570`.
- **The E2E surface is already enforced.** `cmdr/no-arbitrary-sleep-in-e2e` runs at error level
  (`apps/desktop/eslint.config.js:317-326`, plugin in `apps/desktop/eslint-plugins/`), and all 8 spec sites carry a
  reasoned `eslint-disable`. There is no E2E backlog and no second check to write; the only E2E-adjacent work is
  extending that rule to `src/**/*.test.ts` (35 real-wall-clock waits across 8 files, ~10.6 s).
- **`rescan_active` is not a production bug.** Its only non-test readers are the two single-flight guards
  (`rescan.rs:160,183`); the hourglass and sweep read `pending_rescans`. The early clear is a test-observability
  wrinkle, not a user-visible defect. (Corrects an overstatement in the first draft of this plan.)
- **`indexing/writer` IS a real instance of the class.** `Flush` replies inside `process_message` (`writer/mod.rs:1385`)
  while the hourglass clear and deferred `dir_stats` repair drain run afterwards, at the end of the loop iteration
  (`mod.rs:1130-1146`). Production callers exist (`lifecycle/manager.rs:704`, `lifecycle/network_scan.rs:222`,
  `reconcile/local_reconcile.rs:551`), so `flush_blocking()` returns before the work its callers call "caught up".
  Real-world impact is small (the gap is one loop iteration), the ordering claim is still wrong.

## Milestones, in dependency order

### M1: one canonical `wait_until`, and convert every hand-rolled poll loop

**Scope.** A single test-support helper (sync + async variants) that polls a predicate to a deadline and **panics with
the caller's description on timeout**. It must never return an ignorable bool: `listing/streaming_test.rs:390` and
`media_index/scheduler/kick_tests.rs:282` both do today, which is the silent-pass failure `desktop-svelte-bare-poll`
exists to prevent. Home: a `#[cfg(test)] pub(crate) mod test_support;` at the crate root, matching `pub mod test_mode;`
in `lib.rs`; the module-local `test_support.rs` convention already exists in three subsystems.

Then convert all ~11 ad-hoc helpers and the ~49 loop-tail sleep sites to it. This is what makes M5's check simple: after
M1, a sleep in test code is almost always a bug rather than a loop body.

**Landmines.** Sync and async need separate bodies (`std::thread::sleep` vs `tokio::time::sleep`); a sync helper in an
async test blocks the runtime and can deadlock a current-thread scheduler. The helper's own sleep is the one sanctioned
site and carries the directive.

**Test plan.** Unit tests for the helper: satisfied immediately, satisfied late, timeout panics with the description.
Converted call sites: full `pnpm check`.

**DONE.** One helper, ~11 copies gone, ~49 loop sites converted, `docs/testing.md` documents it as the sanctioned wait.

### M2: test-isolation guards for the two contended globals

Lands BEFORE M3 so M3's red/green evidence isn't collected under the contamination this removes.

**Scope.** Tests reach into `LISTING_CACHE` (108 refs from test files) and `WRITE_OPERATION_STATE` (39);
`INDEX_REGISTRY` (36) already has `TestInstanceGuard` (`indexing/tests/stress_test_helpers.rs`). Extend that same RAII
pattern to the first two: unique key per test, entries removed on drop, panic-safe.
`volume_strategy_pause_tests.rs:124,341` leak entries on a failing assert today.

**Intentions.** Explicitly NOT de-globalizing. All three are `HashMap<key, value>` behind a lock, so per-test keys plus
an RAII guard solve isolation structurally at a fraction of the cost of threading a handle through 81 production call
sites on `LISTING_CACHE` alone. `APP_HANDLE` (60 refs), `SESSIONS` (46), `SCAN_CHANGE_BUFFER` (27) have ZERO test
references and get nothing.

**Landmines.** Drop runs on unwind, but a `mem::forget` or leaked `Arc` breaks that. Tests asserting on cache-wide state
(counts across all keys) need rescoping to their own key, not just a guard.

**Also.** Document "new subsystem state hangs off a struct, not a `static`" in the colocated `CLAUDE.md`s of the two
subsystems. NOT a `.claude/rules/` entry: it would contradict 82 existing cells and consume resident-doc budget.

**DONE.** Two guards shipped, contended tests scoped to their own keys, docs updated, `pnpm check` green.

### M3: the two real completion signals

Only two of the four originally planned subsystems need a production change. `file_viewer/session` already has its
signals (`get_session_status().is_indexing`, the `Condvar` handshake in `session/test_hooks.rs:243-249`, monotonic
`rebuild_exit_count()` at `:265`) and 20+ tests already use them; `transfer/*` already awaits its `JoinHandle` under
`tokio::time::timeout` and has a deterministic `Semaphore` chunk gate in `volume_strategy_pause_tests.rs:199-261`. Both
are test-only conversions and move to M4.

**Scope.**

1. `indexing/writer`: append an `idle_epoch: AtomicU64`, bumped after the hourglass clear and repair drain, so "caught
   up" becomes observable. Do NOT move the `Flush` reply: that IS a behavior change. Report the ordering wrinkle to
   David rather than fixing it here.
2. `file_system/write_operations/manager`: add `admission_passes: AtomicU64`, bumped at the end of `run_admission_pass`
   (`manager.rs:409`). Five tests currently sleep 150–200 ms to assert "B is still Queued".

**Landmines.** `Notify` is the wrong primitive in both: `manager`'s settle runs the pass before the test awaits
(guaranteed lost wakeup), and the writer is a `std::thread` with sync `#[test]`s where `Notify` and `watch::changed()`
are async. Preference order is **counter → `watch` → `Notify`**, and counters use `SeqCst`, not `Relaxed`: getting the
ordering wrong buys a rarer flake than the sleep had.

**Test plan.** True verify-red per converted test, then `pnpm check` plus the subsystem's slow lane.

**DONE.** Two counters shipped, their tests converted, colocated docs updated, the writer ordering wrinkle written up.

### M4: mop-up

**Scope.** Every remaining sleep site in Rust test code, including the two subsystems demoted from M3:

- `file_viewer/session_test.rs`: only 5 sleeps are non-loop (`:332`, `:654`, `:904`, `:937`, `:1642`); three are
  legitimate and take directives, `:332` sleeps 500 ms and then deliberately asserts weakly, which is a real fix.
- `transfer/*`: extend the existing `Semaphore` gate from `ReleasingSource` to `SlowSource`. Worth a spike first: the
  nine yield tests are already `flavor = "current_thread"` + `LocalSet`, so `start_paused = true` may make the debounce
  tests exact and instant, and `CheckpointStream` already takes injected `debounce`/`min_progress_floor`
  (`checkpoint_stream.rs:100-106`). Verify real-fs writes on the blocking pool don't defeat auto-advance.
- The long tail: ~30 files with 1–3 sites each.

**Verification.** Delete-the-wait-and-run (see Loud rules). Legitimate sites (fake latency, a test whose subject IS the
debounce window) take an `// allowed-test-sleep: <reason>` directive.

**DONE.** Zero unjustified sleep sites in Rust test code.

### M5: the check, error-level, on a clean tree

**Scope.** One check, `desktop-rust-test-sleep`, modeled on `desktop-svelte-bare-poll.go`. **Error level from the start
and no seeded allowlist**: it lands after M1–M4 have cleared the backlog, so `main` is never red and there is nothing to
grandfather. (Warn level was rejected: warns are never cached and never quieted, so `pnpm check -q` would reprint ~87
lines on every run until the backlog cleared.)

**Design.** No poll-loop exemption: after M1 there are no hand-rolled loops left to exempt, and an exemption would
permanently bless the shape M1 exists to delete. Every sleep in test code is either converted or carries
`// allowed-test-sleep: <reason>`.

**Landmines.** Test-vs-production detection is the hard part (~86 production sleep sites must not be flagged, and
`writer/mod.rs` has a `#[cfg(test)] mod tests;` *declaration* at line 19 with production sleeps far below). Reuse
`lock-poison.go:124-200`'s tested `#[cfg(test)]` region tracking rather than writing a third copy, and make the
test-file predicate path-aware (`isRustTestFile` is base-name-only today and misses `indexing/tests/*.rs` and
`*test_support.rs`). Directive plumbing: `newDirectiveTracker` → `observe` on scanned lines → `markUsed` → `orphans` +
`formatOrphanDirectives`. Per `checks/CLAUDE.md`: declare `Inputs`, wire CI, no length truncation, update the count
table in `checks/DETAILS.md`.

**Riders.** (a) Extend `cmdr/no-arbitrary-sleep-in-e2e` to `src/**/*.test.ts` (35 real-wall-clock waits, 8 files).
(b) Fix the live single-source violation: `docs/tooling/testing.md:65` calls `pollUntil` "the canonical way to wait"
while `docs/testing.md:81` bans the bare form. (c) Fix `checks/DETAILS.md`'s reference to a `Warning(message)`
constructor that doesn't exist.

**DONE.** Check registered, CI-wired, error-level, green with zero allowlist entries.

## Invariants (checked at close-out)

1. `desktop-rust-test-sleep` is error-level and green with no allowlist file.
2. Exactly one `wait_until` helper exists; it panics on timeout.
3. Every remaining sleep in Rust test code carries an `// allowed-test-sleep:` directive with a real reason.
4. M3's converted tests were verified red; M4's were delete-the-wait verified.
5. No production behavior changed except the two counters, and every production wrinkle found is written up for David.
6. Every touched area's `CLAUDE.md` / `DETAILS.md` reflects the new state; no doc narrates the old one.
