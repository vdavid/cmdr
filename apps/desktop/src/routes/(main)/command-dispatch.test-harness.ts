/**
 * Shared harness for the `handleCommandExecute` characterization suites.
 *
 * Holds the reusable pieces both characterization test files need: the
 * `makeCtx` / `makeExplorerSpy` builders, the derived dispatchable / exempt id
 * sets, and the `DELEGATE_ROWS` table + `DelegateRow` type.
 *
 * ⚠️ This file holds NO `vi.mock(...)` calls. Vitest hoists `vi.mock` PER TEST
 * FILE; a mock declared in an imported helper would NOT be hoisted into the
 * importing test file's module graph, so the mocks live (duplicated, by design)
 * in each test file. See the header note in `command-dispatch.characterization.test.ts`
 * and `command-dispatch.delegate-arms.test.ts`.
 */
import { expect, vi } from 'vitest'
import { COMMAND_IDS, type CommandId } from '$lib/commands'
import type { CommandDispatchContext } from './command-dispatch'
import type { ExplorerAPI } from './explorer-api'

// --- The exempt-id set (derived; the suite self-checks the count) ----------
/**
 * The 20 ids registered (for the rebinding UI) but with NO dispatch handler —
 * three families: native-menu-owned (`app.quit`/`hide`/`hideOthers`/`showAll`),
 * per-keystroke P2 (`nav.up`/`down`/`left`/`right`/`firstInFull`/`lastInFull`),
 * and component-scoped (palette/volume/network/share/contextMenu). Mirrors the
 * production `DispatchExemptId` union (`command-handlers/types.ts`), kept as an
 * independent local copy so this characterization file derives its own dispatchable
 * vs exempt split rather than trusting the code it characterizes.
 */
export const EXEMPT_IDS = [
  'app.quit',
  'app.hide',
  'app.hideOthers',
  'app.showAll',
  'nav.up',
  'nav.down',
  'nav.left',
  'nav.right',
  'nav.firstInFull',
  'nav.lastInFull',
  'palette.up',
  'palette.down',
  'palette.execute',
  'palette.close',
  'volume.select',
  'volume.close',
  'network.selectHost',
  'share.back',
  'share.selectShare',
  'file.contextMenu',
] as const satisfies readonly CommandId[]

export const EXEMPT_SET: ReadonlySet<string> = new Set(EXEMPT_IDS)
export const DISPATCHABLE_IDS = COMMAND_IDS.filter((id) => !EXEMPT_SET.has(id))

// --- Shared harness --------------------------------------------------------
/** A ctx whose dialogs callbacks are all spies. */
export function makeCtx(explorer: Partial<ExplorerAPI>): CommandDispatchContext {
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
  }
}

/** A fully-stubbed ExplorerAPI: every method a `vi.fn()`, so an unexpected call surfaces. */
export function makeExplorerSpy(): Record<string, ReturnType<typeof vi.fn>> {
  const methods = [
    'toggleHiddenFiles',
    'setViewMode',
    'setViewModeFromMenu',
    'switchPane',
    'swapPanes',
    'toggleVolumeChooser',
    'copyPathBetweenPanes',
    'refreshPane',
    'newTab',
    'closeActiveTabWithConfirmation',
    'reopenLastClosedTab',
    'cycleTab',
    'togglePinActiveTab',
    'closeOtherTabs',
    'handleMcpTabAction',
    'sendKeyToFocusedPane',
    'navigate',
    'getFocusedPane',
    'openItemUnderCursor',
    'moveCursor',
    'scrollTo',
    'refreshNetworkHosts',
    'selectVolumeByName',
    'setSortColumn',
    'setSortOrder',
    'setSort',
    'openViewerForCursor',
    'startRename',
    'getFileAndPathUnderCursor',
    'toggleTagOnFocusedSelection',
    'openCopyDialog',
    'openMoveDialog',
    'openCompressDialog',
    'openNewFolderDialog',
    'openNewFileDialog',
    'openDeleteDialog',
    'confirmDialog',
    'handleSelectionAction',
    'handleMcpSelect',
    'handleMcpSelectNames',
    'copyToClipboard',
    'cutToClipboard',
    'pasteFromClipboard',
  ]
  const spy: Record<string, ReturnType<typeof vi.fn>> = {}
  for (const m of methods) spy[m] = vi.fn()
  // Defaults for the methods whose return value the arm branches on.
  spy.getFocusedPane.mockReturnValue('left')
  return spy
}

// --- Table-driven: the simple-delegate arms (one method call, exact args) ---
// Each row pins which explorer method (or dialog callback) fires, with what.
export type DelegateRow = {
  id: CommandId
  args?: CommandId extends never ? never : unknown
  /** Asserts the expected call(s) given the explorer spy + the ctx's dialogs. */
  expect: (e: Record<string, ReturnType<typeof vi.fn>>, dialogs: CommandDispatchContext['dialogs']) => void
}

export const DELEGATE_ROWS: DelegateRow[] = [
  // --- App / dialog openers ---
  {
    id: 'app.commandPalette',
    expect: (_e, d) => {
      expect(d.showCommandPalette).toHaveBeenLastCalledWith(true)
    },
  },
  {
    id: 'search.open',
    expect: (_e, d) => {
      expect(d.showSearchDialog).toHaveBeenCalledExactlyOnceWith(true)
    },
  },
  {
    id: 'nav.goToPath',
    expect: (_e, d) => {
      expect(d.showGoToPathDialog).toHaveBeenCalledExactlyOnceWith(true)
    },
  },
  {
    id: 'app.about',
    expect: (_e, d) => {
      expect(d.showAboutWindow).toHaveBeenCalledExactlyOnceWith(true)
    },
  },
  {
    id: 'app.licenseKey',
    expect: (_e, d) => {
      expect(d.showLicenseKeyDialog).toHaveBeenCalledExactlyOnceWith(true)
    },
  },
  {
    id: 'cmdr.openOnboarding',
    expect: (_e, d) => {
      expect(d.openOnboarding).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'about.close',
    expect: (_e, d) => {
      expect(d.showAboutWindow).toHaveBeenCalledExactlyOnceWith(false)
    },
  },
  {
    id: 'selection.selectFiles',
    expect: (_e, d) => {
      expect(d.showSelectionDialog).toHaveBeenCalledExactlyOnceWith('add')
    },
  },
  {
    id: 'selection.deselectFiles',
    expect: (_e, d) => {
      expect(d.showSelectionDialog).toHaveBeenCalledExactlyOnceWith('remove')
    },
  },

  // --- View ---
  {
    id: 'view.briefMode',
    expect: (e) => {
      expect(e.setViewMode).toHaveBeenCalledExactlyOnceWith('brief')
    },
  },
  {
    id: 'view.fullMode',
    expect: (e) => {
      expect(e.setViewMode).toHaveBeenCalledExactlyOnceWith('full')
    },
  },

  // --- Pane ---
  {
    id: 'pane.switch',
    expect: (e) => {
      expect(e.switchPane).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'pane.swap',
    expect: (e) => {
      expect(e.swapPanes).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'pane.leftVolumeChooser',
    expect: (e) => {
      expect(e.toggleVolumeChooser).toHaveBeenCalledExactlyOnceWith('left')
    },
  },
  {
    id: 'pane.rightVolumeChooser',
    expect: (e) => {
      expect(e.toggleVolumeChooser).toHaveBeenCalledExactlyOnceWith('right')
    },
  },
  {
    id: 'pane.copyPathLeftToRight',
    expect: (e) => {
      expect(e.copyPathBetweenPanes).toHaveBeenCalledExactlyOnceWith('left', 'right')
    },
  },
  {
    id: 'pane.copyPathRightToLeft',
    expect: (e) => {
      expect(e.copyPathBetweenPanes).toHaveBeenCalledExactlyOnceWith('right', 'left')
    },
  },
  {
    id: 'pane.refresh',
    expect: (e) => {
      expect(e.refreshPane).toHaveBeenCalledOnce()
    },
  },

  // --- Tab (non-toast arms) ---
  {
    id: 'tab.next',
    expect: (e) => {
      expect(e.cycleTab).toHaveBeenCalledExactlyOnceWith('next')
    },
  },
  {
    id: 'tab.prev',
    expect: (e) => {
      expect(e.cycleTab).toHaveBeenCalledExactlyOnceWith('prev')
    },
  },
  {
    id: 'tab.togglePin',
    expect: (e) => {
      expect(e.togglePinActiveTab).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'tab.closeOthers',
    expect: (e) => {
      expect(e.closeOtherTabs).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'tab.mcpAction',
    args: { pane: 'right', action: 'close', tabId: 't1', pinned: true },
    expect: (e) => {
      expect(e.handleMcpTabAction).toHaveBeenCalledExactlyOnceWith('right', 'close', 't1', true)
    },
  },

  // --- Navigation ---
  {
    id: 'nav.open',
    expect: (e) => {
      expect(e.sendKeyToFocusedPane).toHaveBeenCalledExactlyOnceWith('Enter')
    },
  },
  {
    id: 'nav.parent',
    expect: (e) => {
      expect(e.navigate).toHaveBeenCalledExactlyOnceWith({
        pane: 'left',
        to: { history: 'parent' },
        source: 'user',
      })
    },
  },
  {
    id: 'nav.back',
    expect: (e) => {
      expect(e.navigate).toHaveBeenCalledExactlyOnceWith({ pane: 'left', to: { history: 'back' }, source: 'user' })
    },
  },
  {
    id: 'nav.forward',
    expect: (e) => {
      expect(e.navigate).toHaveBeenCalledExactlyOnceWith({
        pane: 'left',
        to: { history: 'forward' },
        source: 'user',
      })
    },
  },
  {
    id: 'nav.home',
    expect: (e) => {
      expect(e.sendKeyToFocusedPane).toHaveBeenCalledExactlyOnceWith('Home')
    },
  },
  {
    id: 'nav.end',
    expect: (e) => {
      expect(e.sendKeyToFocusedPane).toHaveBeenCalledExactlyOnceWith('End')
    },
  },
  {
    id: 'nav.pageUp',
    expect: (e) => {
      expect(e.sendKeyToFocusedPane).toHaveBeenCalledExactlyOnceWith('PageUp')
    },
  },
  {
    id: 'nav.pageDown',
    expect: (e) => {
      expect(e.sendKeyToFocusedPane).toHaveBeenCalledExactlyOnceWith('PageDown')
    },
  },

  // --- Cursor (scrollTo is fire-and-forget; the two round-trip ids are pinned separately) ---
  {
    id: 'cursor.scrollTo',
    args: { pane: 'left', index: 7 },
    expect: (e) => {
      expect(e.scrollTo).toHaveBeenCalledExactlyOnceWith('left', 7)
    },
  },

  // --- Network / volume ---
  {
    id: 'network.refresh',
    expect: (e) => {
      expect(e.refreshNetworkHosts).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'volume.selectByName',
    args: { pane: 'right', name: 'Macintosh HD' },
    expect: (e) => {
      expect(e.selectVolumeByName).toHaveBeenCalledExactlyOnceWith('right', 'Macintosh HD')
    },
  },

  // --- Sort ---
  {
    id: 'sort.byName',
    expect: (e) => {
      expect(e.setSortColumn).toHaveBeenCalledExactlyOnceWith('name')
    },
  },
  {
    id: 'sort.byExtension',
    expect: (e) => {
      expect(e.setSortColumn).toHaveBeenCalledExactlyOnceWith('extension')
    },
  },
  {
    id: 'sort.bySize',
    expect: (e) => {
      expect(e.setSortColumn).toHaveBeenCalledExactlyOnceWith('size')
    },
  },
  {
    id: 'sort.byModified',
    expect: (e) => {
      expect(e.setSortColumn).toHaveBeenCalledExactlyOnceWith('modified')
    },
  },
  {
    id: 'sort.byCreated',
    expect: (e) => {
      expect(e.setSortColumn).toHaveBeenCalledExactlyOnceWith('created')
    },
  },
  {
    id: 'sort.ascending',
    expect: (e) => {
      expect(e.setSortOrder).toHaveBeenCalledExactlyOnceWith('asc')
    },
  },
  {
    id: 'sort.descending',
    expect: (e) => {
      expect(e.setSortOrder).toHaveBeenCalledExactlyOnceWith('desc')
    },
  },
  {
    id: 'sort.toggleOrder',
    expect: (e) => {
      expect(e.setSortOrder).toHaveBeenCalledExactlyOnceWith('toggle')
    },
  },
  {
    id: 'sort.set',
    args: { pane: 'left', column: 'size', order: 'desc' },
    expect: (e) => {
      expect(e.setSort).toHaveBeenCalledExactlyOnceWith('size', 'desc', 'left')
    },
  },

  // --- File actions (delegate / dialog-opener arms) ---
  {
    id: 'file.view',
    expect: (e) => {
      expect(e.openViewerForCursor).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'file.rename',
    expect: (e) => {
      expect(e.startRename).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'file.copy',
    expect: (e) => {
      expect(e.openCopyDialog).toHaveBeenCalledExactlyOnceWith(undefined, undefined)
    },
  },
  {
    id: 'file.copy',
    args: { autoConfirm: true, onConflict: 'overwrite_all' },
    expect: (e) => {
      expect(e.openCopyDialog).toHaveBeenCalledExactlyOnceWith(true, 'overwrite_all')
    },
  },
  {
    id: 'file.move',
    expect: (e) => {
      expect(e.openMoveDialog).toHaveBeenCalledExactlyOnceWith(undefined, undefined)
    },
  },
  {
    id: 'file.move',
    args: { autoConfirm: false, onConflict: 'skip_all' },
    expect: (e) => {
      expect(e.openMoveDialog).toHaveBeenCalledExactlyOnceWith(false, 'skip_all')
    },
  },
  {
    id: 'file.newFolder',
    expect: (e) => {
      expect(e.openNewFolderDialog).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'file.newFile',
    expect: (e) => {
      expect(e.openNewFileDialog).toHaveBeenCalledOnce()
    },
  },
  {
    id: 'file.delete',
    expect: (e) => {
      expect(e.openDeleteDialog).toHaveBeenCalledExactlyOnceWith(false, undefined)
    },
  },
  {
    id: 'file.delete',
    args: { autoConfirm: true },
    expect: (e) => {
      expect(e.openDeleteDialog).toHaveBeenCalledExactlyOnceWith(false, true)
    },
  },
  {
    id: 'file.deletePermanently',
    expect: (e) => {
      expect(e.openDeleteDialog).toHaveBeenCalledExactlyOnceWith(true)
    },
  },
  {
    id: 'dialog.confirm',
    args: { type: 'transfer-confirmation', onConflict: 'overwrite_all' },
    expect: (e) => {
      expect(e.confirmDialog).toHaveBeenCalledExactlyOnceWith('transfer-confirmation', 'overwrite_all')
    },
  },

  // --- Selection (non-branching arms) ---
  {
    id: 'selection.toggle',
    expect: (e) => {
      expect(e.handleSelectionAction).toHaveBeenCalledExactlyOnceWith('toggleAtCursor')
    },
  },
  {
    id: 'selection.toggleAndDown',
    expect: (e) => {
      expect(e.handleSelectionAction).toHaveBeenCalledExactlyOnceWith('toggleAtCursorAndMoveDown')
    },
  },
  {
    id: 'selection.deselectAll',
    expect: (e) => {
      expect(e.handleSelectionAction).toHaveBeenCalledExactlyOnceWith('deselectAll')
    },
  },
  {
    id: 'selection.mcpSelect',
    args: { pane: 'left', start: 2, count: 5, mode: 'add' },
    expect: (e) => {
      expect(e.handleMcpSelect).toHaveBeenCalledExactlyOnceWith('left', 2, 5, 'add')
    },
  },

  // --- Edit (pasteAsMove: no activeElement check) ---
  {
    id: 'edit.pasteAsMove',
    expect: (e) => {
      expect(e.pasteFromClipboard).toHaveBeenCalledExactlyOnceWith(true)
    },
  },
]
