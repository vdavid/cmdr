# Transfer queue window — details

Depth for [CLAUDE.md](CLAUDE.md). The window is the frontend of the transfer-queue + pause feature
([`docs/specs/2026-06-21-transfer-queue-pause-plan.md`](../../../../../../docs/specs/2026-06-21-transfer-queue-pause-plan.md)).

## Why a hard window

A copy/move/delete today shows only a modal progress dialog: it blocks the main window and there's no single place to
manage several operations at once. The queue is the "stop blocking me, let me keep working" surface, so it must be a
real OS window you can leave open in the background — not a sheet, not a panel inside the main window. It's built on the
exact Settings-window pattern (singleton, vibrancy, overlay title bar, position via `$lib/window-positioning`) so it
reads as a first-class macOS utility window, consistent with Settings and Keyboard shortcuts.

## The two-stream model

The window renders from `createOperationsStore()`, which merges:

1. **`operations-changed`** (`onOperationsChanged`): the thin registry snapshot the backend emits whenever an operation
   is registered, admitted, paused, resumed, or settles. Payload is `{ operations: OperationSnapshot[] }`, where each
   `OperationSnapshot` is `{ operationId, operationType, status, source, destination }` — membership + lifecycle status
   only, NOT 200 ms progress. This decides which rows exist and each row's status.
2. **`write-progress`** (`onWriteProgress`, the existing per-file stream): drives the live per-row progress bars and
   ETA. The store keys the latest `WriteProgressEvent` by `operationId`.

On every snapshot tick the store prunes the progress map to the new membership, so a finished op's bar can't outlive its
row, and the map can't grow unbounded. Progress for an op not (yet) in the snapshot is ignored — the snapshot is the
membership source of truth.

This split keeps `operations-changed` cheap (no progress fattening it every 200 ms) while still giving each row a live
bar — the design the plan mandates under "subscribe, don't poll" and "thin snapshot".

### Why snapshot status, not `is_running`

A paused Running op stays in `WRITE_OPERATION_STATE`, so `get_operation_status().is_running` reports `true` while paused
("running but not progressing"). The bar-is-moving truth is therefore the snapshot `status`: only `'running'` shows the
spinner and an animated bar; `'paused'` shows a static bar and the Paused label. Rows never read `is_running`.

## Store public API (the progress-dialog reuse contract)

`createOperationsStore()` returns:

- `operations: OperationRow[]` — reactive; each `OperationRow` is
  `{ snapshot: OperationSnapshot, progress: WriteProgressEvent | null }`. Ordered as the backend emits them.
- `hasRunning: boolean` — any op with `status === 'running'` (gates "Pause all").
- `hasPaused: boolean` — any op with `status === 'paused'` (gates "Resume all").
- `init(): Promise<void>` — subscribes to both streams, then seeds from `list_operations`. Subscribe-before-seed so a
  tick during the await isn't missed; the seed only applies if no snapshot tick beat it. Failures `log.warn` (perms /
  IPC), never throw.
- `dispose(): void` — drops both listeners. Call on window teardown.
- `_testApplySnapshot` / `_testApplyProgress` — test seams that drive the reducers without a live backend.

`isTerminalStatus(status)` (module export) is the typed terminal-set test (`done` / `cancelled` / `failed`), used by the
page to hide settled rows. Typed set, not a string-substring test (`no-string-matching`).

The progress-dialog Queue button and the auto-queue surfacing open the window via `openQueueWindow()` and read this same
store. Don't fork a second opener or store.

## Vibrancy + reduce-transparency

`queue-window.ts` opens transparent and applies `Effect.UnderWindowBackground` via `setEffects` after creation (the
`windowEffects` creation option drops silently in this Tauri version; `setEffects` is the reliable IPC path, gated by
`core:window:allow-set-effects`). UnderWindowBackground reads as a clean utility/HUD-style panel — the macOS convention
for a transfer/activity manager — and follows the window's active state.

Under macOS "Reduce transparency" the window opens opaque (no material, `backgroundColor` mirroring the theme) and the
page surface uses the shared `--color-bg-glass` / `--color-border-glass` tokens, which flip to opaque under
`html.reduce-transparency` (toggled from the backend `NSWorkspace` value via `$lib/reduce-transparency`, since WKWebView
doesn't reflect `prefers-reduced-transparency`). `prefers-color-scheme` IS reflected, so dark detection stays a media
query. Reduced motion is honored by the shared `ProgressBar` / `Spinner` (their shimmer/spin freeze under
`prefers-reduced-motion`).

## Capabilities

`src-tauri/capabilities/queue.json` mirrors `settings.json`'s window perms (close, set-focus, set-min/max-size,
set-effects, start-dragging, outer-position/size, scale-factor, `core:event:default`, `core:app:allow-set-app-theme`,
`core:webview:allow-internal-toggle-devtools`) but DROPS `store:default` (no persistence in v1) and `dialog:allow-ask`
(keep-partials cancel needs no confirm). The pause/resume/cancel app commands go through the `tauri_specta` invoke
handler, not the capability ACL, so they need no per-command grant. The opener's `getByLabel` + `readMonitors()` run on
the MAIN window, which already holds those perms — nothing to add there (see `docs/guides/adding-a-window.md`).

## Opening the window

- Command palette + Help menu: `queue.show` ("Show transfer queue"), handled in
  `routes/(main)/command-handlers/app-dialog-handlers.ts` → `openQueueWindow()`. Wired through the full command path
  (id, registry, handler, Rust menu mappings, both platform menu builders, the drift-test excuse).
- The progress dialog also opens/raises it automatically when an op lands on a busy lane (auto-queue surfacing).

## Tests

- `operations-store.svelte.test.ts`: the reducers (snapshot → rows, progress merge + unknown-op drop, prune on leave,
  running/paused presence) and `isTerminalStatus`.
- `QueueRow.svelte.test.ts`: per-status controls (Pause vs Resume vs queued), click wiring, the select checkbox, the
  live bar from a progress event, and the `data-status` / `data-operation-id` E2E hooks.
- `QueueRow.a11y.test.ts`: axe over the row in running / paused / queued / selected states.
- E2E: `test/e2e-playwright/transfer-queue.spec.ts` — two same-lane ops → one Running + one Queued, cancel the queued,
  pause + resume the running.
