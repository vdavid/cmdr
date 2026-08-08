# Operation queue: make background work (and its failures) visible

**Status**: specced, not started. **Owner**: David. **Date**: 2026-08-08.

Backgrounded file operations are invisible. Start a copy, press Queue (F2), close the queue window, and the transfer
keeps running with nothing in the main window saying so. If it then FAILS, the reason is gone too: the error dialog
belonged to the progress modal that Queue unmounted, and the queue window drops the row. Work vanishes silently, twice.

Four parts, in this order:

- **A**: move `queue.show` from Help to View, give it ⌥⌘Q.
- **B**: rename "Transfer queue" to "Operation queue" in user-facing copy (no code identifiers).
- **C**: a corner progress chip in the main window, left of the indexing hourglass.
- **D**: a failure surface, so a backgrounded operation that fails says why.

Read before starting: `apps/desktop/src/lib/file-operations/queue/CLAUDE.md` + its `DETAILS.md`,
`apps/desktop/src/lib/file-operations/CLAUDE.md` (the `getMessage()` error pipeline),
`apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`, `docs/guides/i18n-translation.md`, and
`docs/testing.md`.

## What the code says that the brief doesn't

Findings from reading the code, before any of this was written. Two of them change what part D has to be.

**F1. Terminal statuses never reach the frontend, so the `isTerminalStatus` filter is not why failures disappear.**
`LifecycleStatus::Done` / `Cancelled` / `Failed` are declared in `manager.rs:86` and assigned NOWHERE. `on_settled`
calls `free_and_remove`, which deletes the record outright, and only then emits `operations-changed`. So every snapshot
that ever crosses IPC carries `queued` / `running` / `paused` and nothing else, and `routes/queue/+page.svelte:46`'s
`!isTerminalStatus(...)` is dead defensive code. `src-tauri/src/mcp/terminal_ops.rs` states this in its module doc and
exists precisely because of it. **Consequence**: part D cannot be built by relaxing a frontend filter. The backend has
to retain the failure, or there is nothing to render.

**F2. Nothing outside the backend can hold the failure for both windows.** An operation can fail while the queue window
is closed, and the queue window is a separate webview: a store in the main window cannot be read by it, and a store in
the queue window dies with it. The manager registry is already the shared membership source of truth that both windows
subscribe to. Retention belongs there. See "Part D design" for the alternatives considered.

**F3. `write-error` fires for things that are not failures.** `WriteOperationError::Cancelled` reaches `emit_error` on
some volume paths (the `write_operations/CLAUDE.md` guardrail "Volume-aware ops must not emit `write-error` on
`Cancelled`" exists because they did), and `archive_needs_password` is a deliberate recoverable prompt intercepted
upstream by `handleTransferError`. Both must be excluded from retention by TYPED variant match, never by message text
(`no-string-matching`).

**F4. `write-error` can fire twice for one operation.** An inner handler emits (for example `transfer/copy/mod.rs:571`)
and returns `Err`, and `mod.rs:317`'s safety net emits again for the same op. Retention is therefore
**first-write-wins** per operation id: the first error is the one that stopped it.

**F5. The frontend has no way to know the foreground dialog's operation id.** `transfer-progress-state.svelte.ts`'s
returned API (line 1168) exposes no `operationId`, and `pane/dialog-state.svelte.ts` never learns it either (it holds
props, not the spawned id). Part C's "hidden while the foreground dialog owns that op" gate and part D's "don't
double-report a foreground failure" both need it. See M6 for the seam.

**F6. `ProgressBar`'s shimmer does not respect `prefers-reduced-motion`.** `lib/ui/ProgressBar.svelte:54-75` animates
`.fill::after` unconditionally; the only global reduced-motion rule (`app-utilities.css:92`) covers `.spinner`. The
queue window's `DETAILS.md` § "Vibrancy + reduce-transparency" claims otherwise. It's a real (small) bug against
principle 1, and the chip needs a static bar for the paused state anyway, so M7 fixes both together.

**F7. The OS window title is a separate string from the catalog.** `queue-window.ts:101` hardcodes
`decorateChildWindowTitle('Transfer queue')`. Renaming `queue.windowTitle` alone leaves the macOS window title reading
"Transfer queue".

**F8. `queue.row.status` already has a `failed` arm** ("Couldn't finish"), written when the window was built and never
reachable because of F1. Part D makes it reachable; do not invent a second status word.

**F9. `emit_error` fires while the record is still live.** It runs inside the op's own task, before `on_settled` removes
the record. A naive "append the failure row on error" would put the same `operationId` in the snapshot twice (live
`running` row plus failure row), and `{#each rows (row.snapshot.operationId)}` throws on duplicate keys. M8 handles this
explicitly.

## Settled decisions (don't relitigate)

These are David's, already decided.

1. **Menu label is "Operation queue", no ellipsis**, in View immediately after "Command palette…". Removed from Help on
   both platforms.
2. **Default shortcut ⌥⌘Q**, wired exactly the way `log.operationLog` does ⌥⌘L. If a real conflict turns up, **STOP and
   report** — do not substitute another key.
3. **The rename is user-facing copy only.** ❌ Do NOT rename `operations-store`, `OperationSnapshot`,
   `operations-changed`, `openQueueWindow`, `queue.show`, `QUEUE_SHOW_ID`, the `/queue` route, or the `queue.*` message
   namespace.
4. **The label uses the category noun; body copy stays concrete.** The title becomes "Operation queue", but the empty
   state keeps "Copies, moves, and deletes show up here while they run". Don't abstract concrete prose into
   "operations".
5. **The chip reuses `createOperationsStore()`** in the main window. ❌ No new backend event for part C, no new IPC for
   part C, no polling, no second store, no second opener. Both streams are already `payload.emit(app)`, app-wide.
6. **❌ Don't reuse `TransferProgressReadout`** in the chip: its cells are fixed-width and blow past 80 px. Verb label
   plus a ~80 px `ProgressBar`, no percentage text, **no "+N" overflow affix** (David cut it as noise).
7. **Show the FIRST running operation** when lanes parallelize several.
8. **The bar is bytes, falling back to the count bar when `bytesTotal` is 0** (a same-volume move moves zero bytes).
9. **A paused-only queue KEEPS the chip visible**, static bar, "Paused". Hiding it on pause would re-hide the work,
   which is the exact bug being fixed.
10. **The chip stays visible while the queue window is open.** It's ambient status, not a notification.
11. **Instant ops (`rename`, `create_folder`, `create_file`) are excluded by TYPED `operationType`**, never by string
    matching.
12. **Cancelled and done operations keep settling away silently.** Only failures get a new surface.

### Out of scope, deliberately

❌ `CHANGELOG.md`, ❌ `apps/website/public/latest.json` release notes, ❌ `apps/website/src/lib/roadmap.ts`. Those are
shipped history and stay as written. Do not "fix" the old wording in them.

## Why "Operation queue"

Record this rationale in `queue/DETAILS.md` (M11); it's the durable part of part B, and the only part of it that
survives this folder's periodic wipe.

- The window lists deletes, trashes, renames, folder and file creates, and archive edits, not only transfers. Calling it
  a transfer queue was wrong on the facts.
- "Transfer" already means copy-or-move in the code (`transfer/`, the transfer driver, `TransferProgressReadout`). The
  old name made one word mean two different things at two different altitudes, which is how a reader ends up wondering
  whether a delete belongs in the "transfer" queue.
- It now pairs with "Operation log" as present tense versus past tense, sitting next to each other in the same View
  block.

## Part D design: failures must not vanish

David's requirement: **the user must see the error message when a backgrounded operation fails.** Hard constraints: the
actual reason (not "something didn't finish"), the existing `transfer-error-messages.ts` / `errors.write.*` pipeline
(`getMessage()` raw lookup, per-operation variant keys), and it must survive the queue window being closed at the moment
of failure.

### The shape

Three surfaces, one source of truth.

1. **The backend retains the failure.** A bounded list of failed operations in the operation manager, carried on the
   existing `operations-changed` snapshot with the existing `LifecycleStatus::Failed` and a new typed
   `error: Option<WriteOperationError>` field. It is the ONLY new IPC data.
2. **The queue window keeps the failed row**, in place, with its reason rendered by the existing pipeline, and a Dismiss
   button. This is the durable surface: it survives toast dismissal, window close, and reopen.
3. **The main window raises a persistent toast** naming the reason, with a button that opens the queue window. Plus the
   corner chip carries a failure state so there's still an ambient trace after the toast is dismissed.

### Why backend retention, and not the alternatives

- **A store in the queue window** (listen to `write-error` there): fails the hard constraint outright. The window is
  closed at the moment of failure in the exact scenario this is for.
- **A store in the main window** (always alive, so it catches every `write-error`): catches the failure, but the queue
  window is a separate webview and cannot read it. Making the queue window ask the main window for the failure list over
  `emitTo` builds a hand-rolled state server in the frontend, for data the backend already owns and already broadcasts.
  It also inverts the dependency: the queue window would need the main window alive and answering.
- **Read the operation log** (`journal.rs` already records `ExecutionStatus::Failed` per op): the log is the durable
  history and stays that; but it stores an execution status, not the typed `WriteOperationError` the copy pipeline
  needs, and it is a different surface with a different job (browse the past, roll back). A live notice is not a history
  query.
- **Backend retention** puts the failure on the stream both windows already subscribe to, seeds correctly through the
  existing `list_operations` on window open, needs no new event and no new listener, and reuses `LifecycleStatus`'s
  already-declared `Failed`. One source, two renderers, zero new plumbing.

The cost is honest: the manager gains a small piece of state whose lifetime is not "while the op runs", and
`free_and_remove`'s removal-on-terminal design gains a documented exception. That exception is bounded (see below) and
is written into `write_operations/DETAILS.md` as part of M9.

### Why a toast and not a modal

The user pressed Queue specifically to stop being blocked. A modal error dialog is an interruption that asks for a
decision, and there is no decision here: the operation already ended, nothing waits on an answer (unlike a conflict
prompt, which genuinely blocks). A modal would also steal focus and eat the keystroke the user was mid-way through. So:
persistent toast (never auto-dismisses, so a failure during lunch is still there afterwards), carrying the real title
and explanation, with "Show in operation queue" as its action. Precedent for the component-shaped toast:
`lib/downloads/DownloadToastContent.svelte` + `addToast(DownloadToastContent, { props })`.

### The rules

**What gets retained.** Exactly the operations whose `write-error` carries a real failure. Excluded by typed match:
`WriteOperationError::Cancelled` (F3) and `WriteOperationError::ArchiveNeedsPassword` (F3, a recoverable prompt). First
error per operation id wins (F4).

**Bounded.** Cap at 20 retained failures, oldest evicted first, mirroring `mcp::terminal_ops::CAPACITY` and its
reasoning. Runtime-only: a restart clears them, consistent with the rest of the manager's state. The operation log is
where a failure lives permanently.

**What dismisses a failed row.** Only an explicit action: the row's Dismiss button, the toolbar's "Dismiss all" when
more than one is retained, and — for a foreground failure — the frontend calling `dismiss_failed_operation(id)` when the
user closes `TransferErrorDialog` for that op. ❌ Never a timer, ❌ never automatically on window close, ❌ never "the
next operation starting". A 40-minute copy that failed while the user was away must still be there.

**Several failures at once.** Each is its own row in the queue, in failure order. Toasts: one per failure up to three,
each with its own reason; past three, they collapse to one summary toast ("4 operations couldn't finish. Open the
operation queue to see why."). The reason for the cap is mechanical, not aesthetic: the toast stack silently drops new
toasts when it is full of persistent ones (`lib/ui/CLAUDE.md`), so an unbounded burst would lose failures. Every reason
stays reachable in the queue, which is the surface that promises completeness. Use `toastGroup: 'operation-failure'` so
a burst can't evict unrelated toasts.

**Foreground failures don't double-report.** The backend retains unconditionally (it cannot know a modal is up), so the
frontend suppresses the toast when the failing op is the one the foreground `TransferProgressDialog` owns (the M6 seam).
The row still appears in the queue — the queue is the operation-status surface, and honesty beats tidiness — and gets
dismissed when the user closes the error dialog, so the common case leaves nothing behind.

**Should the chip reflect a failure?** Yes. Without it, dismissing the toast with the queue window closed leaves zero
trace in the main window, which is the bug this whole spec exists to fix. Rules, deliberately narrow so it stays a
preview and not a notification centre: when at least one failure is retained AND nothing is running, the chip shows a
`triangle-alert` glyph in `--color-warning-*` with the "Couldn't finish" label and no bar; when something IS running,
the running operation wins the chip (live work is the more useful readout) and the failure stays in the queue and in the
toast. Clicking the chip opens the queue window either way. This extends part C past David's brief by one state; it's
called out here rather than done silently.

### What has to cross IPC

Minimal typed addition, nothing more:

- `OperationSnapshot` gains `pub error: Option<WriteOperationError>` (`None` on every live row).
- Two commands: `dismiss_failed_operation(operation_id: String)` and `dismiss_all_failed_operations()`.

Nothing else. Deliberately NOT added: a failure timestamp (row order carries recency, and the operation log has real
timestamps), a retry affordance (out of scope; the user re-runs the operation), and a rendered error string (the
pipeline's contract is that no prose crosses IPC).

## Milestones

Each milestone is independently committable and independently testable. Run `pnpm check -q` scoped to what the milestone
touched (`pnpm check desktop` / `rust` / a named check), and the full `pnpm check` before wrapping.

### M1 — `queue.show` moves to View, with ⌥⌘Q

Part A. No behavior change beyond where the item lives and that a keystroke now opens it.

**Verify the shortcut FIRST, before touching anything.** Add `'⌘⌥Q'` to the `queue.show` registry entry
(`lib/commands/sources/app.ts:99`) and run `pnpm check desktop-svelte-tests`. The gate is
`shortcuts/registry-conflicts.test.ts`, which walks every shipped default and fails on any overlapping-scope clash;
`conflict-detector.test.ts` covers the detector itself with synthetic commands and will not catch a registry clash.
Reading the code, ⌘⌥Q is claimed by no command and is absent from the macOS system table in
`settings/sections/keyboard-shortcuts-banner.ts:94-108` — but confirm it by running the test, not by trusting this line.
**If there IS a real conflict, STOP and report it. Do not substitute another key.**

⚠️ **Spelling gotcha**: the registry string is `'⌘⌥Q'`, ⌘ before ⌥. `shortcut-vocabulary.test.ts` enforces the `⌘⌃⌥⇧`
order `formatKeyCombo` emits; Apple's display order `⌥⌘Q` would be dead on the keyboard. The Rust accelerator is
`Some("Cmd+Alt+Q")`, matching how `log.operationLog` spells ⌥⌘L.

Then:

1. `menu/macos.rs`: build `queue_show_item` in the View block with label `"Operation queue"` (no ellipsis) and
   `Some("Cmd+Alt+Q")`, inserted between `command_palette_item` and `operation_log_item`. Drop it from the Help
   `Submenu::with_items` list.
2. `menu/macos.rs` position comments and `register_item` indices — **both menus shift**:
   - View becomes full(0), brief(1), sep(2), hidden(3), sort(4), zoom(5), sep(6), switch(7), swap(8), sep(9),
     command(10), **queue(11)**, operation_log(**12**), ask_cmdr(**13**).
   - Help becomes shortcuts(0), sep(1), whats_new(**2**), send_feedback(**3**), send_error_report(**4**).
3. `menu/linux.rs`: same move. Label `"Operation &queue"` — `q` is the free mnemonic (`L`, `R`, `h`, `S`, `w`, `p`, `C`,
   `O`, `A` are taken in the View menu; verify against the current labels before committing). View indices match
   macOS's. Help becomes about(0), acknowledgements(1), sep(2), shortcuts(3), whats_new(**4**), send_feedback(**5**),
   send_error_report(**6**).
4. `menu/command_map.rs:180`: the `QUEUE_SHOW_ID` doc comment says "under the Help menu". Fix it.
5. `lib/shortcuts/shortcuts-store.ts`: add `'queue.show'` to `menuCommands` (next to `log.operationLog`), so a custom
   binding syncs its accelerator.
6. `lib/commands/rust-command-id-drift.test.ts:123`: delete the `'queue.show'` entry from `UNREGISTERED_MENU_ITEMS`. It
   now IS registered and IS in `menuCommands`; the test's "no stale excuses" arm will fail if it's left.

**Tests**: `registry-conflicts.test.ts` (green with the new default), `shortcut-vocabulary.test.ts` (the combo is
reachable and correctly ordered), `rust-command-id-drift.test.ts` (menuCommands ↔ the Rust map). Run the app and check
the View menu renders ⌥⌘Q against "Operation queue", and that pressing it opens the window.

**Optional, David's call**: the View menu's SF Symbol table (`macos.rs:708`) gives "Command palette…" an icon but not
"Operation log" or "Ask Cmdr", so leaving "Operation queue" iconless is consistent with its neighbours. If David wants
one, the map matches by EXACT title string (`menu/CLAUDE.md`), so it must read `"Operation queue"` byte-for-byte.

### M2 — Rename to "Operation queue" (English)

Part B, English side. Copy edits plus every non-catalog place the old words are hardcoded.

`src/lib/intl/messages/en/queue.json`:

- `queue.windowTitle` → "Operation queue"; `queue.heading` → "Operations".
- `queue.row.pauseAria` → "Pause this operation", `resumeAria` → "Resume this operation", `cancelAria` → "Cancel this
  operation", `selectAria` → "Select this operation", `queue.list.aria` → "Operations".
- Update each changed key's `@key.description` too: they say "transfer" throughout and the descriptions are the
  translator's brief.
- ✅ **Leave `queue.empty.body` alone** ("Copies, moves, and deletes show up here while they run") per decision 4. Its
  `@key.description` mentions "the transfer queue window" and DOES get updated (a description, not user copy).

`src/lib/intl/messages/en/commands.json`: `commands.queueShow.label` → "Operation queue" (matching the menu item, so
palette and menu read alike); `commands.queueShow.description` → reword off "transfer"; both `@key` descriptions say
"Help menu", which is now wrong.

`src/lib/intl/messages/en/fileOperations.json`:

- `transferProgress.backgroundedToast`: "Still running in the background. Find it in the operation queue."
- `transferProgress.queuedToast`: "…so it's waiting its turn. Find it in the operation queue."
- `transferProgress.queuedToastCount`: currently `{count, plural, one {# transfer} other {# transfers}}`, and what's
  ahead can be a delete. **Needs David's sign-off.** Draft: `{count, plural, one {# operation} other {# operations}}`,
  which keeps the sentence structure and the `{countText}` slot intact. See "Copy needing sign-off" below.
- `transferProgress.queueAria`: "Send to the operation queue".
- `transferProgress.queueTooltip`: "…manage it in the operation queue (F2)".
- Every touched `@key.description` and `screenshotNote`.

Non-catalog English (these are why a catalog-only rename leaves the old name on screen):

- `lib/file-operations/queue/queue-window.ts:101` — `decorateChildWindowTitle('Transfer queue')`, the actual macOS
  window title (F7).
- `src-tauri/src/menu/macos.rs` / `linux.rs` — already done in M1.
- `lib/commands/command-registry.parity.test.ts:34` and `:156` — hardcoded expected label and description.
- `test/e2e-playwright/transfer-queue.spec.ts` — rename the file to `operation-queue.spec.ts`, rename the `describe`
  block, and update the two `[aria-label="Send to the transfer queue"]` selectors (lines 240, 244) to the new label.
- `lib/file-operations/transfer/TransferProgressDialog.queue.test.ts:203, 228, 384` — same aria-label selector.
- Doc titles and prose: `lib/file-operations/queue/CLAUDE.md` + `DETAILS.md` (title lines, and `DETAILS.md:99` says
  "Command palette + Help menu"), `src-tauri/src/file_system/write_operations/DETAILS.md:285, 301` ("Transfers window"),
  `src-tauri/src/settings/loader.rs:623` (comment).
- `docs/architecture.md:38` — the subsystem map line for `file-operations/queue/`.
- `test/e2e-playwright/i18n-capture.spec.ts:549` — a section comment naming the surface.
- The remaining "Transfers window" mentions across `lib/units/`, `lib/settings/`, `lib/file-operations/` docs and
  comments are the same rename;
  `grep -rn "Transfers window\|transfer queue\|transfer-queue" apps/desktop/src apps/desktop/src-tauri apps/desktop/test docs`
  is the checklist. Skip `CHANGELOG.md`, `apps/website/`, and the historical references inside `docs/specs/` (an old
  spec describing what shipped in June is history, not copy).

Then `pnpm intl:keys`.

**Tests**: `pnpm check desktop-message-keys-fresh desktop-message-keys-unused desktop-i18n-parity`, the parity tests
(`file-operations-i18n-parity.test.ts`, `command-registry.parity.test.ts`), and `pnpm check desktop-e2e-playwright` for
the renamed spec. `desktop-i18n-stale` will now list every locale's version of each edited key as stale — that's M3's
job, and it's a warn, not a failure.

### M3 — Translate the renamed strings (nine locales)

Follow `docs/guides/i18n-translation.md` § "New feature → add strings and translate to ALL languages", steps 2–4:
`node apps/desktop/scripts/sync-locale-keys.ts`, then per locale read `docs/i18n/<lang>/style.md` and
`docs/i18n/<lang>/glossary.md` and translate the changed keys in place, updating each `@key.sourceHash` in the same
edit. de, es, fr, hu, nl, pt, sv, vi, zh.

Judgement call per language, worth stating in the commit message: several locales translated "transfer queue" into a
domain word (Swedish "överföringskön", German "Übertragungs-Warteschlange", Hungarian "átviteli sor"). The rename is a
meaning change, not a wording tweak, so each of those needs the same widening English got, not a restamp. ❌ Do NOT use
`--restamp` here: the meaning genuinely changed.

**Update each language's glossary in the same commit.** `docs/i18n/<lang>/glossary.md` records the SETTLED term each
language chose for "transfer queue" during the June queue pass (search each file for "transfer-queue"). A glossary left
saying "överföringskön" is a standing instruction to the next translator agent to use the old term, so the rename would
quietly undo itself on the next pass. Each glossary needs the new term, and a one-line note that the English widened
from transfers to all operations and why.

Re-capture `queue.png` and `queue-empty.png` with `pnpm i18n:shots` (the surfaces are driven from
`test/e2e-playwright/i18n-capture-special.ts:163-195`; the capture dispatches `queue.show` as a COMMAND, so M1's menu
move doesn't affect it).

**Tests**:
`pnpm check desktop-i18n-coverage desktop-i18n-stale desktop-i18n-icu desktop-i18n-plural desktop-message-screenshots-fresh`.

### M4 — `StatusCorner`: the wrapper, with no chip in it yet

Pure structural refactor, provably no visual change. Ships alone so the chip's milestone is about the chip.

Today `IndexingStatusIndicator` positions ITSELF
(`position: absolute; top/right: var(--spacing-sm); z-index: var(--z-sticky)`) and is mounted bare at
`routes/(main)/+page.svelte:729`. Note `.main-content` is not a positioned ancestor, so those offsets resolve against
the initial containing block; the wrapper must reproduce that, which it does by living in the same place in the DOM with
the same declarations.

1. New `src/lib/status-corner/StatusCorner.svelte` (+ `CLAUDE.md` + `DETAILS.md`, they're enforced as a pair): a
   right-aligned flex row (`display: flex; align-items: center; gap: var(--spacing-xs)`) carrying the absolute
   positioning and z-index the hourglass used to carry, rendering `{@render children?.()}` then
   `<IndexingStatusIndicator />`.
2. `pointer-events: none` on the wrapper, `pointer-events: auto` on its children: the wrapper is always mounted, and an
   empty flex box must not sit over the pane and eat clicks.
3. `IndexingStatusIndicator.svelte`: drop `position`, `top`, `right`, `z-index` from `.indexing-status`. Keep the
   inline-flex box, the colour, the focus ring, and the pulse untouched.
4. `+page.svelte`: `<IndexingStatusIndicator />` becomes `<StatusCorner />`.

**Tests**: `IndexingStatusIndicator.a11y.test.ts` stays green unchanged (it renders the component standalone). Add
`StatusCorner.a11y.test.ts` (required by `desktop-svelte-a11y-coverage`). Verify by eye in `pnpm dev` that the hourglass
sits pixel-identically: same corner, same gap, same pulse, tooltip still opens on hover and on focus.

### M5 — The operations store, live in the main window

The data foundation for parts C and D. No UI.

`routes/(main)/+page.svelte` instantiates `createOperationsStore()`, calls `store.init()` in `onMount` and
`store.dispose()` in `onDestroy`, exactly as `routes/queue/+page.svelte:31, 169, 191` does. Decision 5: the same
factory, the same two app-wide streams, no fork.

Two things to get right:

- **Cost.** The store keeps one `WriteProgressEvent` and one ETA smoother per live operation and re-derives rows on each
  tick. With an empty queue that is two idle listeners; during a transfer it's one object per 200 ms event. That's the
  same load the queue window already carries. Don't add memoisation before measuring.
- **The queue window keeps its own instance.** Two instances of the same factory in two webviews is the correct shape;
  they cannot share state across webviews and don't need to.

**Tests**: extend `operations-store.svelte.test.ts` only if a reducer changes (it shouldn't here). Add a main-page test
asserting `init` is called once on mount and `dispose` once on destroy, so a later refactor can't leak the listeners.

### M6 — The foreground-operation seam

F5: nothing tells the main window which operation the foreground progress dialog owns. Both C and D need it.

❌ Don't prop-drill it (`transfer-progress-state` → `TransferProgressDialog` → `DialogManager` → `DualPaneExplorer` →
`+page.svelte` is four hops of a value nobody in between cares about). ✅ A module-scoped signal in
`src/lib/file-operations/foreground-operation.svelte.ts`:

- `setForegroundOperationId(id: string | null)` / `getForegroundOperationId(): string | null`, backed by `$state`.
- `transfer-progress-state.svelte.ts` sets it where `operationId` is assigned from the start command's response, and
  clears it in `destroy()` AND in `handleQueue()` (backgrounding hands ownership to the queue) AND in
  `handleAutoQueued()`.
- Module scope is per-webview, so this is main-window-only by construction and cannot leak into the queue window.
  Exactly one foreground progress dialog exists at a time, which is what makes a single slot correct; assert that in the
  doc comment so nobody turns it into a set without thinking.

The delete/trash path is covered for free: `DeleteDialog` runs the same `TransferProgressDialog` state machine.

**Tests**: unit tests over the module (set, clear, clear-on-queue), plus an assertion in
`transfer-progress-state.svelte.test.ts` that `handleQueue()` clears the slot. This is the red step for M7's "suppressed
while the dialog owns it" gate.

### M7 — The corner progress chip

Part C proper. `src/lib/status-corner/OperationChip.svelte`, rendered by `StatusCorner` before the hourglass.

**Content**: the verb from `tString('queue.row.label', { type })` (the same vocabulary the queue rows use), then an 80
px `ProgressBar`. No percentage text, no "+N" (decision 6).

**Which operation**: the first row in `store.operations` with `status === 'running'`, falling back to the first `paused`
row when nothing runs (decision 9). Snapshot order is the manager's FIFO order, so "first" is stable.

**The bar's fraction** (decision 8):

```
progress.bytesTotal > 0
  ? progress.bytesDone / progress.bytesTotal
  : progress.filesTotal > 0
    ? progress.filesDone / progress.filesTotal
    : 0
```

The zero-bytes case is real, not theoretical: a same-volume move renames server-side and moves no bytes, which is why
`TransferProgressReadout` gates its size row on `bytesTotal > 0`.

**Visibility gates**, all of them:

- Never when there are no rows.
- Never for an instant op: exclude `'rename' | 'create_folder' | 'create_file'` by comparing the typed
  `snapshot.operationType`. ❌ No substring test (`no-string-matching`). Prefer a typed
  `const INSTANT_OPERATION_TYPES = new Set<OperationSnapshot['operationType']>([...])` next to the store's
  `TERMINAL_STATUSES`, so the two typed sets sit together.
- Hidden while `getForegroundOperationId() === row.snapshot.operationId` (M6). The modal already shows that operation in
  full; a duplicate readout in the corner is noise.
- Visible for a paused-only queue, static bar, status word "Paused" (decision 9).
- Visible while the queue window is open (decision 10). No check for that at all — just don't add one.

**Interaction and a11y**:

- A real `<button>`, not the `role="img"` span the hourglass uses, because it does something. Visible focus ring
  (`:focus-visible`, same treatment as `.indexing-status`).
- `onclick` → `openQueueWindow()` (opens or raises; the singleton opener already handles both).
- `aria-label` carries the percentage, e.g. "Copying, 42 percent. Open the operation queue."
- Tooltip with the full detail, via `use:tooltip={{ contentEl }}` following `IndexingStatusIndicator`'s pattern. ⚠️
  **Gotcha**: the tooltip action ADOPTS `contentEl`, and an adopted element keeps its own `hidden` attribute — so the
  content must be the inner `<div bind:this={...}>` inside a `<div hidden>` wrapper, never the wrapper itself
  (`IndexingStatusIndicator.svelte:99-103`). Target content: "Copying 214 items to Naspolya · 42% · about 1m 20s left",
  built from the snapshot summary plus the store's SMOOTHED `row.etaSecondsDisplay` — ❌ never `progress.etaSeconds`, or
  the chip and the queue window will disagree about the same operation, which has happened before (`queue/CLAUDE.md`).
- Sizes and durations through `$lib/units` (`<Size>`, `formatDuration`); ❌ never a local formatter
  (`cmdr/no-private-unit-format`).

**Strings**: every new string in `en/queue.json` (it's the queue's vocabulary) with a `@key.description` and a
`screenshot` / `screenshotNote`. `cmdr/no-raw-user-facing-string` enforces it.

**ProgressBar** (F6): add an `animated?: boolean` prop (default `true`) that drops the shimmer, and wrap the existing
shimmer in `@media (prefers-reduced-motion: no-preference)`. The chip passes `animated={false}` when paused. A new
primitive prop owes a Debug > Components row (`routes/dev/components/sections/Progress.svelte`) and a
`docs/design-system.md` note. Fix `queue/DETAILS.md`'s claim that the shimmer already froze under reduced motion.

**Tests** — this is where the real red-then-green work is. `OperationChip.svelte.test.ts`, driving the store through
`_testApplySnapshot` / `_testApplyProgress`:

- Empty queue → nothing rendered.
- A running copy → verb, bar, and the aria-label percentage.
- Two running ops on disjoint lanes → the FIRST one is shown, and nothing hints at the second.
- `bytesTotal: 0, filesDone: 3, filesTotal: 10` → the bar reads 30%, not 0% (write this one first; it is the fallback's
  whole point).
- `bytesTotal: 0, filesTotal: 0` → 0%, no crash, no `NaN` in the aria-label.
- A `rename` / `create_folder` / `create_file` row → nothing rendered.
- The foreground dialog owns the op → nothing rendered; clear the seam → it appears.
- Paused-only queue → visible, "Paused", `animated={false}`.
- Click → `openQueueWindow` called once.
- `OperationChip.a11y.test.ts` (axe, running + paused states), as its neighbours have.

### M8 — Backend: retain failures on the snapshot

Part D, Rust side. Nothing renders it yet; the test is that the snapshot carries it.

1. `types.rs`: nothing new. `WriteOperationError` already derives `Serialize` + `specta::Type`.
2. `manager.rs`:
   - `OperationSnapshot` gains `pub error: Option<WriteOperationError>`, `None` for live rows.
   - `ManagerInner` gains `failures: VecDeque<OperationSnapshot>`, capped at `FAILURE_CAPACITY = 20`, oldest evicted.
   - `OperationManager::record_failure(&self, operation_id: &str, operation_type: WriteOperationType, error: &WriteOperationError)`:
     - returns early on
       `matches!(error, WriteOperationError::Cancelled { .. } | WriteOperationError::ArchiveNeedsPassword { .. })` (F3);
     - returns early if `failures` already holds this id (F4, first-write-wins);
     - builds the row from the still-live record's descriptor (source / destination summary), falling back to the
       event's `operation_type` and `None` summaries if the record is gone;
     - pushes with `status: LifecycleStatus::Failed` and `supports_rollback: false` (a settled failure offers no
       rollback from this row).
     - ❌ Does NOT emit. See the next point.
   - `ManagerInner::snapshot()` appends failure rows AFTER the live rows, **skipping any failure whose id is still in
     `records`**. ⚠️ This is load-bearing (F9): `emit_error` runs before `on_settled`, so for a moment the op is both
     live and failed, and a duplicate `operationId` in the list would throw in the keyed `{#each}`. The failure row
     appears on `on_settled`'s existing `emit_changed`, which is the correct moment anyway.
   - `dismiss_failed_operation(operation_id)` / `dismiss_all_failed_operations()`: drop from `failures`, then
     `emit_changed()`.
3. `event_sinks.rs`: in `TauriEventSink::emit_error`, call `manager().record_failure(...)` next to the existing
   `mcp::terminal_ops::record(...)`. Same emit-site pattern, same place, one more line. ❌ Do NOT put it in the trait or
   in `CollectorEventSink`: test sinks must stay side-effect-free.
4. `commands/file_system/write_ops.rs`: two `#[tauri::command] #[specta::specta]` pass-throughs next to
   `cancel_operations`. Register them in the `tauri_specta` invoke handler in `lib.rs`; ✅ no capability change needed
   (`queue/DETAILS.md` § Capabilities: manager commands go through the invoke handler, not the ACL).
5. `pnpm bindings:regen`, then thin wrappers in `lib/tauri-commands/operations.ts` mirroring `cancelOperations`.

**Tests** (Rust, `manager.rs`'s `tests` module, using `TestOperationGuard` — ❌ never a literal id plus a manual remove,
and ❌ never `cancel_all_write_operations()`):

- `record_failure` then settle → `list_operations()` contains one `Failed` row carrying the typed error.
- A failure recorded while the op is still live does NOT appear in the snapshot (F9); it appears after `on_settled`.
- `Cancelled` and `ArchiveNeedsPassword` record nothing.
- Two `record_failure` calls for one id keep the FIRST error.
- 25 failures leave 20, oldest gone.
- `dismiss_failed_operation` removes exactly one; `dismiss_all_failed_operations` empties the list; both re-emit.
- Lanes: a failed op's lane is freed exactly as before (retention must not touch `free_and_remove`). Assert via
  `lane_use_snapshot()`.

Frontend: `desktop-svelte-type-drift` and `desktop-bindings-fresh` cover the new field crossing the wire.

### M9 — The queue window shows failed rows

1. `routes/queue/+page.svelte:46`: the filter keeps hiding `done` and `cancelled` (which, per F1, never arrive anyway)
   and stops hiding `failed`. Prefer an explicit `SETTLED_HIDDEN_STATUSES` set over inverting `isTerminalStatus`, and
   leave `isTerminalStatus` exported with its current meaning for the store's own use.
2. `QueueRow.svelte`: a `failed` branch. Status word from the existing `queue.row.status` `failed` arm, "Couldn't
   finish" (F8). No pause, no cancel, no rollback, no select checkbox. One Dismiss button.
3. The reason, rendered through the EXISTING pipeline: `getTransferErrorMessage(snapshot.error, operationType)` from
   `lib/file-operations/transfer/transfer-error-messages.ts`, giving `{ title, message, suggestion }`. ⚠️ That pipeline
   uses `getMessage()` (raw catalog lookup, NO ICU) and per-operation variant keys
   (`errors.write.<field>.<copy|move|delete|trash>`) selected by `operationType` — read `lib/file-operations/CLAUDE.md`
   before touching it, and ❌ do not invent new error prose. The composed explanation and suggestion are
   `{@html}`-injected via the same escaping boundary the dialog uses; if the row renders them, it goes through
   `renderErrorMarkdown`, never a raw interpolation.
   - The row shows the title inline and the explanation on a second line; the suggestion is the natural tooltip or an
     expandable, David's call on layout.
   - ⚠️ `operationType` on a snapshot is the wire enum (`archive_edit`, `create_folder`), while
     `transfer-error-messages.ts` takes `TransferOperationType` (`copy | move | delete | trash`). Map explicitly and
     fall back to the generic arm for `archive_edit`; do NOT cast.
4. Toolbar: "Dismiss all" appears only when more than one failure is retained.
5. New strings (dismiss labels, aria labels) in `en/queue.json` with `@key` descriptions.

**Tests**: `QueueRow.svelte.test.ts` gains failed-state cases (controls hidden, Dismiss wired, the real reason text
rendered for at least two distinct `WriteOperationError` variants and two operation types, proving the variant-key
selection works). `QueueRow.a11y.test.ts` gains the failed state. E2E in the renamed `operation-queue.spec.ts`: start a
copy that fails (a read-only destination is the cheapest deterministic failure), close the queue window, reopen it,
assert the failed row is there with its reason — that single test IS the "survives the window being closed" requirement.

### M10 — The main-window failure notice

1. `src/lib/status-corner/operation-failure-watch.svelte.ts`: watches the main window's store (M5) for rows entering
   `failed` that it hasn't announced yet, keyed by `operationId` so a re-emitted snapshot can't double-toast.
2. For each newly-announced failure: skip if `getForegroundOperationId()` matches (M6, the foreground dialog is already
   showing it); otherwise
   `addToast(OperationFailedToastContent, { level: 'error', dismissal: 'persistent', toastGroup: 'operation-failure', props: { snapshot } })`.
   Past three concurrent failure toasts, replace with the summary toast (see "The rules").
3. `OperationFailedToastContent.svelte`: the reason from the same pipeline as M9, plus a "Show in operation queue"
   button calling `openQueueWindow()`.
4. `TransferErrorDialog`'s close path calls `dismissFailedOperation(operationId)`, so a foreground failure the user has
   already read and closed doesn't linger as a queue row.
5. `OperationChip.svelte`: the failure state from "Should the chip reflect a failure?" — `triangle-alert` in
   `--color-warning-*`, "Couldn't finish", no bar, shown only when nothing is running.
6. Strings for the toast and the chip's failure state, in the catalog with `@key` descriptions.

**Tests**: unit tests over the watcher (one toast per failure; no second toast on a re-emitted snapshot; suppressed when
the foreground dialog owns the op; three failures give three toasts, four give a summary),
`OperationFailedToastContent.svelte.test.ts` + its a11y test, and a chip test for the failure state and its precedence
against a running op.

### M11 — Translate part C and D's strings, and the docs

1. `pnpm intl:keys`, `node apps/desktop/scripts/sync-locale-keys.ts`, translate the new keys into all nine locales per
   `docs/guides/i18n-translation.md`.
2. Screenshots: add a `queue-failed` surface to `test/e2e-playwright/i18n-capture-special.ts` next to the existing
   `queue` / `queue-empty` pair (drive a failing copy the same way the E2E does), and a main-window surface for the
   chip. If a direct chip capture proves fiddly, a `screenshotNote` mapping onto an existing main-window surface is
   acceptable and is the established fallback. Then `pnpm i18n:shots`.
3. Docs, all of them in this milestone so nothing ships undocumented:
   - `lib/file-operations/queue/CLAUDE.md` + `DETAILS.md`: the new name and the "why" from this spec's § "Why 'Operation
     queue'"; the failure-retention model; the fact that `LifecycleStatus::Failed` is now reachable and how (F1
     corrected); the reduced-motion correction (F6).
   - `src-tauri/src/file_system/write_operations/DETAILS.md`: the removal-on-terminal exception — failures are retained
     out-of-band in a bounded list, lanes and busy-state are freed exactly as before, and why.
   - `src-tauri/src/mcp/terminal_ops.rs`'s module doc asserts "`LifecycleStatus` never reaches
     `Done`/`Cancelled`/`Failed` on a live record". After M8 that's still true of LIVE records but no longer of the
     snapshot. Correct it, or the next reader trusts a stale claim.
   - New `lib/status-corner/CLAUDE.md` + `DETAILS.md` (they're enforced as a pair).
   - `docs/architecture.md`: a map line for `status-corner` (what + where + pointer, never how).
   - `lib/file-operations/CLAUDE.md`'s module map: the queue entry's description.
4. `pnpm check` in full, then `pnpm check --include-slow`.

## Copy needing David's sign-off

Drafts, not decisions. Everything else in part B is a mechanical substitution of one noun for another.

1. **`queuedToastCount`** — currently `{count, plural, one {# transfer} other {# transfers}}`, and what's ahead can be a
   delete. Draft: `{count, plural, one {# operation} other {# operations}}`. Keeps the `{countText}` slot and the host
   sentence unchanged. The alternative, rewriting the host sentence to drop the count noun entirely ("Something else is
   using this drive, so this one is waiting its turn"), reads better but loses the count and changes a string nine
   locales have already translated.
2. **The chip's tooltip** — "Copying 214 items to Naspolya · 42% · about 1m 20s left". The `·` separators and the
   ordering are a design choice.
3. **The failure toast** — draft: title from the error pipeline, then "Show in operation queue". And the summary form,
   draft: "4 operations couldn't finish. Open the operation queue to see why."
4. **The chip's failure label** — draft "Couldn't finish", matching `queue.row.status`'s `failed` arm so the two
   surfaces say the same words.
5. **Row dismiss labels** — draft "Dismiss" (row) and "Dismiss all" (toolbar).

Every one of these must obey `docs/style-guide.md`: never the words "error" or "failed" in user-facing copy,
conversational and actionable, sentence case.

## Gotcha register

Things that will bite whoever builds this, collected in one place.

- **The shortcut string spells ⌘ before ⌥** (`'⌘⌥Q'`), enforced by `shortcut-vocabulary.test.ts`. Apple's display order
  is dead on the keyboard.
- **Both menus' position comments AND `register_item` indices shift** when the item moves. The comments are the only
  documentation of those magic numbers; a wrong index silently breaks accelerator sync for the items after it.
- **`rust-command-id-drift.test.ts` fails if the excuse is left behind.** Its "no stale excuses" arm is deliberate.
- **The tooltip action adopts `contentEl` and adopted elements keep `hidden`.** Bind the inner div, not the wrapper.
- **Render `row.etaSecondsDisplay`, never `progress.etaSeconds`.** The raw value once showed one operation as "8m 12s"
  in one window and "5m 46s" in the other.
- **A paused op reports `is_running: true`.** The bar-is-moving truth is the snapshot `status`.
- **`emit_error` fires before the record is removed**, so a failure row and a live row can briefly share an id — filter
  the failure out while the record lives, or the keyed `{#each}` throws (F9).
- **`emit_error` can fire twice per op** (F4) and fires for non-failures (F3).
- **The error pipeline is `getMessage()`, not `t()`.** Catalog values there use normal apostrophes, not ICU's doubled
  ones, and carry `{token}` placeholders the `.ts` interpolates.
- **Window perms fail SILENTLY.** Every Tauri call in the queue window is awaited in try/catch with a `log.warn`; keep
  it that way for anything new.
- **`.main-content` is not a positioned ancestor.** `StatusCorner`'s absolute offsets resolve against the initial
  containing block, exactly as the hourglass's do today. Don't "fix" this by adding `position: relative` somewhere.

## Honest read: what I'd flag

Delivered as specified. These are the concerns, not changes.

1. **F1 makes part D roughly twice the job the brief implies.** The brief reads as though a frontend filter is hiding
   failures; it isn't, the backend deletes them. M8 is unavoidable Rust work in the operation manager, the most
   safety-sensitive scheduler in the app. It's contained (retention is out-of-band; `free_and_remove` is untouched), but
   it isn't a copy change. Worth knowing before committing to the whole spec in one sitting.

2. **Retention adds state whose lifetime isn't "while the op runs".** That's a real departure from the manager's current
   discipline, and the reason the cap, the runtime-only lifetime, and the explicit dismissal rules are all specified
   rather than left to taste. If David would rather not touch the manager at all, the fallback is main-window-only:
   toast plus chip, no failed row in the queue window. That satisfies "the user must see the error message" and drops M9
   entirely — but it means the queue window, the one surface named after listing operations, still can't tell you an
   operation failed. I don't recommend it.

3. **The chip's failure state is me extending part C.** David asked me to think it through; I decided yes, and said why
   above. If he disagrees, drop step 5 of M10 — nothing else depends on it.

4. **Two windows now render the same error prose through the same pipeline, for the first time.**
   `transfer-error-messages.ts` was written for a modal with room. In an 80 px chip's tooltip and a 360 px toast, some
   variants (`files_too_large_for_filesystem` lists up to ten files) will be long. The toast and the row need a sensible
   truncation story; the queue row has the space, the toast doesn't. Flagging it rather than pre-solving it, because the
   right answer is visual and David reviews visuals.

5. **⌥⌘Q sits one modifier away from ⌘Q.** No conflict exists (verified against the registry and the system table), and
   the fat-finger risk is Quit, which has its own confirmation behavior. Recording it because "one slip from quitting
   the app" is the kind of thing that only feels wrong once it ships.

6. **The chip shows the first running operation, so on a busy queue it can look stuck on one drive** while another
   finishes several. That's decision 7 and it's the right call for a preview, but a user with three lanes running will
   see one third of the truth in the corner. The queue window is the answer, and the chip clicking through to it is what
   makes that acceptable.
