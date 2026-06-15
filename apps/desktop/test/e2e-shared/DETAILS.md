# E2E shared helpers details

Depth and rationale for the pre-launch E2E helpers. `CLAUDE.md` holds the must-knows.

## Test coverage

`fixtures.test.ts` is the Vitest suite for the fixture builder: covers the cache population race, hardlink cross-shard
sharing, the `EXDEV` fallback, the recreate-text-files contract, and the legacy single-shard path. The race scenarios
are covered deterministically.

## Decisions

- **Per-instance fixture root with hardlinks instead of full copies.** Copying 170 MB × N shards × M concurrent runs
  blows past `/tmp` quotas and adds seconds to every E2E launch. Hardlinks are zero-cost after the first populate; tests
  treat the files as read-only.
- **Text files are NOT cached: full copies per shard.** `file-operations.spec.ts` and similar mutate them. Recreating
  from a small in-memory template costs less than tracking which files got mutated and re-syncing from the cache.
- **Port-file read NEVER falls back to legacy ports silently.** A silent fallback hides bugs (the test "works" but
  against the wrong instance). The strict precedence ladder (env → file → typed error) makes mis-configurations loud.

See [`docs/tooling/instance-isolation.md`](../../../../docs/tooling/instance-isolation.md) § "Per-resource breakdown"
for the full port-file design.
