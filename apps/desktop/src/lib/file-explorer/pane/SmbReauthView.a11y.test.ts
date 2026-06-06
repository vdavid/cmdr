/**
 * Tier 3 a11y tests for `SmbReauthView.svelte`.
 *
 * The sign-in prompt shown when an SMB reconnect gave up on an auth failure.
 * Audits the default state (stale-password message + login form).
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import SmbReauthView from './SmbReauthView.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  reconnectSmbVolumeWithCredentials: vi.fn(() => new Promise<never>(() => {})),
  // `NetworkLoginForm` (rendered inside) pre-fills the username from these on mount.
  getUsernameHints: vi.fn(() => Promise.resolve({})),
}))

describe('SmbReauthView a11y', () => {
  it('default state (stale-password message + form) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(SmbReauthView, {
      target,
      props: { volumeId: 'smb-test', serverLabel: 'Test server', onCancel: vi.fn() },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
