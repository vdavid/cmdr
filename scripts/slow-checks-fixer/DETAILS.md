# Slow-checks fixer: details

## Why it exists

The nightly workflow's red-run issue said "read the failing step's log, fix the cause, and land it", but nothing read
it. Of the 40 slow-checks runs before 2026-11-10, 16 were red: real findings (new osv-scanner advisories, fuzz,
property-mutant, and Kani failures) mixed with outside blips (the Groq and OpenAI smokes, a mise download outage on
2026-11-08 that failed all six jobs). Each one cost David three emails and a judgment call about which kind it was. Now
the agent makes that call, fixes or reruns, and David hears about it only when a decision is his.

## Flow

1. launchd (`com.vdavid.slow-checks-fixer`, hourly at minute 17) runs `run.sh` through a login `zsh`, so it has the
   box's normal agent environment (`gh` via the box's wrapper and `GH_TOKEN`, `secret`, mise, `claude`).
2. `run.sh` exits at once unless an open `slow-checks` issue by `app/github-actions` names a red run that isn't in
   `~/.local/state/slow-checks-fixer/handled-runs`. The run id is the body's first `actions/runs/<id>` link, or the
   newest bot comment that says "went red".
3. It appends the run id to `handled-runs`, fast-forwards the M1's `~/projects-git/vdavid/cmdr` clone to `origin/main`
   (refusing if the clone is off `main` or has local changes), and starts `claude -p` there with `prompt.md`, Opus, and
   `--permission-mode bypassPermissions`, under a three-hour `alarm`.
4. The agent works the user-level way (its own worktree, `pnpm land`), and writes its notes into the issue body.

## Decisions

- **Hourly polling, not a push.** The box has no inbound path, and a GitHub-side trigger would need a token on GitHub
  that can start work on the M1. An idle tick is two `gh` calls. The nightly run usually finishes around 12:00 UTC, so
  the fixer starts within an hour of that.
- **Body notes, comment only for David.** A body edit sends no notification; a comment emails every watcher. The
  workflow's own close step follows the same rule (`.github/workflows/slow-checks.yml` § Red-run issue).
- **Full permissions.** Fixing means editing code, running checks, and landing, which the report triager's sandbox rules
  out by design. The trade is acceptable because the input is our own CI's output, checked by author.
- **Claim before running.** The alternative, marking a run handled only on success, retries a crashing run every hour
  and burns quota. A run that still needs work comes back the next night as a new run id.
- **`gh run rerun --failed` for blips**, not a fresh dispatch: a rerun tests the same commit, which is the question
  ("was it the outside world?"), and its `report` job reruns too, so a green rerun closes the issue.

## Install

Installed on the M1 by the infra repo's Ansible role `slow_checks_fixer`, which copies `run.sh` and `prompt.md` from the
laptop's cmdr checkout into `~/.local/share/slow-checks-fixer/` and loads the launchd agent:
`cd m1mbp/ansible && mise exec -- ansible-playbook site.yml --tags slow_checks_fixer`. Re-run that after editing either
file. Log: `~/slow-checks-fixer/logs/fixer.log` on the M1. A manual run:
`ssh m1mbp 'zsh -lc ~/.local/share/slow-checks-fixer/run.sh'`.
