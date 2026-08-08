# Status corner

The main window's top-right ambient-status row: `StatusCorner.svelte`, mounted once by `routes/(main)/+page.svelte`. It
hosts `OperationChip.svelte` (the backgrounded-operation preview) and `$lib/indexing/IndexingStatusIndicator.svelte`
(the hourglass), and renders any `children` to their left. The chip's pick-and-measure rules are pure, in
`operation-chip.ts`.

## Must-knows

- **The corner owns placement, its members don't.** `StatusCorner` carries `position: absolute`, the `--spacing-sm`
  offsets, and `--z-sticky`; each indicator inside is a plain inline box. A member that positions itself would overlap
  its neighbours instead of sitting beside them.
- **No positioned ancestor, on purpose.** `.main-content` is static, so the corner's offsets resolve against the initial
  containing block, which is where the hourglass has always sat. ❌ Don't add `position: relative` to an ancestor to
  "fix" it: that moves the corner.
- **The row is always mounted, so it's `pointer-events: none`** with `auto` on its children (the `ToastContainer`
  pattern). An empty or gap-sized box over the pane must not eat clicks.
- **The hourglass stays last.** It's the most ambient member, and everything else renders left of it: a member this
  module owns can render inline, one owned elsewhere comes in through `children`.
- **The chip is a PREVIEW, not a queue.** One operation (the first running one, else the first paused one), a verb and
  an 80 px bar, no percentage text and no "+N" affix. Detail belongs to its tooltip; completeness belongs to the queue
  window, one click away. ❌ Don't reuse `TransferProgressReadout` here: its cells are fixed-width and blow past the
  corner.
- **Every gate lives in `pickChipOperation`**, so add one there and test it there, not in the markup. The bar is bytes,
  falling back to the file count when `bytesTotal` is 0 (a same-volume move moves no bytes, and a bytes bar would read
  0% start to finish). Instant ops are excluded by TYPED `operationType`, never a substring test.
- **A paused-only queue KEEPS the chip**, with a still bar (`animated={false}`) and the word "Paused". Hiding it on
  pause would re-hide the work, which is the bug the chip exists to fix.
- **Render `row.etaSecondsDisplay`, never `progress.etaSeconds`.** The raw value once had one operation reading "8m 12s"
  in one window and "5m 46s" in the other.
- **The chip waits `CHIP_SETTLE_MS` before its FIRST appearance.** Work over in a blink never flashes the corner, and
  the beat closes a race with the foreground modal's claim. A handover to the next operation is immediate.

Layout model, member contract, the chip's states, and decisions: `DETAILS.md`.
