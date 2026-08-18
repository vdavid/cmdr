# Suggested ops details

Pull-tier docs for `agent/suggested_ops/`. Must-knows live in `CLAUDE.md`; the persistence layer is
`../store/proposals/DETAILS.md`; the feature plan is `docs/specs/agent-suggested-ops-plan.md`.

## Selectors: proposing 60 000 ops without naming 60 000 paths

The agent can't enumerate 60 000 paths through its context window, so it may propose over a **pattern**: a root, a name
glob, and deterministic predicates (size and modification-time bounds). Every predicate is one the user can check
against the file itself, which is what makes a selector reviewable rather than a claim to take on faith.

Resolution turns that pattern into concrete ops **once, at creation**, and the group's rows are the only account of what
it proposes from then on. Two things depend on that freeze:

- The review dialog shows the pattern and expands to the exact list, so "what the user saw is what runs" is literally
  true.
- Re-resolving at approval would silently widen a group between the review and the click. A file that landed in
  `~/Downloads` while the user was reading would get trashed without ever having been shown.

`tests.rs` pins it with a fake index that counts how often it's asked: after the group exists, the fake grows two more
matching files, and the approval still runs the original two ops with the resolver asked exactly once.

## Why the drive index and not the search module

`search::execute::run_blocking` looks like the natural resolver and isn't: the engine caps results at 1 000
(`engine.rs`'s `query.limit.min(1000)`) because it serves a ranked, interactive top-k, and it loads a whole-volume arena
to do it. A selector needs the EXHAUSTIVE set. So `DriveIndex` reads the volume's index DB directly through
`Index::read_pool`, resolves the root with `store::resolve_path`, and descends one directory's children at a time,
carrying each directory's path down rather than reconstructing a path per file.

What it does reuse is the search module's matcher: `CompiledQuery` + `Candidate` decide whether a row satisfies the
selector, so a selector and the search box compile `*.dmg` the same way and fold case the same way. The compile runs
with `Evaluator::Arena { entries: 0 }`, which never refuses a broad query — the scope is one subtree the agent named,
and "everything in this folder" is a legitimate proposal, unlike an unbounded search.

Paths: a non-root volume's index stores them relative to its mount root, so the root is mapped in with
`Index::read_path` and every hit is mapped back out with the mount root from `search::volumes::registry_mount_root`
(the one place that answers "where is this volume mounted", so nothing forks it). Resolution sorts by path, so the same
selector over the same index freezes the same op sequence twice running.

**Not available through the index today: "last opened".** The plan's flagship example ("installers you've already
opened") wants it, and the drive index carries size, modification time, and inode but no access time. The visit counts
in `importance.db` are per-FOLDER, not per-file. So a selector can express "old `.dmg` files in Downloads" but not yet
"ones you opened"; wiring an access-time source is its own effort, not a line in this module.

## The display text, and why it carries no prose

`OpSelector::pattern_text` returns `<root>/<glob>` and nothing else. The predicates (age, size) render from the stored
JSON in the review dialog, where they can be localized. A sentence built here ("older than 30 days") would ship one
language into the database, and a proposal that waits two weeks would still be in that language after the user switched
theirs.

## The metric

Acceptance rate is the agent's north-star metric (agent-spec D46): a suggestion feature whose suggestions get rejected
is worse than none. `analytics.rs` emits `suggestion_group_proposed`, `suggestion_group_approved`, and
`suggestion_group_rejected`, each carrying the verb token and a coarse count bucket through the shared
`analytics::item_count_bucket` (shared so two dashboards can't end up with two ideas of what "a lot" means).

The events land in M1, before the dialog exists, deliberately: David's own QA pass then produces real numbers before
launch rather than after.

An approval is only reported when a claim actually went through — a refused claim is not an approval. A rejection reads
the group's verb and live op count BEFORE the transition, because that's the group the user was looking at when they
said no.

## The approval bridge

`bridge/` is the whole hand-off from an approved group to a running operation: preflight records what the user accepted, the claim transaction binds against it, the live ops become an ordinary executor call, and the injected sink goes in wrapped so each source reports its outcome back.

**Every cross-volume verb goes through the ROUTED entry points** (`start_volume_{copy, move, compress}`), the same ones the transfer commands use, so an approved transfer resolves its volumes and anchors its destination exactly as a clicked one does. Extract needs no arm of its own: its sources resolve to an `ArchiveVolume` inside that routing, which is why extract has no operation type.

**Decision: two connections, and the second is MOVED into the decorator.**
**Why**: the operation outlives the call that started it by minutes or hours, so its writer cannot borrow anything the caller owns. One connection for the operation lifetime rather than one per event, because a group may carry 60 000 sources and a connection per source would dominate the run.

**Decision: `completed` is written on SETTLE, not on `write-complete`.**
**Why**: settle fires exactly once per operation after full teardown, on every ending — success, cancel, failure, panic — and the question `ProposalStatus::Completed` answers is "is this group still in flight?". Marking only on success leaves a cancelled group `approved`, and the next launch recovery sweep calls it `interrupted`: a claim that the app died, about an operation the user deliberately stopped. The per-op statuses carry what actually happened; a cancelled group keeps `pending` rows for the ops nothing reached.

**Decision: rename has no route yet, and says so as a typed refusal.**
**Why**: `start_bulk_rename` takes rows carrying a server-owned `SourceFingerprint` per source, and a frozen `OpSnapshot` cannot produce one (see below). `ApprovalRefusal::NoRouteYet` keeps the dispatch total rather than leaving a silent fallthrough; rename gets its route when the shipped rename feature moves onto the spine (plan M6).

### The source binding is not wired yet, and the reason is precision

M2 gave every executor an optional `ExpectedSources`, so an operation approved a week ago can notice that a source changed since. The bridge passes none, because `OpSnapshot` cannot build a `SourceFingerprint`:

- `OpSnapshot` carries `{ size, mtime (seconds), inode }`; `SourceFingerprint::Local` needs `{ device, inode, size, modified_nanos }`.
- The missing `device` could be taken from the source volume root, with an assumption about firmlinks and nested mounts.
- **The precision gap cannot be papered over.** `capture_local` reads real nanosecond mtime, so a snapshot holding whole seconds turns into a `modified_nanos` with a zero sub-second part and mismatches nearly every real file. A naive conversion would make a bound group skip everything and complete having done nothing, which is worse than running unbound.

Closing it needs either a device-less, second-precision fingerprint variant (compared by flooring the live nanos) or a snapshot that carries device and nanoseconds. Both are cross-milestone decisions; until one lands, an approved group runs unbound and this is the place that says so.
