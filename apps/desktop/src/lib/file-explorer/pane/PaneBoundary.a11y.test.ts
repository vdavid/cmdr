/**
 * Tier 3 a11y test for `PaneBoundary.svelte`, in its one visible state: the
 * "This pane hit a problem" panel with its Reload button.
 */

import { describe, it, vi } from 'vitest'
import { flushSync, mount, tick } from 'svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import Host from '../../../../test/fixtures/pane-boundary-host-fixture.svelte'
import ThrowsOnRender from '../../../../test/fixtures/dialog-throw-fixture.svelte'

vi.mock('#lib/logging/logger.js', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

describe('PaneBoundary a11y', () => {
  it('the problem panel has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Host, { target, props: { view: { open: true, pane: ThrowsOnRender, tabId: 'tab-1' } } })
    flushSync()
    await tick()
    await expectNoA11yViolations(target)
  })
})
