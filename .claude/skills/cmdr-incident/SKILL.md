---
name: cmdr-incident
description: >
  Start a Cmdr security incident: record the aware time in UTC, post the CRA and GDPR deadlines, open the private
  `INCIDENT:` issue, run the day-1 provider audit-log checks, and prefill the CRA early warning as a draft. Use when
  David says "Cmdr incident", "security incident", "we have an incident", or reports a leaked signing key, a fake or
  tampered update, a compromised account (GitHub, Cloudflare, Apple, Paddle), a malicious dependency in a shipped build,
  a user-data breach, or a flaw being actively exploited.
---

# Cmdr incident kickoff

The procedure is `docs/security-incident-procedure.md`. Read it first, top to bottom: it's short. This skill runs its
first hour and day-1 checks so David doesn't have to remember them. Speed beats polish: the first three steps take
minutes, not an hour.

## Hard limits

- ❌ Never file on the SRP, and never contact CERT-SE, IMY, the police, or the press. The early warning is a draft for
  David or the backup to submit with their own EU Login.
- ❌ Never post publicly: no user notice, advisory, website note, social post, or customer email.
- Everything else follows § What agents may do without asking again in the procedure. Outside that list, ask.

## Steps

1. **Aware time.** Ask David once when he first had reasonable certainty, or take the moment he said "incident". Record
   it in UTC (`date -u +%Y-%m-%dT%H:%MZ`). Stockholm is UTC+2 in summer time and UTC+1 in winter.
2. **Deadlines.** Compute them from the aware time per § Deadlines (severe incident or exploited vulnerability; unsure
   means severe incident) and post them to David as a short list with weekday, UTC, and Stockholm time.
3. **Open the issue** (pre-approved): `gh issue create -R vdavid/cmdr-reports --label claude-created` (add
   `--label security-gap` when it's a flaw in Cmdr or its servers) titled `INCIDENT: <short name>`, with the aware time,
   the deadlines, the source, what's known, and an empty UTC timeline. Log every later action there as a comment with
   its UTC time.
4. **Contain.** Walk David through § First hour step 4 and do the pre-approved parts. Don't wait for the root cause.
5. **Day-1 audit-log checks**, read-only, per § Day 1. Run what the CLI and APIs reach, with the access recipes in the
   vault's `agents/tooling/` notes (`cloudflare.md`, `paddle.md`, `posthog.md`), and since the earliest plausible leak:
   - GitHub: recent Actions runs, tags, releases, deploy keys, webhooks, repo variables, and the `release` environment's
     secrets on `getcmdr/cmdr` (`gh run list`, `gh release list`, `gh api repos/getcmdr/cmdr/keys`,
     `gh api repos/getcmdr/cmdr/hooks`, `gh variable list`, `gh secret list --env release`).
   - Cloudflare: the account audit log through the API (D1, R2, Workers, Pages, and token changes).
   - Paddle, PostHog, and SMTP2Go: whatever their APIs expose about key use and account activity.
   - For what no API reaches (the GitHub account security log, dashboard-only logs), give David the exact pages to open.
   - Personal data accessed? Say so at once and add the IMY deadline (72 hours from that discovery) to the issue, plus a
     48-hour deadline per affected customer with a signed DPA (§ Deadlines). Their notice is a draft for David, like
     every customer email.
6. **Prefill the early warning** from the vault template
   `projects/Cmdr/business/2026-10-08 Cmdr CRA early-warning template.md`: copy it, fill the blanks from what's known,
   and put the draft in the incident issue as a comment marked "DRAFT, not submitted". If the SRP is reported down, also
   draft the CERT-SE email from the procedure's § Reporting step 2.
7. **Hand over** to David in one message: the issue link, the deadlines, what's contained, the audit-log findings, the
   draft's location, and what needs him next (anything behind his phone's MFA, the SRP submission, a fix release per
   `docs/guides/releasing.md` § Emergency release during an incident).

If David is unreachable, follow the procedure's § When David is away.
