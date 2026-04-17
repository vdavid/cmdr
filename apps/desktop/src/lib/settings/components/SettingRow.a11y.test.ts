/**
 * Tier 3 a11y tests for `SettingRow.svelte`.
 *
 * Wrapper that renders a label, description, and a slot for a control.
 * Tests mount the row with a plain `<input>` as the child so axe can
 * check label-control association.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import SettingRow from './SettingRow.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings', () => ({
  isModified: vi.fn(() => false),
  resetSetting: vi.fn(),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/settings/settings-search', () => ({
  getMatchIndicesForLabel: vi.fn(() => []),
  highlightMatches: vi.fn((label: string) => [{ text: label, matched: false }]),
}))

const controlSnippet = createRawSnippet(() => ({
  render: () => `<input id="appearance.uiDensity" type="text" aria-label="control" />`,
}))

describe('SettingRow a11y', () => {
  it('default has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingRow, {
      target,
      props: {
        id: 'appearance.uiDensity',
        label: 'UI density',
        description: 'How much vertical space each row uses.',
        children: controlSnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('split layout has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingRow, {
      target,
      props: {
        id: 'appearance.uiDensity',
        label: 'UI density',
        description: 'How much vertical space each row uses.',
        split: true,
        children: controlSnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('disabled + requires-restart has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingRow, {
      target,
      props: {
        id: 'appearance.uiDensity',
        label: 'UI density',
        description: 'How much vertical space each row uses.',
        disabled: true,
        disabledReason: 'Preview only',
        requiresRestart: true,
        children: controlSnippet,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
