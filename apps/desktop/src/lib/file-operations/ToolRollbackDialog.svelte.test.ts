/**
 * The third host of `RollbackConfirmDialog`: a question a tool asked for. What it pins:
 *
 * - it words the question from the operation's facts,
 * - it never stacks: a modal in the way refuses typed, and a progress dialog showing the
 *   operation takes the question itself,
 * - no answer, nothing runs; a yes takes the path a person's press takes, under the initiator
 *   that asked; a question about an operation that can't roll back any more comes down.
 */
import { flushSync, mount, tick, unmount } from 'svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { OperationRow } from './queue/operations-store.svelte'
import { pressRollbackConfirm } from './rollback-confirmers'

const rollbackOperation = vi.hoisted(() => vi.fn())
const sessionRollback = vi.hoisted(() => vi.fn())
const addToast = vi.hoisted(() => vi.fn())

vi.mock('./queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => live.rows,
}))
vi.mock('#lib/tauri-commands/index.js', () => ({
  rollbackOperation: (...args: unknown[]): unknown => rollbackOperation(...args),
  notifyDialogOpened: vi.fn(),
  notifyDialogClosed: vi.fn(),
}))
vi.mock('#lib/ui/toast/index.js', () => ({
  addToast: (...args: unknown[]): unknown => addToast(...args),
}))
vi.mock('./operation-session/bind-operation-session.svelte', () => ({
  bindOperationSession: () => ({ current: { rollback: (): unknown => sessionRollback() } }),
}))

import ToolRollbackDialog from './ToolRollbackDialog.svelte'
import { RollbackRefusalFailure } from '#lib/operation-log/rollback-refusal.js'
import { _resetOpenDialogsForTesting, markDialogOpen } from '#lib/ui/open-dialogs.svelte.js'
import {
  dismissToolRollback,
  getToolRollbackQuestion,
  raiseToolRollback,
  registerProgressRollbackAsker,
} from './tool-rollback.svelte'

function row(overrides: {
  status?: string
  operationType?: string
  rollbackSupport?: string
  phase?: string | null
}): OperationRow {
  const { status = 'running', operationType = 'copy', rollbackSupport = 'supported', phase = 'copying' } = overrides
  return {
    snapshot: { operationId: 'op-1', status, operationType, rollbackSupport },
    progress: phase === null ? null : { phase },
  } as unknown as OperationRow
}

const LIVE = { target: 'live', operationId: 'op-1', initiator: 'aiClient' } as const
const LOGGED = { target: 'logged', operationId: 'op-1', initiator: 'aiClient', kind: 'move', finishing: false } as const

/** The real store's rows are reactive, and so are these. */
const live = $state({ rows: [] as OperationRow[] })

let target: HTMLElement
let host: ReturnType<typeof mount> | undefined

function mountHost(): void {
  host = mount(ToolRollbackDialog, { target })
  flushSync()
}

function dialog(): Element | null {
  return document.querySelector('[data-dialog-id="rollback-confirmation"]')
}

/** The safe answer, which holds focus in every variant. */
function keepButton(): HTMLButtonElement {
  const button = document.querySelector<HTMLButtonElement>(
    '[data-dialog-id="rollback-confirmation"] .modal-footer-actions .btn-secondary',
  )
  if (button === null) throw new Error('no safe answer on the dialog')
  return button
}

beforeEach(() => {
  target = document.createElement('div')
  document.body.appendChild(target)
  live.rows = []
  rollbackOperation.mockReset()
  sessionRollback.mockReset()
  addToast.mockReset()
  _resetOpenDialogsForTesting()
})

afterEach(() => {
  dismissToolRollback()
  flushSync()
  if (host !== undefined) void unmount(host)
  host = undefined
  target.remove()
})

describe('raising the question', () => {
  it('words a finished move as a move back, and a finished copy as a delete', () => {
    expect(raiseToolRollback(LOGGED)).toEqual({ ok: true })
    expect(getToolRollbackQuestion()?.variant).toBe('undoByMovingBack')
    expect(raiseToolRollback({ ...LOGGED, kind: 'copy' })).toEqual({ ok: true })
    expect(getToolRollbackQuestion()?.variant).toBe('undoByDeleting')
  })

  it('words a running copy and a running move by what stopping them does', () => {
    live.rows = [row({ operationType: 'copy' })]
    expect(raiseToolRollback(LIVE)).toEqual({ ok: true })
    expect(getToolRollbackQuestion()?.variant).toBe('stopAndDelete')
    live.rows = [row({ operationType: 'move' })]
    expect(raiseToolRollback(LIVE)).toEqual({ ok: true })
    expect(getToolRollbackQuestion()?.variant).toBe('stopAndMoveBack')
  })

  it('refuses typed when the moment has passed, and raises nothing', () => {
    for (const [overrides, refusal] of [
      [{ phase: 'scanning' }, 'nothingWrittenYet'],
      [{ phase: 'rolling_back' }, 'alreadyRollingBack'],
      [{ status: 'queued', phase: null }, 'notRunning'],
      [{ operationType: 'move', phase: 'deleting' }, 'reversalWindowClosed'],
    ] as const) {
      live.rows = [row(overrides)]
      expect(raiseToolRollback(LIVE)).toMatchObject({ ok: false, refusal })
      expect(getToolRollbackQuestion()).toBeNull()
    }
    live.rows = []
    expect(raiseToolRollback(LIVE)).toMatchObject({ ok: false, refusal: 'notRunning' })
  })

  it('refuses typed with another modal up, and never stacks on it', () => {
    markDialogOpen('operation-log')
    expect(raiseToolRollback(LOGGED)).toMatchObject({ ok: false, blockedBy: 'operation-log' })
    expect(getToolRollbackQuestion()).toBeNull()
  })

  it("a finished operation's question doesn't stack on a progress dialog either", () => {
    markDialogOpen('transfer-progress')
    expect(raiseToolRollback(LOGGED)).toMatchObject({ ok: false, blockedBy: 'transfer-progress' })
  })

  it('a running operation with a progress dialog on show asks through that dialog', () => {
    markDialogOpen('transfer-progress')
    const ask = vi.fn(() => null)
    const unregister = registerProgressRollbackAsker('op-1', ask)
    expect(raiseToolRollback(LIVE)).toEqual({ ok: true })
    expect(ask).toHaveBeenCalledOnce()
    expect(getToolRollbackQuestion()).toBeNull()

    // The same dialog already asking is still that one question, ❌ not an obstacle.
    markDialogOpen('rollback-confirmation')
    expect(raiseToolRollback(LIVE)).toEqual({ ok: true })
    unregister()
  })

  it('passes on what the progress dialog says, and what else is over it', () => {
    markDialogOpen('transfer-progress')
    const unregister = registerProgressRollbackAsker('op-1', () => 'nothingWrittenYet')
    expect(raiseToolRollback(LIVE)).toMatchObject({ ok: false, refusal: 'nothingWrittenYet' })
    markDialogOpen('alert')
    expect(raiseToolRollback(LIVE)).toMatchObject({ ok: false, blockedBy: 'alert' })
    unregister()
  })

  it('a progress dialog showing ANOTHER operation is in the way', () => {
    markDialogOpen('transfer-progress')
    const unregister = registerProgressRollbackAsker('op-other', () => null)
    live.rows = [row({})]
    expect(raiseToolRollback(LIVE)).toMatchObject({ ok: false, blockedBy: 'transfer-progress' })
    unregister()
  })
})

describe('answering it', () => {
  it('runs nothing until answered, and nothing on a no', async () => {
    mountHost()
    raiseToolRollback(LOGGED)
    await tick()
    expect(dialog()).not.toBeNull()
    expect(rollbackOperation).not.toHaveBeenCalled()

    keepButton().click()
    await tick()
    expect(dialog()).toBeNull()
    expect(rollbackOperation).not.toHaveBeenCalled()
  })

  it('a yes on a finished operation dispatches under the initiator that asked', async () => {
    rollbackOperation.mockResolvedValue({ inverseOpId: 'op-inverse' })
    mountHost()
    raiseToolRollback(LOGGED)
    await tick()

    await expect(pressRollbackConfirm()).resolves.toEqual({ status: 'dispatched', inverseOpId: 'op-inverse' })
    expect(rollbackOperation).toHaveBeenCalledWith('op-1', 'aiClient')
    await tick()
    expect(dialog()).toBeNull()
  })

  it('a refused yes comes back typed, and tells the person', async () => {
    rollbackOperation.mockRejectedValue(new RollbackRefusalFailure({ kind: 'alreadyRolledBack' }))
    mountHost()
    raiseToolRollback(LOGGED)
    await tick()

    await expect(pressRollbackConfirm()).resolves.toEqual({
      status: 'refused',
      refusal: { kind: 'alreadyRolledBack' },
    })
    expect(addToast).toHaveBeenCalledOnce()
  })

  it('a yes on a running operation takes the cancel-with-rollback path', async () => {
    live.rows = [row({})]
    sessionRollback.mockResolvedValue(true)
    mountHost()
    raiseToolRollback(LIVE)
    await tick()

    await expect(pressRollbackConfirm()).resolves.toEqual({ status: 'dispatched', inverseOpId: null })
    expect(sessionRollback).toHaveBeenCalledOnce()
    expect(rollbackOperation).not.toHaveBeenCalled()
  })

  it('says so when a stop was already on its way', async () => {
    live.rows = [row({})]
    sessionRollback.mockResolvedValue(false)
    mountHost()
    raiseToolRollback(LIVE)
    await tick()

    await expect(pressRollbackConfirm()).resolves.toEqual({
      status: 'refused',
      refusal: { kind: 'alreadyStopping' },
    })
  })

  it('comes down when a running operation can no longer roll back', async () => {
    live.rows = [row({})]
    mountHost()
    raiseToolRollback(LIVE)
    await tick()
    expect(dialog()).not.toBeNull()

    live.rows = [row({ phase: 'rolling_back' })]
    await tick()
    expect(dialog()).toBeNull()
    expect(sessionRollback).not.toHaveBeenCalled()
  })

  it('comes down when the operation ends', async () => {
    live.rows = [row({})]
    mountHost()
    raiseToolRollback(LIVE)
    await tick()

    live.rows = []
    await tick()
    expect(dialog()).toBeNull()
  })

  it('has nothing to press with no question up', async () => {
    mountHost()
    await expect(pressRollbackConfirm()).resolves.toBeNull()
  })
})
