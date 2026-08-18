# Rename proposals

`rename/` stages Ask Cmdr's rename plans as ONE group on the durable proposal spine
(`agent/store/proposals/CLAUDE.md`, the canonical home for how sweeps, groups, ops, and the claim transaction work). It
validates only cached pane and index state: never probe a live mount, follow a symlink, or rename anything here. The
agent can propose; only the frontend can approve.

Four files, one concern each: `plan.rs` is the tool boundary (schema, dispatch, scope, validation, the evidence check),
`store.rs` is what a staged proposal IS and where it lives, `preflight.rs` is user-action-time revalidation (blocks,
warnings, fingerprints), `revise.rs` replaces one row's name with the user's own. All of it is re-exported from
`rename/mod.rs`, so callers keep saying `propose::rename::X`.

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
