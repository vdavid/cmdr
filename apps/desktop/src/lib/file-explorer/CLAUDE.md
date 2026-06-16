# File explorer module

Dual-pane file explorer with keyboard-driven navigation, file selection, sorting, command palette, and adaptive layout.

## Module map

- `selection/`, `navigation/`, `rename/`, `operations/`, `views/`, `tabs/`, `git/`, `network/`, `quick-look/`: the
  feature subdirectories (each has its own colocated docs).
- `pane/`: `DualPaneExplorer` + `FilePane` + dialog manager + per-pane state. Owns type-to-jump, live disk space, the
  `navigate()` transaction, volume capabilities, and the error-display pipeline (see
  [`pane/CLAUDE.md`](pane/CLAUDE.md)).
- Sorting and the command palette have no own directory; their detail is in [DETAILS.md](DETAILS.md).

## Must-knows

- **Selection's `SvelteSet` needs mutation, not reassignment.** `selectedIndices.add(i)` works;
  `selectedIndices = new SvelteSet([i])` breaks reactivity. Selection state is a `SvelteSet<number>` in `FilePane`.
- **Parent offset**: when `hasParent`, frontend indices = backend indices + 1 (index 0 is the synthetic `..`). Selection
  diff/cursor/type-to-jump code all apply this; forgetting it lands the cursor or selection one row off.
- **The selection snapshot for an operation happens at CONFIRM, not when the progress dialog opens.** Same-FS moves are
  instant and may finish before the dialog mounts. `startTransferProgress` (clipboard paste), `handleTransferConfirm`,
  and `handleDeleteConfirm` all snapshot. A `diffGeneration` counter discards stale async diff/selection results.
- **`allSelected: true` is an IPC optimization** to avoid shipping 500k indices. On cancel it calls `selectAll()` for
  move/delete/trash (source listing changed) but leaves copy untouched (source unchanged); keep that asymmetry.
- **Don't load full listings into Svelte `$state`.** `FileDataStore` is deliberately non-reactive (only the visible
  ~50-100 items enter reactivity). Loading 20k+ entries into `$state` causes 9+ second freezes even with virtual
  scrolling. See `views/CLAUDE.md` and the benchmarks in `docs/notes/non-reactive-file-store.md`.
- **File entries carry an `iconId` ref, never icon blobs.** Icons come from a separate `get_icons()` call cached in
  IndexedDB; inlining 50k icon blobs would transmit ~100-200 MB.
- **Root-layout HMR can trigger a SvelteKit TDZ crash** (`Cannot access 'component' before initialization`,
  sveltejs/kit#15287) when an update propagates through `+layout.svelte` (for example `app.css` edits).
  `$lib/hmr-recovery.ts`, imported from the stable `+layout.ts`, catches it and forces a clean reload. Don't remove it
  until the upstream bug is fixed.
- **The stale-listing token + drop-foreign-listings policy is what keeps navigation state uncorruptible** when a pane
  flips volume between `listing-start` and `listing-complete`. Both mechanisms live in `pane/navigate.ts`; if you add a
  virtual-volume namespace with a non-filesystem prefix, extend the explicit prefix branch in `commitPathFromListing`.
  Full contract in [DETAILS.md](DETAILS.md) § Gotchas and [`pane/CLAUDE.md`](pane/CLAUDE.md).
- **Error/provider WORDS live on the FE** (`$lib/errors/`), error CLASSIFICATION in Rust. Rust ships a typed, word-free
  `ListingError` (reason + params + category + provider); the FE factories render the copy. To change wording, edit
  `$lib/errors/` (and keep the parity test green); to add a reason/provider, change both sides. See
  [`$lib/errors/CLAUDE.md`](../errors/CLAUDE.md).

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
