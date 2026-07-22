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

`DebugDialogsPanel.svelte` emits `emitTo('main', 'debug-open-gallery-dialog', { dialogId, stateId, fixtures })`
(`fixtures` is `null` unless the row's `usesFixtureDir` is set; the panel resolves it through the dev-only
`createDialogGalleryFixtures` IPC, which only an eslint-exempt path may call); `routes/(main)/listener-setup.ts`
consumes it inside the existing `if (import.meta.env.DEV)` block (the same seam `debug-inject-error` uses, recorded at
`routes/(main)/DETAILS.md`). The listener calls `openGalleryDialog(...)` and then focuses the main window **from the
main window's own side**: the Debug window's capability set is minimal and permission failures are silent, so it must
not try to push focus itself.

**Gotcha: that focus call needs `core:window:allow-set-focus` in `capabilities/default.json`.** The main window's
capability didn't grant it, so `focusMainWindow()` rejected and the previewed dialog opened BEHIND the Debug window,
which reads as a dialog bug rather than a permissions one (Tauri permission failures are silent, and the old handler
swallowed the rejection). It now logs the failure. Same call serves the confirmation-dialog focus request, so that path
was broken too.

`routes/(main)/+layout.svelte` mounts `DialogGallery.svelte` inside `{#if import.meta.env.DEV}`, alongside the other
always-mounted dialogs (`crash-report`, `error-report`, `feedback`, `mtp-permission`, `ptpcamerad`). Not `+page.svelte`:
it's already over its `file-length` allowlist entry.

`+page.svelte` still reads `isGalleryDialogOpen()` in `isModalDialogOpen()`. Without it, global shortcuts fire behind
the previewed dialog, which looks like a dialog bug and would poison the review. That call is the ONLY thing production
code imports from this directory, which is why `gallery-state.svelte.ts` pulls in nothing else (its type-only imports
are erased, so the disk-fixture shape can live there without pulling anything in).

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

## The disk-backed dialogs

`delete-confirmation`, `transfer-confirmation`, `mkdir-confirmation`, `new-file-confirmation`, and `go-to-path` do real
work on mount: background scans, folder-suggestion streams, volume-space queries, conflict lookups, path resolution.
Faking that would fake the very numbers the design displays, so they run against a real throwaway directory instead.

**The tree.** `src-tauri/src/dev_fixtures.rs` (a `#[cfg(debug_assertions)]` module) creates
`<app data dir>/dialog-gallery-fixtures/`: ~35 files across nested folders, sizes from 0 bytes to 486 MB, including a
213-character filename, non-ASCII folder and file names, and an empty folder. Everything above the first line of content
is sparse (`set_len`), so the tree reports hundreds of megabytes to a scan while costing kilobytes of disk. It's
idempotent by construction (a file is written only when missing or the wrong length) and never deletes, so a folder the
reviewer created inside it survives the next trigger. Its Rust tests pin all three properties.

The command returns landmarks (`destinationDir`, `existingFolderName`, `existingFileName`, `nestedPath`) rather than
just the path: the side that CREATES the tree is the only one that can name its parts without drifting from disk. The
destination folder deliberately already holds entries named like some sources, so the transfer conflict pre-check finds
real conflicts.

**The listing handle.** `NewFolderDialog` / `NewFileDialog` take a `listingId` and use it for the conflict lookup, the
directory-diff filter, and `refreshListing`. It's PANE-owned, not something a directory produces, so `disk-fixture.ts`
navigates the focused pane to the fixture directory (`navigateToDirInPane`) and reads back
`ExplorerAPI.getPaneListingId(pane)`. ❌ A fabricated id fails silently — the conflict check just stops working — so no
id means the preview doesn't open at all. Navigating the focused pane is a real side effect; the Debug panel discloses
it on every fixture-directory row.

The same pass fetches real entries via `getFilesAtIndices` (backend indices, so the synthetic `..` row can't reach a
fixture), which is where delete's items and transfer's sources come from. That's why `fixtures/disk.ts` holds BUILDERS
rather than data: names, sizes, and folder flags come off the disk, not out of a literal.

**Correct safety story** (the intuitive one is backwards): `DeleteDialog` and `TransferDialog` take `onConfirm` as a
PROP and perform nothing themselves, so a gallery no-op is harmless wherever they point. `NewFolderDialog` calls
`createDirectory()` ITSELF (and `NewFileDialog` `createFile()`), so mkdir and mkfile genuinely write. Those two are what
the fixture directory actually protects.

## Adding an entry

1. **Check how the dialog is actually built before deciding the mechanism.** Open the component and read its props. If
   it takes everything it renders as props it's prop-driven; if it reads a module store and takes no content props,
   rendering it directly renders it EMPTY and it belongs to the store-seeded path instead. The dialog decides, not the
   name.
2. Add a row to `DIALOG_GALLERY_ENTRIES` with the `dialogId`, a sentence-case label, the `hostWindow` the dialog really
   lives in, and its states. The `dialog-gallery-coverage` check fails until the row exists.
3. If you can't open it honestly yet, ship the row as `status: 'not-triggerable'` with a `reason` that's true. The check
   asserts id presence only, never state completeness, so an honest gap never fights the check. ❌ Don't invent a
   technical blocker for a dialog that simply isn't wired yet: publishing a false reason inside an instrument whose
   thesis is "must not lie" is the worst available outcome.
4. Add fixtures under `fixtures/`, keyed by the state ids, register the record in `fixtures/index.ts`, and add an entry
   to `DialogGallery.svelte`'s `planResolvers` table plus a branch in its template. A dialog that does real work on
   mount sets `usesFixtureDir: true` on its row and holds BUILDERS in `fixtures/disk.ts` instead of literals (see "The
   disk-backed dialogs"). Two tests cover the seams: `fixtures.test.ts` walks `fixtureRecords` against the registry (a
   state id with no fixture, or a fixture with no row), and `DialogGallery.svelte.test.ts` mounts EVERY advertised state
   and asserts the dialog reported its own id to the tracker. A dead button fails there rather than mid-review.
5. **Fixture data is part of the design review.** Include the cases that break layouts: a very long filename, a
   deeply-nested path, a large file count with thousands separators, a multi-line error. A gallery of tidy 12-character
   names hides exactly the problems this exists to surface.
6. State coverage is the point. A dialog that's a comparison (`rename-conflict`) reviews nothing in a single state.
   Where a typed union drives the whole rendering (`transfer-error`), key the fixtures by an exhaustive `Record` over
   that union so a new variant is a compile error here.
7. **Say what the row can't show.** A dialog that reads live app state, or that has states props can't reach, or whose
   buttons do something real, needs an entry `note` saying so. Those notes are the instrument, not decoration.

`apps/desktop/test/e2e-playwright/i18n-capture-surfaces.ts` already stages many of these dialogs for screenshot capture,
so the staging work is often already done there.

## What the fixtures deliberately don't do

Every fixture callback closes the preview and nothing else. `onResolve`, `onCommit`, `onSaveAs`, `onRetry`, and friends
have nothing real behind them: the gallery has no rename in flight, no pane selection, no failed transfer. A no-op is
honest; wiring a plausible-looking fake action wouldn't be.

Some dialogs still act for real, because the ACTION lives inside the component rather than in a callback the gallery
supplies. Dismissing `commercial-reminder` or `expiration` records the real flag; `extension-change` writes the real
"always allow" setting; `license` activates and resets keys for real; `connect-to-server` opens a real socket and fires
real mDNS; `crash-report`'s Send skips the upload in dev but still writes settings and deletes a pending crash file;
`mkdir-confirmation` / `new-file-confirmation` create a real folder or file (inside the fixture directory); removing a
row from `go-to-path`'s recents removes it for real. Each of those rows carries a `note` saying so. Don't silence one by
adding a preview branch to the component.

## Rows that can't be fixtures

`about` and `license` take only callbacks and read the licensing store's cached status plus an on-mount IPC, so a
reviewer sees whatever license the dev machine has; their other states (existing-license panel, server-invalid retry,
confirm-reset, loading) have no prop to reach them. `connect-to-server` is the same shape plus the mDNS side effect.
They're `ready` with one state and a `note` that says exactly this, which beats both a false `not-triggerable` reason
and a silent row that implies a curated preview.

Seeding the licensing store to unlock the other `license` / `about` states is the obvious next step, but it's
store-seeding: it mutates real app state and owes a restore-on-close, so it belongs with the other store-seeded entries
rather than bolted onto a prop-driven row.

## Why `transfer-error` isn't the Debug error panel's job any more

`DebugErrorPreviewPanel` used to open this dialog too, by fabricating an `io_error` and stuffing a listing error's title
into its `message`. That showed the io-error copy no matter which error you picked, which is the kind of quiet
distortion this gallery exists to remove; the panel's pane-error injection (`debug-inject-error`, which genuinely uses
`preview_friendly_error`) is untouched. The gallery renders the real component from a real typed `WriteOperationError`,
one state per variant, so the copy, category tint, icon, and Retry visibility are all the production ones.

## The coverage check

`scripts/check/checks/desktop-svelte-dialog-gallery-coverage.go` compares `{ id: '…' }` in `lib/ui/dialog-registry.ts`
against `dialogId: '…'` in `gallery-registry.ts`, both directions. Nested state objects use `id` (not `dialogId`) and
unregistered overlays use `overlayId`, so neither can be mistaken for a dialog id. Without this check the gallery
silently stops being an inventory the first time someone adds a dialog, and a review that trusts it reviews the wrong
set.
