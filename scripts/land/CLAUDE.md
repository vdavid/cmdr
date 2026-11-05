# Land

`pnpm land` takes a worktree branch onto `main` through CI. With the rulesets in `.github/rulesets/`, it's the only way
onto `main`: nobody, admin included, lands a SHA without a green `CI OK`.

## Module map

- `main.go`: the flow (push, PR, wait, land, sync), each step skipped when done. `record.go`: the `-record N` phase.
  `finish.go`: the tidy-up.
- `state.go`: the pure decisions (start step, CI verdict, messages, worktree parsing).
- `exec.go`: git and gh plumbing (pushes tested against bare origins). `yield.go`: the Claude Code yield. `ghbudget.go`:
  rate-limit waits and the ETag cache. `joblog.go`: why a red job failed, from its log. `rulesets.go`:
  `pnpm land rulesets [-apply]`. `eta.go`, `etagh.go`: the CI ETA.

## Usage

- `pnpm land`: rebase if `origin/main` moved, push, open or reuse the PR, label it `land` (a land run), wait for `CI OK`
  (printing an ETA), land, sync `main`, and tidy up.
- `pnpm land eta`: read-only. CI's load, typical land and light run lengths, and HEAD's live ETA.
- `pnpm land -no-wait`: drop the `land` label, push, open the PR, return. A light run: no Rust lanes, macOS, or E2E.
- `pnpm land -squash [-m MESSAGE]`: squash first (default: every commit message, oldest first).
- `pnpm land -record N`: land, then record contributor PR N as merged (`docs/guides/handling-prs.md`).
- `pnpm land -merge`: never rebase, and stop on a conflict (the release merge-back).
- `pnpm land rulesets`: print what `-apply` would create or update on GitHub.

## Must-knows

- **Before telling David how long landing takes, run `pnpm land eta`**; while landing, quote the ETA `land` printed,
  never a remembered number (`DETAILS.md` § Decision: the ETA comes from recent runs).
- **A green head lands one of two ways, both signed.** Still on top of `origin/main`: `git push origin <sha>:main`.
  `main` moved: `gh pr merge --admin --merge --match-head-commit <sha>`, a merge commit GitHub signs. `main`'s CI then
  skips the suite (`DETAILS.md` § Decision: merge a green PR onto a moved `main`).
- ❌ **Never a GitHub rebase merge**: it rewrites commits unsigned (`DETAILS.md` § Decision: every commit on `main` is
  signed).
- **Re-run it at any stage.** Once a job `ci-ok` needs fails, it prints each failed job, why (from its log), and
  `gh run rerun <id> --failed`. Cancel the rest only when the failure is clearly real (`DETAILS.md` § Decision: stop
  waiting at the first required failure). Only a conflict GitHub can't merge makes it rebase and rerun CI, eight runs at
  most. No CI run at all? The PR conflicts with `main`.
- **It never rebases a merge or a tagged commit**: a merge stops on a conflict, a release needs `-merge`.
- **It never overwrites commits only origin's branch has**: keep the bare lease plus `--force-if-includes` (`DETAILS.md`
  § Gotcha: the branch push).
- ❌ **Don't use the PR page's merge button**: `land` syncs `main` and tidies up.
- **The current land run's `CI OK` decides**: never the run status, an earlier attempt's `CI OK`, or a light run
  (`DETAILS.md` § Decision: `CI OK` decides, not the run status; § Decision: ask for a land run).
- **The landing push uses `--no-verify`**: a pre-push hook's commit would land something CI never saw.
- **Only a lead agent lands**, in the background. A subagent runs `-no-wait` and hands over.
- **Exit 75 ("Still waiting") means run it again**: inside Claude Code it yields after 50 min, before the prompt cache
  expires (`DETAILS.md` § Decision: yield before the prompt cache expires).
- ❌ **Never commit on local `main`**: `land` fast-forwards it wherever it's checked out (clean trees only).

Rulesets, the probe, and decisions: `DETAILS.md`.
