/**
 * Tests for the native drag event wrappers and the drop-folder release command.
 */

import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    releasePromiseDrop: vi.fn(),
  },
  events: {
    dragImageSize: { listen: vi.fn() },
    dragModifiers: { listen: vi.fn() },
    dragOutSessionStarted: { listen: vi.fn() },
    dragOutSessionComplete: { listen: vi.fn() },
    dragPayloadKind: { listen: vi.fn() },
    promiseDropStarted: { listen: vi.fn() },
    promiseDropReady: { listen: vi.fn() },
    promiseDropFailed: { listen: vi.fn() },
  },
}))

import { commands, events } from '#lib/ipc/bindings.js'
import {
  onDragImageSize,
  onDragModifiers,
  onDragOutSessionStarted,
  onDragOutSessionComplete,
  onDragPayloadKind,
  onPromiseDropStarted,
  onPromiseDropReady,
  onPromiseDropFailed,
  releasePromiseDrop,
} from './native-drag'

beforeEach(() => {
  vi.clearAllMocks()
})

type Listen = { listen: Mock<(cb: unknown) => Promise<() => void>> }

/** Wires `event.listen` to capture its callback, subscribes, delivers `payload`, then unlistens. */
async function deliverThrough(
  event: Listen,
  subscribe: (handler: (value: unknown) => void) => Promise<() => void>,
  payload: unknown,
): Promise<{ handler: ReturnType<typeof vi.fn>; unlisten: ReturnType<typeof vi.fn> }> {
  const unlisten = vi.fn()
  let deliver: ((e: { payload: unknown }) => void) | undefined
  event.listen.mockImplementation((cb: unknown) => {
    deliver = cb as (e: { payload: unknown }) => void
    return Promise.resolve(unlisten)
  })
  const handler = vi.fn()
  const stop = await subscribe(handler)
  deliver?.({ payload })
  stop()
  return { handler, unlisten }
}

describe('payload-passing listeners', () => {
  const cases: [string, Listen, (h: (v: unknown) => void) => Promise<() => void>, unknown][] = [
    ['onDragImageSize', events.dragImageSize as unknown as Listen, onDragImageSize, { width: 1, height: 2 }],
    ['onDragModifiers', events.dragModifiers as unknown as Listen, onDragModifiers, { altHeld: true }],
    ['onDragOutSessionStarted', events.dragOutSessionStarted as unknown as Listen, onDragOutSessionStarted, { x: 1 }],
    [
      'onDragOutSessionComplete',
      events.dragOutSessionComplete as unknown as Listen,
      onDragOutSessionComplete,
      { x: 2 },
    ],
    ['onPromiseDropStarted', events.promiseDropStarted as unknown as Listen, onPromiseDropStarted, { dropId: 'd' }],
    [
      'onPromiseDropReady',
      events.promiseDropReady as unknown as Listen,
      onPromiseDropReady,
      { dropId: 'd', paths: [] },
    ],
    ['onPromiseDropFailed', events.promiseDropFailed as unknown as Listen, onPromiseDropFailed, { dropId: 'd' }],
  ]

  it.each(cases)(
    '%s hands the payload to the handler and gives back the unlisten',
    async (_, event, subscribe, payload) => {
      const { handler, unlisten } = await deliverThrough(event, subscribe, payload)
      expect(handler).toHaveBeenCalledExactlyOnceWith(payload)
      expect(unlisten).toHaveBeenCalledOnce()
    },
  )
})

describe('onDragPayloadKind', () => {
  it('hands just the kind to the handler', async () => {
    const { handler, unlisten } = await deliverThrough(events.dragPayloadKind as unknown as Listen, onDragPayloadKind, {
      kind: 'promises',
    })
    expect(handler).toHaveBeenCalledExactlyOnceWith('promises')
    expect(unlisten).toHaveBeenCalledOnce()
  })
})

describe('releasePromiseDrop', () => {
  it('passes the drop id through', async () => {
    vi.mocked(commands.releasePromiseDrop).mockResolvedValueOnce(undefined)
    await releasePromiseDrop('drop-1-2')
    expect(commands.releasePromiseDrop).toHaveBeenCalledWith('drop-1-2')
  })

  it('never throws when the backend rejects', async () => {
    vi.mocked(commands.releasePromiseDrop).mockRejectedValueOnce(new Error('gone'))
    await expect(releasePromiseDrop('drop-1-2')).resolves.toBeUndefined()
  })
})
