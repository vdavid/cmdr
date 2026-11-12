/**
 * Tier 3 a11y + behavior tests for `QuickLookHintToastContent.svelte`.
 *
 * Educational toast shown each time a Finder convert presses plain Space in
 * the file list (until they click "Don't show again", which flips the
 * `fileExplorer.suppressQuickLookHint` setting). Two interactive elements:
 * the inline "Settings > Keyboard shortcuts" link (deep-link) and the
 * "Don't show again" button (suppress + dismiss).
 */

import { describe, it, vi, expect, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'

import QuickLookHintToastContent from './QuickLookHintToastContent.svelte'
import { expectNoA11yViolations } from '#lib/test-a11y.js'
import { dismissToast } from '#lib/ui/toast/index.js'
import { setSetting } from '#lib/settings/index.js'
import { openShortcutCustomization } from '#lib/settings/settings-window.js'

vi.mock('#lib/ui/toast/index.js', () => ({
  dismissToast: vi.fn(),
}))
vi.mock('#lib/settings/index.js', () => ({
  setSetting: vi.fn(),
}))
vi.mock('#lib/settings/settings-window.js', () => ({
  openShortcutCustomization: vi.fn(() => Promise.resolve()),
}))
// `file.quickLook` needs the macOS-only `quickLook` capability, so Vitest (which runs as Linux)
// sees it unbound. Model the Mac, where this toast shows, with its default binding.
vi.mock('#lib/shortcuts/reactive-shortcuts.svelte.js', async (importOriginal) => ({
  ...(await importOriginal<typeof import('#lib/shortcuts/reactive-shortcuts.svelte.js')>()),
  commandHasShortcut: (id: string) => id === 'file.quickLook',
  getLiveShortcutsReactive: (id: string) => (id === 'file.quickLook' ? ['⇧Space'] : []),
}))

describe('QuickLookHintToastContent', () => {
  beforeEach(() => {
    vi.mocked(dismissToast).mockClear()
    vi.mocked(setSetting).mockClear()
    vi.mocked(openShortcutCustomization).mockClear()
  })

  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QuickLookHintToastContent, { target, props: {} })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('renders the educational copy verbatim', () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QuickLookHintToastContent, { target, props: {} })
    const text = target.textContent
    expect(text).toContain('Space')
    expect(text).toContain('selects the file under the cursor')
    expect(text).toContain('Finder')
    expect(text).toContain('Finder opens Quick Look on')
    expect(text).toContain('⇧Space')
    expect(text).toContain('works in Finder, too')
    expect(text).toContain('Enter')
    expect(text).toContain('Settings > Keyboard shortcuts')
    expect(text).toContain('Don’t show again')
  })

  it('Settings link dismisses the toast and deep-links into Keyboard shortcuts', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QuickLookHintToastContent, { target, props: {} })
    await tick()
    const settingsButton = Array.from(target.querySelectorAll('button')).find((b) =>
      b.textContent.includes('Settings > Keyboard shortcuts'),
    )
    if (!settingsButton) throw new Error('Settings link missing')
    settingsButton.click()
    expect(dismissToast).toHaveBeenCalledWith('quick-look-hint')
    expect(openShortcutCustomization).toHaveBeenCalledWith('quick-look-toast', 'file.quickLook')
    expect(setSetting).not.toHaveBeenCalled()
  })

  it('Don’t show again button flips the suppress setting and dismisses the toast', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(QuickLookHintToastContent, { target, props: {} })
    await tick()
    const suppressButton = Array.from(target.querySelectorAll('button')).find(
      (b) => b.textContent.trim() === 'Don’t show again',
    )
    if (!suppressButton) throw new Error('Don’t show again button missing')
    suppressButton.click()
    expect(setSetting).toHaveBeenCalledWith('fileExplorer.suppressQuickLookHint', true)
    expect(dismissToast).toHaveBeenCalledWith('quick-look-hint')
    // The settings window should NOT have opened — that's a different action.
    expect(openShortcutCustomization).not.toHaveBeenCalled()
  })
})
