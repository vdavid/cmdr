/**
 * Tier-3 a11y tests for `SearchModeChips.svelte`.
 *
 * Covers AI-on (four chips) and AI-off (three chips) states, plus the disabled state. The Content
 * chip is visible-disabled, so its disabled-but-described pattern lives in every case.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import SearchModeChips from './SearchModeChips.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof SearchModeChips>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    mode: 'filename',
    aiEnabled: true,
    disabled: false,
    onSelect: () => {},
    ...overrides,
  }
}

describe('SearchModeChips a11y', () => {
  it('AI-on (four chips) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI-off (three chips) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps({ aiEnabled: false, mode: 'filename' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps({ disabled: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI mode active has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchModeChips, { target, props: baseProps({ mode: 'ai' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
