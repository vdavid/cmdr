/**
 * The Ask Cmdr rail's error boundary: a rail that throws while it renders OR while it
 * tears down closes through `closeRail`, says so in its own toast, and leaves the window
 * and the next open of the rail working.
 *
 * Regression anchor: cmdr-reports#78, an `each_key_duplicate` in the rail replaced the
 * whole main window with the crash panel, twice in a row. The host fixture mounts the
 * rail in the route's shape, the boundary above `{#if askCmdrState.open}`.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, type Component } from 'svelte'

const { addToast, logError, saveAppStatus, shrinkMainWindowForRail } = vi.hoisted(() => ({
  addToast: vi.fn(),
  logError: vi.fn(),
  saveAppStatus: vi.fn(),
  shrinkMainWindowForRail: vi.fn(() => Promise.resolve()),
}))

vi.mock('#lib/ui/toast/index.js', () => ({ addToast }))
vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: logError }),
}))
vi.mock('#lib/ui/focus-restore.js', () => ({ focusHome: vi.fn(() => true) }))
vi.mock('#lib/app-status-store.js', () => ({ saveAppStatus }))
vi.mock('./rail-window', () => ({ shrinkMainWindowForRail, growMainWindowForRail: vi.fn(() => Promise.resolve()) }))

import Host from '../../../test/fixtures/ask-cmdr-rail-boundary-host-fixture.svelte'
import ThrowsOnRender from '../../../test/fixtures/dialog-throw-fixture.svelte'
import ThrowsOnTeardown from '../../../test/fixtures/teardown-throw-fixture.svelte'
import RendersFine from '../../../test/fixtures/content-marker-fixture.svelte'
import { askCmdrState } from './ask-cmdr-trigger.svelte'
import { tString } from '#lib/intl/messages.svelte.js'

describe('the Ask Cmdr rail boundary', () => {
  let target: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    addToast.mockReset()
    logError.mockReset()
    saveAppStatus.mockReset()
    askCmdrState.open = true
    target = document.createElement('div')
    document.body.appendChild(target)
  })

  afterEach(() => {
    if (component) {
      void unmount(component)
      component = null
    }
    target.remove()
    askCmdrState.open = false
    vi.useRealTimers()
  })

  function render(rail: Component) {
    const view = $state({ rail })
    component = mount(Host, { target, props: { view } }) as Record<string, unknown>
    flushSync()
    return view
  }

  const query = (testId: string) => target.querySelector(`[data-testid="${testId}"]`)

  /** Re-arms the boundary, then reopens the rail with a child that renders fine. */
  function reopenWithWorkingRail(view: { rail: Component }) {
    vi.advanceTimersByTime(0)
    flushSync()
    view.rail = RendersFine
    askCmdrState.open = true
    flushSync()
  }

  it('closes a rail that throws while rendering, with its own toast, and the window lives on', () => {
    let view: { rail: Component } | undefined
    expect(() => {
      view = render(ThrowsOnRender)
    }).not.toThrow()

    expect(askCmdrState.open).toBe(false)
    expect(saveAppStatus).toHaveBeenCalledWith({ askCmdrRailOpen: false })
    expect(query('window-alive')).not.toBeNull()
    expect(logError).toHaveBeenCalledTimes(1)
    expect(addToast).toHaveBeenCalledTimes(1)
    expect(addToast).toHaveBeenCalledWith(tString('askCmdr.railBoundary.closedToast'), { level: 'error' })

    reopenWithWorkingRail(view as { rail: Component })
    expect(query('content-marker')).not.toBeNull()
  })

  it('catches a rail that throws while tearing down, and reopening works', async () => {
    const view = render(ThrowsOnTeardown)
    expect(query('teardown-throw')).not.toBeNull()

    expect(() => {
      askCmdrState.open = false
      flushSync()
    }).not.toThrow()
    // Svelte hands the boundary its error on a microtask.
    await Promise.resolve()
    flushSync()

    expect(logError).toHaveBeenCalledTimes(1)
    expect(addToast).toHaveBeenCalledWith(tString('askCmdr.railBoundary.closedToast'), { level: 'error' })
    expect(askCmdrState.open).toBe(false)
    expect(query('window-alive')).not.toBeNull()

    reopenWithWorkingRail(view)
    expect(query('content-marker')).not.toBeNull()
  })
})
