/**
 * Tier 3 a11y tests for `CommercialReminderModal.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import CommercialReminderModal from './CommercialReminderModal.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  markCommercialReminderDismissed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

describe('CommercialReminderModal a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(CommercialReminderModal, { target, props: { onClose: () => {} } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
