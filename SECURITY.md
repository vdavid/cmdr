# Security policy

Thanks for helping keep Cmdr and its users safe. This page says how to report a security problem, what's in scope, and
what happens after you write.

Cmdr is made by Rymdskottkärra AB, a Swedish company, and maintained by one person, David Veszelovszki. The response
times below are sized for that: modest, and ones we can actually keep.

For how Cmdr handles data, signing, updates, and permissions, see the trust page at
[getcmdr.com/trust](https://getcmdr.com/trust).

## How to report a vulnerability

Email **[security@getcmdr.com](mailto:security@getcmdr.com)**. Please don't open a public GitHub issue, discussion, or
pull request for a security problem.

You can also use GitHub's private reporting: the **Report a vulnerability** button on the repository's
[Security tab](https://github.com/vdavid/cmdr/security/advisories/new).

A good report has:

- What the problem is and what an attacker could do with it.
- The Cmdr version (**Cmdr > About Cmdr**) and your macOS version, or the URL for a website or API issue.
- Steps to reproduce, or a proof of concept.
- Whether you've told anyone else, and whether you plan to publish.

Write in English, Swedish, or Hungarian. We don't publish a PGP key, by choice. If you'd rather not send the details by
email, use GitHub's private reporting above: only you and the maintainer can see the report.

## What to expect

- **Acknowledgment within five business days**, so you know a human has read it.
- **A first assessment within 14 days**: whether we can reproduce it, how serious we think it is, and a rough plan.
- **A fix for critical and high-severity issues within 30 days** where that's technically possible, and within 90 days
  for the rest. If a fix will take longer, we'll tell you why and agree on a date with you.
- **Updates at least every 14 days** until the issue is closed.

A report without a severity score gets one from us, with the
[CVSS v4.0 calculator](https://www.first.org/cvss/calculator/4.0), and that score sets the fix deadline: critical and
high within 30 days, the rest within 90.

Once the fix has shipped, we publish a note about it (see
[How fixed vulnerabilities are published](#how-fixed-vulnerabilities-are-published)) and, with your permission, credit
you by name.

We ask for **coordinated disclosure**: please give us 90 days from your report, or until a fix has shipped, whichever
comes first, before you publish details. Cmdr updates itself automatically, so a running copy picks up a fix within
hours of its release. If an issue is being actively exploited, tell us, and we'll move faster and can agree on an
earlier date.

There's no bug bounty. We can't pay for reports, but we're grateful for them.

When a flaw is being actively exploited, or something bad happens to Cmdr itself (like a stolen signing key), we follow
our [security incident procedure](docs/security-incident-procedure.md), which includes reporting to CERT-SE and ENISA
under the EU Cyber Resilience Act.

## How fixed vulnerabilities are published

Every fixed vulnerability gets a public note once the fix has shipped. What the note says depends on whose code the flaw
was in.

A flaw in Cmdr's own code gets the full note:

- **What the flaw was** and what it let an attacker do.
- **How severe it is**: its CVSS v4.0 score and rating (low, medium, high, or critical).
- **What it affects**: the app versions, from the first affected one to the last, or the part of Cmdr's servers.
- **What to do**: usually "update to version X.Y.Z", which Cmdr does by itself unless updates are turned off. For a flaw
  on Cmdr's servers, the note says whether users need to do anything, like replacing a key.

A flaw in a dependency (a Rust crate or npm package Cmdr uses, which already has its own published advisory) gets a
shorter note, since the upstream advisory already explains the flaw:

- **What it let an attacker do in Cmdr**, in one line.
- **The upstream advisory**: its ID (like `GHSA-…`, `RUSTSEC-…`, or `CVE-…`) and its severity as that advisory rates it,
  or "unrated" when it gives none.
- **What it affects**: the Cmdr versions, from the first affected one to the last.

It gets no GitHub security advisory or CVE ID of our own: the upstream one is the record, and a second would only split
it.

Where the note goes:

- **Fixes in the app** go in the `### Security` section of the [changelog](CHANGELOG.md), in the release that fixes
  them. Hardening that fixes no flaw goes under "Changed", so every Security line is a fixed vulnerability.
- **A [GitHub security advisory](https://github.com/vdavid/cmdr/security/advisories) with a CVE ID** comes on top for a
  flaw in Cmdr's own code when someone can exploit it without already running code on the user's Mac (for example
  through a file, a network server, a web page, or the update channel), or when it exposes passwords, keys, or user data
  to someone other than the user. Fixes that only guard against an attacker who already runs code on the Mac get the
  changelog note alone.
- **Flaws on Cmdr's servers** (`api.getcmdr.com`, its older alias `license.getcmdr.com`, and `getcmdr.com`, including
  `latest.json`) are part of Cmdr too, so they get a GitHub security advisory on this repository once fixed. There's no
  app version to update, so the advisory gets no CVE ID. If the flaw exposed user data, we also tell the people affected
  directly.

When publishing the details right away would put users at more risk than it protects them, we may hold them back until
users have had the chance to update.

## Scope

In scope:

- **The Cmdr desktop app for macOS**, latest released version. That includes the bundled update mechanism, license
  handling, the local MCP server, the AI features, and the file system backends (local, SMB, SFTP, WebDAV, MTP, ADB,
  archives).
- **api.getcmdr.com** (and its older alias `license.getcmdr.com`): licensing, update checks, crash and error reports,
  feedback, and downloads.
- **getcmdr.com**: the website, including `latest.json`, which the updater reads.

Also welcome, on a best-effort basis: `mail.getcmdr.com` (the newsletter) and `comments.getcmdr.com` (blog comments).
They run third-party open-source software, so we may point you to the upstream project too.

Out of scope:

- Services Cmdr uses but doesn't run: Paddle (payments), PostHog, Cloudflare's platform, GitHub, and the AI providers a
  user connects. Please report those to the vendor.
- Attacks that need an already compromised Mac or account, physical access to an unlocked machine, or root. Cmdr runs as
  the logged-in user and isn't App Sandboxed, so a process already running as that user can do what the user can.
- Denial of service by traffic volume, spam, and social engineering of the maintainer or users.
- Findings from automated scanners without a demonstrated impact, such as a missing security header or a TLS cipher
  preference.
- Versions older than the latest release.

## Supported versions

**Support period**: every Cmdr version gets security fixes for at least five years after its release. The fixes ship as
updates to the latest version, free for every user, so only the **latest released version** gets security fixes. Cmdr
checks for updates every three hours by default and installs them automatically, so the latest version is what most
users run. If you find a problem in an older version, please check that it still happens in the latest one.

## Safe harbor

If you research and report in good faith under this policy, we consider your research authorized. We won't take legal
action against you, report you to the police, or ask anyone else to act against you, and if someone else does, we'll
confirm you had our permission. Good faith means you:

- Only test against your own installation and your own accounts or licenses.
- Don't access, change, or delete other people's data. If you reach some by accident, stop, don't keep a copy, and tell
  us.
- Don't degrade the service for others (no load testing against api.getcmdr.com or getcmdr.com).
- Give us a reasonable time to fix the problem before you publish, as described above.

We can only give permission for what we run. Services Cmdr uses but doesn't run (listed under "Out of scope") have their
own rules, and we can't speak for them. If your research exposes someone's personal data, the law may still require us
to report the exposure to the data protection authority, even though you did nothing wrong.

If you're unsure whether something is OK, ask first at [security@getcmdr.com](mailto:security@getcmdr.com).

<!-- Self-reviewed 2026-10-14, no lawyer by choice. -->
