/**
 * The main window's host for "sources left while this operation queued"
 * (ERR-962JD). `#lib/tauri-commands` is mocked and the registered
 * `onWriteSourcesGone` callback captured, so each case delivers the backend's
 * event and asserts what the person sees and what the host sends back.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'
import type { OperationSnapshot, WriteSourcesGoneEvent } from '#lib/tauri-commands/index.js'
import type { OperationRow } from './queue/operations-store.svelte'

let sourcesGoneCb: ((e: WriteSourcesGoneEvent) => void) | null = null

vi.mock('#lib/tauri-commands/index.js', () => ({
  onWriteSourcesGone: vi.fn((cb: (e: WriteSourcesGoneEvent) => void) => {
    sourcesGoneCb = cb
    return Promise.resolve(() => {})
  }),
  continueWithoutGoneSources: vi.fn(() => Promise.resolve('continued')),
  cancelOperation: vi.fn(() => Promise.resolve()),
}))

let rows: OperationRow[] = []
vi.mock('./queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => rows,
}))

vi.mock('#lib/ui/toast/index.js', () => ({
  addToast: vi.fn(() => 'toast-id'),
}))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { cancelOperation, continueWithoutGoneSources } from '#lib/tauri-commands/index.js'
import { addToast } from '#lib/ui/toast/index.js'
import { _resetOpenDialogsForTesting, markDialogClosed, markDialogOpen } from '#lib/ui/open-dialogs.svelte.js'
import {
  cancelSourcesGonePrompt,
  continueSourcesGonePrompt,
  getSourcesGonePrompt,
  reconcileSourcesGonePrompts,
  startSourcesGoneHost,
  stopSourcesGoneHost,
} from './sources-gone.svelte'

function row(id: string, status: OperationSnapshot['status'] = 'running'): OperationRow {
  return {
    snapshot: {
      operationId: id,
      operationType: 'move',
      status,
      source: 'CD1 (22 items)',
      destination: 'Music',
      rollbackSupport: 'crossVolumeMove',
      reverses: null,
      error: null,
      canRetry: false,
      conflictPolicy: null,
      items: null,
    },
    progress: null,
  }
}

function question(over: Partial<WriteSourcesGoneEvent> = {}): WriteSourcesGoneEvent {
  return {
    operationId: 'op-1',
    operationType: 'move',
    totalCount: 22,
    goneCount: 4,
    goneSample: ['/Music/CD1/01.mp3', '/Music/CD1/02.mp3', '/Music/CD1/03.mp3', '/Music/CD1/04.mp3'],
    next: 'awaiting_answer',
    ...over,
  }
}

async function flush(): Promise<void> {
  for (let i = 0; i < 25; i++) await Promise.resolve()
  flushSync()
}

async function deliver(event: WriteSourcesGoneEvent = question()): Promise<void> {
  if (!sourcesGoneCb) throw new Error('the host never subscribed to write-sources-gone')
  sourcesGoneCb(event)
  await flush()
}

beforeEach(async () => {
  vi.clearAllMocks()
  _resetOpenDialogsForTesting()
  rows = [row('op-1'), row('op-2')]
  await startSourcesGoneHost()
})

afterEach(() => {
  stopSourcesGoneHost()
  sourcesGoneCb = null
})

describe('a queued operation whose sources left while it waited', () => {
  it('asks the person, naming the honest count, once nothing else is on screen', async () => {
    await deliver()
    const prompt = getSourcesGonePrompt()
    expect(prompt?.operationId).toBe('op-1')
    expect(prompt?.event.goneCount).toBe(4)
    expect(prompt?.event.totalCount).toBe(22)
    expect(prompt?.snapshot?.destination).toBe('Music')
  })

  it('waits while another dialog is open, says so quietly, and asks when it closes', async () => {
    markDialogOpen('transfer-confirmation')
    await deliver()
    expect(getSourcesGonePrompt()).toBeNull()
    expect(addToast).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ level: 'info' }))

    markDialogClosed('transfer-confirmation')
    await flush()
    expect(getSourcesGonePrompt()?.operationId).toBe('op-1')
  })

  it('carries the operation on with the rest when the person says so', async () => {
    await deliver()
    await continueSourcesGonePrompt()
    expect(continueWithoutGoneSources).toHaveBeenCalledWith('op-1')
    expect(cancelOperation).not.toHaveBeenCalled()
    expect(getSourcesGonePrompt()).toBeNull()
  })

  it('cancels the operation through the ordinary cancel', async () => {
    await deliver()
    await cancelSourcesGonePrompt()
    expect(cancelOperation).toHaveBeenCalledWith('op-1')
    expect(continueWithoutGoneSources).not.toHaveBeenCalled()
    expect(getSourcesGonePrompt()).toBeNull()
  })

  it('keeps the question up when the answer never reached the backend', async () => {
    vi.mocked(continueWithoutGoneSources).mockRejectedValueOnce(new Error('ipc down'))
    await deliver()
    await continueSourcesGonePrompt()
    expect(getSourcesGonePrompt()?.operationId).toBe('op-1')
  })

  it('asks one question at a time, in arrival order', async () => {
    await deliver()
    await deliver(question({ operationId: 'op-2', goneCount: 1, totalCount: 3 }))
    expect(getSourcesGonePrompt()?.operationId).toBe('op-1')

    await continueSourcesGonePrompt()
    await flush()
    expect(getSourcesGonePrompt()?.operationId).toBe('op-2')
  })

  it('drops the question when the operation ends some other way, such as a cancel in the queue window', async () => {
    await deliver()
    reconcileSourcesGonePrompts(rows)
    rows = [row('op-2')]
    reconcileSourcesGonePrompts(rows)
    await flush()
    expect(getSourcesGonePrompt()).toBeNull()
  })

  it('does not drop a question whose operation simply has not reached the rows yet', async () => {
    rows = []
    await deliver()
    reconcileSourcesGonePrompts(rows)
    expect(getSourcesGonePrompt()?.operationId).toBe('op-1')
  })

  it('says plainly that nothing was left when every source is gone, and asks nothing', async () => {
    await deliver(question({ goneCount: 22, next: 'nothing_left' }))
    expect(getSourcesGonePrompt()).toBeNull()
    expect(addToast).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ level: 'info' }))
  })
})
