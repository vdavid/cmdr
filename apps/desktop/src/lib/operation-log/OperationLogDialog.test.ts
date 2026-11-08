/**
 * Component tests for `OperationLogDialog.svelte`, the master–detail Operation log:
 * the list picks an operation, the detail shows its header, notice, and item tree, and
 * the action bar rolls back what's checked. The tree is the backend's, so it's mocked
 * at `get_operation_log_tree`: what's pinned here is what the dialog renders from an
 * answer and what it sends back. The source's own request rules (generations, settling,
 * backfill) are `operation-tree-source.svelte.test.ts`; paging is
 * `operation-log-trigger.test.ts`.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import type {
  Initiator,
  OperationRow,
  RollbackDispatch,
  OperationTreeRequest,
  OperationTreeView,
  RollbackSelection,
  TreeBackfillStart,
} from '#lib/ipc/bindings.js'
import OperationLogDialog from './OperationLogDialog.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import { operationLogState, closeOperationLog } from './operation-log-trigger.svelte'
import { RollbackRefusalFailure } from './rollback-refusal'
import { fileRow, folderRow, tally, treeView } from './operation-tree-test-fixtures'

const ipc = vi.hoisted(() => ({
  rollbackOperation:
    vi.fn<
      (operationId: string, initiator: Initiator, selection: RollbackSelection | null) => Promise<RollbackDispatch>
    >(),
  getOperationLogTree: vi.fn<(id: string, request: OperationTreeRequest) => Promise<OperationTreeView | null>>(),
  startOperationLogTreeBackfill: vi.fn<(id: string) => Promise<TreeBackfillStart>>(),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  getRecentOperationLogEntries: vi.fn(() => Promise.resolve([])),
  getOperationLogDetail: vi.fn(() => Promise.resolve(null)),
  rollbackOperation: ipc.rollbackOperation,
}))

vi.mock('#lib/tauri-commands/operation-log.js', () => ({
  getOperationLogTree: ipc.getOperationLogTree,
  startOperationLogTreeBackfill: ipc.startOperationLogTreeBackfill,
  cancelOperationLogTreeBackfill: vi.fn(() => Promise.resolve()),
  onOperationLogTreeBackfill: vi.fn(() => Promise.resolve(() => {})),
}))

// Avoid pulling the reactive-settings chain; a stable stamp is all the header needs.
const recordingOn = vi.hoisted(() => ({ value: true }))
vi.mock('#lib/settings/reactive-settings.svelte.js', () => ({
  getFileSizeFormat: () => 'binary',
  formatDateTime: () => '2026-07-09 12:00',
  getOperationLogRecording: () => recordingOn.value,
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), debug: vi.fn(), error: vi.fn() }),
}))

function opRow(overrides: Partial<OperationRow>): OperationRow {
  return {
    opId: 'op-1',
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
    ...overrides,
  }
}

function setEntries(entries: OperationRow[]): void {
  operationLogState.entries = entries
  operationLogState.loading = false
  operationLogState.loadError = false
  operationLogState.hasMore = false
  operationLogState.open = true
}

async function mountDialog(): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(OperationLogDialog, { target, props: {} })
  await tick()
  return target
}

/** Mount over `entries` and wait for the first operation's tree to land. */
async function mountWithTree(entries: OperationRow[]): Promise<HTMLElement> {
  setEntries(entries)
  const target = await mountDialog()
  await vi.waitFor(() => {
    expect(target.querySelector('.tree .column-list-row.is-data')).not.toBeNull()
  })
  return target
}

function lastTreeRequest(): OperationTreeRequest {
  const call = ipc.getOperationLogTree.mock.calls.at(-1)
  if (call === undefined) throw new Error('the tree was never asked for')
  return call[1]
}

function treeRows(target: HTMLElement): HTMLElement[] {
  return [...target.querySelectorAll<HTMLElement>('.tree .column-list-row.is-data')]
}

/** A tree row's checkbox input, by the row's position. */
function rowCheckbox(target: HTMLElement, index: number): HTMLInputElement | null {
  return treeRows(target)[index]?.querySelector<HTMLInputElement>('input[type="checkbox"]') ?? null
}

/** The action bar's rollback button (the primary one beside Close). */
function actionButton(): HTMLButtonElement | null {
  const buttons = [...document.querySelectorAll<HTMLButtonElement>('.op-actions button.btn')]
  return buttons.find((b) => b.textContent.trim().startsWith('Roll back')) ?? null
}

function confirmButtons(): HTMLButtonElement[] {
  return [...document.querySelectorAll<HTMLButtonElement>('[data-dialog-id="rollback-confirmation"] button.btn')]
}

function answerConfirmation(label: string): void {
  const button = confirmButtons().find((b) => b.textContent.trim() === label)
  if (button === undefined) throw new Error(`no "${label}" button in the rollback confirmation`)
  button.click()
}

/** A folder copy: one open folder of two files, everything checked. */
function folderCopy(over: Partial<OperationTreeView> = {}): OperationTreeView {
  return treeView(
    [
      folderRow(1, { label: 'Photos', expanded: true, setSize: 1 }),
      fileRow(10, { label: 'a.jpg', level: 2, posInSet: 1, setSize: 2 }),
      fileRow(11, { label: 'b.jpg', level: 2, posInSet: 2, setSize: 2 }),
    ],
    { tally: tally({ files: 2, selected: 2, selectable: 2 }), ...over },
  )
}

beforeEach(() => {
  recordingOn.value = true
  closeOperationLog()
  document.body.innerHTML = ''
  ipc.rollbackOperation.mockReset()
  ipc.rollbackOperation.mockResolvedValue({ inverseOpId: 'op-inverse' })
  ipc.getOperationLogTree.mockReset()
  ipc.getOperationLogTree.mockResolvedValue(folderCopy())
  ipc.startOperationLogTreeBackfill.mockReset()
  ipc.startOperationLogTreeBackfill.mockResolvedValue({ kind: 'started' })
})

describe('the operation list', () => {
  it('lists every operation with a client-formatted summary, and details the first', async () => {
    setEntries([
      opRow({ opId: 'op-copy', kind: 'copy', itemCount: 3 }),
      opRow({ opId: 'op-rename', kind: 'rename', itemCount: 2_140, initiator: 'aiClient' }),
    ])
    const target = await mountDialog()

    const list = target.querySelector('.op-list')
    expect(list?.textContent).toContain('Copied 3 items')
    // ICU plural with a thousands separator, formatted per viewer.
    expect(list?.textContent).toContain('Renamed 2,140 items')
    expect(list?.textContent).toContain('AI client')
    // A stable hook per row for the E2E suite.
    expect(target.querySelectorAll('.op-list [data-op-id]')).toHaveLength(2)
    expect(target.querySelector('#op-head-op-copy')?.textContent).toBe('Copied 3 items')
  })

  it('shows the picked operation’s detail and reads its tree once the cursor rests', async () => {
    setEntries([opRow({ opId: 'op-a' }), opRow({ opId: 'op-b', kind: 'move', itemCount: 5 })])
    const target = await mountDialog()
    await vi.waitFor(() => {
      expect(ipc.getOperationLogTree).toHaveBeenCalledWith('op-a', expect.anything())
    })

    target.querySelectorAll<HTMLElement>('.op-list .column-list-row.is-data')[1].click()
    await tick()
    // The header follows at once; the tree follows once the cursor has rested.
    expect(target.querySelector('#op-head-op-b')?.textContent).toBe('Moved 5 items')
    await vi.waitFor(() => {
      expect(ipc.getOperationLogTree).toHaveBeenCalledWith('op-b', expect.anything())
    })
  })

  it('shows the ALPHA badge', async () => {
    setEntries([opRow({})])
    const target = await mountDialog()
    // StatusBadge renders the raw status text ('alpha'); CSS uppercases it visually.
    expect(target.querySelector('.feature-status-badge')?.textContent).toBe('alpha')
  })

  it('shows the empty state when no operations are logged', async () => {
    setEntries([])
    const target = await mountDialog()
    expect(target.textContent).toContain('No operations yet')
  })

  it('says recording is off in place of the empty state, so an empty log doesn’t read as “nothing happened”', async () => {
    recordingOn.value = false
    setEntries([])
    const target = await mountDialog()
    expect(target.querySelector('.recording-off')?.textContent).toContain('isn’t recording')
    expect(target.textContent).not.toContain('No operations yet')
  })

  it('says recording is off above the history, which explains why it stops at a point', async () => {
    recordingOn.value = false
    setEntries([opRow({ opId: 'op-old' })])
    const target = await mountDialog()
    expect(target.querySelector('.recording-off')).not.toBeNull()
    expect(target.querySelectorAll('.op-list [data-op-id]')).toHaveLength(1)
  })
})

describe('the detail header', () => {
  it('says "Finished" and the rollback state, and the route from the tree’s roots', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    const badges = [...target.querySelectorAll('.op-badge')].map((b) => b.textContent.trim())
    expect(badges).toEqual(['Finished', 'Can roll back'])
    expect(target.querySelector('.op-route')?.textContent).toBe('From /src to /dest')
  })

  it('says what rolling back does, per what the reversal does', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy', kind: 'copy' })])
    expect(target.querySelector('.op-reason')?.textContent).toBe(
      'Rolling back deletes the selected copies. The originals stay where they are.',
    )
  })

  it('tells the person why an operation can’t be rolled back', async () => {
    ipc.getOperationLogTree.mockResolvedValue(folderCopy({ readOnly: true }))
    const target = await mountWithTree([
      opRow({
        opId: 'op-gone',
        kind: 'delete',
        rollbackState: 'notRollbackable',
        notRollbackableReason: 'permanentDelete',
      }),
    ])
    expect(target.querySelector('.op-reason')?.textContent).toBe('A permanent delete leaves nothing to put back.')
  })

  it('says what became of a partly rolled-back operation, from the tree’s counts', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      folderCopy({ tally: tally({ files: 8, rolledBack: 4, heldBack: 2, selectable: 4, selected: 4 }) }),
    )
    const target = await mountWithTree([opRow({ opId: 'op-part', kind: 'move', rollbackState: 'partiallyRolledBack' })])
    await vi.waitFor(() => {
      expect(target.querySelector('.op-reason')?.textContent).toBe(
        '4 items are back the way they were. Cmdr skipped 2 items it wasn’t sure about. 2 items haven’t been rolled back yet.',
      )
    })
  })
})

describe('the item tree', () => {
  it('starts with everything checked, and the bar counts it', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    expect(lastTreeRequest().selection).toBeNull()
    expect(rowCheckbox(target, 1)?.checked).toBe(true)
    expect(document.querySelector('.selected-count')?.textContent).toBe('2 of 2 selected')
    expect(actionButton()?.textContent.trim()).toBe('Roll back selected (2)')
  })

  it('unchecks a folder as one pick, and the answer cascades to what’s under it', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    const off = { base: 'everything' as const, toggles: [{ kind: 'folder' as const, dirId: 1, on: false }] }
    ipc.getOperationLogTree.mockResolvedValue(
      treeView(
        [
          folderRow(1, { label: 'Photos', expanded: true, checkbox: 'off' }),
          fileRow(10, { label: 'a.jpg', level: 2, checkbox: 'off' }),
          fileRow(11, { label: 'b.jpg', level: 2, checkbox: 'off' }),
        ],
        { selection: off, tally: tally({ files: 2, selected: 0, selectable: 2 }) },
      ),
    )

    rowCheckbox(target, 0)?.click()
    await vi.waitFor(() => {
      expect(rowCheckbox(target, 2)?.checked).toBe(false)
    })
    expect(lastTreeRequest().selection).toEqual(off)
    // 0 of 2: nothing a press could do, so the press is off.
    expect(actionButton()?.disabled).toBe(true)
    expect(document.querySelector('.selected-count')?.textContent).toBe('0 of 2 selected')
  })

  it('shows the header box mixed when some of the operation is checked', async () => {
    ipc.getOperationLogTree.mockResolvedValue(folderCopy({ tally: tally({ files: 2, selected: 1, selectable: 2 }) }))
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    const header = target.querySelector('.tree .column-list-header')
    const box = header?.querySelector<HTMLInputElement>('input[type="checkbox"]')
    expect(box?.getAttribute('aria-label')).toBe('Select all')
    expect(header?.querySelector('[data-state="indeterminate"]')).not.toBeNull()
  })

  it('greys a row inside a folder unit and says it comes back with its folder', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([
        folderRow(1, { label: 'Invoices', role: 'selectable', expanded: true }),
        fileRow(10, { label: 'inv-1.pdf', level: 2, role: 'contents' }),
      ]),
    )
    const target = await mountWithTree([opRow({ opId: 'op-trash', kind: 'trash' })])
    const box = rowCheckbox(target, 1)
    expect(box?.disabled).toBe(true)
    expect(box?.getAttribute('aria-label')).toBe('Comes back with its folder.')
    // The row's accessible description carries the same words a pointer user hovers for.
    const describedBy = treeRows(target)[1].getAttribute('aria-describedby')
    expect(describedBy && document.getElementById(describedBy)?.textContent).toBe('Comes back with its folder.')
  })

  it('has no checkboxes on a read-only operation, nor on one rolling back', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([fileRow(1, { checkbox: 'none' }), fileRow(2, { checkbox: 'none' })], { readOnly: true }),
    )
    const target = await mountWithTree([
      opRow({
        opId: 'op-del',
        kind: 'delete',
        rollbackState: 'notRollbackable',
        notRollbackableReason: 'permanentDelete',
      }),
    ])
    expect(target.querySelector('.tree input[type="checkbox"]')).toBeNull()
    expect(actionButton()).toBeNull()
    expect(document.querySelector('.selected-count')).toBeNull()
  })

  it('speaks in the status column only when it matters, worded by the operation', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([
        fileRow(1, { label: 'index.html', role: 'settled', outcome: 'rolledBack', checkbox: 'none' }),
        fileRow(2, { label: 'hero.jpg', outcome: 'skipped', skipReason: 'drift' }),
        fileRow(3, { label: 'notes.md' }),
      ]),
    )
    const target = await mountWithTree([opRow({ opId: 'op-move', kind: 'move', rollbackState: 'partiallyRolledBack' })])
    const statuses = treeRows(target).map((row) => row.querySelector('.status-word')?.textContent ?? '')
    expect(statuses).toEqual(['Moved back', 'Skipped', ''])
  })

  it('shows the cursor row’s story under the tree', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([
        fileRow(1, { label: 'hero.jpg', outcome: 'skipped', skipReason: 'restoreTargetOccupied' }),
        fileRow(2),
      ]),
    )
    const target = await mountWithTree([opRow({ opId: 'op-move', kind: 'move' })])
    expect(target.querySelector('.tree-tip')?.textContent).toBe(
      'Something else is at the original spot now, so Cmdr didn’t overwrite it.',
    )
  })

  it('says so when a pick goes past what Cmdr can hold, and keeps the selection', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    ipc.getOperationLogTree.mockResolvedValue(folderCopy({ togglesCapped: true }))
    rowCheckbox(target, 1)?.click()
    await vi.waitFor(() => {
      expect(target.querySelector('.tree-capped')?.textContent).toBe(
        'That’s as many separate picks as Cmdr can hold. Pick a folder instead.',
      )
    })
  })

  it('says an old operation is being read while its backfill runs', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([], { summary: { kind: 'unbuilt', reason: 'notBuilt' }, totalRows: 0 }),
    )
    setEntries([opRow({ opId: 'op-old' })])
    const target = await mountDialog()
    await vi.waitFor(() => {
      expect(ipc.startOperationLogTreeBackfill).toHaveBeenCalledWith('op-old')
    })
    await vi.waitFor(() => {
      expect(target.querySelector('.tree-blank [role="status"]')?.textContent).toContain(
        'Reading this operation’s items',
      )
    })
  })
})

describe('rolling back', () => {
  it('asks first, and dispatches nothing on a no', async () => {
    await mountWithTree([opRow({ opId: 'op-copy' })])
    actionButton()?.click()
    await tick()
    expect(confirmButtons()[0].textContent.trim()).toBe('Leave it as is')
    answerConfirmation('Leave it as is')
    await tick()
    expect(ipc.rollbackOperation).not.toHaveBeenCalled()
  })

  it('sends no selection when nothing was narrowed, and words the question as the whole operation', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    actionButton()?.click()
    await tick()
    expect(document.querySelector('#rollback-confirmation-title')?.textContent.trim()).toBe('Roll this operation back?')
    answerConfirmation('Roll back')
    await vi.waitFor(() => {
      expect(ipc.rollbackOperation).toHaveBeenCalledWith('op-copy', 'user', null)
    })
    await vi.waitFor(() => {
      expect(target.querySelector('.op-badge-rollback')?.textContent).toBe('Rolling back')
    })
  })

  it('sends the normalized toggles for a subset, and says "selected" only then', async () => {
    const target = await mountWithTree([opRow({ opId: 'op-copy' })])
    const narrowed = { base: 'everything' as const, toggles: [{ kind: 'item' as const, seq: 11, on: false }] }
    ipc.getOperationLogTree.mockResolvedValue(
      folderCopy({ selection: narrowed, tally: tally({ files: 2, selected: 1, selectable: 2 }) }),
    )
    rowCheckbox(target, 2)?.click()
    await vi.waitFor(() => {
      expect(actionButton()?.textContent.trim()).toBe('Roll back selected (1)')
    })

    actionButton()?.click()
    await tick()
    expect(document.querySelector('#rollback-confirmation-title')?.textContent.trim()).toBe(
      'Roll back the selected items?',
    )
    expect(document.querySelector('#rollback-confirmation-body')?.textContent).toContain(
      'deletes the selected files and folders',
    )
    answerConfirmation('Roll back selected')
    await vi.waitFor(() => {
      expect(ipc.rollbackOperation).toHaveBeenCalledWith('op-copy', 'user', narrowed)
    })
  })

  it('offers "Roll back the rest" on a partly rolled-back operation the person hasn’t narrowed', async () => {
    await mountWithTree([opRow({ opId: 'op-part', kind: 'copy', rollbackState: 'partiallyRolledBack' })])
    expect(actionButton()?.textContent.trim()).toBe('Roll back the rest')
    actionButton()?.click()
    await tick()
    expect(document.querySelector('#rollback-confirmation-title')?.textContent.trim()).toBe('Roll back the rest?')
    // What it does to the files doesn't change because half of it already happened.
    expect(document.querySelector('#rollback-confirmation-body')?.textContent).toContain(
      'deletes the files and folders the operation created',
    )
    answerConfirmation('Roll back the rest')
    await vi.waitFor(() => {
      expect(ipc.rollbackOperation).toHaveBeenCalledWith('op-part', 'user', null)
    })
  })

  it('never strands an operation at 0 of 0: a plain "Roll back" sweeps what’s left', async () => {
    ipc.getOperationLogTree.mockResolvedValue(
      treeView([folderRow(1, { role: 'housekeeping', checkbox: 'none', expandable: false })], {
        tally: tally({ selectable: 0, selected: 0, housekeepingWaiting: 1 }),
      }),
    )
    await mountWithTree([opRow({ opId: 'op-copy' })])
    expect(actionButton()?.textContent.trim()).toBe('Roll back')
    expect(actionButton()?.disabled).toBe(false)
  })

  it('says WHY a rollback was refused, rather than looking like nothing happened', async () => {
    ipc.rollbackOperation.mockRejectedValue(new RollbackRefusalFailure({ kind: 'alreadyRollingBack' }))
    const target = await mountWithTree([opRow({ opId: 'op-raced' })])
    actionButton()?.click()
    await tick()
    answerConfirmation('Roll back')
    await vi.waitFor(() => {
      expect(target.querySelector('.op-refusal')?.textContent).toContain('already rolling back')
    })
    expect(actionButton()).not.toBeNull()
  })

  it('has no a11y violations with the tree and the action bar rendered', async () => {
    const target = await mountWithTree([
      opRow({ opId: 'op-copy' }),
      opRow({ opId: 'op-del', kind: 'delete', rollbackState: 'notRollbackable' }),
    ])
    await expectNoA11yViolations(target)
  })
})
