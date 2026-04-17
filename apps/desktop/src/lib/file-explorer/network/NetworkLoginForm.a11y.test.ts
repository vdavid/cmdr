/**
 * Tier 3 a11y tests for `NetworkLoginForm.svelte`.
 *
 * SMB credential form rendered inline inside a pane. Tests cover each
 * `authMode` value, the connecting state (submit disabled), and the
 * error-visible state. Username-hint IPC is stubbed.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import NetworkLoginForm from './NetworkLoginForm.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  getUsernameHints: vi.fn(() => Promise.resolve({})),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
}))

const host = { id: 'host-1', name: 'nas.local', hostname: 'nas.local', ipAddress: '10.0.0.10', port: 445 }

describe('NetworkLoginForm a11y', () => {
  it('credentials-required mode (no guest option) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        authMode: 'creds_required',
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('guest-allowed mode (radio choice visible) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        shareName: 'Public',
        authMode: 'guest_allowed',
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('connecting state (disabled inputs + spinner button) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        authMode: 'creds_required',
        isConnecting: true,
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with error message visible has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(NetworkLoginForm, {
      target,
      props: {
        host,
        authMode: 'creds_required',
        errorMessage: 'Authentication failed — wrong password',
        onConnect: () => {},
        onCancel: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
