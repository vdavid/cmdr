/**
 * Tier 3 a11y tests for `CopiedPathToastContent.svelte`.
 *
 * Compact toast body shown after ⌘⌥C: a confirmation line plus the copied path.
 * Modeled on `PasteClipboardToastContent.a11y.test.ts`.
 */

import { describe, it, beforeAll, afterAll } from 'vitest'
import { mount, tick } from 'svelte'
import { _setLocaleForTests } from '$lib/intl/locale'
import CopiedPathToastContent from './CopiedPathToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

describe('CopiedPathToastContent a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CopiedPathToastContent, { target, props: { path: '/Users/test/Downloads' } })
    await tick()
    await expectNoA11yViolations(target)
    target.remove()
  })
})
