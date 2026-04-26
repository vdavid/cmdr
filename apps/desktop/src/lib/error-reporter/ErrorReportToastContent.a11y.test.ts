/**
 * Tier 3 a11y tests for `ErrorReportToastContent.svelte`.
 *
 * Toast body shown after a successful error-report send. Reads the last sent ID
 * from a module-level `$state` set via `setLastSentReportId(id)`.
 */

import { describe, it, vi, expect } from 'vitest'
import { mount, tick } from 'svelte'
import ErrorReportToastContent, { setLastSentReportId, getLastSentReportId } from './ErrorReportToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { dismissToast } from '$lib/ui/toast'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

// jsdom doesn't ship navigator.clipboard; stub it for the copy test.
Object.defineProperty(navigator, 'clipboard', {
  value: { writeText: vi.fn(() => Promise.resolve()) },
  writable: true,
})

describe('ErrorReportToastContent', () => {
  it('default render has no a11y violations', async () => {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    setLastSentReportId('ERR-AB23X')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the most recently set sent ID', () => {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    setLastSentReportId('ERR-99XYZ')
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    expect(getLastSentReportId()).toBe('ERR-99XYZ')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportToastContent, { target, props: {} })
    expect(target.textContent).toContain('ERR-99XYZ')
  })

  it('Copy ID button copies to the clipboard', async () => {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    setLastSentReportId('ERR-COPY1')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportToastContent, { target, props: {} })
    await tick()
    const copyButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Copy ID')
    if (!copyButton) throw new Error('Copy ID button missing')
    copyButton.click()
    await tick()
    // eslint-disable-next-line @typescript-eslint/unbound-method -- vitest spy on prototype method
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('ERR-COPY1')
  })

  it('Dismiss button calls dismissToast with the toast ID', async () => {
    // eslint-disable-next-line @typescript-eslint/no-unsafe-call -- Svelte module export type not resolved
    setLastSentReportId('ERR-DISMS')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ErrorReportToastContent, { target, props: {} })
    await tick()
    const dismissButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Dismiss')
    if (!dismissButton) throw new Error('Dismiss button missing')
    dismissButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-sent')
  })
})
