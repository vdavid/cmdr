/**
 * Tier 3 a11y tests for the generic `Combobox` primitive.
 *
 * Covers the closed default, an empty (cold-start) list, the loading overlay, and the disabled
 * state. The open popup is driven by Ark UI's state machine we don't exercise here; axe against the
 * closed field catches label / aria regressions. Contrast is tier 1's job, focus traps are tier 2's.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import Combobox, { type ComboboxItem } from './Combobox.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const modelItems: ComboboxItem[] = [
  { value: 'gpt-4o', label: 'gpt-4o' },
  { value: 'gpt-4o-mini', label: 'gpt-4o-mini' },
]

describe('Combobox a11y', () => {
  it('closed (with suggestions) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Combobox, {
      target,
      props: { items: modelItems, inputValue: 'gpt-4o', onInputValueChange: () => {}, ariaLabel: 'Model' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('empty list (cold start) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Combobox, {
      target,
      props: { items: [], inputValue: 'my-custom-model', onInputValueChange: () => {}, ariaLabel: 'Model' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('loading overlay has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Combobox, {
      target,
      props: { items: [], inputValue: 'gpt-4o', onInputValueChange: () => {}, loading: true, ariaLabel: 'Model' },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Combobox, {
      target,
      props: {
        items: modelItems,
        inputValue: 'gpt-4o',
        onInputValueChange: () => {},
        disabled: true,
        ariaLabel: 'Model',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
