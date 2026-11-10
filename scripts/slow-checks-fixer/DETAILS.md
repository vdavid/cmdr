# Slow-checks fixer: details

## Why it exists

The nightly workflow's red-run issue said "read the failing step's log, fix the cause, and land it", but nothing read
it. Of the 40 slow-checks runs before 2026-11-10, 16 were red: real findings (new osv-scanner advisories, fuzz,
property-mutant, and Kani failures) mixed with outside blips (the Groq and OpenAI smokes, a mise download outage on
2026-11-08 that failed all six jobs). Each one cost David three emails and a judgment call about which kind it was. Now
the agent makes that call, fixes or reruns, and David hears about it only when a decision is his.

## Flow

1. A system LaunchDaemon, `com.vdavid.agents.slow-checks-fixer` (hourly at minute 17, `UserName` `agents`), runs
   `run.sh` through a login `zsh`, so it has the `agents` user's environment (`gh`, `gh-app-token`, `secret`, mise,
   `claude`).
2. `run.sh` mints an App token (§ Credentials) and exits at once unless an open `slow-checks` issue by
   `app/github-actions` names a red run that isn't in `~/.local/state/slow-checks-fixer/handled-runs`. The run id is the
   body's first `actions/runs/<id>` link, or the newest `github-actions` comment that says "went red".
3. It appends the run id to `handled-runs`, then readies the run under `~/slow-checks-fixer/` (§ Paths): it syncs its
   own clone to `origin/main`, adds a worktree on `fixer/run-<id>`, writes the failing jobs' logs to a file, clones the
   warm `target/`, runs `pnpm install`, and times one warm-up `pnpm check clippy`.
4. It starts `claude -p` in the worktree with `prompt.md`, Opus, and `--permission-mode bypassPermissions`, under a
   three-hour `alarm`. The agent fixes, lands or opens a PR, and writes its notes into the issue body.
5. On exit (a trap, so a crash too), it removes the worktree, its target clone, the local branch, and the token. The
   pushed branch stays: a fix waiting for David lives on in its PR. A run killed outright is swept at the next run.

## Paths

All under `/Users/agents/` (`$HOME`); the infra role creates the first two.

- `~/.local/share/slow-checks-fixer/`: `run.sh` and `prompt.md`, installed by the infra role.
- `~/slow-checks-fixer/logs/fixer.log`: launchd's log. `run.sh` logs the target clone, `pnpm install`, and warm-up build
  times there, plus the agent's exit status and minutes.
- `~/slow-checks-fixer/cmdr/`: the fixer's own HTTPS clone. Nobody works in it, so it follows `origin/main`.
- `~/slow-checks-fixer/runs/<run id>/`: one run's worktree (`cmdr/`), `failed-jobs.log`, `warmup-build.log`, and
  `cp-errors.log`, all removed when the run ends.
- `/Users/veszelovszki/projects-git/vdavid/cmdr/target`: the warm build source, read-only to `agents`. `cp -c -R` clones
  it copy-on-write (same APFS volume), so a run starts warm and David's cache is never written. The CI runner's caches
  are never a source: PR code runs there.

## Credentials

The Rule of Two: the agent takes untrusted input and can change Cmdr, so it holds nothing else.

- **The user**: `agents`, which can't read David's home (bar the warm `target/`), keychain, vault, or SSH keys, nor the
  report triager's `agents-readers` home, whose personal-data tokens stay out of reach of a run that can change Cmdr
  (`scripts/report-triage/DETAILS.md` § One account per job shape).
- **Secrets**: `secret NAME` reads the `agents` store, which holds only what this job needs: `CLAUDE_CODE_OAUTH_TOKEN`,
  and `gh-app-token`'s `CMDR_AGENTS_APP_ID` and `CMDR_AGENTS_APP_PRIVATE_KEY`.
- **GitHub**: the Cmdr agents GitHub App, installed on `getcmdr/cmdr` only, with Contents, Pull requests, Issues, and
  Actions read-write and Metadata read. No Workflows, no Administration. `gh-app-token` prints a one-hour installation
  token. `run.sh` keeps it in a private temp dir as `hosts.yml` (`GH_CONFIG_DIR`, which `gh` rereads on every call;
  verified on gh 2.103.3, 2026-11-10) and as the source of a git credential helper set through `GIT_CONFIG_*`, so the
  token never lands in argv, a URL, a log, or a git config file. A background loop re-mints it every 30 minutes, since a
  run can outlast one token. `GH_TOKEN` is unset: it would override the file and wouldn't refresh.
- **Commits**: signed with the box's agent signing key, which the infra side gives `agents`; `main` takes only signed
  commits.
- **Landing**: `pnpm land` pushes the green SHA to `main` or merges the PR through REST, as the App. The App is a bypass
  actor on "main: PR with approval" only, so it lands without a human approval but still needs a green `CI OK` and
  signed commits, and can't create, move, or delete a `v*` tag (`scripts/land/DETAILS.md` § The rulesets).

## The sensitive-path gate

A fix touching the paths in `prompt.md` § Sensitive paths goes to a PR labeled `sensitive-path` and waits for David to
land it. How hard each part holds:

- `.github/workflows/**`: GitHub refuses the App's push outright, since it has no Workflows permission.
- Everything else on the list: **prompt-level only**. GitHub has no path rule that could hold it: push rulesets are
  refused on public repos and branch rulesets take no path restriction (`scripts/land/DETAILS.md` § The rulesets). A
  hijacked or confused run with a green `CI OK` could land one. What still holds then: CI, signed commits, no
  force-push, and no release tag, so nothing ships to users until David tags a release.

## Decisions

- **Hourly polling, not a push.** The box has no inbound path, and a GitHub-side trigger would need a token on GitHub
  that can start work on the M1. An idle tick is a token mint and two `gh` calls. The nightly run usually finishes
  around 12:00 UTC, so the fixer starts within an hour of that.
- **Logs in a file, comments never.** The issue's comments are public input, so `run.sh` reads them only through an
  author filter for run ids, and the agent gets the run id, the issue number, and the logs file. It still edits the
  issue body, which only the workflow and maintainers can write.
- **Body notes, comment only for David.** A body edit sends no notification; a comment emails every watcher. The
  workflow's own close step follows the same rule (`.github/workflows/slow-checks.yml` § Red-run issue).
- **Full permissions.** Fixing means editing code, running checks, and landing, which the report triager's sandbox rules
  out by design. The trade is acceptable because the run holds no sensitive access (§ Credentials).
- **Claim before running.** The alternative, marking a run handled only on success, retries a crashing run every hour
  and burns quota. A run that still needs work comes back the next night as a new run id.
- **`gh run rerun --failed` for blips**, not a fresh dispatch: a rerun tests the same commit, which is the question
  ("was it the outside world?"), and its `report` job reruns too, so a green rerun closes the issue.
- **A worktree per run, removed after.** Each run starts from `origin/main` with nothing a past run left, and a run's
  target clone (copy-on-write, but it grows as the build writes) doesn't pile up on the box.

## Install

Installed on the M1 by the infra repo's Ansible role `slow_checks_fixer`, which copies `run.sh` and `prompt.md` from a
cmdr checkout (`-e slow_checks_fixer_src=<dir>/`) into `/Users/agents/.local/share/slow-checks-fixer/` and loads the
launchd daemon: `cd m1mbp/ansible && mise exec -- ansible-playbook site.yml --tags slow_checks_fixer`. Re-run that after
editing either file.

A manual run: start the daemon itself, in its own environment, with
`ssh -t m1mbp sudo launchctl kickstart system/com.vdavid.agents.slow-checks-fixer`, then read
`/Users/agents/slow-checks-fixer/logs/`. The account's login shell is `/usr/bin/false` on purpose, so
`sudo -u agents -i` doesn't work; a debug shell (which is NOT the daemon's environment) is
`ssh -t m1mbp sudo -u agents -H /bin/zsh`.
