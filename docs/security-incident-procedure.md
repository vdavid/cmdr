# Security incident procedure

What Rymdskottkärra AB (org. nr 559471-0401) does when Cmdr has a severe incident or an actively exploited
vulnerability, as the EU Cyber Resilience Act (CRA, Art. 14) requires since 2026-09-11. Covers the Cmdr app,
`api.getcmdr.com`, `getcmdr.com` and the update channel, and the release pipeline. Owner: David Veszelovszki. Ordinary
vulnerability reports follow `SECURITY.md`; this doc starts when one of them turns out to be exploited, or when
something bad happens to Cmdr itself.

An agent can start all of this with the `cmdr-incident` skill (`.claude/skills/cmdr-incident/SKILL.md`): say "Cmdr
incident".

## First hour: do these in order

Contain before you investigate. In the 2026-10-15 tabletop, containment came 65 minutes in, after root-cause questions,
because this list wasn't in front of anyone.

1. **Write down the aware time, in UTC.** "Aware" is the moment there's reasonable certainty, not the first rumor.
   Unsure whether it counts? Treat it as aware: an unnecessary early warning costs nothing, and a missed one can.
2. **Compute the deadlines and post them** (next section) where everyone working on it sees them.
3. **Open the incident issue**: a private issue in `getcmdr/cmdr-reports` titled `INCIDENT: <short name>`, with the
   aware time, the deadlines, the source, and what's known. Everything after this gets logged there with UTC times.
4. **Contain.**
   - Update channel or release pipeline affected: disable the release workflow
     (`gh workflow disable release.yml -R getcmdr/cmdr`), set any affected GitHub release back to draft, and point
     `latest.json` back at the last good release (`docs/guides/releasing.md` § The tag guard, and rolling back).
   - Credentials exposed: rotate at the provider first, then in the repo and sops, and revoke sessions. Delete secrets
     from GitHub rather than waiting to learn how they leaked.
   - A leaked updater key never expires (one compiled-in public key, no rollover), so treat it as severe even while
     `latest.json` is still ours.

## Deadlines

Count from the aware time. One SRP submission reaches CERT-SE and ENISA together.

- **Severe incident**: early warning within 24 hours, incident notification within 72 hours, final report within one
  month of the 72-hour notification.
- **Actively exploited vulnerability**: early warning within 24 hours, vulnerability notification within 72 hours, final
  report within 14 days of a fix or mitigation being available.
- **Personal data involved** (a GDPR breach too): notify IMY within 72 hours of discovering that personal data was
  affected. That's its own clock, often later than the CRA one.
- **Personal data of a customer with a signed DPA**: notify that customer within 48 hours of becoming aware (DPA § B.6),
  at the notice email in their DPA's Annex I, with what's known so far. This clock is shorter than IMY's.

(From CRA Art. 14 and ENISA's SRP FAQ, checked 2026-10-07; the DPA clock from Cmdr's DPA, decided 2026-10-15.)

## What counts

- **Actively exploited vulnerability**: reliable evidence that someone exploited a flaw in Cmdr (including a dependency
  we ship) without the user's permission. A report or proof of concept alone isn't exploitation; it follows
  `SECURITY.md`.
- **Severe incident**: anything that hurts, or could hurt, Cmdr's ability to protect users' data or functions, or that
  led or could lead to malicious code running in Cmdr or on users' Macs. For example: a stolen signing key (Developer
  ID, updater, license), a tag, release, or `latest.json` we didn't make, a compromised GitHub, Cloudflare, Apple, or
  Paddle account, a malicious dependency in a shipped build, or a breach of `api.getcmdr.com` data.

## Day 1

- **Check the audit logs of every provider that holds personal data or controls the release path**, after any credential
  leak, even if the leak looks narrow. The tabletop's personal-data breach surfaced on day 3 because nobody looked.
  - GitHub: the account security log, recent Actions runs, tags, releases, deploy keys, webhooks, and the `release`
    environment's secrets.
  - Cloudflare: the account audit log, API tokens, and Workers, Pages, D1, and R2 changes.
  - Paddle, PostHog, and SMTP2Go: each dashboard's API-key and activity logs.
  - The api-server's D1 database: unexpected queries or exports, through the Cloudflare audit log.
- **Ship a fix release** if users need one: `docs/guides/releasing.md` § Emergency release during an incident.
- Personal data touched? Start the IMY clock now, and the 48-hour clock for every affected customer with a signed DPA (§
  Deadlines).

## Reporting

1. **Within 24 hours: early warning** on ENISA's Single Reporting Platform (SRP), `portal.cra-srp.enisa.europa.eu`. The
   filer logs in with their own EU Login and MFA. Prefilled template: the vault,
   `projects/Cmdr/business/2026-10-08 Cmdr CRA early-warning template.md`. Keep it short; facts can follow.
2. **SRP down?** Report to CERT-SE directly: phone **010-382 80 00** (24/7) or email `cert@cert.se`, from a
   `getcmdr.com` address. Say the SRP is unavailable and when you tried. File the same notice on the SRP as soon as it
   works again.
3. **Within 72 hours: notification** on the SRP: the nature of the incident or exploit, an initial assessment, the
   measures taken, and what users can do. Personal data involved: notify IMY (imy.se, personuppgiftsincident) on its own
   clock, and record the GDPR assessment even when no notice is needed. Customers with a signed DPA hear from us sooner,
   within their 48 hours.
4. **Tell users** (Art. 14(8)) as soon as there's something they can do, without waiting for the final report: a fix
   release, a note on getcmdr.com, a GitHub security advisory, and an email to license holders. Organizations that
   pinned a version or turned updates off need the email most.
5. **Final report** on the SRP (deadline above): description, severity and impact, root cause or threat type, and the
   mitigations. Publish the CVE and advisory per `SECURITY.md`.
6. **Close**: add a timeline, root cause, and what changed to the incident issue, then add the lesson to
   `docs/threat-model.md` (public part) and the private tracker (exploit detail).

## What agents may do without asking again

During a declared incident (decided 2026-10-15), agents may do these on their own:

- Delete or pause secrets, workflows, and repo variables, and open the `INCIDENT:` issue.
- Send the acknowledgement to the researcher who reported it.
- Submit abuse reports (ad networks, registrars, hosts, Safe Browsing).
- Rotate credentials by driving David's browser and Mac (Cloudflare, Paddle, and the like).
- Build and publish a fix release.
- Prepare the CRA early warning. Only the filer can submit it, since the SRP needs their own EU Login and MFA.
- With David unreachable: alert the backup (next section).

Everything else still needs David's explicit OK: public statements, the user notice, and anything sent in David's name.

## When David is away

The backup contact is a board member of Rymdskottkärra AB who is the Secondary AR ("AR Backup User") on the SRP, with
their own EU Login and MFA. They file the early warning from the prefilled template; they make no public statements and
no technical fixes, and the 72-hour notification and everything after stay with David.

How the backup gets alerted:

- **David is reachable but away from a computer**: David contacts the backup directly.
- **David is unreachable, planned or not**: during a declared incident, an agent may trigger the backup's alert email.
  The recipe, the backup's one-pager, and the out-of-office text are in the vault, `projects/Cmdr/business/` (the
  `2026-10-08 Cmdr CRA *` notes, and fix 9 of `2026-10-15 Cmdr incident tabletop.md`).
- The easiest path for the backup is David's laptop with an agent session: the agent prepares the form, and the backup
  submits it.

## Where reports come in

`security@getcmdr.com`, GitHub private vulnerability reporting on `getcmdr/cmdr`, `vdavid/smb2`, and `vdavid/mtp-rs`,
and public signals: an advisory naming a dependency, a news report, a user's error report or feedback, GitGuardian,
Dependabot, or a Cloudflare or GitHub security notice. David checks `security@` and GitHub notifications at least once a
day, holidays included.

## Contacts

- ENISA SRP: `portal.cra-srp.enisa.europa.eu`.
- CERT-SE (NCSC at FRA, Sweden's coordinating CSIRT): 010-382 80 00 (24/7), `cert@cert.se`.
- IMY (Swedish data protection authority): imy.se.
- Police, if a crime: polisen.se, 114 14.
- Platforms: GitHub security, Cloudflare support, Apple Developer (certificate revocation), Paddle.

## Practice

A one-hour tabletop every October: pick a scenario, run this doc with the clock going, and fix what was unclear. The
2026-10-15 write-up is in the vault, `projects/Cmdr/business/2026-10-15 Cmdr incident tabletop.md`.
