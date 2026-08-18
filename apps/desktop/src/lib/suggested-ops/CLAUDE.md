# Suggested ops (`lib/suggested-ops/`)

The review surface for what Ask Cmdr proposed: a soft dialog listing pending sweeps and the groups inside them, with
per-group approve and reject and per-op deselection. The backend it reads is
`apps/desktop/src-tauri/src/commands/agent/suggested_ops.rs` over the proposal spine
(`apps/desktop/src-tauri/src/agent/store/proposals/CLAUDE.md`). Depth: `DETAILS.md`.

## Module map

- `suggested-ops-trigger.svelte.ts` — the dialog's state: the sweep list, the op WINDOW, deselection, approve, reject.
- `SuggestedOpsDialog.svelte` — the dialog. Reads state, renders disclosure, owns the virtual list.
- `suggested-ops-badge.svelte.ts` / `SuggestedOpsIndicator.svelte` — the status-corner count and its subscription.

## Must-knows

- **The layout is the argument.** Each group shows the agent's reason under a label naming it as the agent's words,
  beside facts Cmdr holds by itself. ❌ Never render a rationale unlabelled: an unmarked reason reads as something Cmdr
  checked, and a hallucinated claim then arrives looking like a finding.
- **Every per-op number is the CREATION SNAPSHOT**, what the index held when the group froze, and the column says so. A
  file the index knew nothing about says that in words. ❌ Never show a zero for an absent size: it reads as an empty
  file, and a size relayed as current is a claim nothing here can back.
- **Reversibility and "the folder will be created" are DISCLOSED, never blocking.** Once the user approves it is exactly
  as if they started the action, so an irreversible group is marked and still offered.
- **Ops load a WINDOW at a time** (`OP_WINDOW_SIZE`), sized by `COUNT(*)`, over the shared `calculateVirtualWindow`. A
  group of 60,000 is legitimate, so ❌ nothing loads a group to show it, and ❌ nothing writes new scroll math here.
- **Deselection is a set of op ids and there is deliberately no "deselect all".** Approving sends the ids the user
  turned OFF, so "all of them" is the empty set and the wire stays small at any size; a deselect-all would have to
  enumerate 60,000 ids to say what Reject already says.
- **A group that changed under an open review is ANNOUNCED, never swapped.** `changedUnderReview` raises a notice and
  the rows stay put: re-ordering a list somebody is halfway through deciding on is how a wrong row gets approved.
- **The badge is its own module, subscribed and never polled.** It's mounted all session while the dialog's state only
  exists during a review, so folding them would leave the corner reading a store nothing fills until the dialog opens.
  It seeds once at startup AND listens: suggestions never expire, so one proposed last week is waiting before any event
  fires. ❌ Don't drop the seed as redundant.
- **A change notice keys on `reason`, not on the group id alone.** An approval and an amendment carry the same
  `groupId`; only `amended` means "the thing you're reading moved". Keying on the id alone raises the notice on the
  user's own approval.
