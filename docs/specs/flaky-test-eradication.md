# Flaky test eradication

**Status**: NOT STARTED. Investigation only so far; no fix attempted.

**Goal**: a Rust test suite where a red run means a real regression. Today it doesn't, so the honest reaction to red is
"run it again", which is the same as having no signal.

## The evidence

Measured 2026-07-27/28 on David's M3 Max (12 P + 4 E), running full `pnpm check rust-tests --fresh` repeatedly on an
otherwise idle machine, single-check runs so load was not a confound.

Clean `main` (4d53b2939), three consecutive runs: **1 passed, 2 failed**. A different `downloads::watcher::tests` test
each time. A feature branch over the same commit, four runs: 3 passed, 1 failed.

So the gating suite fails roughly two runs in three, on an idle machine, with no code change involved.

Under real load (load average 144 during parallel agent work) a single run produced 12 timeouts at once.

## The 19 distinct offenders

Every one is a watcher, debounce, or lock-contention test. Not one is a pure-logic test. That clustering is the main
clue: this is one or two root causes, not 19.

- `downloads::watcher::tests` - `dropping_a_file_emits_one_event`, `latest_download_returns_ring_value_after_event`,
  `note_pending_write_suppresses_matching_event`
- `downloads::runtime::tests` - `note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`,
  `note_pending_write_for_cmdr_outside_downloads_silently_noops`
- `downloads::commands::tests` - `go_to_latest_returns_download_from_scan_fallback_when_ring_is_empty`,
  `go_to_latest_returns_empty_when_ring_and_scan_both_turn_up_nothing`
- `file_viewer::session_test` - `test_session_close_stops_watcher`, `test_session_emits_file_changed_on_append`,
  `test_session_tail_mode_off_does_not_extend_index`, `test_session_rotation_reopens_backend`,
  `tail_mode_on_extends_backend_when_watcher_reports_grew`
- `file_system::listing::caching_reaper_test` - `reaper_keeps_recently_touched_listing_even_if_created_long_ago`,
  `reaper_evicts_stale_listing_and_its_watcher_together`
- `file_system::volume::backends` - `local_posix_test::test_listing_is_watched_flips_with_watcher_lifecycle`,
  `archive::watch::watch_integration_test::lru_eviction_releases_the_archive_and_its_watch`
- `indexing::watch::watcher::tests::current_event_id_returns_nonzero`
- `indexing::store::tests::interrupting_a_subtree_delete_never_strands_a_row`
- `network::manual_servers::tests::rapid_sequential_adds` (Linux; a 20-thread barrier over a `STORE_LOCK` file)

`downloads::` is 7 of 19 and by far the most frequent. Start there: one root cause probably clears a third of the list.

## What the numbers say about the cause

`dropping_a_file_emits_one_event` runs in **0.84 s alone**, and burned **17.75 s before failing** inside the full suite.
nextest flagged that run "leaky" (a handle outlived the test). So the code isn't wrong; the test is asserting a
wall-clock deadline it doesn't control, and something it spawned outlives it.

The nextest budget is 8 s against tests that take under a second. A 10x margin sounds generous and isn't: contention
eats that easily. A timeout is a **deadlock detector**, not a performance assertion, and should sit 50-100x above
healthy runtime, with condition-waits (not the timeout) keeping the suite fast.

## Hypotheses to check first

1. **Shared real-world state.** Does `downloads::` touch the real `~/Downloads`? If so the tests race each other AND the
   machine, which fits the cluster exactly. Per-test tmpdir is the likely fix.
2. **Leaked watchers/tasks.** nextest's "leaky" verdict is a concrete lead: find what outlives the test and join or drop
   it.
3. **Real-clock dependence.** Debounce and throttle windows driven by wall time rather than an injected clock.
4. **Wall-clock assertions** that survived the `test-sleep` check (`crate::test_support::wait_until` /
   `wait_until_async` are mandated by `apps/desktop/src-tauri/CLAUDE.md`; these 19 either predate the rule or slip past
   the checker). Worth asking whether the checker has a hole.

## Suggested approach

1. **Measure before fixing.** Run the full suite 20-50x overnight and produce a per-test flake-rate table. Without a
   baseline there is no way to prove a fix worked; "I ran it and it passed" is exactly the reasoning that lets flakes
   survive.
2. **Fix `downloads::` first** and re-measure. If the rate collapses, the remaining clusters likely share the cause.
3. **Raise the nextest timeout** to an honest hang-detector value, and make the suite fast via condition-waits instead.
4. **Sweep the rest**: replace duration assertions with condition-waits, inject clocks, isolate filesystem state.
5. **Keep it from coming back**: consider running CI deliberately oversubscribed so flakes surface there rather than on
   a laptop, and if `--retries` is ever enabled, report "passed on retry" as a flake rather than swallowing it. A silent
   retry policy is how a suite rots.

## Out of scope

Don't fix these by adding retries, raising individual timeouts one at a time, or marking tests `#[ignore]`. Those hide
the signal, which is the actual problem being solved.

## Success criteria

- A per-test flake-rate table exists, before and after.
- 20 consecutive full `rust-tests` runs on an idle machine, all green.
- The suite still passes when run under deliberate load (the real bar: at load 144 today, a correct suite would not have
  produced a single failure, because no test would be asserting a wall-clock duration).
