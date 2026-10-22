/**
 * Tier 3 a11y tests for the window crash panel and the root boundary that shows it:
 * `PageCrashPanel.svelte` on its own, with and without technical details, and
 * `WindowBoundary.svelte` in its failed state.
 */

import { afterEach, describe, it, vi } from 'vitest'
import { createRawSnippet, flushSync, mount, tick, unmount } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import PageCrashPanel from './PageCrashPanel.svelte'
import WindowBoundary from './WindowBoundary.svelte'

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))
// A window other than main, so the panel doesn't host the report dialog.
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ label: 'viewer' }) }))
vi.mock('./error-report-flow.svelte', () => ({
  openErrorReportFor: vi.fn(),
  errorReportFlow: { open: false },
  hostErrorReportDialog: () => () => {},
}))
vi.mock('./breadcrumbs', () => ({ recordBreadcrumb: vi.fn() }))

let mounted: ReturnType<typeof mount> | null = null

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  document.body.innerHTML = ''
})

function newTarget(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

describe('Window crash panel a11y', () => {
  it('the panel with technical details has no violations', async () => {
    const target = newTarget()
    mounted = mount(PageCrashPanel, { target, props: { detail: 'TypeError: x is undefined', onReload: vi.fn() } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the panel without technical details has no violations', async () => {
    const target = newTarget()
    mounted = mount(PageCrashPanel, { target, props: { detail: null, onReload: vi.fn() } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('the root boundary in its failed state has no violations', async () => {
    const target = newTarget()
    const throwsOnRender = createRawSnippet(() => ({
      render: () => {
        throw new Error('the window content blew up')
      },
    }))
    mounted = mount(WindowBoundary, { target, props: { children: throwsOnRender } })
    flushSync()
    await tick()
    await expectNoA11yViolations(target)
  })
})
