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
- Rendering in the main window means MCP `dialog close <id>` reaches gallery-opened dialogs for free — every one the
  close registry routes. **Three exceptions**: `about`, `delete-confirmation`, and `transfer-confirmation`. MCP closes
  those through dedicated events bound to the app's OWN state (`onCloseConfirmation` →
  `explorer.closeConfirmationDialog()`, and the about flag in `+page.svelte`), which a gallery preview isn't, so the
  tool times out with an honest not-acknowledged error rather than closing nothing silently. Escape, the dialog's own
  buttons, and re-triggering any row all still close the preview. (Verified in a dev build, 2026-07-22: `alert` and
  `feedback` close over MCP, `about` and `delete-confirmation` don't.)

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
the previewed dialog, which looks like a dialog bug and would poison the review. That call is the only thing the
MAIN-WINDOW graph imports from this directory, which is why `gallery-state.svelte.ts` pulls in nothing else (its
type-only imports are erased, so the disk-fixture shape can live there without pulling anything in).

**Gotcha: don't wrap that call in a build-time `DEV` guard.** Guarding it with `import.meta.env.DEV &&` makes knip stop
seeing `+page.svelte`'s dynamic `import('$lib/debug/debug-window')` and report `lib/debug/debug-window.ts` as an unused
file (reproduced on knip 6.27.0, 2026-07-22: adding and removing that one guard flips the failure on and off with
everything else identical; a bare `import.meta.env` elsewhere in the file is fine, and the file already has two). The
guard buys nothing anyway: nothing writes the store outside the DEV-gated listener, so the getter is already a constant
`false` in production.

## What actually reaches a production bundle

Measured, not assumed (`pnpm build`, 2026-07-22, grep for marker literals in `apps/desktop/build/`):

- **Absent**: `DialogGallery.svelte`, every `fixtures/` module, `disk-fixture.ts`, `store-seeding.ts`, and the two
  preview modules — so are the dialog imports the harness would otherwise have added to the main-window graph. The
  `{#if import.meta.env.DEV}` in `+layout.svelte` is what does it (Vite inlines the flag). Markers checked:
  `Cmdr paused indexing because the drive is running on battery.` (`fixtures/alert.ts`) and
  `Dialog gallery has no fixture for` (the harness's own warning). Neither appears anywhere under `build/`.
- **Present**: `gallery-registry.ts`'s row copy and `DebugDialogsPanel.svelte`, inside the Debug route's own lazily
  loaded node chunk (`build/_app/immutable/nodes/<n>.*.js`) — where `DebugErrorPreviewPanel`, the closed-tabs panel, and
  the rest of that window already were. `routes/debug/` is a real SvelteKit route with no production exclusion, so it
  builds like any other; only its opener is DEV-gated. The gallery adds ~11 kB of row copy to that chunk and nothing to
  what the main window loads.
- **Deliberately present**: `gallery-state.svelte.ts`, which `+page.svelte` imports unconditionally (above).

The Rust side is absolute: `create_dialog_gallery_fixtures` and the `dev_fixtures` module are `#[cfg(debug_assertions)]`
at the function, the `ipc.rs` registration, and the collector, and `strings` on the release binary finds neither the
command name nor `dialog-gallery-fixtures` (while shipping command names like `notify_dialog_opened` are there).

❌ So don't write "the whole gallery tree-shakes out". The claim that holds is the one that matters: nothing the harness
renders, and no dialog it pulls in, reaches the main window's bundle. Excluding the Debug route from production builds
would be a separate change, and it would take the rest of the Debug window with it.

## The three ways a dialog gets opened

Which mechanism applies is a **property of how the dialog is already built**. Verify per dialog; don't assume from the
name.

- **Prop-driven**: the harness renders the component with fixture props. Works only when the component takes everything
  it needs as props. `alert` is the reference case.
- **Store-seeded**: the component reads a module-level `$state` store and takes no content props
  (`BulkRenameReviewDialog` has no props at all; `FeedbackDialog` and `ErrorReportDialog` self-gate on their flow stores
  and are mounted bare in `+layout.svelte`). Rendering those directly renders them **empty**. The gallery seeds the
  store and the app's own mount site renders it: the most faithful of the three, since the real trigger path runs. See
  the section below.
- **Event-seeded**: the component self-mounts off a backend event (`StaleDriveDialog`). The gallery arranges the
  preconditions and emits the real event. See the section below.

A row whose mechanism isn't "the harness renders it" says so in `openedBy`, which the Debug panel discloses and the
harness's mount sweep reads (those rows render nothing of their own, so the sweep would otherwise fail on them).

## Store-seeded dialogs

`bulk-rename-review`, `feedback`, `whats-new`, `error-report`, and `operation-log` are opened by patching a real app
store: `fixtures/store-seeded.ts` holds a **patch per state** (not a prop bag), plus `buildStoreSeed`, the one place
that knows, per dialog, which store the patch lands on and how the app itself decides the dialog is showing. Everything
else — the `+page.svelte` / `+layout.svelte` mount site, the component, the close path — is production code running
normally.

**Restoring is structural, not per-fixture.** `store-seeding.ts`'s `seedStore(store, patch)` snapshots exactly the keys
the patch names and returns the undo; `DialogGallery.svelte` runs that closure as an **`$effect` cleanup**. So closing
the dialog, swapping to another preview, and unmounting the harness all put the store back, and a fixture can neither
forget to clean up nor clean up something it never touched. Two gotchas the code guards:

- The seeding call is wrapped in `untrack`. `apply()` reads the fields it's about to overwrite, and a tracked read would
  make the effect depend on its own writes (restore, re-seed, forever).
- A seeded dialog closes through its OWN store (Escape, its Cancel button), never through `closeGalleryDialog()`. A
  second effect watches `seed.isOpen()` and drops the gallery's preview when it flips, or `+page.svelte` would go on
  suppressing every global shortcut behind a dialog that's gone. It reads `isOpen()` unconditionally so the subscription
  exists no matter which effect runs first.

`DialogGallery.svelte.test.ts` walks every store-seeded state, and asserts the store comes back **byte-identical**
(`JSON.stringify` before and after) rather than merely closed: a preview that half-seeds the app is worse than one that
doesn't open.

**`bulk-rename-review`'s Apply fails, by design.** `applyRenameReview` keys on a `proposalId` the backend staged, and a
fixture proposal has no counterpart there, so the attempt logs a warning and the review stays up. Cancel and Escape are
unaffected (`cancel_bulk_rename_proposal` is infallible: it consumes the id if it exists). The row says so.

## `onboarding` isn't store-seeded

`OnboardingWizard` is a hand-rolled `role="dialog"` overlay, not a `ModalDialog` (it reports itself to the tracker with
a direct `notifyDialogOpened` call), and its open flag is a **local `let showOnboarding`** in
`routes/(main)/+page.svelte` — `onboarding-state.svelte.ts` owns the step cursor, not an `open` field. So there's no
store to seed and nothing the harness can render: `onboarding-preview.ts` dispatches the app's own re-entry command
(`cmdr.openOnboarding`, the same one the menu and the palette use) and then moves the step cursor.

The step jump is what makes the wizard reviewable at all: it always opens at step 1, and step 1 refuses to advance until
the user commits to Allow (which wants an app restart) or Deny (which persists a real choice), so steps 2-4 are
otherwise unreachable without changing this machine's FDA setting. Each step's own content stays real — variant and
banner are computed from the live FDA probe when the wizard opens.

The dispatch is **awaited end to end**: `CommandDispatchDialogs.openOnboarding` returns a `Promise` so the handler and
`+page.svelte`'s opener (which loads settings and probes FDA before the wizard exists) can be waited on. Setting the
step before that lands would be overwritten by `openWizard()`. The gallery holds no open-state for the wizard and has
nothing to restore: it closes on its own terms, and it has no Escape affordance by design.

## `drive-index-stale` is event-seeded

`StaleDriveDialog` takes no props and owns its `open` flag. It shows only when an `index-freshness-changed` event lands
with `freshness: 'stale'` for a non-`root` volume, AND `indexing.staleNotify` is on, AND the persisted one-shot flag is
still clear. `stale-drive-preview.ts` arranges those three and emits the real event through the typed
`emitIndexFreshnessChanged` wrapper; the dialog's own listener does the rest, so the shipping trigger path is what runs.
`listener-setup.ts` routes the row before the harness sees it, the same way `onboarding` is routed.

**The JS→JS round trip works.** A frontend `emit` goes through the Tauri backend and comes back to every webview's
`listen`, so the dialog's listener hears an event the main window emitted (verified by hand in a dev build, 2026-07-22:
Debug > Soft dialogs > Stale drive index opens the dialog, twice in a row). Nothing about it is main-window-specific;
what IS main-window-specific is picking the volume.

**The volume comes from the live store, filtered by the app's own `isDriveRow`.** `volumeName()` falls back to the raw
id for a volume that isn't in the store, so a synthetic id renders as `vol-abc123` in the body copy: a preview that
looks fine and is silently wrong, which is the failure this instrument exists to prevent. `isDriveRow`
(`file-explorer/navigation/drive-index-manager.svelte.ts`) is the app's own chokepoint for "a real drive that can carry
an index badge", so the preview can't name a favorite, the synthetic `network` / `search-results` rows, or a disk image.
`root` is excluded because the dialog itself ignores it (the local disk is journaled and never goes stale). Store order
decides the rest, which puts an attached drive ahead of a network share.

**With no drive to name, it opens nothing and says so** in a toast. A disclosed refusal beats a preview with an id in
the body: the row is honest either way, and the reviewer knows what to do about it.

**Both writes are real and stay.** The preview turns `indexing.staleNotify` back on if it was off, and clears the
one-shot before EVERY trigger (`resetFirstStaleDialogShown`, a dev-only export the product never calls) — the dialog
stamps that flag the moment it shows, so without the reset the row would work once per machine. Nothing is restored
afterwards: the dialog writes both itself ("Never show again" turns the setting off; showing stamps the one-shot), so a
restore would fight the component. The row discloses them instead. The freshness badge doesn't move either — replaying
the event changes no backend state, and `drive-index-manager` reacts by refetching the volume's real status.

## The disk-backed dialogs

`delete-confirmation`, `transfer-confirmation`, `mkdir-confirmation`, `new-file-confirmation`, and `go-to-path` do real
work on mount: background scans, folder-suggestion streams, volume-space queries, conflict lookups, path resolution.
Faking that would fake the very numbers the design displays, so they run against a real throwaway directory instead.

**The tree.** `src-tauri/src/dev_fixtures.rs` (a `#[cfg(debug_assertions)]` module) creates
`<app data dir>/dialog-gallery-fixtures/`: 34 files across nested folders, sizes from 412 bytes to 486 MB, including a
195-character filename, non-ASCII folder and file names, and an empty folder. Everything above the first line of content
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
   to `DialogGallery.svelte`'s `planResolvers` table plus a branch in its template. A store-seeded dialog instead sets
   `openedBy: 'store-seeded'`, holds PATCHES in `fixtures/store-seeded.ts`, and adds its store binding to
   `buildStoreSeed`; it needs no template branch, since the app renders it. An event-seeded one sets
   `openedBy: 'event-seeded'`, holds the event payload in its fixture record, and gets a preview module plus a branch in
   `listener-setup.ts` (`stale-drive-preview.ts` is the worked example); it renders nothing here either. A dialog that
   does real work on mount sets `usesFixtureDir: true` on its row and holds BUILDERS in `fixtures/disk.ts` instead of
   literals (see "The disk-backed dialogs"). Two tests cover the seams: `fixtures.test.ts` walks `fixtureRecords`
   against the registry (a state id with no fixture, or a fixture with no row), and `DialogGallery.svelte.test.ts`
   mounts EVERY advertised state and asserts the dialog reported its own id to the tracker. A dead button fails there
   rather than mid-review.
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

The store-seeded rows are where this bites hardest, because the whole point is that the real component runs: `feedback`
and `error-report` really send (to `localhost:8787` in a dev build, so with no local api-server up you get the
send-failed state instead of a message in Discord), `error-report` builds a real redacted bundle from this machine's
logs on mount and has a dev-only "Save bundle to disk" button that writes a zip, `whats-new`'s opt-out writes the real
`whatsNew.showOnUpdate` setting, and expanding an `operation-log` row fetches its items over the real IPC (a fixture
operation isn't in the log, and the detail command answers `None` for an unknown id, so every row expands to "no
recorded items"). `onboarding` is the extreme case: every step's buttons do exactly what they do on first launch,
including recording the FDA choice and signing up for the beta.

## The gap rows

Two registered dialogs have no button, and the reason each row prints is the point: a reader who doesn't know the
internals has to be able to tell a real obstacle from a choice we made.

- **`transfer-progress` is genuinely blocked.** Every phase it shows — the scan, the two bars, pause and queue, the
  flush, and the conflict section it embeds — is driven by a live operation on the backend's `write-progress` /
  `write-conflict` / `write-error` / `write-cancelled` / `write-settled` stream. Choosing a phase from the gallery needs
  a script that replays that stream, which nobody has written. `TransferConflictDialog` is folded into this gap rather
  than getting its own row: it renders a bare `.conflict-section` designed to sit inside `TransferProgressDialog`'s
  body, so standalone it would need the gallery to fake its parent's chrome. The row also says what you CAN do — start a
  real copy — because "not triggerable" must not read as "unreachable".
- **`search` is deferred, not blocked, and the row says exactly that.** `SearchDialog` takes plain props, the drive
  index is live in dev, and ⌘F (or the MCP `open_search_dialog` tool) opens it right now. ❌ Never dress this up as a
  technical obstacle: publishing a false reason inside an instrument whose thesis is "must not lie" is the worst
  available outcome, and it would be invisible to everyone who doesn't already know the dialog.

The three `UNREGISTERED_OVERLAY_ENTRIES` rows exist so the inventory can't imply nothing else is modal-looking. Each
says why it isn't a registered soft dialog and how to evoke it by hand: the command palette is its own overlay (⌘⇧P by
default) and reports nothing to the dialog tracker; `NetworkLoginForm` isn't modal at all (it renders inside a pane,
which is why it's the one sanctioned opt-out from the dialog focus trap); the pane volume chooser is a pane-owned
dropdown (⌥F1 / ⌥F2). Keep the shortcuts honest: they're user-rebindable defaults.

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

## The one dialog we extracted

`delete-ai-model` used to be an inline `<ModalDialog>` inside `AiLocalSection.svelte`, so nothing could open it but the
settings section itself. It now lives in `settings/sections/DeleteAiModelDialog.svelte` with `modelSizeFormatted` /
`isDeleting` / `onConfirm` / `onCancel` props, and `AiLocalSection` uses it. The extraction is behaviour-preserving on
purpose (same `dialogId`, `role="alertdialog"`, Enter handling, and disabled-while-deleting logic, pinned by its own
`a11y.test.ts`) — it's a plain refactor that happens to make the dialog reachable, not a preview affordance.

## The coverage check

`scripts/check/checks/desktop-svelte-dialog-gallery-coverage.go` compares `{ id: '…' }` in `lib/ui/dialog-registry.ts`
against `dialogId: '…'` in `gallery-registry.ts`, both directions. Nested state objects use `id` (not `dialogId`) and
unregistered overlays use `overlayId`, so neither can be mistaken for a dialog id. Without this check the gallery
silently stops being an inventory the first time someone adds a dialog, and a review that trusts it reviews the wrong
set.
