# Land

`pnpm land` takes a worktree branch onto `main` through CI. With `.github/rulesets/`, it's the only way onto `main`:
nobody, admin included, lands a SHA without a green `CI OK`.

## Module map

- `main.go`: the flow (push, PR, wait, land, sync). `record.go`: the `-record N` phase. `finish.go`: the tidy-up.
- `state.go`: the pure decisions (start step, CI verdict, messages, worktree parsing). `ask.go`: the `land` label.
- `exec.go`: git and gh plumbing. `yield.go`: the Claude Code yield. `ghbudget.go`: rate-limit waits and the ETag cache.
  `merge.go`: GitHub merges. `joblog.go`: why a red job failed. `rulesets.go`: `pnpm land rulesets [-apply]`. `eta.go`,
  `etagh.go`: the CI ETA.

## Usage

- `pnpm land`: rebase if `origin/main` moved, push, open or reuse the PR, label it `land`, wait for `CI OK` (printing an
  ETA), land, sync `main`, and tidy up.
- `pnpm land eta`: read-only. CI's load, typical run lengths, and HEAD's live ETA.
- `pnpm land -no-wait`: drop the `land` label, push, open the PR, return. A light run: no Rust lanes, macOS, or E2E.
- `pnpm land -squash [-m MESSAGE]`: squash first (default: every commit message, oldest first).
- `pnpm land -record N`: land, then record contributor PR N as merged (`docs/guides/handling-prs.md`).
- `pnpm land -merge`: never rebase, and stop on a conflict (the release merge-back).
- `pnpm land rulesets`: print what `-apply` would create or update on GitHub.

## Must-knows

- **Before telling David how long landing takes, run `pnpm land eta`**; while landing, quote the ETA `land` printed,
  never a remembered number (`DETAILS.md` § Decision: the ETA comes from recent runs).
- **A green head lands one of two ways, both signed.** On top of `origin/main`: `git push origin <sha>:main`. `main`
  moved: GitHub's REST merge, and `main`'s CI skips the suite (`DETAILS.md` § Decision: merge a green PR onto a moved
  `main`).
- **HTTP status, never wording, tells GitHub failing from a refusal** (`DETAILS.md` § Gotcha: GitHub's own failures).
- ❌ **Never a GitHub rebase merge**: it rewrites commits unsigned (`DETAILS.md` § Decision: every commit on `main` is
  signed).
- **Re-run it at any stage.** A failed `ci-ok` job prints why and `gh run rerun <id> --failed`. Cancel the rest only
  when the failure is clearly real (`DETAILS.md` § Decision: stop waiting at the first required failure). Only a
  conflict GitHub can't merge makes it rebase and rerun CI, eight runs at most. No CI run at all? The PR conflicts with
  `main`.
- **It never rebases a merge or a tagged commit**: a merge stops on a conflict, a release needs `-merge`.
- **It never overwrites commits only origin's branch has**: keep the bare lease plus `--force-if-includes` (`DETAILS.md`
  § Gotcha: the branch push).
- ❌ **Don't use the PR page's merge button**: `land` syncs `main` and tidies up.
- **The NEWEST CI run on the commit must be the land run, and its current attempt's `CI OK` decides**: GitHub reads no
  other, and a re-run can't fix a hidden one (`DETAILS.md` § Decision: `CI OK` decides, not the run status; § Gotcha:
  GitHub reads only the newest run on a commit).
- **The landing push uses `--no-verify`**: a pre-push hook's commit would land something CI never saw.
- **Only a lead agent lands**, in the background. A subagent runs `-no-wait` and hands over.
- **Exit 75 ("Still waiting") means run it again**: inside Claude Code it yields after 50 min, before the prompt cache
  expires (`DETAILS.md` § Decision: yield before the prompt cache expires).
- ❌ **Never commit on local `main`**: `land` fast-forwards it wherever it's checked out (clean trees only).

Rulesets, the probe, and decisions: `DETAILS.md`.
