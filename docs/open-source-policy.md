# Open source policy

This is the open source program of Rymdskottkärra AB, the Swedish company that makes Cmdr: its policy, procedures, and
roles. It covers license compliance (OpenChain, ISO/IEC 5230:2020) and security assurance (ISO/IEC 18974:2023), and it's
part of the company's wider security policy, which is kept as an internal record.

- **Owner**: David Veszelovszki, the company's only developer. He holds every role named below.
- **Status**: version 1.0, adopted 2026-10-15.
- **Review**: yearly, together with the rest of the security policy, and whenever the way we build or ship Cmdr changes
  significantly.
- **Contact**: license and open-source questions to [legal@getcmdr.com](mailto:legal@getcmdr.com), security questions to
  [security@getcmdr.com](mailto:security@getcmdr.com) (see `SECURITY.md`).

## Policy

- **Scope**: all software Rymdskottkärra AB distributes: the Cmdr app, the getcmdr.com website, and the
  `api.getcmdr.com` service, plus the open source components inside them and the company's contributions to open source
  projects. David's own libraries `smb2` and `mtp-rs` are covered as components inside Cmdr (in its SBOMs, license
  checks, and vulnerability scans), not as separately supplied software. The scope grows if someone relies on them
  outside Cmdr.
- **Goals**: we know every open source component we ship and its license, we meet every license's terms, we learn about
  vulnerabilities in those components quickly and fix them in proportion to their risk, and we tell users what they need
  to know.
- **Using open source**: a component comes from crates.io or the npm registry, its project is alive, and its release is
  at least three days old. Rust licenses must be on `deny.toml`'s allowlist, and adding one takes a commit with a
  reason. A new npm license shows up in the regenerated `THIRD-PARTY-NOTICES.md` diff, which we review before merging.
- **Meeting license terms**: every release carries full attribution and license texts (`THIRD-PARTY-NOTICES.md` and the
  app's credits screen) and machine-readable SBOMs. Modified open source lives in a named fork or in `vendor/`, with its
  changes listed.
- **Security assurance**: we check shipped components against public vulnerability databases daily and on every change,
  rank findings by CVSS severity, decide (fix, mitigate, or accept with a written reason), and record the decision where
  the code lives. We fix critical and high within 30 days and the rest within 90, and tell users as `SECURITY.md`
  describes.
- **Contributing**: David may contribute fixes to projects we depend on (for example the merged `tauri-apps/muda` pull
  requests) under each project's license and terms, with no confidential company information and no code we lack the
  right to share. Inbound contributions follow `docs/guides/handling-prs.md`.
- **Questions**: license questions to `legal@getcmdr.com`, security questions to `security@getcmdr.com`, answered within
  30 days.
- **Measuring**: days from advisory to shipped fix, release cadence, and update adoption, published on
  [getcmdr.com/trust/development](https://getcmdr.com/trust/development) and reviewed yearly.
- **Communicating**: this file is public in the Cmdr repository and linked from
  [getcmdr.com/trust](https://getcmdr.com/trust). Anyone reads it before they get access to the code.

## Procedures

1. **Adding a component**: check its source, license, and liveness, and use a release at least three days old.
   `pnpm check` enforces Rust's sources and licenses; the notices diff shows npm licenses.
2. **License obligations**, for the licenses we ship today:
   - **MIT, MIT-0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0, bzip2-1.0.6, Unlicense**: ship the
     copyright notice and license text (MIT-0 and Unlicense ask for nothing; we include them anyway).
   - **Apache-2.0** (with or without the LLVM exception): ship the license and any NOTICE file, and mark our changes.
   - **MPL-2.0**: ship the license, and publish the source of any MPL file we modify.
   - **CC0-1.0**: nothing required.
   - **CC-BY-4.0** (two icons): attribute and say what we changed (done in the notices).
   - A new license gets its line here before it's approved.
3. **Bill of materials and notices**: regenerated on every change (`pnpm check third-party-notices`) and every release
   (the release workflow's `sbom` job, which publishes three signed, attested CycloneDX SBOMs).
4. **Archive**: each release's SBOMs, notices, provenance, and checksums stay on its GitHub release and in git history,
   for as long as that version is distributed plus at least three years.
5. **Inquiries**: acknowledged within five business days, answered within 30, each recorded in a private tracker.
6. **Non-compliance**: fix it in the next release (sooner if the licensor asks), tell the licensor if they raised it,
   and record the cause and the fix. If it can't be fixed, remove the component.
7. **Vulnerabilities in components**: daily scan and Dependabot, then rank by CVSS, then fix, mitigate, or accept with a
   reason (`deny.toml` for Rust, the alert dismissal for npm), then release, then a changelog note in the dependency
   form of `../SECURITY.md` § How fixed vulnerabilities are published. Shipped versions are covered because the scan
   runs on what ships, and auto-update moves users to the fix (52% of active installs on day one, 71% by day three).
8. **Review**: yearly, with the rest of the security policy.

## Roles, competence, and affirmation

- **Program owner** (David): accountable for both programs, approves the policy, runs the yearly review. Competence:
  knows this policy and both standards.
- **Compliance engineer** (David): runs the license checks, keeps notices and SBOMs correct, answers license questions.
  Competence: reads common open source licenses and knows their obligations, runs the tooling.
- **Security engineer** (David): triages reports and advisories, ranks and fixes them, publishes advisories. Competence:
  secure development, vulnerability handling, CVSS.
- **Legal questions**: no outside counsel is retained, by choice: the budget doesn't justify one at this stage. A
  license question the procedures don't answer gets best-effort research by the program owner, from the license texts
  and the SPDX and OSI references, with AI assistance, and the answer and its sources go in the private tracker. A
  question that can't be settled that way is settled by removing or replacing the component (procedure 6).
- **Evidence of competence**: 25+ years of software engineering; designed and runs the license and vulnerability tooling
  in this repo (`deny.toml`, the notices generator, the SBOM and attestation jobs, `SECURITY.md`). Self-assessed on
  2026-10-15.
- **Resources**: time in the yearly review and as reports need; the tooling is free (GitHub, cargo-deny, Renovate,
  Dependabot); no budget for outside legal advice, so an unsettled license question ends in removing or replacing the
  component.
- **Affirmation**, signed 2026-10-15 and renewed every 18 months (next by 2028-04-15): "I've read this policy and its
  procedures. I understand the program's goals, my part in them, and that a missed obligation can mean a license
  violation or an unfixed vulnerability in users' hands. I affirm the program meets every requirement of ISO/IEC
  5230:2020 and ISO/IEC 18974:2023." David Veszelovszki, for Rymdskottkärra AB.

## Requirement map

ISO/IEC 5230:2020 (license compliance):

| Requirement                           | Where it's met                                                                                 | Status |
| ------------------------------------- | ---------------------------------------------------------------------------------------------- | ------ |
| 3.1.1 Policy                          | Policy                                                                                         | Met    |
| 3.1.2 Competence                      | Roles                                                                                          | Met    |
| 3.1.3 Awareness                       | Roles, affirmation                                                                             | Met    |
| 3.1.4 Program scope                   | Policy, scope                                                                                  | Met    |
| 3.1.5 License obligations             | Procedure 2                                                                                    | Met    |
| 3.2.1 Access                          | `legal@getcmdr.com`, published here, in `THIRD-PARTY-NOTICES.md`, and on `/trust`; procedure 5 | Met    |
| 3.2.2 Effectively resourced           | Roles (legal questions, resources), procedure 6                                                | Met    |
| 3.3.1 Bill of materials               | Release SBOMs, procedure 3                                                                     | Met    |
| 3.3.2 License compliance use cases    | Policy, procedures 1–2                                                                         | Met    |
| 3.4.1 Compliance artifacts            | Procedures 3–4                                                                                 | Met    |
| 3.5.1 Contributions                   | Policy, contributing                                                                           | Met    |
| 3.6.1, 3.6.2 Conformance and duration | Affirmation, renewed every 18 months                                                           | Met    |

On 3.2.2: the standard asks for access to legal expertise. Best-effort research with a remove-or-replace fallback is a
documented route, though a reviewer may read it as thin.

ISO/IEC 18974:2023 (security assurance):

| Requirement                            | Where it's met                                                                  | Status |
| -------------------------------------- | ------------------------------------------------------------------------------- | ------ |
| 4.1.1 Policy                           | Policy                                                                          | Met    |
| 4.1.2 Competence                       | Roles                                                                           | Met    |
| 4.1.3 Awareness                        | Roles, affirmation                                                              | Met    |
| 4.1.4 Scope, metrics, improvement      | Policy (scope, measuring), plus the improvements below                          | Met    |
| 4.1.5 Standard practices               | See the breakdown below                                                         | Met    |
| 4.2.1 Access                           | `security@getcmdr.com`, GitHub private reporting, `SECURITY.md`, `security.txt` | Met    |
| 4.2.2 Effectively resourced            | Roles                                                                           | Met    |
| 4.3.1 SBOM                             | Release SBOMs, procedures 3–4                                                   | Met    |
| 4.3.2 Security assurance per component | Procedure 7, ranked by CVSS                                                     | Met    |
| 4.4.1, 4.4.2 Completeness and duration | Affirmation, renewed every 18 months                                            | Met    |

On 4.1.4: the 2026-10 Dependabot cleanup took 113 open alerts down to one, and the 2026-10 internal security review's
findings were fixed or accepted in writing.

On 4.1.5, practice by practice:

- **Threats**: `docs/threat-model.md`.
- **Detection and follow-up, and shipped versions**: procedure 7.
- **Telling customers**: `SECURITY.md`.
- **Testing and verifying before release**: the release gates in `docs/guides/releasing.md` (a full green CI run,
  `cargo deny`, and a clean CodeQL run).
- **Exporting risk information**: SBOMs and advisories.

**Not yet** (an improvement, not a blocker): an automatic npm license allowlist like `cargo deny`'s, so a copyleft npm
license fails the checks instead of relying on review.
