# Dialog gallery (dev-only)

Opens every registered soft dialog on demand, filled with fixture data, so the dialogs can be design-reviewed without
staging the real conditions. Driven from Debug > Soft dialogs (`routes/debug/DebugDialogsPanel.svelte`).

## Module map

- `gallery-registry.ts`: `DIALOG_GALLERY_ENTRIES` (one row per `SOFT_DIALOG_REGISTRY` id) +
  `UNREGISTERED_OVERLAY_ENTRIES` (modal-looking overlays that aren't soft dialogs).
- `gallery-state.svelte.ts`: the open-state store. `DialogGallery.svelte`: the main-window harness. `fixtures/`: fixture
  data per dialog, with `fixtures/index.ts` mapping dialog id → fixture record.
- `disk-fixture.ts`: the real fixture directory (dev-only Rust `dev_fixtures`) plus the focused pane's live listing, for
  the five dialogs that do real work on mount.
- `store-seeding.ts` + `fixtures/store-seeded.ts`: patch a real app store and undo it, for the five the app itself
  mounts. `onboarding-preview.ts`: the one row that opens via the app's own command.

## Must-knows

- **A design-review instrument must not lie.** ❌ Never add a `preview` prop or a dev-only branch to a dialog component
  to make it previewable: you'd stop reviewing the shipping component. Pass real props, seed the real store, or emit the
  real event. A dialog you can't reach honestly gets `status: 'not-triggerable'` and a true reason.
- **The dialogs render in the MAIN window**, mounted from `routes/(main)/+layout.svelte` (never `+page.svelte`, which is
  already over its file-length allowlist entry). Rendering a copy in the Debug window would report a phantom open dialog
  to the Rust `SoftDialogTracker` and lose the two-pane backdrop the overlay is designed for.
- **Copy here stays raw and out of the i18n catalog.** That's why fixtures live under `lib/` rather than in
  `routes/(main)/`, which is i18n-enforced.
- **Everything is gated on `import.meta.env.DEV`** so prod tree-shakes it. `gallery-state.svelte.ts` is the only module
  `+page.svelte` imports, so keep it dependency-free (no registry, no fixtures, no dialog imports).
- **Adding a soft dialog means adding a gallery row**, enforced by `dialog-gallery-coverage` (id presence only).
- **Read the dialog's props before classifying it.** One that reads a module store and takes no content props renders
  EMPTY from the harness: it's store-seeded (`openedBy`), not prop-driven. Verify per dialog, don't guess.
- **A store-seeded preview must never leave the app half-seeded**, so the undo is STRUCTURAL: `seedStore` derives it
  from the patch's keys, the harness runs it as an `$effect` cleanup, and a second effect watches `isOpen()` (those
  dialogs close through their own store). ❌ Don't hand-write per-fixture cleanup, and keep the `untrack` around
  `apply()`.
- **Register every fixture record in `fixtures/index.ts`.** The harness and `fixtures.test.ts` both read
  `fixtureRecords`, which is what makes "state id ↔ fixture key" drift a test failure instead of a dead button.
- **Fixture callbacks close the preview and do nothing else.** But where the action lives INSIDE the component it still
  happens for real: `commercial-reminder` / `expiration` / `whats-new` write their real flags, `license` activates keys,
  `connect-to-server` fires mDNS, feedback and error reports really send (dev builds POST to `localhost:8787`), and
  `onboarding` records real first-launch choices. Those rows carry a `note`. ❌ Never silence one with a preview branch.
- `lib/dialog-gallery/` is NOT exempt from `cmdr/no-raw-tauri-invoke` / `no-raw-bindings-import`. IPC a fixture needs is
  called from `DebugDialogsPanel.svelte` (an exempt path) and ferried in the event payload.
- **`mkdir-confirmation` / `new-file-confirmation` need a pane-owned `listingId`, not a path**, and they really WRITE
  (they call `createDirectory` / `createFile`), which is what the fixture directory protects. ❌ Never fabricate an id:
  the conflict check then misbehaves SILENTLY, so no id means no preview. `DeleteDialog` / `TransferDialog` take
  `onConfirm` as a prop and perform nothing, so a no-op is harmless wherever they point — the intuitive version of this
  is backwards.

Adding an entry, the three open mechanisms, and the transport: [DETAILS.md](DETAILS.md).
