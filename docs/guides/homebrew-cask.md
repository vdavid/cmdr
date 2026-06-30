# Homebrew cask

How Cmdr ships through Homebrew, and how the cask stays current.

Users install with `brew tap vdavid/tap && brew trust --cask vdavid/tap/cmdr && brew install --cask cmdr`. The live cask
lives in the personal tap repo [`vdavid/homebrew-tap`](https://github.com/vdavid/homebrew-tap) (as `Casks/cmdr.rb`).

The `brew trust` step is required as of Homebrew 6.0.0 (2026-06-11): Homebrew now refuses to load a cask from any
third-party tap until the user explicitly trusts it (`brew trust --cask <tap>/<cask>`), since a tap is arbitrary Ruby
that runs with the user's privileges. It's a client-side consent gate with no publisher-side opt-out — there's nothing
the tap can do to pre-bless itself. Landing the cask in `Homebrew/homebrew-cask` (the notability path below) is what
removes both the tap and the trust step, since the official taps are trusted by default.

The cask file in this repo, [`apps/desktop/packaging/homebrew/cmdr.rb`](../../apps/desktop/packaging/homebrew/cmdr.rb),
is the source of truth for the cask's **shape**: the `url`, `livecheck`, `depends_on`, `app`, and `zap` blocks. The tap
copy carries that exact shape; release CI rewrites only its `version` and `sha256` lines on each release. So edit
structural changes here, and let CI propagate version bumps to the tap.

## Release automation: the tap bump

The `bump-tap` job in [`release.yml`](../../.github/workflows/release.yml) runs after the release is published. It
downloads the universal DMG release asset, computes its sha256, clones the tap, rewrites only the `version` + `sha256`
lines in `Casks/cmdr.rb`, and pushes a `cmdr <version>` commit. Releases need zero extra work for brew.

### `HOMEBREW_TAP_TOKEN` setup

The bump job authenticates to the tap with the `HOMEBREW_TAP_TOKEN` Actions secret (stored in `vdavid/cmdr`). It skips
cleanly when the secret is absent, so a missing token never fails a release. To create it:

1. GitHub > Settings > Developer settings > Fine-grained personal access tokens > Generate new token.
2. Resource owner: `vdavid`. Repository access: **Only select repositories** > `vdavid/homebrew-tap`.
3. Permissions: **Contents** > **Read and write** (nothing else).
4. Copy the token, then add it as an Actions secret named `HOMEBREW_TAP_TOKEN` in `vdavid/cmdr` (Settings > Secrets and
   variables > Actions).

Scope the token to `vdavid/homebrew-tap` only: the bump job needs no other repo.

## Why the cask looks the way it does

This shape also satisfies a future `Homebrew/homebrew-cask` resubmission, so keep it intact.

- **`url` points at `license.getcmdr.com/download/#{version}/universal`, not GitHub.** The 302 to GitHub Releases behind
  it is fine: `brew audit` regex-matches only the _declared_ `url`/`homepage` strings, and never follows redirects. A
  `github.com` URL in the cask would auto-fail the notability audit (needs 75+ stars; 225+ for self-submissions). Keep
  `github.com` out of the cask file. Bonus: the endpoint logs downloads to D1, so brew installs show up in our
  telemetry.
- **`livecheck` reads `https://getcmdr.com/latest.json`**, the same Tauri updater manifest the app polls. The release
  flow already publishes it.
- **`auto_updates true`**: Cmdr ships its own updater, so `brew upgrade` skips it unless `--greedy`.
- **`depends_on macos: :monterey`** means "Monterey or later" (current Homebrew semantics) and matches
  `minimumSystemVersion` in `tauri.conf.json`. Floor rationale:
  [`docs/notes/system-requirements-and-es2025.md`](../notes/system-requirements-and-es2025.md).
- **`zap` paths** were verified on a prod machine. `~/Library/HTTPStorages/` is deliberately absent (Cmdr doesn't create
  it).

## Refresh and test locally

If you change the cask's shape, test it in a throwaway local tap (audit requires a tap; it won't take a bare file path).
To refresh `version` and `sha256` by hand for a test (CI does this automatically on release):

```bash
VERSION=$(curl -s https://getcmdr.com/latest.json | jq -r .version)
curl -sL "https://license.getcmdr.com/download/$VERSION/universal" | shasum -a 256
```

Then test in the throwaway tap:

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

## Resubmitting to Homebrew/homebrew-cask

The tap is the live channel until `vdavid/cmdr` clears Homebrew's notability bar. Then the goal is a tap-free
`brew install --cask cmdr` straight from `Homebrew/homebrew-cask`.

The self-submission bar is 90 forks / 90 watchers / 225 stars. PR
[#268854](https://github.com/Homebrew/homebrew-cask/pull/268854) was rejected at ~15 stars under that bar. The cask
points at a domain-served download URL rather than a `github.com` one, which avoids only the **automated** notability
check, not the maintainer's human "is this repo notable?" judgment. So a domain URL alone doesn't clear the bar.

Resubmit when the repo clears ~225 stars. The cask file needs no changes for that: its shape already passes `brew audit`
and `brew style`. The submission PR is opened by a human, never an agent (`no-external-actions`). Steps for the human:

1. Fork `Homebrew/homebrew-cask`, add the cask as `Casks/c/cmdr.rb`, one commit named `cmdr <version> (new cask)` (their
   convention: `token version (new cask)`).
2. Open the PR with their template. Be upfront about affiliation with Cmdr; reviewers ask, and honesty reads far better
   than a discovered connection.

If a reviewer asks to swap the `url` for the final GitHub asset URL (a common simplification request), don't: that
resurfaces the automated star check. Instead offer the fallback: serve the DMG bytes directly (200) from the API Worker
backed by an R2 bucket, keeping the D1 logging, and point the cask at that.

Once accepted, `Homebrew/homebrew-cask` becomes the canonical home, `BrewTestBot` auto-bumps the cask from `latest.json`
via the `livecheck` block, and the `bump-tap` CI job + the tap repo can retire.
