import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { OperationSnapshot } from '$lib/ipc/bindings'

const { subscribeMocks, unlistenMock } = vi.hoisted(() => {
  const unlistenMock = vi.fn()
  const subscribe = () => vi.fn(() => Promise.resolve(unlistenMock))
  return {
    unlistenMock,
    subscribeMocks: {
      onWriteProgress: subscribe(),
      onWriteComplete: subscribe(),
      onWriteError: subscribe(),
      onWriteCancelled: subscribe(),
      onWriteSettled: subscribe(),
      onWriteConflict: subscribe(),
      onWriteConflictResolved: subscribe(),
      onOperationsChanged: subscribe(),
    },
  }
})

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn<() => Promise<OperationSnapshot[]>>(() => Promise.resolve([])),
  ...subscribeMocks,
}))

import {
  initOperationSessions,
  destroyOperationSessions,
  getOperationSessions,
} from './window-operation-sessions.svelte'

beforeEach(() => {
  vi.clearAllMocks()
})

afterEach(() => {
  destroyOperationSessions()
})

describe('the window`s session registry', () => {
  it('is null until init resolves, then holds one instance', async () => {
    expect(getOperationSessions()).toBeNull()

    await initOperationSessions()

    expect(getOperationSessions()).not.toBeNull()
    expect(subscribeMocks.onWriteProgress).toHaveBeenCalledTimes(1)
  })

  it('subscribes one listener set however many times a mount calls it', async () => {
    await initOperationSessions()
    const first = getOperationSessions()
    await initOperationSessions()

    expect(getOperationSessions()).toBe(first)
    expect(subscribeMocks.onOperationsChanged).toHaveBeenCalledTimes(1)
  })

  it('drops the instance and its listeners on teardown', async () => {
    await initOperationSessions()
    destroyOperationSessions()

    expect(getOperationSessions()).toBeNull()
    expect(unlistenMock).toHaveBeenCalledTimes(8)
  })

  it('builds a fresh instance after a teardown rather than reviving a deaf one', async () => {
    await initOperationSessions()
    const first = getOperationSessions()
    destroyOperationSessions()
    await initOperationSessions()

    expect(getOperationSessions()).not.toBe(first)
    expect(subscribeMocks.onWriteSettled).toHaveBeenCalledTimes(2)
  })

  it('survives a teardown that races the init', async () => {
    const pending = initOperationSessions()
    destroyOperationSessions()
    await pending

    // Whatever subscribed late is unsubscribed, so a torn-down window can't
    // keep a listener alive.
    expect(unlistenMock).toHaveBeenCalledTimes(8)
  })
})
