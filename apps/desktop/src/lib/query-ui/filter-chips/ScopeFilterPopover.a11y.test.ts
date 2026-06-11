/**
 * Tier-3 a11y tests for `ScopeFilterPopover.svelte`.
 *
 * Covers the closed state (renders nothing) and the open state with a populated scope, both
 * toggles, and an enabled "Use current folder" footer button. The anchor is provided as a real
 * button in the test DOM so the popover shell has something to position against.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import ScopeFilterPopover from './ScopeFilterPopover.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

function makeProps(overrides: Record<string, unknown> = {}) {
  const anchor = document.createElement('button')
  anchor.textContent = 'Search in'
  return {
    anchor,
    open: false,
    onClose: () => {},
    scope: '',
    excludeSystemDirs: true,
    caseSensitive: false,
    searchableFolder: { path: '/Users/test', disabled: false, disabledReason: '' },
    systemDirExcludeTooltip: 'Excludes system folders',
    onInput: () => () => {},
    onSetScope: () => {},
    onToggleCaseSensitive: () => {},
    onToggleExcludeSystemDirs: () => {},
    scheduleSearch: () => {},
    ...overrides,
  }
}

describe('ScopeFilterPopover a11y', () => {
  it('closed (open=false) renders nothing and has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps()
    target.appendChild(props.anchor)
    mount(ScopeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('open with scope text and both toggles has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const props = makeProps({
      open: true,
      scope: '/Users/test/Documents\n!/Users/test/Documents/archive',
      caseSensitive: true,
    })
    target.appendChild(props.anchor)
    mount(ScopeFilterPopover, { target, props })
    await tick()
    await expectNoA11yViolations(document.body)
    target.remove()
    document.querySelectorAll('.ui-dropdown').forEach((el) => {
      el.remove()
    })
  })
})
