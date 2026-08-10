# Quit prompt: the frontend half

Must-knows and the module map: `CLAUDE.md`. The design, the phase machine, the two clocks, and the teardown ordering all
live on the backend side: `apps/desktop/src-tauri/src/quit/DETAILS.md`.

## Why the store is a class singleton, not a component `$state`

The prompt has to survive whatever the main window is doing, including a dialog stack it knows nothing about. A module
singleton lets `+layout.svelte` mount it as the LAST thing in the markup with a one-line `{#if}`, so the dialog isn't
nested inside the explorer subtree that owns every other dialog. The store also outlives a single component's lifetime,
which matters because the listener is registered before `settingsReady` gates the subtree in.

## Why the countdown is computed, not decremented

`show()` stores `Date.now() + countdownMs` and every tick recomputes `ceil((deadline - now) / 1000)`. A decrementing
counter drifts by exactly the ticks the webview missed, and a webview that misses ticks is the specific situation this
whole feature is built around: the number on screen would claim more time than the backend is actually giving. Ticking
at 250 ms (rather than 1 s) means the visible digit changes within a frame or two of the real second boundary without a
per-frame loop.

## Rendering choices

- **The operation list caps at ~4 rows and scrolls.** A batch session can have a dozen operations, and the countdown
  line underneath must stay on screen no matter how many.
- **Rows reuse the queue window's vocabulary**: `queue.row.label` for the verb, `operationTypeIcon` for the glyph, and
  basenames with the full path in a tooltip, exactly as `QueueRow.svelte` does. Two surfaces describing the same
  operation must not disagree.
- **`aria-live="polite"` on the countdown, not `assertive`.** A screen reader shouldn't interrupt its user once a
  second; the label names what the number measures, mirroring `fileExplorer.smbReconnect.progressBarAriaLabel`.

## What the gallery row can and can't show

The gallery renders the dialog with fixed props, so its countdown doesn't move and its buttons close the preview rather
than quitting. That's the honest arrangement (the component takes both answers as props and never calls IPC itself), and
it's why the last-second wording gets a state of its own instead of a 15-second wait.
