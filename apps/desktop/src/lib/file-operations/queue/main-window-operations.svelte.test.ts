/**
 * Lifecycle tests for the main window's operations-store instance.
 *
 * The point of the module is that the main window subscribes to the SAME two
 * streams the queue window does, once, and lets go of them cleanly: a leaked
 * listener pair survives a remount and double-counts every snapshot. So these
 * tests care about subscribe/unsubscribe counts and about a re-init after
 * teardown producing a working instance (the HMR / route-remount case).
 *
 * `$lib/tauri-commands` is mocked exactly as `operations-store.svelte.test.ts`
 * mocks it, with the unlisten functions captured so teardown is observable.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { flushSync } from 'svelte'
import type { OperationSnapshot } from '$lib/ipc/bindings'

const unlistenSnapshots = vi.fn()
const unlistenProgress = vi.fn()

/** Seeded rows `list_operations` answers with; per-test overridable. */
let seeded: OperationSnapshot[] = []

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve(seeded)),
  onOperationsChanged: vi.fn(() => Promise.resolve(unlistenSnapshots)),
  onWriteProgress: vi.fn(() => Promise.resolve(unlistenProgress)),
}))

import { listOperations, onOperationsChanged, onWriteProgress } from '$lib/tauri-commands'
import {
  initMainWindowOperations,
  destroyMainWindowOperations,
  getMainWindowOperations,
  getMainWindowOperationRows,
} from './main-window-operations.svelte'

function snapshot(id: string, status: OperationSnapshot['status'] = 'running'): OperationSnapshot {
  return {
    operationId: id,
    operationType: 'copy',
    status,
    source: '/src/file',
    destination: '/dst/file',
    supportsRollback: true,
    reverses: null,
    error: null,
  }
}

beforeEach(() => {
  destroyMainWindowOperations()
  vi.clearAllMocks()
  seeded = []
})

describe('main-window operations store', () => {
  it('has no instance and no rows before init', () => {
    expect(getMainWindowOperations()).toBeNull()
    expect(getMainWindowOperationRows()).toEqual([])
    expect(onOperationsChanged).not.toHaveBeenCalled()
    expect(onWriteProgress).not.toHaveBeenCalled()
  })

  it('subscribes to both streams once and seeds from list_operations', async () => {
    seeded = [snapshot('op-1')]
    await initMainWindowOperations()
    expect(onOperationsChanged).toHaveBeenCalledTimes(1)
    expect(onWriteProgress).toHaveBeenCalledTimes(1)
    expect(listOperations).toHaveBeenCalledTimes(1)

    const cleanup = $effect.root(() => {
      flushSync()
      expect(getMainWindowOperationRows().map((r) => r.snapshot.operationId)).toEqual(['op-1'])
    })
    cleanup()
  })

  it('is idempotent: a second init adds no second listener pair', async () => {
    await initMainWindowOperations()
    const first = getMainWindowOperations()
    await initMainWindowOperations()
    expect(getMainWindowOperations()).toBe(first)
    expect(onOperationsChanged).toHaveBeenCalledTimes(1)
    expect(onWriteProgress).toHaveBeenCalledTimes(1)
  })

  it('drops both listeners on destroy and forgets the rows', async () => {
    seeded = [snapshot('op-1')]
    await initMainWindowOperations()
    destroyMainWindowOperations()
    expect(unlistenSnapshots).toHaveBeenCalledTimes(1)
    expect(unlistenProgress).toHaveBeenCalledTimes(1)
    expect(getMainWindowOperations()).toBeNull()
    expect(getMainWindowOperationRows()).toEqual([])
  })

  it('destroy is safe without an init, and safe twice', () => {
    destroyMainWindowOperations()
    destroyMainWindowOperations()
    expect(unlistenSnapshots).not.toHaveBeenCalled()
  })

  it('re-init after destroy yields a LIVE instance, not a disposed one', async () => {
    await initMainWindowOperations()
    destroyMainWindowOperations()

    seeded = [snapshot('op-2')]
    await initMainWindowOperations()
    // A reused (already-disposed) instance would tear its own subscriptions
    // down again inside `init()` and never seed. Both must have happened afresh.
    expect(onOperationsChanged).toHaveBeenCalledTimes(2)
    expect(onWriteProgress).toHaveBeenCalledTimes(2)
    expect(unlistenSnapshots).toHaveBeenCalledTimes(1)

    const cleanup = $effect.root(() => {
      flushSync()
      expect(getMainWindowOperationRows().map((r) => r.snapshot.operationId)).toEqual(['op-2'])
    })
    cleanup()
  })

  it('a destroy during init leaves nothing subscribed', async () => {
    const pending = initMainWindowOperations()
    destroyMainWindowOperations()
    await pending
    expect(getMainWindowOperations()).toBeNull()
    // The store's own disposed-guard unsubscribes whatever landed after teardown.
    expect(unlistenSnapshots).toHaveBeenCalledTimes(1)
    expect(unlistenProgress).toHaveBeenCalledTimes(1)
  })
})
