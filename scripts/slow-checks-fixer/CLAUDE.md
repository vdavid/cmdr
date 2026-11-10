# Slow-checks fixer agent

A headless Claude Code run on the M1 box that picks up each red nightly "Slow checks" run on `main`. The workflow's
`report` job opens (or comments on) one `slow-checks` issue; launchd starts `run.sh` hourly, and when that issue names a
red run nobody has handled, the agent diagnoses each failing job and then reruns an outside blip, lands a clear fix
through `pnpm land`, or writes up what needs David. A green run closes the issue (`.github/workflows/slow-checks.yml`).

- `run.sh`: the launcher (finds the issue and run, claims the run, readies the clone, starts `claude -p`). `prompt.md`:
  the agent's instructions, with `{{ISSUE}}` and `{{RUN_ID}}` filled in by `run.sh`.

## Must-knows

- **Its notes go in the issue BODY, never a comment**, so a fixed or rerun night costs David no extra email. It comments
  only when something needs his call. Keep that split if you touch the prompt: the whole point is that the emails mean
  something.
- **It never loosens anything to go green**: no skipped or ignored checks, tests, advisories, or allowlists, no deleted
  tests, no new dependencies. Those need David, so it writes a recommendation and lands nothing.
- **One rerun per run, on attempt 1 only.** A blip that repeats isn't a blip.
- **It trusts only what the workflow wrote**: the issue must be authored by `app/github-actions`, and only that bot's
  "went red" comments name runs. Anyone with triage rights can add a label. The logs quote third-party text, which the
  prompt treats as data.
- **This agent runs unattended with full permissions and the box's own `GH_TOKEN`**, unlike the locked-down report
  triager (`scripts/report-triage/CLAUDE.md`): it has to edit code and land. That's acceptable only because its input is
  our own CI's output. ❌ Never point it at anything a stranger can write.
- **`handled-runs` claims a run before the agent starts**, so a crash doesn't retry it hourly. The next red night names
  a new run, which gets a fresh look.

Install, schedule, logs, and the design decisions: `DETAILS.md`.
