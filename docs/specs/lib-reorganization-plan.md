# Reorganize `apps/desktop/src/lib/`

The `lib/` directory has grown organically. Root-level files that belong to features sit alongside truly cross-cutting
utilities, and `file-explorer/` is a 46-file flat folder. This plan reorganizes without changing any behavior.

**Constraint**: The ongoing `DualPaneExplorer` split (see `split-up-big-files-plans.md`) touches
`DualPaneExplorer.svelte`, `dialog-state.ts`, `copy-operations.ts`, `sorting-handlers.ts`, and
`DialogManager.svelte`. Coordinate with that work — do file-explorer reorganization after that split lands.

---

## Milestone 1: Clean up `lib/` root

Move ~14 root-level files into feature directories. Keep only truly cross-cutting files at root.

### Files staying at root

| File | Reason |
|---|---|
| `logger.ts` | Used everywhere |
| `benchmark.ts` | Dev utility, used everywhere |
| `app-status-store.ts` | App-wide state persistence |
| `window-state.ts` | App-level window management |
| `tauri-commands.ts` | Re-export barrel for `tauri-commands/` |

### New `ai/` directory

Move these from root:

- `AiNotification.svelte`
- `AiNotification.test.ts`
- `ai-state.svelte.ts`
- `ai-state.test.ts`

### New `updates/` directory

Move these from root:

- `UpdateNotification.svelte`
- `updater.svelte.ts`

### New `ui/` directory (shared UI primitives)

Move these from root:

- `AlertDialog.svelte`
- `LoadingIcon.svelte`
- `streaming-loading.test.ts` (tests LoadingIcon behavior in context)

### Move into existing directories

- `licensing-store.svelte.ts` → `licensing/`
- `settings-store.ts` → `settings/` (it's the legacy settings.json store)

### Remaining root files (decide per file)

- `drag-drop.ts` — used by file-explorer. Move to `file-explorer/` unless other features use it.
- `icon-cache.ts` — used by file-explorer. Same as above.
- `network-store.svelte.ts` — used by file-explorer's network browsing. Move to `file-explorer/network/` (see
  milestone 3).

---

## Milestone 2: Rename `write-operations/` → `file-operations/`

Rename the directory and organize by operation type for future growth.

### New structure

```
file-operations/
├── copy/
│   ├── CopyDialog.svelte
│   ├── CopyProgressDialog.svelte
│   ├── CopyErrorDialog.svelte
│   ├── DirectionIndicator.svelte      # Shared with move later
│   ├── copy-dialog-utils.ts
│   ├── copy-dialog-utils.test.ts
│   ├── copy-error-messages.ts
│   └── copy-error-messages.test.ts
├── mkdir/                              # Moved from file-explorer
│   ├── NewFolderDialog.svelte
│   ├── new-folder-operations.ts
│   ├── new-folder-utils.ts
│   └── new-folder-utils.test.ts
├── move/                               # Future
└── delete/                             # Future
```

When `move` is added later, `DirectionIndicator.svelte` can move to a `shared/` subfolder.

**Rename stays in `file-explorer/`** — it's an in-place, single-file operation with fundamentally different UX (inline
editing in the list, no progress dialog).

---

## Milestone 3: Organize `file-explorer/` into subdirectories

Currently 46 files flat. Organize into ~6 subdirectories by domain.

### New structure

```
file-explorer/
├── types.ts                            # Central types, stays at root
├── test-helpers.ts                     # Shared test factories
│
├── views/                              # List rendering modes
│   ├── BriefList.svelte
│   ├── brief-list-utils.ts
│   ├── brief-list-utils.test.ts
│   ├── FullList.svelte
│   ├── full-list-utils.ts
│   ├── full-list-utils.test.ts
│   ├── file-list-utils.ts             # Shared between both modes
│   ├── file-list-utils.test.ts
│   ├── virtual-scroll.ts
│   ├── virtual-scroll.test.ts
│   └── view-modes.test.ts
│
├── navigation/                         # History + volume switching + keyboard nav
│   ├── VolumeBreadcrumb.svelte
│   ├── navigation-history.ts
│   ├── navigation-history.test.ts
│   ├── path-navigation.ts
│   ├── keyboard-shortcuts.ts
│   └── keyboard-shortcuts.test.ts
│
├── network/                            # Network browsing
│   ├── NetworkBrowser.svelte
│   ├── ShareBrowser.svelte
│   ├── NetworkLoginForm.svelte
│   └── network-store.svelte.ts        # Moved from lib/ root
│
├── pane/                               # Core pane orchestration
│   ├── DualPaneExplorer.svelte
│   ├── DualPaneExplorer.test.ts
│   ├── FilePane.svelte
│   ├── PaneResizer.svelte
│   ├── FunctionKeyBar.svelte
│   ├── FunctionKeyBar.test.ts
│   ├── PermissionDeniedPane.svelte
│   ├── copy-operations.ts             # Extracted from DualPaneExplorer
│   ├── copy-operations.test.ts
│   ├── sorting-handlers.ts            # Extracted from DualPaneExplorer
│   ├── sorting-handlers.test.ts
│   ├── new-folder-operations.ts       # Extracted from DualPaneExplorer
│   ├── dialog-state.ts                # Extracted from DualPaneExplorer
│   ├── DialogManager.svelte           # Extracted from DualPaneExplorer
│   └── integration.test.ts
│
├── selection/                          # Status bar + display helpers
│   ├── SelectionInfo.svelte
│   ├── selection-info-utils.ts
│   ├── selection-info-utils.test.ts
│   ├── FileIcon.svelte
│   ├── SortableHeader.svelte
│   └── components.test.ts
│
└── operations/                         # File watcher + diff application
    ├── apply-diff.ts
    └── apply-diff.test.ts
```

Tests co-locate with their modules.

---

## Milestone 4: Don't touch these (they're fine)

These directories are well-organized and don't need changes:

- `commands/` — clean data+logic layer, used by multiple consumers
- `command-palette/` — clean UI layer, correct separation from `commands/`
- `settings/` — exemplary 3-layer architecture
- `shortcuts/` — clean single-responsibility modules
- `tauri-commands/` — well-organized by domain
- `licensing/` — self-contained feature module
- `mtp/` — self-contained with proper store/utils/UI split
- `font-metrics/`, `file-viewer/`, `onboarding/`, `utils/` — small and focused

---

## Execution notes

- **Update all imports** after each move. Use IDE refactoring or search-and-replace `$lib/old-path` → `$lib/new-path`.
- **Run checks after each milestone**: `./scripts/check.sh --svelte` catches broken imports, unused exports, etc.
- **Update `coverage-allowlist.json`** if any moved files are listed there.
- **Coordinate milestone 3 with the DualPaneExplorer split** — that work creates files in `file-explorer/` root that
  this plan moves into `file-explorer/pane/`.
