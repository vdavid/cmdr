/**
 * The wait behind every instant mutation: an in-time reply is the answer, and a
 * `stillRunning` one waits for its `mutation-settled` event, so a slow create
 * reports how it really ended rather than a timeout (ERR-AREUV).
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { MutationError, MutationReply, MutationSettled } from '$lib/ipc/bindings'
import { asMutationError } from '$lib/file-operations/mutation-error'
import { awaitMutation } from './mutation-reply'

type SettleListener = (event: { payload: MutationSettled }) => void

const listeners = new Set<SettleListener>()
const unlisten = vi.fn()

vi.mock('$lib/ipc/bindings', () => ({
  events: {
    mutationSettled: {
      listen: vi.fn((cb: SettleListener) => {
        listeners.add(cb)
        return Promise.resolve(() => {
          listeners.delete(cb)
          unlisten()
        })
      }),
    },
  },
}))

function settle(payload: MutationSettled): void {
  for (const listener of [...listeners]) listener({ payload })
}

function ok(data: MutationReply) {
  return Promise.resolve({ status: 'ok' as const, data })
}

function refused(error: MutationError) {
  return Promise.resolve({ status: 'error' as const, error })
}

/** Lets queued promise callbacks run. */
async function flush(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve()
}

describe('awaitMutation', () => {
  beforeEach(() => {
    listeners.clear()
    unlisten.mockClear()
  })

  it('resolves on an in-time reply without saying it is slow', async () => {
    const onStillRunning = vi.fn()
    await awaitMutation(() => ok({ type: 'done' }), { onStillRunning })
    expect(onStillRunning).not.toHaveBeenCalled()
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('throws an in-time refusal typed', async () => {
    const thrown = await awaitMutation(() => refused({ type: 'alreadyExists', name: 'photos' })).catch(
      (e: unknown) => e,
    )
    expect(asMutationError(thrown)).toEqual({ type: 'alreadyExists', name: 'photos' })
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('says it is still running, then resolves when the work lands', async () => {
    const onStillRunning = vi.fn()
    let settled = false
    const wait = awaitMutation(() => ok({ type: 'stillRunning', pendingId: 'p1' }), { onStillRunning }).then(() => {
      settled = true
    })
    await flush()

    expect(onStillRunning).toHaveBeenCalledOnce()
    expect(settled, 'still waiting for the real end').toBe(false)

    settle({ pendingId: 'someone-else', outcome: { type: 'landed' } })
    await flush()
    expect(settled, "another mutation's settle isn't ours").toBe(false)

    settle({ pendingId: 'p1', outcome: { type: 'landed' } })
    await wait
    expect(settled).toBe(true)
    expect(unlisten).toHaveBeenCalledOnce()
  })

  it('throws the typed refusal a late end carries', async () => {
    const wait = awaitMutation(() => ok({ type: 'stillRunning', pendingId: 'p2' })).catch((e: unknown) => e)
    await flush()
    settle({ pendingId: 'p2', outcome: { type: 'refused', error: { type: 'parentNotWritable', path: '/docs' } } })
    expect(asMutationError(await wait)).toEqual({ type: 'parentNotWritable', path: '/docs' })
  })

  it('catches a settle that arrives before the reply does', async () => {
    // The backend emits from another task, so the event can overtake the reply.
    const wait = awaitMutation(() => {
      settle({ pendingId: 'p3', outcome: { type: 'landed' } })
      return ok({ type: 'stillRunning', pendingId: 'p3' })
    })
    await expect(wait).resolves.toBeUndefined()
  })
})
