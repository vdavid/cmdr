/**
 * Waiting for an operation to settle, and the one property the whole thing turns
 * on: **a settle that already happened still answers.**
 *
 * `write-settled` follows its terminal event by microseconds, while the frontend
 * holds its own completion handling for up to `MIN_DISPLAY_MS`. So by the time
 * anything asks, the event is almost always in the past. A wait with no memory
 * of it would time out every time and the follow-up would silently never run,
 * which is exactly the failure this module was written to end.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import type { WriteSettledEvent } from '$lib/tauri-commands'

const { onWriteSettled } = vi.hoisted(() => ({ onWriteSettled: vi.fn() }))
vi.mock('$lib/tauri-commands', () => ({ onWriteSettled }))

import { initSettledOperationsWatch, destroySettledOperationsWatch, whenOperationSettled } from './settled-operations'

/** Feeds the module a settle event, the way the backend stream would. */
let emitSettled: (event: WriteSettledEvent) => void = () => {}
const unlisten = vi.fn()

function settledEvent(operationId: string): WriteSettledEvent {
  return { operationId, operationType: 'copy' }
}

beforeEach(async () => {
  vi.clearAllMocks()
  vi.useFakeTimers()
  onWriteSettled.mockImplementation((callback: (event: WriteSettledEvent) => void) => {
    emitSettled = callback
    return Promise.resolve(unlisten)
  })
  await initSettledOperationsWatch()
})

afterEach(() => {
  destroySettledOperationsWatch()
  vi.useRealTimers()
})

describe('whenOperationSettled', () => {
  it('answers immediately for an operation that settled BEFORE anyone asked', async () => {
    emitSettled(settledEvent('op-1'))

    await expect(whenOperationSettled('op-1')).resolves.toBe(true)
  })

  it('answers when the settle lands afterwards', async () => {
    const waiting = whenOperationSettled('op-2')
    emitSettled(settledEvent('op-2'))

    await expect(waiting).resolves.toBe(true)
  })

  it('gives up after the timeout when the settle never comes', async () => {
    const waiting = whenOperationSettled('op-never')
    await vi.advanceTimersByTimeAsync(5000)

    await expect(waiting).resolves.toBe(false)
  })

  it('releases every waiter on one id, and leaves other ids waiting', async () => {
    const first = whenOperationSettled('op-3')
    const second = whenOperationSettled('op-3')
    const other = whenOperationSettled('op-4')
    let otherAnswered = false
    void other.then(() => (otherAnswered = true))

    emitSettled(settledEvent('op-3'))

    await expect(first).resolves.toBe(true)
    await expect(second).resolves.toBe(true)
    expect(otherAnswered).toBe(false)
    await vi.advanceTimersByTimeAsync(5000)
    await expect(other).resolves.toBe(false)
  })

  it('forgets an id once 64 later operations have settled, so the memory stays bounded', async () => {
    emitSettled(settledEvent('op-old'))
    for (let i = 0; i < 64; i++) emitSettled(settledEvent(`op-${String(i)}`))

    const stale = whenOperationSettled('op-old')
    await vi.advanceTimersByTimeAsync(5000)
    await expect(stale).resolves.toBe(false)
    // The newest ones are still remembered.
    await expect(whenOperationSettled('op-63')).resolves.toBe(true)
  })

  it('is idempotent: a second init does not add a second listener', async () => {
    await initSettledOperationsWatch()

    expect(onWriteSettled).toHaveBeenCalledTimes(1)
  })

  it('answers waiters with false when the window tears the watch down', async () => {
    const waiting = whenOperationSettled('op-5')

    destroySettledOperationsWatch()

    await expect(waiting).resolves.toBe(false)
    expect(unlisten).toHaveBeenCalled()
  })
})
