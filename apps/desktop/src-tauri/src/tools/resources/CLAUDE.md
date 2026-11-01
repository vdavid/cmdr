# MCP resources (`tools/resources/`)

The read-only `cmdr://` views an AI client reads instead of asking: `state`, `dialogs/available`, `indexing`,
`importance`, `settings`, and `logs`. `mod.rs` is the shared spine (registry, URI/query parsing, `read_resource`
dispatch, and the `cmdr://state` builder); the independently-evolving builders sit in `logs.rs`, `indexing.rs`,
`importance.rs`, `operations.rs`, and `volumes.rs`.

## Must-knows

- **An agent ACTS on what it reads here, so a number has to say how strong it is.** A directory size is never bare:
  `≥` is a SETTLED lower bound, `~` a number still moving (it wins over `≥`, which has no floor to promise while a walk
  rewrites it), `[size-unsettled]` / `[size-stale]` qualify it, and `(N on disk)` counts hard links and clones in FULL,
  so it isn't "what deleting frees". ❌ Don't strip a qualifier to save tokens: an uncounted total presented as settled
  becomes a confident wrong answer (a 129 GB tree once read as 28.8 GB). ❌ Don't derive `[size-unsettled]` from the raw
  per-folder pending flag: the frontend pushes the whole answer as `recursiveSizeUpdating`.
- **Capacity and free space come from the space poller's CACHE, ❌ never a `statfs` here**: that syscall blocks
  30–120 s on a hung mount, and `cmdr://state` is read constantly. An unwatched volume omits both fields, ❌ never
  renders a zero that reads as a full disk.
- **The user's paths and names render verbatim; raw diagnostic text is redacted.** A read-only agent is trusted and
  must name the user's files to serve them, so panes, `volumes:` `mountPath`, `favorites:`, and every `operations:`
  field stay as-is. `recentErrors` and `cmdr://logs` go through unsalted `crate::redact::redact_line` (bare tokens, ❌
  never a `RedactionContext`); `logs` `filter` matches the REDACTED line, ❌ never the raw one (a raw match lets a
  caller probe for a path by line count). Why: `DETAILS.md` § "What renders verbatim".
- **Builders are pure over an injected snapshot** (`snapshot_volumes`, `snapshot_indexing`, and friends, with `now_*`
  passed in), so formatting is unit-tested off fixtures without a live app. Keep new builders on that seam.
- **A missing store reads as empty or unknown, ❌ never an error and ❌ never a `0`.** An absent index, an unscored
  volume, and a backend that can't classify itself all render as "can't tell"; a zero would be compared against.

Each resource's payload, query shapes, and gotchas: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
