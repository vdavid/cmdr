# Dialog gallery details

Depth for [CLAUDE.md](CLAUDE.md). The gallery is a dev-only instrument: Debug > Soft dialogs lists every registered soft
dialog and opens it, in each of its meaningful states, over the main window.

## Why it exists

Cmdr's soft dialogs are mostly hard to evoke on purpose. The bulk-rename review needs an Ask Cmdr message plus a wired
agent; the transfer-progress dialog needs a live transfer; the stale-drive explainer needs an external drive whose index
actually went stale. That makes design review of the dialogs impractical, and they're the part of the app most in need
of one.

The thesis is that **a design-review instrument must not lie**. Two consequences shape everything below.

### 1. The dialogs render in the main window

They're designed to sit on the two-pane backdrop: the `ModalDialog` overlay deliberately starts at
`inset: var(--titlebar-height)`, uses backdrop blur and macOS vibrancy, and the reduce-transparency fallback keys off
the main window's chrome. Three more reasons the Debug window is the wrong host:

- `debug.json` is a deliberately minimal capability and Tauri permissions **fail silently**, so dialog buttons that open
  windows or external URLs would look broken for reasons unrelated to their design.
- `ModalDialog` reports every mount to the Rust `SoftDialogTracker` via `notifyDialogOpened`, which the MCP `dialog`
  tool and the E2E acks read. A gallery copy mounted in the Debug window would tell the backend a dialog is open in the
  main window when it isn't.
- Rendering in the main window means MCP `dialog close <id>` works on gallery-opened dialogs for free.

Three dialogs don't live in the main window at all (`delete-ai-model` is a settings-window dialog; `viewer-copy-confirm`
/ `viewer-copy-refuse` are viewer-window dialogs). The gallery still shows them over the main window (hosting three
extra windows isn't worth it), so **every row names the window the dialog actually lives in**. Reviewing a settings
dialog over the file panes is fine as long as the instrument says that's what you're looking at.

### 2. Don't distort a dialog to make it previewable

A preview-only branch inside a dialog is a branch that can rot, and it means you're no longer reviewing the shipping
component. No `preview?: boolean` props, no dev-only rendering paths inside dialog components. The gallery either passes
real props, seeds the real state store, or emits the real backend event.

## Transport: Debug window → main window

`DebugDialogsPanel.svelte` emits `emitTo('main', 'debug-open-gallery-dialog', { dialogId, stateId })`;
`routes/(main)/listener-setup.ts` consumes it inside the existing `if (import.meta.env.DEV)` block (the same seam
`debug-inject-error` and `debug-trigger-transfer-error` use, recorded at `routes/(main)/DETAILS.md`). The listener calls
`openGalleryDialog(...)` and then focuses the main window **from the main window's own side**: the Debug window's
capability set is minimal and permission failures are silent, so it must not try to push focus itself.

`routes/(main)/+layout.svelte` mounts `DialogGallery.svelte` inside `{#if import.meta.env.DEV}`, alongside the other
always-mounted dialogs (`crash-report`, `error-report`, `feedback`, `mtp-permission`, `ptpcamerad`). Not `+page.svelte`:
it's already over its `file-length` allowlist entry.

`+page.svelte` still reads `isGalleryDialogOpen()` in `isModalDialogOpen()`. Without it, global shortcuts fire behind
the previewed dialog, which looks like a dialog bug and would poison the review. That call is the ONLY thing production
code imports from this directory, which is why `gallery-state.svelte.ts` pulls in nothing else.

**Gotcha: don't wrap that call in a build-time `DEV` guard.** Guarding it with `import.meta.env.DEV &&` makes knip stop
seeing `+page.svelte`'s dynamic `import('$lib/debug/debug-window')` and report `lib/debug/debug-window.ts` as an unused
file (reproduced on knip 6.27.0, 2026-07-22: adding and removing that one guard flips the failure on and off with
everything else identical; a bare `import.meta.env` elsewhere in the file is fine, and the file already has two). The
guard buys nothing anyway: nothing writes the store outside the DEV-gated listener, so the getter is already a constant
`false` in production.

## The three ways a dialog gets opened

Which mechanism applies is a **property of how the dialog is already built**. Verify per dialog; don't assume from the
name.

- **Prop-driven**: the harness renders the component with fixture props. Works only when the component takes everything
  it needs as props. `alert` is the reference case.
- **Store-seeded**: the component reads a module-level `$state` store and takes no content props
  (`BulkRenameReviewDialog` has no props at all; `FeedbackDialog` and `ErrorReportDialog` self-gate on their flow stores
  and are mounted bare in `+layout.svelte`). Rendering those directly renders them **empty**. The gallery seeds the
  store and the app's own mount site renders it: the most faithful of the three, since the real trigger path runs. Such
  an entry must restore the store on close, so a preview doesn't leave the app half-seeded.
- **Event-seeded**: the component self-mounts off a backend event (`StaleDriveDialog`). The gallery arranges the
  preconditions and emits the real event.

## Adding an entry

1. Add a row to `DIALOG_GALLERY_ENTRIES` with the `dialogId`, a sentence-case label, the `hostWindow` the dialog really
   lives in, and its states. The `dialog-gallery-coverage` check fails until the row exists.
2. If you can't open it honestly yet, ship the row as `status: 'not-triggerable'` with a `reason` that's true. The check
   asserts id presence only, never state completeness, so an honest gap never fights the check. ❌ Don't invent a
   technical blocker for a dialog that simply isn't wired yet: publishing a false reason inside an instrument whose
   thesis is "must not lie" is the worst available outcome.
3. Add fixtures under `fixtures/`, keyed by the state ids, and a case in `DialogGallery.svelte`'s `plan` derivation.
4. **Fixture data is part of the design review.** Include the cases that break layouts: a very long filename, a
   deeply-nested path, a large file count with thousands separators, a multi-line error. A gallery of tidy 12-character
   names hides exactly the problems this exists to surface.
5. State coverage is the point. A dialog that's a comparison (`rename-conflict`) reviews nothing in a single state.

`apps/desktop/test/e2e-playwright/i18n-capture-surfaces.ts` already stages many of these dialogs for screenshot capture,
so the staging work is often already done there.

## The coverage check

`scripts/check/checks/desktop-svelte-dialog-gallery-coverage.go` compares `{ id: '…' }` in `lib/ui/dialog-registry.ts`
against `dialogId: '…'` in `gallery-registry.ts`, both directions. Nested state objects use `id` (not `dialogId`) and
unregistered overlays use `overlayId`, so neither can be mistaken for a dialog id. Without this check the gallery
silently stops being an inventory the first time someone adds a dialog, and a review that trusts it reviews the wrong
set.
