/**
 * Tier 3 a11y tests for `ShortcutsList.svelte`.
 *
 * Renders the read-only Keyboard shortcuts help list (one card per scope, `<kbd>`
 * chips per command). The shortcut + command registries are real modules; only
 * the `onShortcutChange` subscription boundary is stubbed.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ShortcutsList from './ShortcutsList.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/shortcuts', async () => {
  const actual = await vi.importActual<object>('$lib/shortcuts')
  return {
    ...actual,
    onShortcutChange: vi.fn(() => () => {}),
  }
})

describe('ShortcutsList a11y', () => {
  it('full list has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ShortcutsList, { target, props: { hideEmpty: false } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with empty-shortcut features hidden has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ShortcutsList, { target, props: { hideEmpty: true } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
