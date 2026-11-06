# Moving the repo to the `getcmdr` GitHub org: runbook

The transfer-day runbook for #476 (`vdavid/cmdr` to `getcmdr/cmdr`). The audit behind it (what moves, what breaks,
what's identity-bound) is in the issue. Delete this note once the move is done and verified.

## What's ready

- **v0.55.0 accepts both names** (#481): the advisory link allowlist and its Go check, the changelog commit-URL guards,
  and `release-finish`, `pnpm land`, `release-ci-gate.sh`, `release.sh`, and `release-evidence` ask `gh repo view` for
  the name. Pre-move app versions vet advisory links with their own allowlist, so keep writing advisory URLs as
  `vdavid/cmdr` (they redirect) until builds before v0.55.0 age out.
- **`release-evidence` checks a release under the repo that built it**, read from the `latest.json` on the release and
  accepted only if it's the current name or `vdavid/cmdr` (`scripts/release-evidence/builtin.go`). So the last pre-move
  release still verifies after the move. `release-finish` checks the release in flight, which is never pre-move; its dry
  runs take `-signer-workflow` for older ones.
- **All four rulesets live in `.github/rulesets/`**, and `pnpm land rulesets` reported all four matching the live ones
  (2026-10-22), so `-apply` restores any that don't carry over.
- **The swap is a script, re-run on transfer day**: `./scripts/org-move.sh -cutoff vX.Y.Z [-dry-run] [-v]`. The policy
  is `scripts/org-move/rules.go`: the file types it rewrites, the paths it leaves alone (each with a reason), the
  mentions it keeps wherever they appear, and hand-written edits a rename can't express. It's idempotent, prints every
  change and every kept mention by reason, and exits 1 on a mention no rule covers or an edit whose anchor is gone.
- **What the swap rewrites**: `github.com/vdavid/cmdr`, `git@github.com:vdavid/cmdr`, `repos/vdavid/cmdr`,
  `-R`/`--repo vdavid/cmdr`, bare `vdavid/cmdr` and `vdavid/cmdr#N`, and the runner service name
  `actions.runner.vdavid-cmdr.*`.
- **What it leaves alone**: `CHANGELOG.md`, blog posts, `docs/notes/` (this note included), the hackathon record,
  `apps/website/public/latest.json` (the next release rewrites it), the advisory link allowlist and its Go mirror, the
  release and landing tools (they resolve the name), local paths (`~/projects-git/vdavid/cmdr`), every advisory link
  (`.../security/advisories/GHSA-...`), the pre-v0.51.0 `release.yml` signer, and other repos (`cmdr-reports`,
  `cmdr-fsevent-stream`, `vdavid/tap` and `homebrew-tap`, `smb2`, `mtp-rs`, David's `github.com/vdavid` profile).
- **The hand-written edits**: `/trust`, `/trust/development`, `docs/guides/releasing.md`, and the attest job's comment
  in `release-pipeline.yml` show both attestation repos, naming the `-cutoff` release as the last one built under
  `vdavid/cmdr`. The release guide gains the "keep advisory links on the old name" line. Comments that said "if the repo
  moves" switch to past tense.
- **Decision: issue refs like `vdavid/cmdr#605` swap** in live code and docs. **Why**: both forms redirect, but a bare
  `#605` is ambiguous next to `cmdr-reports` and `smb2`, and a swapped tree makes every later `git grep vdavid/cmdr` hit
  a deliberate one. Dated notes and the changelog keep theirs.
- **Gotcha: the swap can't go green before the transfer.** The website's Docker build fetches `latest.json` from
  `github.com/getcmdr/cmdr/releases`, which 404s until the repo lives there (`docker-build` failed that way on the
  preview, 2026-10-22).

## Before the day

1. **David: org settings** at github.com/organizations/getcmdr/settings. Already set (checked via the API, 2026-10-22):
   Free plan, 2FA required, base permission Read. Still to confirm (the API needs `admin:org`): Actions "allow all"
   (matching the repo), default workflow permissions "read", fine-grained PATs allowed, GitHub CLI approved if OAuth app
   restrictions are on. Optional: verify `getcmdr.com` (an agent can add the DNS TXT record via the Cloudflare API).
2. **David: install the GitHub Apps on the org**, scoped to `cmdr`, before the transfer so there's no gap: Renovate
   (also a bypass actor on the `main: PR with approval` ruleset, integration 2740), GitGuardian, Claude, and Lighthouse
   CI. Confirm the full list at github.com/settings/installations (the API can't enumerate it).
3. **Agent: confirm the rulesets still match their files**: `pnpm land rulesets` reports all four as matching. Commit
   any drift into `.github/rulesets/` first, so the files are what the move restores.
4. **Pick the day**: a quiet evening right after a release, so no release straddles the move.

## Transfer-day order

1. **Pause the agents and the land queue.** No `pnpm land` running, nothing about to land:
   `gh pr list -R vdavid/cmdr --state open` and `gh run list -R vdavid/cmdr --status in_progress` show only what you
   expect. A `pnpm land` already running resolved the old name at start; let it finish or stop it.
2. **No release in flight.** `gh run list -R vdavid/cmdr --workflow release.yml --limit 5` all completed,
   `gh release list -R vdavid/cmdr --limit 3` shows no draft, and the latest tag is on `origin/main` (its merge-back
   landed). In `local` signing mode a release's finishing run re-attests the published release, which would mix names on
   one release.
3. **Note the cutoff**: `CUTOFF=$(gh release view -R vdavid/cmdr --json tagName -q .tagName)`.
4. **David: transfer.** `vdavid/cmdr` > Settings > Danger zone > Transfer > `getcmdr`, keep the name `cmdr`. ❗ Never
   create a new `vdavid/cmdr` while the repo lives in the org: a new repo at the old name takes over the path, and every
   redirect (web, API, git remotes, links in shipped app versions) breaks. A repo transfer doesn't retire the old name,
   so nothing protects it but this rule.
5. **Agent: verify what carried over.** `pnpm land rulesets` (all four match), `gh api repos/getcmdr/cmdr/environments`
   (`release`, `copilot`), `gh secret list -R getcmdr/cmdr` (nine) and `--env release` (eight Apple secrets),
   `gh variable list -R getcmdr/cmdr` (`RELEASE_UPDATE_SIGNING`), `gh api repos/getcmdr/cmdr/hooks`
   (`https://api.getcmdr.com/webhook/github`, `issues`), private vulnerability reporting, secret scanning, and push
   protection. Missing or drifted rulesets: `pnpm land rulesets -apply`.
6. **Agent: update the remotes.** Both laptops: `git remote set-url origin git@github.com:getcmdr/cmdr.git` and
   `gh repo set-default getcmdr/cmdr` in the main clone (worktrees share it). The M1's clone. `/opt/cmdr` on the Hetzner
   box (`https://github.com/getcmdr/cmdr.git`, as the `deploy-cmdr` user). All keep working through the redirect; update
   them anyway.
7. **Credentials.** David issues a fine-grained PAT for the M1 with `getcmdr` as resource owner (a fine-grained PAT has
   one owner, so the `vdavid` one sees nothing in the org, and failures look like plain 404s); an agent stores it as
   `GH_TOKEN` in the M1's sops store. David regenerates `LHCI_GITHUB_APP_TOKEN` for the org install of Lighthouse CI; an
   agent sets it with `gh secret set LHCI_GITHUB_APP_TOKEN -R getcmdr/cmdr`.
8. **Agent: copy the browser demo images to the org.** `ghcr.io/vdavid/cmdr-demo` is a package of David's account, and
   packages don't move with the repo. Copy every tag (at least the latest release's and the pre-cutover content-keyed
   one in `scripts/released-demo.sh`) with
   `docker buildx imagetools create -t ghcr.io/getcmdr/cmdr-demo:<tag> ghcr.io/vdavid/cmdr-demo:<tag>`, which keeps the
   digests, so the attestations still match. Needs `gh auth refresh -s write:packages` and `docker login ghcr.io`. Then
   David sets github.com/orgs/getcmdr/packages/container/cmdr-demo/settings to public and gives the `cmdr` repo Actions
   write access there. `released-demo.sh` then resolves the image under the org and accepts the pre-move signer.
9. **Agent: run the swap on fresh `main` and land it.**
   - `WT=$(~/.claude/scripts/new-worktree.sh org-move-swap)`, then in it `./scripts/org-move.sh -cutoff "$CUTOFF"`. It
     must exit 0. Read its "Kept on the old name" section, and fix `rules.go` for anything new that's misfiled.
   - Commit the output, run `pnpm check oxfmt` (rewraps lines that grew) and commit that too, then run the lanes the
     swap touches:
     `pnpm check website api-server dashboard go desktop-svelte-eslint desktop-svelte-check desktop-svelte-tests desktop-rust-rustfmt desktop-rust-clippy`
     (Rust comments change). `docker-build` passes now.
   - Re-run `./scripts/org-move.sh -cutoff "$CUTOFF" -dry-run`: "Would swap 0", every edit "already applied".
   - `git grep vdavid/cmdr -- ':!docs/notes/' ':!CHANGELOG.md'` lists only what the "Kept" section explains.
   - `pnpm land`. It's the first push under the org: watch CI, both `main` rulesets, Renovate, GitGuardian, and the
     website deploy webhook fire.
10. **Resume the agents.** A session that cached the old name in a running `pnpm land` restarts it.

## Verification checklist

- `curl -sI https://github.com/vdavid/cmdr` answers `301` to `getcmdr/cmdr`, and
  `gh api repos/vdavid/cmdr -q .full_name` says `getcmdr/cmdr`.
- `git ls-remote https://github.com/vdavid/cmdr HEAD` works (old clones and remotes).
- `gh attestation verify <a v0.55.0 asset> --repo vdavid/cmdr --signer-workflow vdavid/cmdr/.github/workflows/release-pipeline.yml`
  still passes. Expected through the API redirect, never tested before the move.
- `https://getcmdr.com/download/latest/aarch64` and `https://api.getcmdr.com/update-check/0.54.0?arch=aarch64` still
  resolve to working assets, before and after the swap deploys the api-server.
- A test issue on `getcmdr/cmdr` reaches the backlog board through the webhook; close it after.
- The website header, footer, `/trust` (both names and the cutoff), and the star count; the dashboard's star and release
  sections.
- Scorecard: `gh workflow run scorecard.yml -R getcmdr/cmdr` republishes now (history restarts at the new URL), then the
  README badge and `https://scorecard.dev/viewer/?uri=github.com/getcmdr/cmdr` show a score.
- Renovate and GitGuardian report on the swap's PR; `pnpm land` printed `getcmdr/cmdr`.

## After the move

- **Outside the repo**: the vault notes naming `vdavid/cmdr` (priority: the CRA technical file, the security policy,
  `github-backlog-project.md`), the `infra` repo's Hetzner and M1 docs, and the report-triage role's defaults (its clone
  URL). David edits the external listings (MacUpdate, AlternativeTo) from `brand/listings/`.
- **The next release, with David**: `release-finish` resolves `getcmdr/cmdr`, attests, and verifies under it; check the
  tap bump and `release-evidence.sh verify`.
- **Later, if wanted**: a merge queue for landing (needs a `merge_group:` trigger in `ci.yml` first, or required checks
  never report on queue entries). `cmdr-reports`, the backlog board, and the Homebrew tap stay under `vdavid` (the
  audit's recommendation).

## If it's ever reversed

- Transfer it back to `vdavid` under the same name: a transfer doesn't retire `vdavid/cmdr`, so the name is free to take
  back. Revert the swap's commits with a normal land, then run this runbook's chores in reverse (remotes, PATs, apps).
- Keep the empty `getcmdr` org, so nobody takes `getcmdr/cmdr` and breaks the redirects from it.
- Attestations from releases built in the org name `getcmdr/cmdr` for good, so `/trust` would list both names the other
  way round.
- ❌ Never create a new `vdavid/cmdr` while the repo lives in the org.
