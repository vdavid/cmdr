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

**Decision: the execution binding is a LIVE capture at preflight, never the stored snapshot.**
**Why**: the two answer different questions, and only one of them is about a race.

- The **creation snapshot** on `proposal_ops` came from the drive index when the agent proposed the group. It answers "has this changed since the agent looked at it?" — a stale BELIEF, surfaced in the review so the user can re-judge. Second precision is plenty: it is noticing a file that moved on days ago, and it is nullable because the index does not always hold all three fields.
- The **preflight fingerprint** answers "has this changed since I showed it to you?" That is a RACE. The window is the review plus however long the operation then waits for its lane, which with `LANE_BUDGET = 1` can be well past an hour, and catching it needs the full nanosecond mtime a live `stat` gives.

Converting the first into the second cannot work: whole seconds turn into a `modified_nanos` with a zero sub-second part, so it would mismatch nearly every real file and a bound group would skip everything. `capture_expected_sources` stats live instead, picking the namespace by `local_path()` exactly as the binding documents, and the fingerprints never reach the database — a restart must force a fresh preflight rather than resurrect one. This is the same rule, for the same reason, that `agent/tools/propose/rename/` already keeps its `AcceptedRenamePreflights` in memory.

A source that cannot be read at preflight simply gets no entry, and the binding drops what it does not name, so it is skipped and reported rather than acted on.

**Rename routes like every other verb.** Its executor is the one that takes per-row destinations and a fingerprint per row, and the live capture supplies exactly that: `rename_rows` rejoins the group's shared parent with each op's stored NAME (the executor refuses a row whose parents differ) and reads the fingerprint out of the binding rather than stat-ing the same files twice.

**Compress is the one verb that runs unbound.** It routes to the archive-changeset driver, which plans from its own walk and has nowhere to apply a per-source binding. The exposure is small and one-directional: compress READS its sources into a new archive rather than moving or deleting them, so a source that changed in the window is copied in its newer form rather than lost.

**What would make that stop being true**, so a future reader can spot it rather than re-deriving it: the exposure is one-directional only while the driver's effect on a SOURCE is a read. Two changes would break it, and both are plausible.

- **A compress that removes its sources** (an "archive and clean up" verb, the shape `route_archive_move_out` already has on the other side of the boundary). Then an unbound compress deletes a file that changed after the user reviewed it, which is precisely the loss the binding exists to prevent.
- **A compress that updates an EXISTING archive in place** rather than seeding a new one. `GroupIntent::Compress` already carries `overwrites_existing`, and an overwrite is `Reversibility::Irreversible` because the prior bytes are not retained; an unbound one would destroy an archive the user reviewed on the strength of what it contained.

Either change makes the binding load-bearing for this verb, and the cheapest honest fix is the same one the transfer routes took: teach the archive driver a per-source pre-flight, or refuse a bound compress the way `route_cannot_hold_a_binding` refuses the other changeset routes. ❌ Don't add either capability while compress still passes no `ExpectedSources`.

## Saying that the pending set moved

`changed.rs`. The status-corner indicator is always mounted and renders a live pending count, and the review dialog needs to know when the agent amends a group somebody has open. Both are subscribe-shaped, and without an event both would poll `main.db` on a timer — against subscribe-don't-poll and principle 5, for a store that changes a handful of times a day.

`SuggestionsChanged` carries the two counts (so the indicator draws without a follow-up query), the group id when the change was about one group, and a typed `SuggestionChange` reason.

**Decision: the reason travels, rather than being inferred.**
**Why**: the count alone cannot separate "the agent amended the group you have open" from "you just approved that group". Same id, and the dialog's recovery differs completely — one wants the non-destructive "this changed" affordance, the other means the review is over. Without the reason a listener would have to re-query the group's status and guess from the transition.

**Decision: counts come from `COUNT(*)` in one statement (`count_pending`), never from loaded rows.**
**Why**: a group of 60 000 ops is legitimate, and an always-mounted badge must not cost 60 000 rows to draw. Only PENDING groups and their LIVE ops count, so an answered group drops off the badge and a deselected op stops inflating it.

**Decision: an amend announces but does not emit analytics.**
**Why**: two different questions. The acceptance-rate metric counts DECISIONS, and counting a re-propose as a second proposal would inflate its denominator and make an agent that revises its own suggestions look worse than one that doesn't. The event refreshes a VIEW, and a view that missed an amend would show the user stale rows.

❌ No path, file name, rationale, or selector pattern rides on the event. It crosses to every window, and `main.db` is a map of the user's life that stays local.

The emitter is wired once at startup (`init_suggestions_event_emitter`, beside the operation manager's). Before wiring — every unit test — emitting is a silent no-op, and a count that can't be read is logged and dropped rather than propagated: failing an approval that already succeeded because a badge couldn't refresh would be the tail wagging the dog.
