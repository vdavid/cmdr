import { describe, it, expect, vi, beforeEach } from 'vitest'

const { commandMocks } = vi.hoisted(() => ({
  commandMocks: {
    pauseOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    resumeOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelOperation: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
    cancelWriteOperation: vi.fn<(id: string, rollback: boolean) => Promise<void>>(() => Promise.resolve()),
    resolveWriteConflict: vi.fn<
      (id: string, conflictId: number, resolution: string, applyToAll: boolean) => Promise<string>
    >(() =>
      Promise.resolve('resolved'),
    ),
    /** The `is_running` trap: a paused operation still reports `true` here, so
     *  nothing in a session may ask. Mocked so a test can prove it isn't asked. */
    getOperationStatus: vi.fn(() => Promise.resolve({ isRunning: true })),
  },
}))

vi.mock('$lib/tauri-commands', () => commandMocks)

import { createOperationSessionCommands } from './operation-session-commands.svelte'

/** A promise plus its resolver, so a test decides exactly when a command's IPC
 *  settles and can leave it pending while it checks the guard. */
function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void; reject: (error: unknown) => void } {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/** The commands for one operation, inside a reactive scope so the in-flight
 *  `$state` has somewhere to live. `paused` is what the registry snapshot says. */
function harness(paused = false) {
  let isPaused = paused
  let commands!: ReturnType<typeof createOperationSessionCommands>
  const stopScope = $effect.root(() => {
    commands = createOperationSessionCommands('op-1', () => isPaused)
  })
  return {
    commands,
    setPaused: (next: boolean): void => {
      isPaused = next
    },
    dispose: stopScope,
  }
}

beforeEach(() => {
  for (const mock of Object.values(commandMocks)) mock.mockReset()
  commandMocks.pauseOperation.mockResolvedValue(undefined)
  commandMocks.resumeOperation.mockResolvedValue(undefined)
  commandMocks.cancelOperation.mockResolvedValue(undefined)
  commandMocks.cancelWriteOperation.mockResolvedValue(undefined)
  commandMocks.resolveWriteConflict.mockResolvedValue('resolved')
})

describe('pause and resume', () => {
  it('parks and wakes the operation, and reports that the request landed', async () => {
    const { commands, dispose } = harness()

    expect(await commands.pause()).toBe(true)
    expect(commandMocks.pauseOperation).toHaveBeenCalledWith('op-1')
    expect(commands.pauseInFlight).toBe(false)

    expect(await commands.resume()).toBe(true)
    expect(commandMocks.resumeOperation).toHaveBeenCalledWith('op-1')
    dispose()
  })

  it('drops a second press while the first is still in flight', async () => {
    const pending = deferred<undefined>()
    commandMocks.pauseOperation.mockReturnValueOnce(pending.promise)
    const { commands, dispose } = harness()

    const first = commands.pause()
    expect(commands.pauseInFlight).toBe(true)
    const second = await commands.pause()

    expect(second).toBe(false)
    expect(commandMocks.pauseOperation).toHaveBeenCalledTimes(1)

    pending.resolve(undefined)
    await first
    expect(commands.pauseInFlight).toBe(false)
    dispose()
  })

  it('lets go of the guard when the request is refused, so the button works again', async () => {
    commandMocks.pauseOperation.mockRejectedValueOnce(new Error('ipc down'))
    const { commands, dispose } = harness()

    expect(await commands.pause()).toBe(false)
    expect(commands.pauseInFlight).toBe(false)

    expect(await commands.pause()).toBe(true)
    expect(commandMocks.pauseOperation).toHaveBeenCalledTimes(2)
    dispose()
  })
})

describe('the pause toggle', () => {
  it('resumes a paused operation and pauses a running one, from the snapshot status', async () => {
    const { commands, setPaused, dispose } = harness(true)

    await commands.togglePause()
    expect(commandMocks.resumeOperation).toHaveBeenCalledWith('op-1')
    expect(commandMocks.pauseOperation).not.toHaveBeenCalled()

    setPaused(false)
    await commands.togglePause()
    expect(commandMocks.pauseOperation).toHaveBeenCalledWith('op-1')
    dispose()
  })

  it('never asks the backend whether the operation is running', async () => {
    // A paused operation stays in the write-op state map and answers
    // `is_running: true`, so a toggle keyed on it would try to pause an
    // operation that is already parked. The snapshot status is the only truth.
    const { commands, dispose } = harness(true)

    await commands.togglePause()

    expect(commandMocks.getOperationStatus).not.toHaveBeenCalled()
    expect(commandMocks.resumeOperation).toHaveBeenCalledWith('op-1')
    dispose()
  })

  it('shares its guard with pause and resume', async () => {
    const pending = deferred<undefined>()
    commandMocks.pauseOperation.mockReturnValueOnce(pending.promise)
    const { commands, dispose } = harness()

    const first = commands.pause()
    expect(await commands.togglePause()).toBe(false)
    expect(commandMocks.resumeOperation).not.toHaveBeenCalled()

    pending.resolve(undefined)
    await first
    dispose()
  })
})

describe('cancel', () => {
  it('stops the operation through the manager, so a queued one is dropped before it spawns', async () => {
    const { commands, dispose } = harness()

    expect(await commands.cancel()).toBe(true)

    expect(commandMocks.cancelOperation).toHaveBeenCalledWith('op-1')
    expect(commandMocks.cancelWriteOperation).not.toHaveBeenCalled()
    dispose()
  })

  it('stays cancelling once the request lands, so a second click sends nothing', async () => {
    const { commands, dispose } = harness()

    await commands.cancel()
    expect(commands.cancelling).toBe(true)

    expect(await commands.cancel()).toBe(false)
    expect(commandMocks.cancelOperation).toHaveBeenCalledTimes(1)
    dispose()
  })

  it('lets go when the request is refused: the operation is still running', async () => {
    commandMocks.cancelOperation.mockRejectedValueOnce(new Error('ipc down'))
    const { commands, dispose } = harness()

    expect(await commands.cancel()).toBe(false)
    expect(commands.cancelling).toBe(false)

    expect(await commands.cancel()).toBe(true)
    dispose()
  })
})

describe('rollback', () => {
  it('asks the write operation to undo what it wrote', async () => {
    const { commands, dispose } = harness()

    expect(await commands.rollback()).toBe(true)

    expect(commandMocks.cancelWriteOperation).toHaveBeenCalledWith('op-1', true)
    expect(commands.rollingBack).toBe(true)
    dispose()
  })

  it('sends nothing on a second click', async () => {
    const { commands, dispose } = harness()

    await commands.rollback()
    expect(await commands.rollback()).toBe(false)

    expect(commandMocks.cancelWriteOperation).toHaveBeenCalledTimes(1)
    dispose()
  })

  it('is refused once a cancel is on its way: there is nothing left to put back', async () => {
    const { commands, dispose } = harness()

    await commands.cancel()
    expect(await commands.rollback()).toBe(false)

    expect(commandMocks.cancelWriteOperation).not.toHaveBeenCalled()
    dispose()
  })

  it('lets go when the request is refused', async () => {
    commandMocks.cancelWriteOperation.mockRejectedValueOnce(new Error('ipc down'))
    const { commands, dispose } = harness()

    expect(await commands.rollback()).toBe(false)
    expect(commands.rollingBack).toBe(false)
    dispose()
  })

  it('can still be cancelled: a cancel mid-rollback stops the undo and keeps the rest', async () => {
    const { commands, dispose } = harness()

    await commands.rollback()
    expect(await commands.cancel()).toBe(true)

    expect(commandMocks.cancelOperation).toHaveBeenCalledWith('op-1')
    dispose()
  })
})

describe('resolving a conflict', () => {
  it('hands the answer to the backend and reports what it acted on', async () => {
    const { commands, dispose } = harness()

    expect(await commands.resolveConflict(4, 'overwrite', false)).toBe('resolved')

    expect(commandMocks.resolveWriteConflict).toHaveBeenCalledWith('op-1', 4, 'overwrite', false)
    expect(commands.resolvingConflict).toBe(false)
    dispose()
  })

  // Any surface may answer; the backend arbitrates and says what it did. A
  // session reports that verdict untouched rather than deciding for itself which
  // surface was allowed to ask.
  it.each(['resolved', 'already_resolved', 'no_pending_conflict', 'unknown_operation'] as const)(
    'passes the %s verdict straight back to the caller',
    async (outcome) => {
      commandMocks.resolveWriteConflict.mockResolvedValueOnce(outcome)
      const { commands, dispose } = harness()

      expect(await commands.resolveConflict(1, 'skip', true)).toBe(outcome)
      dispose()
    },
  )

  it('drops a second answer while the first is in flight', async () => {
    const pending = deferred<string>()
    commandMocks.resolveWriteConflict.mockReturnValueOnce(pending.promise)
    const { commands, dispose } = harness()

    const first = commands.resolveConflict(1, 'overwrite', false)
    expect(commands.resolvingConflict).toBe(true)

    expect(await commands.resolveConflict(1, 'skip', false)).toBeNull()
    expect(commandMocks.resolveWriteConflict).toHaveBeenCalledTimes(1)

    pending.resolve('resolved')
    await first
    expect(commands.resolvingConflict).toBe(false)
    dispose()
  })

  it('reports no verdict when the call never landed, so the question stays on screen', async () => {
    commandMocks.resolveWriteConflict.mockRejectedValueOnce(new Error('ipc down'))
    const { commands, dispose } = harness()

    expect(await commands.resolveConflict(1, 'overwrite', false)).toBeNull()
    expect(commands.resolvingConflict).toBe(false)
    dispose()
  })
})
