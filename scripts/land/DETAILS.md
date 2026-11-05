# Land: details

Must-knows and usage: `CLAUDE.md`. Why `main` is gated at all: GitHub issue #475.

## The rulesets

The JSON in `.github/rulesets/` is the source of truth; `pnpm land rulesets -apply` creates or updates each one on
GitHub by name and leaves every other ruleset alone.

- `main-pr-required.json` ("main: PR with approval"): a PR with one approval, stale approvals dismissed, last-push
  approval. Bypass: the repository admin role (`RepositoryRole` 5, David) and the Renovate app (`Integration` 2740, the
  `renovate` app that opens and merges our Renovate PRs). These are the settings Scorecard's Branch-Protection tier 2
  looks for; the bypass actors cost only tier 5.
- `main-ci-required.json` ("main: CI OK required"): the `CI OK` check (job `ci-ok` in `.github/workflows/ci.yml`),
  pinned to the GitHub Actions app (`integration_id` 15368) so no other app can post a passing `CI OK`. `strict` is off,
  so GitHub merges a PR whose head is green even when `main` moved since (§ Decision: merge a green PR onto a moved
  `main`); that costs Scorecard's "up to date before merge". **No bypass actors**: nobody lands red, and `--admin` can't
  skip it. Releases need none either: the pipeline pushes nothing to `main`, and `release.sh` lands its commit through
  `pnpm land -merge` (`docs/guides/releasing.md` § Merging the release back into `main`).
- `main-no-force-push.json` ("main: no force-push or deletion"): no bypass, so nobody rewrites or deletes `main`.
- `release-tags.json` ("Release tags (v*)"): only the repository admin role creates, moves, or deletes a `v*` tag.
- `main-signed-commits.json` ("main: signed commits"): every commit reaching `main` carries a verified signature (§
  Decision: every commit on `main` is signed). Bypass: the repository admin role only, so a contributor's unsigned
  commits can still arrive through a `-record` land (that push reads "Bypassed rule violations"). Renovate needs no
  bypass: its squash merges are signed by GitHub.
- All five live here so a repo move or a lost setting can be restored with `-apply` (the first four exported from the
  live rulesets, 2026-10-22, where `pnpm land rulesets` reported all four matching).
- `pnpm land rulesets` compares each file with GitHub's copy and names only the ones that drifted. GitHub answers with
  defaults the files leave out, so a field missing from a file isn't drift.

Turning off the two `main` rulesets restores direct pushes, and `land` keeps working without them.

## Decision: land by pushing the green SHA when it fast-forwards

Decision: when the green head still descends from `origin/main`, `git push --no-verify origin <sha>:refs/heads/main`.
Why: it fast-forwards, so SHAs, signatures, tags, and local `main` stay identical to what CI tested, GitHub still marks
the PR merged, and `main`'s own CI run finds `CI OK` already green on the SHA and skips the suite (`verified` in
`ci.yml`).

Probe (2026-10-18, temporary rulesets mimicking A and B on `refs/heads/probe-land/main`, a throwaway workflow posting
`probe-ok`; all of it deleted afterwards):

- A SHA without the check, pushed by the admin: rejected, `GH013 ... Required status check "probe-ok" is expected`. So
  the CI ruleset holds against the admin.
- The same SHA once `probe-ok` was green on its PR: accepted, with
  `Bypassed rule violations: New changes require approval from someone other than the last pusher`. The PR (#477) turned
  `MERGED`, `mergeCommit` = the head SHA, `mergedBy` = vdavid.
- `github-actions[bot]` pushing with `GITHUB_TOKEN`: rejected by both rules. The GitHub Actions app can't be a bypass
  actor on this user-owned repo: the API answers 422 "Actor GitHub Actions integration must be part of the ruleset
  source or owner organization". A `DeployKey` bypass actor and the Renovate app are both accepted.

## Decision: the PR is plumbing

`land` opens the PR on the first push, because CI runs on `pull_request`, so every later push re-runs CI with no extra
trigger. Title: the oldest commit's subject. Body: every commit message. Nobody reads it; the commit messages are the
record. A closed PR is never revived: `land` opens a fresh one.

Gotcha/Why: GitHub refuses a PR body past 65,536 characters, and `gh pr create` then fails the whole land (a big
multi-agent batch with verbose messages reaches it). Past that, `prBody` lists the subjects under a note saying the full
messages are in the commits, and cuts even that list on a rune boundary if it has to. It measures BYTES, never fewer
than characters however GitHub counts them, so the fallback is never one character too long. Pinned in `state_test.go`.

## Decision: idempotent, no state of its own

Each run reads the branch and GitHub and picks up from there (`nextStartStep`, `ciVerdict`): an already-pushed branch
isn't pushed again, an open PR is reused, a `-squash` re-run on one commit is a no-op (a fresh SHA would restart CI),
and a branch already on `origin/main` skips to syncing `main` and deleting the remote branch. The pre-push hook may
commit a formatting fix and stop that push; `pushBranch` then pushes once more with the fix on top. Untracked files
don't count as dirty: they can't change what lands, and the main clone, where `release.sh` lands its release commit
from, often has some.

`scripts/release.sh` depends on this interface: `pnpm land -merge` from the main clone on the release branch, exit 0
only when the release commit is on `origin/main`'s history, and local `main` synced by `fetch origin main:main`. Change
it with the release flow (`docs/guides/releasing.md`).

## Decision: ask for a land run

Decision: a PR run is light (no Rust lanes, macOS, or Linux E2E; gate `CI light`) until `land` asks to land its head by
adding the PR's `land` label (`docs/tooling/ci.md` § Light runs and land runs). Why: most `-no-wait` milestone pushes
are never landed as they are, and each one used to pay the macOS lane's ~34 min and E2E's ~21.

- **Asking** (`askToLand`, `nextLandAsk`): no label adds it, which starts a `labeled` land run on the head. With the
  label on, a push already started one, so it waits; but when every run on the head is light (the label went on before
  this head, or its run never started), it takes the label off and on again. A PR this land opens carries only
  `claude-created` (`prCreateArgs`), so it gets the label here like any other: one `gh pr edit --add-label`, one
  `labeled` land run (§ Gotcha: never two labels on `gh pr create`).
- **`-no-wait` drops the label before it pushes** (`dropLandLabel`), so a milestone push after an earlier land stays
  light. A failed removal stops before the push, which would otherwise run every lane.
- **Waiting reads land runs only** (`landRuns`): a run titled `Light: ` (`ci.yml`'s `run-name`) never reports `CI OK`,
  so it's neither green nor red; with no land run listed yet the head reads as missing, and `appearWithin` bounds it. A
  title that lies only makes the wait follow the wrong run: a light run finished without `CI OK` reads red, never green.
- **Re-runs keep their mode** (a re-run keeps its event payload), so `gh run rerun --failed` on a land run is a land
  run. A newer push while the label is on is a land run too, which is what the conflict rebase needs.
- **`-record N`** opens the record PR the same way and asks the same way; its tree is `main`'s, so it still goes green
  in about a minute.
- Unit-tested (`TestNextLandAsk`, `TestCIVerdictIgnoresLightRuns`, `TestPRCreateArgsCarryOnlyTheAgentLabel`); the GitHub
  side on throwaway PR #978 (`docs/notes/ci-light-runs-2026-11-05.md`).

## Gotcha: never two labels on `gh pr create`

PR #982 (head `1484c2394048`, 2026-11-05): `land` opened it with `--label claude-created --label land`, and the issue
events API lists four distinct `labeled` events, all by `vdavid` with no app: both labels at 11:39:45Z and both AGAIN at
11:39:46Z, with no `unlabeled` between. With the `opened` event that made five CI runs, all created 11:39:48Z: two light
(the `claude-created` events, groups of their own) and three land runs in `pr-982` (the `opened` payload already carried
`land`, plus the two `land` events), which cancelled each other down to one. `ciVerdict` then broke the three-way
`created_at` tie toward a cancelled run and reported red with nothing failing (§ Decision: `CI OK` decides).

- **Not `land`, not gh's retries.** gh 2.103.3's
  [`CreatePullRequest`](https://github.com/cli/cli/blob/v2.103.3/api/queries_pr.go) sends `createPullRequest`, then ONE
  `updatePullRequest` with every `labelIds`; its HTTP stack doesn't retry, and `land`'s `r.gh` retries only on a spent
  budget, minutes later. No workflow or hook adds labels. Every earlier land-opened PR carried one label from the same
  path and got one `labeled` event (#954, #963, #965, #968, #976, #978), so the doubling showed up only with two labels
  in that mutation. GitHub's side, then; one sample, never reproduced (that would take a real PR and CI).
- **So `land` opens a PR with `claude-created` alone** (`prCreateArgs`) and adds `land` through `askToLand`'s
  `gh pr edit --add-label` (`addLabelsToLabelable`, one event per call on #976 and #978). The `opened` payload may still
  carry `land` if the edit beats GitHub's webhook build (#982's did, a second after `opened`), which makes two land runs
  in one second; `newestLandRun` follows the survivor.

## Decision: `CI OK` decides, not the run status

Decision: `ciVerdict` takes the newest CI run on the SHA and the newest `CI OK` check run of that run's current attempt,
and a completed one is the verdict whatever the run's status says. Why: GitHub can mark the run completed long after its
last job: PR #944's `CI OK` finished at 16:19:55Z, the run at 16:35:58Z (2026-11-04), and waiting for the run status
landed it 16 minutes late. `CI OK` is the exact check `main`'s CI ruleset requires, so it's safe to trust alone.

- **Only the current attempt's `CI OK` counts** (`checkRun.ofAttempt`). A `gh run rerun` reopens the run as a new
  attempt in the same check suite, and the earlier attempt's `CI OK` stays in the list (verified on run 41854674280's
  three attempts, `gh api .../check-runs`, 2026-11-05). So a check counts when it's in the run's suite
  (`check_suite_id`; the same SHA's other run, a release's dispatch run, has its own) and either still going or started
  no earlier than the attempt (`run_started_at`). An old green can't land a SHA whose re-run is going, and an old red
  can't stop one (`TestCIVerdictAcrossReRuns`).
- **The newest land run** (`newestLandRun`): by `created_at`, then the higher run ID, since `created_at` has one-second
  precision and one event burst can start several land runs in a second (§ Gotcha: never two labels on `gh pr create`).
  A cancelled newest gives way to a live run created in the same second, in case the IDs disagree with the order the
  concurrency group cancelled them in. A cancelled run with nothing as new beside it was stopped by a person: red, and
  `ciFailure` says it was cancelled and re-runs the whole run (`TestCIVerdictFollowsTheNewestOfABurst`).
- **No such check**: a finished run is red (`CI OK` never ran), an unfinished one pending. A queued `CI OK` on a
  finished run is pending (seen 2026-10-22).
- **Only `success` is green**: GitHub would accept a skipped check, which a finished run should never produce.
- **A red `CI OK` on a run still in progress** reports like an early stop: `gh run rerun --failed` works once the run
  finishes.
- `finish` may then cancel that same run as a leftover (GitHub still calls it in progress); its jobs are done, and
  `CI OK` stays green on the SHA.

## Decision: stop waiting at the first required failure

Decision: while the run is going, each poll also reads its jobs (`repos/{repo}/actions/runs/{id}/jobs?filter=latest`,
`runJobs`), and as soon as a job `CI OK` needs has ended `failure` or `timed_out` (`failedRequiredJobs`), the wait stops
with the usual red report: the failed jobs with links and `gh run rerun <id> --failed`, which works once the run
finishes. Why: `CI OK` runs only after every job, so run 40670839400 (2026-10-28) reported a type-aware ESLint failure
from minute four at minute 20.

- **The run isn't cancelled**: the other jobs keep going, so their failures still show on the PR, and one re-run of the
  failed jobs covers them all.
- **Required jobs come from head's own `ci.yml`** (`git show <head>:.github/workflows/ci.yml`, `requiredJobNames`): the
  `ci-ok` job's `needs`, mapped to each job's `name:` (else its ID), which is what the jobs API reports. A line-based
  read, the same layout `scripts/check/checks/ci-ok-gate.go` relies on. No hardcoded list: `ci-coverage` keeps those
  `needs` complete, so this follows it. No `ci-ok` job, or no readable `ci.yml`, prints a note and waits for `CI OK`
  alone.
- **Names match exactly, or as `<caller name> / <job>`** for a reusable workflow's jobs (the macOS lane, verified on run
  39684718106, 2026-10-28). Matrix legs (`Name (x)`) don't match: ci.yml has none, and a job outside the gate named that
  way would stop a land `CI OK` passes. A name holding `${{ }}` can't be matched, so that job only reports through
  `CI OK`.
- **Cancelled and skipped jobs aren't failures** (a lane skipped by change detection, a run a newer push replaced); the
  gate's own rules decide those.
- **Only the run's current attempt counts** (`run_attempt`): right after `gh run rerun --failed`, the list can still
  hold the red jobs that re-run replaces, so a re-run then `pnpm land` waits as before.
- **Cost**: one more call per poll at the same cadence, and an unchanged jobs list is a free 304 (`responseCache`). A
  failed jobs read skips only that poll's early look.

## Gotcha: the branch push must never overwrite commits only origin has

Gotcha: a `--force-with-lease` whose expected SHA is read (`ls-remote`) right before the push protects nothing: it
always matches, so origin's branch gets overwritten whatever it holds. It clobbered GitHub's "Update branch" merge on PR
#481 (2026-10-20). Why the current shape (`pushBranch`, `pushToOrigin`):

- Overwriting origin's branch is legitimate only when this branch already integrated what's there: a local rebase or
  `-squash` of commits it pushed earlier.
- `fetchBranch` updates `refs/remotes/origin/<branch>` (or deletes it when origin has no such branch, as after an
  earlier land). If that tip is neither an ancestor of `HEAD` nor in the branch's reflog, `pushBranch` stops with
  `errOriginAhead` and tells the user to `git pull --rebase` or `git reset --hard origin/<branch>`. Deciding this up
  front keeps the refusal typed, with no stderr matching.
- The push itself uses a bare `--force-with-lease=refs/heads/<branch>` (checks origin against the tracking ref) plus
  `--force-if-includes` (refuses a tracking tip the local branch never had), the backstop for a push landing between the
  fetch and ours. Git ignores `--force-if-includes` next to an explicit lease value, so never add one back.
- Tested against real bare origins in `push_test.go` (verified on git 2.56.0; removing `--force-if-includes` turns
  `TestPushToOriginRefusesAFetchedButUnintegratedTip` red, 2026-10-20).

## Decision: cheap polling that outlasts the API budget

Many sessions landing at once poll one account, and on 2026-10-20 they spent its 5,000/h REST budget: `gh run list`
answered 403 after CI was green, and that land was lost. So:

- **Three REST calls per poll at most**: the runs (`repos/{repo}/actions/workflows/ci.yml/runs?head_sha=`), the `CI OK`
  check runs once a run exists (§ Decision: `CI OK` decides, not the run status), and the run's jobs while it runs (§
  Decision: stop waiting at the first required failure). `gh run list --workflow` cost two calls (it resolves the
  workflow first).
- **Poll fast early, then back off** (`pollDelay`): every 5 s until the run shows up and through its attempt's first 50
  s, then a tenth of the attempt's age (a re-run restarts the clock), capped at 30 s from minute five, each plus up to
  25% jitter. No guess about the diff: a CI that skips every lane (about a minute) is seen within seconds, a 30-min one
  within half a minute, for about 90 polls.
- **Why that fits the budget**: what GitHub charges is bounded by how often the answers change, not by the cadence.
  While a run went, the runs list and the `CI OK` list each answered five 304s in a row after the first 200, and the
  jobs list one 200 in five polls (10 s apart, `X-RateLimit-Remaining` unmoved by the 304s, run 41945435107,
  2026-11-05). So a land costs about one charged call per job state change, well under 200, and ten lands at once stay
  far from 5,000/h. The 30 s cap keeps the request rate (304s included, which secondary limits may count) near six a
  minute per land; the 5 s start is about 36 a minute for one minute.
- **An unchanged poll is free** (`responseCache`, `ghbudget.go`): `ghAPI` sends the path's last ETag as `If-None-Match`,
  and GitHub answers an unchanged list with a 304 that it doesn't charge to the primary budget (verified 2026-10-21:
  `X-RateLimit-Remaining` stayed at 4,942 across two 304s on the workflow-runs list). Most polls of a running CI see
  nothing new. Gotcha: `gh` exits non-zero on a 304, so the parsed status decides, not the exit code.
- **A failed call waits out a rate limit** (`afterGHFailure`, `ghbudget.go`), then continues: it sleeps until a spent
  budget's reset, or a secondary limit's `Retry-After`. A 403 or 429 with budget left and no `Retry-After` is a
  secondary limit too: GitHub's docs say to wait at least a minute, then longer each time, so it backs off 1, 2, 4, and
  8 min (15 min in all) before giving up. That costs a real permission 403 the same 15 min, which no polled path
  answers. Reads otherwise get two short retries; writes (`gh pr create`, ruleset apply) get none, since they may have
  gone through.
- **A run that showed up never makes the wait give up** (`missingRunGivesUp`): on 2026-10-20 a queued run printed
  "Waiting for CI OK", then dropped out of the `head_sha` list, and the land quit with "no CI run after 5m0s". Only a
  push that started no run at all stops at `appearWithin`; a seen run waits out the usual timeout.
- **The budget comes from the failing response's own headers** (`gh api --include`, parsed by `net/http`, so no stderr
  matching). Gotcha: `gh api rate_limit` can disagree with them (2026-10-20: `core` showed 2,862 left while the Actions
  API answered `X-RateLimit-Remaining: 0`), so it's only the fallback for `gh pr` / `gh run` calls, which print no
  headers.

## Decision: merge a green PR onto a moved `main`

Decision: `land` rebases onto `origin/main` once, before the first push. When `main` moves while CI runs, it doesn't
rebase and wait again: `nextLandPath` picks `gh pr merge <n> --admin --merge --match-head-commit <green sha>`, GitHub
merges the PR into the current `main` with a merge commit, and `main`'s push run recognizes the merge of a green head
and skips the suite (`docs/tooling/ci.md` § Landing doesn't re-run the suite). It prints that it did, with a link to
`main`'s CI runs, and doesn't wait for them. Why a merge commit and never GitHub's rebase merge: § Decision: every
commit on `main` is signed.

Why: with many sessions landing at once, each land that saw `main` move rebased and re-ran CI (25–35 min), which moved
nobody forward. On 2026-10-20 eight-plus PRs went green two or three times each and none landed. Big teams accept the
same trade: patches that passed CI on a slightly older base. A rare semantic clash between two such PRs surfaces on the
next PR's run (it tests the PR merged with the current `main`) and on the release gate's full run, not on `main`'s own.
The guarantee that stays: `--admin` only bypasses the approval ruleset, the CI ruleset has no bypass and `strict` is
off, so GitHub still demands `CI OK` green on the PR's head SHA, and `--match-head-commit` pins that head.

- **A conflict falls back to a rebase here**: GitHub's `mergeable` on `repos/{repo}/pulls/{n}` (null for a few seconds
  while GitHub works it out, so `prMergeable` waits up to 15 s, then tries anyway). False, or a refused merge that reads
  false afterwards, returns `errGitHubConflict` (`conflictError`); `land` rebases here and waits for CI again, up to
  `maxCIRuns` (8) runs, so a `main` that keeps clashing can't spin a land all day. The decision reads that typed field,
  never gh's stderr.
- **A refused merge re-reads the PR**: `gh` waits out a spent API budget and repeats the call, so a merge that went
  through the first time reads `merged` and counts as landed.
- **A re-run after a GitHub merge finishes**: the branch's head is an ancestor of `origin/main`, so `readState` reports
  it landed (`TestLandedAfterAGitHubMergeCommit`). It also counts a branch whose every patch is on `main` under other
  SHAs (`git cherry origin/main HEAD` lists only `-` lines): a cherry-pick, or an older land through GitHub's rebase
  merge.
- **The fast-forward still wins when it can**: it keeps `main` linear and exactly the commits CI tested.
- **History isn't linear**: `main` carries GitHub's "Merge pull request #N" commits. Readers of `main`'s history skip
  them (`git log --no-merges` for the CHANGELOG, `docs/guides/releasing.md` § Writing the CHANGELOG; `--first-parent`
  for `main`'s own states, `scripts/loc-counter/`).

Exception: a branch carrying a merge commit (a security-only patch merged back by hand, `docs/guides/releasing.md`) is
never rebased, by `land` or by GitHub, since a plain rebase flattens the merge. GitHub's merge commit lands it like any
branch; only a conflict, which would need a rebase here, makes `land` stop and name the manual fix. A contributor PR's
record never rides a branch (§ Decision: record a contributor PR in its own land). A tagged commit isn't rebased either
(§ Decision: `-merge` keeps a release commit's SHA).

## Decision: every commit on `main` is signed

Decision: everything `land` puts on `main` carries a verified signature, so `main` can require signed commits. David
picked a merge commit on a moved `main` over a local rebase plus another CI run (2026-10-30): no extra 25–35 min, at the
cost of a non-linear history.

- **The fast-forward** pushes David's own commits, signed locally (`commit.gpgSign`, SSH key).
- **A GitHub merge commit** is signed by GitHub's web-flow key, and the branch's commits keep their SHAs and David's
  signatures (verified on cli/cli's merge commits, `committer: GitHub`, `verification.verified: true`, 2026-10-30).
- ❌ **Never `--rebase` or `--squash`**: GitHub's rebase merge rewrites every commit unsigned (`04f1230654`, PR #792;
  369 such commits on `main` between 2026-10-01 and 2026-10-30), and a required-signatures rule refuses it outright.
- **The commits `land` makes itself**: the start rebase and the conflict rebase (`git rebase`), `-squash`
  (`git commit`), and the pre-push hook's formatting commit all honor `commit.gpgSign`. `git commit-tree` doesn't
  (verified on git 2.56.0, 2026-10-30), so `buildRecord` passes `-S` itself when it's on
  (`TestBuildRecordSignsWhenCommitsAreSigned`).
- **A contributor's record brings their commits onto `main`** as the `ours` merge's second parent, and GitHub checks
  every commit a push adds to `main`, so an unsigned contributor commit blocks the record under a required-signatures
  rule. The signatures ruleset lets the admin bypass it for exactly this (David, 2026-10-30): the Merged badge is worth
  more than a fully signed history.

## Decision: `-merge` keeps a release commit's SHA

Decision: `pnpm land -merge` never rebases. It skips the start rebase, and lands like any green head (a fast-forward,
else GitHub's merge commit onto whatever `main` is by then), but a conflict stops it (`landMergeConflict`) instead of
rebasing here. A plain land refuses to rebase a branch carrying a tagged commit (`stepRefuseTaggedRebase`,
`runner.tagged`) and points at `-merge`.

Why: a release is tagged on its `chore(release)` commit before it reaches `main`, and that commit is the one its full CI
run tested and the pipeline built (`docs/guides/releasing.md` § The CI gate). A rebase would give `main` a different
SHA, leaving the tag off `main`'s history. A merge commit made locally can't land either: `main` takes about eight lands
an hour against a 25–35 min CI, so it'd rebuild and re-run forever (the reason `-record` exists). GitHub's merge needs
no rebuild (§ Decision: merge a green PR onto a moved `main`).

- **The CI wait reads only the PR's runs** (`ciRunsPath`): the release commit also has its release's full dispatch run,
  and the CI ruleset reads only the newest run on a commit, which is the PR's once it starts. Landing on the older green
  run would be refused while the PR run goes.
- **A conflict stops it** (`mergeable` false, `errMergeConflict`): merge by hand in a worktree off `main`, then `land`
  that. Rare, since a release commit touches only version files, `Cargo.lock`, `LICENSE`, and release-prep files.
- **Needs merge commits allowed on `main`**, like every land onto a moved `main`: the repo's `allow_merge_commit` and
  `merge` in `main-pr-required.json`'s `allowed_merge_methods`. Without either, GitHub refuses the merge and land says
  so; the fast-forward works regardless.
- Unit-tested (`nextStartStep`, `nextLandPath`, `conflictError`, `tagged` against a real repo); the GitHub side waits
  for the first real release merge-back.

## Decision: record a contributor PR in its own land

Decision: `pnpm land -record N` lands the cherry-picked branch exactly like `pnpm land`, then lands PR N's record
separately: a `record-pr-N` branch holding the current `origin/main` plus an `ours` merge of `refs/pull/N/head`,
fast-forwarded onto `main` once `CI OK` is green on it (`record.go`).

Why: a merge commit can't ride a rebase, and `main` gets about eight lands an hour against a 25–35 min CI, so a content
branch carrying the record almost never found `main` still under it, and `land` refused it. PR #393 (2026-10-21) needed
a manual `git rebase --rebase-merges`, which replayed the `ours` merge as a conflicting real merge that had to be redone
by hand. Split off, the record's tree equals `main`'s, so CI skips every lane, the light lane's hygiene included
(`docs/tooling/ci.md` § The light lane), and goes green in about a minute: the window `main` has to stay still shrinks
from half an hour to one minute.

- **Built, never rebased**: `git commit-tree <main>^{tree} -p <main> -p <PR head>`, which is what
  `git merge -s ours --no-ff` makes, with no checkout. Before the push, `verifyRecord` checks the tree equals `main`'s
  and that both `main` and the PR head are ancestors.
- **`main` moved under its CI**: `landRecord` returns `errMainMoved` (no GitHub merge: the record's tree must be
  `main`'s), and `record` rebuilds on the new `main`, up to `maxCIRuns` runs, like a phase-1 conflict.
- **Idempotent like the rest** (`nextRecordStep`): PR N `MERGED` says so and stops; `CLOSED` is refused (GitHub marks
  only an open PR merged); a PR head already on `main` skips to the wait for `MERGED` (up to a minute) and the tidy-up.
  A record origin already has for the exact current `main` (same parents, same tree) is reused, so a re-run doesn't push
  a fresh SHA and restart its CI. Phase 1 finds its branch landed and passes straight through.
- **Tidy-up**: deletes the record branch on origin, stops its leftover runs, and drops the `refs/land/pr-N` and tracking
  refs. The record PR itself turns merged with the fast-forward.
- Tested against bare origins with a pushed `refs/pull/N/head` (`record_test.go`); the GitHub side (PR states, the
  record PR's CI) waits for the first real contributor PR.

## Decision: stop the landed branch's leftover runs

GitHub keeps a PR's unfinished runs going after it merges and the branch is deleted (an older push's CI, CodeQL). On
2026-10-20 two landed PRs' runs held two of the account's five macOS runners for up to 40 min while eight agents queued.
So `finish` cancels every unfinished run on the branch (`leftoverRuns`: this repo's, not a fork's namesake).

- **Here, not in a workflow on `pull_request: closed`**: that would cover every way a PR closes, but it needs
  `actions: write`, a Scorecard Token-Permissions finding. `land` cancels with the user's own `gh` token. Renovate's
  automerge isn't covered, but it merges only on green, so it leaves little running.

## Open points

- **Renovate automerge**: Renovate uses GitHub's native auto-merge (`platformAutomerge`). Whether GitHub honors an app's
  bypass for auto-merge is unverified; if Renovate PRs stall on the approval rule, set `platformAutomerge: false` so
  Renovate merges through the API itself. Its merges so far are GitHub squash commits, signed by GitHub (PR #485,
  `3b903143a`, `committer: GitHub`, verified, 2026-10-30), so they pass a required-signatures rule; keep `squash` in
  `allowed_merge_methods` for it.
