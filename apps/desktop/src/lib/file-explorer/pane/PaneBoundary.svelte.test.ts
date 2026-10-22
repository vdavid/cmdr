/**
 * `PaneBoundary`: a pane that throws while it renders OR tears down shows its
 * reload panel, the other pane keeps working, and Reload (or a tab switch) brings
 * the pane back.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync, type Component } from 'svelte'

const { logError, focusHome } = vi.hoisted(() => ({ logError: vi.fn(), focusHome: vi.fn(() => true) }))

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: logError }),
}))
vi.mock('#lib/ui/focus-restore.js', () => ({ focusHome }))

import Host from '../../../../test/fixtures/pane-boundary-host-fixture.svelte'
import ThrowsOnRender from '../../../../test/fixtures/dialog-throw-fixture.svelte'
import ThrowsOnTeardown from '../../../../test/fixtures/teardown-throw-fixture.svelte'
import RendersFine from '../../../../test/fixtures/content-marker-fixture.svelte'

describe('PaneBoundary', () => {
  let target: HTMLDivElement
  let component: Record<string, unknown> | null = null

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
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

  function render(pane: Component) {
    const view = $state({ open: true, pane, tabId: 'tab-1' })
    component = mount(Host, { target, props: { view } }) as Record<string, unknown>
    flushSync()
    return view
  }

  const query = (testId: string) => target.querySelector(`[data-testid="${testId}"]`)
  const problemPanel = () => target.querySelector('[data-testid="left-pane"] [role="alert"]')
  const reloadButton = () =>
    [...target.querySelectorAll('button')].find((button) => button.textContent.includes('Reload pane'))

  it('shows the reload panel for a pane that throws while rendering, and the other pane lives on', () => {
    expect(() => render(ThrowsOnRender)).not.toThrow()

    expect(problemPanel()?.textContent).toContain('This pane hit a problem')
    expect(query('other-pane')).not.toBeNull()
    expect(logError).toHaveBeenCalledTimes(1)
    const [, fields] = logError.mock.calls[0] as [string, { paneId: string; error: unknown }]
    expect(fields.paneId).toBe('left')
  })

  it('brings the pane back on Reload and hands focus home', () => {
    const view = render(ThrowsOnRender)

    view.pane = RendersFine
    reloadButton()?.click()
    flushSync()
    vi.advanceTimersByTime(0)

    expect(problemPanel()).toBeNull()
    expect(query('content-marker')).not.toBeNull()
    expect(focusHome).toHaveBeenCalled()
  })

  it('catches a pane that throws while tearing down', async () => {
    const view = render(ThrowsOnTeardown)
    expect(query('teardown-throw')).not.toBeNull()

    expect(() => {
      view.open = false
      flushSync()
    }).not.toThrow()
    // Svelte hands the boundary its error on a microtask.
    await Promise.resolve()
    flushSync()

    expect(problemPanel()).not.toBeNull()
    expect(query('other-pane')).not.toBeNull()
    expect(logError).toHaveBeenCalledTimes(1)
  })

  it('re-arms when the user switches tabs, so the next tab renders', () => {
    const view = render(ThrowsOnRender)
    expect(problemPanel()).not.toBeNull()

    view.pane = RendersFine
    view.tabId = 'tab-2'
    flushSync()

    expect(problemPanel()).toBeNull()
    expect(query('content-marker')).not.toBeNull()
  })

  it('stays out of the way while nothing throws', () => {
    render(RendersFine)

    expect(query('content-marker')).not.toBeNull()
    expect(problemPanel()).toBeNull()
    expect(logError).not.toHaveBeenCalled()
  })
})
