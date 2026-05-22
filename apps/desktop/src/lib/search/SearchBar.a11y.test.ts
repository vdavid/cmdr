/**
 * Tier-3 a11y tests for `SearchBar.svelte`.
 *
 * The bar's a11y surface is small: a single `<input>` with a per-mode `aria-label` and a
 * decorative SVG inside the wrapper. Covered states: each mode plus the disabled state.
 */

import { describe, it } from 'vitest'
import { mount, tick, type ComponentProps } from 'svelte'
import SearchBar from './SearchBar.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

type Props = ComponentProps<typeof SearchBar>

function baseProps(overrides: Partial<Props> = {}): Props {
  return {
    inputElement: undefined,
    query: '',
    mode: 'filename',
    disabled: false,
    aiHighlight: false,
    showRunHint: false,
    onInput: () => {},
    onRun: () => {},
    onCompositionStart: () => {},
    onCompositionEnd: () => {},
    ...overrides,
  }
}

describe('SearchBar a11y', () => {
  it('filename mode has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps() })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('AI mode has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps({ mode: 'ai' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('regex mode has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps({ mode: 'regex' }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('disabled state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SearchBar, { target, props: baseProps({ disabled: true }) })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
