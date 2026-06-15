/**
 * Tier 3 a11y tests for `AutoSendToastContent.svelte`.
 *
 * Toast body shown after the Flow B auto-dispatcher uploads a report. Reads the last
 * auto-sent ID from a module-level `$state` set via `setLastAutoSentReportId(id)`.
 */

import { describe, it, vi, expect } from 'vitest'
import { mount, tick } from 'svelte'
import AutoSendToastContent from './AutoSendToastContent.svelte'
import { setLastAutoSentReportId, getLastAutoSentReportId } from './auto-send-toast-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { dismissToast } from '$lib/ui/toast'
import { openSettingsWindow } from '$lib/settings/settings-window'
import { openErrorReportDialog } from './error-report-flow.svelte'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))
vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
}))
vi.mock('./error-report-flow.svelte', () => ({
  openErrorReportDialog: vi.fn(),
}))

describe('AutoSendToastContent', () => {
  it('default render has no a11y violations', async () => {
    setLastAutoSentReportId('ERR-AUTO1')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AutoSendToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the most recently set auto-sent ID', () => {
    setLastAutoSentReportId('ERR-AUTO2')
    expect(getLastAutoSentReportId()).toBe('ERR-AUTO2')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AutoSendToastContent, { target, props: {} })
    expect(target.textContent).toContain('ERR-AUTO2')
    expect(target.textContent).toContain('Error report sent')
    expect(target.textContent).toContain('Reference ID')
  })

  it('View button dismisses the toast and opens the report dialog', async () => {
    setLastAutoSentReportId('ERR-VIEW1')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AutoSendToastContent, { target, props: {} })
    await tick()
    const viewButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'View')
    if (!viewButton) throw new Error('View button missing')
    viewButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-auto-sent')
    expect(openErrorReportDialog).toHaveBeenCalled()
  })

  it('Change settings button dismisses the toast and opens the settings window', async () => {
    setLastAutoSentReportId('ERR-SET01')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(AutoSendToastContent, { target, props: {} })
    await tick()
    const settingsButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Change settings',
    )
    if (!settingsButton) throw new Error('Change settings button missing')
    settingsButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-auto-sent')
    expect(openSettingsWindow).toHaveBeenCalled()
  })
})
