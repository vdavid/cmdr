# E2E test timings

Per-test wall-clock comparison for the desktop Playwright E2E suite. Reads the JSON reports Playwright already produces
during normal `--include-slow` runs and emits a sortable table showing macOS vs Linux duration per test.

Built to answer: "Which E2E tests are disproportionately slow on Linux Docker?" — the answer points at where polling
timeouts, magic sleeps, or Docker-friendly rewrites would pay off most.

The same reports feed the automated per-test duration flagger in the check runner (warn-only, 2 s budget); see
`scripts/check/checks/CLAUDE.md` § "E2E test duration flagger". This script stays the manual deep-dive tool.

## Prerequisites

Run the E2E suites at least once so the JSON reports exist on disk:

```sh
pnpm check --include-slow
```

This produces, on the host:

| Report                              | Source                                                      |
| ----------------------------------- | ----------------------------------------------------------- |
| `/tmp/cmdr-e2e-report-mtp.json`     | macOS playwright check, MTP shard                           |
| `/tmp/cmdr-e2e-report-nonmtp1.json` | macOS playwright check, non-MTP shard 1                     |
| `/tmp/cmdr-e2e-report-nonmtp2.json` | macOS playwright check, non-MTP shard 2                     |
| `/tmp/cmdr-e2e-report-linux.json`   | Linux docker check (bind-mounted from inside the container) |

The macOS paths are set by `scripts/check/checks/desktop-svelte-e2e-playwright.go`'s `planShards`. The Linux path is set
by `apps/desktop/scripts/e2e-linux.sh` via `CMDR_E2E_JSON_REPORT` + a bind mount of the same path so the container's
report writes through to the host. `playwright.config.ts`'s reporter declares both `list` (for the check's live output)
and `json` (for this script) so a single suite run produces both.

## Usage

From the repo root:

```sh
cd scripts/e2e-test-timings && go run .
```

Output is a markdown table sorted by Linux/macOS ratio, descending. The first rows are tests that take
disproportionately longer on Linux than macOS — the highest-payoff targets.

## Flags

| Flag             | Default                                                                                             | Effect                                                       |
| ---------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| `--macos`        | `/tmp/cmdr-e2e-report-mtp.json,/tmp/cmdr-e2e-report-nonmtp1.json,/tmp/cmdr-e2e-report-nonmtp2.json` | Comma-separated macOS report paths                           |
| `--linux`        | `/tmp/cmdr-e2e-report-linux.json`                                                                   | Linux report path                                            |
| `--sort`         | `ratio`                                                                                             | `ratio` (linux/macos), `linux`, `macos`, or `delta` (ms)     |
| `--top`          | `0`                                                                                                 | Show only the top N rows (0 = all)                           |
| `--min-linux-ms` | `0`                                                                                                 | Hide tests faster than this on Linux (filter the cheap tail) |
| `--format`       | `md`                                                                                                | `md` or `csv`                                                |

## Examples

Top 20 worst Linux/macOS ratio outliers (the canonical "where to optimize" view):

```sh
go run . --top=20
```

Slowest tests on Linux in absolute terms:

```sh
go run . --sort=linux --top=20
```

Filter out the cheap long tail (tests under 1 s on Linux), then look at absolute deltas:

```sh
go run . --min-linux-ms=1000 --sort=delta --top=30
```

CSV for spreadsheet / scripting:

```sh
go run . --format=csv > timings.csv
```

## Output columns

| Column  | Meaning                                                                               |
| ------- | ------------------------------------------------------------------------------------- |
| `Spec`  | Spec file, with `test/e2e-playwright/` prefix stripped                                |
| `Test`  | Describe chain + test title, joined with `›`                                          |
| `macOS` | Total ms across all attempts (retries summed) on macOS; `—` if test only ran on Linux |
| `Linux` | Total ms across all attempts on Linux; `—` if test only ran on macOS                  |
| `ratio` | `Linux / macOS`; `—` when either side is missing                                      |

Durations are summed across retry attempts so the row reflects real wall-clock cost — a test that's flaky and retries
twice on Linux but passes first-try on macOS shows up as expensive, which is correct.

## When the script is useful

- After a perf-affecting change to the test infrastructure (new polling helper, new fixture recreation, etc.), compare
  the table before/after to see what got faster/slower.
- When deciding which tests to attack first if Linux runs feel slow — `--sort=ratio --top=10` gives a short hit list.
- When tightening a poll timeout (say from 15 s to 5 s) and you want a quick sanity check that the change doesn't push
  some test's median past the new ceiling — re-run, look at `--sort=linux --top=20`.

## Limitations

- Tests that only ran on one platform show `—` for the other side; they're still listed but sort as `ratio = 0` (last).
  Pass `--sort=linux` or `--sort=macos` to see them.
- Aggregates retry attempts. If you want first-attempt-only timing, parse the raw JSON directly (look at
  `tests[].results[0].duration` instead of summing).
- Doesn't model the per-shard parallelism win on macOS — macOS reports three shards run in parallel, but the script sums
  their durations as if sequential. For a "wall-clock budget" comparison the shards' max would be more honest. The
  current sum-then-compare is fine for the per-test optimization question this script answers.
