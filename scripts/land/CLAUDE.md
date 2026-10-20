# Land

`pnpm land` takes a worktree branch onto `main` through CI. Once the rulesets in `.github/rulesets/` are on, it's the
only way commits reach `main`: nobody, admin included, lands a SHA without a green `CI OK`.

## Module map

- `main.go`: the flow (push, PR, wait, land, sync), every step skipped when it's already done.
- `state.go`: the pure decisions (start step, CI verdict, messages, worktree parsing), tested in `state_test.go`.
- `exec.go`: git and gh plumbing; the branch push is tested against real bare origins in `push_test.go`. `ghbudget.go`:
  when a failed gh call waits out a spent API budget. `rulesets.go`: `pnpm land rulesets [-apply]`.

## Usage

- `pnpm land`: rebase onto `origin/main` if it moved, push, open or reuse the PR, wait for `CI OK`, land, sync local
  `main`, delete the remote branch, stop its leftover runs, and print the worktree's teardown command.
- `remove-worktree.sh` counts a branch as merged when `git cherry origin/main` finds every commit upstream, so it tears
  down after a GitHub rebase merge too.
- Every PR it opens carries the `claude-created` label (`agentLabel`); a reused PR, like a contributor's, keeps its own.
- `pnpm land -no-wait`: push and open the PR, then return. Run it after each green milestone and keep working.
- `pnpm land -squash [-m MESSAGE]`: squash first (default message: every commit message, oldest first).
- `pnpm land rulesets`: print what `-apply` would create or update on GitHub.

## Must-knows

- **A green head lands one of two ways.** Still on top of `origin/main`: `git push origin <sha>:main`, SHAs identical.
  `main` moved while CI ran: `gh pr merge --admin --rebase --match-head-commit <sha>`, and CI on `main` catches the rare
  semantic clash. Either way the CI ruleset demands `CI OK` green on the head (`DETAILS.md` § Decision: merge a green PR
  onto a moved `main`).
- **Re-run it at any stage.** A red CI prints the failed jobs and `gh run rerun <id> --failed`. Only when GitHub can't
  rebase the green PR (a conflict) does it rebase here and run CI again, eight runs at most. A branch GitHub already
  rebased onto `main` counts as landed (`git cherry`).
- **It never rebases a branch carrying a merge commit** (a contributor's PR record, a release merge-back), locally or on
  GitHub: when `main` moved, it stops and names the manual fix, since a plain rebase flattens the merge.
- **It never overwrites commits only origin's branch has** (another machine, GitHub's "Update branch"): it stops and
  says how to bring them in. Keep the bare lease plus `--force-if-includes` (`DETAILS.md` § Gotcha: the branch push).
- ❌ **Don't use the PR page's merge button.** `pnpm land` is the way to land: it picks the fast-forward when it can (a
  verified `main` push skips the suite), handles merge-commit branches, syncs local `main`, and tidies the branch.
- **A newer push cancels the PR's running CI** (`cancel-in-progress` in `ci.yml`), so a watcher follows the newest run.
- **The landing push uses `--no-verify`.** CI passed on that exact SHA, and a pre-push hook that adds a commit would
  land something CI never saw.
- **Only a lead agent lands.** CI takes 25–35 min, longer than a foreground tool call, so agents run `pnpm land` in the
  background, and a background command never wakes a subagent. A subagent runs `-no-wait` and hands over.
- ❌ **Never commit on local `main`.** It mirrors `origin/main`; `land` fast-forwards it wherever it's checked out, and
  only when that tree is clean.

Rulesets, the probe, and decisions: `DETAILS.md`.
