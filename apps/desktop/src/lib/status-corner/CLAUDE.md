# Status corner

The main window's word on background work. The top-right row (`StatusCorner.svelte`, mounted once by
`routes/(main)/+page.svelte`) hosts `OperationChip.svelte` (the backgrounded-operation preview, plus the mark a failure
leaves) and `$lib/indexing/IndexingStatusIndicator.svelte` (the hourglass), rendering any `children` to their left. The
chip's pick-and-measure rules are pure, in `operation-chip.ts`; `operation-failure-watch.svelte.ts` raises the failure
toast.

## Must-knows

- **The corner owns placement, its members don't.** `StatusCorner` carries `position: absolute`, the `--spacing-sm`
  offsets, and `--z-sticky`; each indicator inside is a plain inline box. A member that positions itself overlaps its
  neighbours.
- **No positioned ancestor, on purpose.** `.main-content` is static, so the offsets resolve against the initial
  containing block, where the hourglass has always sat. ❌ Don't make an ancestor `relative` to "fix" it: that moves the
  corner.
- **The row is always mounted, so it's `pointer-events: none`** with `auto` on its children (the `ToastContainer`
  pattern). An empty or gap-sized box over the pane must not eat clicks.
- **The hourglass stays last**, the most ambient member. Everything else renders left of it: inline if this module owns
  it, through `children` if not.
- **The chip is a PREVIEW, not a queue.** One operation (first running, else first paused), a verb and an 80 px bar, no
  percentage text, no "+N" affix. Detail belongs to its tooltip, completeness to the queue window. ❌ Don't reuse
  `TransferProgressReadout` here: its fixed-width cells blow past the corner.
- **Both gates are pure and live in `operation-chip.ts`** (`pickChipOperation`, `pickChipState`), so add and test one
  there, not in the markup. The bar is bytes, falling back to the file count when `bytesTotal` is 0 (a same-volume move
  moves no bytes, so a bytes bar would read 0% throughout). Instant ops are excluded by TYPED `operationType`, never a
  substring test.
- **A paused-only queue KEEPS the chip**, with a still bar (`animated={false}`) and the word "Paused". Hiding it on
  pause would re-hide the work, the bug the chip exists to fix. Tooltip and aria-label lead with that same label, ❌
  never the verb: zh's `正在拷贝` beside "Paused" contradicts itself. So every `queue.chip.tooltip` clause carries its
  own leading `·` and none may glue to the label, which is also capped at `12em` and ellipsized.
- **Render `row.etaSecondsDisplay`, never `progress.etaSeconds`.** The raw value once had one operation reading "8m 12s"
  in one window and "5m 46s" in the other.
- **The chip waits `CHIP_SETTLE_MS` before its FIRST appearance.** Work over in a blink never flashes the corner, and
  the beat closes a race with the foreground modal's claim. A handover to the next operation is immediate.
- **The failure toast NEVER auto-dismisses**, and past three they collapse into one summary. That cap is mechanical: a
  stack full of persistent toasts silently DROPS new ones, so an unbounded burst would lose the failures it reports. ❌
  Don't raise it, and keep `toastGroup: 'operation-failure'`.
- **Suppression needs BOTH foreground slots.** The progress dialog releases `getForegroundOperationId()` as it unmounts,
  and the failure row reaches the snapshot only after, so `getForegroundFailureId()` (the error dialog's handover) is
  what stops a double report. Check both, in the chip and the watch.
- **The summary toast reads its count off the store, ❌ never a prop.** The toast store's dedup path replaces content
  and level but NOT props, so a prop-carried count would freeze at the fourth failure.
- **A failure's toast title is `queue.failureToast.title`, ❌ not the pipeline's**: it names the work the user started
  ("Couldn't finish copying"); the pipeline's title names the cause and varies per error class. The explanation under it
  IS the pipeline's.

Layout model, member contract, the chip's states, and decisions: `DETAILS.md`.
