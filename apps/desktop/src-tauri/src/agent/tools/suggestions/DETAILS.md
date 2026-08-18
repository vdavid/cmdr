# Suggested-ops tools — details

Pull-tier docs for `agent/tools/suggestions/`. Must-knows: `CLAUDE.md`. The feature end to end:
`docs/specs/agent-suggested-ops-plan.md`. The rows and the lifecycle: `../../store/proposals/DETAILS.md`. Selector
resolution and the metric: `../../suggested_ops/DETAILS.md`.

## The three tools, and why the access classes fall where they do

- `list_suggestions` — `Read`. Sweeps and groups as summaries with `COUNT(*)` counts.
- `get_suggestion_group` — `Read`. One group's ops, paged and budget-cut.
- `propose_suggestions` — `Propose`. A staged sweep, or an amended pending group.

`Propose` means "stages a proposal and opens a review surface, mutates nothing"; `Write` means "mutates the filesystem
OR app state" and is never reachable from the agent view. Staging rows in `main.db` IS the proposal, which is why it
stays `Propose` — but a standalone `amend` tool would be a stored-state mutation with no review surface of its own, so
the enum's own tiebreaker makes it `Write` and `test_agent_tool_view_never_writes` would refuse it. Folding the amend
into `propose_suggestions` is what keeps the surface honest.

Only `propose_suggestions` joins `EXPECTED_PROPOSE_TOOL_NAMES` (`mcp/tests/tool_registry_tests.rs`), a hand-authored
allowlist by design: no structural check can prove a handler doesn't mutate.

## `propose_suggestions`: resolve, check, then write

`apply_planned_sweep` runs in that order, and the order is the contract:

1. **Resolve** every group's ops — a selector against the drive index, an explicit list as given.
2. **Check** every amendment's target: the group exists, it belongs to the named sweep, and it is still `pending`.
3. **Write**: create the sweep (or reuse the named one), then each group through `suggested_ops::add_group` /
   `repropose`.

A refusal anywhere in 1–2 leaves the store exactly as it was. The alternative (write what validates, report the rest)
was rejected because a sweep is what the user reads in one sitting: a partly-applied amendment shows some groups as the
agent revised them and some as it left them, with nothing on screen to say which is which.

Between the check and the write there is still a race, the user answering a group in that instant. It can't be closed
without one transaction spanning every group, and it doesn't need to be: `repropose_group` is itself conditional on
`pending`, so the loser reports `GroupOutcome::AlreadyAnswered` with `opCount: 0` and the user's answer stands. The
report names what landed, so a partial application is a stated outcome rather than a silent one.

### Why the ownership check exists at all

`repropose_group` already refuses a non-pending group, so the pre-check looks redundant. It isn't:

- It makes the refusal ATOMIC across the call. Without it, group 1 lands and group 2 refuses.
- It carries the group's actual status into the refusal (`GroupNotPending { status }`), so the model can tell "the user
  already approved this" from "there's no such group" and say the right thing.
- The sweep-membership half has no equivalent downstream at all. Without it an agent could rewrite any group by number,
  including one from another conversation's sweep.

### The refusal vocabulary

Every refusal crosses the wire as `{ readyForReview: false, refusal: <token>, group: <index>, problem: <sentence> }`.
The token is the typed classification (nothing downstream matches on the sentence, per the repo's no-error-string-match
rule); the sentence is what the model relays, and it says what to send instead. The group index is the position in the
call, because the model's only recovery is to send the whole call again and it needs to know which group to fix.

## The selector schema, and the one predicate that isn't there

The model writes predicates in units it states reliably: whole **days ago** (converted to unix seconds against `now`,
injected so a test is deterministic) and whole **bytes**. `OpSelector` stores seconds, which is what the index compares.

Two ages BAND a range: "older than 30 and newer than 90" means 30–90 days old and is legitimate; the reverse is empty
and is refused as `ImpossibleWindow`, as is an inverted size window. Proposing over an empty window costs the user a
review that can't contain anything.

**"Last opened" is not expressible**, and it is the plan's flagship example ("installers you've already opened"). The
drive index carries size, modification time, and inode; there is no access time, and `importance.db`'s visit counts are
per-FOLDER, not per-file (verified against M1's selector resolver and the importance schema, 2026-08-18). A predicate
that compiled and matched nothing would be worse than its absence: the agent would propose over it and the user would
review an empty group. So the schema omits it and the system prompt carries the positive instruction — say when the
file last CHANGED, never imply Cmdr knows what the user opened. Wiring an access-time source is its own effort.

## The two reads

**`list_suggestions`** maps rows to `GroupSummaryOut` FIRST, then cuts to the result budget, then reads only the sweeps
the surviving groups belong to (`get_sweep`, one header at a time). A long backlog therefore costs a handful of header
reads rather than one per group. Nesting is by first appearance, so the newest-first order the store returns survives.
A group whose sweep can't be read is dropped rather than shown parentless: the sweep is what dates a suggestion, and an
undated one reads as new.

Every group summary carries BOTH ids (`groupId`, `sweepId`), because an amendment needs the pair, and both counts
(`opCount` live, `excludedOpCount` for what the user deselected), since a deselected op keeps its row.

**`get_suggestion_group`** reports `truncated` when the size cut took rows off the page OR the page doesn't reach the
end of the group: both mean "there's more", and the model has to say "returned of total" either way. Its op fields are
named `snapshotSize` / `snapshotModified` because they are what the index held AT CREATION, not what the file is now; a
size relayed as current would be a claim nothing here can back. An unknown size gets no human string at all, because
`"0 B"` reads as an empty file.

An id that names no group answers `{ found: false, groupId }` rather than erroring: the group may simply be one the
user already dealt with, which is an answer, not a fault.

## The conversation link

`view.rs` routes `propose_suggestions` through `propose_in_thread` with `EvidenceScope::conversation_id()`, so a sweep
records the thread it came out of. The registry path (an external MCP client) has no thread and stores `NULL`, which is
what a background wake will store in M5 too. The column is `ON DELETE SET NULL`, so tidying a chat thread away later
leaves the decision record whole.

## Caps, and what they're for

`MAX_GROUPS` (16) is a sitting's worth of review. `MAX_PATHS` (200) matches `image_facts`' cap, and past it the answer
is a selector: a list the model can't hold is a list it starts inventing. Both are INPUT caps. What a selector resolves
to has no cap at all, because 60,000 ops in one group is a legitimate group.

## Testing

`tests/` runs without a Tauri app: the handlers are thin shells over `apply_planned_sweep` (a `Connection` plus a
`SelectorIndex`), `shape_list`, and `shape_group`. One file per concern (`input`, `propose`, `read`), with the shared
fixtures in `tests/mod.rs`. The fake index counts how often it was asked, which is what pins
"resolved exactly once". The write-path tests run against a real migrated in-memory `main.db`, so the store's own
guards are in the loop rather than mocked away: removing the pre-check's status guard turns an amendment into
`AlreadyAnswered` instead of a rewrite, which is how the two layers were confirmed to be independent.
