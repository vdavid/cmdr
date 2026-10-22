---
description: How to update project dependencies
---

# Update dependencies

## Who updates what

Renovate (`renovate.json`) still owns these lanes and auto-merges them unattended. **Leave them alone**: website,
Cloudflare apps (`api-server` + `analytics-dashboard`), linters and formatters, Docker images, security advisories, and
David's own crates (`smb2`, `mtp-rs`).

❗ **Renovate rebases a PR only on a conflict, and touches it only in its schedule window** (`rebaseWhen: conflicted`,
`updateNotScheduled: false`). The default rebases an automerge PR on every push to `main`, and a red one re-ran the full
CI on each landing: #486 burned about 15 runs in two days on a fixture change no rebase could fix. `main` doesn't
require up-to-date branches, so automerge works without it. The accepted cost: two automerged PRs can land without CI
ever seeing them together, which the lanes' disjoint files make unlikely to matter. A red Renovate PR needs a human fix
(or a ticked "rebase" box on the PR once `main` holds one).

Two lanes are hand-driven, because a bot can't finish them:

- **`desktop non-major updates`** (`apps/desktop/**`, `crates/**`, `rust-toolchain.toml`)
- **`all major updates`**

Both carry `dependencyDashboardApproval`, so Renovate lists them on the Dependency Dashboard (issue #11) and opens no
PR. The weekly `/weekly-upgrades` pass does them.

**Why they can't automerge**, so nobody re-litigates this:

1. Every lockfile touch leaves `THIRD-PARTY-NOTICES.md` and `third-party-packages.gen.json` stale. Those are committed
   and ship to users. The check REGENERATES them on a local run and only VERIFIES in CI, so a bot PR is red by
   construction. Renovate can't run the regeneration either: `postUpgradeTasks` needs `allowedCommands`, which is
   self-hosted admin config the Mend-hosted app doesn't expose.
2. Semver-minor Rust crates break compilation for real. `russh` 0.62.7 → 0.63.0 changed the `check_server_key` trait
   signature and broke `cmdr-sftp` (2026-09-19). No bot writes that fix.

Two attempts proved it: PR #41 closed unmerged, PR #97 red for five days, zero desktop-group PRs ever merged.

## The pass

Work in a worktree. Take npm and Cargo in **separate commits**, so a bisect can tell them apart.

1. **Read the dashboard** (`gh issue view 11`) for the outstanding list. It names packages, not target versions; it's
   the cross-check that you didn't miss a lane, not the source of versions.
2. **npm**: `ncu -u && pnpm install && pnpm dedupe`. The 3-day safety window is automatic here:
   `minimumReleaseAge: 4320` in `pnpm-workspace.yaml` means pnpm won't resolve anything younger. Without `dedupe`,
   nested transitive deps stay pinned to old versions and cause false-positive failures (stylelint/postcss misparsing
   Svelte inline styles, Playwright version skew between AxeBuilder and the e2e specs).
3. **Cargo**: `cargo outdated -w` for the list. ❌ Never bare `cargo update`, which drags the whole graph forward with
   no age gate. Per crate, edit `Cargo.toml` to the chosen version, then `cargo update -p <crate> --precise <version>`
   to refresh just that entry.
   - **The 3-day window is yours to enforce here.** crates.io has no equivalent of pnpm's gate:
     `curl -s https://crates.io/api/v1/crates/<name>/versions | jq -r '.versions[] | "\(.num) \(.created_at)"'` and take
     the newest one older than three days.
   - Verifying a risky bump: download both `.crate` files from `static.crates.io` (the crates.io API refuses
     unauthenticated downloads) and `diff -ru` them, so you review what cargo actually installs rather than a GitHub
     compare.
4. **Fix what breaks.** Expect a couple of trait-signature or renamed-API breaks per pass. That's the job, not a reason
   to drop the crate from the batch. If one crate needs more than a contained fix, leave it at its current version, and
   say so in the commit body and to David.
5. **Regenerate the notices**: `pnpm check third-party-notices`. It rewrites both generated files from the resolved
   graph. Skipping it lands green locally and reds `Desktop (Rust)` on `main` (2026-08-26).
6. **Check**: `pnpm check --include-slow`. A dependency pass is exactly the case the slow lane exists for; a Rust bump
   also means clippy, and the E2E suite is what catches a Tauri-plugin regression.

## Majors

Same pass, but one major per commit, and read the release notes first. Some are pinned shut on purpose, and the
`renovate.json` rule says why, like the `specta` trio (rc.25 types non-`Option` float PARAMETERS as `number | null`).
Don't "fix" those by bumping them.

## Version constraints

- Node, pnpm, Go: `.mise.toml`
- Rust: stable channel, `rust-toolchain.toml`
- Frontend deps: `package.json`
- Rust deps: `apps/desktop/src-tauri/Cargo.toml`

We try to use the latest of everything.
