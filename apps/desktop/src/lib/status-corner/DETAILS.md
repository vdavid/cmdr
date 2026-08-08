# Status corner: details

Depth for `CLAUDE.md`. Up: `apps/desktop/src/CLAUDE.md`.

## What's here

- `StatusCorner.svelte`: the row. One optional `children` snippet, then `OperationChip`, then `IndexingStatusIndicator`.
- `OperationChip.svelte`: the corner progress chip (markup, copy, and the settle timer).
- `operation-chip.ts`: pure — which operation the chip shows, how full its bar is, and the destination name its tooltip
  uses.
- `StatusCorner.svelte.test.ts`: the structural contract (always mounted, children and the chip render before the
  hourglass).
- `StatusCorner.a11y.test.ts`: tier-3 axe pass, idle and populated.
- `operation-chip.test.ts`: every visibility gate and the bar's metric, as pure data.
- `OperationChip.svelte.test.ts`: the rendered chip, driven through a real operations store.
- `OperationChip.a11y.test.ts`: tier-3 axe pass, running and paused.

## Layout model

One absolutely positioned flex row, right-aligned by virtue of `right: var(--spacing-sm)` and shrink-to-fit width: the
row grows leftward as members join, so the hourglass keeps the exact pixels it had when it positioned itself. Members
are separated by `--spacing-xs`.

The row sits in the initial containing block because no ancestor between it and the viewport is positioned
(`.main-content` in `routes/(main)/+page.svelte` is static). That's inherited behavior, not an accident: the hourglass
resolved its own `top`/`right` against the same box before the corner existed, and reproducing it is what makes the
extraction visually identical.

## Member contract

A member is a plain inline box:

- ❌ no `position` / `top` / `right` / `z-index` of its own (the row supplies all four),
- ✅ its own colour, focus ring, and any animation,
- ✅ `pointer-events: auto` comes free from the row's `> :global(*)` rule; a member that wants to stay click-through
  opts out itself.

The hourglass renders last so the eye finds it in the same place regardless of what else is showing. A member this
module owns renders inline, before it; a member owned elsewhere arrives through `children`, which keeps the corner from
importing half the app.

## The operation chip

An ambient preview of the operation queue: it appears when a copy, move, delete, trash, or archive edit is running
without a modal in front of it, and clicking it opens (or raises) the queue window through the shared
`openQueueWindow()`.

### What it shows

- **Which operation**: the first `running` row, falling back to the first `paused` row. Lanes parallelize disjoint
  volumes, so several can run at once; snapshot order is the manager's FIFO order, which makes "first" stable. On a busy
  queue the chip therefore shows one third of the truth, deliberately — it's a preview, and the click-through is what
  makes that acceptable.
- **The bar**: bytes (`bytesDone / bytesTotal`), falling back to the file count when `bytesTotal` is 0, clamped to 0–1.
  The zero-bytes case is real rather than defensive: a same-volume move renames server-side and moves no bytes, so a
  bytes bar would sit at 0% for the whole operation. With neither metric it reads 0% rather than dividing by zero.
- **The label**: the verb from `queue.row.label` (the queue rows' own vocabulary), except while paused, where it becomes
  the status word "Paused" — a frozen bar under "Copying" is ambiguous, and the tooltip still leads with the verb.
- **Nothing else**: no percentage text, no "+N" affix for the operations it isn't showing. Both were considered and cut
  as noise; the queue window is the surface that promises completeness.

### When it stays quiet

Each of these is a branch of `pickChipOperation`, each with its own test:

- an empty queue;
- instant ops (`rename` / `create_folder` / `create_file`), matched on the typed `operationType` via the store's
  `isInstantOperation`, never a substring test (`no-string-matching`); they emit no progress and are gone before the eye
  lands on them;
- the operation the foreground progress modal owns (`getForegroundOperationId()`), which the modal is already showing in
  full;
- a queue that's only `queued`: something else holds the lane it's waiting on, and that row speaks for it.

The chip deliberately does NOT check whether the queue window is open. It's ambient status, not a notification, and it
stays put while the window is up.

### The settle delay

`CHIP_SETTLE_MS` (500 ms) gates the chip's FIRST appearance, so work that's over in a blink never flashes the corner
(the house loading-state rule: under about a second, no indicator). Once the chip is up, a handover to the next
operation is immediate, so two consecutive transfers don't blink the corner between them.

The same beat closes a race the frontend can't close cleanly otherwise: an operation reaches this window's store when
the backend registers it, a hair before the start command's response lets `transfer-progress-state` claim the foreground
slot. Without the delay the chip could flash for an operation the modal is about to own. The alternative would be a
second identity crossing IPC purely to claim ownership earlier, which isn't worth a sub-frame flicker.

The effect behind it depends on the candidate's ID, not on the candidate itself: a `$derived` string doesn't re-notify
while its value is unchanged, so the 200 ms progress ticks can't keep restarting the timer (which would leave the chip
hidden forever). It writes `settledId` and reads a plain, non-reactive mirror of "already showing", so it can't
re-trigger itself.

### The tooltip

`queue.chip.tooltip`, one line of middle-dot-separated facts: "Copying 214 items to Backup · 42% · 1m 20s left". The
count and the destination drop out when there's nothing to say (no progress yet, or a delete with no destination), and
the trailing detail is either the time left or the word "Paused" — a paused operation has no honest countdown.

It goes through the tooltip action's `contentEl` rather than `text` because the numbers tick while the tooltip is up: an
adopted element keeps updating in place. ⚠️ The action adopts the element it's given and an adopted element keeps its
own `hidden` attribute, so the INNER div is bound, never the `<div hidden>` wrapper. The content carries a stable
`min-width` because the action measures once on show.

The ETA is the store's SMOOTHED `row.etaSecondsDisplay`, never `progress.etaSeconds`: the queue window renders the
smoothed one, and the raw value once had the two surfaces disagreeing about the same operation.

### A11y

A real `<button>` (it does something, unlike the hourglass's `role="img"` span), so it's tab-reachable and gets the
global `button:focus-visible` ring. Its `aria-label` carries the state and the percentage plus what pressing it does;
the bar inside is wrapped in `aria-hidden`, since announcing the same percentage twice helps nobody.

## Decisions

- **Why the chip's rules are a pure module.** Every gate is a branch that has to be provable, and a component test per
  branch would pay a mount for each. `pickChipOperation` takes rows plus the foreground id and returns a small view
  model, so the component is markup and the gates are data.
- **Why a wrapper at all.** Two independently-absolute boxes in one corner have to know each other's widths to avoid
  overlapping. A flex row makes placement one concern in one place, and adding a member becomes markup rather than
  arithmetic.
- **Why the corner is always mounted, even when empty.** Mounting the row conditionally would mean the members' own
  visibility gates and the row's gate could disagree, and a mount/unmount on every indexing run is churn for nothing.
  `pointer-events: none` makes an empty row free.
- **Why `children` rather than a members array.** Members differ in props, gates, and lifetime; a snippet lets the host
  compose them without the corner learning about any of them.
