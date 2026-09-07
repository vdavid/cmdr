/**
 * Tier 3 a11y + behavior test for `FunctionKeyBarHiddenToastContent.svelte`.
 *
 * Self-dismissing INFO toast shown after "Hide function key bar" from the
 * bar's own right-click context menu. One interactive element: the inline
 * "Settings" link deep-linking to the row that turns the bar back on.
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

import FunctionKeyBarHiddenToastContent from './FunctionKeyBarHiddenToastContent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'

vi.mock('$lib/settings/settings-window', () => ({
  openSettingsWindow: vi.fn(() => Promise.resolve()),
  settingAnchorId: vi.fn((id: string) => `setting-${id}`),
}))

describe('FunctionKeyBarHiddenToastContent', () => {
  beforeEach(() => {
    vi.mocked(openSettingsWindow).mockClear()
  })

  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBarHiddenToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the hidden-toast copy with a Settings link', () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBarHiddenToastContent, { target, props: {} })
    const text = target.textContent
    expect(text).toContain('Function key bar is now hidden')
    expect(text).toContain('Settings')
  })

  it('Settings link deep-links to the "Show function key bar" row', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(FunctionKeyBarHiddenToastContent, { target, props: {} })
    await tick()
    const settingsButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent === 'Settings')
    if (!settingsButton) throw new Error('Settings link missing')
    settingsButton.click()
    expect(openSettingsWindow).toHaveBeenCalledWith(
      'function-key-bar-toast',
      ['Appearance', 'Listing'],
      settingAnchorId('appearance.showFunctionKeyBar'),
    )
  })
})
