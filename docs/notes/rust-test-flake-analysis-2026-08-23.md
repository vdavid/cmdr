# What actually makes the Rust lanes go red (2026-08-23)

Measured on David's M3 Max (12 P + 4 E), against `~/cmdr-check-log.csv` (382 runs since 2026-08-01 whose verdict named a
starved test) and `~/cmdr-test-log.csv`. Read `docs/testing.md` for the rules; this note is the evidence behind them.

**Read the two logs correctly, or the ranking lies.** `~/cmdr-test-log.csv` keeps every non-pass but only passes over
`testLogSlowSeconds` (1.0 s, `scripts/check/stats.go`), so a "fail rate" over its rows is conditioned on the test also
being slow. FAILURE COUNTS are unconditional and comparable; rates are not. And `~/cmdr-check-log.csv`'s verdict message
carries a test's full path at the time it ran, so a module move splits one test's history across two names, and a test
in an integration-test binary has no `::` in its name at all — a `::`-requiring extraction drops it silently. That is
how the suite's single most persistent flake stayed invisible in a top-offenders list.

## The four causes found, in order of what they cost

**1. Seven per-test nextest overrides selected nothing** (fixed; `nextest-filter-coverage` now fails on it). `test(x)`
is a substring match on a test's FULL path, so a filter spelling a module path dies silently when the module moves.
Three refactors detached seven filters over five weeks: `downloads::watcher::tests::` → `watcher_test::` (five tests
lost `real-notify` serialization and their 20 s cap, against redo loops documented to take ~11 s under saturation),
`cold_drive_tests::a_change_inside_a_walked_branch` → `…::branches::…` (lost `real-notify` and a 40 s cap, leaving its
own 30 s in-test wait unreachable under the 20 s it fell back to), and `indexing::external_drive_fixture::` →
`indexing::tests::external_drive_fixture::tests::` (lost `disk-image` serialization, so concurrent `hdiutil`
attach/detach churn on one FSKit service was exactly what the group exists to prevent).

**2. Two real-mount tests shared one share** (fixed by folding the assertion into the other kernel-mount test).
`smb_integration_volume_id_is_per_mount_not_per_path_shape` failed 35 of 277 runs, and **34 of those 35 at 20.2–22.6
s**: its 20 s NetFS settle poll, never the 16 s kernel-mount connect its comment worried about.
`smb_integration_mount_guest_no_dialog` makes the same guest mount of the same `public` share and failed 1 of 192. Both
pre-clean with `diskutil unmount force /Volumes/public` and both force-unmount on exit, in parallel processes, so either
can tear the other's mount down; the survivor then polls a path that has stopped being a mount. Idle cost of the deleted
test was **0.25 s** against a 40 s cap.

**3. A duplicate test in its own binary** (deleted). `tail_watcher_sees_appender_task_events_within_debounce` was named
in 40 starved-run verdicts, more than any other test and spread evenly from 2026-07-30 to 2026-08-23, and failed 22
times since 2026-08-09. Its four `file_viewer::watcher_test::` siblings assert the same `Grew` on the same live FSEvents
in the same serialized group, more strictly, and failed **zero** times (and have never once used the `retries = 2` they
carry). The watcher observes the filesystem, so "a Tokio task wrote the bytes" was never a distinction it could see.

**4. Thin cap margins under host saturation** (not fixed; see below). This is the residue the 2026-07-29 measurement
already identified, and restructuring the tests is still not the fix.

## The margin table, and why a flat duration budget is the wrong metric

One clean full run (`cargo nextest run --workspace --features cmdr/virtual-mtp`, 2026-08-23): **6,599 tests, 26 s wall
clock, 379 s of summed per-test wall clock, mean 57 ms. 12 tests over 2 s, 3 over 3 s, 1 over 5 s.** These are IDLE
numbers, which is the wrong basis for a speed bar: the standing standard is two seconds on a saturated machine
(`docs/testing.md` § "A Rust test gets two seconds on a saturated machine"). What follows is about predicting flakes, a
separate question.

Two saturated full-suite runs (96 and 192 spinning workers, load 77 and 209) killed nine tests between them. Ranked by
**margin = per-test cap ÷ idle runtime**, they are the thin-margin tests, and nothing else:

| idle  | margin (8 s cap) | test                                                                                                |
| ----- | ---------------- | --------------------------------------------------------------------------------------------------- |
| 5.55s | 1.4×             | `indexing::store::tests::open_and_recover::busy_db_is_retried_not_deleted`                          |
| 3.06s | 2.6×             | `file_system::listing::streaming_test::a_local_directory_read_emits_progress_events`                |
| 2.58s | 3.1×             | `importance::scheduler::walk_memory_tests::the_walk_holds_a_small_fixed_record_per_folder`          |
| 2.57s | 3.1×             | `importance::scheduler::walk_memory_tests::the_walk_does_not_allocate_per_folder_or_per_file`       |
| 2.21s | 3.6×             | `agent::wake::tests::coalesce::five_million_changes_in_one_folder_coalesce_to_one_exact_bundle`     |
| 2.21s | 3.6×             | `indexing::writer::maintenance::tests::handle_incremental_vacuum_reclaims_capped_batch`             |
| 2.03s | 3.9×             | `error_reporter::tests::streaming_tests::streaming_stops_at_cap`                                    |
| 1.34s | 5.9×             | `thread_cpu::tests::a_sibling_threads_work_does_not_land_on_this_threads_counter`                   |
| 0.81s | 9.9×             | `file_viewer::encoding_test::find_newlines_utf8_matches_memchr` (CPU-bound, so it degrades hardest) |

`busy_db_is_retried_not_deleted` is the suite's thinnest margin at **1.4×** and did not happen to die in either run. It
is the one test a saturated CI run is most likely to kill next, and no duration rule catches it that doesn't also catch
a dozen honest 2-second tests.

**None of causes 1–3 has a duration signal at all**: 0.25 s, 1.6 s median, and a config fact with no runtime. A flat "2
s" budget would have found none of them, while flagging 12 tests that were never a problem. The margin ratio ranks the
same population correctly and is the metric to enforce if one is enforced. The 33-test
`indexing::lifecycle::phases::tests` cluster (~1 s each, 8× margin, 40 starved-run mentions between them) is an honest
cluster whose only fault is that margin.

## What was ruled out

- **The `downloads::` cluster is not a live cluster.** Its 320 mentions are 2026-08-06 to 2026-08-12 under the OLD
  module path, i.e. before `417d5408e`. Under the new path it has 8 mentions in the last 14 days. Its in-test
  self-healing (`observe_mutation`, `prime_watch`) works; what it had lost was the nextest override, which is cause 1.
- **Reproducing a contention flake in isolation does not work.** 24 targeted repeats of the two guest-mount tests at
  load 209, and 10 of the two viewer-watcher tests at load 206, were green every time. Only a full-suite run reproduces,
  and then it kills the thin-margin tests rather than the ones being chased. Budget for that before planning a repro.
