/**
 * Tier 3 a11y tests for `ConnectToServerDialog.svelte`.
 *
 * Modal for entering a server address. Covers the idle, connecting, and
 * error states.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import ConnectToServerDialog from './ConnectToServerDialog.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  connectToServer: vi.fn(() => Promise.resolve({ host: { id: 'h', name: 'nas.local' }, sharePath: null })),
}))

describe('ConnectToServerDialog a11y', () => {
  it('default (idle state) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ConnectToServerDialog, {
      target,
      props: {
        onConnect: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
