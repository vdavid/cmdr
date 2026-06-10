# Homebrew cask

How to get `brew install --cask cmdr` live, and how it stays current afterward.

The cask file lives at [`apps/desktop/packaging/homebrew/cmdr.rb`](../../apps/desktop/packaging/homebrew/cmdr.rb). It's
a submission template: once merged into [Homebrew/homebrew-cask](https://github.com/Homebrew/homebrew-cask) (as
`Casks/c/cmdr.rb`), that repo becomes the canonical home and their `BrewTestBot` auto-bumps the version on every Cmdr
release via the `livecheck` block. Our copy then goes stale by design; it only matters again if we ever resubmit.

## Why the cask looks the way it does

- **`url` points at `license.getcmdr.com/download/#{version}/universal`, not GitHub.** The 302 to GitHub Releases behind
  it is fine: `brew audit` regex-matches only the _declared_ `url`/`homepage` strings, and never follows redirects. A
  `github.com` URL in the cask would auto-fail the notability audit (needs 75+ stars; 225+ for self-submissions). Keep
  `github.com` out of the cask file. Bonus: the endpoint logs downloads to D1, so brew installs show up in our
  telemetry.
- **`livecheck` reads `https://getcmdr.com/latest.json`**, the same Tauri updater manifest the app polls. The release
  flow already publishes it, so releases need zero extra work for brew.
- **`auto_updates true`**: Cmdr ships its own updater, so `brew upgrade` skips it unless `--greedy`.
- **`depends_on macos: :monterey`** means "Monterey or later" (current Homebrew semantics) and matches
  `minimumSystemVersion` in `tauri.conf.json`. Floor rationale:
  [`docs/notes/system-requirements-and-es2025.md`](../notes/system-requirements-and-es2025.md).
- **`zap` paths** were verified on a prod machine. `~/Library/HTTPStorages/` is deliberately absent (Cmdr doesn't create
  it).

## Refresh and test before submitting

If a newer version shipped since the cask file was last touched, update `version` and `sha256`:

```bash
VERSION=$(curl -s https://getcmdr.com/latest.json | jq -r .version)
curl -sL "https://license.getcmdr.com/download/$VERSION/universal" | shasum -a 256
```

Then test in a throwaway local tap (audit requires a tap; it won't take a bare file path):

```bash
TAP_DIR="$(brew --repository)/Library/Taps/vdavid/homebrew-localtest"
mkdir -p "$TAP_DIR/Casks"
cp apps/desktop/packaging/homebrew/cmdr.rb "$TAP_DIR/Casks/"
(cd "$TAP_DIR" && git init -q && git add -A && git commit -qm init)

brew style vdavid/localtest          # expect: no offenses
brew livecheck vdavid/localtest/cmdr # expect: current version on both sides
brew audit --new --cask vdavid/localtest/cmdr && echo PASS

brew untap vdavid/localtest          # cleanup
```

Don't `brew install` the cask on a machine that already has Cmdr in `/Applications`; it'll conflict with the real
install. Full install testing needs a clean machine or a VM.

## Submitting

The PR to `Homebrew/homebrew-cask` is opened by a human, never an agent (`no-external-actions`). Steps for the human:

1. Fork `Homebrew/homebrew-cask`, add the cask as `Casks/c/cmdr.rb`, one commit named `cmdr 0.24.0 (new cask)` (their
   convention: `token version (new cask)`).
2. Open the PR with their template. Be upfront about affiliation with Cmdr; reviewers ask, and honesty reads far better
   than a discovered connection.
3. The one real acceptance risk is the human "app is too obscure" judgment (the star thresholds only apply to
   repo-hosted casks, which we deliberately aren't). Counter it in the PR description: link press coverage, download
   numbers, and the website.

If a reviewer asks to swap the `url` for the final GitHub asset URL (a common simplification request), don't: that
resurfaces the star check. Instead offer the fallback we've already scoped: serve the DMG bytes directly (200) from the
API Worker backed by an R2 bucket (upload step in `release.yml` + a streaming route, keeping the D1 logging), and point
the cask at that.

## After it's merged

- Update the README's Installation section to announce `brew install --cask cmdr`.
- Nothing else recurring: `BrewTestBot` bumps the cask from `latest.json` on each release. If we ever rename the
  endpoint, move `latest.json`, or change the DMG layout, the cask in `Homebrew/homebrew-cask` must be updated too.
