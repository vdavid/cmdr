/**
 * Tier-3 a11y tests for `SizeFilterPopover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state in `between` mode, which renders
 * every column (comparator, lower value + unit, upper value + unit). The anchor is provided as a
 * real button in the test DOM so the popover shell has something to position against.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import SizeFilterPopover from './SizeFilterPopover.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function makeProps(overrides: Record<string, unknown> = {}) {
  const anchor = document.createElement('button')
  anchor.textContent = 'Size'
  return {
    anchor,
    open: false,
    onClose: () => {},
    sizeFilter: 'any' as const,
    sizeValue: '',
    sizeUnit: 'MB' as const,
    sizeValueMax: '',
    sizeUnitMax: 'MB' as const,
    setSizeFilter: () => {},
    setSizeValue: () => {},
    setSizeUnit: () => {},
    setSizeValueMax: () => {},
    setSizeUnitMax: () => {},
    onInput: () => () => {},
    scheduleSearch: () => {},
    ...overrides,
  }
}

describe('SizeFilterPopover a11y', () => {
  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps()
    target.appendChild(props.anchor)
    mount(SizeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open in between mode (all columns) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps({
      open: true,
      sizeFilter: 'between' as const,
      sizeValue: '5',
      sizeUnit: 'MB' as const,
      sizeValueMax: '10',
      sizeUnitMax: 'GB' as const,
    })
    target.appendChild(props.anchor)
    mount(SizeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.filter-chip-popover').forEach((el) => {
      el.remove()
    })
  })
})
