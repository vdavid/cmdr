# Land

`pnpm land` takes a worktree branch onto `main` through CI. Once the rulesets in `.github/rulesets/` are on, it's the
only way commits reach `main`: nobody, admin included, lands a SHA without a green `CI OK`.

## Module map

- `main.go`: the flow (push, PR, wait, land, sync), each step skipped when done. `record.go`: the `-record N` phase.
- `state.go`: the pure decisions (start step, CI verdict, messages, worktree parsing), tested in `state_test.go`.
- `exec.go`: git and gh plumbing; pushes are tested against bare origins (`push_test.go`, `record_test.go`).
  `ghbudget.go`: rate-limit waits and the ETag cache (an unchanged poll is a free 304). `rulesets.go`:
  `pnpm land rulesets [-apply]`.

## Usage

- `pnpm land`: rebase onto `origin/main` if it moved, push, open or reuse the PR, wait for `CI OK`, land, sync local
  `main`, delete the remote branch, stop its leftover runs, and print the worktree's teardown command.
- `pnpm land -no-wait`: push and open the PR, then return. Run it after each green milestone and keep working.
- `pnpm land -squash [-m MESSAGE]`: squash first (default message: every commit message, oldest first).
- `pnpm land -record N`: land, then record contributor PR N as merged in a second land (`docs/guides/handling-prs.md`).
- `pnpm land rulesets`: print what `-apply` would create or update on GitHub.
- PRs it opens carry the `claude-created` label; a reused PR, like a contributor's, keeps its own.

## Must-knows

- **A green head lands one of two ways.** Still on top of `origin/main`: `git push origin <sha>:main`, SHAs identical.
  `main` moved while CI ran: `gh pr merge --admin --rebase --match-head-commit <sha>`. Either way the CI ruleset demands
  `CI OK` green on the head, and `main`'s CI trusts it and skips the suite (`DETAILS.md` § Decision: merge a green PR
  onto a moved `main`).
- **Re-run it at any stage.** A red CI prints the failed jobs and `gh run rerun <id> --failed`. Only a conflict GitHub
  can't rebase makes it rebase here and run CI again, eight runs at most. A branch GitHub rebased onto `main` counts as
  landed (`git cherry`, which `remove-worktree.sh` asks too).
- **It never rebases a merge commit**, here or on GitHub: a plain rebase flattens it. A branch carrying one (a release
  merge-back) stops when `main` moved. A contributor PR's record rides no branch: `-record N` builds it on `origin/main`
  and rebuilds it if `main` moves (`DETAILS.md` § Decision: record a contributor PR in its own land).
- **It never overwrites commits only origin's branch has** (another machine, GitHub's "Update branch"): it stops and
  says how to bring them in. Keep the bare lease plus `--force-if-includes` (`DETAILS.md` § Gotcha: the branch push).
- ❌ **Don't use the PR page's merge button.** `land` picks the fast-forward when it can (the SHAs CI tested), syncs
  local `main`, and tidies the branch.
- **A newer push cancels the PR's running CI** (`cancel-in-progress` in `ci.yml`), so a watcher follows the newest run.
- **The landing push uses `--no-verify`.** CI passed on that exact SHA, and a pre-push hook that adds a commit would
  land something CI never saw.
- **Only a lead agent lands.** CI takes 25–35 min, so agents run `pnpm land` in the background, and a background command
  never wakes a subagent. A subagent runs `-no-wait` and hands over.
- ❌ **Never commit on local `main`.** It mirrors `origin/main`; `land` fast-forwards it wherever it's checked out, and
  only when that tree is clean.

Rulesets, the probe, and decisions: `DETAILS.md`.
