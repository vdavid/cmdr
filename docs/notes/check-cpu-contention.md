# Check CPU contention sweep + weights

Why some `--include-slow` runs flaked (E2E modal/popover timeouts, an 8s-cap Rust test) traced back to the check runner
firing up to `NumCPU` checks at once, each itself multi-threaded, so the short CPU-heavy checks piled on top of each
other and oversubscribed the machine 2-3×. This note records the per-check CPU measurement that calibrated
`CheckDefinition.CpuWeight` and the weighted admission gate in `runner.go`.

> **Outcome — `eslint-typecheck` (the headline finding) was a bug, not a cost.** The sweep flagged `eslint-typecheck` as
> the dominant cost (~871 s, ~1 core, the `--include-slow` floor). Follow-up showed that was a typescript-eslint
> projectService cliff: linting `.svelte` and `.ts` in one `eslint .` pass runs ~25× slower than two separate passes
> (~616 s vs ~15 s + ~10 s, same config/coverage — verified with planted cross-file violations). It's now split into the
> normal (non-slow) `eslint-typecheck-svelte` + `eslint-typecheck-typescript` checks that run in parallel (~15 s wall).
> The sweep data below is the snapshot that motivated both the weighted scheduler and that split.

## Methodology

Each non-fast check run in isolation (`./scripts/check.sh --check <id>`), 16-core macOS, two metrics:

- **`proc_cores`** = `(user+sys CPU seconds) / wall seconds` from `/usr/bin/time -l` on the check process tree. Native
  host CPU the check burned.
- **`sys_cores`** (avg + peak) = system-wide cores busy from `iostat` (`(100-idle)/100 × ncpu`), sampled each second.
  Catches CPU burned **inside the Docker/OrbStack VM** that the host process never shows.

The gap between them is the tell: a check with `proc_cores ≈ 0` but high `sys_cores` does all its work in a container
(`rust-tests-linux`, `e2e-linux`). Background baseline was ~2 cores (this session + iostat itself).

## Results (cold/working profile where it matters)

| check                                                                                                     |  wall | proc_cores | sys avg | sys peak | note                                              |
| --------------------------------------------------------------------------------------------------------- | ----: | ---------: | ------: | -------: | ------------------------------------------------- |
| svelte-tests                                                                                              |   40s |       10.7 |    14.2 |       16 | vitest workers — heaviest per-second              |
| integration-tests                                                                                         |  109s |        7.4 |     9.9 |       16 | native compile + SMB Docker                       |
| nilaway                                                                                                   |   11s |        6.5 |     8.5 |       14 | heavy but short                                   |
| website-e2e                                                                                               |   10s |        6.0 |     9.7 |       16 | chromium, short                                   |
| cargo-udeps (CI)                                                                                          |   96s |        4.6 |     7.9 |       15 | nightly full compile                              |
| rust-tests                                                                                                |   31s |        3.5 |     6.4 |       13 | warm-incremental; cold heavier                    |
| rust-tests-linux                                                                                          |  191s |       0.01 |     6.2 |       16 | **all in Docker-VM**                              |
| deadcode                                                                                                  |    4s |        4.4 |     5.6 |        6 | short                                             |
| e2e-playwright                                                                                            |  183s |        3.1 |     5.0 |       16 | builds app + N shards                             |
| govulncheck                                                                                               |    5s |        3.4 |     4.4 |        5 | short                                             |
| jscpd                                                                                                     |    8s |        1.2 |     5.2 |       11 | bursty                                            |
| e2e-linux                                                                                                 |  285s |       0.01 |     4.7 |       16 | Docker; mostly wait + build spikes                |
| eslint-typecheck                                                                                          |  871s |        1.1 |     5.0 |       16 | **longest, but ~1 core avg** (single-threaded TS) |
| clippy                                                                                                    |   24s |        0.9 |     3.9 |       11 | **warm cache** — cold ≈ all cores                 |
| bindings-fresh                                                                                            |    4s |        1.0 |     7.1 |        8 | **cache HIT** — regen compiles the crate          |
| cargo-audit/deny, svelte-check, svelte-eslint, website-{build,typecheck,eslint}, api-eslint, docker-build | 2-10s |    0.1-1.8 | 1.6-3.6 |        — | light / single-threaded                           |

### Caveats (don't read three rows literally)

- `clippy` was measured warm (incremental); **cold it is a fully-parallel compile pinning ~all cores** → treated as
  weight 8.
- `bindings-fresh` was a cache hit; when it actually regenerates it compiles the crate in test mode → weight 8.
- `rust-tests` warm-incremental; cold is heavier → weight 6.

## Headline finding: longest ≠ heaviest

The wall-clock-dominating checks barely use CPU on average:

- `eslint-typecheck` (14.5 min) runs on ~1 core (TypeScript project service is single-threaded).
- `e2e-linux` / `rust-tests-linux` burn ~0 host CPU — work is in the Docker VM, mostly wait + short build spikes.

So they make ideal **backbone fillers**: occupy wall-clock cheaply while the short CPU-heavy checks (`svelte-tests`,
`integration-tests`, `clippy`-cold, `cargo-udeps`, `nilaway`) run packed underneath them.

## Weights assigned

`CpuWeight` ≈ the check's working-profile average busy cores, rounded, Docker-VM-aware:

| weight | checks                                                                                                                       |
| -----: | ---------------------------------------------------------------------------------------------------------------------------- |
|     11 | svelte-tests                                                                                                                 |
|      8 | clippy, cargo-udeps, bindings-fresh, integration-tests                                                                       |
|      7 | nilaway                                                                                                                      |
|      6 | rust-tests, rust-tests-linux, website-e2e                                                                                    |
|      4 | deadcode, e2e-linux, e2e-playwright                                                                                          |
|      3 | govulncheck                                                                                                                  |
|      2 | jscpd, svelte-eslint, eslint-typecheck-{svelte,typescript}, svelte-check, website-{typecheck,build,docker-build}, api-eslint |
|      1 | cargo-audit, cargo-deny, website-eslint, everything unset (fast formatters/scanners)                                         |

The runner admits a check only when `sum(running weights) + weight ≤ NumCPU` (a weight-0/unset check counts as 1; an
over-budget check runs alone). Net effect: wall-clock stays bounded by the critical path (the Docker E2E checks under
`--include-slow`, `clippy`-cold for the default suite) while peak oversubscription drops from ~2-3× to ~1×.
Fast/unmeasured checks default to 1 and can be recalibrated later if the fast lane ever shows contention.

## Rust group: build-caching findings

The four heavy normal-suite Rust checks (clippy, bindings-fresh, rust-tests, rust-integration-tests) share one
`target/`, so cache invalidation in one shows up as a rebuild in the others. Findings from a warm-tree measurement
(`cargo nextest run --no-run` = build-only):

- **clippy used to `touch src/lib.rs` every run** to force a re-lint. That mtime bump invalidated `cmdr_lib` for the
  next debug cargo invocation: a warm test build is ~1-2 s, but **24 s right after the touch** (recompiles `cmdr` + the
  test binary). So `rust-tests`, `bindings-fresh`, and `integration` each ate a ~22 s spurious rebuild, and clippy paid
  it too. **Removed the touch** — with `-D warnings` a lint becomes a compile error, so warnings fail the build (not
  cached) and are re-surfaced on every run until fixed (verified: warm re-runs of an injected `needless_return` all
  caught it; clean stays clean). clippy dropped ~32 s → ~1-2 s warm, and the others stopped rebuilding. The `--fix`
  failure branch keeps its touch (different transition — `--fix` succeeds with unfixable warnings; only runs locally on
  an already-failing clippy, so it never poisons the shared warm cache).
- **`integration-tests` builds in `--release`** — a separate profile from `rust-tests`' debug, so they share no
  artifacts and integration always pays a full release compile. Open question: if the SMB tests don't truly need `-O`,
  switching to debug would let them reuse the test build. Left as-is pending a check that the timing-sensitive SMB tests
  still pass in debug.
- **`bindings-fresh` is content-hash cached** (not mtime) and runs the bindings export test in **debug** — the same
  build as `rust-tests`, so they share artifacts. On a warm tree with no `src-tauri` change it should return `<100 ms`
  ("cached"); a non-cached run means the marker in `target/` didn't persist (e.g. a `cargo clean`).

## svelte-tests coverage false-positive (rare, unpinned)

Seen once: a full-suite run failed `svelte-tests` with ~8 files below the 70% line-coverage gate (`clipboard-shim` 0%,
`apply-diff` 0%, `keyboard-shortcuts` 24%, `quick-look-state` 15%, …). All of them have dedicated, unconditional
`*.test.ts` files and read **96-100% in every normal run** — so it's not a real gap.

The check only reports "coverage below threshold" when vitest **exits 0** (tests passed) yet the coverage report is
incomplete — i.e. the run was incomplete, not the code undertested.

**Refuted hypothesis:** that CPU contention makes v8 drop/mis-merge coverage. Measured the 8 files' coverage (a)
standalone, (b) under 24 CPU hogs on 16 cores, (c) under a heavy cold-clippy compile + 12 hogs (CPU + memory pressure).
**All three: identical 96-100%, vitest exit 0, the usual 4 skipped.** So contention is NOT the trigger, and the
single-fork "fix" (`--no-file-parallelism`) is the wrong move — it would serialize ~338 files and tank the run for a
non-cause. Couldn't reproduce the failure at all; it's rare and the trigger is still unknown.

**What's in place:** the coverage-failure message now surfaces vitest's run tallies + any worker-death lines
(`vitestRunDiagnostics` in `desktop-svelte-tests.go`), so the next occurrence is self-diagnosing — the smoking gun will
be the `Test Files … | N skipped` count (N above the usual handful = files didn't run → coverage unreliable) or a
`Worker terminated` / `reached heap limit` line. Until then: a below-threshold file that has a dedicated test means
re-run `--check svelte-tests` standalone; don't allowlist it.

Render the graph with weights + lanes + median wall-time: `./scripts/check.sh --graph` (also
`--graph-format mermaid|dot`). The wall-time comes from recent passing runs in `~/cmdr-check-log.csv`, so the graph
doubles as a perf dashboard.
