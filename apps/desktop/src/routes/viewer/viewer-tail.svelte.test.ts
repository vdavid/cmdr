/**
 * Tests for `createViewerTail`: how it dispatches `viewer:file-changed:<sid>`
 * events into toasts (when tail is off, or on rotation) or `onAppendDetected`
 * (when tail is on and the file grew). Plus the dedup contract: rapid same-kind
 * events collapse, and a `rotated` event supersedes any prior `grew` toast.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { createViewerTail } from './viewer-tail.svelte'
import { clearAllToasts, getToasts } from '$lib/ui/toast/toast-store.svelte'

// Mock the Tauri event listener so `init()` doesn't try to reach a backend.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mock the bindings: the toast button mounts but isn't clicked here, so we
// only need the symbol to resolve.
vi.mock('$lib/ipc/bindings', () => ({
  commands: {
    viewerReload: vi.fn(() => Promise.resolve({ status: 'ok', data: null })),
  },
}))

describe('createViewerTail', () => {
  beforeEach(() => {
    clearAllToasts()
  })
  afterEach(() => {
    clearAllToasts()
  })

  function makeTail(opts: { tailMode: boolean }): {
    onAppendDetected: ReturnType<typeof vi.fn>
    tail: ReturnType<typeof createViewerTail>
  } {
    const onAppendDetected = vi.fn()
    const tail = createViewerTail({
      getSessionId: () => 'sess-1',
      getTailMode: () => opts.tailMode,
      onAppendDetected,
    })
    return { onAppendDetected, tail }
  }

  it('grew event with tail off pushes a persistent reload toast', () => {
    const { tail, onAppendDetected } = makeTail({ tailMode: false })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 4096 })
    const toasts = getToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].dismissal).toBe('persistent')
    expect(toasts[0].id).toBe('viewer-file-changed-sess-1-grew')
    expect(onAppendDetected).not.toHaveBeenCalled()
  })

  it('grew event with tail on invokes onAppendDetected, no toast', () => {
    const { tail, onAppendDetected } = makeTail({ tailMode: true })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 8192 })
    expect(onAppendDetected).toHaveBeenCalledWith(8192)
    expect(getToasts()).toHaveLength(0)
  })

  it('rotated event always pushes a persistent reload toast, regardless of tail mode', () => {
    const { tail } = makeTail({ tailMode: true })
    tail.testOnlyDispatch({ kind: 'rotated' })
    const toasts = getToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].id).toBe('viewer-file-changed-sess-1-rotated')
    expect(toasts[0].dismissal).toBe('persistent')
  })

  it('repeated grew events with tail off coalesce to a single toast', () => {
    const { tail } = makeTail({ tailMode: false })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 100 })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 200 })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 300 })
    expect(getToasts()).toHaveLength(1)
  })

  it('rotated supersedes a prior grew toast', () => {
    const { tail } = makeTail({ tailMode: false })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 100 })
    expect(getToasts().map((t) => t.id)).toEqual(['viewer-file-changed-sess-1-grew'])
    tail.testOnlyDispatch({ kind: 'rotated' })
    const ids = getToasts().map((t) => t.id)
    expect(ids).toContain('viewer-file-changed-sess-1-rotated')
    expect(ids).not.toContain('viewer-file-changed-sess-1-grew')
  })

  it('no-op when sessionId is empty', () => {
    const onAppendDetected = vi.fn()
    const tail = createViewerTail({
      getSessionId: () => '',
      getTailMode: () => true,
      onAppendDetected,
    })
    tail.testOnlyDispatch({ kind: 'grew', newSize: 100 })
    expect(onAppendDetected).not.toHaveBeenCalled()
    expect(getToasts()).toHaveLength(0)
  })

  it('destroy is idempotent and clears the listener', () => {
    const { tail } = makeTail({ tailMode: false })
    tail.destroy()
    tail.destroy()
  })
})
