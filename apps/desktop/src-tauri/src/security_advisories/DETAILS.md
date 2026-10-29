# Security advisories: details

`CLAUDE.md` holds the must-knows. This is the contract and the reasoning.

## Why it exists

The EU Cyber Resilience Act, Annex I Part II (8), asks that security updates come with advisory messages telling users
what's relevant, including any action to take. Signed automatic updates already cover the update half; this list is
the message half. It feeds two surfaces:

- **The restart toast** (macOS): `install_update` reads the staged bundle's copy once the bundle is trusted and answers
  a `SecurityNotice` (highest severity, count, the most severe advisory's link). Flow and trust argument:
  `../updater/DETAILS.md` § Security notices. Toast variant: `apps/desktop/src/lib/updates/DETAILS.md` § The security
  variant.
- **The What's new popup** (every platform): after a restart, `get_security_advisories(since)` reads this build's
  embedded copy, so the notice still reaches someone who never saw the toast. Frontend flow:
  `apps/desktop/src/lib/whats-new/DETAILS.md` § Security notice.

## What gets an entry

Every fixed vulnerability with a severity rating: our own CVSS v4.0 one, or a dependency advisory's upstream one (an
`(unrated)` advisory gets no entry). Those are the public notes in a release's `### Security` changelog section, per
`SECURITY.md` § How fixed vulnerabilities are published. Releases before 0.57.0 also put hardening there, so "has a
Security section" was never the trigger; the entry is a deliberate, per-release decision, made in the release procedure
(`docs/guides/releasing.md` § Security advisories).

## The file

```json
{
  "advisories": [
    {
      "id": "GHSA-xxxx-xxxx-xxxx",
      "fixedIn": "0.54.0",
      "severity": "high",
      "url": "https://github.com/vdavid/cmdr/security/advisories/GHSA-xxxx-xxxx-xxxx"
    }
  ]
}
```

- `id`: stable and unique. The GHSA id when the fix has a GitHub advisory, else a date slug like `2026-10-16-smb-path`.
- `fixedIn`: the release that fixes it, strict semver (no `v`).
- `severity`: `low` / `medium` / `high` / `critical`, the rating in the public note (ours from CVSS v4.0, or the
  upstream advisory's).
- `url`: the GitHub security advisory when there is one, else that release's entry on the getcmdr.com changelog.
- No `summary`: the app shows generic, translated text only, so per-advisory English would be untranslatable copy.
- Unknown keys at either level make the whole list unparseable (`deny_unknown_fields`), so a typo can't quietly drop
  a field.

It ships twice: at `Contents/Resources/security-advisories.json` in the macOS bundle (`tauri.conf.json` →
`bundle.macOS.files`, so the code signature seals it), and embedded in the binary (`include_str!`), like the changelog.

## The window

`fixed_between(after, upto)` keeps `after < fixedIn <= upto`, most severe first, then newest, then by id. The lower
bound is exclusive because the running build already contains what it was built with; the upper bound is inclusive
because the new build contains its own fix. The toast asks with (running, staged); What's new with (last seen,
current). Both are cumulative over skipped releases.

## The link allowlist

`is_allowed_advisory_url` admits exactly:

- `https://github.com/vdavid/cmdr/security/advisories/<something>` (an advisory, never the bare index),
- the same under `https://github.com/getcmdr/cmdr/`, and
- `https://getcmdr.com/changelog`, or anything below it, fragment included.

**Decision**: both repo names pass. **Why**: the repo may move to the `getcmdr` org (#476), and the update toast vets a
staged list's links with the RUNNING app's allowlist, so a build that only knows `vdavid/cmdr` drops a `getcmdr/cmdr`
link. The org name has to ship in a release before any advisory uses it, and after a move, keep writing `vdavid/cmdr`
links (they redirect) until the pre-move versions age out.

It refuses any other scheme, userinfo (`https://github.com@evil.example/…`, whose host is `evil.example`), an explicit
port, a lookalike host, and any URL that isn't already in the `url` crate's canonical form, which is what catches `..`
segments, mixed case, and backslashes that only read right before normalization. A refused link costs the link, never
the notice: `vetted_link` answers `None` (logged at warn), and the surfaces show the notice without it.

The check in Go mirrors this; Go's `net/url` keeps `..` segments that the `url` crate resolves, so the Go side refuses
`.` and `..` segments outright rather than disagree about where a link points.

## Testing

- `tests.rs`: parse shapes (malformed, unknown severity, not semver, unknown field), the window edges and a skipped
  release, the allowlist's refusals, and `the_committed_list_parses_and_every_link_is_allowed`, the canary that keeps
  the embedded copy from ever reaching `embedded()`'s empty fallback.
- `../updater/advisories.rs` and `../updater/installer.rs` test the notice reduction and the install order.
- `scripts/check/checks/desktop-security-advisories_test.go` tests the validator.
