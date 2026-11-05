# Rename proposals

`rename/` stages Ask Cmdr's rename plans as ONE group on the durable proposal spine
(`agent/store/proposals/CLAUDE.md`, the canonical home for how sweeps, groups, ops, and the claim transaction work).
The agent can propose; only the frontend can approve. Nothing here renames anything.

**A plan names one folder on ANY connected volume; no pane has to show it, and a selection doesn't narrow it.** The
folder is placed by `crate::location`'s resolver (a miss refuses in the read tools' miss shape). The user's review plus
preflight's and apply's live re-checks are the safety; the chat defines intent.

**The plan's one live touch is ONE bounded stat per source** (`sources.rs`, through `suggested_ops::facts::stat_sources`
under one shared deadline and a PER-VOLUME in-flight cap), on every volume alike. Missing → a "(doesn't exist)" row, not
a refusal (unless no row is a file); folder or unanswered → that row refuses. ❌ Never an unbounded stat, a bare
`std::fs` call, or a cap shared across volumes here: the deadline plus the per-volume cap IS the dead-mount guarantee.
The same stat freezes the row's snapshot.

**Every refusal about rows names them, from every check, in ONE answer** (`rows.rs`: typed `RowProblem` per source
path; `plan.rs::Rejected` adds evidence and the stat's findings), so the agent can fix them all and propose again. ❌
Never return on the first bad row or the first stage.

Files: `plan.rs` (schema, dispatch, placement, the evidence check), `rows.rs`, `sources.rs`, `store.rs` (what a staged
proposal IS), `preflight.rs` (user-action-time revalidation), `check.rs` (keeps it honest on a slow volume; ❌ never a
timeout), `revise.rs` (the user's own name for one row), `cut_listing.rs` (analytics only). All re-exported from
`rename/mod.rs`.

**A proposal has no expiry, but an ACCEPTED preflight lasts only as long as the process.** The rows are durable, so a
review can wait two weeks. The fingerprints apply rechecks live in `AcceptedRenamePreflights` in memory, because they
describe files as they were at review time: a restart must force a fresh preflight, never resurrect an approval given
before the app died. Don't persist them.

**Every row must carry verifiable evidence** (`evidence/`): a row claiming `imageText` / `imageTags` is refused unless
`ImageFactsLedger` has a live delivery **for that path in that chat thread** AND `detail` quotes it. One unbacked row
refuses the WHOLE plan; nothing is staged. The ledger fails closed, so don't add a path that trusts a claim when the
ledger is missing or empty, and don't widen `EvidenceScope`: evidence never crosses threads. `EvidenceSource::UserEdited`
is the dialog's word for a name the USER typed — only revise sets it, and a plan claiming it is refused. Evidence is
stored beside the ops (`proposal_rename_evidence`); a group missing any is unreviewable, not partly believable.

**A revise is the one thing that mutates a staged row, and it must invalidate the accepted preflight** (invariant 10):
apply skips its re-check when the allowed row ids match, and every name-level check (duplicates, cycles, case-only,
target-exists) lives in preflight. Two guards, keep both: `revise_row` drops the fingerprints, and the edited name
changes the spine's binding digest so the claim refuses. Don't route an edit back through the plan boundary (its
whole-plan evidence rule would refuse 50 rows over one edit). Rationale: `DETAILS.md` § Revising one row.
