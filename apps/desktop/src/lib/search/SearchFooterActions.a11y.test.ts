/**
 * Tier 3 a11y test for `SearchFooterActions.svelte`.
 *
 * The footer hides itself when there are no results, so the only meaningful axe state is
 * "results present" (both enabled and disabled variants).
 */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SearchFooterActions from './SearchFooterActions.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tooltip/tooltip', () => ({
  tooltip: () => ({ destroy() {} }),
}))

vi.mock('$lib/shortcuts/key-capture', () => ({
  isMacOS: () => true,
}))

describe('SearchFooterActions a11y', () => {
  it('enabled state with results has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: {
        resultCount: 3,
        disabled: false,
        onShowAllInMainWindow: () => {},
        onGoToFile: () => {},
        enterAction: 'go-to-file' as const,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchFooterActions, {
      target,
      props: {
        resultCount: 3,
        disabled: true,
        onShowAllInMainWindow: () => {},
        onGoToFile: () => {},
        enterAction: 'run-search' as const,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
