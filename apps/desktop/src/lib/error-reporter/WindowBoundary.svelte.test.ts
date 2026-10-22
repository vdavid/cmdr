/**
 * `WindowBoundary`: whatever breaks past every inner boundary shows the crash panel
 * instead of a blank window, whether it threw while rendering or while tearing down.
 * Reload re-arms in place first, and reloads the webview only when re-arming didn't help.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, type Component } from 'svelte'

const { logError, reloadWindow } = vi.hoisted(() => ({ logError: vi.fn(), reloadWindow: vi.fn() }))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: logError }),
}))
vi.mock('./reload-window', () => ({ reloadWindow }))
// A window other than main, so the panel doesn't host the report dialog.
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'viewer' }) }))
vi.mock('./error-report-flow.svelte', () => ({
  openErrorReportFor: vi.fn(),
  errorReportFlow: { open: false },
  hostErrorReportDialog: () => () => {},
}))
vi.mock('./breadcrumbs', () => ({ recordBreadcrumb: vi.fn() }))

import Host from '../../../test/fixtures/window-boundary-host-fixture.svelte'
import ThrowsOnRender from '../../../test/fixtures/dialog-throw-fixture.svelte'
import ThrowsOnTeardown from '../../../test/fixtures/teardown-throw-fixture.svelte'
import RendersFine from '../../../test/fixtures/content-marker-fixture.svelte'

describe('WindowBoundary', () => {
  let target: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] })
    logError.mockReset()
    reloadWindow.mockReset()
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

  function render(content: Component) {
    const view = $state({ open: true, content })
    component = mount(Host, { target, props: { view } }) as Record<string, unknown>
    flushSync()
    return view
  }

  const heading = () => target.querySelector('h1')
  const reloadButton = () =>
    [...target.querySelectorAll('button')].find((button) => button.textContent.includes('Reload'))

  it('shows the crash panel, with what broke, for content that throws while rendering', () => {
    expect(() => render(ThrowsOnRender)).not.toThrow()

    expect(heading()?.textContent).toBe('Something went wrong in this window')
    expect(target.querySelector('pre')?.textContent).toContain('dialog blew up while rendering')
    expect(logError).toHaveBeenCalledTimes(1)
  })

  it('shows the crash panel for content that throws while tearing down, the case that blanked the window', async () => {
    const view = render(ThrowsOnTeardown)

    expect(() => {
      view.open = false
      flushSync()
    }).not.toThrow()
    // Svelte hands the boundary its error on a microtask.
    await Promise.resolve()
    flushSync()

    expect(heading()?.textContent).toBe('Something went wrong in this window')
    expect(logError).toHaveBeenCalledTimes(1)
  })

  it('re-arms in place on Reload, keeping the window', () => {
    const view = render(ThrowsOnRender)

    view.content = RendersFine
    reloadButton()?.click()
    flushSync()

    expect(target.querySelector('[data-testid="content-marker"]')).not.toBeNull()
    expect(reloadWindow).not.toHaveBeenCalled()
  })

  it('reloads the webview when the window breaks again right after a re-arm', () => {
    render(ThrowsOnRender)

    // The re-arm renders the same throw, so the panel comes straight back.
    reloadButton()?.click()
    flushSync()
    expect(heading()).not.toBeNull()
    expect(reloadWindow).not.toHaveBeenCalled()

    reloadButton()?.click()
    expect(reloadWindow).toHaveBeenCalledTimes(1)
  })

  it('re-arms again when the next break comes long after the last re-arm', () => {
    const view = render(ThrowsOnRender)

    view.content = RendersFine
    reloadButton()?.click()
    flushSync()

    vi.advanceTimersByTime(60_000)
    view.content = ThrowsOnRender
    flushSync()
    expect(heading()).not.toBeNull()

    reloadButton()?.click()
    expect(reloadWindow).not.toHaveBeenCalled()
  })
})
