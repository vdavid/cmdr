/**
 * Tier 3 a11y tests for `BundleSavedToastContent.svelte`.
 *
 * Toast body shown after a successful "Save bundle to disk (debug)" action.
 * Reads the saved-bundle path from a module-level `$state` set via
 * `setLastSavedBundlePath(path)`.
 */

import { describe, it, vi, expect } from 'vitest'
import { mount, tick } from 'svelte'
import BundleSavedToastContent from './BundleSavedToastContent.svelte'
import { setLastSavedBundlePath } from './bundle-saved-toast-state.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import { dismissToast } from '$lib/ui/toast'
import { showInFinder } from '$lib/tauri-commands'

vi.mock('$lib/ui/toast', () => ({
  dismissToast: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  showInFinder: vi.fn(() => Promise.resolve()),
}))

describe('BundleSavedToastContent', () => {
  it('default render has no a11y violations', async () => {
    setLastSavedBundlePath('/Users/test/Application Support/com.veszelovszki.cmdr-dev/error-report-debug.zip')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BundleSavedToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the most recently saved path', () => {
    setLastSavedBundlePath('/tmp/bundle-XYZ.zip')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BundleSavedToastContent, { target, props: {} })
    expect(target.textContent).toContain('/tmp/bundle-XYZ.zip')
  })

  it('Reveal in Finder button calls showInFinder with the saved path', async () => {
    setLastSavedBundlePath('/tmp/bundle-REV.zip')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BundleSavedToastContent, { target, props: {} })
    await tick()
    const revealButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Reveal in Finder',
    )
    if (!revealButton) throw new Error('Reveal in Finder button missing')
    revealButton.click()
    expect(showInFinder).toHaveBeenCalledWith('/tmp/bundle-REV.zip')
  })

  it('Dismiss button calls dismissToast with the toast ID', async () => {
    setLastSavedBundlePath('/tmp/bundle-DIS.zip')
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(BundleSavedToastContent, { target, props: {} })
    await tick()
    const dismissButton = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Dismiss')
    if (!dismissButton) throw new Error('Dismiss button missing')
    dismissButton.click()
    expect(dismissToast).toHaveBeenCalledWith('error-report-bundle-saved')
  })
})
