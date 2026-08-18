# Suggested ops (`lib/suggested-ops/`)

The review surface for what Ask Cmdr proposed: a soft dialog listing pending sweeps and the groups inside them, with
per-group approve and reject and per-op deselection. The backend it reads is
`apps/desktop/src-tauri/src/commands/agent/suggested_ops.rs` over the proposal spine
(`apps/desktop/src-tauri/src/agent/store/proposals/CLAUDE.md`). Depth: `DETAILS.md`.

## Module map

- `suggested-ops-trigger.svelte.ts` — the state: the sweep list, the op WINDOW, deselection, and the reject path.
- `SuggestedOpsDialog.svelte` — the dialog. Reads state, renders disclosure, owns the virtual list.

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
- **Approve is not wired yet** (`APPROVE_WIRED`), because claiming a group belongs to the M4a bridge. The button is not
  rendered at all rather than rendered inert: a control that silently does nothing passes review unnoticed.
