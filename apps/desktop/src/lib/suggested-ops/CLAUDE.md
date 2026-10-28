# AI suggestions (`lib/suggested-ops/`)

The review surface for what Ask Cmdr proposed (titled "AI suggestions"; code says "suggested ops"): a soft dialog
listing pending sweeps and the groups inside them, every group's files shown at once, with per-group approve and reject
and per-op selection. Backend: `apps/desktop/src-tauri/src/commands/agent/suggested_ops.rs` over the proposal spine
(`apps/desktop/src-tauri/src/agent/store/proposals/CLAUDE.md`). Depth: `DETAILS.md`.

## Module map

- `suggested-ops-trigger.svelte.ts`: the state: sweeps, one `GroupReview` per group (window, shown rows, selection, live
  facts, change notice), approve, reject. `suggested-ops-selection.svelte.ts`: a selection's flips and wire.
- `SuggestedOpsDialog.svelte` → `SuggestedOpsGroup.svelte` (one `SectionCard` per group; `SizedLabel` evens its button)
  → `SuggestedOpFiles.svelte` (the `ColumnList`, Show more / Show all), or for a server card
  `SuggestedServerFacts.svelte` (what the click saves, no files). `AgentReason`: the labelled AI words; `VolumePill`: a
  volume with the picker's icon.
- `suggested-ops-copy.ts`: pure verb → button / undo-reason keys, a row's live-vs-snapshot facts, a sweep's age.
- `suggested-ops-badge.svelte.ts` / `SuggestedOpsIndicator.svelte`: the status-corner count and its subscription.
- `suggested-ops-failure.ts` / `suggested-ops-refusal.ts`: the typed failure, and a refused approval's reason.

## Must-knows

- **The layout is the argument.** The AI's reason renders ONLY through `AgentReason`, under a label naming it as the
  AI's words, beside facts Cmdr holds itself (folders, destination, sizes). ❌ Never render a rationale unlabelled: it
  reads as something Cmdr checked.
- **Size and Last modified are LIVE**, read for drawn rows only, sized exactly as the panes size them. A value that
  moved shows a warning glyph with both; a gone file says so; an unchecked one falls back to the snapshot WITH a label.
  ❌ Never a zero for an absent value, ❌ never an unlabelled snapshot.
- **Reversibility and "the folder will be created" are DISCLOSED, never blocking.** The undo note under the list says
  WHY, by verb (`reversibilityKey`). A group that runs as something else (`runsAs`: a trash on a drive with no Trash is
  a permanent delete) names THAT action everywhere (`shownVerb`), and approve sends the `runsAs` it showed back.
- **Each group loads a WINDOW at a time** (`OP_WINDOW_SIZE`), sized by `COUNT(*)`, through `ColumnList`'s windowed
  source. A group of 60,000 is legitimate: ❌ nothing loads a group to show it, "Show all" included.
- **A gone file is never ticked** (`review.missing`), counted, or sent on.
- **A row never loses its op while it tears down.** Ark reads `checked` on the way out, so `SuggestedOpFiles` keeps the
  drawn ops for a group whose review is gone (`drawnOps`). Regression anchor: `SuggestedOpsDialog.rows-vanish.test.ts`.
- **Selection is the user's hand flips over a base** (`OpSelection`): the stored ticks (`fromStored`, where a review
  opens, so an earlier "no" stays off), or all / none after the header box. ❌ Never enumerate ops the user didn't flip:
  "none" of 60,000 would send them all.
- **A group that changed under the review is ANNOUNCED, never swapped**, Approve waits for its reload (`canApprove`) and
  sends the `revision` the user read, and the reload keeps the selection: dropping it would quietly turn a "no" back
  into a "yes".
- **A change notice keys on `reason`, ❌ never the group id alone** (an approval carries the same id).
- **The badge is its own module, seeded once AND subscribed, never polled.** ❌ Don't drop the seed: suggestions never
  expire.
- **A refused approval gives the group back with its reason under it.** ❌ Never word an engine refusal with new copy.
- **A server card takes no input and pages nothing.** ❌ No field that types, least of all a password; no `GroupReview`
  window (`ensureOpWindow` skips it). Its refusals read through the servers sheet's own sentences.

Flows, decisions, and the gallery preview: `DETAILS.md`.
