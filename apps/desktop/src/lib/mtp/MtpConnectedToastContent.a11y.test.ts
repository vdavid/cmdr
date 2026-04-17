/**
 * Tier 3 a11y tests for `MtpConnectedToastContent.svelte`.
 *
 * Sticky toast shown after an MTP device connects. The only state that
 * matters for a11y is the "Don't show again" checkbox and the two
 * action buttons. The body text is platform-dependent (macOS adds a
 * `ptpcamerad` note), which we simulate by mocking `isMacOS()`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import MtpConnectedToastContent, { setLastConnectedDeviceName } from './MtpConnectedToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('$lib/settings', () => ({
  setSetting: vi.fn(),
}))

let mockIsMac = true
vi.mock('$lib/shortcuts/key-capture', () => ({
  isMacOS: () => mockIsMac,
}))

describe('MtpConnectedToastContent a11y', () => {
  it('macOS variant has no a11y violations', async () => {
    mockIsMac = true
    setLastConnectedDeviceName('Pixel 8')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectedToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('non-macOS variant has no a11y violations', async () => {
    mockIsMac = false
    setLastConnectedDeviceName('Pixel 8')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpConnectedToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })
})
