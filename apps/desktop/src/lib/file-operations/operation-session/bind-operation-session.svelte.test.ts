/**
 * The view side of a session: taking one, following it, and letting go.
 *
 * Letting go is the half that fails silently. A binding that never releases
 * leaves a session listening for an operation that ended, which is invisible
 * until a window has been open all day, so the test asks the registry rather
 * than the view: after the last viewer goes, the next `acquire` must build a
 * FRESH session, which can only happen if the refcount reached zero.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { flushSync } from 'svelte'

vi.mock('$lib/tauri-commands', () => ({
  listOperations: vi.fn(() => Promise.resolve([])),
  onOperationsChanged: vi.fn(() => Promise.resolve(() => {})),
  onWriteProgress: vi.fn(() => Promise.resolve(() => {})),
  onWriteComplete: vi.fn(() => Promise.resolve(() => {})),
  onWriteError: vi.fn(() => Promise.resolve(() => {})),
  onWriteCancelled: vi.fn(() => Promise.resolve(() => {})),
  onWriteSettled: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflict: vi.fn(() => Promise.resolve(() => {})),
  onWriteConflictResolved: vi.fn(() => Promise.resolve(() => {})),
}))

import { bindOperationSession, type BoundOperationSession } from './bind-operation-session.svelte'
import {
  destroyOperationSessions,
  getOperationSessions,
  initOperationSessions,
} from './window-operation-sessions.svelte'

/** A view, as far as this module is concerned: a reactive scope that binds and
 *  can be torn down. */
function view(operationId: () => string | null): { bound: BoundOperationSession; unmount: () => void } {
  let bound!: BoundOperationSession
  const stop = $effect.root(() => {
    bound = bindOperationSession(operationId)
  })
  flushSync()
  return { bound, unmount: stop }
}

beforeEach(async () => {
  await initOperationSessions()
})

afterEach(() => {
  destroyOperationSessions()
})

describe('bindOperationSession', () => {
  it('hands two views of one operation the same session', () => {
    const first = view(() => 'op-a')
    const second = view(() => 'op-a')

    expect(first.bound.current).not.toBeNull()
    expect(second.bound.current).toBe(first.bound.current)

    first.unmount()
    second.unmount()
  })

  it('releases when the last view goes away', () => {
    const first = view(() => 'op-a')
    const second = view(() => 'op-a')
    const session = first.bound.current

    first.unmount()
    // One viewer left: still the same session, still live.
    expect(getOperationSessions()?.acquire('op-a')).toBe(session)
    getOperationSessions()?.release('op-a')

    second.unmount()
    const afterLastRelease = getOperationSessions()?.acquire('op-a')
    expect(afterLastRelease).not.toBe(session)
    getOperationSessions()?.release('op-a')
  })

  it('follows the view when it looks at another operation, and lets the old one go', () => {
    let watching = $state('op-a')
    const { bound, unmount } = view(() => watching)
    const first = bound.current

    watching = 'op-b'
    flushSync()

    expect(bound.current).not.toBe(first)
    expect(bound.current?.operationId).toBe('op-b')
    // `op-a` had one viewer, which just left.
    expect(getOperationSessions()?.acquire('op-a')).not.toBe(first)
    getOperationSessions()?.release('op-a')

    unmount()
  })

  it('holds nothing while the view names no operation', () => {
    const { bound, unmount } = view(() => null)
    expect(bound.current).toBeNull()
    unmount()
  })
})
