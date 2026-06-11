/**
 * Tier-3 a11y tests for `DateFilterPopover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state in `between` mode with custom
 * (non-preset) bounds, which renders every column plus both inline `<input type="date">` custom
 * cells. The anchor is provided as a real button in the test DOM so the popover shell has
 * something to position against.
 */

import { describe, it, expect } from 'vitest'
import { mount, tick } from 'svelte'
import axe from 'axe-core'
import DateFilterPopover from './DateFilterPopover.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function makeProps(overrides: Record<string, unknown> = {}) {
  const anchor = document.createElement('button')
  anchor.textContent = 'Modified'
  return {
    anchor,
    open: false,
    onClose: () => {},
    dateFilter: 'any' as const,
    dateValue: '',
    dateValueMax: '',
    setDateFilter: () => {},
    setDateValue: () => {},
    setDateValueMax: () => {},
    onInput: () => () => {},
    scheduleSearch: () => {},
    ...overrides,
  }
}

describe('DateFilterPopover a11y', () => {
  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps()
    target.appendChild(props.anchor)
    mount(DateFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open in preset mode (after) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps({ open: true, dateFilter: 'after' as const })
    target.appendChild(props.anchor)
    mount(DateFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-dropdown').forEach((el) => {
      el.remove()
    })
  })

  // The Custom… cell renders its `<input type="date">` INSIDE the cell button (one click
  // selects + focuses — see `filter-chips/CLAUDE.md` § "Chip-side behavior"). Axe's
  // `nested-interactive` rule flags that structural nesting; we disable that one rule for
  // this state and let every other rule run, mirroring `QueryResults.a11y.test.ts`.
  it('open in between mode with custom bounds has no a11y violations (nested-interactive intentionally disabled)', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    // 2020-01-02 / 2021-03-04 match no dynamic preset, so both Custom cells render their inline inputs.
    const props = makeProps({
      open: true,
      dateFilter: 'between' as const,
      dateValue: '2020-01-02',
      dateValueMax: '2021-03-04',
    })
    target.appendChild(props.anchor)
    mount(DateFilterPopover, { target, props })
    await tick()
    const out = await axe.run(document.body, {
      runOnly: {
        type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'],
      },
      rules: {
        'color-contrast': { enabled: false },
        region: { enabled: false },
        // Intentional: the custom date input lives inside the Custom cell button.
        // See block comment above.
        'nested-interactive': { enabled: false },
      },
    })
    expect(out.violations).toEqual([])
    target.remove()
    document.querySelectorAll('.ui-dropdown').forEach((el) => {
      el.remove()
    })
  })
})
