/**
 * The main window's conflict host: the part that decides, pauses, and resumes.
 *
 * Driven the way the rest of the file-operations state machines are tested —
 * `$lib/tauri-commands` fully mocked, the registered `onWriteConflict` callback
 * captured into a module-level `let`, and events delivered by calling it. That
 * keeps every assertion here about behaviour (what got paused, what got asked,
 * what got resumed) rather than about wiring.
 *
 * The scenario each of these stands in for is one bug: a transfer sent to the
 * queue hits a name clash deep inside a merging folder, and the only listener
 * for it left with the progress dialog.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { OperationSnapshot, WriteConflictEvent } from '$lib/tauri-commands'
import type { OperationRow } from './queue/operations-store.svelte'

let conflictCb: ((e: WriteConflictEvent) => void) | null = null
const noopUnlisten = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  onWriteConflict: vi.fn((cb: (e: WriteConflictEvent) => void) => {
    conflictCb = cb
    return Promise.resolve(noopUnlisten)
  }),
  resolveWriteConflict: vi.fn(() => Promise.resolve()),
  cancelWriteOperation: vi.fn(() => Promise.resolve()),
  pauseOperation: vi.fn(() => Promise.resolve()),
  resumeOperation: vi.fn(() => Promise.resolve()),
}))

let rows: OperationRow[] = []
vi.mock('./queue/main-window-operations.svelte', () => ({
  getMainWindowOperationRows: () => rows,
}))

const setFocus = vi.fn(() => Promise.resolve())
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setFocus }),
}))

vi.mock('$lib/app-mode', () => ({
  isE2eRun: () => false,
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import {
  onWriteConflict,
  resolveWriteConflict,
  cancelWriteOperation,
  pauseOperation,
  resumeOperation,
} from '$lib/tauri-commands'
import {
  startOperationConflictHost,
  stopOperationConflictHost,
  getConflictPrompt,
  resolveConflictPrompt,
  cancelConflictPrompt,
  reconcileConflictPrompts,
} from './operation-conflict.svelte'
import { setForegroundOperationId, beginForegroundClaim, endForegroundClaim } from './foreground-operation.svelte'

function snapshot(
  id: string,
  status: OperationSnapshot['status'],
  over: Partial<OperationSnapshot> = {},
): OperationSnapshot {
  return {
    operationId: id,
    operationType: 'copy',
    status,
    source: '/src/folder',
    destination: '/dst/folder',
    supportsRollback: true,
    error: null,
    ...over,
  }
}

function operationRow(id: string, status: OperationSnapshot['status'], over: Partial<OperationSnapshot> = {}) {
  return { snapshot: snapshot(id, status, over), progress: null, etaSecondsDisplay: null } satisfies OperationRow
}

function conflictEvent(over: Partial<WriteConflictEvent> = {}): WriteConflictEvent {
  return {
    operationId: 'op-1',
    sourcePath: '/src/folder/notes.txt',
    destinationPath: '/dst/folder/notes.txt',
    sourceSize: 200,
    destinationSize: 100,
    sourceModified: 2,
    destinationModified: 1,
    sourceIsDirectory: false,
    destinationIsDirectory: false,
    destinationIsNewer: false,
    sizeDifference: -100,
    ...over,
  }
}

/** Drains the microtask queue so the host's `await` chains settle. */
async function flush(): Promise<void> {
  for (let i = 0; i < 25; i++) await Promise.resolve()
}

/** Delivers a conflict the way the backend does, then lets the host react. */
async function deliver(event: WriteConflictEvent = conflictEvent()): Promise<void> {
  if (!conflictCb) throw new Error('the host never subscribed to write-conflict')
  conflictCb(event)
  await flush()
}

beforeEach(async () => {
  vi.clearAllMocks()
  conflictCb = null
  rows = [operationRow('op-1', 'running')]
  setForegroundOperationId(null)
  await startOperationConflictHost()
})

afterEach(() => {
  stopOperationConflictHost()
})

describe('a conflict nobody in the foreground owns', () => {
  it('subscribes to write-conflict for the life of the window', () => {
    expect(onWriteConflict).toHaveBeenCalledTimes(1)
  })

  it('prompts, instead of leaving the operation parked with nobody listening', async () => {
    await deliver()

    const prompt = getConflictPrompt()
    expect(prompt?.event.destinationPath).toBe('/dst/folder/notes.txt')
    expect(prompt?.operationId).toBe('op-1')
  })

  it('pauses everything that was running', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver()

    expect(pauseOperation).toHaveBeenCalledWith('op-1')
    expect(pauseOperation).toHaveBeenCalledWith('op-2')
    expect(pauseOperation).toHaveBeenCalledTimes(2)
  })

  it('says so, once there is something other than the asking operation on hold', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver()
    expect(getConflictPrompt()?.pausedOthers).toBe(true)
  })

  it('claims no hold it does not have', async () => {
    await deliver()
    expect(getConflictPrompt()?.pausedOthers).toBe(false)
  })

  it('brings the main window forward, since the queue window is what is in front', async () => {
    await deliver()
    expect(setFocus).toHaveBeenCalledTimes(1)
  })

  it('carries the operation, so the prompt can say which transfer is asking', async () => {
    await deliver()
    expect(getConflictPrompt()?.snapshot?.destination).toBe('/dst/folder')
  })
})

describe('a conflict the progress dialog owns', () => {
  it('is left to the dialog, with nothing paused and nothing prompted', async () => {
    setForegroundOperationId('op-1')
    await deliver()

    expect(getConflictPrompt()).toBeNull()
    expect(pauseOperation).not.toHaveBeenCalled()
    expect(setFocus).not.toHaveBeenCalled()
  })

  it('does not silence a DIFFERENT operation while that dialog is up', async () => {
    setForegroundOperationId('op-1')
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver(conflictEvent({ operationId: 'op-2' }))

    expect(getConflictPrompt()?.operationId).toBe('op-2')
  })
})

describe('a conflict that beats the start command response', () => {
  it('waits for the claim instead of prompting over the dialog about to own it', async () => {
    beginForegroundClaim()
    await deliver()

    expect(getConflictPrompt()).toBeNull()
    expect(pauseOperation).not.toHaveBeenCalled()

    // The response lands and the dialog takes the slot: its own conflict.
    setForegroundOperationId('op-1')
    endForegroundClaim()
    await flush()

    expect(getConflictPrompt()).toBeNull()
    expect(pauseOperation).not.toHaveBeenCalled()
  })

  it('prompts once the claim settles on somebody else', async () => {
    beginForegroundClaim()
    await deliver(conflictEvent({ operationId: 'op-2' }))
    expect(getConflictPrompt()).toBeNull()

    setForegroundOperationId('op-1')
    endForegroundClaim()
    await flush()

    expect(getConflictPrompt()?.operationId).toBe('op-2')
  })

  it('prompts when the dispatch is abandoned and nobody ever claims the slot', async () => {
    beginForegroundClaim()
    await deliver()
    endForegroundClaim()
    await flush()

    expect(getConflictPrompt()?.operationId).toBe('op-1')
  })
})

describe('answering', () => {
  it('resolves through the same path the progress dialog uses', async () => {
    await deliver()
    await resolveConflictPrompt('overwrite', false)

    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 'overwrite', false)
  })

  it('carries an apply-to-all through untouched', async () => {
    await deliver()
    await resolveConflictPrompt('skip', true)

    expect(resolveWriteConflict).toHaveBeenCalledWith('op-1', 'skip', true)
  })

  it('closes the prompt and resumes exactly what it paused', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver()
    await resolveConflictPrompt('overwrite', false)

    expect(getConflictPrompt()).toBeNull()
    expect(resumeOperation).toHaveBeenCalledWith('op-1')
    expect(resumeOperation).toHaveBeenCalledWith('op-2')
    expect(resumeOperation).toHaveBeenCalledTimes(2)
  })

  it('leaves an operation the user paused by hand alone', async () => {
    // Resuming everything would quietly override a decision the person made.
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'paused')]
    await deliver()
    await resolveConflictPrompt('skip', false)

    expect(resumeOperation).toHaveBeenCalledWith('op-1')
    expect(resumeOperation).not.toHaveBeenCalledWith('op-2')
  })

  it('resolves before it resumes, so nothing runs on an unanswered question', async () => {
    const order: string[] = []
    vi.mocked(resolveWriteConflict).mockImplementationOnce(() => {
      order.push('resolve')
      return Promise.resolve()
    })
    vi.mocked(resumeOperation).mockImplementation(() => {
      order.push('resume')
      return Promise.resolve()
    })

    await deliver()
    await resolveConflictPrompt('overwrite', false)

    expect(order).toEqual(['resolve', 'resume'])
  })

  it('keeps the prompt up and stays paused when the resolve does not land', async () => {
    vi.mocked(resolveWriteConflict).mockImplementationOnce(() => Promise.reject(new Error('ipc down')))
    await deliver()
    await resolveConflictPrompt('overwrite', false)

    expect(getConflictPrompt()?.operationId).toBe('op-1')
    expect(resumeOperation).not.toHaveBeenCalled()
  })
})

describe('several operations clashing at once', () => {
  it('asks one at a time, in the order they arrived', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver(conflictEvent({ operationId: 'op-1' }))
    await deliver(conflictEvent({ operationId: 'op-2', destinationPath: '/dst/other/notes.txt' }))

    expect(getConflictPrompt()?.operationId).toBe('op-1')

    await resolveConflictPrompt('skip', false)
    expect(getConflictPrompt()?.operationId).toBe('op-2')
  })

  it('stays paused until the last one is answered', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver(conflictEvent({ operationId: 'op-1' }))
    await deliver(conflictEvent({ operationId: 'op-2' }))

    await resolveConflictPrompt('skip', false)
    expect(resumeOperation).not.toHaveBeenCalled()

    await resolveConflictPrompt('skip', false)
    expect(resumeOperation).toHaveBeenCalledWith('op-1')
    expect(resumeOperation).toHaveBeenCalledWith('op-2')
  })

  it('raises the window once for the run of prompts, not once per prompt', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver(conflictEvent({ operationId: 'op-1' }))
    await deliver(conflictEvent({ operationId: 'op-2' }))

    expect(setFocus).toHaveBeenCalledTimes(1)
  })

  it('takes the newer event when one operation somehow asks twice', async () => {
    // The backend serializes prompts per operation, so this shouldn't happen.
    // If it ever did, the newer clash is the live one: `resolveWriteConflict`
    // is keyed by operation id alone, so an answer lands on whatever that
    // operation is parked on right now.
    await deliver(conflictEvent({ destinationPath: '/dst/folder/one.txt' }))
    await deliver(conflictEvent({ destinationPath: '/dst/folder/two.txt' }))

    expect(getConflictPrompt()?.event.destinationPath).toBe('/dst/folder/two.txt')

    await resolveConflictPrompt('skip', false)
    expect(getConflictPrompt()).toBeNull()
  })
})

describe('the operation going away mid-prompt', () => {
  it('cancels through the same path the progress dialog uses', async () => {
    await deliver()
    await cancelConflictPrompt(true)

    expect(cancelWriteOperation).toHaveBeenCalledWith('op-1', true)
    expect(getConflictPrompt()).toBeNull()
  })

  it('does not leave the rest of the queue paused after a cancel', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver()
    await cancelConflictPrompt(false)

    expect(resumeOperation).toHaveBeenCalledWith('op-2')
  })

  it('drops the prompt when the operation is cancelled from somewhere else', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver()

    rows = [operationRow('op-2', 'paused')]
    reconcileConflictPrompts(rows)
    await flush()

    expect(getConflictPrompt()).toBeNull()
    expect(resumeOperation).toHaveBeenCalledWith('op-2')
  })

  it('drops the prompt when the operation stops on a failure', async () => {
    await deliver()

    rows = [operationRow('op-1', 'failed')]
    reconcileConflictPrompts(rows)
    await flush()

    expect(getConflictPrompt()).toBeNull()
  })

  it('holds a prompt whose operation has not reached the snapshot yet', async () => {
    // The rows arrive on their own stream. Dropping an entry just because it
    // isn't there yet would throw away the question and re-wedge the operation.
    rows = []
    await deliver(conflictEvent({ operationId: 'op-9' }))
    reconcileConflictPrompts([])
    await flush()

    expect(getConflictPrompt()?.operationId).toBe('op-9')
  })

  it('keeps the queue moving when a later prompt outlives its operation', async () => {
    rows = [operationRow('op-1', 'running'), operationRow('op-2', 'running')]
    await deliver(conflictEvent({ operationId: 'op-1' }))
    await deliver(conflictEvent({ operationId: 'op-2' }))

    rows = [operationRow('op-1', 'running')]
    reconcileConflictPrompts(rows)
    await flush()

    expect(getConflictPrompt()?.operationId).toBe('op-1')
    await resolveConflictPrompt('skip', false)
    expect(getConflictPrompt()).toBeNull()
  })
})

describe('teardown', () => {
  it('drops the listener and forgets every prompt', async () => {
    await deliver()
    stopOperationConflictHost()

    expect(noopUnlisten).toHaveBeenCalledTimes(1)
    expect(getConflictPrompt()).toBeNull()
  })
})
