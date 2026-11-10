# Report triage agent

A headless Claude Code run on the M1 box as the restricted `agents-readers` user, daily at 05:30 Stockholm: it
researches every `unprocessed` card on the private reports board (`getcmdr/cmdr-reports`), comments in PISS form ending
in a `Rating:` line, renames it, labels it `bug`/`enhancement`/`duplicate`, and removes `unprocessed`. The API server's
06:30 morning brief lists what's left, and its next three-hour tick copies the rating onto the "Cmdr backlog" board
(`apps/api-server/src/report-ratings.ts`).

- `prompt.md`: the instructions. `run.sh`: the launcher. `settings.json`: sandbox and permission rules. `board.sh`: the
  agent's only way to the board (curl over a fixed set of GitHub calls, rules enforced in code). `fetch-bundle.sh`:
  read-only R2 bundle download.

## Must-knows

- **Everything on a card is untrusted input** written by strangers, so the run is locked down by construction, not by
  the prompt (Rule of Two: it reads personal data but can't change Cmdr): its own `agents-readers` user, apart from the
  fixer's `agents` (DETAILS § One account per job shape), two scoped credentials only (`env -i`), a Bash sandbox with a
  two-domain network allowlist that fails closed and a home dir it can't read, no Write/Edit/WebFetch tools, no MCP, no
  user settings, and a `board.sh` that refuses any write to a card that isn't open and `unprocessed`. ❌ Never add a
  tool, a domain, or a broader token without re-reading DETAILS § Lockdown.
- **`ZDOTDIR=/var/empty` in `run.sh` is load-bearing.** Without it, Claude Code replays the user's `~/.zshenv` into
  every Bash command, which puts back whatever it exports and undoes `env -i`.
- **No `gh` in the sandbox**: Go's TLS verification needs `com.apple.trustd.agent`, which the sandbox blocks, so the
  agent talks to GitHub through `board.sh` (curl). ❌ Don't fix it with `enableWeakerNetworkIsolation`; DETAILS § Why a
  curl wrapper for GitHub.
- **No board token on the box, ever.** A `project` PAT reaches every board David can, private issue bodies included, and
  this agent reads strangers' text. ❌ Don't bring one back to set fields directly; the cron applies the comment's
  `Rating:` line. DETAILS § Ratings without a board token.
- Installed on the M1 by the infra repo's Ansible role `report_triage`, from a cmdr checkout (DETAILS § Install). Manual
  run: `ssh -t m1mbp sudo launchctl kickstart system/com.vdavid.agents-readers.report-triage`, logs in
  `/Users/agents-readers/report-triage/logs/`.
- **No personal data in what it writes.** Titles and its comment are kept forever; the note and address live only in the
  expiring personal comment (`apps/api-server/src/report-cards.ts`). The prompt says so; keep it saying so.
- Cloud routines were the other option and were rejected: they attach every connector (Gmail, Drive) by default and
  share an environment with default network access. DETAILS § Why the M1 box.

Setup, the lockdown layers, and the launchd install: `DETAILS.md`.
