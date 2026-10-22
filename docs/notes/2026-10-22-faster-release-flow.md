# Faster release flow (2026-10-22)

The release flow rebuilt so the build runs while one CI run tests the exact commit that ships, and nothing ships until
that run is green. The canonical description is `docs/guides/releasing.md` (§ The release procedure, § The CI gate, §
Merging the release back into `main`); this note is the record of what changed, why, how it was verified, and what's
still open.

## Why

v0.55.0 (2026-10-21) took ~3 h from David's "go" to shipped, and ~2 h 10 min even without hiccups:

- `release-ci-gate.sh`: a full `run_all` CI run on `main` HEAD (~20 min). Then `release.sh` landed the version bump
  through `pnpm land`: a second CI run, the PR run on the release commit (39 min). Only then the signed tag, and the tag
  push started the ~60 min build. Strictly sequential: gate, PR run, build.
- `main` moved twice under the gate (other sessions land all the time), and each move restarted it (~33 min wasted).
- A fixture job went red 25 min before the gate's run ended, but the gate only reported when the whole run ended.
- `release.sh` aborted twice on things a check would have caught: the prep edits stay uncommitted, so no hook sees them.

## What changed

1. **Release from a tested commit, not from `main` HEAD.** `release.sh` commits `chore(release): vX.Y.Z` (R) on
   `release/vX.Y.Z` on top of `origin/main` (X), signs the tag on R itself, and pushes both. `main` keeps moving freely;
   nothing locks it. R reaches `main` only after the release is published, through `pnpm land -merge` (never rebased, so
   the tag stays on `main`'s history).
2. **One CI run per release**: `release.sh` dispatches `ci.yml` (`run_all`) and `codeql.yml` on the release branch, so
   their `head_sha` is R. That one run replaces both the gate on X and the PR run on R.
3. **Build early, ship only on green.** In `local` mode the tag goes out right away and builds into the private draft
   beside CI. `release-finish.sh` waits for the build AND R's CI together and won't sign until CI and CodeQL are green.
   The pipeline's `ci-gate` job moved: it no longer blocks `draft`/`compile`/`sbom` in `local` mode, and runs in
   whichever run publishes (the `local` finishing dispatch, before `publish`, which now needs it; or a `ci` mode tag
   push, before `compile`). `ci` mode keeps the old gate-first order: `release.sh` waits for green CI before pushing a
   `ci` mode tag, since that push publishes on its own.
4. **The gate fails fast.** One implementation, `scripts/release-finish/ci.go`, prints each job as it moves and stops at
   the first red job with the job name, a log command that works mid-run, and the two ways on (re-run a flake, or
   `release.sh -abort`). `release-ci-gate.sh X.Y.Z` is now a thin wrapper for the waiting form; the pipeline's `ci-gate`
   calls `release-finish.sh -ci-gate` instead of its own copy of the rules.
5. **The prep gets checked before "go"**: step 5 of the procedure runs `pnpm check -m` (docs only).

Also built:

- `release.sh -abort X.Y.Z`: stops the release's runs, deletes the build artifacts, the draft, the tag, and the branch
  (GitHub and local), and hands the release-prep edits back as uncommitted edits (changelog under `[Unreleased]` again).
  Refuses on a published release.
- `release.sh -test X.Y.Z-pipelinetest.N`: the whole flow off the current branch's HEAD, for pipeline tests; refuses
  unless `RELEASE_UPDATE_SIGNING=local`. It writes a one-line changelog section (the What's new smoke test pins the
  changelog's newest entry to the app version) and stamps the settings-defaults manifest without an entry.
- `pnpm land -merge`, and a plain land now refuses to rebase a tagged commit.
- `release-codeql-gate.sh` takes the one commit whose CI counted; an unfinished run there fails rather than passing the
  turn to another commit.

### Which commit's run counts

R's own full run whenever R has one: a red or running run on R holds the release however green its parent is. R's
parent's only when R is the `chore(release)` commit and has no full run at all. That keeps the hand-made security-only
patch flow working unchanged (it gates the fix commit, then bumps on top by hand), and it means the main flow can't slip
through on a green parent: `release.sh` always dispatches R's run. Server side and on the laptop it's the same Go code.

## The new flow, with expected timings

From David's "go" (procedure step 7), `local` mode:

1. `release.sh X.Y.Z`, ~2–5 min: pull, changelog-links, the bumps, the settings-defaults stamp, visual baselines
   (Docker), oxfmt, strict i18n-stale, commit R, sign the tag, push the branch, dispatch CI and CodeQL, push the tag.
   (The test-mode run took 70 s end to end.)
2. In parallel:
   - The tag push's build: ~60 min (v0.55.0: universal compile 52 min, aarch64 33, x86_64 27, then sign ~5, pkg/vet/
     attest ~3). The parallel lipo work brings the build to roughly 35–40 min.
   - R's full CI run: 25–40 min (the macOS lane is the long pole), CodeQL 12–20 min.
3. `release-finish.sh`: waits for both (fail fast on CI), CodeQL gate, provenance, vet, sign, upload: ~2 min after the
   later of the two.
4. The finishing run (`ci-gate`, vet, publish, attest, bump-tap): ~3 min.
5. Shipped. Then `release.sh X.Y.Z` again merges R back (a PR run, ~30 min, blocking nothing users get).

Go to shipped: ~65–70 min today (the build is the long pole), ~45 min once the lipo change lands (CI becomes the long
pole). Before: ~2 h 10 min without hiccups, ~3 h with v0.55.0's.

## Verification

All on GitHub with `RELEASE_UPDATE_SIGNING=local`, pre-release tags only, nothing published, `main` untouched.

- **Gate logic on a real old release**: `release-finish.sh -ci-gate 0.55.0` took the parent's full run (old flow, R had
  none) and the CodeQL gate, green.
- **`v0.55.1-pipelinetest.1`** (R `07cc1bd08`): `release.sh -test` pushed branch and tag, dispatched CI run 39739624070
  and CodeQL run 39739638822. The build run 39739661545 started at once with `ci-gate` skipped and `draft`, `sbom`, and
  all three `compile` jobs running while CI ran (the early build). `release-finish.sh -dry-run` refused at 07:21 UTC:
  "the full CI run on 07cc1bd08 is still going". CI then went red for real: the What's new smoke test needs a changelog
  section for the app version, which test mode didn't write yet (fixed in `e22c6a004`), plus a timeout in a `cmdr-index`
  test under load. `release.sh -abort` then cancelled all three runs, deleted the SBOM artifact, the draft, the tag
  (GitHub and local), and the branch.
- **Fail fast** (version suffix `.3`, CI only, no tag pushed): a commit with a deliberately misformatted file on top of
  R, dispatched as CI run 39739972274. `release-ci-gate.sh 0.55.1-pipelinetest.3` printed jobs as they moved and stopped
  after 2 min 5 s on "Repo hygiene: failure" while the Rust lanes were still running, naming the job, its log, the
  re-run command, and the abort command. `release-finish.sh -dry-run` refused on the red run too. That run showed
  `gh run view --log-failed` refusing mid-run, so the hint now uses the jobs logs API (checked live: it printed the
  oxfmt failure naming the file). `release.sh -abort` cancelled the run and deleted the branch and local tag.
- **`v0.55.1-pipelinetest.4`** (R `6545a2e4a`): CI run 39742222240, CodeQL run 39742237296, build run 39742260113.
  RESULTS PENDING.

## Found and fixed on the way

- **`website-bundle-size` ratcheted its baseline to 0 B after every release.** `update-visual-baselines.sh` mounts a
  container volume over `apps/website/dist`, which leaves an empty `dist/` on the host; the next `pnpm check` read it as
  a 0 B build and rewrote the committed baseline, and `html-validate` failed on no files. Both now skip an empty `dist/`
  (`cad3ecb93`).
- **An abort would have lost the drafted changelog**, since the prep edits live in R once committed. `-abort` puts them
  back.

## Skipped, and open questions

- **Merge-back needs David's call**: merge commits are off on the repo (`allow_merge_commit: false`, and the "main: PR
  with approval" ruleset allows squash and rebase only), so `land -merge` can fast-forward but GitHub refuses the merge
  commit when `main` moved, which it nearly always will have. Options: (a) allow merge commits (repo setting plus
  `merge` in `allowed_merge_methods`, `pnpm land rulesets -apply`); (b) keep them off and land R's diff as a cherry-pick
  through a normal land, then record R with an `ours` merge via the `-record` mechanics (works today, but the release
  commit shows up twice in `main`'s history). Built for (a); nothing on GitHub was changed. The GitHub side of
  `land -merge` is unit-tested only (no pipeline test can touch `main`).
- **The merge-back PR runs CI once more** (~30 min, including the macOS lane), testing R merged with the current `main`.
  It blocks nothing users get, but it costs runner minutes. A possible follow-up: `ci.yml`'s `verified` step (today
  push-only) could also skip the lanes on a PR whose head SHA already has a green full dispatch run, the same trust as
  the existing post-land skip. Not built: it changes a security-relevant gate, so it wants David's yes.
- **A commit landing between the changelog draft and `release.sh` ships unlisted.** True before too (`release.sh`
  pulled); the changelog draft cites what was on `main` at drafting time.
- **`ci` mode is implemented but not exercised**: `release.sh` waits for green CI before pushing a `ci` mode tag, and
  `ci-gate` runs before `compile` in that run. Same Go code and job conditions as `local` mode, but no live run.
- Steps 12–13 of the procedure (post-release checks and CRA evidence) belong to the parallel effort. Their evidence
  lookup changes: the CI and CodeQL runs are now on `vX.Y.Z^{commit}` itself, not its parent (except a hand-made
  security-only release).
