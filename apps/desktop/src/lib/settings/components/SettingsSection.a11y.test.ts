/**
 * Tier 3 a11y tests for `SettingsSection.svelte`.
 *
 * Thin wrapper: `<h2>` section title + children slot.
 */

import { describe, it } from 'vitest'
import { mount, tick, createRawSnippet } from 'svelte'
import SettingsSection from './SettingsSection.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const bodySnippet = createRawSnippet(() => ({
  render: () => `<div><p>Section content goes here.</p></div>`,
}))

describe('SettingsSection a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SettingsSection, { target, props: { title: 'Appearance', children: bodySnippet } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
