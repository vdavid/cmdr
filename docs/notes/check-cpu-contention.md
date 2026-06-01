# Check CPU contention sweep + weights

Why some `--include-slow` runs flaked (E2E modal/popover timeouts, an 8s-cap Rust test) traced back to the check runner
firing up to `NumCPU` checks at once, each itself multi-threaded, so the short CPU-heavy checks piled on top of each
other and oversubscribed the machine 2-3×. This note records the per-check CPU measurement that calibrated
`CheckDefinition.CpuWeight` and the weighted admission gate in `runner.go`.

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

| weight | checks                                                                                                   |
| -----: | -------------------------------------------------------------------------------------------------------- |
|     11 | svelte-tests                                                                                             |
|      8 | clippy, cargo-udeps, bindings-fresh, integration-tests                                                   |
|      7 | nilaway                                                                                                  |
|      6 | rust-tests, rust-tests-linux, website-e2e                                                                |
|      4 | deadcode, e2e-linux, e2e-playwright                                                                      |
|      3 | govulncheck                                                                                              |
|      2 | jscpd, svelte-eslint, eslint-typecheck, svelte-check, website-{typecheck,build,docker-build}, api-eslint |
|      1 | cargo-audit, cargo-deny, website-eslint, everything unset (fast formatters/scanners)                     |

The runner admits a check only when `sum(running weights) + weight ≤ NumCPU` (a weight-0/unset check counts as 1; an
over-budget check runs alone). Net effect: wall-clock stays bounded by the critical path (`eslint-typecheck` under
`--include-slow`, `clippy`-cold for the default suite) while peak oversubscription drops from ~2-3× to ~1×.
Fast/unmeasured checks default to 1 and can be recalibrated later if the fast lane ever shows contention.

Render the graph with weights + lanes: `./scripts/check.sh --graph` (also `--graph-format mermaid|dot`).
