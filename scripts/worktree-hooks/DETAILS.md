# Worktree hooks: details

The contract is in `CLAUDE.md`. What the Cmdr hooks do to the Linux target volume (the copy, its locks, the swap, the
races, the numbers) is `scripts/check/checks/DETAILS.md` § "The Linux Docker lanes share an image and a build cache".
This file holds the decisions behind the hook mechanism itself.

## Decisions

- **Decision: the caller runs the MAIN CLONE's hooks, and a hook runs its own checkout's Go.** Why: the main clone
  exists at both events and holds the current policy; the worktree's copy may be missing (a base older than the hooks)
  or unmerged. After a merge the two are identical anyway.
- **Decision: the hooks are bash wrappers over a Go CLI (`scripts/check/linux-cache/`), like `stack-lease`.** Why: the
  volume name comes from `checkoutCacheKey` and the labels from the lane's constants; a shell copy of the key formula
  would be one more thing to pin with a test. The CLI costs ~0.5 s of `go run` when the build cache is warm.
- **Decision: the hooks take paths, not a slug or a branch.** Why: the volume key is the checkout's absolute path, and
  "merged" is read from git in both checkouts; a slug would make every project re-derive the paths. The wrappers make
  both paths absolute before `cd`-ing into `scripts/check`.
- **Decision: a hook decides "merged" itself, from git.** Why: `remove-worktree.sh --force` also removes unmerged work,
  so the flag can't mean "merged", and the hook is callable by hand too.

## What seeding buys, measured

2026-09-30, OrbStack, a tree with every source file's mtime set to "now" (what `git worktree add` produces) against a
reflink seed of a warm volume: `clippy-linux`'s cargo run took 47 s (the 13 workspace crates rechecked, every
third-party crate fresh), against about 90 s cold and under 1 s warm. Cargo judges a path crate by source mtime, and a
fresh checkout's mtimes are all newer than the seeded fingerprints, so a seed can't make the workspace crates fresh. End
to end, the same day: a worktree from `new-worktree.sh` (seeded in 0.8 s) ran `pnpm check clippy-linux` in 47 s.

Resetting unchanged files' mtimes to the main clone's would make the workspace crates fresh too, but the host `target/`
clone has the same trait, so if that's ever done, it belongs in one place serving both.
