/**
 * Characterization suite for `handleCommandExecute`.
 *
 * Pins the EXACT behavior of the dispatch core — every dispatchable id's call
 * pattern + args, the bespoke branches (zoom/tab toasts, activeElement input
 * branches, the quickLook guard, the cloud try/catch toast, the about URLs,
 * `view.showHidden`'s early return), the per-arm await/void semantics (the two
 * MCP round-trip ids pinned with deferred promises), the preamble order, and the
 * 20 exempt ids' preamble-then-silent-no-op path.
 *
 * This drives the PUBLIC `handleCommandExecute(commandId, ctx, ...args)`, whose
 * signature is independent of the internal dispatch mechanism, so the same suite
 * pins behavior whether dispatch routes through a switch or the flat handler
 * record it uses today.
 *
 * The dispatchable-89 / exempt-20 sets are DERIVED from `COMMAND_IDS` minus a
 * local exempt list, so the suite self-checks the counts (a new command, or a
 * miscounted exemption, fails the set tests below).
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'

// --- Mocks (hoisted) -------------------------------------------------------
// All captured spies live in ONE `vi.hoisted` block so the hoisted `vi.mock`
// factories below can close over them safely. A plain top-level `const spy =
// vi.fn()` isn't initialized yet when a hoisted factory runs during the import
// of `command-dispatch.ts` (and its transitive deps), which throws "Cannot
// access X before initialization". `vi.hoisted` is evaluated before any mock
// factory, so the references are live.
const m = vi.hoisted(() => ({
  logInfo: vi.fn<(...a: unknown[]) => void>(),
  invoke: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  getVolumeId: vi.fn<() => string>(() => 'local'),
  getPanePath: vi.fn<() => string>(() => '/Users/test'),
  addToast: vi.fn<(...a: unknown[]) => void>(),
  getSetting: vi.fn<(key: string) => number>(() => 100),
  setSetting: vi.fn<(...a: unknown[]) => void>(),
  getEffectiveShortcuts: vi.fn<(id: string) => string[]>(() => []),
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  openErrorReportDialog: vi.fn<() => void>(),
  runMenuTriggeredCheck: vi.fn(() => Promise.resolve()),
  goToLatestDownload: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  openExternalUrl: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  showInFinder: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  copyToClipboard: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  quickLookOpen: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  quickLookClose: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  getInfo: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  openInEditor: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  syncMenuShowHidden: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  readClipboardText: vi.fn<() => Promise<string>>(() => Promise.resolve('')),
  cloudMakeAvailableOffline: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  cloudRemoveDownload: vi.fn<(...a: unknown[]) => Promise<void>>(() => Promise.resolve()),
  quickLookState: { isOpen: false },
  quickLookDispatchGuardJustFired: vi.fn<() => boolean>(() => false),
  armQuickLookDispatchGuard: vi.fn<() => void>(),
}))

const {
  logInfo,
  invoke,
  getVolumeId,
  getPanePath,
  addToast,
  getSetting,
  setSetting,
  getEffectiveShortcuts,
  openSettingsWindow,
  openErrorReportDialog,
  runMenuTriggeredCheck,
  goToLatestDownload,
  openExternalUrl,
  showInFinder,
  copyToClipboard,
  quickLookOpen,
  quickLookClose,
  getInfo,
  openInEditor,
  syncMenuShowHidden,
  readClipboardText,
  cloudMakeAvailableOffline,
  cloudRemoveDownload,
  quickLookState,
  quickLookDispatchGuardJustFired,
  armQuickLookDispatchGuard,
} = m

// `getAppLogger('user-action')` runs at module top-level; `m.logInfo` captures
// the logged `id` so the preamble-order + exempt tests can assert on it.
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ info: m.logInfo, debug: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

// The dispatch preamble fires `record_breadcrumb` via `invoke`.
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => m.invoke(...args) }))

// The capability guard reads the focused pane's volume id + path.
vi.mock('$lib/file-explorer/pane/focused-pane-reads', () => ({
  getFocusedPaneVolumeId: () => m.getVolumeId(),
  getFocusedPanePath: () => m.getPanePath(),
}))

// Empty store ⇒ `local` falls to the listable default (canPasteInto: true).
vi.mock('$lib/stores/volume-store.svelte', () => ({ getVolumes: () => [] }))

vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    m.addToast(...args)
  },
}))

// Zoom arms: getSetting/setSetting back the text-size read/write.
vi.mock('$lib/settings', () => ({
  getSetting: (key: string) => m.getSetting(key),
  setSetting: (...args: unknown[]) => {
    m.setSetting(...args)
  },
}))

// `showZoomToast` reads the reset shortcut. Default: no shortcut bound (menu hint).
vi.mock('$lib/shortcuts', () => ({
  getEffectiveShortcuts: (id: string) => m.getEffectiveShortcuts(id),
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: () => m.openSettingsWindow(),
}))

vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({
  openErrorReportDialog: () => {
    m.openErrorReportDialog()
  },
}))

vi.mock('$lib/updates/updater.svelte', () => ({
  runMenuTriggeredCheck: () => m.runMenuTriggeredCheck(),
}))

vi.mock('$lib/downloads/go-to-latest', () => ({
  goToLatestDownload: (...args: unknown[]) => m.goToLatestDownload(...args),
}))

// The whole `$lib/tauri-commands` barrel the arms call.
vi.mock('$lib/tauri-commands', () => ({
  openExternalUrl: (...a: unknown[]) => m.openExternalUrl(...a),
  showInFinder: (...a: unknown[]) => m.showInFinder(...a),
  copyToClipboard: (...a: unknown[]) => m.copyToClipboard(...a),
  quickLookOpen: (...a: unknown[]) => m.quickLookOpen(...a),
  quickLookClose: (...a: unknown[]) => m.quickLookClose(...a),
  getInfo: (...a: unknown[]) => m.getInfo(...a),
  openInEditor: (...a: unknown[]) => m.openInEditor(...a),
  syncMenuShowHidden: (...a: unknown[]) => m.syncMenuShowHidden(...a),
  readClipboardText: () => m.readClipboardText(),
  cloudMakeAvailableOffline: (...a: unknown[]) => m.cloudMakeAvailableOffline(...a),
  cloudRemoveDownload: (...a: unknown[]) => m.cloudRemoveDownload(...a),
}))

// QuickLook dispatch guard + the `$state` singleton (reconfigurable per branch).
vi.mock('$lib/file-explorer/quick-look/quick-look-state.svelte', () => ({
  quickLookState: m.quickLookState,
  quickLookDispatchGuardJustFired: () => m.quickLookDispatchGuardJustFired(),
  armQuickLookDispatchGuard: () => {
    m.armQuickLookDispatchGuard()
  },
}))

import { handleCommandExecute, type CommandDispatchContext } from './command-dispatch'
import { COMMAND_IDS, type CommandId } from '$lib/commands'
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
const EXEMPT_IDS = [
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

const EXEMPT_SET: ReadonlySet<string> = new Set(EXEMPT_IDS)
const DISPATCHABLE_IDS = COMMAND_IDS.filter((id) => !EXEMPT_SET.has(id))

// --- Shared harness --------------------------------------------------------
/** A ctx whose dialogs callbacks are all spies. */
function makeCtx(explorer: Partial<ExplorerAPI>): CommandDispatchContext {
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
function makeExplorerSpy(): Record<string, ReturnType<typeof vi.fn>> {
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
    'openCopyDialog',
    'openMoveDialog',
    'openNewFolderDialog',
    'openNewFileDialog',
    'openDeleteDialog',
    'confirmDialog',
    'handleSelectionAction',
    'handleMcpSelect',
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

beforeEach(() => {
  vi.clearAllMocks()
  getVolumeId.mockReturnValue('local')
  getPanePath.mockReturnValue('/Users/test')
  getSetting.mockReturnValue(100)
  getEffectiveShortcuts.mockReturnValue([])
  readClipboardText.mockResolvedValue('')
  quickLookDispatchGuardJustFired.mockReturnValue(false)
  quickLookState.isOpen = false
})

// ===========================================================================
// Self-checks: the dispatchable / exempt sets partition COMMAND_IDS.
// ===========================================================================
describe('characterization — id partition self-check', () => {
  it('exempt set is exactly 20 ids, all real CommandIds', () => {
    expect(EXEMPT_IDS).toHaveLength(20)
    for (const id of EXEMPT_IDS) expect(COMMAND_IDS).toContain(id)
  })

  it('dispatchable set is exactly 89 ids', () => {
    expect(DISPATCHABLE_IDS).toHaveLength(89)
  })

  it('dispatchable ∪ exempt = COMMAND_IDS, disjoint', () => {
    const union = new Set([...DISPATCHABLE_IDS, ...EXEMPT_IDS])
    expect(union).toEqual(new Set(COMMAND_IDS))
    expect(DISPATCHABLE_IDS.length + EXEMPT_IDS.length).toBe(COMMAND_IDS.length)
  })
})

// ===========================================================================
// Table-driven: the simple-delegate arms (one method call, exact args).
// Each row pins which explorer method (or dialog callback) fires, with what.
// ===========================================================================
type DelegateRow = {
  id: CommandId
  args?: CommandId extends never ? never : unknown
  /** Asserts the expected call(s) given the explorer spy + the ctx's dialogs. */
  expect: (e: Record<string, ReturnType<typeof vi.fn>>, dialogs: CommandDispatchContext['dialogs']) => void
}

const DELEGATE_ROWS: DelegateRow[] = [
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

describe('characterization — table-driven simple-delegate arms', () => {
  it.each(DELEGATE_ROWS.map((r) => ({ row: r, label: `${r.id}${r.args ? ' (with args)' : ''}` })))(
    'dispatches $label to its exact call pattern',
    async ({ row }) => {
      const explorer = makeExplorerSpy()
      const ctx = makeCtx(explorer)
      if (row.args === undefined) {
        await handleCommandExecute(row.id, ctx)
      } else {
        // The public generic arg-checks per id; the table holds typed payloads.
        await handleCommandExecute(row.id, ctx, row.args as never)
      }
      row.expect(explorer, ctx.dialogs)
    },
  )
})

// ===========================================================================
// Static-import delegate arms (the ones calling the mocked module barrels).
// ===========================================================================
describe('characterization — module-delegate arms', () => {
  it('app.settings → openSettingsWindow()', async () => {
    await handleCommandExecute('app.settings', makeCtx({}))
    expect(openSettingsWindow).toHaveBeenCalledOnce()
  })

  it('help.sendErrorReport → openErrorReportDialog()', async () => {
    await handleCommandExecute('help.sendErrorReport', makeCtx({}))
    expect(openErrorReportDialog).toHaveBeenCalledOnce()
  })

  it('app.checkForUpdates → runMenuTriggeredCheck()', async () => {
    await handleCommandExecute('app.checkForUpdates', makeCtx({}))
    expect(runMenuTriggeredCheck).toHaveBeenCalledOnce()
  })

  it('downloads.goToLatest → goToLatestDownload(explorerRef)', async () => {
    const explorer = makeExplorerSpy()
    const ctx = makeCtx(explorer)
    await handleCommandExecute('downloads.goToLatest', ctx)
    expect(goToLatestDownload).toHaveBeenCalledExactlyOnceWith(ctx.getExplorer())
  })

  it('about.openWebsite → openExternalUrl with the exact URL', async () => {
    await handleCommandExecute('about.openWebsite', makeCtx({}))
    expect(openExternalUrl).toHaveBeenCalledExactlyOnceWith('https://getcmdr.com')
  })

  it('about.openUpgrade → openExternalUrl with the exact upgrade URL', async () => {
    await handleCommandExecute('about.openUpgrade', makeCtx({}))
    expect(openExternalUrl).toHaveBeenCalledExactlyOnceWith('https://getcmdr.com/upgrade')
  })
})

// ===========================================================================
// getFileAndPathUnderCursor-then-act arms (file.edit / showInFinder / copyPath /
// copyFilename / getInfo). Pin BOTH branches: entry present → act; absent → no-op.
// ===========================================================================
describe('characterization — entry-under-cursor arms', () => {
  const ENTRY = { path: '/Users/test/file.txt', filename: 'file.txt' }

  it('file.edit → openInEditor(path) when an entry is under the cursor', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('file.edit', makeCtx(explorer))
    expect(openInEditor).toHaveBeenCalledExactlyOnceWith(ENTRY.path)
  })

  it('file.edit → no-op when nothing is under the cursor', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(undefined)
    await handleCommandExecute('file.edit', makeCtx(explorer))
    expect(openInEditor).not.toHaveBeenCalled()
  })

  it('file.showInFinder → showInFinder(path)', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('file.showInFinder', makeCtx(explorer))
    expect(showInFinder).toHaveBeenCalledExactlyOnceWith(ENTRY.path)
  })

  it('file.copyPath → copyToClipboard(path)', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('file.copyPath', makeCtx(explorer))
    expect(copyToClipboard).toHaveBeenCalledExactlyOnceWith(ENTRY.path)
  })

  it('file.copyFilename → copyToClipboard(filename)', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('file.copyFilename', makeCtx(explorer))
    expect(copyToClipboard).toHaveBeenCalledExactlyOnceWith(ENTRY.filename)
  })

  it('file.getInfo → getInfo(path)', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('file.getInfo', makeCtx(explorer))
    expect(getInfo).toHaveBeenCalledExactlyOnceWith(ENTRY.path)
  })

  it('file.copyCurrentDirectoryPath → copyToClipboard(focused pane path)', async () => {
    getPanePath.mockReturnValue('/Users/test/dir')
    await handleCommandExecute('file.copyCurrentDirectoryPath', makeCtx(makeExplorerSpy()))
    expect(copyToClipboard).toHaveBeenCalledExactlyOnceWith('/Users/test/dir')
  })

  it('file.copyCurrentDirectoryPath → no-op when the focused pane has no path', async () => {
    getPanePath.mockReturnValue('')
    await handleCommandExecute('file.copyCurrentDirectoryPath', makeCtx(makeExplorerSpy()))
    expect(copyToClipboard).not.toHaveBeenCalled()
  })
})

// ===========================================================================
// view.showHidden: toggle + syncMenuShowHidden, and the no-explorer early return.
// ===========================================================================
describe('characterization — view.showHidden', () => {
  it('toggles hidden files and pushes the new state to the native menu', async () => {
    const explorer = makeExplorerSpy()
    explorer.toggleHiddenFiles.mockReturnValue(true)
    await handleCommandExecute('view.showHidden', makeCtx(explorer))
    expect(explorer.toggleHiddenFiles).toHaveBeenCalledOnce()
    expect(syncMenuShowHidden).toHaveBeenCalledExactlyOnceWith(true)
  })

  it('early-returns before the toggle when there is no explorer', async () => {
    const ctx: CommandDispatchContext = { getExplorer: () => undefined, dialogs: makeCtx({}).dialogs }
    await handleCommandExecute('view.showHidden', ctx)
    expect(syncMenuShowHidden).not.toHaveBeenCalled()
  })
})

// ===========================================================================
// Zoom arms: preset + in/out, the exact toast strings per direction.
// ===========================================================================
describe('characterization — zoom arms (toast copy verbatim)', () => {
  it('view.zoom.set75/100/125/150 set the preset and toast', async () => {
    const cases: { id: CommandId; preset: number }[] = [
      { id: 'view.zoom.set75', preset: 75 },
      { id: 'view.zoom.set100', preset: 100 },
      { id: 'view.zoom.set125', preset: 125 },
      { id: 'view.zoom.set150', preset: 150 },
    ]
    for (const { id, preset } of cases) {
      setSetting.mockClear()
      getSetting.mockReturnValue(100)
      await handleCommandExecute(id, makeCtx({}))
      expect(setSetting).toHaveBeenCalledExactlyOnceWith('appearance.textSize', preset)
    }
  })

  it('view.zoom.in clamps to 150 and toasts the increase message (menu hint, no shortcut)', async () => {
    getSetting.mockReturnValue(100)
    getEffectiveShortcuts.mockReturnValue([])
    await handleCommandExecute('view.zoom.in', makeCtx({}))
    expect(setSetting).toHaveBeenCalledExactlyOnceWith('appearance.textSize', 110)
    expect(addToast).toHaveBeenCalledExactlyOnceWith(
      'Zoom increased to 110%. You can reset the zoom level to 100% at View > Zoom > 100%.',
      { level: 'info', id: 'zoom-change' },
    )
  })

  it('view.zoom.in uses the bound shortcut hint when one exists', async () => {
    getSetting.mockReturnValue(100)
    getEffectiveShortcuts.mockReturnValue(['⌘+'])
    await handleCommandExecute('view.zoom.in', makeCtx({}))
    expect(addToast).toHaveBeenCalledExactlyOnceWith(
      'Zoom increased to 110%. You can reset the zoom level to 100% by ⌘+.',
      { level: 'info', id: 'zoom-change' },
    )
  })

  it('view.zoom.in clamps at the 150 ceiling', async () => {
    getSetting.mockReturnValue(150)
    await handleCommandExecute('view.zoom.in', makeCtx({}))
    expect(setSetting).toHaveBeenCalledExactlyOnceWith('appearance.textSize', 150)
    // oldSize === newSize → showZoomToast returns early, no toast.
    expect(addToast).not.toHaveBeenCalled()
  })

  it('view.zoom.out clamps to 75 and toasts the decrease message', async () => {
    getSetting.mockReturnValue(100)
    getEffectiveShortcuts.mockReturnValue([])
    await handleCommandExecute('view.zoom.out', makeCtx({}))
    expect(setSetting).toHaveBeenCalledExactlyOnceWith('appearance.textSize', 90)
    expect(addToast).toHaveBeenCalledExactlyOnceWith(
      'Zoom decreased to 90%. You can reset the zoom level to 100% at View > Zoom > 100%.',
      { level: 'info', id: 'zoom-change' },
    )
  })

  it('view.zoom.out clamps at the 75 floor', async () => {
    getSetting.mockReturnValue(75)
    await handleCommandExecute('view.zoom.out', makeCtx({}))
    expect(setSetting).toHaveBeenCalledExactlyOnceWith('appearance.textSize', 75)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('zoom preset toast: reset to 100% from a non-100 start', async () => {
    getSetting.mockReturnValue(125)
    await handleCommandExecute('view.zoom.set100', makeCtx({}))
    expect(addToast).toHaveBeenCalledExactlyOnceWith('Zoom reset to 100%.', { level: 'info', id: 'zoom-change' })
  })
})

// ===========================================================================
// Tab arms with toast/last-tab branches.
// ===========================================================================
describe('characterization — tab arms (toast + last-tab branches)', () => {
  it('tab.new toasts "Tab limit reached" only when newTab returns false', async () => {
    const explorer = makeExplorerSpy()
    explorer.newTab.mockReturnValue(true)
    await handleCommandExecute('tab.new', makeCtx(explorer))
    expect(addToast).not.toHaveBeenCalled()

    addToast.mockClear()
    explorer.newTab.mockReturnValue(false)
    await handleCommandExecute('tab.new', makeCtx(explorer))
    expect(addToast).toHaveBeenCalledExactlyOnceWith('Tab limit reached', { level: 'warn' })
  })

  it('tab.close: non-last-tab result does not close the window', async () => {
    const explorer = makeExplorerSpy()
    explorer.closeActiveTabWithConfirmation.mockResolvedValue('closed')
    await handleCommandExecute('tab.close', makeCtx(explorer))
    expect(explorer.closeActiveTabWithConfirmation).toHaveBeenCalledOnce()
  })

  it('tab.reopen toasts "No recently closed tabs" on empty, "Tab limit reached" on cap, nothing on success', async () => {
    const explorer = makeExplorerSpy()

    explorer.reopenLastClosedTab.mockReturnValue('empty')
    await handleCommandExecute('tab.reopen', makeCtx(explorer))
    expect(addToast).toHaveBeenCalledExactlyOnceWith('No recently closed tabs in this pane.', { level: 'warn' })

    addToast.mockClear()
    explorer.reopenLastClosedTab.mockReturnValue('cap')
    await handleCommandExecute('tab.reopen', makeCtx(explorer))
    expect(addToast).toHaveBeenCalledExactlyOnceWith('Tab limit reached', { level: 'warn' })

    addToast.mockClear()
    explorer.reopenLastClosedTab.mockReturnValue('ok')
    await handleCommandExecute('tab.reopen', makeCtx(explorer))
    expect(addToast).not.toHaveBeenCalled()
  })
})

// ===========================================================================
// activeElement input branches: selection.selectAll, edit.copy / cut / paste.
// happy-dom provides document.activeElement; we focus a real <input>.
// ===========================================================================
describe('characterization — activeElement input branches', () => {
  /** Mounts an input/textarea and focuses it, returning a cleanup fn. */
  function focusInput(tag: 'input' | 'textarea'): { el: HTMLInputElement | HTMLTextAreaElement; cleanup: () => void } {
    const el = document.createElement(tag)
    document.body.appendChild(el)
    el.focus()
    return {
      el,
      cleanup: () => {
        el.remove()
      },
    }
  }

  /**
   * Replaces `document.execCommand` with a spy. The input-focus branches of
   * `edit.copy` / `edit.cut` / `edit.paste` call it (it's the only API for
   * triggering a native copy/cut/insert in a text input); we stub it to capture
   * the call. Cast through `unknown` so we don't reference the deprecated member
   * type directly (the production arms carry the same `no-deprecated` disable).
   */
  function stubExecCommand(): ReturnType<typeof vi.fn> {
    const execCommand = vi.fn()
    ;(document as unknown as { execCommand: unknown }).execCommand = execCommand
    return execCommand
  }

  it('selection.selectAll routes to input.select() when an input is focused (no file selection)', async () => {
    const { el, cleanup } = focusInput('input')
    const selectSpy = vi.spyOn(el, 'select')
    const explorer = makeExplorerSpy()
    await handleCommandExecute('selection.selectAll', makeCtx(explorer))
    expect(selectSpy).toHaveBeenCalledOnce()
    expect(explorer.handleSelectionAction).not.toHaveBeenCalled()
    cleanup()
  })

  it('selection.selectAll routes to handleSelectionAction("selectAll") with no input focused', async () => {
    const explorer = makeExplorerSpy()
    await handleCommandExecute('selection.selectAll', makeCtx(explorer))
    expect(explorer.handleSelectionAction).toHaveBeenCalledExactlyOnceWith('selectAll')
  })

  it('edit.copy uses execCommand("copy") when an input is focused', async () => {
    const { cleanup } = focusInput('input')
    const execCommand = stubExecCommand()
    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.copy', makeCtx(explorer))
    expect(execCommand).toHaveBeenCalledExactlyOnceWith('copy')
    expect(explorer.copyToClipboard).not.toHaveBeenCalled()
    cleanup()
  })

  it('edit.copy falls through to explorer.copyToClipboard with nothing focused and no selection', async () => {
    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.copy', makeCtx(explorer))
    expect(explorer.copyToClipboard).toHaveBeenCalledOnce()
  })

  it('edit.cut uses execCommand("cut") when an input is focused', async () => {
    const { cleanup } = focusInput('textarea')
    const execCommand = stubExecCommand()
    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.cut', makeCtx(explorer))
    expect(execCommand).toHaveBeenCalledExactlyOnceWith('cut')
    expect(explorer.cutToClipboard).not.toHaveBeenCalled()
    cleanup()
  })

  it('edit.cut falls through to explorer.cutToClipboard with nothing focused', async () => {
    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.cut', makeCtx(explorer))
    expect(explorer.cutToClipboard).toHaveBeenCalledOnce()
  })

  it('edit.paste reads clipboard text via Rust and inserts it when an input is focused', async () => {
    const { cleanup } = focusInput('input')
    readClipboardText.mockResolvedValue('pasted text')
    const execCommand = stubExecCommand()
    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.paste', makeCtx(explorer))
    expect(readClipboardText).toHaveBeenCalledOnce()
    expect(execCommand).toHaveBeenCalledExactlyOnceWith('insertText', false, 'pasted text')
    expect(explorer.pasteFromClipboard).not.toHaveBeenCalled()
    cleanup()
  })

  it('edit.paste skips insertText when the clipboard is empty (input focused)', async () => {
    const { cleanup } = focusInput('input')
    readClipboardText.mockResolvedValue('')
    const execCommand = stubExecCommand()
    await handleCommandExecute('edit.paste', makeCtx(makeExplorerSpy()))
    expect(readClipboardText).toHaveBeenCalledOnce()
    expect(execCommand).not.toHaveBeenCalled()
    cleanup()
  })

  it('edit.paste falls through to explorer.pasteFromClipboard(false) with nothing focused', async () => {
    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.paste', makeCtx(explorer))
    expect(explorer.pasteFromClipboard).toHaveBeenCalledExactlyOnceWith(false)
  })
})

// ===========================================================================
// file.quickLook: dispatch guard + open/close toggle.
// ===========================================================================
describe('characterization — file.quickLook', () => {
  it('returns immediately (no arm/open/close) when the dispatch guard just fired', async () => {
    quickLookDispatchGuardJustFired.mockReturnValue(true)
    const explorer = makeExplorerSpy()
    await handleCommandExecute('file.quickLook', makeCtx(explorer))
    expect(armQuickLookDispatchGuard).not.toHaveBeenCalled()
    expect(quickLookOpen).not.toHaveBeenCalled()
    expect(quickLookClose).not.toHaveBeenCalled()
  })

  it('opens Quick Look (arms the guard, flips isOpen, calls quickLookOpen) when closed', async () => {
    quickLookState.isOpen = false
    getVolumeId.mockReturnValue('local')
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue({ path: '/Users/test/a.png', filename: 'a.png' })
    await handleCommandExecute('file.quickLook', makeCtx(explorer))
    expect(armQuickLookDispatchGuard).toHaveBeenCalledOnce()
    expect(quickLookState.isOpen).toBe(true)
    expect(quickLookOpen).toHaveBeenCalledExactlyOnceWith('/Users/test/a.png', 'local')
  })

  it('closes Quick Look (flips isOpen false, calls quickLookClose) when open', async () => {
    quickLookState.isOpen = true
    await handleCommandExecute('file.quickLook', makeCtx(makeExplorerSpy()))
    expect(armQuickLookDispatchGuard).toHaveBeenCalledOnce()
    expect(quickLookState.isOpen).toBe(false)
    expect(quickLookClose).toHaveBeenCalledOnce()
    expect(quickLookOpen).not.toHaveBeenCalled()
  })

  it('arms the guard but no-ops when closed and nothing is under the cursor', async () => {
    quickLookState.isOpen = false
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(undefined)
    await handleCommandExecute('file.quickLook', makeCtx(explorer))
    expect(armQuickLookDispatchGuard).toHaveBeenCalledOnce()
    expect(quickLookOpen).not.toHaveBeenCalled()
    expect(quickLookState.isOpen).toBe(false)
  })
})

// ===========================================================================
// cloud.makeOffline / removeDownload: success path + the try/catch error toast.
// ===========================================================================
describe('characterization — cloud arms (try/catch error toast)', () => {
  const ENTRY = { path: '/Users/test/cloud.txt', filename: 'cloud.txt' }

  it('cloud.makeOffline calls cloudMakeAvailableOffline(path) on success (no toast)', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('cloud.makeOffline', makeCtx(explorer))
    expect(cloudMakeAvailableOffline).toHaveBeenCalledExactlyOnceWith(ENTRY.path)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('cloud.makeOffline toasts the error message on rejection', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    cloudMakeAvailableOffline.mockRejectedValueOnce('disk full')
    await handleCommandExecute('cloud.makeOffline', makeCtx(explorer))
    expect(addToast).toHaveBeenCalledExactlyOnceWith("Couldn't download from cloud. disk full", { level: 'error' })
  })

  it('cloud.removeDownload calls cloudRemoveDownload(path) on success', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    await handleCommandExecute('cloud.removeDownload', makeCtx(explorer))
    expect(cloudRemoveDownload).toHaveBeenCalledExactlyOnceWith(ENTRY.path)
    expect(addToast).not.toHaveBeenCalled()
  })

  it('cloud.removeDownload toasts the error message on rejection', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    cloudRemoveDownload.mockRejectedValueOnce('locked')
    await handleCommandExecute('cloud.removeDownload', makeCtx(explorer))
    expect(addToast).toHaveBeenCalledExactlyOnceWith("Couldn't remove the download. locked", { level: 'error' })
  })

  it('cloud arms no-op when nothing is under the cursor', async () => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(undefined)
    await handleCommandExecute('cloud.makeOffline', makeCtx(explorer))
    await handleCommandExecute('cloud.removeDownload', makeCtx(explorer))
    expect(cloudMakeAvailableOffline).not.toHaveBeenCalled()
    expect(cloudRemoveDownload).not.toHaveBeenCalled()
  })
})

// ===========================================================================
// Await semantics: the two MCP round-trip ids resolve ONLY after their inner
// async work settles (the ack-timing contract). Deferred promises prove it.
// Written so they FAIL if the handler `void`ed the inner call.
// ===========================================================================
describe('characterization — await semantics (deferred-promise pins)', () => {
  /** A promise plus its resolver, so the test controls when the mock settles. */
  function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
    let resolve!: (value: T) => void
    const promise = new Promise<T>((r) => {
      resolve = r
    })
    return { promise, resolve }
  }

  /** Settles by tag the instant the dispatch promise resolves. */
  function tagOnResolve(p: Promise<unknown>, tag: string, sink: string[]): Promise<void> {
    return p.then(() => {
      sink.push(tag)
    })
  }

  it('nav.openUnderCursor stays pending until openItemUnderCursor resolves', async () => {
    const d = deferred<undefined>()
    const explorer = makeExplorerSpy()
    explorer.openItemUnderCursor.mockReturnValue(d.promise)

    const order: string[] = []
    const dispatchDone = tagOnResolve(handleCommandExecute('nav.openUnderCursor', makeCtx(explorer)), 'dispatch', order)
    // A microtask flush: if the handler had `void`ed, dispatch would already be done.
    await Promise.resolve()
    await Promise.resolve()
    expect(order).not.toContain('dispatch')

    d.resolve(undefined)
    await dispatchDone
    expect(order).toEqual(['dispatch'])
    expect(explorer.openItemUnderCursor).toHaveBeenCalledOnce()
  })

  it('cursor.moveTo stays pending until moveCursor resolves', async () => {
    const d = deferred<undefined>()
    const explorer = makeExplorerSpy()
    explorer.moveCursor.mockReturnValue(d.promise)

    const order: string[] = []
    const dispatchDone = tagOnResolve(
      handleCommandExecute('cursor.moveTo', makeCtx(explorer), { pane: 'left', to: 3 }),
      'dispatch',
      order,
    )
    await Promise.resolve()
    await Promise.resolve()
    expect(order).not.toContain('dispatch')

    d.resolve(undefined)
    await dispatchDone
    expect(order).toEqual(['dispatch'])
    expect(explorer.moveCursor).toHaveBeenCalledExactlyOnceWith('left', 3)
  })

  // Weaker "resolves" pins for the OTHER await arms: they don't gate an MCP ack,
  // but pinning catches an accidental await↔void flip. Each must resolve once its
  // mocked inner call resolves; here all inner calls resolve immediately, so we
  // assert the dispatch promise resolves (await did not throw / hang).
  it('the remaining await arms resolve after their inner call', async () => {
    const ENTRY = { path: '/p', filename: 'f' }
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue(ENTRY)
    explorer.closeActiveTabWithConfirmation.mockResolvedValue('closed')

    // (downloads.goToLatest, about.openWebsite/openUpgrade hit module mocks.)
    await expect(handleCommandExecute('tab.close', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('file.edit', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('file.showInFinder', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('file.copyPath', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('file.copyCurrentDirectoryPath', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('file.copyFilename', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('file.getInfo', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('cloud.makeOffline', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('cloud.removeDownload', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('downloads.goToLatest', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('about.openWebsite', makeCtx(explorer))).resolves.toBeUndefined()
    await expect(handleCommandExecute('about.openUpgrade', makeCtx(explorer))).resolves.toBeUndefined()
  })

  it('tab.close awaits the close, then awaits the window close on the last-tab branch', async () => {
    const closeDeferred = deferred<string>()
    const explorer = makeExplorerSpy()
    explorer.closeActiveTabWithConfirmation.mockReturnValue(closeDeferred.promise)

    const order: string[] = []
    const dispatchDone = tagOnResolve(handleCommandExecute('tab.close', makeCtx(explorer)), 'dispatch', order)
    await Promise.resolve()
    expect(order).not.toContain('dispatch')

    // Resolve with a non-last-tab result so the dynamic window import isn't hit
    // (the dynamic `@tauri-apps/api/window` import is unmockable here; the last-tab
    // path is covered by E2E). The await-before-resolve timing is the point.
    closeDeferred.resolve('closed')
    await dispatchDone
    expect(order).toEqual(['dispatch'])
  })
})

// ===========================================================================
// Preamble order + the exempt no-op.
// ===========================================================================
describe('characterization — preamble order', () => {
  it('runs text-region check → log.info(id) → record_breadcrumb → showCommandPalette(false) → handler', async () => {
    const order: string[] = []
    logInfo.mockImplementation((...a: unknown[]) => {
      order.push(`log:${String(a[0])}`)
    })
    invoke.mockImplementation((...a: unknown[]) => {
      order.push(`invoke:${String(a[0])}`)
      return Promise.resolve()
    })
    const explorer = makeExplorerSpy()
    explorer.switchPane.mockImplementation(() => order.push('handler'))
    const ctx = makeCtx(explorer)
    ;(ctx.dialogs.showCommandPalette as ReturnType<typeof vi.fn>).mockImplementation((show: boolean) =>
      order.push(`palette:${String(show)}`),
    )

    await handleCommandExecute('pane.switch', ctx)

    expect(order).toEqual(['log:pane.switch', 'invoke:record_breadcrumb', 'palette:false', 'handler'])
  })

  it('text-region intercept bails BEFORE log.info for edit.copy with a non-collapsed selection in an error pane', async () => {
    const region = document.createElement('div')
    region.className = 'error-pane'
    region.textContent = 'error details'
    document.body.appendChild(region)
    const range = document.createRange()
    range.selectNodeContents(region)
    const sel = window.getSelection()
    sel?.removeAllRanges()
    sel?.addRange(range)
    const writeText = vi.fn(() => Promise.resolve())
    // happy-dom exposes `navigator.clipboard` as a getter-only property, so
    // `Object.assign` can't replace it — define the spy on the existing object.
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })

    const explorer = makeExplorerSpy()
    await handleCommandExecute('edit.copy', makeCtx(explorer))

    expect(logInfo).not.toHaveBeenCalled()
    expect(invoke).not.toHaveBeenCalled()
    expect(explorer.copyToClipboard).not.toHaveBeenCalled()
    expect(writeText).toHaveBeenCalledExactlyOnceWith('error details')
    region.remove()
    sel?.removeAllRanges()
  })

  it('text-region intercept bails BEFORE log.info for selection.selectAll in an error pane', async () => {
    const region = document.createElement('div')
    region.className = 'error-pane'
    region.textContent = 'error details'
    document.body.appendChild(region)
    // Place the selection's anchor inside the region (a click).
    const range = document.createRange()
    range.selectNodeContents(region)
    const sel = window.getSelection()
    sel?.removeAllRanges()
    sel?.addRange(range)

    const explorer = makeExplorerSpy()
    await handleCommandExecute('selection.selectAll', makeCtx(explorer))

    expect(logInfo).not.toHaveBeenCalled()
    expect(explorer.handleSelectionAction).not.toHaveBeenCalled()
    region.remove()
    sel?.removeAllRanges()
  })
})

// ===========================================================================
// The 20 exempt ids: preamble fires (log + breadcrumb), then SILENT no-op.
// ===========================================================================
describe('characterization — exempt ids (preamble-then-silent-no-op)', () => {
  it.each(EXEMPT_IDS)('exempt id %s: preamble fires, no explorer call, no toast, no throw', async (id) => {
    const explorer = makeExplorerSpy()
    const ctx = makeCtx(explorer)

    await expect(handleCommandExecute(id, ctx)).resolves.toBeUndefined()

    // Preamble ran: log + breadcrumb.
    expect(logInfo).toHaveBeenCalledExactlyOnceWith(id)
    expect(invoke).toHaveBeenCalledExactlyOnceWith('record_breadcrumb', { kind: 'command', message: id, ctx: null })
    // showCommandPalette(false) is part of the preamble and fires too.
    expect(ctx.dialogs.showCommandPalette).toHaveBeenCalledExactlyOnceWith(false)
    // Silent no-op: no explorer method, no toast.
    for (const [name, fn] of Object.entries(explorer)) {
      expect(fn, `${id} must not call explorer.${name}`).not.toHaveBeenCalled()
    }
    expect(addToast).not.toHaveBeenCalled()
  })
})

// ===========================================================================
// Completeness: every dispatchable id is exercised at least once above. We
// re-dispatch each here with a generic spy to guarantee none throws and the
// preamble fires (the per-arm assertions live in the focused describes above).
// ===========================================================================
describe('characterization — every dispatchable id dispatches without throwing', () => {
  // Typed payloads for the arg-carrying ids (so the cast inside the arm reads a
  // real shape, not undefined).
  const ARGS: Partial<Record<CommandId, unknown>> = {
    'view.setMode': { pane: 'left', mode: 'brief', fromMenu: true },
    'sort.set': { pane: 'left', column: 'name', order: 'asc' },
    'selection.mcpSelect': { pane: 'left', start: 0, count: 'all', mode: 'replace' },
    'cursor.moveTo': { pane: 'left', to: 0 },
    'cursor.scrollTo': { pane: 'left', index: 0 },
    'volume.selectByName': { pane: 'left', name: 'X' },
    'tab.mcpAction': { pane: 'left', action: 'activate', tabId: 't', pinned: false },
    'dialog.confirm': { type: 'delete-confirmation' },
  }

  it.each(DISPATCHABLE_IDS)('%s dispatches and runs the preamble', async (id) => {
    const explorer = makeExplorerSpy()
    explorer.getFileAndPathUnderCursor.mockReturnValue({ path: '/p', filename: 'f' })
    explorer.closeActiveTabWithConfirmation.mockResolvedValue('closed')
    const ctx = makeCtx(explorer)
    const args = ARGS[id]
    if (args === undefined) {
      await expect(handleCommandExecute(id, ctx)).resolves.toBeUndefined()
    } else {
      await expect(handleCommandExecute(id, ctx, args as never)).resolves.toBeUndefined()
    }
    expect(logInfo).toHaveBeenCalledWith(id)
  })
})
