# A cover walk with no ground used to block on the writer

What a search paid when every frontier root it asked for belonged to a walk already running, measured in the running app
on 2026-08-15 (dev build of `worktree-david+cover-block`, boot disk, macOS 26.1, `RUST_LOG=…lifecycle::cover=debug`).

## The symptom

A search over an area a drive's first index hasn't reached yet sat silent, saying "0 matches so far", and then reported
that its walk had covered nothing: `Cover: 0 entries over 0 frontier roots`. A first report from the app measured 50.1 s
of that silence, ~35 s of it inside the walk.

## What the time was

All of it was the writer.

`cover::start` claims each frontier root, and a root another walk holds is left to that walk. When that took EVERY root,
the walk thread still ran `walk_frontier` end to end: it opened a backend session, walked an empty frontier, and then
committed the writer, because a search-driven walk takes `FlushOnFinish::BeforeReporting` (the marks matter more than
the rows: a caller that asks a coverage question the moment its walk ends would otherwise be told the wrong thing). The
writer is one thread behind one bounded queue per database, so that commit parks behind everything already queued — and
during a drive's first index the queue is the first index.

Two runs of the same scripted repro (wipe the data dir, launch, let the phased first index start, run one search over a
big cold subtree so it claims the ground, then a second search inside it):

- `Cover: 0 entries over 0 frontier roots in 5.8s (5.8s of it waiting on the writer)`
- `Cover: 0 entries over 0 frontier roots in 4.5s (4.5s of it waiting on the writer)`

100% of the block, both times. The call that asked for the walk was never the problem: `Index::cover` itself returned in
33-104 µs. What blocked was `CoverWalk::finish`, waiting for that thread to get through its commit.

End to end, the same searches took 6.5 s and 7.2 s against ~0.1 s for a search over the same scope once its ground was
free. The app's own first report of 35 s is the same mechanism at a bigger backlog: this machine's boot disk was
page-cache warm, so its whole first index ran in ~2.5 min where a cold one takes much longer, and the depth of the
writer queue at any moment is what the wait is.

## After

A claim that takes no ground now returns a walk with no thread at all (`CoverWalk::took_no_ground`), so there is no
session to open, no frontier to walk, and nothing to commit.

- `TEMP: the walk that took no ground finished in 20.4µs` (against 4.5-5.8 s).
- The run reaches `SearchPhase::WaitingForAnotherWalk` — "Waiting for another scan of these folders..." — in about a
  second rather than after the block, so the honest affordance fires while the person is still looking at it.

## The instrumentation that found it

`Cover:`'s summary line now carries its own duration and how much of it was `writer::wait_probe` (the same split
`reconcile_subtree` reports). Without the split the line reads as a slow walk, and the reader goes hunting in the
walker. Both numbers stay in the shipped debug log.

## What this note does NOT settle

A cold-drive search with thousands of frontier roots spends measurable time on the CALLER's thread before the walk
starts: one run showed `Index::cover` taking 3.0 s for 2,503 roots, which is `state::begin_branch_coverage` registering
them one at a time under the registry lock. That is a different cost with a different fix, and nothing here measured it
properly.
