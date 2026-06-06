import { describe, it, expect, vi, beforeEach } from 'vitest'

/**
 * The bridge mounts one `drag-out-session-started` + one
 * `drag-out-session-complete` listener and turns each session into a single
 * signs-of-life → completion toast keyed by the session. These tests pump the
 * two listener callbacks and assert the resulting `addToast` calls (id, level,
 * dismissal, message).
 */

interface StartedPayload {
  sessionKey: number
  totalItems: number
}
interface CompletePayload {
  sessionKey: number
  filesSucceeded: number
  foldersSucceeded: number
  failures: string[]
}

type Listener<T> = (ev: { payload: T }) => void

interface ToastOptionsLike {
  id?: string
  level?: string
  dismissal?: string
  toastGroup?: string
}

const { listenMock, addToastMock } = vi.hoisted(() => ({
  listenMock: vi.fn<(event: string, cb: unknown) => Promise<() => void>>(),
  addToastMock: vi.fn<(content: string, options?: ToastOptionsLike) => string>(() => 'toast-id'),
}))

vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }))
vi.mock('$lib/ui/toast', () => ({ addToast: addToastMock }))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

import { startDragOutEventBridge } from './drag-out-event-bridge'

/**
 * Wires the mocked `listen` so each event name's callback is captured, mounts
 * the bridge, and returns the two captured callbacks.
 */
async function mountBridge(): Promise<{
  started: Listener<StartedPayload>
  complete: Listener<CompletePayload>
}> {
  const callbacks: Record<string, unknown> = {}
  listenMock.mockImplementation((event: string, cb: unknown) => {
    callbacks[event] = cb
    return Promise.resolve(() => {})
  })
  await startDragOutEventBridge()
  return {
    started: callbacks['drag-out-session-started'] as Listener<StartedPayload>,
    complete: callbacks['drag-out-session-complete'] as Listener<CompletePayload>,
  }
}

describe('drag-out event bridge', () => {
  beforeEach(() => {
    listenMock.mockReset()
    addToastMock.mockReset()
    addToastMock.mockReturnValue('toast-id')
  })

  it('shows a persistent default in-progress toast when a session starts', async () => {
    const { started } = await mountBridge()
    started({ payload: { sessionKey: 7, totalItems: 3 } })

    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [message, options] = addToastMock.mock.calls[0]
    expect(message).toBe('Downloading 3 items…')
    expect(options).toMatchObject({ id: 'drag-out:7', level: 'default', dismissal: 'persistent' })
  })

  it('pluralizes the in-progress count for a single item', async () => {
    const { started } = await mountBridge()
    started({ payload: { sessionKey: 1, totalItems: 1 } })
    expect(addToastMock.mock.calls[0][0]).toBe('Downloading 1 item…')
  })

  it('replaces the in-progress toast with a success toast under the same id', async () => {
    const { started, complete } = await mountBridge()
    started({ payload: { sessionKey: 9, totalItems: 2 } })
    complete({ payload: { sessionKey: 9, filesSucceeded: 2, foldersSucceeded: 0, failures: [] } })

    expect(addToastMock).toHaveBeenCalledTimes(2)
    const [message, options] = addToastMock.mock.calls[1]
    expect(message).toBe('Copied 2 files.')
    // Same id replaces the in-progress toast in place; transient so it self-dismisses.
    expect(options).toMatchObject({ id: 'drag-out:9', level: 'success', dismissal: 'transient' })
  })

  it('surfaces a warn toast naming the failure on a partial session', async () => {
    const { complete } = await mountBridge()
    complete({
      payload: { sessionKey: 4, filesSucceeded: 1, foldersSucceeded: 0, failures: ['clip.mov'] },
    })
    const [message, options] = addToastMock.mock.calls[0]
    expect(message).toBe("Copied 1 file, but couldn't copy clip.mov.")
    expect(options).toMatchObject({ id: 'drag-out:4', level: 'warn' })
  })

  it('surfaces an error toast on a total failure', async () => {
    const { complete } = await mountBridge()
    complete({ payload: { sessionKey: 5, filesSucceeded: 0, foldersSucceeded: 0, failures: ['a.jpg'] } })
    const [message, options] = addToastMock.mock.calls[0]
    expect(message).toBe("Couldn't copy a.jpg.")
    expect(options).toMatchObject({ id: 'drag-out:5', level: 'error' })
  })
})
