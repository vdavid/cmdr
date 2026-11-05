Do this week's dependency pass, the one Renovate structurally can't: the `desktop non-major updates` and
`all major updates` lanes. Read @docs/guides/update-dependencies.md first; it's the canonical procedure and the
rationale. This file is only the wrapper around it.

The pass also covers David's three sibling repos: `~/projects-git/vdavid/mtp-rs`, `~/projects-git/vdavid/mtp-mount`, and
`~/projects-git/vdavid/smb2`. Renovate automerges their non-majors, so there the job is what it can't finish: red or
stuck Renovate PRs, the Dependency Dashboard's pending approvals and majors, and crates held by a `renovate.json` rule
(each rule's `description` says why and when to lift it).

Run unattended, start to finish, without asking David anything. He's reading the result, not supervising the run.

1. **Worktree.** `~/.claude/scripts/new-worktree.sh weekly-upgrades`, then `EnterWorktree` with the path it prints. Wait
   for `.warming-worktree` to disappear before the first build.
2. **Fan out.** Before the Cmdr work, spawn one background subagent per sibling repo. Each one reads that repo's
   `AGENTS.md` and `renovate.json` (its auto-loaded instructions are Cmdr's), makes its own `weekly-upgrades` worktree
   there, runs every command as `(cd <its worktree> && ...)` (a bare `cd` or `EnterWorktree` would move the whole
   session), runs the repo's own checks, lands on that repo's `main` on green GitHub CI, and tears its worktree down. No
   releases, no crates.io publishes. A repo with nothing to do reports that in one line.
3. **Do the Cmdr pass** per the guide: dashboard for the list, npm (`ncu -u`, `pnpm install`, `pnpm dedupe`), Cargo
   (`cargo outdated -w`, then per-crate `Cargo.toml` edit + `cargo update -p <crate> --precise <version>`, 3-day window
   enforced by hand against crates.io), fix the breakage, `pnpm check third-party-notices`, the checks per the guide,
   then `pnpm check --only-slow`.
4. **Commit in logical pieces**, not one blob: npm bumps, Cargo bumps, and each non-trivial API fix separately. Lead
   with impact. A crate you had to hold back gets a line in the body saying which and why.
5. **Land and tear down.** `pnpm land` (`scripts/land/CLAUDE.md`), then
   `~/.claude/scripts/remove-worktree.sh weekly-upgrades`.
6. **Report** in about ten lines for Cmdr plus two or three per sibling repo: what moved (counts per ecosystem, and name
   anything interesting), what broke and how you fixed it, what you held back and why, and whether the slow lane (Cmdr)
   and GitHub CI (siblings) went green.

**When it's a quiet week**, say so in two lines and stop. An empty dashboard is a fine outcome; don't manufacture work.

**When something needs David**, finish everything else first, commit it, and put the question at the end of the report.
A single crate needing an architectural call doesn't block the other forty.
