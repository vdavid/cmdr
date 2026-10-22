/**
 * `DialogBoundary`: a dialog that throws while it renders OR while it tears down
 * closes, tells the user, reports, hands focus home, and leaves the window and
 * the next dialog working.
 *
 * The teardown case is the one that blanked the main window (the Multi-rename
 * sheet threw `derived_references_self` as it closed): Svelte sends a teardown
 * throw to the nearest LIVE ancestor boundary, so the boundary has to sit above
 * the `{#if}` that unmounts the dialog. The host fixture mounts it that way, the
 * same shape every route uses.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, type Component } from 'svelte'

const { addToast, logError, focusHome } = vi.hoisted(() => ({
  addToast: vi.fn(),
  logError: vi.fn(),
  focusHome: vi.fn(() => true),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: logError }),
}))
vi.mock('#lib/ui/focus-restore.js', () => ({ focusHome }))

import Host from '../../../../test/fixtures/dialog-boundary-host-fixture.svelte'
import ThrowsOnRender from '../../../../test/fixtures/dialog-throw-fixture.svelte'
import ThrowsOnTeardown from '../../../../test/fixtures/teardown-throw-fixture.svelte'
import RendersFine from '../../../../test/fixtures/content-marker-fixture.svelte'

describe('DialogBoundary', () => {
  let target: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    addToast.mockReset()
    logError.mockReset()
    focusHome.mockClear()
    target = document.createElement('div')
    document.body.appendChild(target)
  })

  afterEach(() => {
    if (component) {
      void unmount(component)
      component = null
    }
    target.remove()
    vi.useRealTimers()
  })

  /** Mounts the host with `dialog` open; `onClose` closes it unless `keepOpen`. */
  function render(dialog: Component, options: { open?: boolean; keepOpen?: boolean } = {}) {
    const view = $state({ open: options.open ?? true, dialog })
    const onClose = vi.fn(() => {
      if (!options.keepOpen) view.open = false
    })
    component = mount(Host, { target, props: { view, onClose } }) as Record<string, unknown>
    flushSync()
    return { view, onClose }
  }

  const query = (testId: string) => target.querySelector(`[data-testid="${testId}"]`)

  it('closes a dialog that throws while rendering, and the window lives on', () => {
    let onClose: ReturnType<typeof vi.fn> | undefined
    expect(() => {
      ;({ onClose } = render(ThrowsOnRender))
    }).not.toThrow()

    expect(onClose).toHaveBeenCalledTimes(1)
    expect(query('window-alive')).not.toBeNull()
    expect(logError).toHaveBeenCalledTimes(1)
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(addToast.mock.calls[0][1]).toMatchObject({ level: 'error' })
  })

  it('reports the raw thrown value, so the log bridge renders it with its cause chain', () => {
    render(ThrowsOnRender)

    const [, fields] = logError.mock.calls[0] as [string, { name: string; error: unknown }]
    expect(fields.name).toBe('fixture')
    expect((fields.error as Error).message).toContain('blew up while rendering')
  })

  it('re-arms on the next tick and hands focus home, so the next dialog opens', () => {
    const { view } = render(ThrowsOnRender)

    vi.advanceTimersByTime(0)
    flushSync()
    expect(focusHome).toHaveBeenCalled()

    view.dialog = RendersFine
    view.open = true
    flushSync()
    expect(query('content-marker')).not.toBeNull()
  })

  it('catches a dialog that throws while tearing down, which is what blanked the window', async () => {
    const { view, onClose } = render(ThrowsOnTeardown)
    expect(query('teardown-throw')).not.toBeNull()

    expect(() => {
      view.open = false
      flushSync()
    }).not.toThrow()
    // Svelte hands the boundary its error on a microtask.
    await Promise.resolve()
    flushSync()

    expect(logError).toHaveBeenCalledTimes(1)
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(query('window-alive')).not.toBeNull()

    vi.advanceTimersByTime(0)
    flushSync()
    view.dialog = RendersFine
    view.open = true
    flushSync()
    expect(query('content-marker')).not.toBeNull()
  })

  it('stops re-arming after three failures in a row, so a dialog that keeps throwing cannot spin', () => {
    // The host never really closes it, so every re-arm renders the same throw.
    const { onClose } = render(ThrowsOnRender, { keepOpen: true })

    for (let i = 0; i < 10; i++) {
      vi.advanceTimersByTime(0)
      flushSync()
    }

    // The first failure plus three re-armed retries, then it stays down.
    expect(onClose).toHaveBeenCalledTimes(4)
    expect(query('window-alive')).not.toBeNull()
  })

  it('stays out of the way while nothing throws', () => {
    const { onClose } = render(RendersFine)

    expect(query('content-marker')).not.toBeNull()
    expect(onClose).not.toHaveBeenCalled()
    expect(logError).not.toHaveBeenCalled()
  })
})
