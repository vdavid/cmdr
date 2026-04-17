/**
 * Tier 3 a11y tests for `UpdateToastContent.svelte`.
 *
 * Simple toast body with a text message and two buttons (Restart /
 * Later). No props, no state — a single default test covers it.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import UpdateToastContent from './UpdateToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-process', () => ({
  relaunch: vi.fn(() => Promise.resolve()),
}))

describe('UpdateToastContent a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(UpdateToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
