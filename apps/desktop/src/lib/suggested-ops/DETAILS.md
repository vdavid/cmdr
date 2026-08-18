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

**Two signals, and the weaker one is not redundant.** A refresh compares the open group's `liveOpCount` before and
after; the `suggestions-changed` event carries a typed `reason`. The count comparison misses an amendment that swapped a
path but kept the op count, so the event is what actually catches an amendment. Both stay.

**Decision: the notice keys on `reason`, never on `groupId` alone.** An amendment and the user's own approval carry the
SAME group id, and only one of them means "the list under you moved". Keying on the id would raise "Ask Cmdr changed
this" the instant the user approved something, which presents as a glitch rather than a bug and gets found late. A
mutation test pins it: drop the `reason` check and the approval case goes red.

## The badge, and why it is a separate module

`suggested-ops-badge.svelte.ts` is mounted for the whole session; the dialog's state only exists while a review is open.
Folding them would leave the corner reading a store nothing populates until the dialog opens, so the indicator would sit
at zero forever, which is the failure mode of an indicator that silently never updates.

It does two things that look redundant and are not:

- **Seeds once at startup**, because suggestions never expire. A group proposed last week is waiting before this session
  emits anything.
- **Subscribes**, because after that first read only being told moves it. ❌ Never poll.

A count it cannot read is logged and dropped rather than propagated: an approval that already succeeded must not fail
because a badge could not refresh.

## What isn't here yet

- **`interrupted` groups**: re-approving one mints a NEW group with a fresh preflight, which is spine machinery rather
  than dialog work, so surfacing them would mean designing the re-approval flow inside a dialog milestone. Deliberate,
  not missing.
- **The degraded-state actions**: `WakeReadiness` types consent / Full Disk Access / no-API-key as distinct states, each
  with its own action (open consent, open the FDA screen, open AI settings). The indicator renders a count today and
  should render those states with their specific action rather than a generic "unavailable".
- **The dialog-gallery preview**: the row is `not-triggerable` until fixtures exist for the interesting shapes (an
  irreversible group, a folder that will be created, a pattern-matched group, a 60,000-op group).
- **Nine of the ten locales**: English and German ship; the rest wait on David's copy review, since translating copy
  that is about to be revised is wasted work.
