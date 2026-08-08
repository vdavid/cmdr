# E2E shared helpers details

Depth and rationale for the pre-launch E2E helpers. `CLAUDE.md` holds the must-knows.

## Test coverage

`fixtures.test.ts` is the Vitest suite for the fixture builder: covers the cache population race, hardlink cross-shard
sharing, the `EXDEV` fallback, the recreate-text-files contract, and the legacy single-shard path. The race scenarios
are covered deterministically.

`fixture-manifest.test.ts` covers the drift guard: every mutation shape the specs make (add, remove, rename,
same-length overwrite, in-place archive edit, shortened bulk file, file↔dir swap, stray symlink, missing directory),
the sibling dirs it deliberately ignores, and the two properties of the repair — it clears every shape in one pass, and
it leaves untouched entries on their original inode.

## Hardlink cache protocol

The cache is built once at `/tmp/cmdr-e2e-fixtures-cache/`. Two concurrent runs both finding it missing each write to
their own `/tmp/cmdr-e2e-fixtures-cache-tmp-<pid>/`, populate the deterministic zero-fill `.dat` files, verify via size
+ content check, then atomically `renameSync` to the final path; the rename loser cleans up its tmp dir. The cache's
existence means "populated and verified", so torn writes are structurally impossible. On `EXDEV` (cross-filesystem
hardlink) it falls back to copy with a warning. Source of truth: `ensureCacheBuilt()` in `fixtures.ts`.

Its one soft spot is a cache that gets corrupted IN PLACE through a fixture hardlink: `ensureCacheBuilt` sees an
invalid cache, builds a replacement, and then loses its `renameSync` onto the populated directory to the "another
builder won" branch — so the corruption survives. Hence the never-write-in-place rule in `CLAUDE.md`; recovering means
removing `/tmp/cmdr-e2e-fixtures-cache/` by hand.

## The fixture manifest

`pristineFixtureEntries()` in `fixtures.ts` is the one description of the pristine tree: each entry carries its path,
kind, and the recipe that recreates it (inline text, a copy of a committed `media-fixtures/` / `archive-fixtures/`
source, or a bulk `.dat` of a known size). `fixture-manifest.ts` turns that into both the expected manifest and the
repair, so the layout can't drift from the thing that checks it.

Contract and cost live where the guard is used: `../e2e-playwright/DETAILS.md` § "The fixture-tree leak guard".

## Decisions

- **Per-instance fixture root with hardlinks instead of full copies.** Copying 170 MB × N shards × M concurrent runs
  blows past `/tmp` quotas and adds seconds to every E2E launch. Hardlinks are zero-cost after the first populate; tests
  treat the files as read-only.
- **Text files are NOT cached: full copies per shard.** `file-operations.spec.ts` and similar mutate them. Recreating
  from a small in-memory template costs less than tracking which files got mutated and re-syncing from the cache.
- **Port-file read NEVER falls back to legacy ports silently.** A silent fallback hides bugs (the test "works" but
  against the wrong instance). The strict precedence ladder (env → file → typed error) makes mis-configurations loud.

See `docs/tooling/instance-isolation.md` § "Per-resource breakdown" for the full port-file design.
