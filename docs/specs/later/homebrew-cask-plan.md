# Homebrew cask (`brew install --cask cmdr`)

Plan for getting Cmdr accepted into Homebrew Cask. This document captures the **intention** and the hard constraints so
the implementing agent can adapt details when reality pushes back, as long as the intentions stay intact.

## Why

- `brew install --cask cmdr` is a real, evergreen distribution channel. A direct competitor (TheCommander, a near-exact
  contemporary) has had a working cask since launch and gets ~400 installs over a few months with zero marketing effort.
- **The README's current brew gate is based on the wrong rule.** README says the cask "will be available as soon as this
  repo hits 50+ forks, 50+ watchers, and 100+ stars." That is the **GitHub notability threshold**, and per Homebrew's
  [Acceptable Casks](https://docs.brew.sh/Acceptable-Casks) doc it **only applies to casks that are homed on a code
  repository** (i.e. whose `url`/`homepage` resolve to a GitHub/GitLab/Bitbucket repo). The real numbers are **under 30
  forks / 30 watchers / 75 stars** for general (third-party) submissions, or **90 / 90 / 225** for self-submissions.
- Cmdr today has **15 stars, 1 fork, 0 watchers** — far under the bar. But the bar is avoidable: a cask served from our
  own domain (not a code repo) is judged on website-presence criteria instead, which Cmdr already passes. TheCommander
  proves this: proprietary, no public repo, accepted at zero stars.

## The core constraint (the whole reason this is non-trivial)

Homebrew's `brew audit --new --cask` follows the `url` to verify the download. **Our download URL currently
302-redirects to GitHub Releases**, which re-ties the cask to `vdavid/cmdr` and re-triggers the 75-star notability check
we'd fail:

```
https://license.getcmdr.com/download/0.24.0/aarch64
  -> 302 -> https://github.com/vdavid/cmdr/releases/download/v0.24.0/Cmdr_0.24.0_aarch64.dmg
```

**The fix is to make the cask's `url` resolve to a file served from our own domain with no redirect to github.com.**
Once the cask has zero GitHub footprint, the notability check never fires and we're judged on:

- A public presence so users can verify authenticity ✅ (getcmdr.com + press coverage)
- An identifiable developer ✅
- A stable, versioned, login-free download URL ✅ (once it's direct)

We pass all three. The GitHub release can remain the build artifact; the cask just must not **point** at it.

## Scope / non-goals

- **Non-goal: bumping the GitHub repo's star count.** That's the wrong lever. The work is to remove the repo from the
  cask's resolution path, not to chase stars.
- **Non-goal: submitting the PR ourselves.** The cask PR to `Homebrew/homebrew-cask` is opened by a **third party, not
  the repo owner** (David's wife will submit). Self-submission triggers the much higher 225-star bar and reads as
  self-promotion; a third-party submission stays on the general track (and, with no repo footprint, the star bar is moot
  anyway). This is an external action — **do not open the PR from any agent.** Prepare everything; David's wife submits.
- **Non-goal: changing how releases are built or where build artifacts live.** GitHub Releases can stay as the artifact
  store. Only the _cask-facing_ download path must be domain-served.
- **Out of scope: a `@beta` cask.** Cmdr presents as a 1.x-style app; ship a single stable cask. Revisit if we later
  want an explicit beta channel.

## What needs to exist before the cask can be submitted

Two prerequisites, both on our own infrastructure (we're already all-in on Cloudflare: Pages + Worker; **R2** is the
natural home):

### 1. A direct, domain-served DMG URL (no github redirect)

The cask `url` must fetch the actual DMG bytes from a getcmdr.com-controlled host, returning `200` (not a `302` to
github.com). Two acceptable shapes; pick whichever fits the existing release pipeline best:

- **Preferred — R2 object**: upload each release DMG to R2, expose at a stable, versioned, public URL, e.g.
  `https://downloads.getcmdr.com/Cmdr_#{version}_universal.dmg`. Robust, cacheable, no Worker egress for large files.
- **Alternative — streaming proxy**: change `license.getcmdr.com/download/...` to **stream** the bytes from the GitHub
  asset (200 OK) instead of 302-redirecting. Hides github from the audit, but pipes large DMGs through the Worker
  (egress + timeout risk). Only if R2 is undesirable.

Decide single-`universal` vs per-arch:

- A single **universal** DMG is the cleanest cask (one `url`, both arches, no `on_arm`/`on_intel` split). Recommended if
  the universal build is the canonical artifact.
- If per-arch is required, the cask uses `on_arm`/`on_intel` blocks with separate `url` + `sha256`. More moving parts in
  the livecheck/version story.

The URL must be **stable and versioned** (the `#{version}` interpolation pattern), login-free, and durable across
releases (don't reuse a "latest" path that mutates — Homebrew pins a version + sha256).

### 2. A version manifest endpoint on our domain for `livecheck`

For Homebrew to auto-bump the cask on every release (the way TheCommander's `BrewTestBot` does — 24 of its 26 cask
commits are automated), the cask needs a `livecheck` block pointing at a machine-readable version source **on
getcmdr.com**, not github.

- There is **no working endpoint today**: `getcmdr.com/appcast.xml` returns the Astro SPA 404 HTML page (HTTP 200 but
  not a manifest); `license.getcmdr.com/{latest,version,appcast.xml,update}` all 404.
- Add a small endpoint served by the API Worker (or a static file). Two reasonable formats:
  - A **Sparkle-style `appcast.xml`** (lets the cask use `strategy :sparkle, &:short_version`, matching TheCommander's
    setup exactly), or
  - A minimal **`/latest` JSON** (`{ "version": "0.24.0" }`) with livecheck `strategy :json`.
- Keep it self-owned (getcmdr.com). Pointing livecheck at GitHub would technically work (livecheck URLs are **not** part
  of the notability check), but a domain-served manifest keeps the whole cask github-free and consistent.
- The version string this returns must match the cask `version` and the DMG filename interpolation.

## The cask file (target)

Lands as `Casks/c/cmdr.rb` in `Homebrew/homebrew-cask`. Draft, pending the prerequisites and the confirmations below:

```ruby
cask "cmdr" do
  version "0.24.0"
  sha256 "<sha256 of the universal dmg for this version>"

  url "https://downloads.getcmdr.com/Cmdr_#{version}_universal.dmg"
  name "Cmdr"
  desc "Fast, keyboard-driven two-pane file manager with optional privacy-first AI"
  homepage "https://getcmdr.com/"

  livecheck do
    url "https://getcmdr.com/appcast.xml"   # the endpoint added in prereq 2
    strategy :sparkle, &:short_version
  end

  auto_updates true                          # Cmdr ships its own updater
  depends_on macos: ">= :sonoma"             # set to Cmdr's real minimum

  app "Cmdr.app"

  zap trash: [
    "~/Library/Application Support/com.veszelovszki.cmdr",
    "~/Library/Caches/com.veszelovszki.cmdr",
    "~/Library/Logs/com.veszelovszki.cmdr",
    "~/Library/Preferences/com.veszelovszki.cmdr.plist",
    "~/Library/Saved Application State/com.veszelovszki.cmdr.savedState",
  ]
end
```

### Confirm before finalizing the cask

- **Minimum macOS version** for the `depends_on macos:` stanza (placeholder `:sonoma` may be wrong).
- **Universal vs per-arch** DMG (drives the `url`/`sha256` shape — see prereq 1).
- **Exact installed `.app` name** (`Cmdr.app`?) and the **bundle id** for the `zap` paths. `com.veszelovszki.cmdr` is
  the prod id per the data-dir convention; verify the prod bundle id and the real list of paths Cmdr writes (data dir,
  logs, caches, prefs, saved state, file-backed secret store) so `zap` cleans everything.
- **`auto_updates true`** is correct because Cmdr has its own updater; confirm Homebrew shouldn't manage updates.
- Run `brew audit --new --cask cmdr` and `brew style --fix Casks/c/cmdr.rb` locally (or in a throwaway tap) and confirm
  **no GitHub host appears** in the resolved `url` — that's the make-or-break check.

## Definition of done

1. The cask `url` resolves to a domain-served DMG (200, no github redirect), verified with `curl -I`.
2. A getcmdr.com version manifest exists and `brew livecheck` reads the current version from it.
3. `brew audit --new --cask cmdr` passes with **no** notability failure and no github.com in the resolved download.
4. `brew install --cask cmdr` works end to end from a clean machine/tap (installs, launches, `auto_updates` respected).
5. The cask PR is **prepared** (branch + file + passing local audit) and **handed to David's wife to submit** — no agent
   opens it.

## Follow-up: fix the README

Once the path is real, correct the README's Installation section. It currently states the brew gate as "50+ forks, 50+
watchers, and 100+ stars" — replace with the actual requirement (domain-served download + third-party submission), or
simply announce `brew install --cask cmdr` once it's live. The current text misleads readers and ourselves about why
brew "isn't available yet."
