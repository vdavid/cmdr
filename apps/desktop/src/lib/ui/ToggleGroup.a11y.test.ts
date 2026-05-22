/**
 * Tier 3 a11y tests for the generic `ToggleGroup` primitive.
 *
 * One audit per semantics mode. Confirms the role/aria structure and that
 * badge + hint markup doesn't break the accessible name on the underlying
 * button. Color contrast and full-page focus traps are covered by tiers 1 / 2.
 */

import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import ToggleGroup from './ToggleGroup.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('ToggleGroup a11y', () => {
  it('tabs semantics: default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToggleGroup, {
      target,
      props: {
        semantics: 'tabs',
        value: 'filename',
        options: [
          { value: 'ai', label: 'Ask anything', badge: 'AI', hint: '⌥A', ariaLabel: 'AI mode (Alt+A)' },
          { value: 'filename', label: 'Filename', hint: '⌥F', ariaLabel: 'Filename mode (Alt+F)' },
          {
            value: 'content',
            label: 'Content',
            disabled: true,
            tooltip: 'Coming soon: full-text search inside files',
            ariaLabel: 'Content mode (coming soon)',
          },
          { value: 'regex', label: 'Regex', hint: '⌥R', ariaLabel: 'Regex mode (Alt+R)' },
        ],
        onChange: () => {},
        ariaLabel: 'Search mode',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('toggles semantics: default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToggleGroup, {
      target,
      props: {
        semantics: 'toggles',
        value: 'comfortable',
        options: [
          { value: 'compact', label: 'Compact' },
          { value: 'comfortable', label: 'Comfortable' },
          { value: 'spacious', label: 'Spacious' },
        ],
        onChange: () => {},
        ariaLabel: 'UI density',
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('toggles semantics: disabled root has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ToggleGroup, {
      target,
      props: {
        semantics: 'toggles',
        value: 'comfortable',
        options: [
          { value: 'compact', label: 'Compact' },
          { value: 'comfortable', label: 'Comfortable' },
          { value: 'spacious', label: 'Spacious' },
        ],
        onChange: () => {},
        ariaLabel: 'UI density',
        disabled: true,
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
