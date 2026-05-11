/**
 * Tier 3 a11y tests for `Size.svelte`.
 *
 * Renders the human-friendly byte string in one or more colored spans. There's
 * no interactive surface, ARIA, or labelling to validate — axe just confirms
 * the produced markup has no structural a11y violations. Contrast for the
 * `.size-*` color classes is covered by tier 1 (`scripts/check-a11y-contrast`).
 */
import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import Size from './Size.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  getFileSizeFormat: () => 'binary',
}))

describe('Size a11y', () => {
  it('typical byte count has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Size, { target, props: { bytes: 1_234_567 } })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('null bytes (renders fallback) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(Size, { target, props: { bytes: null, fallback: '—' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
