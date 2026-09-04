# MCP resources (`mcp/resources/`)

The read-only `cmdr://` views an AI client reads instead of asking: `state`, `dialogs/available`, `indexing`,
`importance`, `settings`, and `logs`. `mod.rs` is the shared spine (registry, URI/query parsing, `read_resource`
dispatch, and the `cmdr://state` builder); the independently-evolving builders sit in `logs.rs`, `indexing.rs`,
`importance.rs`, `operations.rs`, and `volumes.rs`.

## Must-knows

- **An agent ACTS on what it reads here, so a number has to say how strong it is.** A directory size is never bare:
  `≥` means lower bound, `[size-pending]` / `[size-stale]` qualify it, and `(N on disk)` counts hard links and clones
  in FULL, so it isn't "what deleting frees". ❌ Don't strip a qualifier to save tokens: an uncounted total presented
  as settled becomes a confident wrong answer (a 129 GB tree once read as 28.8 GB).
- **Capacity and free space come from the space poller's CACHE, ❌ never a `statfs` here**: that syscall blocks
  30–120 s on a hung mount, and `cmdr://state` is read constantly. An unwatched volume omits both fields, ❌ never
  renders a zero that reads as a full disk.
- **`cmdr://state` and `cmdr://logs` redact through `crate::redact::redact_line`** — the only thing keeping home
  paths, SMB URIs, and emails out, since a loopback caller has no filesystem read. `logs` `filter` matches the RAW
  (pre-redaction) line. The two exceptions are deliberate: `favorites:` paths and a `pendingConflict:` block render
  verbatim, because a clash an agent can't name is one it can't answer.
- **Builders are pure over an injected snapshot** (`snapshot_volumes`, `snapshot_indexing`, and friends, with `now_*`
  passed in), so formatting is unit-tested off fixtures without a live app. Keep new builders on that seam.
- **A missing store reads as empty or unknown, ❌ never an error and ❌ never a `0`.** An absent index, an unscored
  volume, and a backend that can't classify itself all render as "can't tell"; a zero would be compared against.

Each resource's payload, query shapes, and gotchas: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
