# Suggested ops details

Pull-tier docs for `lib/suggested-ops/`. Must-knows live in `CLAUDE.md`. The feature end to end:
`docs/specs/agent-suggested-ops-plan.md`.

## Why the disclosure is shaped this way

We do not trust the agent. A suggestion can be formally valid and factually hallucinated, and nothing in the app can
tell which, so the dialog's job is not to vet the suggestion; it is to lay it out for the user to decide. Everything
here follows from that.

**The agent's words and Cmdr's facts sit side by side, each labelled.** "Ask Cmdr's reason" heads the rationale; "What
Cmdr knows" heads the size and date columns. The juxtaposition is the mechanism: a user can check a claim against
something the agent could not invent. An unlabelled rationale would read as a finding, which is exactly the failure this
surface exists to prevent.

**The per-op numbers are the frozen creation snapshot.** `agent/tools/suggestions/group.rs` made the same call for the
agent's own read, and the reasoning carries: what the index held when the group froze is checkable, while a size relayed
as current would be a claim nothing in this dialog can back. The column heading says "Size when suggested" for that
reason, and a row the index knew nothing about says "Not in the index" rather than rendering a zero, which would read as
an empty file and a 1970 date.

**Reversibility is disclosed, never enforced.** `Reversibility` is a group column, and the marker renders whatever it
says, including "This can't be undone" on a permanent delete. Refusing the group there would be an agent-specific safety
behaviour on the execution path, which the guiding principle rules out: an approved op is a user-started op.

## Scale: the window, and what the wire carries

A group of 60,000 ops is legitimate, so two things must not grow with it.

**The list.** `COUNT(*)` gives the total, the scrollbar sizes itself from that, and only the rows the viewport reaches
are fetched (`OP_WINDOW_SIZE`, centred on the request so scrolling either way stays inside the window). The scroll math
is the shared `calculateVirtualWindow` from `file-explorer/views/virtual-scroll.ts`: it is tested, and this dialog has
no business having its own opinion about scrolling. A row whose window isn't loaded renders a placeholder and `opAt`
answers `null` rather than a neighbouring row.

**The wire.** Approving sends the group id plus the ids the user turned OFF. So the common case (approve everything)
carries an empty list at any group size.

**Decision: there is no "deselect all", and that is not an omission.** Turning every row off IS rejecting the group,
which the Reject button already does in one call. A deselect-all button would have to enumerate 60,000 op ids across IPC
to express the same decision, which is the thing the deselected-ids format exists to avoid. The affordance exists under
the honest name.

## A group that changes under an open review

The agent may re-propose a pending group while the user is reading it. `refreshSuggestions` compares the open group's
live op count against what it held and, on a difference, sets `changedUnderReview` and leaves the rows exactly as they
are. The notice offers the reload; the user takes it when ready.

❌ Never swap the rows silently. The user is midway through a decision keyed to row positions, and a list that re-orders
under the cursor is how a row nobody chose gets approved.

**Not yet subscribed.** Nothing emits an event when the pending set changes, so this fires on the refreshes the dialog
already does rather than the moment the agent amends. The event lands with the M4a bridge (it needs the approve path,
which is why it belongs there), and both this affordance and the status-corner indicator subscribe to it then.

## What isn't here yet

- **Approve** (`APPROVE_WIRED`): claiming a group and handing it to the queue is the M4a bridge. The button is not
  rendered rather than rendered inert.
- **The status-corner indicator**: waiting on the same event, because an indicator that never updates is worse than one
  that isn't there.
- **`interrupted` groups**: re-approving one mints a NEW group with a fresh preflight, which is spine machinery rather
  than dialog work. The dialog lists `pending` only for now.
- **The dialog-gallery preview**: the row is `not-triggerable` until approving is real, so David reviews the finished
  surface rather than a partial one.
