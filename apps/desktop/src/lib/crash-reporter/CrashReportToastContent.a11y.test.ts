/**
 * Tier 3 a11y tests for `CrashReportToastContent.svelte`.
 *
 * Compact toast body shown after a crash report is sent. Just a text +
 * "Change in Settings > Updates" button. Renders deterministically (no
 * props), so a single default-state test covers it.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CrashReportToastContent from './CrashReportToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))

describe('CrashReportToastContent a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CrashReportToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
