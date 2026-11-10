/**
 * The Operation log's keyboard map (`Main window/Operation log`, `sources/operation-log.ts`),
 * row by row, through the real registry: each key is read by `eventMatchesCommand`, so a
 * rebound or renamed command fails here. ⌘A and ⌘⇧A also go down the native menu's road,
 * which the dialog gate refuses over any dialog, so those cells drive the real dispatch
 * core (`handleCommandExecute`) with the dialog mounted: a keydown cell alone passes while
 * the app fails.
 */

import { describe, it, vi, expect, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import type {
  Initiator,
  OperationRow,
  OperationTreeRequest,
  OperationTreeView,
  RollbackDispatch,
  RollbackSelection,
} from '#lib/ipc/bindings.js'
import OperationLogDialog from './OperationLogDialog.svelte'
import { operationLogState, closeOperationLog } from './operation-log-trigger.svelte'
import { fileRow, folderRow, tally, treeView } from './operation-tree-test-fixtures'
import { handleCommandExecute, type CommandDispatchContext } from '../../routes/(main)/command-dispatch'
import { _resetDedupForTests } from '../../routes/(main)/dispatch-dedup'
import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

const ipc = vi.hoisted(() => ({
  rollbackOperation:
    vi.fn<
      (operationId: string, initiator: Initiator, selection: RollbackSelection | null) => Promise<RollbackDispatch>
    >(),
  getOperationLogTree: vi.fn<(id: string, request: OperationTreeRequest) => Promise<OperationTreeView | null>>(),
}))

// Partial: the dispatch core's handler graph reads constants from the same barrel.
vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: vi.fn(() => Promise.resolve(null)),
  rollbackOperation: ipc.rollbackOperation,
}))
vi.mock('#lib/file-explorer/pane/focused-pane-reads.js', () => ({
  getFocusedPaneVolumeId: () => 'local',
  getFocusedPanePath: () => '/Users/test',
}))

vi.mock('#lib/tauri-commands/operation-log.js', () => ({
  getOperationLogTree: ipc.getOperationLogTree,
  startOperationLogTreeBackfill: vi.fn(() => Promise.resolve({ kind: 'started' })),
  cancelOperationLogTreeBackfill: vi.fn(() => Promise.resolve()),
  onOperationLogTreeBackfill: vi.fn(() => Promise.resolve(() => {})),
  onOperationLogTreeIndexesSettled: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  formatDateTime: () => '2026-07-09 12:00',
  getOperationLogRecording: () => true,
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))
// The dispatch preamble records a breadcrumb through `invoke`.
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }))

function opRow(opId: string): OperationRow {
  return {
    opId,
    kind: 'copy',
    archiveSubkind: null,
    initiator: 'user',
    executionStatus: 'done',
    rollbackState: 'rollbackable',
    notRollbackableReason: null,
    rollsBackOpId: null,
    inverseOpId: null,
    sourceVolumeId: 'root',
    destVolumeId: null,
    startedAt: 1_700_000_000,
    endedAt: 1_700_000_010,
    itemCount: 3,
    itemsDone: 3,
    bytesTotal: 0,
    searchCoverage: 'full',
    searchCoverageReason: null,
    devSummary: null,
  }
}

/** A folder copy, one open folder of two files, answering with the selection it was asked about. */
function folderCopy(selection: RollbackSelection | null): OperationTreeView {
  return treeView(
    [
      folderRow(1, { label: 'Photos', expanded: true, setSize: 1 }),
      fileRow(10, { label: 'a.jpg', level: 2, posInSet: 1, setSize: 2 }),
      fileRow(11, { label: 'b.jpg', level: 2, posInSet: 2, setSize: 2 }),
    ],
    {
      tally: tally({ files: 2, selected: 2, selectable: 2 }),
      selection: selection ?? { base: 'everything', toggles: [] },
    },
  )
}

/** Mount over `entries` without waiting for any tree. */
async function mountList(entries: OperationRow[]): Promise<HTMLElement> {
  operationLogState.entries = entries
  operationLogState.loading = false
  operationLogState.loadError = false
  operationLogState.hasMore = false
  operationLogState.open = true
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(OperationLogDialog, { target, props: {} })
  await tick()
  return target
}

async function mountWithTree(entries: OperationRow[]): Promise<HTMLElement> {
  const target = await mountList(entries)
  await vi.waitFor(() => {
    expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
  })
  return target
}

function opList(): HTMLElement {
  const grid = document.querySelector<HTMLElement>('#operation-log-body .op-list [role="grid"]')
  if (grid === null) throw new Error('no operation list')
  return grid
}

function treeGrid(): HTMLElement {
  const grid = document.querySelector<HTMLElement>('#operation-log-body [role="treegrid"]')
  if (grid === null) throw new Error('no item tree')
  return grid
}

function press(on: HTMLElement, key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  on.dispatchEvent(event)
  return event
}

function lastSelection(): RollbackSelection | null {
  const call = ipc.getOperationLogTree.mock.calls.at(-1)
  if (call === undefined) throw new Error('the tree was never asked for')
  return call[1].selection
}

function treeRequests(): number {
  return ipc.getOperationLogTree.mock.calls.length
}

function menuCtx(): CommandDispatchContext {
  return {
    getExplorer: () => ({}) as ExplorerAPI,
    dialogs: {
      showCommandPalette: vi.fn(),
      showSearchDialog: vi.fn(),
      showGoToPathDialog: vi.fn(),
      showAboutWindow: vi.fn(),
      showLicenseKeyDialog: vi.fn(),
      showSelectionDialog: vi.fn(),
      showMultiRename: vi.fn(),
      openOnboarding: vi.fn(),
    },
    source: 'menu',
    getDialogsOnScreen: () => ({ dialogOpen: true, paletteOpen: false }),
  }
}

// `formatKeyCombo` emits ⌘-form modifiers only when `isMacOS()` is true, and happy-dom reports a Linux UA.
const navigatorSpy = vi.spyOn(globalThis, 'navigator', 'get')

beforeEach(() => {
  navigatorSpy.mockReturnValue({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X)' } as Navigator)
  closeOperationLog()
  document.body.innerHTML = ''
  _resetDedupForTests()
  ipc.rollbackOperation.mockReset()
  ipc.rollbackOperation.mockResolvedValue({ inverseOpId: 'op-inverse' })
  ipc.getOperationLogTree.mockReset()
  ipc.getOperationLogTree.mockImplementation((_id, request) => Promise.resolve(folderCopy(request.selection)))
})

afterEach(() => {
  navigatorSpy.mockReset()
})

describe('the operation list', () => {
  it('goes into the picked operation’s items on → and on Enter', async () => {
    await mountWithTree([opRow('op-1')])
    for (const key of ['ArrowRight', 'Enter']) {
      opList().focus()
      const event = press(opList(), key)
      expect(event.defaultPrevented).toBe(true)
      expect(document.activeElement).toBe(treeGrid())
    }
  })

  it('remembers → while the tree still loads, and moves in once its rows arrive', async () => {
    let land: () => void = () => {}
    ipc.getOperationLogTree.mockImplementation(
      (_id, request) =>
        new Promise((resolve) => {
          land = () => {
            resolve(folderCopy(request.selection))
          }
        }),
    )
    const target = await mountList([opRow('op-1')])
    opList().focus()
    press(opList(), 'ArrowRight')
    expect(document.activeElement).toBe(opList())

    await vi.waitFor(() => {
      expect(ipc.getOperationLogTree).toHaveBeenCalled()
    })
    land()
    await vi.waitFor(() => {
      expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
    })
    await vi.waitFor(() => {
      expect(document.activeElement).toBe(treeGrid())
    })
  })

  it('drops the remembered → when the list cursor moves on before the tree loads', async () => {
    const target = await mountList([opRow('op-1'), opRow('op-2')])
    opList().focus()
    press(opList(), 'ArrowRight')
    press(opList(), 'ArrowDown')
    await vi.waitFor(() => {
      expect(ipc.getOperationLogTree.mock.calls.at(-1)?.[0]).toBe('op-2')
    })
    await vi.waitFor(() => {
      expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
    })
    await tick()
    expect(document.activeElement).toBe(opList())
  })

  it('gets the focus back from ← on a top-level row of the tree', async () => {
    ipc.getOperationLogTree.mockImplementation(() =>
      Promise.resolve(treeView([fileRow(1), fileRow(2)], { tally: tally({ selected: 2, selectable: 2 }) })),
    )
    await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    const event = press(treeGrid(), 'ArrowLeft')
    expect(event.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(opList())
  })
})

describe('the item tree', () => {
  it('checks or unchecks the cursor row on Space, a folder as one pick', async () => {
    await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    const event = press(treeGrid(), ' ', { code: 'Space' })
    expect(event.defaultPrevented).toBe(true)
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({ base: 'everything', toggles: [{ kind: 'folder', dirId: 1, on: false }] })
    })
  })

  it('checks or unchecks on Insert, then moves down, so a run of Inserts walks the tree', async () => {
    await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    press(treeGrid(), 'ArrowDown')
    press(treeGrid(), 'Insert')
    press(treeGrid(), 'Insert')
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({
        base: 'everything',
        toggles: [
          { kind: 'item', seq: 10, on: false },
          { kind: 'item', seq: 11, on: false },
        ],
      })
    })
  })

  it('a checkbox click keeps the focus on the tree, and Space then toggles exactly once more', async () => {
    const target = await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    const box = target.querySelector<HTMLInputElement>('.tree .column-list-row.is-data input[type="checkbox"]')
    if (box === null) throw new Error('no checkbox on the top row')
    box.click()
    await vi.waitFor(() => {
      expect(lastSelection()?.toggles).toHaveLength(1)
    })
    expect(document.activeElement).toBe(treeGrid())

    press(treeGrid(), ' ', { code: 'Space' })
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({
        base: 'everything',
        toggles: [
          { kind: 'folder', dirId: 1, on: false },
          { kind: 'folder', dirId: 1, on: true },
        ],
      })
    })
  })

  it('checks nothing on ⌘⇧A and everything on ⌘A, from its keydown', async () => {
    await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    press(treeGrid(), 'A', { code: 'KeyA', metaKey: true, shiftKey: true })
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({ base: 'nothing', toggles: [] })
    })
    press(treeGrid(), 'a', { code: 'KeyA', metaKey: true })
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({ base: 'everything', toggles: [] })
    })
  })
})

describe('⌘A and ⌘⇧A down the native menu’s road', () => {
  it('reach the tree while it holds focus, past the gate that refuses them over a dialog', async () => {
    await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    await handleCommandExecute('selection.deselectAll', menuCtx())
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({ base: 'nothing', toggles: [] })
    })
    await handleCommandExecute('selection.selectAll', menuCtx())
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({ base: 'everything', toggles: [] })
    })
  })

  it('land as one pick when the keydown and the menu both fire for one press', async () => {
    await mountWithTree([opRow('op-1')])
    treeGrid().focus()
    press(treeGrid(), 'A', { code: 'KeyA', metaKey: true, shiftKey: true })
    await handleCommandExecute('selection.deselectAll', menuCtx())
    await vi.waitFor(() => {
      expect(lastSelection()).toEqual({ base: 'nothing', toggles: [] })
    })
    const asked = treeRequests()
    await tick()
    expect(treeRequests()).toBe(asked)
  })

  it('pass on when the tree doesn’t hold focus, and the gate refuses them as before', async () => {
    await mountWithTree([opRow('op-1')])
    opList().focus()
    const asked = treeRequests()
    await handleCommandExecute('selection.deselectAll', menuCtx())
    await tick()
    expect(treeRequests()).toBe(asked)
  })
})

describe('anywhere in the dialog', () => {
  it('asks to roll back on ⌘↩, from the tree, the list, and a focused button, and the button shows the key', async () => {
    await mountWithTree([opRow('op-1')])
    const rollBack = [...document.querySelectorAll<HTMLButtonElement>('.op-actions button.btn')].find((b) =>
      b.textContent.trim().startsWith('Roll back'),
    )
    expect(rollBack?.querySelector('.shortcut-chip')?.textContent).toBe('⌘↩')
    const close = [...document.querySelectorAll<HTMLButtonElement>('.op-actions button.btn')].find(
      (b) => b.textContent.trim() === 'Close',
    )
    if (close === undefined) throw new Error('no Close button')

    for (const from of [treeGrid(), opList(), close]) {
      from.focus()
      press(from, 'Enter', { code: 'Enter', metaKey: true })
      await tick()
      expect(document.querySelector('[data-dialog-id="rollback-confirmation"]')).not.toBeNull()
      const keep = [
        ...document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="rollback-confirmation"] button.btn'),
      ].find((b) => b.textContent.trim() === 'Leave it as is')
      keep?.click()
      await tick()
      expect(document.querySelector('[data-dialog-id="rollback-confirmation"]')).toBeNull()
    }
    expect(ipc.rollbackOperation).not.toHaveBeenCalled()
    expect(operationLogState.open).toBe(true)
  })
})
