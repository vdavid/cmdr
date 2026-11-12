# Light runs and land runs: live validation (2026-11-05)

The record behind `docs/tooling/ci.md` § Light runs and land runs: how GitHub treats the gate's two names, and what each
kind of run did on throwaway PRs. All throwaway PRs, branches, and rulesets were deleted afterwards.

## How GitHub treats the gate's name (probe PR #976)

A throwaway workflow mirrored the design: job `a` slept 75 s and wrote `land` from the PR's labels, and the gate job
`name: ${{ needs.a.outputs.land == 'true' && 'PROBE OK' || 'PROBE light' }}` ran on `always()`. A temporary ruleset
required `PROBE OK` from GitHub Actions (`integration_id` 15368) on `refs/heads/probe-ruleset/**`.

- **No early check run under another name**: while `a` ran, the SHA's check runs listed `a` but no gate check at all;
  the gate's check run appeared when it was queued, already named `PROBE light` or `PROBE OK`. A skipped job does get a
  check run (`Full run (run_all)` read `skipped` on the same SHA), which is why the gate never skips.
- **The `labeled` payload carries the new label**: the run started by adding `land` read it in
  `github.event.pull_request.labels` and named its gate `PROBE OK`. Another label's run (`claude-created`) didn't.
- **An empty `run-name` falls back to GitHub's default title** (the PR title); a non-empty one is used as is.
- **A light gate never satisfies the required check**: pushing `e73d0b23` (only a green `PROBE light`) to the protected
  branch was refused: `GH013: Repository rule violations found ... Required status check "PROBE OK" is expected.`
- **An older light run's later re-run doesn't shadow a green land gate**: `a01bfb33` carried a green `PROBE OK`, then a
  re-run of a light run on it finished later with a green `PROBE light`; pushing it was accepted. That light run
  (41977787817, created 06:02:37Z) was OLDER than both `PROBE OK` runs (41977824020 at 06:02:58Z, 41978008805 at
  06:04:41Z; `gh api .../actions/runs?head_sha=`, read 2026-11-12), so this never tested a light run made after the land
  run. One made after it does hide the `CI OK`: `scripts/land/DETAILS.md` § Gotcha: GitHub reads only the newest run on
  a commit.
- One push that created a branch seconds after the ruleset was created went through without the check; later creations
  were refused like updates, so it reads as the ruleset taking a moment to apply.

## The real workflow (PR #977, and throwaway PR #978 stacked on it)

PR #978 added one comment line to `crates/cmdr-update-trust/src/lib.rs` on top of #977's branch. A second temporary
ruleset required the real `CI OK` (integration 15368) on `refs/heads/probe-ruleset/**`.

1. **The milestone push, light** (run 41978949827, no label): `Detect changes`, the light lane (both jobs), and the gate
   ran; `Desktop (Rust)`, `Desktop (Rust, fixtures)`, `Desktop (Rust, macOS)`, `E2E base image`, and
   `E2E tests (Playwright, Linux)` were skipped. The gate reported `CI light` (success), 3 min 51 s wall. The
   `claude-created` label's run (41978949739) checked nothing and reported `CI light` in 57 s, in its own group.
2. **The PR can't merge on it**: pushing that head to the protected branch was refused with
   `GH013 ... Required status check "CI OK" is expected`, both before and after its `CI light` went green; so was PR
   #977's own head (`b3ebbc06`, a green `CI light` only).
3. **The land request** (adding `land`, run 41979431517, titled with the plain PR title): every heavy lane ran, the Rust
   lanes scoped to `cmdr cmdr-update-trust`. Rust 20 min, fixtures 13.5, E2E 21, macOS 38.6 (the critical path); `CI OK`
   green at 39 min 20 s. The same head then pushed to the protected branch.
4. **A docs-only milestone after it** (label off, run 41983896099): light, 1 min 38 s, `CI light` green.
5. **The second land request** (label on again, run 41984106123): the plan found `desktop-rust`, `desktop-rust-macos`,
   and `desktop-e2e-linux` "these exact inputs already passed (marker on refs/pull/978/merge)", they skipped, and
   `lane-markers verify` proved all six skips. `CI OK` green at about 11 min. `desktop-rust-fixtures` ran again (13 min)
   with the very key it had saved at 06:31: the marker was already gone. The repo's caches sat at 9.52 GB of the 10 GB
   cap, and LRU evicts markers first since nothing ever restores them (`scripts/lane-markers/DETAILS.md` § Store
   choice). Correct (an eviction only reruns), but under cache pressure it eats what light runs save.

## Cost

A milestone push that touches Rust now costs a light run (2–4 min, two hosted jobs) instead of the ~40 min, four-lane
heavy set with one macOS runner. The land request pays the heavy lanes once, and later land requests on the same inputs
skip them while their markers survive.
