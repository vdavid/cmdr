/**
 * Tier 3 a11y tests for `ExtensionChangeDialog.svelte`.
 *
 * Simple confirmation dialog with a description, "Always allow"
 * checkbox, and two action buttons.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ExtensionChangeDialog from './ExtensionChangeDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  setSetting: vi.fn(),
}))

describe('ExtensionChangeDialog a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExtensionChangeDialog, {
      target,
      props: {
        oldExtension: 'txt',
        newExtension: 'md',
        onKeepOld: () => {},
        onUseNew: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('uncommon extension switch has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExtensionChangeDialog, {
      target,
      props: {
        oldExtension: 'png',
        newExtension: 'jpg',
        onKeepOld: () => {},
        onUseNew: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
