# Dialog gallery (dev-only)

Opens every registered soft dialog on demand with fixture data, for design review without staging the real conditions.
Driven from Debug > Soft dialogs (`routes/debug/DebugDialogsPanel.svelte`).

## Module map

- `gallery-registry.ts`: `DIALOG_GALLERY_ENTRIES` (one row per `SOFT_DIALOG_REGISTRY` id) +
  `UNREGISTERED_OVERLAY_ENTRIES` (modal-looking overlays that aren't soft dialogs).
- `gallery-state.svelte.ts`: the open-state store. `DialogGallery.svelte`: the main-window harness. `fixtures/`: fixture
  data per dialog, keyed dialog id → record in `fixtures/index.ts`.
- `disk-fixture.ts`: the real fixture directory (dev-only Rust `dev_fixtures`) plus the focused pane's live listing, for
  the five that do real work on mount.
- `store-seeding.ts` + `fixtures/store-seeded.ts`: patch a real app store and undo it, for the five the app itself
  mounts. `onboarding-preview.ts` / `stale-drive-preview.ts`: the two an app command or a real event opens.

## Must-knows

- **A design-review instrument must not lie.** ❌ Never add a `preview` prop or a dev-only branch to a dialog component:
  you'd stop reviewing the shipping component. Pass real props, seed the real store, or emit the real event. A dialog
  you can't reach honestly gets `status: 'not-triggerable'` and a TRUE reason: ❌ never a technical excuse for one
  that's merely unwired (`search`).
- **The dialogs render in the MAIN window**, mounted from `routes/(main)/+layout.svelte` (never `+page.svelte`, already
  over its file-length entry). A Debug-window copy would report a phantom open dialog to the Rust `SoftDialogTracker`
  and lose the two-pane backdrop the overlay needs.
- **Copy here stays raw and out of the i18n catalog**, which is why fixtures live under `lib/` rather than in
  i18n-enforced `routes/(main)/`.
- **The harness, its fixtures, and the dialogs they pull in tree-shake out of prod**; `gallery-registry.ts` doesn't (it
  rides the Debug route's chunk). Keep `gallery-state.svelte.ts`, the only module `+page.svelte` imports,
  dependency-free: no registry, no fixtures, no dialog imports.
- **Adding a soft dialog means adding a gallery row**, enforced by `dialog-gallery-coverage` (id presence only), and
  **its fixture record belongs in `fixtures/index.ts`**: harness and `fixtures.test.ts` both read `fixtureRecords`, so
  "state id ↔ fixture key" drift is a test failure, not a dead button.
- **Read the dialog's props before classifying it.** One that reads a module store and takes no content props renders
  EMPTY from the harness: it's store-seeded (`openedBy`), not prop-driven. Ditto the two an app command or a real event
  opens. Verify per dialog, don't guess.
- **A store-seeded preview must never leave the app half-seeded**, so the undo is STRUCTURAL: `seedStore` derives it
  from the patch's keys, and the harness runs it as an `$effect` cleanup. ❌ No hand-written per-fixture cleanup, and
  keep the `untrack` around `apply()`.
- **`drive-index-stale` must name a volume that's really in the store** (`isDriveRow`, minus `root`): `volumeName()`
  falls back to the raw id, so a synthetic one renders as `vol-abc123`. ❌ No drive mounted means no preview, never a
  stand-in.
- **Fixture callbacks close the preview and do nothing else** — but where the action lives INSIDE the component it still
  happens for real: flag and settings writes, license activation, mDNS, report sends, first-launch choices. Those rows
  carry a `note`. ❌ Never silence one with a preview branch.
- `lib/dialog-gallery/` is NOT exempt from `cmdr/no-raw-tauri-invoke` / `no-raw-bindings-import`. A fixture's IPC is
  called from `DebugDialogsPanel.svelte` (an exempt path) and ferried in the event payload.
- **`mkdir-confirmation` / `new-file-confirmation` need a pane-owned `listingId`, not a path**, and they really WRITE,
  which is what the fixture directory protects. ❌ Never fabricate an id: the conflict check then fails SILENTLY.
  `DeleteDialog` / `TransferDialog` take `onConfirm` as a prop and perform nothing, so a no-op is harmless; the
  intuitive version is backwards.

Adding an entry, the open mechanisms, the gap rows, and the transport: [DETAILS.md](DETAILS.md).
