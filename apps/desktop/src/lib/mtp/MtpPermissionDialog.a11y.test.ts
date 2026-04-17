/**
 * Tier 3 a11y tests for `MtpPermissionDialog.svelte`.
 *
 * Linux-specific help dialog with a copyable install command.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import MtpPermissionDialog from './MtpPermissionDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  copyToClipboard: vi.fn(() => Promise.resolve()),
}))

describe('MtpPermissionDialog a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MtpPermissionDialog, {
      target,
      props: { onClose: () => {}, onRetry: () => {} },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
