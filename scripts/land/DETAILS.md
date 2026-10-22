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
- Two older ones stay as they are: "main: no force-push or deletion" and "Release tags (v*)".
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

`ciVerdict` takes the newest CI run on the SHA (a `gh run rerun` reopens it, so an in-flight re-run reads as pending,
not red) and the newest `CI OK` check run. Only `success` is green: GitHub would accept a skipped check, which a
finished run should never produce.

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

- **One REST call per poll** (`repos/{repo}/actions/workflows/ci.yml/runs?head_sha=`), plus the `CI OK` check runs only
  once the run finished. `gh run list --workflow` cost two calls (it resolves the workflow first).
- **Poll by run age** (`pollDelay`): 20 s until the run shows up, every 3 min while it's younger than 15 min (CI takes
  25–35 min), then every minute, each plus up to 25% jitter. About 15–25 calls per CI run, down from ~180 (three calls
  every 30 s).
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
rebase and wait again: `nextLandPath` picks `gh pr merge <n> --admin --rebase --match-head-commit <green sha>`, GitHub
rebases the PR onto the current `main`, and `main`'s push run recognizes the merge of a green head and skips the suite
(`docs/tooling/ci.md` § Landing doesn't re-run the suite). It prints that it did, with a link to `main`'s CI runs, and
doesn't wait for them.

Why: with many sessions landing at once, each land that saw `main` move rebased and re-ran CI (25–35 min), which moved
nobody forward. On 2026-10-20 eight-plus PRs went green two or three times each and none landed. Big teams accept the
same trade: patches that passed CI on a slightly older base. A rare semantic clash between two such PRs surfaces on the
next PR's run (it tests the PR merged with the current `main`) and on the release gate's full run, not on `main`'s own.
The guarantee that stays: `--admin` only bypasses the approval ruleset, the CI ruleset has no bypass and `strict` is
off, so GitHub still demands `CI OK` green on the PR's head SHA, and `--match-head-commit` pins that head.

- **A conflict falls back to the old path**: GitHub's `rebaseable` on `repos/{repo}/pulls/{n}` (null for a few seconds
  while GitHub works it out, so `prRebaseable` waits up to 15 s, then tries anyway). False, or a refused merge that
  reads false afterwards, returns `errRebaseConflict`; `land` rebases here and waits for CI again, up to `maxCIRuns` (8)
  runs, so a `main` that keeps clashing can't spin a land all day. The decision reads that typed field, never gh's
  stderr.
- **A refused merge re-reads the PR**: `gh` waits out a spent API budget and repeats the call, so a merge that went
  through the first time reads `merged` and counts as landed.
- **A re-run after a GitHub rebase merge finishes**: the branch's SHAs aren't on `main`, but
  `git cherry origin/main HEAD` lists only `-` lines (every patch is upstream), so `readState` reports it landed.
  `remove-worktree.sh` asks the same question before tearing a worktree down.
- **The fast-forward still wins when it can**: it keeps SHAs identical, so `main` gets exactly the commits CI tested.

Exception: a branch carrying a merge commit (a security-only patch merged back by hand, `docs/guides/releasing.md`) is
never rebased, by `land` or by GitHub, since a plain rebase flattens the merge. When `main` moved, `land` stops and
names the manual fix instead. A contributor PR's record never rides a branch (§ Decision: record a contributor PR in its
own land). A tagged commit isn't rebased either (§ Decision: `-merge` keeps a release commit's SHA).

## Decision: `-merge` keeps a release commit's SHA

Decision: `pnpm land -merge` never rebases. The green head lands by fast-forward while it still descends from
`origin/main`, else with `gh pr merge <n> --admin --merge --match-head-commit <sha>` (`nextMergePath`): GitHub makes a
merge commit onto whatever `main` is by then. A plain land refuses to rebase a branch carrying a tagged commit
(`stepRefuseTaggedRebase`, `runner.tagged`) and points at `-merge`.

Why: a release is tagged on its `chore(release)` commit before it reaches `main`, and that commit is the one its full CI
run tested and the pipeline built (`docs/guides/releasing.md` § The CI gate). A rebase would give `main` a different
SHA, leaving the tag off `main`'s history. A merge commit made locally can't land either: `main` takes about eight lands
an hour against a 25–35 min CI, so it'd rebuild and re-run forever (the reason `-record` exists). GitHub's merge needs
no rebuild, like the rebase path of § Decision: merge a green PR onto a moved `main`.

- **The CI wait reads only the PR's runs** (`ciRunsPath`): the release commit also has its release's full dispatch run,
  and the CI ruleset reads only the newest run on a commit, which is the PR's once it starts. Landing on the older green
  run would be refused while the PR run goes.
- **A conflict stops it** (`mergeable` false, `errMergeConflict`): merge by hand in a worktree off `main`, then `land`
  that. Rare, since a release commit touches only version files, `Cargo.lock`, `LICENSE`, and release-prep files.
- **Needs merge commits allowed on `main`**: the repo's `allow_merge_commit` and `merge` in `main-pr-required.json`'s
  `allowed_merge_methods` (David chose this over a cherry-pick plus an `ours` record, which would put the release commit
  on `main` twice, 2026-10-22). Without either, GitHub refuses the merge and land says so; the fast-forward works
  regardless. The PR page's button offers merge commits too, which is one more reason land owns merging.
- Unit-tested (`nextStartStep`, `nextMergePath`, `tagged` against a real repo); the GitHub side waits for the first real
  release merge-back.

## Decision: record a contributor PR in its own land

Decision: `pnpm land -record N` lands the cherry-picked branch exactly like `pnpm land` (linear, no merge commit), then
lands PR N's record separately: a `record-pr-N` branch holding the current `origin/main` plus an `ours` merge of
`refs/pull/N/head`, fast-forwarded onto `main` once `CI OK` is green on it (`record.go`).

Why: a merge commit can't ride a rebase, and `main` gets about eight lands an hour against a 25–35 min CI, so a content
branch carrying the record almost never found `main` still under it, and `land` refused it. PR #393 (2026-10-21) needed
a manual `git rebase --rebase-merges`, which replayed the `ours` merge as a conflicting real merge that had to be redone
by hand. Split off, the record's tree equals `main`'s, so CI skips every lane, `hygiene` included (`docs/tooling/ci.md`
§ The hygiene job), and goes green in about a minute: the window `main` has to stay still shrinks from half an hour to
one minute.

- **Built, never rebased**: `git commit-tree <main>^{tree} -p <main> -p <PR head>`, which is what
  `git merge -s ours --no-ff` makes, with no checkout. Before the push, `verifyRecord` checks the tree equals `main`'s
  and that both `main` and the PR head are ancestors.
- **`main` moved under its CI**: `landRecord` returns `errMainMoved` (no GitHub rebase: it'd flatten the merge), and
  `record` rebuilds on the new `main`, up to `maxCIRuns` runs, like a phase-1 conflict.
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
  Renovate merges through the API itself.
