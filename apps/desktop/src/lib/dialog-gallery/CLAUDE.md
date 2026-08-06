# Dialog gallery (dev + capture builds)

Opens every registered soft dialog on demand with fixture data, for design review without staging the real conditions.
Two drivers: Debug > Soft dialogs (`routes/debug/DebugDialogsPanel.svelte`), and the i18n screenshot capture
(`apps/desktop/test/e2e-playwright/i18n-capture-gallery.ts`), which walks the same registry to shoot dialogs for
translators.

## Module map

- `gallery-registry.ts`: `DIALOG_GALLERY_ENTRIES` (one row per `SOFT_DIALOG_REGISTRY` id) +
  `UNREGISTERED_OVERLAY_ENTRIES` (modal-looking overlays that aren't soft dialogs).
- `gallery-state.svelte.ts`: the open-state store. `DialogGallery.svelte`: the main-window harness. `fixtures/`: fixture
  data per dialog, keyed dialog id → record in `fixtures/index.ts`.
- `disk-fixture.ts`: the real fixture directory (debug-only Rust `dev_fixtures`) plus the focused pane's live listing,
  for the five that do real work on mount.
- `store-seeding.ts` + `fixtures/store-seeded.ts`: patch a real app store and undo it, for the five the app mounts
  itself. `onboarding-preview.ts` / `stale-drive-preview.ts`: the two an app command or a real event opens.

## Must-knows

- **A design-review instrument must not lie.** ❌ Never add a `preview` prop or a dev-only branch to a dialog component:
  you'd stop reviewing the shipping one. Pass real props, seed the real store, or emit the real event. A dialog you
  can't reach honestly gets `status: 'not-triggerable'` and a TRUE reason, ❌ never a technical excuse.
- **The dialogs render in the MAIN window**, mounted from `routes/(main)/+layout.svelte` (never `+page.svelte`, already
  over its file-length entry). A Debug-window copy would report a phantom open dialog to the Rust `SoftDialogTracker`
  and lose the two-pane backdrop.
- **Copy here stays raw and out of the i18n catalog**, which is why fixtures live under `lib/`, not i18n-enforced
  `routes/(main)/`.
- **The harness, its fixtures, and the dialogs they pull in tree-shake out of prod**; `gallery-registry.ts` doesn't (it
  rides the Debug route's chunk). Keep `gallery-state.svelte.ts`, the only module `+page.svelte` imports,
  dependency-free: no registry, no fixtures, no dialog imports.
- **The gate is `import.meta.env.DEV || __CMDR_DIALOG_GALLERY__`** (`+layout.svelte`, `listener-setup.ts`), the define
  every capture AND E2E build sets. ❌ Never narrow a site to `DEV` or to `__CMDR_I18N_CAPTURE__`: both of those builds
  are prod Vite builds, so the dialog screenshots go silently to zero and `dialog-inset.spec.ts` stops measuring.
- **Adding a soft dialog means adding a gallery row**, enforced by `dialog-gallery-coverage` (id presence only), and
  **its fixture record belongs in `fixtures/index.ts`**: harness and `fixtures.test.ts` both read `fixtureRecords`, so
  "state id ↔ fixture key" drift is a test failure, not a dead button.
- **Read the dialog's props before classifying it.** One that reads a module store and takes no content props renders
  EMPTY from the harness: it's store-seeded (`openedBy`), not prop-driven. Verify per dialog, don't guess.
- **A store-seeded preview must never leave the app half-seeded**, so the undo is STRUCTURAL: `seedStore` derives it
  from the patch's keys, run as an `$effect` cleanup. ❌ No per-fixture cleanup; keep the `untrack` around `apply()`.
- **`drive-index-stale` must name a volume that's really in the store** (`isDriveRow`, minus `root`): `volumeName()`
  falls back to the raw id. ❌ No drive mounted means no preview, never a stand-in.
- **Fixture callbacks close the preview and do nothing else** — but an action living INSIDE the component still happens
  for real (settings writes, license activation, mDNS, report sends). Those rows carry a `note`. ❌ Never silence one
  with a preview branch.
- **`mkdir-confirmation` / `new-file-confirmation` need a pane-owned `listingId`, not a path**, and they really WRITE,
  which is what the fixture directory protects. ❌ Never fabricate an id: the conflict check then fails SILENTLY.

Adding an entry, the open mechanisms, the gap rows, and the transport: `DETAILS.md`.
