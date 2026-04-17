/**
 * Tier 3 a11y tests for `ExpirationModal.svelte`.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ExpirationModal from './ExpirationModal.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  markExpirationModalShown: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
}))

describe('ExpirationModal a11y', () => {
  it('with org name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExpirationModal, {
      target,
      props: {
        organizationName: 'Acme Corp',
        expiredAt: '2025-03-01T00:00:00Z',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('without org name has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ExpirationModal, {
      target,
      props: {
        organizationName: null,
        expiredAt: '2025-03-01T00:00:00Z',
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
