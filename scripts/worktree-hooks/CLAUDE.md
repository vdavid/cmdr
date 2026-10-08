# Worktree hooks

Per-repo hooks that David's shared worktree scripts (`~/.claude/scripts/new-worktree.sh`, `remove-worktree.sh`) call, so
a project can warm or hand back its own caches without the scripts knowing anything project-specific.

## Module map

- `post-create`: seeds the new worktree's Linux target volume from the main clone's.
- `pre-remove`: promotes a merged worktree's Linux target volume to the main clone's.
- Both are thin wrappers over `go run ./linux-cache` in `scripts/check/`; the logic is
  `scripts/check/linux-cache/handoff.go`.

## The contract

- **Found at** `<main clone>/scripts/worktree-hooks/<name>`, run only if executable. Missing means nothing to do.
- **Arguments**: `$1` the worktree's absolute path, `$2` the main clone's. `pre-remove` may get `--dry-run` as `$3`, and
  then changes nothing. stdin is `/dev/null`.
- **When**: `post-create` runs in the background warming stage, while `.warming-worktree` still blocks builds.
  `pre-remove` runs before anything is deleted, and may run even when the teardown then refuses (uncommitted work), so
  it must copy, never move or delete what the worktree still owns.
- **Exit**: 0 for done, nothing to do, AND skipped (no Docker, a build holding the cache). Non-zero means it broke; the
  caller prints one line and carries on. ❌ A hook never fails worktree creation or teardown.
- **Output**: at most a line or two, human-readable. The caller routes it (the warming log, or its own stdout).
- **Fast**: seconds. Anything slow belongs in a check lane, not a hook.

## Must-knows

- **"Merged" is git ancestry plus a clean worktree**, never the teardown's `--force`: an rlib built from an uncommitted
  edit would pass as fresh next to the main clone's older copy of that file.
- **Copies are btrfs reflinks** (15 GB in ~1 s, ~0 new disk). ❌ Don't fall back to a plain copy: it'd put minutes of
  I/O in a hook.

Why the design, races, and measurements: `DETAILS.md`.
