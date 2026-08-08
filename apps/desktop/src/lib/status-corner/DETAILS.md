# Status corner: details

Depth for `CLAUDE.md`. Up: `apps/desktop/src/CLAUDE.md`.

## What's here

- `StatusCorner.svelte`: the row. One optional `children` snippet, then `OperationChip`, then `IndexingStatusIndicator`.
- `OperationChip.svelte`: the corner chip (markup, copy, and the settle timer), in either of its two states.
- `operation-chip.ts`: pure — what the corner has to say (`pickChipState`), which operation it previews, how full its
  bar is, and the destination name its tooltip uses.
- `operation-failure-watch.svelte.ts`: the main window's failure notice — which failures get a toast, which are left to
  the dialog already showing them, and what a burst collapses into.
- `OperationFailedToastContent.svelte` / `OperationFailuresToastContent.svelte`: one failure's notice, and the summary a
  burst collapses into.
- `StatusCorner.svelte.test.ts`: the structural contract (always mounted, children and the chip render before the
  hourglass).
- `StatusCorner.a11y.test.ts`: tier-3 axe pass, idle and populated.
- `operation-chip.test.ts`: every visibility gate, the bar's metric, and the progress-beats-failure precedence, as pure
  data.
- `OperationChip.svelte.test.ts`: the rendered chip, driven through a real operations store.
- `OperationChip.a11y.test.ts`: tier-3 axe pass, running and paused.
- `operation-failure-watch.svelte.test.ts`: one toast per failure, no double-toast on a re-emitted snapshot, both
  suppression paths, and the collapse past three.
- `OperationFailedToastContent.svelte.test.ts` (+ both toasts' `.a11y.test.ts`): the wording, the reason, and the
  action.

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

### The failure state

`pickChipState` returns one of two things, never both: the progress preview above, or a mark that something stopped
before it was done. Live work wins — a `triangle-alert` glyph, "Couldn't finish" in `--color-warning-text`, and no bar,
shown only when nothing is running or paused.

It exists because of what happens otherwise: dismiss the toast with the queue window closed, and the main window carries
zero trace that anything went wrong, which is the exact bug this corner was built to fix. It stays deliberately narrow
(a count and a glyph, no list, no reason) so it reads as a mark, not a notification centre. That narrowness is also why
it's amber where the toast and the failed queue row are red: severity follows the THING, and a surface that names
neither the operation nor the reason only points. Clicking it opens the queue, same as the progress state. The failure
the foreground error dialog is showing is left to that dialog (`getForegroundFailureId()`).

The label reuses `queue.row.status`'s `failed` arm, so the corner and the failed row say the same two words. One string,
`queue.chip.failed`, serves as both the tooltip and the spoken label.

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

## The failure notice

`operation-failure-watch.svelte.ts` watches the main window's store for rows entering `failed` and raises a toast per
new one. It's a reaction to the snapshot the window already subscribes to: no new event, no listener, no polling.

- **Persistent, never a timer.** The person this is for was away from the keyboard when it happened. A modal was
  considered and rejected: the user pressed Queue precisely to stop being blocked, a settled failure asks for no
  decision, and a modal would steal the keystroke they were mid-way through.
- **The toast is a PREVIEW: title, the reason's explanation, and "Show in operation queue".** It leaves out the
  suggestion and clamps the explanation to three lines. The pipeline's prose was written for a dialog with room, and its
  interpolating variants (`invalid_name`, `read_only_device`) carry paths and device names with no length limit; three
  lines covers every stock message, so in practice nothing is cut. The full reason, the suggestion, and the Dismiss live
  on the queue row, one press away.
- **The toast's title is NOT the pipeline's title.** `queue.failureToast.title` selects on the operation TYPE ("Couldn't
  finish copying"), so the toast names the work the user started. The pipeline's title names the immediate cause instead
  and changes per error class ("Not enough space", "Couldn't find the file"), which reads as a different subject in an
  ambient notice. The body below it is the pipeline's, unchanged.
- **Past three, they collapse into one summary.** Mechanical, not aesthetic: a toast stack full of persistent toasts
  silently drops new arrivals (`lib/ui/CLAUDE.md`), so an unbounded burst would lose failures. The summary carries a
  dedup id and reads its count off the store rather than a prop, because the toast store's dedup path replaces content
  and level but NOT props — a prop-carried count would freeze at whatever the fourth failure saw. Reading live also
  keeps it honest as the user clears rows. `toastGroup: 'operation-failure'` (cap `MAX_FAILURE_TOASTS + 1`, pure
  backstop) keeps a burst from evicting unrelated toasts.
- **Announced ids are remembered, and forgotten when the row leaves.** A re-emitted snapshot can't double-toast, and the
  set can't grow for the life of the process. Operation ids are never reused, so forgetting one can't resurrect it.
  Suppressed failures count as announced: they were reported, just not by us, and must not get a late toast when the
  dialog that reported them closes.
- **What's live is read off the toast store, not tracked here.** The user can dismiss a toast at any moment, and the
  store is the only thing that knows; local bookkeeping would drift the first time one self-dismissed.

### Why the foreground handover needs two slots

The backend retains a failure unconditionally (it can't know a modal is up), so the frontend decides what not to
double-report. `getForegroundOperationId()` alone can't: the progress dialog releases that slot as it unmounts, and the
failure row only reaches the snapshot AFTER that (the backend emits `write-error` first and settles the record second).
By then the slot is empty and the corner would happily announce a failure the user is reading in the dialog in front of
them.

So `dialog-state.svelte.ts`'s `handleTransferError` reads the slot while the progress dialog still holds it and hands
the id to `setForegroundFailureId`. Both the chip and the toast check both slots. Closing the error dialog releases the
second slot AND calls `dismissFailedOperation(id)`, so the common case — a foreground failure the user read and closed —
leaves nothing behind in the queue.

## Decisions

- **Why the watch lives outside a component.** A toast isn't part of the status row, so tying its lifetime to
  `StatusCorner` would make the corner responsible for something it doesn't render. `$effect.root`, started and stopped
  by `routes/(main)/+page.svelte` next to `initMainWindowOperations()`, keeps that honest and keeps the pass
  (`announceFailures`) callable straight from a test.
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
