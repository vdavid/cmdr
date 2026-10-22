/**
 * The main window's host for "these items are Locked: move them anyway?"
 * (#408). `#lib/tauri-commands` is mocked and the registered
 * `onWriteLockedSources` callback captured, so each case delivers the backend's
 * event and asserts what the person sees and what the host sends back.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { OperationSnapshot, WriteLockedSourcesEvent } from '#lib/tauri-commands/index.js'
import type { OperationRow } from './queue/operations-store.svelte'

let lockedCb: ((e: WriteLockedSourcesEvent) => void) | null = null

vi.mock('#lib/tauri-commands/index.js', () => ({
  onWriteLockedSources: vi.fn((cb: (e: WriteLockedSourcesEvent) => void) => {
    lockedCb = cb
    return Promise.resolve(() => {})
  }),
  answerLockedSources: vi.fn(() => Promise.resolve('answered')),
  cancelOperation: vi.fn(() => Promise.resolve()),
}))

let rows: OperationRow[] = []
vi.mock('./queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => rows,
}))

let foreground: string | null = null
vi.mock('./foreground-operation.svelte', () => ({
  getForegroundOperationId: () => foreground,
}))

vi.mock('#lib/ui/toast/index.js', () => ({
  addToast: vi.fn(() => 'toast-id'),
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { answerLockedSources, cancelOperation } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { _resetOpenDialogsForTesting, markDialogClosed, markDialogOpen } from '#lib/ui/open-dialogs.svelte.js'
import {
  cancelLockedSourcesPrompt,
  getLockedSourcesPrompt,
  moveLockedSourcesAnyway,
  reconcileLockedSourcesPrompts,
  skipLockedSources,
  startLockedSourcesHost,
  stopLockedSourcesHost,
} from './locked-sources.svelte'

function row(id: string, status: OperationSnapshot['status'] = 'running'): OperationRow {
  return {
    snapshot: {
      operationId: id,
      operationType: 'move',
      status,
      source: 'Contracts (3 items)',
      destination: 'Archive',
      rollbackSupport: 'crossVolumeMove',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      startedAt: null,
      items: null,
    },
    progress: null,
  }
}

function question(over: Partial<WriteLockedSourcesEvent> = {}): WriteLockedSourcesEvent {
  return {
    operationId: 'op-1',
    operationType: 'move',
    lockedCount: 2,
    lockedSample: ['/Documents/contract.pdf', '/Documents/deed.pdf'],
    ...over,
  }
}

async function flush(): Promise<void> {
  for (let i = 0; i < 25; i++) await Promise.resolve()
  flushSync()
}

async function deliver(event: WriteLockedSourcesEvent = question()): Promise<void> {
  if (!lockedCb) throw new Error('the host never subscribed to write-locked-sources')
  lockedCb(event)
  await flush()
}

beforeEach(async () => {
  vi.clearAllMocks()
  _resetOpenDialogsForTesting()
  rows = [row('op-1'), row('op-2')]
  foreground = null
  await startLockedSourcesHost()
})

afterEach(() => {
  stopLockedSourcesHost()
  lockedCb = null
})

describe('a move that found Locked items', () => {
  it('asks the person, naming the honest count and the asking move', async () => {
    await deliver()
    const prompt = getLockedSourcesPrompt()
    expect(prompt?.operationId).toBe('op-1')
    expect(prompt?.event.lockedCount).toBe(2)
    expect(prompt?.snapshot?.destination).toBe('Archive')
  })

  it('moves them anyway when the person says so', async () => {
    await deliver()
    await moveLockedSourcesAnyway()
    expect(answerLockedSources).toHaveBeenCalledWith('op-1', 'move_anyway')
    expect(getLockedSourcesPrompt()).toBeNull()
  })

  it('skips them when the person says so', async () => {
    await deliver()
    await skipLockedSources()
    expect(answerLockedSources).toHaveBeenCalledWith('op-1', 'skip')
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(getLockedSourcesPrompt()).toBeNull()
  })

  it('cancels the move through the ordinary cancel', async () => {
    await deliver()
    await cancelLockedSourcesPrompt()
    expect(cancelOperation).toHaveBeenCalledWith('op-1')
    expect(answerLockedSources).not.toHaveBeenCalled()
    expect(getLockedSourcesPrompt()).toBeNull()
  })

  it('keeps the question up when the answer never reached the backend', async () => {
    vi.mocked(answerLockedSources).mockRejectedValueOnce(new Error('ipc down'))
    await deliver()
    await moveLockedSourcesAnyway()
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-1')
  })

  it('asks over the move’s own progress dialog, which would otherwise hold it forever', async () => {
    foreground = 'op-1'
    markDialogOpen('transfer-progress')
    await deliver()
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-1')
    expect(addToast).not.toHaveBeenCalled()
  })

  it('waits behind another move’s progress dialog, says so quietly, and asks when it closes', async () => {
    foreground = 'op-2'
    markDialogOpen('transfer-progress')
    await deliver()
    expect(getLockedSourcesPrompt()).toBeNull()
    expect(addToast).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ level: 'info' }))

    markDialogClosed('transfer-progress')
    await flush()
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-1')
  })

  it('waits behind any other dialog, even with its own progress dialog up', async () => {
    foreground = 'op-1'
    markDialogOpen('transfer-progress')
    markDialogOpen('transfer-confirmation')
    await deliver()
    expect(getLockedSourcesPrompt()).toBeNull()

    markDialogClosed('transfer-confirmation')
    await flush()
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-1')
  })

  it('asks one question at a time, in arrival order', async () => {
    await deliver()
    await deliver(question({ operationId: 'op-2', lockedCount: 1, lockedSample: ['/a.txt'] }))
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-1')

    await skipLockedSources()
    await flush()
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-2')
  })

  it('drops the question when the move ends some other way, such as a cancel in the queue window', async () => {
    await deliver()
    reconcileLockedSourcesPrompts(rows)
    rows = [row('op-2')]
    reconcileLockedSourcesPrompts(rows)
    await flush()
    expect(getLockedSourcesPrompt()).toBeNull()
  })

  it('does not drop a question whose move simply has not reached the rows yet', async () => {
    rows = []
    await deliver()
    reconcileLockedSourcesPrompts(rows)
    expect(getLockedSourcesPrompt()?.operationId).toBe('op-1')
  })
})
