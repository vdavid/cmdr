# Status corner: details

Depth for `CLAUDE.md`. Up: `apps/desktop/src/CLAUDE.md`.

## What's here

- `StatusCorner.svelte`: the row. One optional `children` snippet, then `OperationChip`, then `WakeIndicator`, then
  `SuggestedOpsIndicator`, then `IndexingStatusIndicator`.
- `OperationChip.svelte`: the corner chip (markup, copy, and the settle timer), in either of its two states.
- `operation-chip.ts`: pure — what the corner has to say (`pickChipState`), which operation it previews, how full its
  bar is, and the destination name its tooltip uses.
- `operation-failure-watch.svelte.ts`: the main window's failure notice — which failures get a toast, which are left to
  the dialog already showing them, and what a burst collapses into.
- `OperationFailedToastContent.svelte` / `OperationFailuresToastContent.svelte`: one failure's notice, and the summary a
  burst collapses into.
- `StatusCorner.svelte.test.ts`: the structural contract (always mounted, children and the chip render before the
  hourglass).
- `operation-chip.test.ts`: every visibility gate, the bar's metric, and the progress-beats-failure precedence, as pure
  data.
- `OperationChip.svelte.test.ts`: the rendered chip, driven through a real operations store.
- `operation-failure-watch.svelte.test.ts`: one toast per failure, no double-toast on a re-emitted snapshot, both
  suppression paths, and the collapse past three.
- `OperationFailedToastContent.svelte.test.ts`: the wording, the reason, and the action.
- `status-corner.a11y.test.ts`: one directory-level tier-3 audit, because `svelte-tests` charges per test FILE
  (`docs/testing.md` § "What a test actually costs"). Four blocks: the corner (idle and populated), the chip (running,
  paused, and scanning), and both toasts. The chip's fake timers stay inside its own block; file-wide they'd stall the
  other blocks' async renders.

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
importing half the app. The two AI members break that last rule on purpose and are NAMED imports, because David placed
them between the chip and the hourglass and `children` renders left of both — the corner owns ordering, and having it
visible in one file beats spreading it across the callers.

Among those two, the wake indicator goes on the LEFT. The row is right-aligned and shrink-to-fit, so it grows leftward:
a member that comes and goes with every wake would otherwise shove the persistent suggestions badge sideways each time
the agent had a look at something. The transient member takes the moving edge.

⚠️ **A member must open no subscription at mount.** `StatusCorner.svelte.test.ts` and `StatusCorner.a11y.test.ts` mount
the real corner with the AI members unstubbed, so a listener in a member's `onMount` breaks both. Each member reads
module `$state` that a `*.svelte.ts` populates, started from `routes/(main)/window-services.ts`. Both AI members are
also silent in their default state, which is what lets those suites mount them without any setup at all.

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
- **The label**: the verb from `queue.row.label` (the queue rows' own vocabulary), except twice. While PAUSED it becomes
  the status word "Paused" — a frozen bar under "Copying" is ambiguous. On an operation that IS a REVERSAL
  (`snapshot.reverses` set) it comes from `$lib/file-operations/reversal-wording.ts` instead, the same string the queue
  row shows, because the wire `operationType` names the syscall: undoing a copy runs as a delete, and a corner reading
  "Deleting" over an undo the person just asked for is the one thing this label must never say. It's capped at `12em` and ellipsized, because a
  localized verb runs to twice English's longest ("Wird in den Papierkorb bewegt"), and the corner must not grow across
  the pane; the tooltip carries the full text. The tooltip and the spoken label both lead with this same label.
- **Nothing else**: no percentage text, no "+N" affix for the operations it isn't showing. Both were considered and cut
  as noise; the queue window is the surface that promises completeness.

### The scanning state

An operation that is still counting reaches the corner like any other running one, but `barFraction` can only ever
return 0 for it: bytes and files both have `total == 0` through the whole scan, by design (finding the totals is what
the scan is for). A bar at 0% for minutes is not honest progress, so the chip swaps it for a `<Spinner>`, and the
tooltip becomes `fileOperations.shared.scanningTooltip` ("Scanning…") rather than `queue.chip.tooltip`, whose
`· {percentText}%` clause would be the dishonest part. The visible label stays the verb.

The SPOKEN label is its own key, `queue.chip.scanningAriaLabel` ("{label}, scanning. Open the operation queue."), not
the tooltip's string. It's `queue.chip.ariaLabel` with the percentage swapped for the state word, so the scan state
gives up exactly the dishonest part and keeps the two that matter: the visible verb, which voice control needs to press
the chip by the word a person can see on it (WCAG 2.5.3), and the closing promise naming what pressing it does. ❌ Don't
re-collapse the two labels into one key to save a string: the tooltip is read, the aria label is spoken, and they
legitimately want different sentences. The `OperationChip` block of `status-corner.a11y.test.ts` pins both halves.

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
  `isInstantOperation`, never a substring test; they emit no progress and are gone before the eye lands on them;
- the operation the foreground progress modal owns (`getForegroundOperationId()`), which the modal is already showing in
  full;
- a queue that's only `queued`: something else holds the lane it's waiting on, and that row speaks for it.

`pickChipState` applies the same instant-op and foreground exclusions to the FAILURE count, so the two states agree
about what the corner is allowed to mention: a rename that couldn't finish stays in the queue window and never marks the
corner.

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

`queue.chip.tooltip`, one line of middle-dot-separated facts: "Copying · 214 items · to Backup · 42% · 1m 20s left". The
count and the destination drop out when there's nothing to say (no progress yet, or a delete with no destination), and
so does the trailing time left while the estimate is warming up. It STAYS through a pause, like every other surface
showing that operation: the backend keeps the seconds a person spends deciding out of its rate window, so the countdown
is still what remains once they resume (`write_operations/DETAILS.md` § "ETA + throughput").

- **It leads with `chipLabel`, not the verb**, so a chip reading "Paused" can't open a line claiming the copy is running
  right now. English's aspect-free "Copying" hid that; zh's `正在拷贝` states it outright. The cost is that a paused
  tooltip no longer names the operation type, which is the queue window's job one click away.
- **Every clause carries its own leading `·`**, in all ten catalogs. The label is a whole fact, and once it can be a
  state word rather than a verb, a clause glued to it ("Paused 214 items", zh `已暂停到“Backup”`, which reads "paused
  until Backup") stops being grammatical. ⚠️ A locale that re-glues one is the bug coming back.

It goes through the tooltip action's `contentEl` rather than `text` because the numbers tick while the tooltip is up: an
adopted element keeps updating in place. ⚠️ The action adopts the element it's given and an adopted element keeps its
own `hidden` attribute, so the INNER div is bound, never the `<div hidden>` wrapper. The content carries a stable
`min-width` because the action measures once on show.

The ETA is the SMOOTHED `session.etaSecondsDisplay`, never `progress.etaSeconds`: the queue window renders that same
number for that same operation, and the raw value once had the two surfaces disagreeing. The chip binds to the session
with `bindOperationSession` (`$lib/file-operations/operation-session/CLAUDE.md`), following its own candidate, so the
smoother belongs to the operation rather than to whichever surface is watching. It's the chip's only session read: what
to show and how full the bar is stay pure, in `operation-chip.ts`.

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
