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

- `pnpm land`: rebase onto `origin/main` if it moved, push, open or reuse the PR, add the `land` label (a land run),
  wait for `CI OK`, land, sync `main`, delete the remote branch, stop leftover runs, and print the teardown command.
- `pnpm land -no-wait`: drop the `land` label, push, open the PR, return. A light run: no Rust lanes, macOS, or E2E.
- `pnpm land -squash [-m MESSAGE]`: squash first (default message: every commit message, oldest first).
- `pnpm land -record N`: land, then record contributor PR N as merged in a second land (`docs/guides/handling-prs.md`).
- `pnpm land -merge`: never rebase, even before the first push, and stop on a conflict (the release merge-back).
- `pnpm land rulesets`: print what `-apply` would create or update on GitHub.
- PRs it opens carry the `claude-created` label; a reused PR keeps its own.

## Must-knows

- **A green head lands one of two ways, both signed.** Still on top of `origin/main`: `git push origin <sha>:main`.
  `main` moved: `gh pr merge --admin --merge --match-head-commit <sha>`, a merge commit GitHub signs. The CI ruleset
  demands `CI OK` green on the head, and `main`'s CI skips the suite (`DETAILS.md` § Decision: merge a green PR onto a
  moved `main`).
- ❌ **Never a GitHub rebase merge**: it rewrites commits unsigned (`DETAILS.md` § Decision: every commit on `main` is
  signed). Commits `land` makes itself sign by `commit.gpgSign`.
- **Re-run it at any stage.** A red CI prints the failed jobs and `gh run rerun <id> --failed` once a job `ci-ok` needs
  fails (`DETAILS.md` § Decision: stop waiting at the first required failure). Only a conflict GitHub can't merge makes
  it rebase here and run CI again, eight runs at most.
- **It never rebases a merge or a tagged commit.** A branch carrying a merge stops on a conflict; a tagged one (a
  release) needs `-merge`. `-record N` builds a contributor PR's record on `origin/main` (`DETAILS.md` § Decision:
  record a contributor PR in its own land).
- **It never overwrites commits only origin's branch has** (another machine, GitHub's "Update branch"): it stops. Keep
  the bare lease plus `--force-if-includes` (`DETAILS.md` § Gotcha: the branch push).
- ❌ **Don't use the PR page's merge button.** `land` fast-forwards when it can, syncs `main`, and tidies up.
- **The current land run's `CI OK` decides**: never the run status, an earlier attempt's `CI OK`, or a light run
  (`DETAILS.md` § Decision: `CI OK` decides, not the run status; § Decision: ask for a land run).
- **A newer push cancels the PR's running CI**, so a watcher follows the newest run.
- **The landing push uses `--no-verify`**: a pre-push hook's commit would land something CI never saw.
- **Only a lead agent lands**, in the background. A subagent runs `-no-wait` and hands over.
- ❌ **Never commit on local `main`.** It mirrors `origin/main`; `land` fast-forwards it wherever it's checked out
  (clean trees only).

Rulesets, the probe, and decisions: `DETAILS.md`.
