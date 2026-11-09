# Backlog board

Open work lives on the "Cmdr backlog" GitHub project (owner `vdavid`, project 2). It holds cards from `getcmdr/cmdr`,
`vdavid/cmdr-reports`, `vdavid/smb2`, and `vdavid/mtp-rs`. Each field option carries its definition as its GitHub
description too.

Rule: an agent filing a card sets **Impact** and **Size**, plus **Severity** when it's a bug or a security issue.

Report cards (`vdavid/cmdr-reports`) get theirs without any agent touching the board. The morning report-triage agent
reads this file for the definitions and ends its triage comment with a line like
`Rating: Severity=Medium Impact=Low Size=S` (`Severity=none` for anything that isn't a bug). The API server's
three-hourly cron copies that line onto the three fields, usually within a few hours, and alerts in the ops digest when
it can't. Grammar and mechanics: `apps/api-server/DETAILS.md` § Card ratings. To re-rate a report card, set the fields
by hand or edit the comment's `Rating:` line; the cron never overwrites a field from a line it already applied.

## Labels

- **`security-gap`** (`vdavid/cmdr-reports`): a known weakness in Cmdr or its servers that `docs/threat-model.md` only
  calls "tracked privately". Every release lists the open ones (`docs/guides/releasing.md`, step 1). Not for error
  reports, crashes, feedback, or account-hygiene tasks. It stays on the issue after closing, as history.

## Severity

Bugs and security issues only; leave it blank on everything else. A security card takes its severity from its CVSS v4.0
score when one exists.

- **Critical**: Data loss, an exploitable security flaw (CVSS 9–10), or a crash in a core flow for many users. Drop
  other work.
- **High**: A core flow broken with a painful workaround, or CVSS 7–8.9. To be fixed within 30 days.
- **Medium**: Annoying, but a workaround exists, or CVSS 4–6.9. Security ones within 90 days.
- **Low**: Cosmetic or rare, or CVSS 0.1–3.9.

## Impact

Set on every open card, bugs included.

- **High**: Helps most users noticeably, or unblocks the business (enterprise readiness, revenue, a launch).
- **Medium**: Helps a meaningful group of users, or makes a common flow clearly better.
- **Low**: Niche, polish, or nice-to-have.

## Size

How much agent work it takes. It's separate from whether the card needs David's review or QA, and translator agents
don't count.

- **XS**: One agent can do it in under 200k tokens of context.
- **S**: One agent can do it in 200k–400k tokens of context.
- **M**: One agent with at most three subagents.
- **L**: An agent coordinating 4–15 agents.
- **XL**: Needs 16 or more agents.

For Cmdr, a card that asks for a plan, or one that spans many subsystems, is usually L or XL.
