/**
 * Unit coverage for the arg-carrying dispatch shape.
 *
 * `view.setMode` is the first command that carries typed args (`{ pane, mode }`).
 * This pins that `handleCommandExecute('view.setMode', ctx, args)` reaches the
 * right per-pane primitive (`setViewModeFromMenu`) with the args intact — the
 * native-menu `view-mode-changed` event's path onto the bus. The routes file has
 * no coverage gate, so this is a behavioral guard, not a coverage filler.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

// `getAppLogger('user-action')` runs at module top-level in command-dispatch,
// and the dispatch preamble fires `record_breadcrumb` via `invoke`. Mock both so
// the import is side-effect-free and the breadcrumb IPC is a no-op.
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: vi.fn(), debug: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }))
// The capability guard reads the focused pane's volume id; `getVolumeId` is a
// reconfigurable mock so each test picks the kind (`local` keeps dispatch flowing
// to the handler; `search-results` / `network` exercise the guard).
const getVolumeId = vi.fn<() => string>(() => 'local')
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPaneVolumeId: () => getVolumeId(),
  getFocusedPanePath: () => '/Users/test',
}))
// `capabilitiesFor` resolves a real volumeId's fsType/category from the store;
// the virtual ids (`search-results` / `network`) short-circuit before this. An
// empty store makes `local` fall to the listable default (canWrite: true).
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))
const addToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToast(...args)
  },
}))

const openSettingsWindow = vi.fn()
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: (...args: unknown[]) => {
    openSettingsWindow(...args)
    return Promise.resolve()
  },
}))

import { handleCommandExecute, type CommandDispatchContext } from './command-dispatch'
import type { DialogsOnScreen, DispatchSource } from './command-dispatch-context'
import { SEARCH_RESULTS_NOT_A_FOLDER_TOAST } from '$lib/search/capabilities'
import type { ExplorerAPI } from './explorer-api'

const NOTHING_OPEN: DialogsOnScreen = { dialogOpen: false, paletteOpen: false }
const DIALOG_OPEN: DialogsOnScreen = { dialogOpen: true, paletteOpen: false }

function makeCtx(
  explorer: Partial<ExplorerAPI>,
  { source = 'palette', onScreen = NOTHING_OPEN }: { source?: DispatchSource; onScreen?: DialogsOnScreen } = {},
): CommandDispatchContext {
  return {
    getExplorer: () => explorer as ExplorerAPI,
    dialogs: {
      showCommandPalette: vi.fn(),
      showSearchDialog: vi.fn(),
      showGoToPathDialog: vi.fn(),
      showAboutWindow: vi.fn(),
      showLicenseKeyDialog: vi.fn(),
      showSelectionDialog: vi.fn(),
      openOnboarding: vi.fn(),
    },
    source,
    getDialogsOnScreen: () => onScreen,
  }
}

/**
 * The dialog gate in the core. Before it, only the keyboard road knew about dialogs, so a
 * native-menu accelerator (⌘T, ⌘W, ⌘K) reached its handler behind an open one.
 */
describe('handleCommandExecute — the dialog gate', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getVolumeId.mockReturnValue('local')
  })

  it('refuses a pane command from the native menu while a dialog is up', async () => {
    const newTab = vi.fn(() => true)
    await handleCommandExecute('tab.new', makeCtx({ newTab }, { source: 'menu', onScreen: DIALOG_OPEN }))
    expect(newTab).not.toHaveBeenCalled()
  })

  it('runs the same command from the native menu with nothing open', async () => {
    const newTab = vi.fn(() => true)
    await handleCommandExecute('tab.new', makeCtx({ newTab }, { source: 'menu', onScreen: NOTHING_OPEN }))
    expect(newTab).toHaveBeenCalledOnce()
  })

  it('runs Settings over a dialog, since it opens its own window', async () => {
    await handleCommandExecute('app.settings', makeCtx({}, { source: 'menu', onScreen: DIALOG_OPEN }))
    expect(openSettingsWindow).toHaveBeenCalledOnce()
  })

  it('lets MCP through, since its tools answer for themselves', async () => {
    const newTab = vi.fn(() => true)
    await handleCommandExecute('tab.new', makeCtx({ newTab }, { source: 'mcp', onScreen: DIALOG_OPEN }))
    expect(newTab).toHaveBeenCalledOnce()
  })

  it("doesn't count the palette against its own row", async () => {
    const newTab = vi.fn(() => true)
    const onScreen: DialogsOnScreen = { dialogOpen: false, paletteOpen: true }
    await handleCommandExecute('tab.new', makeCtx({ newTab }, { source: 'palette', onScreen }))
    expect(newTab).toHaveBeenCalledOnce()
  })
})

describe('handleCommandExecute — view.setMode (arg-carrying dispatch)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('fromMenu: true routes to setViewModeFromMenu, not setViewMode', async () => {
    const setViewModeFromMenu = vi.fn()
    const setViewMode = vi.fn()
    const ctx = makeCtx({ setViewModeFromMenu, setViewMode })

    await handleCommandExecute('view.setMode', ctx, { pane: 'right', mode: 'brief', fromMenu: true })

    expect(setViewModeFromMenu).toHaveBeenCalledExactlyOnceWith('right', 'brief')
    // The menu already toggled its CheckMenuItem, so the focused-pane setter
    // (which would push menu state) must NOT run.
    expect(setViewMode).not.toHaveBeenCalled()
  })

  it('fromMenu: false routes to setViewMode (the MCP path that pushes menu state)', async () => {
    const setViewModeFromMenu = vi.fn()
    const setViewMode = vi.fn()
    const ctx = makeCtx({ setViewModeFromMenu, setViewMode })

    await handleCommandExecute('view.setMode', ctx, { pane: 'left', mode: 'full', fromMenu: false })

    // `setViewMode(mode, pane)` — note arg order — pushes the menu state since
    // nothing toggled it (the MCP `set_view_mode` tool's byte-identical path).
    expect(setViewMode).toHaveBeenCalledExactlyOnceWith('full', 'left')
    expect(setViewModeFromMenu).not.toHaveBeenCalled()
  })
})

/**
 * The capability guard (`blockedByCapabilities`): destination-side ops are blocked
 * on a `search-results` pane (with the L10 toast), allowed on a `local` pane, and
 * — the PR3 edge — silently allowed-through on a `network` pane (its caps are also
 * false, but the toast is search-results-only; network historically fell through
 * to the explorer no-op). The blocked set is paste / pasteAsMove / newFolder /
 * newFile / rename; copy/move/delete (source ops) stay enabled on every kind.
 */
describe('handleCommandExecute — blockedByCapabilities (search-results / network guard)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    getVolumeId.mockReturnValue('local')
  })

  /** A ctx whose explorer records whether each guarded destination op ran. */
  function makeGuardCtx() {
    const pasteFromClipboard = vi.fn()
    const openNewFolderDialog = vi.fn()
    const openNewFileDialog = vi.fn()
    const startRename = vi.fn()
    const ctx = makeCtx({ pasteFromClipboard, openNewFolderDialog, openNewFileDialog, startRename })
    return { ctx, pasteFromClipboard, openNewFolderDialog, openNewFileDialog, startRename }
  }

  it('blocks each destination-side id on a search-results pane and toasts the L10 string', async () => {
    getVolumeId.mockReturnValue('search-results')

    for (const id of ['edit.paste', 'edit.pasteAsMove', 'file.newFolder', 'file.newFile', 'file.rename'] as const) {
      addToast.mockClear()
      const g = makeGuardCtx()
      const ranBy = {
        'edit.paste': g.pasteFromClipboard,
        'edit.pasteAsMove': g.pasteFromClipboard,
        'file.newFolder': g.openNewFolderDialog,
        'file.newFile': g.openNewFileDialog,
        'file.rename': g.startRename,
      }[id]

      await handleCommandExecute(id, g.ctx)

      expect(ranBy, `${id} must be blocked before the explorer call`).not.toHaveBeenCalled()
      // The exact string, byte-for-byte.
      expect(addToast, `${id} must surface the toast`).toHaveBeenCalledExactlyOnceWith(
        SEARCH_RESULTS_NOT_A_FOLDER_TOAST,
        { level: 'info' },
      )
    }
  })

  it('keeps source-side ops enabled on a search-results pane (canBeSource: true)', async () => {
    getVolumeId.mockReturnValue('search-results')
    const openCopyDialog = vi.fn()
    const openMoveDialog = vi.fn()
    const openDeleteDialog = vi.fn()
    const ctx = makeCtx({ openCopyDialog, openMoveDialog, openDeleteDialog })

    await handleCommandExecute('file.copy', ctx)
    await handleCommandExecute('file.move', ctx)
    await handleCommandExecute('file.delete', ctx)

    expect(openCopyDialog).toHaveBeenCalledOnce()
    expect(openMoveDialog).toHaveBeenCalledOnce()
    expect(openDeleteDialog).toHaveBeenCalledOnce()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('allows each destination-side id on a local pane (no toast, explorer runs)', async () => {
    getVolumeId.mockReturnValue('local')
    const g = makeGuardCtx()

    await handleCommandExecute('edit.paste', g.ctx)
    await handleCommandExecute('file.newFolder', g.ctx)
    await handleCommandExecute('file.rename', g.ctx)

    expect(g.pasteFromClipboard).toHaveBeenCalledOnce()
    expect(g.openNewFolderDialog).toHaveBeenCalledOnce()
    expect(g.startRename).toHaveBeenCalledOnce()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('exempts edit.paste into a focused text input, even on a search-results pane', async () => {
    // Pasting into a dialog's text field is a TEXT op: the pane behind it is
    // irrelevant, so neither the block nor the toast may fire. Before the modal
    // keydown fix WebKit's native paste papered over this; now the dispatch is
    // the only actor, so a block here would swallow the paste entirely.
    getVolumeId.mockReturnValue('search-results')
    const input = document.createElement('input')
    document.body.appendChild(input)
    input.focus()
    const g = makeGuardCtx()

    await handleCommandExecute('edit.paste', g.ctx)

    expect(addToast).not.toHaveBeenCalled()
    // The handler took its own text-input branch, so the pane paste never ran.
    expect(g.pasteFromClipboard).not.toHaveBeenCalled()
    input.remove()
  })

  it('still blocks edit.pasteAsMove from a focused text input (its handler always drives the pane)', async () => {
    getVolumeId.mockReturnValue('search-results')
    const input = document.createElement('input')
    document.body.appendChild(input)
    input.focus()
    const g = makeGuardCtx()

    await handleCommandExecute('edit.pasteAsMove', g.ctx)

    expect(g.pasteFromClipboard).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledExactlyOnceWith(SEARCH_RESULTS_NOT_A_FOLDER_TOAST, { level: 'info' })
    input.remove()
  })

  it('does NOT toast on a network pane and falls through to the explorer (PR3 byte-identical silence)', async () => {
    // network caps are also false for these ops, but the toast is search-results
    // only; the old string-compare guard never fired on network, so the keystroke
    // historically fell through to the explorer call (which no-ops deep down).
    getVolumeId.mockReturnValue('network')
    const g = makeGuardCtx()

    await handleCommandExecute('edit.paste', g.ctx)
    await handleCommandExecute('file.newFolder', g.ctx)
    await handleCommandExecute('file.rename', g.ctx)

    expect(addToast).not.toHaveBeenCalled()
    expect(g.pasteFromClipboard).toHaveBeenCalledOnce()
    expect(g.openNewFolderDialog).toHaveBeenCalledOnce()
    expect(g.startRename).toHaveBeenCalledOnce()
  })
})
