# Flaky test eradication

**Status**: SHIPPED (2026-07-29): the measurement tooling, the contention verdict across all three Rust lanes, and
Playwright retry-pass reporting. One open item at the bottom, needing David's OK: a per-test duration budget for the
Rust suites.

**Goal**: a Rust test suite where a red run means a real regression. It now does, by a different route than this spec
originally proposed: rather than restructuring 19 tests, the suite got the instrumentation to say WHY a run went red,
and a red run re-runs its failures alone before believing them.

## What the measurements actually showed

Re-measured 2026-07-29 on David's M3 Max (12 P + 4 E) under deliberate oversubscription (96 busy workers, load ~198,
above the load ~144 that motivated this spec). A full `pnpm check rust-tests --fresh` produced 13 failures:

- **9 killed at the 8 s nextest cap**, every one a CPU-bound test:
  `file_viewer::encoding_test::find_newlines_utf8_matches_memchr`,
  `file_viewer::line_index_test::extend_to_n_equals_open_at_n`, `importance::scheduler::walk_memory_tests::*` (2),
  `archive::read::multiformat_test::tar_each_codec_round_trips_a_file`,
  `error_reporter::tests::streaming_tests::streaming_stops_at_cap`,
  `indexing::writer::maintenance::tests::handle_incremental_vacuum_reclaims_capped_batch`,
  `mtp::macos_workaround::tests::test_get_usb_exclusive_owner_returns_option`,
  `operation_log::writer::retention_tests::size_prune_brings_db_under_budget_and_shrinks_the_file`
- **2 leaks** (which nextest counts as PASSED, not failures)
- **2 ordinary assertion failures**, both timing-shaped:
  `live_throttle_collapses_rapid_rewrites_and_trailing_flushes_last_size` and
  `tail_watcher_sees_appender_task_events_within_debounce`
- **0 in-test `wait_until` deadline expiries**

Three of this spec's original premises did not survive that run:

- **"Every offender is a watcher, debounce, or lock test. Not one is a pure-logic test."** False under saturation. The
  dominant failures were pure compute (a memchr comparison, allocation-counting walks, codec round-trips). No test
  restructuring fixes those: a test needing 0.1 s of CPU cannot finish in 8 s of wall-clock when 200 threads share 16
  cores.
- **"The nextest budget is 8 s against tests that take under a second."** Not for the headline offender.
  `dropping_a_file_emits_one_event` already had a 20 s cap and `real-notify` serialization; the 17.75 s burn was against
  that 20 s, not 8 s. Several of the cap-killed tests are also not sub-second: `find_newlines_utf8_matches_memchr` takes
  **3.3 s alone on an idle machine**, a 2.4x margin against the cap, not 10x.
- **"19 offenders, one or two root causes."** The overlap between this spec's list and what actually fails under
  saturation is small. Also worth knowing: much of the `downloads::` cluster had already been fixed before this spec was
  written (`0359b38e7`, `8c485aa91`, 2026-07-06/09), which the original measurement didn't account for.

## What shipped

**A run's verdict is now self-explaining** (`scripts/check/checks/rust-test-diagnostics.go`):

- Retry-rescued runs are no longer silent. nextest exits 0 when a retry saves a run, so the suite used to report a clean
  pass while hiding the exact flake the retries exist to tolerate. All three Rust lanes now parse `FLAKY n/m` and
  downgrade such a run to **warn**, naming the test and the rescuing attempt. The retry budget became a standing
  flake-rate meter.
- Failures are sorted by which deadline blew: killed at the nextest cap, blew its own in-test `wait_until` deadline
  (quoting the wait), leaked, or ordinary panic. The two timeout classes look identical in raw nextest output and need
  opposite fixes; conflating them is what produced the wrong analysis above.

**A red run re-runs its failures alone before believing them** (`scripts/check/checks/rust-test-contention.go`). Rather
than loosening the cap globally (which would cost every idle run its hang detector, and the cap encodes a real 4-day
red-CI incident), only the failing tests re-run, serialized, and the outcome classifies them: passed alone at the
unchanged deadline means the suite was starving it; needed headroom on a quiet machine means real slowness; needed
headroom on a busy one is inconclusive; failed alone with headroom is a genuine failure. Full contract:
`docs/testing.md`.

Verified both directions: at load 256 a real run returned warn (9 cleared as contention, 1 left unsettled) in 2m51s
against a 2m0s idle baseline; a planted always-failing test came back red at load 70.

Both original follow-ups SHIPPED (2026-07-29): `rust-integration-tests` now gets the contention re-run (with
`--run-ignored only` riding in `baseArgs`, since every test there is `#[ignore]`-gated), and both Playwright lanes warn
on retry-passes read from the structured JSON report.

The stated blocker for the integration lane turned out not to exist. It rested on a claim that healthy SMB tests take
up to 130 s, which was inferred from a per-test cap rather than measured. **Caps are hang backstops, typically 20-50x
the real runtime.** Measured on an idle M3 Max, that test runs in **2.8 s** and the whole 53-test integration suite is
**5.3 s** wall-clock, so the 40 s retry cap already had ~14x headroom.

A contention re-run for the E2E suites is NOT worth building: Playwright already runs `workers: 1` with
`fullyParallel: false`, so there is no intra-suite parallelism for a serialized probe to remove, and the probe stage
would be indistinguishable from the original run. The Rust mechanism works precisely because that suite is massively
parallel.

## Open: a per-test duration budget for the Rust suites

David's standing goal is every test well under 2 s. Measured 2026-07-29 on an idle machine, that's nearly true already:
**16 tests out of ~4,900 exceed 2 s.**

- Rust unit (4,858 tests, ~30 s wall-clock): 7 over 2 s. The one real outlier is
  `indexing::store::tests::open_and_recover::busy_db_is_retried_not_deleted` at **5.5 s**; the rest sit at 2.0-2.2 s.
- SMB integration (53 tests, 5.3 s wall-clock): 9 over 2 s, topping out at 2.8 s.
- Playwright E2E: already enforces this exact budget. `e2eSlowTestThresholdMs = 2000` warns on any spec over 2 s, with
  a reasoned allowlist (24 macOS / 14 Linux entries, capped at 3 s).

The systematic fix is to give the Rust lanes the same treatment E2E already has: a warn-only per-test duration budget
with a reasoned allowlist, mirroring `e2e-durations.go`. That makes the goal enforced and visible rather than a one-off
cleanup. NOT STARTED: it seeds a new allowlist with ~16 entries, and agents don't create or raise an allowlist without
David's OK (`.claude/rules/file-length-allowlist.md`).

Shrinking `smb_integration_concurrent_streaming_writes_no_deadlock` specifically is NOT the lever. It buys ~1 s on a
5.3 s suite and trades away repro strength on a deadlock regression test whose shape (200 files, 60 × 1 MB writes forced
through the streaming fallback at concurrency 8) is deliberately tuned to the production workload that surfaced the bug.
No reduction can be shown to still catch it without reproducing the original deadlock.

## Still-open guidance

Don't fix flakes by adding retries, raising individual timeouts one at a time, or marking tests `#[ignore]`. The one
sanctioned retry carve-out is narrow and now visible (`docs/testing.md`). If a test lands in the "needed headroom on a
quiet machine" verdict, that's real slowness: tweak the test or give it an explicit, documented per-test override.
