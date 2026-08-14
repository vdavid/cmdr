# Status corner

The main window's word on background work: the top-right row (`StatusCorner.svelte`, mounted once by
`routes/(main)/+page.svelte`) hosting `OperationChip.svelte` and `$lib/indexing/IndexingStatusIndicator.svelte` (the
hourglass), with any `children` to their left. The chip's pick-and-measure rules are pure, in `operation-chip.ts`;
`operation-failure-watch.svelte.ts` raises the failure toast.

## Must-knows

- **The corner owns placement, its members don't**: it carries the `position: absolute`, the offsets, and `--z-sticky`,
  and each member is a plain inline box. The hourglass stays last; everything else renders left of it.
- **No positioned ancestor, on purpose**: `.main-content` stays static, or the corner moves with it.
- **The row is always mounted, so it's `pointer-events: none`** with `auto` on its children, or an empty box over the
  pane eats clicks.
- **The chip is a PREVIEW, not a queue**: one operation (first running, else first paused), a verb and an 80 px bar, no
  percentage, no "+N". ❌ Not `TransferProgressReadout` — its fixed-width cells blow past the corner.
- **Both gates are pure and live in `operation-chip.ts`** (`pickChipOperation`, `pickChipState`), so add and test one
  there, not in the markup. The bar is bytes, falling back to the file count when `bytesTotal` is 0 (a same-volume move
  moves no bytes); instant ops are excluded by TYPED `operationType`, ❌ never a substring test.
- **A scanning operation gets a SPINNER and "Scanning…", never a bar** (both totals are 0). Its spoken label is
  `queue.chip.scanningAriaLabel`, ❌ never the tooltip's string: that one drops the verb and the "Open the operation
  queue" tail.
- **A paused-only queue KEEPS the chip**, still bar, label "Paused": hiding it would re-hide the work the chip exists to
  surface. Tooltip and aria-label lead with that label, ❌ never the verb, so every `queue.chip.tooltip` clause carries
  its own leading `·`.
- **Render the session's `etaSecondsDisplay`, ❌ never `progress.etaSeconds`**: the raw value once read "8m 12s" in one
  window and "5m 46s" in the other.
- **The FIRST appearance waits `CHIP_SETTLE_MS`** (blink-long work never flashes the corner, and the beat closes a race
  with the foreground modal's claim); a handover to the next operation is immediate.
- **The failure toast NEVER auto-dismisses**, and past three they collapse into one summary. A stack full of persistent
  toasts silently DROPS new ones, so ❌ don't raise that cap, and keep `toastGroup: 'operation-failure'`.
- **Suppression needs BOTH foreground slots**, in the chip and in the watch: the dialog releases
  `getForegroundOperationId()` as it unmounts and the failure row lands only after, so `getForegroundFailureId()` is
  what stops a double report.
- **The summary toast reads its count off the store, ❌ never a prop** (the dedup path replaces content and level, not
  props, so a prop-carried count freezes at the fourth failure).
- **A failure's toast title is `queue.failureToast.title`, ❌ not the pipeline's**: it names the work the user started,
  while the explanation under it IS the pipeline's.

Layout model, member contract, the chip's states, and decisions: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
