# Releasing

How to release a new version of Cmdr. An agent runs the whole flow by following § The release procedure: it drafts the
CHANGELOG, monitors the build, verifies the public surface afterwards, and handles failure recovery. The human's role is
to review the CHANGELOG draft, confirm the version, and click any macOS permission prompts.

Related guides for the signing and distribution steps: `apple-signing-and-notarization.md` and `homebrew-cask.md`.

## Prerequisites

- The signing secrets in the `release` GitHub environment (next section).
- Where the update archives get signed, set by the `RELEASE_UPDATE_SIGNING` variable: in `ci` mode the updater key is
  among those secrets, in `local` mode it's in this laptop's sops store (§ Who signs the update archives).

## Signing secrets

The secrets that sign, notarize, and updater-sign a build live in the `release` environment on `vdavid/cmdr`, and only
the `sign` and `pkg` jobs declare `environment: release` (`compile`, which runs the third-party build code, doesn't: §
How a release build is split). The environment's deployment policy admits `v*` tags only, with no required reviewers
(releases run unattended), and no signing secret exists at repo level, so a workflow on a branch can't reach them. Where
each value comes from: the vault note `projects/Cmdr/workflow/Cmdr signing keys.md`.

- **The eight secrets**: `APPLE_API_KEY`, `APPLE_API_KEY_BASE64`, `APPLE_API_ISSUER`, `APPLE_SIGNING_IDENTITY`,
  `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_INSTALLER_CERTIFICATE`, and
  `APPLE_INSTALLER_CERTIFICATE_PASSWORD`. The `.p12` files and their passwords come from Bitwarden, which agents can't
  reach; set or replace one with `gh secret set <NAME> --env release -R vdavid/cmdr` (it prompts for the value).
- ❌ **Never add a signing secret at repo level.** `release.yml` calls the pipeline with `secrets: inherit`, so a
  repo-level copy reaches every job, and any branch workflow could read it. (The repo-level copies were deleted on
  2026-10-08, after v0.51.0 signed and notarized from the environment alone.)
- **The updater key isn't on GitHub at all**: it signs on the laptop (§ Who signs the update archives). Its
  `TAURI_SIGNING_*` copies were deleted from the `release` environment on 2026-10-15, after v0.53.0, the first `local`
  release, shipped and an installed copy updated to it. Going back to `ci` mode puts them back (§ Switching modes).

## How a release build is split

Each arch builds in two jobs of `release-pipeline.yml`, so no third-party build code runs where the signing material is.
Two `compile` jobs (aarch64, x86_64) feed three `sign` jobs (aarch64, x64, universal). Compiling runs every npm install
script, Vite plugin, and crate `build.rs`, and the 2026-10-15 incident tabletop found all of that sharing one
`tauri-action` step with every signing secret.

- **`compile`** has no environment, a read-only token, and no credential left in `.git/config`. It installs the npm
  packages, then runs `pnpm tauri build --no-bundle`: `beforeBuildCommand` (the llama-server fetch and the Vite build),
  then cargo, stopping before the bundler, the only part of `tauri build` that signs. It hands on one file,
  `target/<triple>/release/Cmdr`, as the `compiled-<arch>` artifact.
- **`sign`** (`environment: release`) starts on a fresh runner once both `compile` jobs are green. It takes exactly that
  one regular file per arch and checks its `lipo -archs`. The universal job takes both and joins them with
  `lipo -create` into `target/universal-apple-darwin/release/Cmdr`, which is all
  `tauri build --target universal-apple-darwin` does after compiling both arches one after the other (that third compile
  was the release's critical path, 52 min in v0.55.0's run against 33 for aarch64). Then it installs the Tauri CLI with
  `pnpm install --ignore-scripts`, and fetches llama-server again (pinned by SHA-256). Only then does it set up the
  signing keychain and the notarization key, sign the llama-server binaries (`download-llama-server.go sign`, since the
  bundler signs the app but not the resources it copies in), and run `tauri bundle`, which bundles, signs, notarizes,
  staples, and updater-signs. It deletes the signing material, runs the macOS floor checks on the bundled binary, and
  uploads.
- **Why two jobs, not two steps**: code from one step can leave a process or a patched tool behind on its runner and
  read a later step's environment or keychain. A new job is a new machine.
- **What still runs next to the secrets**: the Tauri CLI (it's the signer), pnpm and node to start it, Go running our
  stdlib-only script, `cargo metadata`, Apple's tools, and the pinned actions. `tauri bundle` runs no
  `beforeBuildCommand` and no cargo build (verified on Tauri CLI 2.12.1 by reading the `bundle` command's source in the
  tauri repo, 2026-10-15).
- **What it doesn't protect**: the binary itself. A malicious dependency can still change what `compile` produces, and
  `sign` signs it. The split keeps the keys out of its reach, not the build.
- **The upload** names the assets `Cmdr_<version>_<arch>.dmg` and `Cmdr_<version>_<arch>.app.tar.gz[.sig]` (the bundler
  writes the DMG name; the archive is renamed from `Cmdr.app.tar.gz`). In `ci` mode the first job to get there creates
  the release (published, like every `ci` release) and the others find it; in `local` mode it uploads only into the
  `draft` job's draft. A draft in `ci` mode, or a published release in `local` mode, fails the job. Each file gets four
  tries with `--clobber`, since GitHub's asset API fails often enough to have cost v0.46.0 two runs.
- **Verified locally** (Tauri CLI 2.12.1, macOS 27.0, 2026-10-15): a `--no-bundle` compile, then `tauri bundle` on a
  copy of the binary in an empty target dir, gave the same `Cmdr.app` tree (file modes included), `Info.plist`, updater
  archive listing, and bundle folder as a one-step `tauri build`, with no `create-dmg` on `PATH` (the bundler ships its
  own `bundle_dmg.sh`). The DMGs came out `_aarch64`, `_x64`, and `_universal`. **Verified on GitHub** with the
  `v0.52.1-pipelinetest.1` test tag (run 38755716207, 2026-10-15): every job green, the three bundles Developer ID
  signed, notarized, and stapled, and an installed app updated itself to one
  (`docs/notes/2026-10-15-update-path-proof.md`).
- **The `lipo`'d universal app matches a one-step universal build** (Tauri CLI 2.12.3, `v0.55.1-pipelinetest.2`, run
  39741595209, 2026-10-22). Tauri's universal build is `lipo -create` of the two arch binaries into
  `target/universal-apple-darwin/release/Cmdr`
  ([`desktop.rs`'s `build`](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.3/crates/tauri-cli/src/interface/rust/desktop.rs)),
  and nothing else in the CLI or bundler special-cases the universal target. Against v0.55.0's universal app: identical
  file tree and modes, identical updater-archive listing, `Info.plist` differing only in the version, and every
  llama-server file byte-identical once its signature is stripped. Each slice of the universal binary is byte-identical
  to that arch's own app binary, signatures stripped. `codesign --verify --deep --strict` and `spctl` passed (Notarized
  Developer ID).

## The release gate

No update archive gets signed or published unless the macOS updater would install it. Since the updater requires Cmdr's
Developer ID code signature on the bundle (`apps/desktop/src-tauri/src/updater/DETAILS.md` § Code signature), a release
whose bundle fails that check would strand every install on the version before it, and no later update could reach them
(only a manual download). The gate runs the app's own code, `cmdr-vet-update` (`crates/cmdr-update-trust/DETAILS.md`):
it unpacks each archive with the installer's extraction, checks the code signature, and requires the bundle's version to
be the release's.

- **On the laptop**, `release-finish.sh` runs it after the provenance check and before signing, dry runs included, and
  refuses to sign if any archive fails.
- **On GitHub**, the `vet-update-archives` job (macOS) runs it on the release's three archives, and `publish` needs it,
  in `ci` and `local` mode alike. In a `local` tag push it runs right after `sign`, so a bad build shows up before the
  laptop is asked to sign it.
- **By hand**: `cargo run --release -p cmdr-update-trust --bin cmdr-vet-update -- --version X.Y.Z <archives>`.

## Testing the pipeline without releasing

In `local` mode a tag push builds only into a private draft (`publish` and `bump-tap` skip, `latest.json` stays), so a
pre-release tag tests the whole flow for real: the release commit, its CI run beside the build, signing, notarization,
the gate, provenance. Done for `v0.52.1-pipelinetest.1` on 2026-10-15 (the record and the installed-app half:
`docs/notes/2026-10-15-update-path-proof.md`) and for the release flow itself with `v0.55.1-pipelinetest.4` on
2026-10-22 (`docs/notes/2026-10-22-faster-release-flow.md`).

1. Confirm `RELEASE_UPDATE_SIGNING` is `local`. In `ci` mode the same push would PUBLISH, so `release.sh -test` refuses.
2. In a clean worktree whose branch carries what you're testing (the pipeline runs the workflow files at the tag's
   commit): `./scripts/release.sh -test X.Y.Z-pipelinetest.N`, with an `N` no tag on origin has yet
   (`git ls-remote --tags origin 'v*pipelinetest*'`). It runs the release prep on top of HEAD, with a one-line changelog
   section and no settings-defaults entry, then commits, tags, pushes, and starts CI and CodeQL as for a real release.
   The guard's `sort -V` ranks the pre-release above the live version.
3. While CI runs, `./scripts/release-finish.sh -dry-run -out <dir> X.Y.Z-pipelinetest.N` refuses (the run is still
   going), and `./scripts/release-ci-gate.sh X.Y.Z-pipelinetest.N` follows it.
4. When the build and CI are both green: the same dry run (dry runs take a pre-release version; never a real run on a
   test tag), then `codesign --verify --deep --strict` and `spctl -a -vv` on each extracted app.
5. Clean up: `./scripts/release.sh -abort X.Y.Z-pipelinetest.N` stops the test's runs and deletes its artifacts, draft,
   tag, and branch, on GitHub and locally. The run's attestations stay (on the repo and in Sigstore's public log) and
   describe assets nobody can fetch.

## Who signs the update archives

The updater key (minisign, key ID `A2601F36BB168C0A`) is the hardest release secret to rotate: its public half is
compiled into every installed copy, beside an offline backup key's (`B72B6B32B7D71B58`) that exists to replace it (§
Rotating to the backup updater key). With the Developer ID identity, whoever holds the private half can ship an update
to every Mac; alone, it can only re-sign genuine releases, since the updater also requires Cmdr's Apple code signature.
The repository variable `RELEASE_UPDATE_SIGNING` picks where it signs, read by the `guard` job of each tag push:

- **`ci`, or unset**: the tag push does everything, and the `sign` job signs each archive with `TAURI_SIGNING_*` from
  the `release` environment.
- **`local`**: the key never leaves this laptop. Decided 2026-10-06; the threat-model discussion is private issue
  `vdavid/cmdr-reports#39`.

Read it with `gh variable get RELEASE_UPDATE_SIGNING -R vdavid/cmdr` (an error means unset, so `ci`). Any other value
fails the guard. Both modes stay implemented in the same files (`release-pipeline.yml`, `release.yml`), so switching is
the variable alone.

### How a `local` release runs

1. **`release.sh`** commits the release on top of `origin/main`, tags that commit, pushes both, and starts the release's
   full CI run and CodeQL beside the build (§ The CI gate).
2. **The tag push's run** builds without publishing: `guard`, then `draft`, which creates the draft release up front
   (three parallel jobs each looking for a draft by tag could each create one; drafts can't be fetched by tag).
   `ci-gate` skips: nothing here is public, and CI is still running beside it. The three `compile` jobs and then the
   three `sign` jobs build into it: the DMGs and `.app.tar.gz` archives, no `.sig` files. `sbom` runs beside them, and
   `attest` attests the draft's assets. `publish` and `bump-tap` skip, so `latest.json` doesn't move and nothing is
   public.
   - The Tauri CLI won't bundle without an updater key once the config has a pubkey, and the archive must still come out
     of the bundler in the shape `ci` mode ships. So each `sign` job makes a throwaway key (a mismatch the bundler only
     warns about) and keeps its signatures on the runner (its upload step skips `.sig` files in `local` mode).
3. **On the laptop, `./scripts/release-finish.sh X.Y.Z`** (step 8 of § The release procedure starts it right after
   `release.sh`). It waits for that run and the release's full CI run to go green, failing at the first red CI job, then
   requires the CodeQL gate. It downloads the three archives and runs `gh attestation verify` on each, pinned to
   `release-pipeline.yml` at the tag's commit (`--signer-digest`, `--source-digest`, `--source-ref`) on a GitHub-hosted
   runner. It refuses to sign anything that doesn't verify, or that the updater would refuse to install (§ The release
   gate). Then it signs with `tauri signer sign`, the key and password read from sops (`CMDR_TAURI_SIGNING_PRIVATE_KEY`,
   `CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) into the signer's environment only, verifies each new signature against
   the pubkey in `tauri.conf.json` with its own minisign verifier (the check the app runs), uploads the `.sig` files,
   reads them back, and dispatches `release.yml` on the tag.
4. **The dispatched run** finishes it: `guard` (it also refuses a dispatch on a branch), `ci-gate` (the CI gate once
   more, server-side), `vet-update-archives` (§ The release gate), then `publish`, which needs both, verifies every
   archive's `.sig` against the app's trusted keys (main or backup), publishes the draft, and then runs exactly the `ci`
   mode steps (`latest.json` from the `.sig` assets, checksums, release notes, marking the release Latest, the website
   deploy), then `attest` and `bump-tap`. `draft`, `compile`, `sign`, and `sbom` skip.
5. **Back into `main`**: `./scripts/release.sh X.Y.Z` again, once the release is published (§ Merging the release back
   into `main`).

`release-finish.sh` is resumable: re-run it after anything (a closed laptop, a red job). It holds no state of its own
and reads where the release stands on GitHub. A build or CI run still going gets waited for, a draft gets signed
(re-signing a draft is harmless, since nothing reads its signatures yet), a published release skips to the finishing
run, a red finishing run gets its failed jobs re-run once (a fresh dispatch would meet the guard, since `publish` may
have moved the manifest already), and a green one ends it. It doesn't read the variable: a release finishes in the mode
its tag push ran in.

**Dry run**: `./scripts/release-finish.sh -dry-run -out <dir> X.Y.Z` checks the CI gate once (it refuses while the run
is red or still going, never waits), stops after verifying the new signatures, and touches nothing on GitHub; it works
on a published release too. `-signer-workflow` (dry runs only) checks a release from before the reusable workflow, which
`release.yml` signed. (Verified on v0.50.0, 2026-10-06: provenance verified with
`-signer-workflow vdavid/cmdr/.github/workflows/release.yml`, the sops key signed all three archives and each signature
verified against the app's pubkey; the default pin refused the same archives; the verifier accepted v0.50.0's CI-made
signatures and rejected one swapped across arches.)

`release-finish.sh` asks `gh repo view` for the repo's current name instead of pinning it, and verifies an attestation
against the repo its signer workflow names. So the first release after a move to the `getcmdr` org (#476) needs no code
change, and a dry run of a pre-move release passes
`-signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml`.

### What `local` mode protects, and what it doesn't

- **Protects against the key leaking from GitHub**: an exfiltrated secret, a workflow on a branch, or a compromised
  action step reading its environment. There's no copy of the key on GitHub to take.
- **Protects against an archive swapped on the release**: one that wasn't built by the pipeline at the signed tag has no
  matching provenance, so the laptop refuses it, and one swapped after signing fails the app's signature check.
- **Doesn't protect against a malicious build from inside the pipeline**: a compromised third-party action or dependency
  in the `compile` job produces an archive with valid provenance, and the laptop signs it. Provenance proves where an
  archive was built, not what went into it.
- **Moves the key's risk to the laptop**, which holds it in sops (the age key in its Keychain) either way.

### Attestations in `local` mode

`attest` runs twice. The tag push attests the draft's assets (DMGs, archives, SBOMs) before anything is signed, which is
what the laptop verifies; the draft's bytes are final, so "attest as published" still holds. The dispatch attests the
published release again, which adds `latest.json` and `checksums.txt` (and repeats the rest, which is harmless). It
skips the `.sig` files: the laptop made them, so provenance naming the workflow would be false, and a minisign signature
is its own proof of origin.

### Switching modes

- **To `local`**: `gh variable set RELEASE_UPDATE_SIGNING -R vdavid/cmdr --body local`, before pushing the tag.
- **Back to `ci`** (the revert):
  1. `gh variable set RELEASE_UPDATE_SIGNING -R vdavid/cmdr --body ci` (or `gh variable delete`).
  2. Put the key back in the `release` environment from sops, piped so the values never reach the shell history or the
     screen:
     `secret CMDR_TAURI_SIGNING_PRIVATE_KEY | gh secret set TAURI_SIGNING_PRIVATE_KEY --env release -R vdavid/cmdr`,
     then the same for `CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD` into `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
  3. Release as usual. § The release procedure reads the variable and skips `release-finish.sh` in `ci` mode.

### Rotating to the backup updater key

Every build after 0.52.0 trusts two updater keys: the main key (`plugins.updater.pubkey` in `tauri.conf.json`, private
half in sops) and an offline backup (`apps/desktop/src-tauri/updater-backup.pub`, key ID `B72B6B32B7D71B58`, private
half only in David's Bitwarden as "Cmdr Tauri BACKUP updater key (offline)", never in sops, CI, or GitHub). An update
signed by either verifies, in the app and in `release-finish.sh`. How the app checks: the updater `DETAILS.md` § Two
updater keys.

**When**: the main key's private half leaked, or it's lost. One release signed with the backup key moves every install
that takes it onto a new pair. If you still hold the main key, signing the rotation release with it works too, and it
also reaches installs from 0.52.0 or earlier, which trust only the main key and refuse a backup-signed update.

**What the rotation release changes** (in one commit, before tagging):

1. **The backup becomes the main key**: copy `updater-backup.pub` into `plugins.updater.pubkey` in `tauri.conf.json`,
   and into `MAIN_PUBKEY_BASE64` in the updater's `signature.rs` (a test pins the two equal).
2. **A new backup**: David generates it offline, `pnpm exec tauri signer generate -w <file>`, stores the private key and
   its password in Bitwarden as the new backup item, deletes the file, and puts the new public key in
   `updater-backup.pub`.
3. **The leaked key is gone from the trust list**, since it's in neither place anymore.
4. Update the pinned key IDs: the Rust test `the_trusted_keys_are_the_main_key_then_the_backup`, the Go tests in
   `scripts/release-finish/minisign_test.go` (`cmdrPubkey`, the two key-ID tests), this guide, the updater `DETAILS.md`,
   `docs/threat-model.md`, and the vault note `projects/Cmdr/workflow/Cmdr signing keys.md`.

**How to sign it** (`RELEASE_UPDATE_SIGNING` must be `local`; in `ci` mode the sign job uses the GitHub copy of the main
key):

1. Release as usual up to the push (§ The release procedure).
2. Instead of the plain `release-finish.sh`, David copies the backup key and its password from Bitwarden into the shell,
   prompted so neither lands in the history:
   `read -rs CMDR_BACKUP_UPDATER_KEY; read -rs CMDR_BACKUP_UPDATER_KEY_PASSWORD; export CMDR_BACKUP_UPDATER_KEY CMDR_BACKUP_UPDATER_KEY_PASSWORD`
3. `caffeinate -i ./scripts/release-finish.sh -backup-key X.Y.Z`. It signs with the key from those two variables and
   never touches sops for it; the verification line names the key that signed (`key B72B6B32B7D71B58`).
4. Once it's published, `unset CMDR_BACKUP_UPDATER_KEY CMDR_BACKUP_UPDATER_KEY_PASSWORD`. The old backup is now the main
   key, so it takes the main key's place in sops (`secret edit`: `CMDR_TAURI_SIGNING_PRIVATE_KEY` and
   `CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), replacing the leaked one. From then on releases sign the usual way.
5. Update an installed copy and confirm it reaches the new version (§ Emergency release during an incident has the rest
   of the after-checks).

**Installs that never update keep trusting the leaked key**, and an older version's trust list can't be changed from
outside. On macOS the leaked key alone still can't ship a build to them, because the updater also requires Cmdr's
Developer ID code signature; with the Developer ID identity as well, it could. Linux builds have no code-signature
check.

## The release procedure

1. **Check the prerequisites**: find out who signs the update archives,
   `gh variable get RELEASE_UPDATE_SIGNING -R vdavid/cmdr` (an error means unset, which is `ci`; § Who signs the update
   archives).
   - **`ci`**: run `gh secret list --env release` and verify that `TAURI_SIGNING_PRIVATE_KEY` and
     `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` both exist. If either is missing, warn the user and stop (§ Switching modes
     puts them back).
   - **`local`**: verify the key is readable from sops without printing it:
     `secret CMDR_TAURI_SIGNING_PRIVATE_KEY > /dev/null && secret CMDR_TAURI_SIGNING_PRIVATE_KEY_PASSWORD > /dev/null`.
     If it fails, warn the user and stop.
   - **CI is enabled**, since the CI gate and `land` wait on it. David sometimes disables it to save GHA minutes; if
     `gh workflow list --all` shows `CI` as `disabled_manually`, `gh workflow enable CI`, tell the user, and ask whether
     to disable it again after the release.
   - **No known exploitable vulnerability ships** (EU Cyber Resilience Act, Annex I Part I (2)(a)). List the open
     security gaps in the private tracker, the ones `docs/threat-model.md` calls "tracked privately":
     `gh issue list -R vdavid/cmdr-reports --label security-gap --state open`. For each, read it and say whether this
     release's code is exploitable through it. David confirms. An exploitable one stops the release until it's fixed,
     unless David writes a risk decision on its issue. Then add one line to the vault's release log,
     `projects/Cmdr/business/CRA technical file/Release log.md`: the date, the version (filled in at step 6), the issue
     numbers reviewed, and "none exploitable" or links to the risk decisions. Step 13 adds the evidence under it. ❌ No
     exploit detail in this repo; the line stays in the vault.
2. **Sync `main`.** Everything the release ships must be landed already (`pnpm land`, `scripts/land/CLAUDE.md`). In the
   main clone: `git pull --ff-only origin main`. Nothing waits on `main` from here on: `release.sh` puts the release
   commit on top of whatever `origin/main` is when it runs, and its one CI run tests exactly that commit (§ The CI
   gate). A commit landing after this sync ships too, unlisted in the changelog unless you add it.
3. **Draft the CHANGELOG** from the git commits since the last release, per § Writing the CHANGELOG, and present the
   draft for review.
   - **Ask David: "Does this release fix a vulnerability users should act on?"** Yes means an entry per fixed
     vulnerability in the security advisory list, beside its CHANGELOG Security line, per § Security advisories. Most
     releases add nothing. Leave the edit uncommitted: it rides the release commit, like the changelog.
4. **Self-hosted runner only** (`release-pipeline.yml`'s `build.runs-on` is GitHub-hosted `macos-26` today, so normally
   skip): right after presenting the draft, read the runner node's Finder Automation `auth_value` per § "`bundle_dmg.sh`
   hangs ~2 minutes then fails on every matrix job", so a denial surfaces before anything is tagged. `0` blocks the
   release until fixed; empty or `1` means the user stays at the keyboard to click Allow.
5. **Apply the roadmap and feature-status updates** (edit the files, don't just advise; the user reviews before
   committing).
   - **Roadmap** (`apps/website/src/lib/roadmap.ts`, the milestones; `roadmap.astro` is only the layout): add a dated
     milestone for each major development this release, and tick off or remove any "coming soon" item that just shipped.
     Match the existing curation: milestones only, not every release. Group under the month of the actual main
     development (from the commits), not the release date, adding a new `<h3>` when the month rolls over.
   - **Feature status** (`feature-status.json` at the repo root, the single source of truth behind the `/features` page
     and the in-app badges): review every feature against what shipped. Flip `planned` → `alpha` for a feature that just
     launched, graduate `alpha` → `beta` → `stable` as one matures, and refresh any note the release made stale. Keep
     notes to one honest line, website voice (no "I"/"we"). Schema and consumers: `docs/feature-status.md`. Graduating
     `search` or `select-files` out of `alpha` also means updating the pinned assertion in
     `apps/desktop/src/lib/feature-status.test.ts`. A new feature id also needs its card in
     `apps/website/src/pages/features.astro`, or the page fails to build; `release.sh` lets that file ride the release
     commit too. Present the diff for review.
   - **If either file changed, start the website for David** so he can check `/roadmap` and `/features` live: stop any
     running one first (`lsof -ti tcp:4829 -sTCP:LISTEN | xargs kill`; `astro dev stop` can't see it, since
     `--ignore-lock` skips the lock file), then run `pnpm dev:website` as a background command, confirm the URL answers,
     and give it to him. The script passes `--ignore-lock`, which keeps Astro in the foreground; without it, Astro
     7.3.8+ detaches the server whenever it detects an agent's env vars.
   - **Check the prep before asking for the go**: `pnpm check -m` (a plain full default run, about 5 minutes; `-m`
     because release prep happens in the main clone), and fix what it finds. The prep edits stay uncommitted until
     `release.sh` commits them, so no commit hook ever sees them, and a broken edit would otherwise surface only as a
     red release CI run. One finding is expected: a new security advisory entry fails `desktop-security-advisories` (its
     `fixedIn` is newer than the app version until `release.sh` bumps it).
6. **Advise the version** (patch: bug fixes, minor: new features, major: major launches), and give the user the
   `./scripts/release.sh x.x.x` command.
7. **Offer to run the release script**, and wait for confirmation before running it. Run it as a background command (a
   few minutes): it syncs `main`, runs the release-prep checks (§ Release prep checks that abort before tagging),
   commits `chore(release): vX.Y.Z` (call it R) on a `release/vX.Y.Z` branch on top of `origin/main`, signs the tag on
   R, pushes the branch, and starts R's full CI run and CodeQL on it. In `local` mode it pushes the tag right away, so
   the build (about an hour) runs beside CI (25–40 min); in `ci` mode, whose tag push publishes on its own, it waits for
   green CI first. Re-run it with the same version after any stop: it resumes. If it stops before tagging, fix what it
   says and re-run; if CI goes red later, § The CI gate says what to do.
8. **`local` mode: start `./scripts/release-finish.sh X.Y.Z` right after `release.sh`**, as a background command. It
   waits for the build and R's CI together, printing each job as it moves, and stops at the first red CI job (§ The CI
   gate: a flake gets its failed jobs re-run, a real failure aborts the release). Once both are green it requires the
   CodeQL gate, verifies each update archive's provenance, signs on this laptop, uploads the signatures, and dispatches
   the finishing run that publishes, then waits for that too. It's resumable: if it stops (a red job, a closed laptop),
   fix the cause and run it again; never sign or publish by hand. Its failure messages map to § `release-finish.sh`
   stopped. The laptop has to stay on until it ends, so tell the user that instead of the "laptop is free" note in
   step 10.
9. **Confirm the build started**: wait ~30 seconds, then run `gh run view <release-run-id> --json jobs` and check the
   `Compile (...)` jobs. On GitHub-hosted runners both should be `in_progress` together; the three
   `Sign and notarize (...)` jobs start once both are green. Self-hosted: see § Keep the Mac awake during the build.
10. **Monitor the build.** Tell the user their laptop is free (the build runs on GitHub's hardware). Poll `gh run view`
    every few minutes in the background and report progress (which jobs are done, which are still running). Report when
    all jobs complete; if one fails, show the failure details and advise how to fix it. Suggest the user also track it
    at https://github.com/vdavid/cmdr/actions.
11. **Merge the release back into `main`** once it's published: `./scripts/release.sh X.Y.Z` again, as a background
    command (§ Merging the release back into `main`). Its land puts the version bump and the changelog on `main`, and
    `main`'s push run that follows deploys the roadmap, feature-status, and landing-page changes (the release workflow
    only deploys the new `latest.json`). If it stops or goes red, fix it in the background and surface it when
    convenient; it doesn't block the shipped release.
12. **After the release run succeeds (in `local` mode: after `release-finish.sh` reports the finishing run green),
    verify the public surface** with `./scripts/release-evidence.sh verify X.Y.Z` (about a minute; what it checks:
    `scripts/release-evidence/main.go`). It stops at the first thing wrong and says what:
    - **A missing or tiny asset**: the three DMGs, the three `.app.tar.gz` and their `.sig`, `latest.json`,
      `checksums.txt`, the three `.cdx.json` SBOMs, `Cmdr_X.Y.Z.intoto.jsonl`, and `Cmdr_X.Y.Z_universal.pkg`. A release
      that shipped without the pkg on purpose (§ The installer package) takes `-no-pkg`.
    - **`getcmdr.com/latest.json` still on the old version** after its two-minute wait: the deploy webhook may have
      failed silently. Tell the user (the fix is re-triggering the website-deploy workflow via `workflow_dispatch`), but
      don't block release success on it, since the GitHub release is what users download.
    - **An updater archive URL that doesn't name this release's file or doesn't answer 200**: the workflow spells these
      `.app.tar.gz` names out by hand in two places (the `sign` job's upload and `publish`), so a naming drift between
      them surfaces here and nowhere else until installs stop updating.
    - **A red `attest` job** (in `local` mode it checks both the tag push's run and the finishing run) **or a failed
      `gh attestation verify`** on the Apple Silicon DMG (both commands in § Provenance and SBOM attestations, pinned to
      the tag's commit). Neither blocks release success: see § "The attest or sbom job failed".
    - **No green full CI run or CodeQL run** behind the release (step 13 says which runs it looks for). The release gate
      should have left both, so this means the tag skipped the gate.

    Once it passes, **stop the website dev server** started in step 5: the deployed site has replaced it.
    `lsof -ti tcp:4829 -sTCP:LISTEN | xargs kill` (its `--ignore-lock` hides it from `astro dev stop`).

13. **Record the release evidence** (EU Cyber Resilience Act, Annex VII 6 and 8). The technical file keeps each
    release's evidence for 10 years, and GitHub drops run logs and artifacts after 90 days, so every release saves its
    own. ❌ No archive host, share, or path in this repo: the release log note in the vault (the one step 1 wrote to)
    names the archive folder and the three environment variables below.
    - **Run** `./scripts/release-evidence.sh evidence X.Y.Z` with `CMDR_RELEASE_EVIDENCE_ARCHIVE` (the archive folder,
      `//host/share/path`; `-archive` overrides it), `SMB2_USER`, and `SMB2_PASS_COMMAND` set. Add `-old-macos-smoke`
      when § Pre-release smoke test on old macOS ran for this release. It runs step 12's checks again, then:
      - finds the runs: the full (`run_all`) `ci.yml` run on the tagged `chore(release)` commit, else on its parent
        (where the release gate passed when `release.sh` didn't need a run of its own), the `codeql.yml` run on that
        same commit, and the `release.yml` runs for the tag (two in `local` mode: the tag push and the dispatch);
      - collects into a fresh `vX.Y.Z` folder (`-out DIR`, default a temp dir): the SBOMs, the `.intoto.jsonl`,
        `checksums.txt`, `latest.json`, each run's jobs JSON and gzipped log, one CodeQL SARIF per language, and
        `attestation-verify.txt`;
      - uploads it with the `smb2` CLI into `<archive>/vX.Y.Z` and lists it back, size for size. A re-run resumes a
        partial upload, and it uploads nothing if the folder holds anything else;
      - prints each test lane's result from the CI log and the release log's evidence sub-bullets.
    - **Paste the sub-bullets** under the release's line in the release log note, then add the judgment the script
      can't: what a `warn` test lane means (a confirmed defect, or load flakiness that passed alone), and what the
      `### Security` section covers. Trim each test lane to its count; the bullet only needs the gist.
    - `-no-upload` collects without uploading, for a dry look; the NAS copy bullet then says so.
14. **Minor or major release: offer to refresh the app-directory listings** (skip for patches), per § Refreshing the
    app-directory listings.

## Emergency release during an incident

A fix release during a declared security incident (`docs/security-incident-procedure.md`) runs § The release procedure
with these differences. The procedure pre-approves building and publishing it, so step 7 doesn't wait for a go-ahead.
`RELEASE_UPDATE_SIGNING` is `local` (set 2026-10-15), so `release-finish.sh` signs on this laptop.

**What needs David, or at least this laptop:**

- **The signed tag doesn't prompt while the SSH agent holds the key.** `release.sh` runs `git tag -s` with the Git SSH
  signing key (`gpg.format ssh`, `user.signingkey` names a passphrase-protected ed25519 key whose passphrase is in the
  login Keychain, loaded into the launchd `ssh-agent` through `UseKeychain` and `AddKeysToAgent`). Every agent commit is
  signed by the same key unattended, and so is the tag. After a reboot the agent starts empty until something loads the
  key: `ssh-add --apple-load-keychain` does it without a prompt once David has logged in. Without the key in the agent,
  the tag can't be signed unattended and `release.sh` aborts before pushing. (Verified on David's laptop with
  `git config`, `ssh-add -l`, and the `v0.52.0` tag's SSH signature, 2026-10-15; the reboot path is untested.)
- **Docker running**: `release.sh` aborts before tagging without it (§ Release prep checks that abort before tagging).
- **The laptop awake and online for about 75 minutes after `release.sh`**, while `release-finish.sh` waits for the build
  and CI, signs, and waits for the finishing run. Run it as `caffeinate -i ./scripts/release-finish.sh X.Y.Z`, on power
  with the lid open. If it stops, re-run it: it's resumable.
- MFA, Bitwarden, and the phone aren't needed: everything else runs on GitHub with credentials already in place.

**Before tagging:**

- Containment may have disabled the release workflow. Re-enable it: `gh workflow enable release.yml -R vdavid/cmdr`.
- CI red for reasons outside the fix? Use the emergency bypass in § The CI gate, and clear `RELEASE_SKIP_CI_GATE_TAG`
  afterwards.
- If the updater key itself leaked, make the fix release the rotation release too: § Rotating to the backup updater key,
  which also says when to sign it with the backup key and when with the leaked one.
- If `main` carries unreleased features, cut the fix from the last release tag instead: § Security-only patch release.

**Timeline**, not yet measured on a real incident: two to three hours of agent work on the fix and changelog, then about
75 minutes from `release.sh` to `latest.json`: the hour-long build with the release's CI run beside it, then signing and
the finishing run. A running copy checks every three hours by default, so it picks the fix up within hours.

**After it ships:** run steps 12 and 13 of § The release procedure, then:

- Update an installed copy (**Cmdr > Check for updates**) and confirm it reaches the new version. For a key rotation,
  confirm the new build's `tauri.conf.json` pubkey is the new one.
- Confirm no release or tag exists that we didn't make (`gh release list -R vdavid/cmdr`), and that `latest.json` names
  ours.
- Clear any bypass variable you set, and log the release with its UTC time in the `INCIDENT:` issue.

## Security-only patch release

When a security fix is ready and `main` already holds unreleased features, ship the fix on its own as a patch release
(X.Y.Z+1) cut from the last release tag. The EU Cyber Resilience Act asks for security updates separate from feature
updates where that's technically feasible (Annex I Part II (2)). If `main` holds nothing since the last tag but the fix
(`git log --oneline vX.Y.Z..main`), release from `main` as usual instead.

**What works from any tag, unchanged**: the whole tag-push pipeline (`guard`, `ci-gate`, `compile`, `sign`, the release
gate, `sbom`, `attest`, `publish`, `bump-tap`) and `release-finish.sh`. They read the tag, and `publish` marks the
release Latest (which is what the website serves) whichever branch the tag sits on. The CI gate takes the tagged
commit's parent's full run when the tagged commit is the `chore(release): vX.Y.Z` commit and has no full run of its own,
on any branch (§ The CI gate), which is what step 4 below gives it.

**What assumes `main` HEAD, and how this procedure gets around it:**

- ❌ **Don't run `release.sh`.** It runs on `main` only and puts its release commit on top of `origin/main`, which would
  ship every unreleased feature. Step 5 does its steps by hand.
- **The pipeline runs from the tag's own commit**, so the workflow files, `release-finish.sh`, and
  `.github/release-signers` are the old tag's. Before cutting, run
  `git diff vX.Y.Z main -- .github/workflows/release.yml .github/workflows/release-pipeline.yml scripts/release-finish.sh scripts/release-finish/ .github/release-signers`.
  A change there that a release depends on (a moved secret, a new signer key) has to come along to the patch branch.
- **The website and `CHANGELOG.md` on `main`** don't see the patch until step 7 merges it back.

Steps:

1. **Prerequisites and the known-gaps check**: § The release procedure, step 1.
2. **Branch from the tag** in a worktree: `~/.claude/scripts/new-worktree.sh security-vX.Y.Z+1 vX.Y.Z`.
3. **Land only the fix**, with its tests: no features, refactors, or unrelated dependency bumps. Write it on this branch
   first. If it already landed on `main`, `git cherry-pick -x` it. A dependency advisory fix bumps only that dependency.
4. **Get the gates green on the branch's HEAD**: push the branch (`git push origin HEAD`), then
   `gh workflow run ci.yml --ref <branch> -f run_all=true` and `gh workflow run codeql.yml --ref <branch>`. Watch the CI
   run (`gh run list --workflow ci.yml --event workflow_dispatch --commit $(git rev-parse HEAD) --limit 1`, then
   `gh run watch <id> --exit-status`), then run `./scripts/release-codeql-gate.sh --wait $(git rev-parse HEAD)`. The CI
   run doesn't deploy the website: `deploy-website` runs on `main` only.
   - **CI from an old tag can go red on what `main` has already fixed**, like a new dependency advisory or a lint from a
     newer tool. Bring over only what the gate needs, as its own commit. A red lane that has nothing to do with the fix
     can take the emergency bypass in § The CI gate.
   - **The CodeQL gate also requires no open high or critical alert on `main`.** That's on purpose: fix or triage the
     alert first.
5. **Changelog, then the release commit and tag.** Add a `## [Unreleased]` section with a one-line lead and only
   `### Security` entries, per § Writing the CHANGELOG, citing this branch's hashes. Add the fix's entry to the security
   advisory list too (§ Security advisories), and include it in the release commit. Run
   `pnpm check changelog-links --fresh`, then do by hand every step of `scripts/release.sh` from the version bump
   (`npm pkg set version=...`) through `git tag -s`, in its order, with the new version: the settings-defaults stamp,
   `tauri.conf.json`, `Cargo.toml` plus `cargo update --workspace`, the CHANGELOG heading, the `LICENSE` change date,
   the visual baselines, both `oxfmt` passes, the strict `i18n-stale` check, the `chore(release): vX.Y.Z+1` commit, and
   the signed tag. Skip its pull, its pushes, and its CI dispatch (step 4 replaced them). The commit subject must match
   exactly, or the CI gate looks for a full CI run on the release commit itself and fails.
6. **Push the tag only** (`git push origin vX.Y.Z+1`), then follow § The release procedure from step 8 on: in `local`
   mode `release-finish.sh`, run from this worktree so it reads the tag's own `tauri.conf.json` pubkey, then the build
   monitoring, step 12's checks, and step 13's evidence. Step 11 doesn't apply: step 7 below merges the patch back.
7. **Merge the patch back into `main`** once it's published: from the patch worktree, `pnpm land -merge`, which has
   GitHub merge the tagged commit into `main` with a merge commit (§ Merging the release back into `main`). If GitHub
   can't merge it cleanly, merge by hand: in a fresh worktree off `main`, `git merge --no-ff vX.Y.Z+1`, then `pnpm land`
   (never `-squash`, which would flatten the merge; if `main` moved, `land` refuses to rebase a merge, so redo the merge
   on top of it). The merge is what puts the patch's `CHANGELOG.md` section on the website and in the next release's
   history, makes its hashes reachable for `changelog-links`, and moves `main`'s version up to the live one. Resolve
   conflicts in favor of the patch for the version files and the settings-defaults stamp, keep both sides in
   `CHANGELOG.md`, and keep `main`'s copy of a cherry-picked fix.
8. **Tear down the worktree**: `~/.claude/scripts/remove-worktree.sh security-vX.Y.Z+1`.

The next regular release drafts its CHANGELOG from `vX.Y.Z+1..main` and doesn't list the fix again.

## Writing the CHANGELOG

Mechanics, which the release script and the `changelog-links` check depend on:

- **Draft from a synced `main`** (`git pull --ff-only origin main`, step 2). Cite only landed commits: a hash on an
  unlanded branch changes if `land` rebases or squashes it, and `changelog-links` refuses any ref not on `main`.
- **Read every commit's title AND body.** An entry may cite several commits.
- **Add a `## [Unreleased]` heading** right after the format preamble (before the first versioned section), and put the
  entries under it. The release script replaces it with the versioned heading, so the committed changelog has no
  `[Unreleased]` section between releases: you create it fresh each time.
- **Cite commits as bare hashes in a trailing group**: `- Some change (b626d7a4b, 2d41cc147)`. ❌ Never a markdown link:
  the website and the What's new popup linkify (or strip) the hashes themselves, and `changelog-links` rejects a
  `…/commit/<sha>` URL.
- **Get hashes from `git log --format='%h' --abbrev=9`**, ❌ never by extending a 7-char `--oneline` prefix by guessing.
  `changelog-links` enforces exactly 9 characters and rejects fabricated hashes, aborting the release either way. git
  widens `%h` when a prefix would be ambiguous, so a 10-character one means the whole file needs re-deriving one
  character wider: don't trim it by hand, raise it with David.

**Audience.** One file, two readers:

- **Primary: Cmdr users.** The lead and the Added / Changed / Fixed / Security sections become the GitHub release notes
  and the in-app "What's new" popup, rendered with hashes stripped and Non-app dropped. Every entry must work
  standalone, in plain English, with zero internals.
- **Secondary: David and agents tracing changes**, served by the hashes and the Non-app section. Non-app is the only
  place internals (tooling, refactors, infra, website) belong, and only the major ones.

**Scope: no inflated fixes.** A changelog is the diff between the last release and this one, not a diary of the work. A
bug earns a Fixed line only if a USER COULD HAVE HIT IT: the defect existed in the previous release's tree. A bug
introduced and fixed inside this release window doesn't qualify. `git cat-file -e v<prev>:<path>` helps when in doubt.

**Style: plain-sentence, dense, impact-focused.**

- **Write a short lead** under the `## [Unreleased]` heading, before `### Added`: what this release means for users,
  naming the highlights. It opens the release notes and the What's new popup, and it's the part most people read. Any
  CommonMark works (paragraphs, a numbered list of highlights, bullets nested under a highlight with a three-space
  indent); see recent releases for examples.
- **File each entry where a user would look for it.** A fix to previously shipped behavior is Fixed even when this
  release also adds a feature in that area; perf and behavior tweaks are Changed; pure internals go to Non-app. Only
  Added / Changed / Fixed / Security / Non-app; never invent sections like "Improved".
- **Each entry is one sentence**, and the headline IS the entry (no `**Bold title:** Body.`). Most land under 20 words,
  many under 10. Only a tentpole feature bundling many commits (think Linux alpha) runs several lines. Wrote a period
  and kept going? Merge with a colon or cut the rest.
- **A Security entry that fixes a vulnerability names its severity and the affected versions** after a colon, before the
  hashes: `Fix a path escape in zip extraction: high (CVSS 8.2), 0.31.0–0.53.0 (b626d7a4b)`. The release it sits in is
  the remediation, so it needs no "update" clause. Hardening with no flaw behind it (a new check, a backup key) stays a
  plain entry. The contract is `SECURITY.md` § How fixed vulnerabilities are published.
- **No trailing benefit clauses**: delete ", so you always know…" and every other ", so [reader benefit]" ending. A
  well-written entry implies the benefit.
- **No em-dashes**: use parens, commas, colons, or rephrase (en-dashes in ranges are fine). Vary the connector rather
  than defaulting to `;`.
- **Lead with a verb often**: `Add X`, `Fix X`, `Make Y`, `Drop Z`.
- **Cut aggressively**: strip internal type names, file paths, code fragments, and "why we picked X"; git history has
  those. Keep impactful, interesting value details.
- **Omit low-impact entries.** Tooling-only commits like "release script now stages oxfmt fixes" don't earn a line.
- **Calibrate on the two most recent release sections plus the bottom 160 lines** of the file; both are curated. Don't
  pattern-match on anything else, and never on your own draft: verbosity compounds release over release that way.

Before / after examples:

- Don't: "**Dynamic text size.** New `Settings > Appearance > Text size` slider (75–150 %, default 100 %) that compounds
  with the macOS Accessibility text-size setting. New `View > Zoom` submenu with `⌘+` / `⌘-` / `⌘0`…" Do: "Add dynamic
  text size slider in Settings (75–150%, ⌘+/⌘-/⌘0 shortcuts)".
- Don't: "Brief network blips no longer kick you out of the folder; only a real not-found triggers eviction". Do: "Fix
  temp network issues kicking users out of folders".
- Don't: "Friendly errors for the git browser: damaged repos, orphaned worktrees, shallow-boundary commits, locked
  indexes get plain-language explanations and a next step". Do: "Add friendly errors for git browser".
- Don't (multi-sentence narration): "Folders always merge on copy and move. A folder landing on a same-named folder now
  blends into it instead of asking you to overwrite, skip, or rename the whole folder. Your conflict choice…" Do:
  "Folders always merge on copy and move: your conflict choice (skip, overwrite, or rename) applies to the clashing
  files inside, and dest-only files survive".
- Don't (trailing benefit): "Add per-feature stability badges (ALPHA, BETA) in the app and a Feature status page on the
  website, so you always know how solid each feature is". Do: "Add stability badges (ALPHA, BETA) in the app and a
  feature status page on the website".
- Long is right for a true tentpole: "Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink
  support, trash via FreeDesktop spec, inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, … (13
  SHAs)".

**Self-edit pass (mandatory)**, after drafting and before presenting:

- **Hold every entry to the UI-copy bar** of `docs/style-guide.md`: the What's new popup renders these exact sections.
- **Check every Fixed and Security entry against the scope rule above**: name the release the bug shipped in. If that's
  this release, fold the hash into the feature's Added entry and delete the line. This is the easiest pass to skip and
  the one that inflates a release most.
- Cut any entry over ~20 words unless it's a genuine tentpole, and merge or delete any second sentence.
- Delete any trailing ", so [benefit]" clause.
- Merge entries that share hashes or describe one change from two angles (a real past draft had a Fixed entry whose
  hashes were a strict subset of an Added entry's).
- Strip internal symbol names, file paths, and enum variants that survived the first pass.

## Security advisories

When a release fixes a vulnerability, the app says so: the restart toast leads with "This update fixes a security
issue." and links the advisory, and the What's new popup names it after the restart. Both read
`apps/desktop/src-tauri/resources/security-advisories.json`, which ships inside the signed bundle. The list, its
readers, and why it's never `latest.json`: `apps/desktop/src-tauri/src/security_advisories/DETAILS.md`.

- **What gets an entry**: every fixed vulnerability that has a severity score, the same set that gets a public note
  under `SECURITY.md` § How fixed vulnerabilities are published. Hardening without a vulnerability gets none, even when
  it's in the `### Security` section.
- **The entry**: `id` (the GHSA id, or a date slug like `2026-10-16-smb-path` when there's no GitHub advisory),
  `fixedIn` (this release's version, no `v`), `severity` (`low` / `medium` / `high` / `critical`, the CVSS v4.0 rating
  in the CHANGELOG line), and `url` (the GitHub security advisory if there is one, else this release's entry on
  `https://getcmdr.com/changelog`). Nothing else: the app shows generic, translated text.
- **Append, never prune.** The list is cumulative, which is how an install that skips releases still hears about every
  fix it jumped over.
- **Leave it uncommitted for `release.sh`**, which lets it through its dirty-tree guard and puts it in the release
  commit. `desktop-security-advisories` refuses a `fixedIn` newer than the app version, so committing it before the bump
  turns CI red (and step 5's `pnpm check -m` reports it, expectedly).
- **The GitHub advisory goes public after the release ships**, so a link to it 404s until then. Publish it in the same
  sitting, or, when its details are held back (`SECURITY.md`), link the changelog entry instead.

## Which runner builds the release

**GitHub-hosted (`macos-26`) builds every release**, set in `release-pipeline.yml` (`compile.runs-on` and
`sign.runs-on`, which must match). This repo is public, so hosted macOS minutes are free. No self-hosted runner is
registered, and nothing needs one.

- **Why the image and not `macos-latest`**: an image carries exactly one Xcode major, so that line picks the macOS SDK
  the bundle links against, and Apple gates behavior changes on it. A build step asserts the SDK major and fails before
  the compile, so a GitHub image bump can't quietly change the shipped app. What an SDK move would cost today, and what
  to check before making one: `apps/desktop/src-tauri/src/menu/DETAILS.md` § SF Symbol icons.

- **Why hosted, and why going back is unappealing**: `bundle_dmg.sh` drives Finder over AppleScript, which on a
  self-hosted Mac needs a TCC Automation grant for the runner's bundled `node`. That path changes on every runner
  auto-update, so the grant lapses silently, and once a prompt times out unattended the entry sticks at denied with no
  supported way to clear it short of a machine-wide `tccutil reset AppleEvents`. That failure took all three matrix jobs
  on the 0.37.0 release; the troubleshooting section below has the full anatomy. Hosted images have no such gate. A
  self-hosted Mac also has to stay awake for the whole build, or every in-flight job dies with
  `The self-hosted runner lost communication with the server`.
- **What hosted costs**: every job is ephemeral, so each pays a cold ~1000-crate Tauri compile instead of reusing a warm
  cargo cache. Partly offset by the two arch compiles running in parallel rather than queueing on one machine, so
  wall-clock is roughly one cold build.
- **What hosted needs that self-hosted didn't**: mise's action cache left on (there's no local tool dir to persist).
- **Hosted is also what makes the build split hold**: every job gets a fresh machine, so nothing `compile` ran can be
  waiting when `sign` unlocks the keys (§ How a release build is split). A self-hosted Mac runs both on one machine,
  where that's no longer true.

**If a self-hosted runner ever earns its keep again**, it's a from-scratch setup: register one with a token from
`gh api -X POST repos/vdavid/cmdr/actions/runners/registration-token`, run `./config.sh` and `./svc.sh install`, then
set `runs-on: [self-hosted, macOS, ARM64]` on both jobs. Fix the Finder Automation grant before the first run, or the
DMG step hangs ~2 minutes and fails every job. Mind the runner version too: the pinned actions declare
`runs.using: node24` (`actions/checkout` and `actions/download-artifact` at their pinned commits, read 2026-10-15),
which needs `actions-runner` 2.327.0 or newer (added in that release, 2025-07-22); an older runner rejects the step
outright. The workflow still carries its self-hosted-specific guards (the stale-`/Volumes/Cmdr` detach, the keychain
search-list restore), so nothing else has to change, but the build split then protects much less (above).

Re-enabling the persistent cargo target dir belongs with that switch, not before it: the old `CARGO_TARGET_DIR`
(`~/.cache/cmdr-release-target`, outside the workspace `actions/checkout` wipes) was safe ONLY because the jobs ran
sequentially on one machine. Under hosted runners they run concurrently, where a shared target dir would have cargo
locking and corrupting it. The directory is gone (reclaimed 21 GB); switching back recreates it, at the cost of one cold
compile.

## The CI gate

Nothing ships unless the release commit itself, R (the tagged `chore(release): vX.Y.Z` commit), has a green full CI run
and green CodeQL. One implementation decides it, `scripts/release-finish/ci.go`, and three places run it:

- **`release.sh`** starts the runs: right after it pushes the tag, it dispatches `ci.yml` with `run_all` and
  `codeql.yml` on `release/vX.Y.Z`, R's branch, so they run beside the build. In `ci` mode it waits for green before
  pushing the tag (`release-ci-gate.sh`), since that tag push publishes on its own.
- **`release-finish.sh`** waits for them beside the build and won't sign until they're green. Its dry run checks once
  and refuses while the run is red or still going.
- **`release-pipeline.yml`'s `ci-gate` job** checks again, without waiting, in whichever run publishes: before `publish`
  in the `local` finishing run, before `compile` in a `ci` mode tag push. So a tag pushed by hand can't skip it. A
  `local` tag push skips the job: it builds only a private draft.

`./scripts/release-ci-gate.sh X.Y.Z` follows the run on its own; `./scripts/release-finish.sh -ci-gate X.Y.Z` checks it
once.

- **Only a full run counts**: a `workflow_dispatch` run with `run_all`. ❗ A push or PR run doesn't: CI's change
  detection skips unchanged lanes, so a docs-only commit reads green while the commit before it broke Rust. The runs API
  carries no inputs, so `ci.yml` has a `Full run (run_all)` job that runs only with `run_all` on (`if: inputs.run_all`,
  no needs). The gate counts a run only when that job succeeded or hasn't finished, matching it by name
  (`fullRunMarker`): renaming the job means updating that.
- **Which commit**: R's own full run whenever R has one, so a red or running run on R holds the release however green
  its parent is. R's parent's only when R is the `chore(release)` commit and has no full run at all: the hand-made
  security-only patch release (§ Security-only patch release, step 4).
- **CodeQL, on the same commit**: a green `codeql.yml` run on it, and no open high or critical CodeQL alert on `main`
  (`scripts/release-codeql-gate.sh`, `docs/tooling/ci.md` § CodeQL).
- **It fails fast**: a wait prints each job as it moves and stops at the first red job, naming it, its log
  (`gh api --allow-escape-sequences repos/vdavid/cmdr/actions/jobs/<job id>/logs`, which works mid-run;
  `gh run view --log-failed` waits for the whole run), and the two ways on:
  - **A flake**: `gh run rerun <run id> --failed`, then re-run the command that stopped (`release-finish.sh` or
    `release-ci-gate.sh`). Its wait picks up the new attempt.
  - **A real failure**: `./scripts/release.sh -abort X.Y.Z`. It stops the release's runs and deletes the build
    artifacts, the draft, the tag, and the release branch (nothing was public: `latest.json` and the Homebrew tap move
    only at publish), and puts the changelog and the other prep edits back as uncommitted edits. Fix it in a worktree,
    land the fix, sync `main`, and run `./scripts/release.sh X.Y.Z` again.
- **Emergency bypass** for a hotfix while CI is red for reasons outside the repo: `RELEASE_SKIP_CI_GATE=1` in the
  environment of `release-finish.sh` (in `ci` mode, `release.sh`), AND the repository variable
  `RELEASE_SKIP_CI_GATE_TAG` set to the exact tag (like `v0.51.1`) before the run that publishes starts. Same shape as
  `RELEASE_REPUBLISH_TAG`: it unblocks only the tag written in it. It skips the CodeQL half too. Clear it once that run
  finishes.
- **Verified** with pipeline test releases on 2026-10-22 (`docs/notes/2026-10-22-faster-release-flow.md`): the build ran
  while CI ran, a dry run refused while CI was running and on a red run and went through signing once both were green,
  the `ci-gate` job passed in a finishing dispatch, and the wait stopped two minutes in on a deliberately misformatted
  file, with the Rust lanes still running.

## Merging the release back into `main`

R reaches `main` only once the release is published, so an aborted release never touches `main`. Re-running
`./scripts/release.sh X.Y.Z` on a published release lands R with `pnpm land -merge` from the main clone
(`scripts/land/CLAUDE.md`): a PR whose head is R, its CI run, then a fast-forward when `main` hasn't moved since R's
parent, else a merge commit GitHub makes onto whatever `main` is by then. ❌ Never a rebase or a squash: R has to stay
on `main`'s history as the exact commit CI tested and the tag names. A plain `pnpm land` refuses to rebase a tagged
commit.

- **Needs merge commits allowed on `main`**: the repository's "Allow merge commits" setting, and `merge` in
  `.github/rulesets/main-pr-required.json`'s `allowed_merge_methods` (`pnpm land rulesets -apply`). Without them GitHub
  refuses the merge and `land` says so; the fast-forward works either way.
- **A conflict GitHub can't merge** (rare: R changes only the version files, `Cargo.lock`, `LICENSE`, and the
  release-prep files): `land` stops. Merge by hand as in § Security-only patch release, step 7.
- **Its PR runs CI once more** (~30 min), on R merged with the current `main`. Nothing users get waits for it.

## Release prep checks that abort before tagging

`scripts/release.sh` runs a few hard checks locally before it commits and tags, so a release that can't ship is never
tagged. Beyond the version/CHANGELOG checks and `oxfmt --ci`, three are worth knowing:

- **Changelog refs must be on `main`.** After its fast-forward pull, the script runs
  `pnpm check changelog-links --fresh`, which fails the release on any ref not reachable from HEAD (a commit cited from
  a branch that was rebased or squashed while landing). Fix: re-derive those refs from `main`, then re-run. Why
  `--fresh`: `scripts/check/checks/DETAILS.md` § "CHANGELOG commit refs".
- **Visual baselines are auto-refreshed (Docker required).** After the CHANGELOG/roadmap/version are finalized, the
  script runs `apps/website/scripts/update-visual-baselines.sh`, which re-shoots any stale website baseline in a pinned
  Playwright container and folds the result into the release commit. Docker must be running; a stopped Docker aborts the
  release before tagging. The baselines no longer include `/features`, so release-prep copy shouldn't move any of them;
  the call stays as a cheap guard. Mechanism: `apps/website/DETAILS.md` § Visual baselines.
- **No stale translations may ship.** A non-`en` translation whose stored `@key.sourceHash` no longer matches the
  current English value is STALE (it renders text translated from a sentence that no longer exists). The
  `desktop-i18n-stale` check is warn-only in normal `pnpm check` (a maintenance signal, not a daily-dev build breaker),
  but the release script escalates it to a build-failing ERROR by running
  `CMDR_I18N_STALE_STRICT=1 pnpm check i18n-stale`. With `set -e`, a stale finding aborts the release before tagging, so
  the re-translation lands first. Fix: re-translate the changed keys and refresh `@key.sourceHash` (and re-review), then
  re-run the release. English-only today, so this is a clean no-op until a real locale exists. Mechanism and schema:
  `apps/desktop/src/lib/intl/messages/DETAILS.md` § `@key` metadata schema.

## The settings-defaults stamp

Right after bumping `package.json`, `scripts/release.sh` runs `gen-analytics-defaults.ts --promote <version>`. That
records the release's settings defaults in the analytics manifest (an entry only when a default moved) and stamps
`promotedThrough` either way. `settings-defaults` fails while the stamp is behind `package.json`. If a release ever
skips the step, run the promote on a tree whose settings match that release's tag: a later working tree would record
defaults the release never shipped. Mechanism: `apps/desktop/scripts/DETAILS.md` § "The defaults manifest".

## Pre-release smoke test on old macOS

Cmdr opens on macOS 10.15 Catalina and up, which means two different old-WebKit paths, and both want a look before a
tag. The floor rationale and version evidence: `docs/notes/system-requirements-and-es2025.md`.

### Degraded but working: the `color-mix()` fallbacks

Safari below 16.2 doesn't support `color-mix()`, and below 16.4 not `color-mix(in oklch, …)`. We carry static sRGB
fallbacks in `app.css` (`@supports not (color: color-mix(...))` blocks) and via JS in `accent-color.ts` /
`volume-tint.svelte.ts`. They have to stay in sync as new tokens land.

1. On any Mac, run `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` from the repo root. This fakes `hasColorMix = false` (routing
   the JS branches through sRGB mix) and sets `data-force-old-webkit` on `<html>` (activating the mirror of the
   `@supports not (...)` blocks). It doesn't replicate Safari 15.x's renderer, but it proves the fallback values look
   reasonable.
2. Optionally, boot a Monterey 12.7+ VM or a real old Mac and open the dev build. Note that ARM Monterey VMs ship with
   current Safari (17.x), so the bug isn't reproducible there without an early-12.x IPSW.
3. Either way, confirm the four user-visible spots aren't broken:
   - The "Open System Settings" button hovers to a lighter gold (not black).
   - The per-pane disk usage bar fills with green/orange/red instead of just the gray track.
   - The file-list cursor row has a visible gold-tinted background.
   - In dark mode, file-list size column shows the rainbow tier colors (not uniform gray).
4. Grep the app log for `Old WebKit detected:` — `logWebkitCompat()` emits this on startup when `color-mix()` isn't
   supported (or when the dev override is on). If you see it on Monterey 12.7+, the fallback path is doing its job.

If a new `color-mix()` token lands without a matching entry in the `@supports not` blocks, those four spots silently
break on old WebKit. Keep the lists in `app.css` in sync, and prefer the JS-derivation pattern (`accent-color.ts`,
`volume-tint.svelte.ts`) for any token that depends on the live macOS accent color.

### Below the floor: the boot guard

Under Safari 15.4 the app can't run at all, and the inline guard in `apps/desktop/src/app.html` replaces the window with
a translated "Cmdr needs a newer Safari" screen instead of leaving a white one. Nobody on the team has that Safari, so
the dev override is the only routine way to see it.

1. Run `VITE_CMDR_FORCE_OLD_WEBKIT=unsupported pnpm dev` from the repo root. The flag is read at BUILD time by
   `vite.config.js`, so it has to be set before the dev server starts; restarting an already-running one won't pick it
   up.
2. Confirm the screen paints: title, one paragraph, one Quit button, centered, correct in both light and dark mode.
3. Click Quit. The app should exit. (It goes through `plugin:process|exit`, so the quit gate sees it like ⌘Q.)
4. Check a second language: set macOS to one of the shipped locales, or run the same command after temporarily pointing
   `navigator.language`'s answer at another tag in the dev tools. Copy comes from `main.oldWebkit.*` in the catalog, so
   any locale you can pick in Settings is one the guard can show.

Nothing here needs a real old Mac: `apps/desktop/scripts/app-boot-guard.test.ts` runs the actual guard with each Safari
15.4 capability removed in turn, and fails on ES6 syntax that old WebKit couldn't parse. The smoke test is about how the
screen LOOKS, which no test can judge.

## Keep the Mac awake during the build (self-hosted only)

**Not needed today**: `runs-on` is a GitHub-hosted image, so the build happens on GitHub's hardware, this Mac can sleep,
and the laptop can close with no effect on the release. Everything below applies only if a self-hosted runner is ever
set up again.

How a self-hosted build looks from the outside: exactly one `Compile (...)` job goes `in_progress` and the other stays
`queued` (one machine, serialized), at ~5 min each on a warm cache, so the user keeps the laptop open for the whole
build. If both are still `queued` after ~30 s, the runner is down: confirm with `launchctl list | grep cmdr` (look for
`actions.runner.vdavid-cmdr.*`) and restart with `cd ~/actions-runner && ./svc.sh start`, falling back to
`launchctl bootout` + `bootstrap` if `svc.sh` errors with "Load failed: 5: Input/output error". Queued jobs pick up
automatically once the runner reports in; no re-trigger or re-tag needed.

A self-hosted runner would live on this Mac. If the machine sleeps (even briefly, or just the display), GitHub Actions
drops the runner connection and every in-flight matrix job fails with
`The self-hosted runner lost communication with the server.` This bit us on the 0.13.0 release: all three jobs failed at
exactly 11m1s each.

Before pushing the tag, make sure `caffeinate` is holding the Mac awake. The release script does NOT do this
automatically; the agent running the release is responsible for it.

**Check first, then arm only if needed.** A `caffeinate -dimsu` may already be running (a previous release, or the user
started one). Don't stack a second one, and don't kill one you didn't start.

```bash
if pgrep -lf 'caffeinate -dimsu' >/dev/null; then
    echo "caffeinate already running, leaving it"
else
    caffeinate -dimsu &          # -d display, -i idle, -m disk, -s on AC, -u user active
    CAFFEINATE_PID=$!
    # ... push the tag, monitor the build ...
    kill $CAFFEINATE_PID         # once all matrix jobs are done (success or fail)
fi
```

Agents: check with `pgrep -lf 'caffeinate -dimsu'` right after the push. If one's already running, skip arming and skip
the disarm at the end. Otherwise arm it as a Bash `run_in_background` call and `kill` it once the release monitor
reports the run has finished (wait for the overall run to be `completed`, not just the build matrix) - but only if you
armed it yourself. If the release fails and the user wants to re-run failed jobs with no caffeinate running, re-arm it
first.

## Refreshing the app-directory listings (optional, minor and major releases)

Cmdr is listed on app directories (MacUpdate, AlternativeTo), each with a file in `brand/listings` holding every field
of that site's form, filled and ready to paste. Those files are the source of truth: edit them first, then paste from
them. Never retype a listing from the CHANGELOG at the form.

This is optional and deliberately not part of the release script:

- **Skip it for patch releases.** A patch's changelog isn't interesting enough to spend a review cycle on, and the
  listing pages age gracefully.
- **The download URL never goes stale**, so an outdated listing still hands visitors the current DMG. Only the version
  string and the changelog text on the page age. (`getcmdr.com/download/latest/<arch>` resolves at request time; see
  `apps/api-server/src/telemetry/DETAILS.md` § Download tracking.)
- **No directory offers an API**, so submitting is a human pasting into a web form. An agent prepares the text and stops
  there: submitting is an external action.

For a minor or major release, the agent updates `brand/listings/macupdate.md` in place:

- The version number. The Download URL is the dynamic `latest` link, so it needs no bump.
- The screenshots: run the five `magick` commands in its § 3. Media (keep them matching the screenshot list there), and
  give David the folder, `brand/listings/macupdate-media/`. The PNGs are gitignored; never commit them.
- The "Version changes" HTML, rewritten from the new CHANGELOG section into their `<h5>` + `<ul>` format (New / Improved
  / Fixed). Cover the whole minor line, patches included, since the listing skipped those.
- The description, but only where the release actually changed it: a feature that graduated out of alpha, a claim that
  no longer holds, a "coming soon" that shipped. Leave the wording alone otherwise; David reviews every human-facing
  string, and needless churn costs him a review.

Then hand David the submission link: https://member.macupdate.com/content/submit, where "Modify an existing listing?" at
the top takes the app name and loads the current listing. MacUpdate prefers updating an existing listing over a new one
(downloads keep accumulating, version history stays catalogued, and Watch List users get notified). Remind him to ping
the reviewer right after submitting: without a ping, a submission never gets reviewed (contact in the vault's directory
blitz plan).

`brand/listings/alternativeto.md` gets the same pass, minus the version (AlternativeTo shows none): fix any line the
release made false (language count, alpha list, a claim that broke), measure the description against its 3,000-character
cap after every edit, and record what changed and why in the file's status notes. Hand David
https://alternativeto.net/software/cmdr/about/ with the "Note about your changes" text.

## What a release publishes

One GitHub release per tag, carrying these assets for each of the three arches (`aarch64`, `x64`, `universal`):

- `Cmdr_<version>_<arch>.dmg`: what people download. `getcmdr.com/download/latest/<arch>`, the website's download
  buttons, and the Homebrew cask each rebuild this name from the version, so it's the one asset name the rest of the
  repo hard-codes.
- `Cmdr_<version>_<arch>.app.tar.gz` plus its `.sig`: the updater payload and its minisign signature, made in CI or on
  the laptop (§ Who signs the update archives).
- `latest.json`, the update manifest. `getcmdr.com/latest.json` serves the one on the release GitHub marks "Latest":
  each website deploy fetches it into the Docker build (`apps/website/scripts/fetch-live-manifest.ts`), and `publish`
  sets that mark only after uploading the manifest, so the mark is the one pointer to the live release and nothing
  commits to `main`. The copy committed at `apps/website/public/latest.json` is a frozen one that dev and the check
  lanes build with; ❌ never read the current version from it.
- `checksums.txt`: one `shasum -a 256` line per DMG (plus the pkg when there is one), the only way a person who
  downloaded from the website (or from an aggregator listing) can check what they got. The Homebrew cask carries its own
  `sha256` and the updater verifies a minisign signature, so this covers the one install route that had nothing. The
  publish job builds it by downloading the DMGs back FROM the release, ❌ never from the runner's `target/`, so it
  describes what users actually receive, and it fails the job unless all three lines are there. First published with
  v0.46.0 (uploaded by hand after the fact; every release from v0.47.0 on gets it from the workflow). The website links
  it as `SHA-256 checksums` in the download card through `getcmdr.com/download/latest/checksums`, an api-server redirect
  beside the per-arch DMG ones that resolves `latest` the same way; ❌ it writes no `downloads` row, since a checksum
  fetch is not an app download and would inflate the per-version counts.
- `Cmdr_<version>_universal.pkg`: the signed, notarized installer package for MDM deployment, once the installer
  certificate is set up (§ The installer package). Universal only: an MDM pushes one package to every Mac.
- Three CycloneDX SBOMs, uploaded by the `attest` job: `Cmdr_<version>_aarch64.rust.cdx.json` and
  `Cmdr_<version>_x64.rust.cdx.json` (the Rust crate graph per target triple) and `Cmdr_<version>_frontend.cdx.json`
  (the npm packages whose code is in the built frontend). Details in § Provenance and SBOM attestations.

Every asset also carries a signed SLSA build provenance attestation, stored on the repo, and the same provenance is on
the release as `Cmdr_<version>.intoto.jsonl` (same section).

Two naming details are load-bearing:

- **File names say `x64`, never `x86_64`.** Tauri's CLI names the Intel bundles that way, while the rest of the repo
  (URL paths, D1 columns, Rust target triples) says `x86_64`. The mapping happens at the file-name boundary only; see
  `apps/api-server/src/telemetry/DETAILS.md` § Gotchas.
- **Every bundle name carries the app version, `.app.tar.gz[.sig]` included.** The bundler writes `Cmdr.app.tar.gz`, and
  the `sign` job's upload step renames it. Releases up to 0.41.0 carry unversioned `Cmdr_<arch>.app.tar.gz` instead.

**The publish job owns `latest.json`**, with `https://github.com/<repo>/releases/download/<tag>/<file>` URLs. ❌ Never
API asset URLs (`api.github.com/repos/.../releases/assets/<id>`): those hand back the binary only to a caller sending
`Accept: application/octet-stream`, and Cmdr's updater does a plain `reqwest` GET (`download_update` in
`apps/desktop/src-tauri/src/updater/mod.rs`), so it would read JSON metadata and fail signature verification. Before
uploading the manifest, the job asserts that every URL in it names an asset actually on the release: a name that drifts
from what `sign` uploaded would strand every install in the field, with no fallback in the app to recover.

## Provenance and SBOM attestations

Two jobs in `release-pipeline.yml`, beside the build-and-publish chain:

- **`sbom`** (the tag push only: right away in `local` mode, after `ci-gate` in `ci` mode; read-only token, no OIDC)
  runs beside the macOS builds. It generates the three SBOMs: `cargo cyclonedx` (version pinned in the workflow,
  CycloneDX 1.5) on `apps/desktop/src-tauri/Cargo.toml` with default features, once per target triple, and a frontend
  build with `CMDR_FRONTEND_SBOM=1` for the npm side, which lists the packages the bundler actually put in the app
  (`apps/desktop/scripts/vite-frontend-sbom.ts`; `package.json` can't say, since Svelte ships from devDependencies). It
  fails if a lockfile moved while resolving, or if an SBOM isn't CycloneDX, doesn't name this version as its root, or
  lists too few components (100 for Rust, 30 for the frontend). It hands them on as the `sboms` workflow artifact.
- **`attest`** (`needs: [publish, sbom]`, holds `id-token: write` and `attestations: write`) downloads every asset back
  from the published release, uploads the SBOMs to it, and runs `actions/attest`: one SLSA provenance attestation
  covering every asset (SBOMs included), then one SBOM attestation per SBOM, bound to the builds it describes (each Rust
  SBOM to its arch's DMG and tarball plus the universal ones and the pkg; the frontend SBOM to all of them). It also
  uploads the provenance's Sigstore bundle as `Cmdr_<version>.intoto.jsonl`: one file whose subjects are all the assets,
  for offline verification and for OpenSSF Scorecard's Signed-Releases check, which scores 10/10 only with an
  `*.intoto.jsonl` asset. A later run of the job replaces it (in `local` mode the dispatch's bundle, which adds
  `latest.json` and `checksums.txt`, wins). The repo-stored attestation stays what `gh attestation verify` reads.

Why it's shaped this way:

- **Attest the bytes as published.** Same rule as `checksums.txt`: the digests come from the release, ❌ never from
  `target/`, so they're what users get after signing, notarization, stapling, and the upload.
- **In `ci` mode nothing waits on `attest`.** It runs after `publish` has already shipped `latest.json`, so a Sigstore
  or attestation-API outage leaves the release exactly as it was before these jobs existed. `attest` also runs when
  `sbom` failed (its SBOM steps skip), so provenance never waits on SBOM tooling. In `local` mode the laptop waits on
  the tag push's `attest`, since it signs only what that provenance covers (§ Attestations in `local` mode).
- **The job that can sign runs no third-party code.** `cargo install` and `pnpm install` live in `sbom`, which has no
  OIDC grant; `attest` only downloads and calls `actions/attest`.
- **Every job lives in a reusable workflow.** `release.yml` is only the `v*` tag trigger: one job that calls
  `release-pipeline.yml` with the union of the jobs' permissions and `secrets: inherit`. So the attestation's signing
  certificate names `release-pipeline.yml` as the signer workflow, which is GitHub's recipe for SLSA Build Level 3
  (verified against GitHub's "Using artifact attestations and reusable workflows to achieve SLSA v1 Build Level 3",
  2026-10-05). ❌ Don't move jobs back into `release.yml`. Inside the call, `github.*` and `vars.*` are the caller's
  (the tag, this repo's variables), and job names show as `Release / <job>`.
- **The public claim is Level 3** on `/trust` and `/trust/development` (verified on v0.51.0, the first release through
  `release-pipeline.yml`, with the `--signer-workflow` check below, 2026-10-08).

Checking a release (step 12 of § The release procedure runs both through `release-evidence.sh verify`, also pinned to
the tag and its commit):

```bash
gh attestation verify Cmdr_X.Y.Z_aarch64.dmg --repo vdavid/cmdr --format json \
  --signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml
gh attestation verify Cmdr_X.Y.Z_aarch64.dmg --repo vdavid/cmdr --format json \
  --predicate-type https://cyclonedx.org/bom
```

The first checks provenance and that the reusable workflow signed it, the second the SBOM binding. Earlier releases were
signed by `release.yml` itself, so check those with `--signer-workflow` pointing at `release.yml`. The public
instruction on `/trust` is the shorter `gh attestation verify <file> --repo vdavid/cmdr`.

**Gotcha**: without `--format json`, `gh attestation verify` prints nothing when its output isn't a terminal (an agent's
shell, a pipe), only exit 0. Read the exit code, or keep the flag to see what it verified (`gh` 2.x, 2026-10-08).

## The installer package

MDMs deploy software as flat, signed, notarized `.pkg` installers, so each release can carry
`Cmdr_<version>_universal.pkg` beside the DMGs. The `pkg` job in `release-pipeline.yml` builds it; the packaging itself
lives in `scripts/build-pkg.sh`, which also runs locally. Admin-facing side (deploying it, the Full Disk Access
profile): `/trust#mdm-deploy`.

**How it's built.** The job downloads the universal DMG from the release, as uploaded, takes `Cmdr.app` out of it, and
checks it's stapled and Gatekeeper-accepted. So the pkg wraps exactly the app people download. `build-pkg.sh` then:

- runs `pkgbuild` with a component plist: **not relocatable** (otherwise Installer "upgrades" whatever copy with the
  same bundle id it finds, in Downloads or a dev build, instead of installing to `/Applications`), **not
  version-checked** (an admin can roll back by pushing an older pkg), upgrade in place, strict bundle id;
- installs to `/Applications`, owned `root:wheel` (`--ownership recommended`), with **no scripts**;
- wraps it with `productbuild` in a distribution that allows the system domain only, the app's own architectures, and
  macOS 10.15 and up; signs it with `Developer ID Installer: Rymdskottkarra AB (83H6YAQMNP)` (the application identity
  in `tauri.conf.json` with the kind swapped, so the team has one source);
- notarizes it (`notarytool`, the same API key as the DMGs), staples it, and checks it: `pkgutil --check-signature`,
  `spctl --assess --type install`, and the expanded package (install location, `relocatable="false"`, every Bom entry
  `0/0`, the binary in the payload).

Apple's guidance (Xcode docs, "Packaging Mac software for distribution", read 2026-10-07): sign with a Developer ID
Installer identity, notarize the outermost container, staple it. The app inside is already notarized from the DMG build,
which does no harm.

**Verified through a real MDM.** Fleet pushed `Cmdr_0.55.0_universal.pkg` over a `.pkg`-installed 0.52.0 with Apple's
`InstallEnterpriseApplication` command: installed in about six seconds with no prompt, the bundle still `root:wheel`,
`codesign --verify --deep --strict` and `spctl` passing, Full Disk Access intact after relaunch (verified on macOS
27.0.1, the M1 enrolled in David's Fleet test rig, 2026-10-22). Cmdr wasn't running during the install.

**Skips until the certificate exists.** Without `APPLE_INSTALLER_CERTIFICATE` in the `release` environment, the job logs
a notice and ends green, and the release ships without a pkg. `publish` and `attest` wait for the job (so
`checksums.txt` and the provenance cover the pkg) but don't need it to succeed: in `ci` mode a red `pkg` job never holds
up the release. In `local` mode it does, since `release-finish.sh` waits for a fully green tag-push run: re-run the
failed job, or, if the pkg can't be fixed now, delete `APPLE_INSTALLER_CERTIFICATE` from the environment and re-run it,
which makes it skip.

**Signed in CI in both modes.** `RELEASE_UPDATE_SIGNING=local` keeps the updater key off GitHub because whoever holds it
can ship an update to every installed Cmdr, for good. The installer certificate can't: a pkg reaches a Mac only when an
admin or a user runs it, and the certificate is revocable like the application one that already signs in CI. Moving it
to the laptop would add a second signing round trip to every release for no reduction in reach.

**The updater on a pkg install.** The app lands in `/Applications` owned by root, so the in-app updater can't write it,
and no admin password would help (App Management refuses an admin-run helper on macOS 13+). It skips the download and
tells the user once per version: an administrator to download and drag in the new version, anyone else to ask their IT
team (`apps/desktop/src-tauri/src/updater/DETAILS.md` § A bundle that can't be written). Decided 2026-10-07: keep
`root:wheel` and no postinstall script. That's the norm for `/Applications` and what a security review expects, and a
`chown` to the console user would let any process running as that user rewrite an app that may hold Full Disk Access.
Managed Macs get each new version as a pkg pushed from the MDM, and setting `DisableUpdates` or `MaxUpdateVersion`
(`/trust#mdm`) also silences the per-release notice; the trust page says so.

**Testing the packaging locally**: `./scripts/build-pkg.sh` packages `/Applications/Cmdr.app` unsigned into a temp dir
(or `--app <path> --out <dir>`). It never installs: `installer` needs root, and a test install would replace the real
`/Applications/Cmdr.app`. So it reads the package instead (above). Expect `write: Permission denied` lines from
`pkgbuild` and a warning about `._` entries when an agent runs it: the agent's process stamps `com.apple.provenance` on
every file it copies, which nothing can strip (verified on macOS 27.0, 2026-10-07, building from 0.50.0).

**Not verified yet**: a signed, notarized build (no certificate), and an install through a real MDM. Both are open on
`vdavid/cmdr#118`.

### Setting up the installer certificate

1. In Keychain Access, select the "Developer ID Installer: Rymdskottkarra AB (83H6YAQMNP)" certificate with its private
   key (My Certificates), then File › Export Items… as `developer-id-installer.p12` with a new password. Same steps as
   the application certificate: `docs/guides/apple-signing-and-notarization.md` § 1.5.
2. Add both to the `release` environment, with the values never on screen or in shell history:
   `base64 -i developer-id-installer.p12 | gh secret set APPLE_INSTALLER_CERTIFICATE --env release -R vdavid/cmdr`, then
   `gh secret set APPLE_INSTALLER_CERTIFICATE_PASSWORD --env release -R vdavid/cmdr` (it prompts).
3. Back up the `.p12` and its password where the other signing keys live (vault note
   `projects/Cmdr/workflow/Cmdr signing keys.md`), then delete the file.
4. Optional local proof before a release:
   `./scripts/build-pkg.sh --sign "Developer ID Installer: Rymdskottkarra AB (83H6YAQMNP)"` signs from the login
   keychain and runs `pkgutil --check-signature`.
5. The next release attaches the pkg. Then update `/trust` (the `DevTodo` under "Deploying Cmdr") and this section's
   "Not verified yet".

### Renewing the Developer ID Installer certificate

David's calendar reminds him two months before expiry, then every 58 months (a certificate lasts about five years);
after a renewal, he moves the next reminder to two months before the new expiry. Steps 1–3 and 5–6 are his (portal and
Bitwarden), step 4 is an agent's. The Developer ID Application certificate (expires 2031-03-03) has its own reminder and
the same flow: pick "Developer ID Application" (G2 Sub-CA) in step 2, replace `APPLE_CERTIFICATE` and
`APPLE_CERTIFICATE_PASSWORD` in step 4, and check that the app's designated requirement still matches `/trust` and the
Full Disk Access profile.

1. Keychain Access › Certificate Assistant › Request a Certificate From a Certificate Authority: his email, a common
   name, "Saved to disk".
2. At `developer.apple.com/account/resources/certificates/add`, choose **Developer ID Installer** and the **G2 Sub-CA**,
   upload the CSR, download the `.cer`, and double-click it to install.
3. ❗ The G1 trap: always pick G2 in the portal. A certificate made through Xcode on 2026-10-07 chained to the old
   "Developer ID Certification Authority" (G1) and expires 2027-02-01.
4. Export and set the secrets, keeping the password out of shell history (hand it to David once, for step 5):
   - `security export -k login.keychain-db -t identities -f pkcs12 -P "$PW" -o all.p12` exports every identity (with
     `PW=$(openssl rand -base64 24)` in the same shell).
   - Pull out the one identity: `openssl pkcs12 -in all.p12 -passin env:PW -nodes`, keep the "Developer ID Installer"
     certificate and the key whose `localKeyID` matches it, then `openssl pkcs12 -export -legacy` them into
     `developer-id-installer.p12` with `-passout env:PW` (without `-legacy`, macOS `security import` in CI may reject an
     OpenSSL 3 file). Check with `openssl pkcs12 -in developer-id-installer.p12 -passin env:PW -info -noout -legacy`.
   - Set both:
     `base64 -i developer-id-installer.p12 | gh secret set APPLE_INSTALLER_CERTIFICATE --env release -R vdavid/cmdr`,
     then `printf %s "$PW" | gh secret set APPLE_INSTALLER_CERTIFICATE_PASSWORD --env release -R vdavid/cmdr`. Delete
     `all.p12` and the PEM files.
5. David stores the `.p12` and its password in Bitwarden, then deletes the file.
6. Delete the previous certificate (and its private key) from the login keychain, and let it expire. The portal has no
   Revoke button for Developer ID certificates (verified 2026-10-08): revoking one invalidates everything it ever
   signed, so only Apple Developer Support does it. Ask them only if its private key may have leaked.

## How updates work

- App asks `https://api.getcmdr.com/update-check/<version>?arch=<arch>` on start and every 3 hours by default
  (`advanced.updateCheckInterval`)
- If newer version found → downloads silently → shows "Restart to update" toast
- Signatures verified with public key embedded in app

## The tag guard, and rolling back

Pushing a `v*` tag runs the whole Release workflow, and `publish` makes whatever tag fired it the live manifest. It has
no idea which version that is, so re-pushing an old tag points the entire install base at an old build. The `guard` job
first verifies the annotated tag's SSH signature against `.github/release-signers`, then requires the tag's version to
be strictly greater than the live manifest's (the `latest.json` of the release marked "Latest"). ❗ Never the checkout's
`apps/website/public/latest.json`, a frozen dev copy that would let an old tag through. The release script creates that
signed tag with the configured Git signing key. A missing key or declined passphrase aborts locally, and an unsigned or
differently signed tag aborts in CI. The guard runs in seconds and blocks `build`, so a bad push never reaches the three
90-minute macOS jobs and never overwrites assets on an old release.

The signing key is replaceable. To rotate it, commit the new public key in `.github/release-signers` before cutting the
first release signed by its private half. An older tag keeps the signer file from its own tagged commit, so replacing
the current public key does not invalidate published releases or prevent their workflow jobs from being re-run.

Two consequences worth knowing before they surprise you:

- **Re-pushing the current version is refused.** That's a rebuild of something users already have. Retrying a _failed_
  build is unaffected: publish never ran, so the manifest still names the previous version and the tag is still ahead of
  it.
- **A full re-run after publish already marked the release Latest is refused too**, because by then the manifest names
  this very version. Use the override below, or re-run only the failed jobs rather than all of them.

**To roll back deliberately** (a shipped release turns out to be bad and you want everyone back on the previous one):

1. Set the repository variable `RELEASE_REPUBLISH_TAG` to the exact tag you're republishing, like `v0.41.0`. It's under
   Settings → Secrets and variables → Actions → Variables. It has to match the tag exactly, so a leftover value can
   never act as a blanket "always allow".
2. Push or re-run that tag. The guard logs a warning saying the downgrade is deliberate, and the release proceeds;
   `publish` marks the old release Latest again and redeploys the website.
3. **Clear the variable.** Nothing expires it for you.

The variable lives outside git on purpose. A tag push is the mistake this guards against, so no combination of tag
pushes should be able to unlock it.

## Troubleshooting

### Release build failed, need to retry same version

A flake (a notarization timeout, a runner hiccup): re-run the failed jobs (`gh run rerun <id> --failed`), then re-run
`release-finish.sh`. A failure that needs a code change: abort the release and start over, which is safe before publish,
since nothing was public yet:

```bash
./scripts/release.sh -abort X.Y.Z   # its runs, artifacts, draft, tag, and branch go; the prep edits come back
# ... fix in a worktree, pnpm land, then in the main clone: git pull --ff-only origin main ...
./scripts/release.sh X.Y.Z          # a fresh release commit on the new main, with its own CI run
```

The `guard` job doesn't get in the way here: the failed run never published, so the live manifest still names the
previous version and this tag is still ahead of it. It only refuses once the manifest already advertises this same
version, at which point the retry would be a rebuild of something users have.

### Draft release left on GitHub after failed build

In `ci` mode, delete the draft before retrying (the `sign` job refuses to upload into a draft there):
`./scripts/release.sh -abort X.Y.Z` does it with the rest of the cleanup. In `local` mode the draft is the point: a
re-run of the same tag's jobs reuses it (the `draft` job finds it, and the `sign` jobs replace same-named assets). Only
delete one by hand when the `draft` job reports more than one release on the tag.

### `release-finish.sh` stopped

Fix the cause, then run it again; it picks up where the release stands.

- **"the release's full CI run is red"**: § The CI gate. Re-run a flake's failed jobs and run this again; abort a real
  failure with `./scripts/release.sh -abort X.Y.Z`.
- **"no full CI run … on <sha>"**: the CI dispatch never happened. `./scripts/release.sh X.Y.Z` resumes and starts it.
- **"refusing to sign … its build provenance didn't verify"**: the tag push's `attest` job failed or didn't run, or the
  archive on the draft isn't the one the pipeline built at the tag. Check the `attest` job first (re-run failed jobs if
  it was an outage). ❌ Never sign around it, and never pass `-signer-workflow` outside a dry run (the tool refuses):
  that check is the whole point of signing locally.
- **"the build run ended failure"**: re-run its failed jobs, `gh run rerun <id> --failed`, then run this again.
- **"has N releases on GitHub"**: duplicate drafts on one tag. Keep the one with the assets, delete the rest.
- **"the signature for … doesn't verify against the app's trusted keys"**: the key in sops isn't the app's main key (key
  ID `A2601F36BB168C0A`; the error names the key that signed). Compare with the Bitwarden copy (vault note
  `projects/Cmdr/workflow/Cmdr signing keys.md`). The finishing run's `publish` job runs the same check
  (`release-finish.sh -verify-dir`) before it publishes the draft, so a dispatch with missing, empty, or foreign
  signatures fails there and nothing goes public.
- **"the finishing run failed again"**: it already re-ran the failed jobs once. Read the run; § Publish job failed but
  builds succeeded applies, as does § The attest or sbom job failed. ❌ Don't dispatch a fresh run by hand once
  `publish` has marked the release Latest: the guard refuses it. Re-run the failed jobs instead.

### Apple notarization is slow (the `sign` job times out)

Apple's notarization can take anywhere from minutes to 20+ hours. If a `sign` job hits its 60-minute timeout waiting for
notarization, the publish job won't run, with no broken state.

To check notarization status manually:

```bash
KEY=$(mktemp -t AuthKey_C9VUN857DD)
trap 'rm -f "$KEY"' EXIT
secret APPLE_API_KEY_BASE64 | base64 -d > "$KEY"
xcrun notarytool info <SUBMISSION_ID> \
  --key "$KEY" \
  --key-id C9VUN857DD \
  --issuer "$(secret APPLE_API_ISSUER)"
```

The credentials live in the infra sops store, so nothing needs to sit unencrypted on disk. See
`docs/guides/apple-signing-and-notarization.md` for the full manual submit-and-staple path.

The submission ID is logged in the `sign` job's output before the timeout. Once the status shows `Accepted`, re-run the
failed job(s) in GitHub Actions; the `sign` job bundles the same compiled binary again and re-submits, Apple returns
`Accepted` immediately (same binary), and the job completes in minutes.

Use "Re-run failed jobs" (not "Re-run all jobs"): it re-runs only the failed `sign` jobs, and the `compile` jobs stay
done (their `compiled-<arch>` artifacts last seven days).

### Publish job failed but builds succeeded

The publish job downloads signatures from the release, generates and uploads `latest.json`, updates the release body,
marks the release Latest, and triggers a website deploy. If it fails:

- **Missing signatures**: check that all three `sign` jobs (in `local` mode, `release-finish.sh`) uploaded their `.sig`
  files. The publish job validates this upfront and fails fast with a clear message. It looks for
  `Cmdr_<version>_<arch>.app.tar.gz.sig`. (A Tauri CLI bump that renames a bundle fails the `sign` job's upload step
  before this, since it copies each file by its expected name.)
- **`latest.json` points at an asset that isn't on the release**: same cause, caught by the assertion the job runs
  before uploading the manifest. Compare `gh release view <tag> --json assets` against the names the workflow builds,
  and fix the workflow rather than the release.
- **Website deploy webhook failed**: re-run the publish job's failed steps, or SSH into the server and run the deploy
  script (`docs/guides/deploy-website.md` § Manual deploy). A deploy whose manifest fetch fails keeps the old site up;
  its log is in `/var/log/cmdr/deploy-website.log`.

### The attest or sbom job failed

The release already shipped, so nothing is urgent: users get the same build as before these jobs existed, just without
attestations (or without SBOMs). Read the failure, then use "Re-run failed jobs". That re-runs only the failed jobs and
the ones depending on them, so the guard, the builds, and `publish` stay as they were. ❌ Don't re-run all jobs: the
guard refuses, because the manifest already names this version.

- **`sbom` failed**: `attest` still ran and attested the other assets. Re-running `sbom` re-runs `attest` too, which
  then uploads the SBOMs and re-attests everything (a second provenance attestation for the same digests is harmless).
- **`attest` failed on the asset check**: an expected DMG or `.app.tar.gz` isn't on the release, which `publish` should
  have caught first. Compare `gh release view <tag> --json assets` against the names the job builds.
- **`attest` failed in `actions/attest`**: usually Sigstore or the attestation API. Retry later.
- **`attest` failed with "Unsupported SBOM format"**: the action accepts CycloneDX only with `bomFormat`, `specVersion`,
  AND `serialNumber`, and the last is optional in the spec, so neither generator writes it. The `sbom` job stamps a
  UUIDv5 serial and its validation requires one. ❗ A re-run can't repair a release that shipped without it: a re-run
  uses the workflow as it was at the tag, so the SBOMs stay published and unattested until the next release (v0.47.0
  shipped this way).

### `codesign` fails with `errSecInternalComponent` (and `gh` stops working after a release)

`errSecInternalComponent` from `codesign` means the signing key can't be resolved or accessed cleanly. Three ways this
happened on the self-hosted runner:

- **The llama-server signing leaned on the login keychain.** The bundler sets up its own signing keychain from
  `APPLE_CERTIFICATE`, but only inside `tauri bundle`; `download-llama-server.go sign` signs the bundled dylibs before
  that. The runner's launchd service runs with `SessionCreate=true` (GitHub's `svc.sh` plist), so its jobs live in their
  own security session where the login keychain's private key isn't usable (the exact same `codesign` command works from
  a GUI shell), and every matrix job failed ~30 s in. A runner-service restart doesn't help. The fix in
  `release-pipeline.yml` ("Set up llama-server signing keychain") imports the cert into a dedicated keychain that the Go
  script targets explicitly via `codesign --keychain` (`LLAMA_SIGN_KEYCHAIN`). The keychain must ALSO be in the user
  keychain search list: `--keychain` alone fails with the same `errSecInternalComponent` for a keychain outside the
  search list (verified empirically on this runner). The explicit `--keychain` is what keeps the login keychain's copy
  of the identity from making resolution ambiguous; the "Remove the signing material" cleanup step resets the list
  afterwards.

The other two are about the **same Developer ID identity being reachable from more than one keychain in the search
list** (ambiguous resolution):

- **A duplicate cert across keychains.** The Developer ID Application cert existed in both the login keychain (with its
  private key) and the System keychain (a stray keyless copy). Check with `security find-identity -v -p codesigning`: if
  the same identity (same SHA-1) appears twice, that's the cause. Remove the stray copy from the offending keychain, for
  example `sudo security delete-certificate -Z <SHA1> /Library/Keychains/System.keychain`. The login keychain copy (the
  one with the private key) is the one to keep. Verify local signing still works: `codesign -s <SHA1> --force /tmp/x`.
- **Double import in the workflow.** An earlier version of `release.yml` imported the cert manually _and_ let the
  bundler import it too, putting the cert in two keychains in the search list. The bundler owns the app's signing
  keychain (no manual `security import` for it), so only the llama-server keychain also holds the cert, and the Go
  script pins that one with `--keychain`. Don't add another cert-import step.

The companion symptom is **`gh` reporting an invalid token after a release**. `gh` stores its OAuth token in the login
keychain (secure storage, no `oauth_token` in `~/.config/gh/hosts.yml`). The old manual signing step ran
`security list-keychain -d user -s <temp>`, which _replaced_ the user search list and dropped the login keychain, so
`gh` (and any keychain-backed tool) couldn't find its token until the list was restored. The token is never actually
lost. Restore it with `security list-keychains -d user -s "$HOME/Library/Keychains/login.keychain-db"`. The `sign` job's
`Remove the signing material` step does this on every release (`if: always()`).

### `bundle_dmg.sh` hangs ~2 minutes then fails on every matrix job (self-hosted only)

This is the failure that moved releases to GitHub-hosted runners (see § "Which runner builds the release"). It can't
happen on a hosted image; read on only when running self-hosted.

The `actions-runner` auto-updated to a new version and its bundled `node` at
`~/actions-runner/externals.<version>/node20/bin/node` is a TCC client macOS has never seen. The first `osascript` call
in `bundle_dmg.sh` pops a "control Finder" prompt; if no one's at the keyboard, the prompt times out after ~2 minutes
and TCC records `auth_value=0` (denied) for that node path in `~/Library/Application Support/com.apple.TCC/TCC.db`.
Every subsequent DMG build hangs the same way until you flip the bit.

Read the state first, resolving the REAL path (`externals/` is a symlink into `externals.<version>/`, and tccd keys its
rows on the resolved path):

```bash
NODE=$(readlink -f ~/actions-runner/externals/node20/bin/node)
sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
  "SELECT auth_value FROM access WHERE client='$NODE' AND service='kTCCServiceAppleEvents' AND indirect_object_identifier='com.apple.finder';"
```

`auth_value` codes: 0=denied, 1=ask, 2=allowed. Empty or `1` just needs someone at the keyboard when the next build
runs. `2` is fine. `0` blocks every DMG build until it's cleared, and clearing it is the hard case:

- **The `externals/` symlink carries its OWN row**, left at `2` by earlier grants, so a check against the symlink path
  reports "allowed" while the resolved path sits at `0`. ❌ Never read the state through the symlink. This masked a real
  denial on the 0.37.0 release and cost all three matrix jobs.
- **You cannot fire the prompt from a Terminal or agent shell.** TCC attributes the request to the responsible process,
  which there is the already-granted shell, so `osascript` succeeds without ever asking about node and no row changes.
  Only the runner's launchd service (`SessionCreate=true`) puts node in that role, which is why the prompt appears
  during a real build and nowhere else.
- **System Settings → Privacy & Security → Automation may refuse to flip it.** A stuck `0` row's Finder checkbox can
  bounce straight back off (observed on the 0.37.0 release, macOS 26.5.2). Several older runner-node entries sit there
  allowed, which makes the broken one easy to miss.
- **`tccutil` can't target it**: it takes a bundle identifier, and this client is a bare binary path
  (`tccutil: No such bundle identifier`). The only supported clear is `tccutil reset AppleEvents` with no argument,
  which drops EVERY app's Automation grant on the machine, so every one of them re-prompts on next use. Get the user's
  explicit consent before running it; it's their whole system, not just the runner.
- ❌ Don't `UPDATE` the row to 2 by hand: tccd re-validates each row's `csreq` against the live binary's signature, plus
  there's an integrity layer on Sonoma+. It reads back fine via `SELECT` and still behaves as untrusted.

After a reset, start a build with the user at the keyboard: the prompt lands within a second or two of
`Running bundle_dmg.sh`, and one Allow authorizes that runner-node path until the runner auto-updates again.

Prevention: step 4 of § The release procedure reads the resolved path's `auth_value` right after the CHANGELOG draft, so
a denial is found before anything is tagged rather than after three jobs burn.

### `bundle_dmg.sh` fails fast (~3 s) on the universal/aarch64/x86_64 build

A leftover `/Volumes/Cmdr` mount (typically from a Finder double-click on an old DMG) makes the new bundle fail because
the volume name is already taken. Both `scripts/release.sh` and the release workflow detach `/Volumes/Cmdr*` mounts
before building, so this should be self-healing. If you hit it anyway (for example, you mounted a DMG between the
workflow's detach step and the actual build), detach manually and re-run failed jobs:

```bash
hdiutil detach /Volumes/Cmdr -force      # or "Cmdr 1", etc.
gh run rerun <release-run-id> --failed
```

### Tauri bundles unexpected binaries

Tauri's bundler includes all `[[bin]]` targets from the cmdr package, not just the main `Cmdr` binary. Dev-only tools
must live in separate workspace crates (like `crates/index-query/`) to stay out of the bundle. Non-`.rs` files in
`src/bin/` (like `CLAUDE.md`) also confuse the bundler; it strips the extension and tries to bundle the result as a
binary.
